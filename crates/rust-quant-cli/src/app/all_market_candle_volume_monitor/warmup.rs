use super::config::AllMarketCandleVolumeMonitorConfig;
use crate::app::market_velocity_backfill::{build_okx_http_client, fetch_okx_history_candles};
use anyhow::{Context, Result};
use chrono::Utc;
use okx::dto::market_dto::CandleOkxRespDto;
use rust_quant_market::models::{CandlesEntity, CandlesModel, SelectCandleReqDto};
use rust_quant_market::streams::confirmed_candle_aggregator::{
    AggregatedTimeframe, ConfirmedCandle, VOLUME_LOOKBACK,
};
use sqlx::{PgPool, Row};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::info;

const ONE_MINUTE_WARMUP_CANDLES: usize = 260;
const HIGHER_TIMEFRAME_WARMUP_CANDLES: usize = VOLUME_LOOKBACK + 2;

/// 单个交易对的预热结果，按完成顺序流式交给实时聚合任务。
pub(super) struct SymbolWarmupResult {
    /// 交易对。
    pub symbol: String,
    /// 四个周期的已确认历史，错误时由实时任务隔离该交易对。
    pub result: Result<Vec<(AggregatedTimeframe, Vec<ConfirmedCandle>)>>,
}

/// 从 quant_core 的交易所事实表加载 OKX 当前可交易永续合约。
pub async fn load_active_okx_perpetual_symbols(
    pool: &PgPool,
    max_symbols: Option<usize>,
) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT upper(exchange_symbol) AS symbol
          FROM exchange_symbols
         WHERE lower(exchange) = 'okx'
           AND lower(market_type) = 'perpetual'
           AND lower(status) IN ('trading', 'live')
           AND exchange_symbol IS NOT NULL
           AND btrim(exchange_symbol) <> ''
         ORDER BY symbol
        "#,
    )
    .fetch_all(pool)
    .await
    .context("load active OKX perpetual symbols from quant_core")?;
    let mut symbols = rows
        .into_iter()
        .map(|row| row.get::<String, _>("symbol"))
        .collect::<Vec<_>>();
    if let Some(max_symbols) = max_symbols {
        symbols.truncate(max_symbols);
    }
    anyhow::ensure!(
        !symbols.is_empty(),
        "no active OKX perpetual symbols found in quant_core.exchange_symbols"
    );
    Ok(symbols)
}

/// 逐交易对预热并立即发送结果，避免全市场预热期间积压多个分钟的实时收盘。
pub(super) async fn stream_symbol_warmups(
    symbols: Vec<String>,
    config: AllMarketCandleVolumeMonitorConfig,
    sender: mpsc::Sender<SymbolWarmupResult>,
) -> Result<()> {
    let client = build_okx_http_client(config.proxy_url.as_deref())?;
    let total = symbols.len();
    for (index, symbol) in symbols.iter().enumerate() {
        let result = warmup_symbol(symbol, &config, &client, Utc::now().timestamp_millis()).await;
        sender
            .send(SymbolWarmupResult {
                symbol: symbol.clone(),
                result,
            })
            .await
            .map_err(|_| anyhow::anyhow!("all-market warmup consumer closed"))?;
        if (index + 1) % 50 == 0 || index + 1 == total {
            info!(
                event = "all_market_candle_warmup_progress",
                completed = index + 1,
                total,
                "全市场 K 线内存预热进度"
            );
        }
    }
    Ok(())
}

/// 优先使用足量且新鲜的 Core 历史；仅对缺失周期调用 REST，降低启动带宽。
async fn warmup_symbol(
    symbol: &str,
    config: &AllMarketCandleVolumeMonitorConfig,
    client: &reqwest::Client,
    now_ms: i64,
) -> Result<Vec<(AggregatedTimeframe, Vec<ConfirmedCandle>)>> {
    let mut histories = Vec::with_capacity(4);
    for timeframe in [
        AggregatedTimeframe::M1,
        AggregatedTimeframe::M5,
        AggregatedTimeframe::M15,
        AggregatedTimeframe::H4,
    ] {
        let required = if timeframe == AggregatedTimeframe::M1 {
            ONE_MINUTE_WARMUP_CANDLES
        } else {
            HIGHER_TIMEFRAME_WARMUP_CANDLES
        };
        let database_history = load_database_history(symbol, timeframe, required)
            .await
            .unwrap_or_default();
        let history = if history_is_fresh(&database_history, timeframe, now_ms) {
            database_history
        } else {
            let start_ms = now_ms.saturating_sub(
                timeframe
                    .duration_ms()
                    .saturating_mul(required.saturating_add(2) as i64),
            );
            let rows = fetch_okx_history_candles(
                client,
                &config.okx_rest_base,
                symbol,
                timeframe.as_str(),
                start_ms,
                now_ms,
                100,
                config.rest_request_sleep_ms,
            )
            .await
            .with_context(|| format!("REST warmup {symbol} {}", timeframe.as_str()))?;
            sleep(Duration::from_millis(config.rest_request_sleep_ms)).await;
            confirmed_from_okx(rows)?
        };
        anyhow::ensure!(
            history.len() >= VOLUME_LOOKBACK,
            "{symbol} {} warmup returned only {} confirmed candles; need at least {VOLUME_LOOKBACK}",
            timeframe.as_str(),
            history.len()
        );
        histories.push((timeframe, history));
    }
    Ok(histories)
}

/// 从 quant_core 读取指定周期最近的已确认 K 线，禁止盘中数据进入基线。
async fn load_database_history(
    symbol: &str,
    timeframe: AggregatedTimeframe,
    limit: usize,
) -> Result<Vec<ConfirmedCandle>> {
    let rows = CandlesModel::new()
        .get_all(SelectCandleReqDto {
            inst_id: symbol.to_string(),
            time_interval: timeframe.as_str().to_string(),
            limit,
            select_time: None,
            confirm: Some(1),
        })
        .await?;
    confirmed_from_entities(rows)
}

/// 将 Core 持久化模型转换为与实时流一致的确认 K 线模型。
fn confirmed_from_entities(rows: Vec<CandlesEntity>) -> Result<Vec<ConfirmedCandle>> {
    let rows = rows
        .into_iter()
        .map(|row| CandleOkxRespDto {
            ts: row.ts.to_string(),
            o: row.o,
            h: row.h,
            l: row.l,
            c: row.c,
            v: row.vol,
            vol_ccy: row.vol_ccy,
            vol_ccy_quote: String::new(),
            confirm: row.confirm,
        })
        .collect::<Vec<_>>();
    confirmed_from_okx(rows)
}

/// 过滤未确认行并按开盘时间排序去重，供预热和缺口修复共同使用。
pub(super) fn confirmed_from_okx(rows: Vec<CandleOkxRespDto>) -> Result<Vec<ConfirmedCandle>> {
    let mut candles = rows
        .iter()
        .filter(|row| row.confirm == "1")
        .map(ConfirmedCandle::try_from_okx)
        .collect::<Result<Vec<_>>>()?;
    candles.sort_unstable_by_key(|candle| candle.open_time_ms);
    candles.dedup_by_key(|candle| candle.open_time_ms);
    Ok(candles)
}

/// 同时检查样本数量与尾部时效，避免用陈旧数据库历史直接启动实时比较。
fn history_is_fresh(
    candles: &[ConfirmedCandle],
    timeframe: AggregatedTimeframe,
    now_ms: i64,
) -> bool {
    let required = if timeframe == AggregatedTimeframe::M1 {
        ONE_MINUTE_WARMUP_CANDLES
    } else {
        HIGHER_TIMEFRAME_WARMUP_CANDLES
    };
    candles.len() >= required
        && candles.last().is_some_and(|last| {
            now_ms.saturating_sub(last.open_time_ms) <= timeframe.duration_ms().saturating_mul(2)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn candle(open_time_ms: i64) -> ConfirmedCandle {
        ConfirmedCandle {
            open_time_ms,
            open: Decimal::ONE,
            high: Decimal::ONE,
            low: Decimal::ONE,
            close: Decimal::ONE,
            volume_contracts: Decimal::ONE,
            volume_base: Decimal::ONE,
            volume_quote: Decimal::ONE,
        }
    }

    #[test]
    fn database_history_must_be_both_complete_and_recent() {
        let now_ms = 20_000_000;
        let recent = (0..ONE_MINUTE_WARMUP_CANDLES)
            .map(|index| candle(now_ms - (ONE_MINUTE_WARMUP_CANDLES - index) as i64 * 60_000))
            .collect::<Vec<_>>();
        assert!(history_is_fresh(&recent, AggregatedTimeframe::M1, now_ms));
        assert!(!history_is_fresh(
            &recent[..VOLUME_LOOKBACK],
            AggregatedTimeframe::M1,
            now_ms
        ));
    }
}
