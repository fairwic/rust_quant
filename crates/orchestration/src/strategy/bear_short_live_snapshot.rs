use anyhow::{anyhow, Context, Result};
use rust_quant_common::CandleItem;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::{StrategyConfig, StrategyType};
use rust_quant_infrastructure::repositories::ShardedExternalMarketSnapshotRepository;
use rust_quant_market::models::{CandlesEntity, SelectTime, TimeDirect};
use rust_quant_services::market::get_confirmed_candles_for_backtest;
use rust_quant_strategies::implementations::{
    BearShortStackBacktestMarketContext, BearShortStackConfig, BearShortStackStrategy,
};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tracing::{info, warn};

const OKX_SOURCE: &str = "okx";
const FUNDING_RATE_METRIC: &str = "funding_rate";
const OPEN_INTEREST_VOLUME_METRIC: &str = "open_interest_volume";
const TAKER_VOLUME_METRIC: &str = "taker_volume";
const LONG_SHORT_RATIO_METRIC: &str = "long_short_ratio";
const LIVE_CANDLE_WARMUP_LIMIT: usize = 600;
const MARKET_CONTEXT_QUERY_LIMIT: i64 = 256;
const MARKET_CONTEXT_QUERY_LOOKBACK_MS: i64 = 72 * 60 * 60 * 1_000;
const MARKET_CONTEXT_MAX_STALENESS_MS: i64 = 36 * 60 * 60 * 1_000;

/// External market context series loaded from the sharded quant_core tables.
#[derive(Debug, Default)]
struct MarketContextSnapshotSeries {
    /// Funding rate snapshots.
    funding: Vec<ExternalMarketSnapshot>,
    /// Open-interest snapshots.
    open_interest: Vec<ExternalMarketSnapshot>,
    /// Taker buy/sell volume snapshots.
    taker: Vec<ExternalMarketSnapshot>,
    /// Long/short ratio snapshots.
    long_short: Vec<ExternalMarketSnapshot>,
}

/// Enriches BearShortStack live configs with a real snapshot built from candles and market context.
pub async fn enrich_bear_short_live_snapshot(
    config: &StrategyConfig,
    inst_id: &str,
    period: &str,
    trigger_candle: Option<&CandlesEntity>,
) -> Result<StrategyConfig> {
    if config.strategy_type != StrategyType::BearShortStack
        || has_existing_snapshot(&config.parameters)
    {
        return Ok(config.clone());
    }
    let Some(trigger_candle) = trigger_candle else {
        return Ok(config.clone());
    };
    let mut bear_config: BearShortStackConfig = serde_json::from_value(config.parameters.clone())
        .context("解析 BearShortStack live 参数失败")?;
    bear_config.apply_strategy_key_preset();

    let candles = load_live_candles(inst_id, period, trigger_candle).await?;
    let Some(market_context) = load_live_market_context(inst_id, trigger_candle.ts).await? else {
        warn!(
            "BearShortStack live snapshot 缺少真实 market context: inst_id={}, period={}, ts={}",
            inst_id, period, trigger_candle.ts
        );
        return Ok(config.clone());
    };
    let Some(snapshot) = BearShortStackStrategy::build_live_snapshot_from_context(
        bear_config.preset,
        inst_id,
        &candles,
        market_context,
    ) else {
        info!(
            "BearShortStack live K线结构未满足，保留 flat 门禁: inst_id={}, period={}, preset={:?}, ts={}",
            inst_id, period, bear_config.preset, trigger_candle.ts
        );
        return Ok(config.clone());
    };

    let mut enriched = config.clone();
    enriched.parameters =
        insert_snapshot(config.parameters.clone(), serde_json::to_value(snapshot)?);
    Ok(enriched)
}

/// Checks whether caller-provided config already contains an explicit snapshot.
fn has_existing_snapshot(parameters: &Value) -> bool {
    parameters
        .get("snapshot")
        .is_some_and(|snapshot| !snapshot.is_null())
}

/// Inserts the computed live snapshot while preserving unrelated strategy parameters.
fn insert_snapshot(mut parameters: Value, snapshot: Value) -> Value {
    match &mut parameters {
        Value::Object(map) => {
            map.insert("snapshot".to_string(), snapshot);
            parameters
        }
        _ => serde_json::json!({ "snapshot": snapshot }),
    }
}

/// Loads confirmed candles up to the trigger candle and prevents old replays from seeing future data.
async fn load_live_candles(
    inst_id: &str,
    period: &str,
    trigger_candle: &CandlesEntity,
) -> Result<Vec<CandleItem>> {
    let select_time = Some(SelectTime {
        start_time: trigger_candle.ts,
        end_time: None,
        direct: TimeDirect::BEFORE,
    });
    let mut entities =
        get_confirmed_candles_for_backtest(inst_id, period, LIVE_CANDLE_WARMUP_LIMIT, select_time)
            .await?;
    entities.push(trigger_candle.clone());
    let mut candles = entities
        .iter()
        .map(|entity| candle_entity_to_item(entity, inst_id, period))
        .collect::<Result<Vec<_>>>()?;
    candles.retain(|candle| candle.ts <= trigger_candle.ts);
    candles.sort_unstable_by_key(|candle| candle.ts);
    candles.dedup_by_key(|candle| candle.ts);
    Ok(candles)
}

/// Converts persisted candle rows into the strategy-facing candle contract.
fn candle_entity_to_item(
    entity: &CandlesEntity,
    inst_id: &str,
    period: &str,
) -> Result<CandleItem> {
    Ok(CandleItem {
        ts: entity.ts,
        o: parse_candle_number(&entity.o, "open", entity.ts, inst_id, period)?,
        h: parse_candle_number(&entity.h, "high", entity.ts, inst_id, period)?,
        l: parse_candle_number(&entity.l, "low", entity.ts, inst_id, period)?,
        c: parse_candle_number(&entity.c, "close", entity.ts, inst_id, period)?,
        v: parse_candle_number(&entity.vol_ccy, "volume", entity.ts, inst_id, period)?,
        confirm: entity.confirm.parse::<i32>().unwrap_or(1),
    })
}

/// Parses a candle numeric field and includes enough context for production logs.
fn parse_candle_number(
    value: &str,
    field: &str,
    ts: i64,
    inst_id: &str,
    period: &str,
) -> Result<f64> {
    value.parse::<f64>().with_context(|| {
        format!("invalid candle {field}: inst_id={inst_id} period={period} ts={ts}")
    })
}

/// Loads recent sharded market snapshots without creating missing shards.
async fn load_live_market_context(
    inst_id: &str,
    trigger_ts: i64,
) -> Result<Option<BearShortStackBacktestMarketContext>> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .map_err(|_| anyhow!("BearShortStack live context requires QUANT_CORE_DATABASE_URL"))?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&database_url)
        .context("创建 quant_core market context 连接池失败")?;
    let repo = ShardedExternalMarketSnapshotRepository::new(pool);
    let start_time = trigger_ts.saturating_sub(MARKET_CONTEXT_QUERY_LOOKBACK_MS);
    let series = MarketContextSnapshotSeries {
        funding: load_metric(&repo, inst_id, FUNDING_RATE_METRIC, start_time, trigger_ts).await?,
        open_interest: load_metric(
            &repo,
            inst_id,
            OPEN_INTEREST_VOLUME_METRIC,
            start_time,
            trigger_ts,
        )
        .await?,
        taker: load_metric(&repo, inst_id, TAKER_VOLUME_METRIC, start_time, trigger_ts).await?,
        long_short: load_metric(
            &repo,
            inst_id,
            LONG_SHORT_RATIO_METRIC,
            start_time,
            trigger_ts,
        )
        .await?,
    };
    Ok(build_market_context_at(&series, trigger_ts))
}

/// Loads one metric series from an existing sharded market context table.
async fn load_metric(
    repo: &ShardedExternalMarketSnapshotRepository,
    inst_id: &str,
    metric_type: &str,
    start_time: i64,
    end_time: i64,
) -> Result<Vec<ExternalMarketSnapshot>> {
    let mut rows = repo
        .find_range_existing(
            OKX_SOURCE,
            inst_id,
            metric_type,
            start_time,
            end_time,
            Some(MARKET_CONTEXT_QUERY_LIMIT),
        )
        .await
        .with_context(|| {
            format!("load OKX market context failed: inst_id={inst_id} metric_type={metric_type}")
        })?;
    rows.sort_unstable_by_key(|row| row.metric_time);
    Ok(rows)
}

/// Builds the BearShortStack market context at the trigger timestamp.
fn build_market_context_at(
    series: &MarketContextSnapshotSeries,
    trigger_ts: i64,
) -> Option<BearShortStackBacktestMarketContext> {
    let funding = latest_fresh_snapshot_at(&series.funding, trigger_ts)
        .and_then(|snapshot| snapshot.funding_rate)?;
    let (oi_growth_pct, _latest_oi) = oi_growth_at(&series.open_interest, trigger_ts)?;
    let (taker_buy, taker_sell) =
        latest_fresh_snapshot_at(&series.taker, trigger_ts).and_then(taker_volumes)?;
    let long_short_ratio = latest_fresh_snapshot_at(&series.long_short, trigger_ts)
        .and_then(|snapshot| snapshot.long_short_ratio)?;
    Some(BearShortStackBacktestMarketContext {
        ts: trigger_ts,
        funding_rate: funding,
        oi_growth_pct,
        long_short_ratio,
        taker_buy_volume: taker_buy,
        taker_sell_volume: taker_sell,
    })
}

/// Returns the newest snapshot at or before the trigger, subject to the live freshness window.
fn latest_fresh_snapshot_at(
    rows: &[ExternalMarketSnapshot],
    trigger_ts: i64,
) -> Option<&ExternalMarketSnapshot> {
    let snapshot = rows
        .iter()
        .take_while(|row| row.metric_time <= trigger_ts)
        .last()?;
    if trigger_ts.saturating_sub(snapshot.metric_time) > MARKET_CONTEXT_MAX_STALENESS_MS {
        return None;
    }
    Some(snapshot)
}

/// Computes OI growth from the latest fresh OI point and the previous OI point.
fn oi_growth_at(rows: &[ExternalMarketSnapshot], trigger_ts: i64) -> Option<(f64, f64)> {
    let latest = latest_fresh_snapshot_at(rows, trigger_ts)?;
    let latest_index = rows
        .iter()
        .position(|row| row.metric_time == latest.metric_time && row.open_interest.is_some())?;
    let previous_index = rows[..latest_index]
        .iter()
        .enumerate()
        .filter(|(_, row)| row.open_interest.is_some())
        .map(|(index, _)| index)
        .last()?;
    let latest_oi = rows[latest_index].open_interest?;
    let previous_oi = rows[previous_index].open_interest?;
    if previous_oi.abs() <= f64::EPSILON {
        return None;
    }
    Some((
        (latest_oi - previous_oi) / previous_oi.abs() * 100.0,
        latest_oi,
    ))
}

/// Extracts taker buy/sell volumes from the raw exchange payload.
fn taker_volumes(snapshot: &ExternalMarketSnapshot) -> Option<(f64, f64)> {
    let payload = snapshot.raw_payload.as_ref()?;
    let buy = payload_number(payload, &["buy_volume", "buyVolume", "buyVol"])?;
    let sell = payload_number(payload, &["sell_volume", "sellVolume", "sellVol"])?;
    Some((buy, sell))
}

/// Reads a numeric payload field across the naming variants produced by OKX collectors.
fn payload_number(payload: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| match payload.get(*key)? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds one test snapshot with optional metric fields.
    fn snapshot(
        metric_type: &str,
        metric_time: i64,
        funding_rate: Option<f64>,
        open_interest: Option<f64>,
        long_short_ratio: Option<f64>,
        raw_payload: Option<Value>,
    ) -> ExternalMarketSnapshot {
        ExternalMarketSnapshot {
            id: None,
            source: OKX_SOURCE.to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            metric_type: metric_type.to_string(),
            metric_time,
            funding_rate,
            premium: None,
            open_interest,
            oracle_price: None,
            mark_price: None,
            long_short_ratio,
            raw_payload,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn build_market_context_requires_real_fresh_series() {
        let trigger_ts = 1_800_000_000_000;
        let series = MarketContextSnapshotSeries {
            funding: vec![snapshot(
                FUNDING_RATE_METRIC,
                trigger_ts,
                Some(0.00004),
                None,
                None,
                None,
            )],
            open_interest: vec![
                snapshot(
                    OPEN_INTEREST_VOLUME_METRIC,
                    trigger_ts - 86_400_000,
                    None,
                    Some(100.0),
                    None,
                    None,
                ),
                snapshot(
                    OPEN_INTEREST_VOLUME_METRIC,
                    trigger_ts,
                    None,
                    Some(104.0),
                    None,
                    None,
                ),
            ],
            taker: vec![snapshot(
                TAKER_VOLUME_METRIC,
                trigger_ts,
                None,
                None,
                None,
                Some(json!({"buy_volume": "10.0", "sell_volume": "25.0"})),
            )],
            long_short: vec![snapshot(
                LONG_SHORT_RATIO_METRIC,
                trigger_ts,
                None,
                None,
                Some(1.24),
                None,
            )],
        };

        let context = build_market_context_at(&series, trigger_ts).expect("fresh context");

        assert_eq!(context.funding_rate, 0.00004);
        assert_eq!(context.oi_growth_pct, 4.0);
        assert_eq!(context.taker_buy_volume, 10.0);
        assert_eq!(context.taker_sell_volume, 25.0);
        assert_eq!(context.long_short_ratio, 1.24);
    }

    #[test]
    fn build_market_context_rejects_stale_funding() {
        let trigger_ts = 1_800_000_000_000;
        let stale_ts = trigger_ts - MARKET_CONTEXT_MAX_STALENESS_MS - 1;
        let series = MarketContextSnapshotSeries {
            funding: vec![snapshot(
                FUNDING_RATE_METRIC,
                stale_ts,
                Some(0.00004),
                None,
                None,
                None,
            )],
            open_interest: vec![
                snapshot(
                    OPEN_INTEREST_VOLUME_METRIC,
                    trigger_ts - 86_400_000,
                    None,
                    Some(100.0),
                    None,
                    None,
                ),
                snapshot(
                    OPEN_INTEREST_VOLUME_METRIC,
                    trigger_ts,
                    None,
                    Some(104.0),
                    None,
                    None,
                ),
            ],
            taker: vec![snapshot(
                TAKER_VOLUME_METRIC,
                trigger_ts,
                None,
                None,
                None,
                Some(json!({"buyVolume": 10.0, "sellVolume": 25.0})),
            )],
            long_short: vec![snapshot(
                LONG_SHORT_RATIO_METRIC,
                trigger_ts,
                None,
                None,
                Some(1.24),
                None,
            )],
        };

        assert!(build_market_context_at(&series, trigger_ts).is_none());
    }
}
