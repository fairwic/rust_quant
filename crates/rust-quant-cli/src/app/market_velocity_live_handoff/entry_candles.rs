use super::super::market_velocity_backfill::{
    fetch_okx_history_candles, is_okx_missing_instrument_error, mark_okx_exchange_symbol_deleted,
};
use super::MarketVelocityLiveHandoffConfig;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use okx::dto::market_dto::CandleOkxRespDto;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use serde::Serialize;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::{future::Future, time::Duration};
use tokio::time::sleep;
use tracing::warn;
const ENTRY_CANDLE_FETCH_MAX_ATTEMPTS: usize = 3;
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
    /// 是否已把刷新得到的 K 线落库。
    pub persisted_to_db: bool,
    /// 落库影响行数。
    pub rows_upserted: u64,
    /// 落库错误；不阻断本次内存入场判断。
    pub persist_error: Option<String>,
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
    let query = entry_candle_load_sql(&quote_identifier(&table_name)?);
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
            if row.get::<String, _>("confirm").trim() == "1" {
                candle.confirm();
            }
            Ok(candle)
        })
        .collect()
}
/// DB candles can include an in-progress latest row, so live entry checks must read exchange confirmation state.
fn entry_candle_load_sql(table_name: &str) -> String {
    format!("SELECT ts, o, h, l, c, vol, confirm FROM {table_name} ORDER BY ts DESC LIMIT $1")
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
                    persisted_to_db: false,
                    rows_upserted: 0,
                    persist_error: None,
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
                            persisted_to_db: false,
                            rows_upserted: 0,
                            persist_error: None,
                        },
                    }
                });
            };
            let fetch_result = fetch_market_velocity_latest_entry_candles(
                pool,
                client,
                config,
                symbol,
                limit.max(1),
            )
            .await;
            if config.entry_candle_request_sleep_ms > 0 {
                sleep(Duration::from_millis(config.entry_candle_request_sleep_ms)).await;
            }
            let candles = fetch_result?;
            let candle_count = candles.len();
            let persist_result =
                persist_market_velocity_entry_candles(pool, &candles, db_error.is_some()).await;
            let (persisted_to_db, rows_upserted, persist_error) = match persist_result {
                Ok(rows_upserted) => (!candles.is_empty(), rows_upserted, None),
                Err(error) => {
                    warn!(
                        symbol,
                        error = %error,
                        "failed to persist on-demand Market Velocity entry candles"
                    );
                    (false, 0, Some(error.to_string()))
                }
            };
            Ok(MarketVelocityEntryCandleLoad {
                candles,
                status: MarketVelocityEntryCandleLoadStatus {
                    source: "okx_history_candles_on_demand".to_string(),
                    refreshed_from_exchange: true,
                    db_error,
                    candle_count,
                    persisted_to_db,
                    rows_upserted,
                    persist_error,
                },
            })
        }
    }
}
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
async fn fetch_market_velocity_latest_entry_candles(
    pool: &PgPool,
    client: &reqwest::Client,
    config: &MarketVelocityLiveHandoffConfig,
    symbol: &str,
    limit: u32,
) -> Result<Vec<Candle>> {
    let now_ms = Utc::now().timestamp_millis();
    let candle_window_ms = i64::from(limit.max(1)) * 15 * 60 * 1_000;
    let start_ms = now_ms - candle_window_ms.saturating_mul(2);
    let page_limit = usize::try_from(limit.min(100)).unwrap_or(100).max(1);
    let candles = match fetch_entry_candles_with_retry(
        || {
            fetch_okx_history_candles(
                client,
                &config.entry_candle_okx_rest_base,
                symbol,
                "15m",
                start_ms,
                now_ms,
                page_limit,
                config.entry_candle_request_sleep_ms,
            )
        },
        config.entry_candle_request_sleep_ms,
    )
    .await
    {
        Ok(candles) => candles,
        Err(error) => {
            if is_okx_missing_instrument_error(&error) {
                match mark_okx_exchange_symbol_deleted(pool, symbol).await {
                    Ok(rows) => warn!(
                        symbol,
                        rows_affected = rows,
                        "marked OKX exchange symbol deleted after on-demand missing instrument response"
                    ),
                    Err(mark_error) => warn!(
                        symbol,
                        error = %mark_error,
                        "failed to mark OKX exchange symbol deleted after on-demand missing instrument response"
                    ),
                }
            }
            bail!("on-demand fetch latest 15m candles failed: symbol={symbol}: {error:#}");
        }
    };
    okx_candles_to_market_velocity_domain(symbol, candles)
}
/// 对 OKX 公共 K 线做有限重试，降低临时限频/网络抖动对 live handoff 的误阻断。
async fn fetch_entry_candles_with_retry<F, Fut>(
    mut fetch: F,
    request_sleep_ms: u64,
) -> Result<Vec<CandleOkxRespDto>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Vec<CandleOkxRespDto>>>,
{
    let mut last_error = None;
    for attempt in 1..=ENTRY_CANDLE_FETCH_MAX_ATTEMPTS {
        match fetch().await {
            Ok(candles) => return Ok(candles),
            Err(error) => {
                if attempt == ENTRY_CANDLE_FETCH_MAX_ATTEMPTS {
                    return Err(error).with_context(|| {
                        format!("OKX history-candles failed after {attempt} attempts")
                    });
                }
                warn!(
                    attempt,
                    max_attempts = ENTRY_CANDLE_FETCH_MAX_ATTEMPTS,
                    error = %error,
                    "retrying Market Velocity entry candle fetch after transient failure"
                );
                last_error = Some(error);
                let delay_ms = request_sleep_ms.saturating_mul(attempt as u64);
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }
    Err(last_error
        .map(|error| anyhow!("OKX history-candles failed: {error}"))
        .unwrap_or_else(|| anyhow!("OKX history-candles failed before first attempt")))
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
/// 提供入场K线持久化目标的集中实现，避免调用方重复推断 symbol 与周期。
fn entry_candle_persist_target(candles: &[Candle]) -> Result<Option<(&str, Timeframe)>> {
    let Some(first) = candles.first() else {
        return Ok(None);
    };
    if first.timeframe != Timeframe::M15 {
        bail!("entry candle persist target requires 15m candles");
    }
    if candles
        .iter()
        .any(|candle| candle.timeframe != Timeframe::M15)
    {
        bail!("entry candle persist target requires 15m candles");
    }
    if let Some(mixed) = candles.iter().find(|candle| candle.symbol != first.symbol) {
        bail!(
            "entry candle persist target does not support mixed symbols: {} vs {}",
            first.symbol,
            mixed.symbol
        );
    }
    Ok(Some((first.symbol.as_str(), first.timeframe)))
}
/// 持久化按需刷新的入场 K 线；失败由调用方降级，不阻断本次内存判断。
async fn persist_market_velocity_entry_candles(
    pool: &PgPool,
    candles: &[Candle],
    ensure_table: bool,
) -> Result<u64> {
    let Some((symbol, timeframe)) = entry_candle_persist_target(candles)? else {
        return Ok(0);
    };
    if ensure_table {
        let repository = PostgresCandleRepository::new(pool.clone());
        repository.ensure_table(symbol, timeframe).await?;
    }
    let table_name = PostgresCandleRepository::quoted_table_name(symbol, timeframe)?;
    let mut query_builder = build_entry_candle_batch_upsert_query(&table_name, candles);
    let result = query_builder.build().execute(pool).await?;
    Ok(result.rows_affected())
}
/// 构造单条批量 upsert，避免按每根 K 线逐条写库。
fn build_entry_candle_batch_upsert_query<'a>(
    table_name: &str,
    candles: &'a [Candle],
) -> QueryBuilder<'a, Postgres> {
    let mut query_builder = QueryBuilder::new(format!(
        "INSERT INTO {} (ts, o, h, l, c, vol, vol_ccy, confirm) ",
        table_name
    ));
    query_builder.push_values(candles.iter(), |mut row, candle| {
        row.push_bind(candle.timestamp)
            .push_bind(candle.open.value().to_string())
            .push_bind(candle.high.value().to_string())
            .push_bind(candle.low.value().to_string())
            .push_bind(candle.close.value().to_string())
            .push_bind(candle.volume.value().to_string())
            .push_bind(candle.volume.value().to_string())
            .push_bind(if candle.confirmed { "1" } else { "0" });
    });
    query_builder.push(
        " ON CONFLICT (ts) DO UPDATE SET
            o = EXCLUDED.o,
            h = EXCLUDED.h,
            l = EXCLUDED.l,
            c = EXCLUDED.c,
            vol = EXCLUDED.vol,
            vol_ccy = EXCLUDED.vol_ccy,
            confirm = EXCLUDED.confirm,
            updated_at = CURRENT_TIMESTAMP",
    );
    query_builder
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sqlx::Execute;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
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
    #[test]
    fn entry_candle_persist_target_requires_single_symbol_and_15m() {
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 11, 30, 0).unwrap();
        let cap = sample_symbol_candle_at("CAP-USDT-SWAP", Timeframe::M15, now);
        let ip = sample_symbol_candle_at("IP-USDT-SWAP", Timeframe::M15, now);
        let cap_4h = sample_symbol_candle_at("CAP-USDT-SWAP", Timeframe::H4, now);

        assert_eq!(
            entry_candle_persist_target(&[cap.clone()]).unwrap(),
            Some(("CAP-USDT-SWAP", Timeframe::M15))
        );
        assert!(entry_candle_persist_target(&[cap.clone(), ip])
            .unwrap_err()
            .to_string()
            .contains("mixed symbols"));
        assert!(entry_candle_persist_target(&[cap_4h])
            .unwrap_err()
            .to_string()
            .contains("15m"));
        assert_eq!(entry_candle_persist_target(&[]).unwrap(), None);
    }
    #[test]
    fn entry_candle_persist_query_uses_single_batch_upsert() {
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 11, 30, 0).unwrap();
        let candles = vec![
            sample_symbol_candle_at("CAP-USDT-SWAP", Timeframe::M15, now),
            sample_symbol_candle_at(
                "CAP-USDT-SWAP",
                Timeframe::M15,
                now + chrono::Duration::minutes(15),
            ),
        ];
        let mut query_builder =
            build_entry_candle_batch_upsert_query("\"cap-usdt-swap_candles_15m\"", &candles);
        let sql = query_builder.build().sql().to_string();

        assert_eq!(sql.matches("INSERT INTO").count(), 1);
        assert_eq!(sql.matches("ON CONFLICT (ts) DO UPDATE").count(), 1);
        assert!(sql.contains("\"cap-usdt-swap_candles_15m\""));
    }
    #[test]
    fn entry_candle_db_load_query_reads_exchange_confirm_state() {
        let sql = entry_candle_load_sql("\"cap-usdt-swap_candles_15m\"");

        assert!(sql.contains("confirm"));
        assert!(sql.contains("ORDER BY ts DESC LIMIT $1"));
    }
    #[tokio::test]
    async fn entry_candle_fetch_retry_recovers_from_first_transient_failure() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_fetch = Arc::clone(&attempts);

        let candles = fetch_entry_candles_with_retry(
            || {
                let attempts_for_fetch = Arc::clone(&attempts_for_fetch);
                async move {
                    let attempt = attempts_for_fetch.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        return Err(anyhow!("temporary OKX history-candles failure"));
                    }
                    Ok(vec![sample_okx_candle_row(1_781_503_200_000)])
                }
            },
            0,
        )
        .await
        .expect("second attempt should recover");

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].ts, "1781503200000");
    }
    /// 构造样例K 线at，集中维护行情数据的载荷组装规则。
    fn sample_candle_at(datetime: DateTime<Utc>) -> Candle {
        sample_symbol_candle_at("ASTER-USDT-SWAP", Timeframe::M15, datetime)
    }
    /// 构造样例K 线at，集中维护行情数据的载荷组装规则。
    fn sample_symbol_candle_at(
        symbol: &str,
        timeframe: Timeframe,
        datetime: DateTime<Utc>,
    ) -> Candle {
        let mut candle = Candle::new(
            symbol.to_string(),
            timeframe,
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
    /// 构造 OKX 样例 K 线行，避免测试依赖真实网络。
    fn sample_okx_candle_row(ts: i64) -> CandleOkxRespDto {
        CandleOkxRespDto {
            ts: ts.to_string(),
            o: "100".to_string(),
            h: "103".to_string(),
            l: "99".to_string(),
            c: "102".to_string(),
            v: "10000".to_string(),
            vol_ccy: "10000".to_string(),
            vol_ccy_quote: "1020000".to_string(),
            confirm: "1".to_string(),
        }
    }
}
