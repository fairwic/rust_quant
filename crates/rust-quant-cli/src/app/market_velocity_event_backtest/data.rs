use super::{
    build_computed_candles, BacktestCandle, BacktestDataSet, CandlePair,
    MarketVelocityEventBacktestArgs, RadarEvent,
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
    let rows = sqlx::query(
        r#"
        WITH candidates AS (
          SELECT DISTINCT upper(symbol) AS symbol
          FROM market_rank_events
          WHERE event_type IN ('rank_velocity', 'top_entry')
            AND delta_rank >= $1
            AND new_rank BETWEEN 1 AND $2
            AND lower(price_direction) = 'up'
            AND current_price IS NOT NULL
            AND NOT (new_rank <= $3 AND COALESCE(price_change_pct, 0) >= $4)
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
        "#,
    )
    .bind(args.min_delta_rank)
    .bind(args.max_new_rank)
    .bind(args.chase_top_rank)
    .bind(args.chase_price_change_pct)
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
    let rows = sqlx::query(
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
          AND new_rank BETWEEN 1 AND $3
          AND lower(price_direction) = 'up'
          AND current_price IS NOT NULL
          AND NOT (new_rank <= $4 AND COALESCE(price_change_pct, 0) >= $5)
        ORDER BY detected_at, id
        "#,
    )
    .bind(symbols)
    .bind(args.min_delta_rank)
    .bind(args.max_new_rank)
    .bind(args.chase_top_rank)
    .bind(args.chase_price_change_pct)
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

fn parse_f64(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("parse numeric value {value}"))
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
