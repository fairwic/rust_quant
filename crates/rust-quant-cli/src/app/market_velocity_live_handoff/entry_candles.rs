use super::super::market_velocity_backfill::fetch_okx_history_candles;
use super::MarketVelocityLiveHandoffConfig;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use okx::dto::market_dto::CandleOkxRespDto;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use serde::Serialize;
use sqlx::{PgPool, Row};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarketVelocityEntryCandleLoadStatus {
    /// 数据来源。
    pub source: String,
    /// refreshedfrom交易所，用于行情、K 线或市场扫描。
    pub refreshed_from_exchange: bool,
    /// 错误信息。
    pub db_error: Option<String>,
    /// K 线数量。
    pub candle_count: usize,
}
#[derive(Debug, Clone)]
pub(super) struct MarketVelocityEntryCandleLoad {
    /// 列表数据。
    pub(super) candles: Vec<Candle>,
    /// 当前状态。
    pub(super) status: MarketVelocityEntryCandleLoadStatus,
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
async fn load_market_velocity_entry_candles(
    pool: &PgPool,
    symbol: &str,
    limit: u32,
) -> Result<Vec<Candle>> {
    let table_name = format!("{}_candles_15m", symbol.trim().to_ascii_lowercase());
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM {} ORDER BY ts DESC LIMIT $1",
        quote_identifier(&table_name)?
    );
    let mut rows = sqlx::query(&query)
        .bind(i64::from(limit.max(1)))
        .fetch_all(pool)
        .await
        .with_context(|| format!("load 15m entry candles from {table_name}"))?;
    rows.reverse();
    rows.into_iter()
        .map(|row| {
            let ts: i64 = row.get("ts");
            let mut candle = Candle::new(
                symbol.to_string(),
                Timeframe::M15,
                ts,
                Price::new(parse_decimal_text(row.get::<String, _>("o").as_str())?)?,
                Price::new(parse_decimal_text(row.get::<String, _>("h").as_str())?)?,
                Price::new(parse_decimal_text(row.get::<String, _>("l").as_str())?)?,
                Price::new(parse_decimal_text(row.get::<String, _>("c").as_str())?)?,
                Volume::new(parse_decimal_text(row.get::<String, _>("vol").as_str())?)?,
            );
            candle.confirm();
            Ok(candle)
        })
        .collect()
}
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
pub(super) async fn load_market_velocity_live_entry_candles(
    pool: &PgPool,
    refresh_client: Option<&reqwest::Client>,
    config: &MarketVelocityLiveHandoffConfig,
    symbol: &str,
    limit: u32,
) -> Result<MarketVelocityEntryCandleLoad> {
    let db_result = load_market_velocity_entry_candles(pool, symbol, limit).await;
    let now = Utc::now();
    match db_result {
        Ok(candles)
            if !market_velocity_entry_candles_need_refresh(
                &candles,
                now,
                config.entry_candle_max_staleness_minutes,
            ) =>
        {
            let candle_count = candles.len();
            Ok(MarketVelocityEntryCandleLoad {
                candles,
                status: MarketVelocityEntryCandleLoadStatus {
                    source: "quant_core_db".to_string(),
                    refreshed_from_exchange: false,
                    db_error: None,
                    candle_count,
                },
            })
        }
        db_result => {
            let db_error = db_result.as_ref().err().map(ToString::to_string);
            let Some(client) = refresh_client else {
                return db_result.map(|candles| {
                    let candle_count = candles.len();
                    MarketVelocityEntryCandleLoad {
                        candles,
                        status: MarketVelocityEntryCandleLoadStatus {
                            source: "quant_core_db_stale_refresh_disabled".to_string(),
                            refreshed_from_exchange: false,
                            db_error: None,
                            candle_count,
                        },
                    }
                });
            };
            let candles =
                fetch_market_velocity_latest_entry_candles(client, config, symbol, limit.max(1))
                    .await?;
            let candle_count = candles.len();
            Ok(MarketVelocityEntryCandleLoad {
                candles,
                status: MarketVelocityEntryCandleLoadStatus {
                    source: "okx_history_candles_on_demand".to_string(),
                    refreshed_from_exchange: true,
                    db_error,
                    candle_count,
                },
            })
        }
    }
}
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
async fn fetch_market_velocity_latest_entry_candles(
    client: &reqwest::Client,
    config: &MarketVelocityLiveHandoffConfig,
    symbol: &str,
    limit: u32,
) -> Result<Vec<Candle>> {
    let now_ms = Utc::now().timestamp_millis();
    let candle_window_ms = i64::from(limit.max(1)) * 15 * 60 * 1_000;
    let start_ms = now_ms - candle_window_ms.saturating_mul(2);
    let page_limit = usize::try_from(limit.min(100)).unwrap_or(100).max(1);
    let candles = fetch_okx_history_candles(
        client,
        &config.entry_candle_okx_rest_base,
        symbol,
        "15m",
        start_ms,
        now_ms,
        page_limit,
        config.entry_candle_request_sleep_ms,
    )
    .await
    .with_context(|| format!("on-demand fetch latest 15m candles failed: symbol={symbol}"))?;
    okx_candles_to_market_velocity_domain(symbol, candles)
}
/// 提供OKXK 线to市场动量domain的集中实现，避免行情数据调用方重复处理相同细节。
fn okx_candles_to_market_velocity_domain(
    symbol: &str,
    candles: Vec<CandleOkxRespDto>,
) -> Result<Vec<Candle>> {
    let mut converted = candles
        .into_iter()
        .map(|row| {
            let ts = row
                .ts
                .parse::<i64>()
                .with_context(|| format!("invalid OKX candle timestamp: {}", row.ts))?;
            let mut candle = Candle::new(
                symbol.to_string(),
                Timeframe::M15,
                ts,
                Price::new(parse_decimal_text(&row.o)?)?,
                Price::new(parse_decimal_text(&row.h)?)?,
                Price::new(parse_decimal_text(&row.l)?)?,
                Price::new(parse_decimal_text(&row.c)?)?,
                Volume::new(parse_decimal_text(&row.v)?)?,
            );
            if row.confirm.trim() == "1" {
                candle.confirm();
            }
            Ok(candle)
        })
        .collect::<Result<Vec<_>>>()?;
    converted.sort_by_key(|candle| candle.timestamp);
    Ok(converted)
}
/// 提供市场动量入场K 线needrefresh的集中实现，避免行情数据调用方重复处理相同细节。
fn market_velocity_entry_candles_need_refresh(
    candles: &[Candle],
    now: DateTime<Utc>,
    max_staleness_minutes: i64,
) -> bool {
    let Some(latest) = candles.last() else {
        return true;
    };
    if max_staleness_minutes <= 0 {
        return false;
    }
    let age_seconds = now
        .signed_duration_since(latest.datetime)
        .num_seconds()
        .max(0);
    let age_minutes = (age_seconds + 59) / 60;
    age_minutes > max_staleness_minutes
}
/// 提供quoteidentifier的集中实现，避免行情数据调用方重复处理相同细节。
fn quote_identifier(identifier: &str) -> Result<String> {
    if identifier.is_empty()
        || !identifier
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        bail!("unsafe table identifier: {identifier}");
    }
    Ok(format!("\"{}\"", identifier.replace('"', "\"\"")))
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_decimal_text(value: &str) -> Result<f64> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid decimal {value}: {error}"))?;
    if !parsed.is_finite() {
        bail!("decimal must be finite: {value}");
    }
    Ok(parsed)
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn entry_candle_on_demand_refresh_only_runs_for_missing_or_stale_db_candles() {
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 11, 30, 0).unwrap();
        let fresh = vec![sample_candle_at(now - chrono::Duration::minutes(30))];
        let stale = vec![sample_candle_at(now - chrono::Duration::minutes(90))];
        assert!(market_velocity_entry_candles_need_refresh(&[], now, 45));
        assert!(!market_velocity_entry_candles_need_refresh(&fresh, now, 45));
        assert!(market_velocity_entry_candles_need_refresh(&stale, now, 45));
        assert!(!market_velocity_entry_candles_need_refresh(&stale, now, 0));
    }
    /// 构造样例K 线at，集中维护行情数据的载荷组装规则。
    fn sample_candle_at(datetime: DateTime<Utc>) -> Candle {
        let mut candle = Candle::new(
            "ASTER-USDT-SWAP".to_string(),
            Timeframe::M15,
            datetime.timestamp_millis(),
            Price::new(100.0).unwrap(),
            Price::new(103.0).unwrap(),
            Price::new(99.0).unwrap(),
            Price::new(102.0).unwrap(),
            Volume::new(10_000.0).unwrap(),
        );
        candle.confirm();
        candle
    }
}
