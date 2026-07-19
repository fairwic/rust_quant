use super::args::{Args, PortfolioUniverse};
use super::{quoted_4h_candle_table, CandidateTrade};
use anyhow::{Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1_000;
const LOOKBACK_DAYS: i64 = 30;
const MIN_LISTING_DAYS: i64 = 150;
const TARGET_SIZE: usize = 100;
const LOAD_LIVE_SYMBOLS_SQL: &str = r#"
    SELECT DISTINCT ON (upper(normalized_symbol))
           upper(normalized_symbol) AS symbol,
           NULLIF(raw_payload->>'listTime', '')::bigint AS listed_at_ms
      FROM exchange_symbols
     WHERE exchange = 'okx'
       AND market_type = 'perpetual'
       AND lower(status) IN ('trading', 'live')
       AND upper(normalized_symbol) LIKE '%-USDT-SWAP'
       AND raw_payload->>'instCategory' = '1'
     ORDER BY upper(normalized_symbol), updated_at DESC
"#;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct LiveUniverseReport {
    pub(super) rule: &'static str,
    pub(super) delisted_symbols_excluded: bool,
    pub(super) survivorship_bias_accepted: bool,
    pub(super) current_live_symbols: usize,
    pub(super) current_live_symbols_with_4h_table: usize,
    pub(super) trades_before_filter: usize,
    pub(super) trades_after_filter: usize,
    pub(super) trades_filtered_out: usize,
    pub(super) monthly: Vec<MonthlyUniverseReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct MonthlyUniverseReport {
    pub(super) month: String,
    pub(super) eligible_symbols: usize,
    pub(super) symbols_with_complete_volume_window: usize,
    pub(super) selected_symbols: usize,
    pub(super) additions_from_prior_month: Option<usize>,
    pub(super) cutoff_median_daily_quote_volume: Option<f64>,
}

#[derive(Debug, Clone)]
struct LiveSymbol {
    symbol: String,
    listed_at_ms: i64,
}

#[derive(Debug, Clone, Copy)]
struct VolumeBar {
    ts: i64,
    quote_volume: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct RankedVolume {
    symbol: String,
    median_daily_quote_volume: f64,
}

pub(super) async fn apply_live_universe(
    pool: &PgPool,
    trades: &mut Vec<CandidateTrade>,
    args: Args,
) -> Result<Option<LiveUniverseReport>> {
    if args.portfolio_universe == PortfolioUniverse::Backtest {
        return Ok(None);
    }
    let trades_before = trades.len();
    let month_starts = trade_month_starts(trades)?;
    if month_starts.is_empty() {
        return Ok(Some(LiveUniverseReport {
            rule: universe_rule(),
            delisted_symbols_excluded: true,
            survivorship_bias_accepted: true,
            current_live_symbols: 0,
            current_live_symbols_with_4h_table: 0,
            trades_before_filter: 0,
            trades_after_filter: 0,
            trades_filtered_out: 0,
            monthly: Vec::new(),
        }));
    }
    let live_symbols = load_live_symbols(pool).await?;
    let symbols_with_tables = load_symbols_with_4h_tables(pool, &live_symbols).await?;
    let first_window_start = month_starts[0] - LOOKBACK_DAYS * DAY_MS;
    let last_window_end = *month_starts
        .last()
        .context("missing final universe month")?;
    let bars = load_volume_bars(
        pool,
        &symbols_with_tables,
        first_window_start,
        last_window_end,
    )
    .await?;
    let (memberships, monthly) =
        build_monthly_memberships(&month_starts, &symbols_with_tables, &bars, TARGET_SIZE);
    trades.retain(|trade| {
        month_start_ms(trade.open_ts)
            .ok()
            .and_then(|month| memberships.get(&month))
            .is_some_and(|members| members.contains(&trade.symbol))
    });
    Ok(Some(LiveUniverseReport {
        rule: universe_rule(),
        delisted_symbols_excluded: true,
        survivorship_bias_accepted: true,
        current_live_symbols: live_symbols.len(),
        current_live_symbols_with_4h_table: symbols_with_tables.len(),
        trades_before_filter: trades_before,
        trades_after_filter: trades.len(),
        trades_filtered_out: trades_before - trades.len(),
        monthly,
    }))
}

fn universe_rule() -> &'static str {
    "current_live_okx_crypto_usdt_swaps_age150d_monthly_prior30_complete_utc_days_median_quote_volume_top100_exclude_delisted"
}

async fn load_live_symbols(pool: &PgPool) -> Result<Vec<LiveSymbol>> {
    let rows = sqlx::query(LOAD_LIVE_SYMBOLS_SQL)
        .fetch_all(pool)
        .await
        .context("load current live OKX USDT swaps")?;
    let mut symbols = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(listed_at_ms) = row.try_get::<Option<i64>, _>("listed_at_ms")? else {
            continue;
        };
        symbols.push(LiveSymbol {
            symbol: row.try_get("symbol")?,
            listed_at_ms,
        });
    }
    Ok(symbols)
}

async fn load_symbols_with_4h_tables(
    pool: &PgPool,
    symbols: &[LiveSymbol],
) -> Result<Vec<LiveSymbol>> {
    let table_names = sqlx::query_scalar::<_, String>(
        r#"
        SELECT table_name
          FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name LIKE '%-usdt-swap_candles_4h'
        "#,
    )
    .fetch_all(pool)
    .await
    .context("load local 4H candle table inventory")?
    .into_iter()
    .collect::<HashSet<_>>();
    Ok(symbols
        .iter()
        .filter(|symbol| {
            table_names.contains(&format!(
                "{}_candles_4h",
                symbol.symbol.to_ascii_lowercase()
            ))
        })
        .cloned()
        .collect())
}

async fn load_volume_bars(
    pool: &PgPool,
    symbols: &[LiveSymbol],
    start_ts: i64,
    end_ts: i64,
) -> Result<HashMap<String, Vec<VolumeBar>>> {
    let mut all = HashMap::with_capacity(symbols.len());
    for symbol in symbols {
        let table = quoted_4h_candle_table(&symbol.symbol)?;
        let query = format!(
            "SELECT ts, c::double precision * vol_ccy::double precision AS quote_volume \
               FROM {table} \
              WHERE ts >= $1 AND ts < $2 AND confirm = '1' \
              ORDER BY ts"
        );
        let rows = sqlx::query(&query)
            .bind(start_ts)
            .bind(end_ts)
            .fetch_all(pool)
            .await
            .with_context(|| format!("load causal quote-volume bars for {}", symbol.symbol))?;
        let mut bars = Vec::with_capacity(rows.len());
        for row in rows {
            let quote_volume = row.try_get::<f64, _>("quote_volume")?;
            if quote_volume.is_finite() && quote_volume >= 0.0 {
                bars.push(VolumeBar {
                    ts: row.try_get("ts")?,
                    quote_volume,
                });
            }
        }
        all.insert(symbol.symbol.clone(), bars);
    }
    Ok(all)
}

fn trade_month_starts(trades: &[CandidateTrade]) -> Result<Vec<i64>> {
    let observed = trades
        .iter()
        .map(|trade| month_start_ms(trade.open_ts))
        .collect::<Result<BTreeSet<_>>>()
        .context("derive observed trade months")?;
    let Some(first) = observed.first().copied() else {
        return Ok(Vec::new());
    };
    let last = observed
        .last()
        .copied()
        .context("missing last trade month")?;
    let mut months = Vec::new();
    let mut cursor = first;
    while cursor <= last {
        months.push(cursor);
        cursor = next_month_start_ms(cursor)?;
    }
    Ok(months)
}

fn month_start_ms(ts: i64) -> Result<i64> {
    let datetime = Utc
        .timestamp_millis_opt(ts)
        .single()
        .context("trade timestamp is out of range")?;
    Utc.with_ymd_and_hms(datetime.year(), datetime.month(), 1, 0, 0, 0)
        .single()
        .map(|value| value.timestamp_millis())
        .context("trade month is out of range")
}

fn next_month_start_ms(ts: i64) -> Result<i64> {
    let datetime = Utc
        .timestamp_millis_opt(ts)
        .single()
        .context("universe month is out of range")?;
    let (year, month) = if datetime.month() == 12 {
        (datetime.year() + 1, 1)
    } else {
        (datetime.year(), datetime.month() + 1)
    };
    Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .map(|value| value.timestamp_millis())
        .context("next universe month is out of range")
}

fn build_monthly_memberships(
    month_starts: &[i64],
    symbols: &[LiveSymbol],
    bars: &HashMap<String, Vec<VolumeBar>>,
    target_size: usize,
) -> (HashMap<i64, HashSet<String>>, Vec<MonthlyUniverseReport>) {
    let mut memberships = HashMap::new();
    let mut reports = Vec::with_capacity(month_starts.len());
    let mut prior = None::<HashSet<String>>;
    for month_start in month_starts {
        let eligible = symbols
            .iter()
            .filter(|symbol| symbol.listed_at_ms <= *month_start - MIN_LISTING_DAYS * DAY_MS)
            .collect::<Vec<_>>();
        let mut ranked = eligible
            .iter()
            .filter_map(|symbol| {
                median_daily_quote_volume(
                    bars.get(&symbol.symbol).map(Vec::as_slice).unwrap_or(&[]),
                    *month_start,
                )
                .map(|median| RankedVolume {
                    symbol: symbol.symbol.clone(),
                    median_daily_quote_volume: median,
                })
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .median_daily_quote_volume
                .partial_cmp(&left.median_daily_quote_volume)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.symbol.cmp(&right.symbol))
        });
        let complete_count = ranked.len();
        ranked.truncate(target_size);
        let selected = ranked
            .iter()
            .map(|item| item.symbol.clone())
            .collect::<HashSet<_>>();
        let additions = prior.as_ref().map(|previous| {
            selected
                .iter()
                .filter(|symbol| !previous.contains(*symbol))
                .count()
        });
        reports.push(MonthlyUniverseReport {
            month: format_month(*month_start),
            eligible_symbols: eligible.len(),
            symbols_with_complete_volume_window: complete_count,
            selected_symbols: selected.len(),
            additions_from_prior_month: additions,
            cutoff_median_daily_quote_volume: ranked
                .last()
                .map(|item| item.median_daily_quote_volume),
        });
        prior = Some(selected.clone());
        memberships.insert(*month_start, selected);
    }
    (memberships, reports)
}

fn median_daily_quote_volume(bars: &[VolumeBar], month_start: i64) -> Option<f64> {
    let start = month_start - LOOKBACK_DAYS * DAY_MS;
    let mut daily = BTreeMap::<i64, (usize, f64)>::new();
    for bar in bars
        .iter()
        .filter(|bar| bar.ts >= start && bar.ts < month_start)
    {
        if bar.ts.rem_euclid(FOUR_HOURS_MS) != 0 {
            return None;
        }
        let day = bar.ts - bar.ts.rem_euclid(DAY_MS);
        let value = daily.entry(day).or_default();
        value.0 += 1;
        value.1 += bar.quote_volume;
    }
    if daily.len() != usize::try_from(LOOKBACK_DAYS).ok()?
        || daily.values().any(|(count, _)| *count != 6)
    {
        return None;
    }
    let mut values = daily
        .into_values()
        .map(|(_, volume)| volume)
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let midpoint = values.len() / 2;
    Some((values[midpoint - 1] + values[midpoint]) / 2.0)
}

fn format_month(ts: i64) -> String {
    Utc.timestamp_millis_opt(ts)
        .single()
        .map(|value| value.format("%Y-%m").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_bars(month_start: i64, daily_volume: f64) -> Vec<VolumeBar> {
        (0..LOOKBACK_DAYS * 6)
            .map(|index| VolumeBar {
                ts: month_start - LOOKBACK_DAYS * DAY_MS + index * FOUR_HOURS_MS,
                quote_volume: daily_volume / 6.0,
            })
            .collect()
    }

    #[test]
    fn median_volume_requires_all_thirty_complete_days() {
        let month = month_start_ms(1_735_689_600_000).unwrap();
        let mut bars = complete_bars(month, 600.0);
        assert_eq!(median_daily_quote_volume(&bars, month), Some(600.0));
        bars.pop();
        assert_eq!(median_daily_quote_volume(&bars, month), None);
    }

    #[test]
    fn monthly_top_set_uses_only_causally_old_enough_symbols() {
        let month = month_start_ms(1_735_689_600_000).unwrap();
        let old = LiveSymbol {
            symbol: "OLD-USDT-SWAP".to_string(),
            listed_at_ms: month - 200 * DAY_MS,
        };
        let young = LiveSymbol {
            symbol: "YOUNG-USDT-SWAP".to_string(),
            listed_at_ms: month - 149 * DAY_MS,
        };
        let bars = HashMap::from([
            (old.symbol.clone(), complete_bars(month, 100.0)),
            (young.symbol.clone(), complete_bars(month, 1_000.0)),
        ]);
        let (memberships, reports) =
            build_monthly_memberships(&[month], &[old.clone(), young], &bars, 100);
        assert_eq!(reports[0].eligible_symbols, 1);
        assert!(memberships[&month].contains(&old.symbol));
    }

    #[test]
    fn live_symbol_query_skips_delisted_and_non_crypto_contracts() {
        assert!(LOAD_LIVE_SYMBOLS_SQL.contains("lower(status) IN ('trading', 'live')"));
        assert!(LOAD_LIVE_SYMBOLS_SQL.contains("raw_payload->>'instCategory' = '1'"));
    }
}
