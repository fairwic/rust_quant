use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use crypto_exc_all::raw::binance::api::websocket::BinanceWebsocket;
use crypto_exc_all::raw::binance::config::{Config, DEFAULT_WS_STREAM_URL};
use okx::dto::market_dto::CandleOkxRespDto;
use rust_quant_domain::traits::CandleRepository;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use rust_quant_market::models::{CandlesEntity, CandlesModel};
use rust_quant_market::repositories::StrategyTrigger;
use serde::Deserialize;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct BinanceKlineUpdate {
    pub inst_id: String,
    pub time_interval: String,
    pub candle_entity: CandlesEntity,
    pub domain_candle: Candle,
    pub okx_candle: CandleOkxRespDto,
}

#[derive(Debug, Deserialize)]
struct BinanceKlineEvent {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: BinanceKlinePayload,
}

#[derive(Debug, Deserialize)]
struct BinanceKlinePayload {
    #[serde(rename = "t")]
    open_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "q", default)]
    quote_volume: String,
    #[serde(rename = "x")]
    closed: bool,
}

enum BinanceCandlePersister {
    QuantCore(PostgresCandleRepository),
    LegacyCompatTables,
}

impl BinanceCandlePersister {
    async fn from_env() -> Result<Self> {
        if super::should_use_quant_core_candle_source()? {
            let database_url = env::var("QUANT_CORE_DATABASE_URL")
                .context("CANDLE_SOURCE=quant_core 时必须设置 QUANT_CORE_DATABASE_URL")?;
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .connect_lazy(&database_url)
                .context("创建 quant_core Postgres K线连接池失败")?;
            Ok(Self::QuantCore(PostgresCandleRepository::new(pool)))
        } else {
            Ok(Self::LegacyCompatTables)
        }
    }

    async fn persist(&self, update: &BinanceKlineUpdate) -> Result<()> {
        match self {
            Self::QuantCore(repository) => {
                repository
                    .save_candles(vec![update.domain_candle.clone()])
                    .await
                    .with_context(|| {
                        format!(
                            "保存 Binance K线到 quant_core 分表失败: {} {}",
                            update.inst_id, update.time_interval
                        )
                    })?;
            }
            Self::LegacyCompatTables => {
                CandlesModel::new()
                    .upsert_batch(
                        vec![update.okx_candle.clone()],
                        &update.inst_id,
                        &update.time_interval,
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "保存 Binance K线到 Postgres 分表失败: {} {}",
                            update.inst_id, update.time_interval
                        )
                    })?;
            }
        }
        Ok(())
    }
}

pub fn binance_kline_stream_name(inst_id: &str, period: &str) -> String {
    format!(
        "{}@kline_{}",
        binance_symbol_from_inst_id(inst_id),
        binance_interval_from_period(period)
    )
}

pub fn parse_binance_kline_message(
    message: &Value,
    inst_id: &str,
    period: &str,
) -> Result<BinanceKlineUpdate> {
    let data = message
        .get("data")
        .cloned()
        .unwrap_or_else(|| message.clone());
    let event: BinanceKlineEvent =
        serde_json::from_value(data).context("解析 Binance kline websocket 消息失败")?;

    if event.event_type != "kline" {
        return Err(anyhow!("忽略非 kline Binance websocket 消息"));
    }

    let event_symbol = event.symbol.to_ascii_lowercase();
    let kline_symbol = event.kline.symbol.to_ascii_lowercase();
    let expected_symbol = binance_symbol_from_inst_id(inst_id);
    if event_symbol != expected_symbol || kline_symbol != expected_symbol {
        return Err(anyhow!(
            "Binance websocket 交易对不匹配: expected={}, event={}, kline={}",
            expected_symbol,
            event.symbol,
            event.kline.symbol
        ));
    }

    let expected_interval = binance_interval_from_period(period);
    if event.kline.interval != expected_interval {
        return Err(anyhow!(
            "Binance websocket K线周期不匹配: expected={}, actual={}",
            expected_interval,
            event.kline.interval
        ));
    }

    let quote_volume = if event.kline.quote_volume.trim().is_empty() {
        event.kline.volume.clone()
    } else {
        event.kline.quote_volume.clone()
    };
    let confirm = if event.kline.closed { "1" } else { "0" }.to_string();
    let timeframe = Timeframe::from_str(period).map_err(|err| anyhow!("无效的K线周期: {}", err))?;

    let mut domain_candle = Candle::new(
        inst_id.to_string(),
        timeframe,
        event.kline.open_time,
        Price::new(event.kline.open.parse::<f64>()?)
            .map_err(|err| anyhow!("创建开盘价失败: {:?}", err))?,
        Price::new(event.kline.high.parse::<f64>()?)
            .map_err(|err| anyhow!("创建最高价失败: {:?}", err))?,
        Price::new(event.kline.low.parse::<f64>()?)
            .map_err(|err| anyhow!("创建最低价失败: {:?}", err))?,
        Price::new(event.kline.close.parse::<f64>()?)
            .map_err(|err| anyhow!("创建收盘价失败: {:?}", err))?,
        Volume::new(quote_volume.parse::<f64>()?)
            .map_err(|err| anyhow!("创建成交量失败: {:?}", err))?,
    );
    if event.kline.closed {
        domain_candle.confirm();
    }

    let candle_entity = CandlesEntity {
        id: None,
        ts: event.kline.open_time,
        o: event.kline.open.clone(),
        h: event.kline.high.clone(),
        l: event.kline.low.clone(),
        c: event.kline.close.clone(),
        vol: event.kline.volume.clone(),
        vol_ccy: quote_volume.clone(),
        confirm: confirm.clone(),
        created_at: None,
        updated_at: None,
    };

    let okx_candle = CandleOkxRespDto {
        ts: event.kline.open_time.to_string(),
        o: event.kline.open,
        h: event.kline.high,
        l: event.kline.low,
        c: event.kline.close,
        v: event.kline.volume,
        vol_ccy: quote_volume.clone(),
        vol_ccy_quote: quote_volume,
        confirm,
    };

    Ok(BinanceKlineUpdate {
        inst_id: inst_id.to_string(),
        time_interval: period.to_string(),
        candle_entity,
        domain_candle,
        okx_candle,
    })
}

pub async fn receive_one_binance_public_message(
    streams: &[String],
    timeout_secs: u64,
) -> Result<Value> {
    let websocket = build_binance_public_websocket();
    let stream_refs: Vec<&str> = streams.iter().map(String::as_str).collect();
    let url = websocket.market_stream_url(&stream_refs);
    let mut session = websocket
        .connect_url(&url)
        .await
        .with_context(|| format!("连接 Binance websocket 失败: {}", url))?;

    let message = timeout(Duration::from_secs(timeout_secs), session.recv_json())
        .await
        .context("等待 Binance websocket 消息超时")?
        .ok_or_else(|| anyhow!("Binance websocket 在收到消息前关闭"))?;
    let _ = session.close().await;

    Ok(message)
}

pub async fn run_binance_websocket_with_strategy_trigger(
    inst_ids: &[String],
    periods: &[String],
    strategy_trigger: Option<StrategyTrigger>,
) -> Result<()> {
    let mut stream_targets = HashMap::new();
    for inst_id in inst_ids {
        for period in periods {
            let stream = binance_kline_stream_name(inst_id, period);
            stream_targets.insert(stream, (inst_id.clone(), period.clone()));
        }
    }

    if stream_targets.is_empty() {
        warn!("Binance WebSocket启动参数为空，跳过启动");
        return Ok(());
    }

    let persister = Arc::new(BinanceCandlePersister::from_env().await?);
    let stream_names: Vec<String> = stream_targets.keys().cloned().collect();
    info!("📡 Binance WebSocket 订阅K线频道: {:?}", stream_names);

    tokio::spawn(async move {
        loop {
            if let Err(error) = run_binance_websocket_loop(
                &stream_names,
                &stream_targets,
                persister.clone(),
                strategy_trigger.clone(),
            )
            .await
            {
                error!("❌ Binance WebSocket 连接异常，将重连: {}", error);
                sleep(Duration::from_secs(5)).await;
            }
        }
    });

    Ok(())
}

async fn run_binance_websocket_loop(
    stream_names: &[String],
    stream_targets: &HashMap<String, (String, String)>,
    persister: Arc<BinanceCandlePersister>,
    strategy_trigger: Option<StrategyTrigger>,
) -> Result<()> {
    let websocket = build_binance_public_websocket();
    let stream_refs: Vec<&str> = stream_names.iter().map(String::as_str).collect();
    let url = websocket.market_stream_url(&stream_refs);
    let mut session = websocket
        .connect_url(&url)
        .await
        .with_context(|| format!("连接 Binance websocket 失败: {}", url))?;
    info!("✅ Binance public websocket启动成功: {}", url);

    while let Some(message) = session.recv_json().await {
        let Some((inst_id, period)) = resolve_stream_target(&message, stream_targets) else {
            debug!("忽略未匹配 Binance websocket 消息: {}", message);
            continue;
        };

        match parse_binance_kline_message(&message, inst_id, period) {
            Ok(update) => {
                persister.persist(&update).await?;
                if update.candle_entity.confirm == "1" {
                    info!(
                        "📈 Binance K线确认，触发策略执行: inst_id={}, time_interval={}, ts={}",
                        update.inst_id, update.time_interval, update.candle_entity.ts
                    );
                    if let Some(trigger) = &strategy_trigger {
                        trigger(
                            update.inst_id.clone(),
                            update.time_interval.clone(),
                            update.candle_entity.clone(),
                        );
                    }
                }
            }
            Err(error) => warn!("忽略 Binance websocket 消息: {}", error),
        }
    }

    Err(anyhow!("Binance websocket 已关闭"))
}

fn resolve_stream_target<'a>(
    message: &Value,
    stream_targets: &'a HashMap<String, (String, String)>,
) -> Option<(&'a str, &'a str)> {
    if let Some(stream) = message.get("stream").and_then(Value::as_str) {
        return stream_targets
            .get(stream)
            .map(|(inst_id, period)| (inst_id.as_str(), period.as_str()));
    }

    if stream_targets.len() == 1 {
        return stream_targets
            .values()
            .next()
            .map(|(inst_id, period)| (inst_id.as_str(), period.as_str()));
    }

    None
}

fn build_binance_public_websocket() -> BinanceWebsocket {
    let config = Config::from_env();
    let stream_base_url =
        env::var("BINANCE_WS_STREAM_URL").unwrap_or_else(|_| DEFAULT_WS_STREAM_URL.to_string());
    let mut websocket = BinanceWebsocket::new_public_with_stream_base_url(stream_base_url);
    if let Some(proxy_url) = config.proxy_url {
        websocket = websocket.with_proxy_url(proxy_url);
    }
    websocket
}

fn binance_symbol_from_inst_id(inst_id: &str) -> String {
    let parts: Vec<&str> = inst_id
        .split('-')
        .map(str::trim)
        .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case("SWAP"))
        .collect();

    if parts.len() >= 2 {
        return format!("{}{}", parts[0], parts[1]).to_ascii_lowercase();
    }

    inst_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn binance_interval_from_period(period: &str) -> String {
    match period {
        "1Dutc" | "1DUTC" => "1d".to_string(),
        "1M" => "1M".to_string(),
        value => value.to_ascii_lowercase(),
    }
}
