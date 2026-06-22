use super::{
    build_computed_candles, BacktestCandle, BacktestDataSet, CandlePair,
    MarketVelocityEventBacktestArgs, MarketVelocityEventSource, RadarEvent,
};
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

pub(super) async fn load_backtest_data(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<BacktestDataSet> {
    let pairs = load_candle_pairs(pool, args).await?;
    let symbols = pairs
        .iter()
        .map(|pair| pair.symbol.clone())
        .collect::<Vec<_>>();
    let mut candles_15m = HashMap::new();
    let mut candles_1h = HashMap::new();
    let mut candles_4h = HashMap::new();
    let mut candles_15m_computed = HashMap::new();
    let mut candles_4h_computed = HashMap::new();

    for pair in &pairs {
        let raw_15m = load_candles(pool, &pair.candles_15m).await?;
        let raw_1h = match pair.candles_1h.as_deref() {
            Some(table_name) => load_candles(pool, table_name).await?,
            None => Vec::new(),
        };
        let raw_4h = load_candles(pool, &pair.candles_4h).await?;
        candles_15m_computed.insert(
            pair.symbol.clone(),
            build_computed_candles(raw_15m.clone(), args.entry_period),
        );
        candles_4h_computed.insert(
            pair.symbol.clone(),
            build_computed_candles(raw_4h.clone(), args.entry_period),
        );
        candles_15m.insert(pair.symbol.clone(), raw_15m);
        candles_1h.insert(pair.symbol.clone(), raw_1h);
        candles_4h.insert(pair.symbol.clone(), raw_4h);
    }

    let events = load_events(pool, &symbols, args).await?;
    Ok(BacktestDataSet {
        pairs,
        candles_15m,
        candles_1h,
        candles_4h,
        candles_15m_computed,
        candles_4h_computed,
        events,
    })
}

async fn load_candle_pairs(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<CandlePair>> {
    let sql = candidate_symbols_sql(args);
    let rows = sqlx::query(sql)
        .bind(args.min_delta_rank)
        .bind(args.max_delta_rank)
        .bind(args.max_new_rank)
        .bind(args.min_price_change_pct)
        .bind(args.tail_new_rank_threshold)
        .bind(args.tail_rank_min_price_change_pct)
        .bind(args.chase_top_rank)
        .bind(args.chase_price_change_pct)
        .bind(args.trade_direction.label())
        .fetch_all(pool)
        .await
        .context("load market velocity candle table pairs")?;

    Ok(rows
        .into_iter()
        .map(|row| CandlePair {
            symbol: row.get("symbol"),
            candles_15m: row.get("candles_15m"),
            candles_1h: row.try_get("candles_1h").ok(),
            candles_4h: row.get("candles_4h"),
        })
        .collect())
}

fn candidate_symbols_sql(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    match args.event_source {
        MarketVelocityEventSource::Episodes => {
            r#"
            WITH candidates AS (
              SELECT DISTINCT upper(symbol) AS symbol
              FROM market_velocity_episodes
              WHERE event_type IN ('rank_velocity', 'top_entry')
                AND status IN ('active', 'closed')
                AND COALESCE(max_delta_rank, latest_delta_rank, 0) >= $1
                AND ($2::int IS NULL OR COALESCE(max_delta_rank, latest_delta_rank, 0) <= $2)
                AND COALESCE(best_new_rank, latest_new_rank) BETWEEN 1 AND $3
                AND (
                  ($10 = 'long' AND lower(price_direction) = 'up')
                  OR ($10 = 'short' AND lower(price_direction) = 'down')
                  OR ($10 = 'both' AND lower(price_direction) IN ('up', 'down'))
                )
                AND current_price IS NOT NULL
                AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $4)
                AND (
                  $5::int IS NULL
                  OR $6::double precision IS NULL
                  OR COALESCE(best_new_rank, latest_new_rank) < $5
                  OR ABS(COALESCE(price_change_pct, 0)) >= $6
                )
                AND NOT (
                  COALESCE(best_new_rank, latest_new_rank) <= $7
                  AND ABS(COALESCE(price_change_pct, 0)) >= $8
                )
            )
            SELECT
              candidates.symbol,
              t15.table_name AS candles_15m,
              t1.table_name AS candles_1h,
              t4.table_name AS candles_4h
            FROM candidates
            JOIN information_schema.tables t15
              ON t15.table_schema = 'public'
             AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            JOIN information_schema.tables t4
              ON t4.table_schema = 'public'
             AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
            ORDER BY candidates.symbol
            "#
        }
        MarketVelocityEventSource::RawEvents | MarketVelocityEventSource::RawState => {
            r#"
            WITH candidates AS (
              SELECT DISTINCT upper(symbol) AS symbol
              FROM market_rank_events
              WHERE event_type IN ('rank_velocity', 'top_entry')
                AND delta_rank >= $1
                AND ($2::int IS NULL OR delta_rank <= $2)
                AND new_rank BETWEEN 1 AND $3
                AND (
                  ($10 = 'long' AND lower(price_direction) = 'up')
                  OR ($10 = 'short' AND lower(price_direction) = 'down')
                  OR ($10 = 'both' AND lower(price_direction) IN ('up', 'down'))
                )
                AND current_price IS NOT NULL
                AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $4)
                AND (
                  $5::int IS NULL
                  OR $6::double precision IS NULL
                  OR new_rank < $5
                  OR ABS(COALESCE(price_change_pct, 0)) >= $6
                )
                AND NOT (new_rank <= $7 AND ABS(COALESCE(price_change_pct, 0)) >= $8)
            )
            SELECT
              candidates.symbol,
              t15.table_name AS candles_15m,
              t1.table_name AS candles_1h,
              t4.table_name AS candles_4h
            FROM candidates
            JOIN information_schema.tables t15
              ON t15.table_schema = 'public'
             AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            JOIN information_schema.tables t4
              ON t4.table_schema = 'public'
             AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
            ORDER BY candidates.symbol
            "#
        }
    }
}

async fn load_candles(pool: &PgPool, table_name: &str) -> Result<Vec<BacktestCandle>> {
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM {} ORDER BY ts",
        quote_identifier(table_name)
    );
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load candles from {table_name}"))?;
    rows.into_iter()
        .map(|row| {
            Ok(BacktestCandle {
                ts: row.get::<i64, _>("ts"),
                open: parse_f64(row.get::<String, _>("o").as_str())?,
                high: parse_f64(row.get::<String, _>("h").as_str())?,
                low: parse_f64(row.get::<String, _>("l").as_str())?,
                close: parse_f64(row.get::<String, _>("c").as_str())?,
                volume: parse_f64(row.get::<String, _>("vol").as_str())?,
            })
        })
        .collect()
}

async fn load_events(
    pool: &PgPool,
    symbols: &[String],
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<RadarEvent>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let sql = event_source_sql(args);
    let rows = sqlx::query(sql)
        .bind(symbols)
        .bind(args.min_delta_rank)
        .bind(args.max_delta_rank)
        .bind(args.max_new_rank)
        .bind(args.min_price_change_pct)
        .bind(args.tail_new_rank_threshold)
        .bind(args.tail_rank_min_price_change_pct)
        .bind(args.chase_top_rank)
        .bind(args.chase_price_change_pct)
        .bind(args.trade_direction.label())
        .fetch_all(pool)
        .await
        .context("load market velocity radar events")?;

    rows.into_iter()
        .map(|row| {
            Ok(RadarEvent {
                id: row.get("id"),
                exchange: row.get("exchange"),
                symbol: row.get("symbol"),
                ts: row.get("detected_ms"),
                detected_at: row.get("detected_at"),
                new_rank: row.get("new_rank"),
                delta_rank: row.get("delta_rank"),
                current_price: parse_f64(row.get::<String, _>("current_price").as_str())?,
                price_change_pct: parse_f64(row.get::<String, _>("price_change_pct").as_str())?,
            })
        })
        .collect()
}

fn event_source_sql(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    match args.event_source {
        MarketVelocityEventSource::Episodes => {
            r#"
            SELECT
              id::bigint AS id,
              lower(exchange) AS exchange,
              upper(symbol) AS symbol,
              floor(extract(epoch from started_at) * 1000)::bigint AS detected_ms,
              started_at::text AS detected_at,
              COALESCE(best_new_rank, latest_new_rank)::int AS new_rank,
              COALESCE(max_delta_rank, latest_delta_rank)::int AS delta_rank,
              current_price::text AS current_price,
              COALESCE(price_change_pct, 0)::text AS price_change_pct
            FROM market_velocity_episodes
            WHERE upper(symbol) = ANY($1)
              AND event_type IN ('rank_velocity', 'top_entry')
              AND status IN ('active', 'closed')
              AND COALESCE(max_delta_rank, latest_delta_rank, 0) >= $2
              AND ($3::int IS NULL OR COALESCE(max_delta_rank, latest_delta_rank, 0) <= $3)
              AND COALESCE(best_new_rank, latest_new_rank) BETWEEN 1 AND $4
              AND (
                ($10 = 'long' AND lower(price_direction) = 'up')
                OR ($10 = 'short' AND lower(price_direction) = 'down')
                OR ($10 = 'both' AND lower(price_direction) IN ('up', 'down'))
              )
              AND current_price IS NOT NULL
              AND ($5::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $5)
              AND (
                $6::int IS NULL
                OR $7::double precision IS NULL
                OR COALESCE(best_new_rank, latest_new_rank) < $6
                OR ABS(COALESCE(price_change_pct, 0)) >= $7
              )
              AND NOT (
                COALESCE(best_new_rank, latest_new_rank) <= $8
                AND ABS(COALESCE(price_change_pct, 0)) >= $9
              )
            ORDER BY started_at, id
            "#
        }
        MarketVelocityEventSource::RawEvents => {
            r#"
            SELECT
              id::bigint AS id,
              lower(exchange) AS exchange,
              upper(symbol) AS symbol,
              floor(extract(epoch from detected_at) * 1000)::bigint AS detected_ms,
              detected_at::text AS detected_at,
              new_rank::int AS new_rank,
              delta_rank::int AS delta_rank,
              current_price::text AS current_price,
              COALESCE(price_change_pct, 0)::text AS price_change_pct
            FROM market_rank_events
            WHERE upper(symbol) = ANY($1)
              AND event_type IN ('rank_velocity', 'top_entry')
              AND delta_rank >= $2
              AND ($3::int IS NULL OR delta_rank <= $3)
              AND new_rank BETWEEN 1 AND $4
              AND (
                ($10 = 'long' AND lower(price_direction) = 'up')
                OR ($10 = 'short' AND lower(price_direction) = 'down')
                OR ($10 = 'both' AND lower(price_direction) IN ('up', 'down'))
              )
              AND current_price IS NOT NULL
              AND ($5::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $5)
              AND (
                $6::int IS NULL
                OR $7::double precision IS NULL
                OR new_rank < $6
                OR ABS(COALESCE(price_change_pct, 0)) >= $7
              )
              AND NOT (new_rank <= $8 AND ABS(COALESCE(price_change_pct, 0)) >= $9)
            ORDER BY detected_at, id
            "#
        }
        MarketVelocityEventSource::RawState => {
            r#"
            WITH filtered AS (
              SELECT
                id,
                exchange,
                symbol,
                detected_at,
                new_rank,
                delta_rank,
                current_price,
                price_change_pct,
                floor(extract(epoch from detected_at) / 900) AS detected_15m_bucket
              FROM market_rank_events
              WHERE upper(symbol) = ANY($1)
                AND event_type IN ('rank_velocity', 'top_entry')
                AND delta_rank >= $2
                AND ($3::int IS NULL OR delta_rank <= $3)
                AND new_rank BETWEEN 1 AND $4
                AND (
                  ($10 = 'long' AND lower(price_direction) = 'up')
                  OR ($10 = 'short' AND lower(price_direction) = 'down')
                  OR ($10 = 'both' AND lower(price_direction) IN ('up', 'down'))
                )
                AND current_price IS NOT NULL
                AND ($5::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $5)
                AND (
                  $6::int IS NULL
                  OR $7::double precision IS NULL
                  OR new_rank < $6
                  OR ABS(COALESCE(price_change_pct, 0)) >= $7
                )
                AND NOT (new_rank <= $8 AND ABS(COALESCE(price_change_pct, 0)) >= $9)
            )
            SELECT *
            FROM (
              SELECT DISTINCT ON (upper(symbol), detected_15m_bucket)
                id::bigint AS id,
                lower(exchange) AS exchange,
                upper(symbol) AS symbol,
                floor(extract(epoch from detected_at) * 1000)::bigint AS detected_ms,
                detected_at::text AS detected_at,
                new_rank::int AS new_rank,
                delta_rank::int AS delta_rank,
                current_price::text AS current_price,
                COALESCE(price_change_pct, 0)::text AS price_change_pct
              FROM filtered
              ORDER BY upper(symbol), detected_15m_bucket, detected_at, id
            ) deduped
            ORDER BY detected_ms, id
            "#
        }
    }
}

fn parse_f64(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("parse numeric value {value}"))
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        MarketVelocityEventBacktestArgs, MarketVelocityEventSource,
    };

    #[test]
    fn episode_event_source_reads_episode_table() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Episodes,
            ..MarketVelocityEventBacktestArgs::default()
        };

        let sql = event_source_sql(&args);

        assert!(sql.contains("FROM market_velocity_episodes"));
        assert!(!sql.contains("FROM market_rank_events"));
    }

    #[test]
    fn raw_event_source_keeps_legacy_rank_event_table() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawEvents,
            ..MarketVelocityEventBacktestArgs::default()
        };

        let sql = event_source_sql(&args);

        assert!(sql.contains("FROM market_rank_events"));
    }

    #[test]
    fn raw_state_event_source_deduplicates_scanner_hits_by_15m_candle() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawState,
            ..MarketVelocityEventBacktestArgs::default()
        };

        let sql = event_source_sql(&args);

        assert!(sql.contains("FROM market_rank_events"));
        assert!(sql.contains("DISTINCT ON (upper(symbol), detected_15m_bucket)"));
        assert!(sql.contains("floor(extract(epoch from detected_at) / 900)"));
        assert!(sql.contains("ORDER BY upper(symbol), detected_15m_bucket, detected_at, id"));
    }

    #[test]
    fn episode_event_source_keeps_closed_historical_episodes_in_backtests() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Episodes,
            ..MarketVelocityEventBacktestArgs::default()
        };

        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);

        assert!(candidate_sql.contains("status IN ('active', 'closed')"));
        assert!(event_sql.contains("status IN ('active', 'closed')"));
        assert!(!candidate_sql.contains("status = 'active'"));
        assert!(!event_sql.contains("status = 'active'"));
    }
}
