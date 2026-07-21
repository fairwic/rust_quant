use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use okx::config::CONFIG;
use okx::dto::market_dto::CandleOkxRespDto;
use okx::websocket::auto_reconnect_client::{
    AutoReconnectWebsocketClient, ConnectionState, ReconnectConfig,
};
use okx::websocket::{Args, ChannelType};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

const HEALTH_INTERVAL: Duration = Duration::from_secs(10);
const MAX_SYMBOLS_PER_SHARD: usize = 200;

/// 从 OKX 收到的一根全市场 1m 已确认 K 线。
#[derive(Debug, Clone)]
pub struct ConfirmedOneMinuteMessage {
    /// OKX 交易对，例如 `BTC-USDT-SWAP`。
    pub symbol: String,
    /// 交易所最终确认的 1m K 线。
    pub candle: CandleOkxRespDto,
    /// 当前进程收到消息的单调时钟，用于测量本机处理延迟。
    pub received_at: Instant,
    /// 当前进程收到消息的 Unix 毫秒时间，用于测量交易所收盘到达延迟。
    pub received_at_ms: i64,
}

/// 把全市场 1m K 线订阅拆成多个故障隔离分片。
pub fn partition_symbol_shards(symbols: &[String], shard_size: usize) -> Result<Vec<Vec<String>>> {
    if shard_size == 0 || shard_size > MAX_SYMBOLS_PER_SHARD {
        bail!("WebSocket shard_size must be between 1 and {MAX_SYMBOLS_PER_SHARD}");
    }
    let mut normalized = symbols
        .iter()
        .map(|symbol| symbol.trim().to_ascii_uppercase())
        .filter(|symbol| !symbol.is_empty())
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    if normalized.is_empty() {
        bail!("all-market 1m WebSocket requires at least one symbol");
    }
    Ok(normalized
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect())
}

/// 运行全市场 1m 确认流；任一分片异常退出时整体失败，让容器统一重启并恢复缺口。
pub async fn run_all_market_confirmed_1m_stream(
    symbols: &[String],
    shard_size: usize,
    confirmed_sender: mpsc::Sender<ConfirmedOneMinuteMessage>,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let shards = partition_symbol_shards(symbols, shard_size)?;
    info!(
        event = "all_market_1m_websocket_start",
        symbols = symbols.len(),
        shards = shards.len(),
        shard_size,
        "启动全市场 1m 已确认 K 线 WebSocket"
    );

    let mut tasks = JoinSet::new();
    for (shard_index, shard_symbols) in shards.into_iter().enumerate() {
        tasks.spawn(run_confirmed_shard(
            shard_index,
            shard_symbols,
            confirmed_sender.clone(),
            shutdown.clone(),
        ));
    }
    drop(confirmed_sender);

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) if *shutdown.borrow() => {}
            Ok(Ok(())) => {
                tasks.abort_all();
                return Err(anyhow!("all-market WebSocket shard exited unexpectedly"));
            }
            Ok(Err(error)) => {
                tasks.abort_all();
                return Err(error);
            }
            Err(error) => {
                tasks.abort_all();
                return Err(anyhow!("all-market WebSocket shard panicked: {error}"));
            }
        }
    }
    Ok(())
}

/// 维护一个订阅分片；健康或队列异常会返回上层，由容器重启后统一补洞。
async fn run_confirmed_shard(
    shard_index: usize,
    symbols: Vec<String>,
    confirmed_sender: mpsc::Sender<ConfirmedOneMinuteMessage>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let client = AutoReconnectWebsocketClient::new_with_config(
        &CONFIG.business_websocket_url,
        None,
        ReconnectConfig::default(),
    );

    // 先登记再连接，使首次连接与后续重连走同一批订阅恢复路径，避免启动时双重发送。
    for symbol in &symbols {
        client
            .subscribe(
                ChannelType::Candle("1m".to_string()),
                Args::new().with_inst_id(symbol.clone()),
            )
            .await
            .with_context(|| format!("register 1m candle subscription for {symbol}"))?;
    }
    let mut receiver = client
        .start()
        .await
        .with_context(|| format!("start OKX business WebSocket shard {shard_index}"))?;
    let mut health_interval = tokio::time::interval(HEALTH_INTERVAL);
    health_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    health_interval.tick().await;

    let result = loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break Ok(());
                }
            }
            message = receiver.recv() => {
                let Some(message) = message else {
                    break Err(anyhow!("OKX business WebSocket shard {shard_index} receiver closed"));
                };
                let received_at = Instant::now();
                let received_at_ms = Utc::now().timestamp_millis();
                for confirmed in parse_confirmed_one_minute(message, received_at, received_at_ms)? {
                    confirmed_sender
                        .try_send(confirmed)
                        .map_err(|error| anyhow!("confirmed candle queue unavailable: {error}"))?;
                }
            }
            _ = health_interval.tick() => {
                let health = client.health_snapshot();
                let healthy = health.manager_task_alive
                    && health.connection_state == ConnectionState::Connected
                    && health.all_subscriptions_acknowledged;
                if healthy {
                    debug!(
                        event = "all_market_1m_websocket_shard_health",
                        shard_index,
                        symbols = symbols.len(),
                        subscriptions = health.subscription_count,
                        reconnects = health.reconnect_count,
                        last_message_elapsed_ms = health.last_message_elapsed_ms,
                        "全市场 1m WebSocket 分片健康"
                    );
                } else {
                    warn!(
                        event = "all_market_1m_websocket_shard_unhealthy",
                        shard_index,
                        symbols = symbols.len(),
                        connection_state = ?health.connection_state,
                        manager_alive = health.manager_task_alive,
                        acknowledged = health.acknowledged_subscription_count,
                        subscriptions = health.subscription_count,
                        reconnects = health.reconnect_count,
                        last_message_elapsed_ms = health.last_message_elapsed_ms,
                        "全市场 1m WebSocket 分片尚未就绪或正在恢复"
                    );
                }
            }
        }
    };
    client.stop().await;
    result
}

/// 在构造 DTO 前先检查 `confirm`，盘中每秒更新不会进入后续分配、缓存和持久化链路。
fn parse_confirmed_one_minute(
    message: Value,
    received_at: Instant,
    received_at_ms: i64,
) -> Result<Vec<ConfirmedOneMinuteMessage>> {
    if let Some(event) = message.get("event").and_then(Value::as_str) {
        let code = message.get("code").and_then(Value::as_str).unwrap_or("0");
        if event == "error" || code != "0" {
            let detail = message
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or_default();
            bail!("OKX WebSocket control error code={code} msg={detail}");
        }
        return Ok(Vec::new());
    }

    let arg = message
        .get("arg")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("OKX candle message is missing arg"))?;
    if arg.get("channel").and_then(Value::as_str) != Some("candle1m") {
        return Ok(Vec::new());
    }
    let symbol = arg
        .get("instId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("OKX candle message is missing instId"))?;
    let rows = message
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("OKX candle message is missing data"))?;

    let mut confirmed = Vec::new();
    for row in rows {
        let fields = row
            .as_array()
            .ok_or_else(|| anyhow!("OKX candle row is not an array"))?;
        if fields.len() < 9 {
            bail!(
                "OKX candle row has {} fields, expected at least 9",
                fields.len()
            );
        }
        if fields[8].as_str() != Some("1") {
            continue;
        }
        let values = fields
            .iter()
            .take(9)
            .map(|field| {
                field
                    .as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| anyhow!("OKX candle field is not a string"))
            })
            .collect::<Result<Vec<_>>>()?;
        confirmed.push(ConfirmedOneMinuteMessage {
            symbol: symbol.to_string(),
            candle: CandleOkxRespDto::from_vec(values),
            received_at,
            received_at_ms,
        });
    }
    Ok(confirmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn partitions_and_deduplicates_symbols_below_connection_limit() {
        let symbols = vec![
            "eth-usdt-swap".to_string(),
            "BTC-USDT-SWAP".to_string(),
            "ETH-USDT-SWAP".to_string(),
        ];
        let shards = partition_symbol_shards(&symbols, 1).expect("valid shards");
        assert_eq!(shards, vec![vec!["BTC-USDT-SWAP"], vec!["ETH-USDT-SWAP"]]);
        assert!(partition_symbol_shards(&symbols, MAX_SYMBOLS_PER_SHARD + 1).is_err());
    }

    #[test]
    fn parser_drops_provisional_rows_before_dto_allocation_path() {
        let message = json!({
            "arg": {"channel": "candle1m", "instId": "BTC-USDT-SWAP"},
            "data": [["0", "1", "2", "1", "2", "3", "3", "6", "0"]]
        });
        let parsed = parse_confirmed_one_minute(message, Instant::now(), 1)
            .expect("valid provisional message");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parser_keeps_only_confirmed_one_minute_rows() {
        let message = json!({
            "arg": {"channel": "candle1m", "instId": "BTC-USDT-SWAP"},
            "data": [
                ["0", "1", "2", "1", "2", "3", "3", "6", "0"],
                ["60000", "2", "3", "2", "3", "4", "4", "12", "1"]
            ]
        });
        let parsed = parse_confirmed_one_minute(message, Instant::now(), 2)
            .expect("valid confirmed message");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].symbol, "BTC-USDT-SWAP");
        assert_eq!(parsed[0].candle.ts, "60000");
        assert_eq!(parsed[0].candle.confirm, "1");
    }

    #[test]
    fn parser_rejects_malformed_confirmed_rows_without_panicking() {
        let message = json!({
            "arg": {"channel": "candle1m", "instId": "BTC-USDT-SWAP"},
            "data": [["0", "1", "2", "1", "2", "3", "1"]]
        });
        assert!(parse_confirmed_one_minute(message, Instant::now(), 1).is_err());
    }
}
