use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{
    MarketRankEvent, MarketRankEventType, MarketRankTechnicalSnapshot,
};
use rust_quant_services::market::MarketVelocityStrategySignalConfig;
use sqlx::{PgPool, Row};
pub(super) fn normalize_candidate_limit(limit: i64) -> u32 {
    limit.clamp(1, 100) as u32
}

pub(super) async fn load_market_velocity_live_candidate_events(
    pool: &PgPool,
    event_id: Option<i64>,
    lookback_hours: i64,
    limit: u32,
    config: &MarketVelocityStrategySignalConfig,
) -> Result<Vec<MarketRankEvent>> {
    let rows = sqlx::query(market_velocity_live_candidate_events_sql())
        .bind(config.min_delta_rank)
        .bind(config.max_new_rank)
        .bind(event_id)
        .bind(lookback_hours.to_string())
        .bind(i64::from(normalize_candidate_limit(i64::from(limit))))
        .fetch_all(pool)
        .await
        .context("load recent market velocity live candidate events")?;
    rows.into_iter().map(market_rank_event_from_row).collect()
}

fn market_velocity_live_candidate_events_sql() -> &'static str {
    r#"
        WITH eligible_events AS (
          SELECT
            id,
            lower(exchange) AS exchange,
            upper(symbol) AS symbol,
            event_type,
            timeframe,
            old_rank,
            new_rank,
            delta_rank,
            volume_24h_quote,
            current_price,
            previous_price,
            price_change_pct,
            price_direction,
            technical_timeframe,
            technical_period,
            technical_close_price,
            technical_ma_value,
            technical_ema_value,
            technical_ma_distance_pct,
            technical_ema_distance_pct,
            technical_ma_state,
            technical_ema_state,
            technical_candle_count,
            technical_snapshot_at,
            technical_snapshot_status,
            detected_at,
            source,
            notification_state
          FROM market_rank_events
          WHERE event_type IN ('rank_velocity', 'top_entry')
            AND delta_rank >= $1
            AND new_rank > 0
            AND new_rank <= $2
            AND lower(price_direction) = 'up'
            AND current_price IS NOT NULL
            AND lower(exchange) = 'okx'
            AND upper(replace(symbol, '-', '')) NOT LIKE 'LINKUSDT%'
            AND ($3::bigint IS NULL OR id = $3)
            AND detected_at >= NOW() - ($4::text || ' hours')::interval
        ),
        latest_per_symbol AS (
          SELECT DISTINCT ON (symbol) *
          FROM eligible_events
          ORDER BY symbol, detected_at DESC, id DESC
        )
        SELECT
          id,
          exchange,
          symbol,
          event_type,
          timeframe,
          old_rank,
          new_rank,
          delta_rank,
          volume_24h_quote,
          current_price,
          previous_price,
          price_change_pct,
          price_direction,
          technical_timeframe,
          technical_period,
          technical_close_price,
          technical_ma_value,
          technical_ema_value,
          technical_ma_distance_pct,
          technical_ema_distance_pct,
          technical_ma_state,
          technical_ema_state,
          technical_candle_count,
          technical_snapshot_at,
          technical_snapshot_status,
          detected_at,
          source,
          notification_state
        FROM latest_per_symbol
        ORDER BY detected_at DESC, id DESC
        LIMIT $5
        "#
}

fn market_rank_event_from_row(row: sqlx::postgres::PgRow) -> Result<MarketRankEvent> {
    let event_type_raw: String = row.get("event_type");
    let event_type = MarketRankEventType::try_from(event_type_raw.as_str())?;
    let technical_snapshot_status: String = row.get("technical_snapshot_status");
    let technical_snapshot = if technical_snapshot_status == "captured" {
        Some(MarketRankTechnicalSnapshot {
            timeframe: row.try_get::<String, _>("technical_timeframe")?,
            period: row.try_get::<i32, _>("technical_period")?,
            close_price: row.try_get::<Decimal, _>("technical_close_price")?,
            ma_value: row.try_get::<Decimal, _>("technical_ma_value")?,
            ema_value: row.try_get::<Decimal, _>("technical_ema_value")?,
            ma_distance_pct: row.try_get::<Decimal, _>("technical_ma_distance_pct")?,
            ema_distance_pct: row.try_get::<Decimal, _>("technical_ema_distance_pct")?,
            ma_state: row.try_get::<String, _>("technical_ma_state")?,
            ema_state: row.try_get::<String, _>("technical_ema_state")?,
            candle_count: row.try_get::<i32, _>("technical_candle_count")?,
            snapshot_at: row.try_get::<DateTime<Utc>, _>("technical_snapshot_at")?,
        })
    } else {
        None
    };

    Ok(MarketRankEvent {
        id: row.get("id"),
        exchange: row.get("exchange"),
        symbol: row.get("symbol"),
        event_type,
        timeframe: row.try_get("timeframe").ok(),
        old_rank: row.try_get("old_rank").ok(),
        new_rank: row.try_get("new_rank").ok(),
        delta_rank: row.try_get("delta_rank").ok(),
        volume_24h_quote: row.try_get("volume_24h_quote").ok(),
        current_price: row.try_get("current_price").ok(),
        previous_price: row.try_get("previous_price").ok(),
        price_change_pct: row.try_get("price_change_pct").ok(),
        price_direction: row.get("price_direction"),
        technical_snapshot_status,
        technical_snapshot,
        detected_at: row.get("detected_at"),
        source: row.get("source"),
        notification_state: row.get("notification_state"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_scan_limit_is_bounded_for_live_handoff() {
        assert_eq!(normalize_candidate_limit(-10), 1);
        assert_eq!(normalize_candidate_limit(0), 1);
        assert_eq!(normalize_candidate_limit(20), 20);
        assert_eq!(normalize_candidate_limit(500), 100);
    }

    #[test]
    fn candidate_scan_sql_uses_latest_event_per_symbol_before_limit() {
        let sql = market_velocity_live_candidate_events_sql();

        assert!(
            sql.contains("DISTINCT ON (symbol)"),
            "live candidate scan must not let repeated events from a few symbols fill the limit: {sql}"
        );
        assert!(
            sql.contains("ORDER BY symbol, detected_at DESC, id DESC"),
            "latest event per symbol must be selected before global ordering: {sql}"
        );
        assert!(
            sql.contains("FROM latest_per_symbol"),
            "global live scan should order already deduplicated symbols: {sql}"
        );
    }
}
