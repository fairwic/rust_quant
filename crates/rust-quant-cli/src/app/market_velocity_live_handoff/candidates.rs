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
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
pub(super) async fn load_market_velocity_live_candidate_events(
    pool: &PgPool,
    event_id: Option<i64>,
    lookback_hours: i64,
    limit: u32,
    config: &MarketVelocityStrategySignalConfig,
) -> Result<Vec<MarketRankEvent>> {
    let rows = sqlx::query(market_velocity_live_candidate_events_sql())
        .bind(config.min_delta_rank)
        .bind(config.max_delta_rank)
        .bind(config.min_price_change_pct)
        .bind(config.max_price_change_pct)
        .bind(event_id)
        .bind(lookback_hours.to_string())
        .bind(i64::from(normalize_candidate_limit(i64::from(limit))))
        .fetch_all(pool)
        .await
        .context("load recent market velocity live candidate events")?;
    rows.into_iter().map(market_rank_event_from_row).collect()
}
/// 提供市场动量live候选事件SQL的集中实现，避免行情数据调用方重复处理相同细节。
fn market_velocity_live_candidate_events_sql() -> &'static str {
    r#"
        WITH available_okx_symbols AS (
          SELECT DISTINCT upper(normalized_symbol) AS symbol
          FROM exchange_symbols
          WHERE exchange = 'okx'
            AND market_type = 'perpetual'
            AND lower(status) IN ('trading', 'live')
        ),
        eligible_events AS (
          SELECT
            market_rank_events.id,
            lower(market_rank_events.exchange) AS exchange,
            upper(market_rank_events.symbol) AS symbol,
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
          JOIN available_okx_symbols
            ON available_okx_symbols.symbol = upper(market_rank_events.symbol)
          WHERE event_type = 'rank_velocity'
            AND COALESCE(timeframe, '') = '15分钟'
            AND delta_rank >= $1
            AND ($2::int IS NULL OR delta_rank <= $2)
            AND ($3::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $3)
            AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $4)
            AND lower(price_direction) = 'up'
            AND current_price IS NOT NULL
            AND lower(market_rank_events.exchange) = 'okx'
            AND upper(replace(market_rank_events.symbol, '-', '')) NOT LIKE 'LINKUSDT%'
            AND COALESCE(live_handoff_state, 'pending') = 'pending'
            AND ($5::bigint IS NULL OR market_rank_events.id = $5)
            AND detected_at >= NOW() - ($6::text || ' hours')::interval
        ),
        earliest_per_symbol AS (
          SELECT DISTINCT ON (symbol) *
          FROM eligible_events
          ORDER BY symbol, detected_at ASC, id ASC
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
        FROM earliest_per_symbol
        ORDER BY detected_at ASC, id ASC
        LIMIT $7
        "#
}
/// 提供市场rankeventfrom数据行的集中实现，避免行情数据调用方重复处理相同细节。
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
    fn candidate_scan_sql_uses_earliest_event_per_symbol_before_limit() {
        let sql = market_velocity_live_candidate_events_sql();
        assert!(
            sql.contains("DISTINCT ON (symbol)"),
            "live candidate scan must not let repeated events from a few symbols fill the limit: {sql}"
        );
        assert!(
            sql.contains("ORDER BY symbol, detected_at ASC, id ASC"),
            "earliest generated event per symbol must be selected before global ordering: {sql}"
        );
        assert!(
            sql.contains("FROM earliest_per_symbol"),
            "global live scan should order already deduplicated symbols: {sql}"
        );
        assert!(
            sql.contains("ORDER BY detected_at ASC, id ASC"),
            "global handoff must evaluate selected symbols by generated time first: {sql}"
        );
    }
    #[test]
    fn candidate_scan_sql_does_not_filter_by_new_rank() {
        let sql = market_velocity_live_candidate_events_sql();
        assert!(!sql.contains("new_rank <="));
        assert!(!sql.contains("new_rank >"));
    }

    #[test]
    fn candidate_scan_sql_only_consumes_pending_live_handoff_states() {
        let sql = market_velocity_live_candidate_events_sql();
        assert!(
            sql.contains("live_handoff_state"),
            "candidate scan must read the trade handoff state separately from notification_state"
        );
        assert!(
            sql.contains("COALESCE(live_handoff_state, 'pending') = 'pending'"),
            "blocked, created, expired and failed candidates must not be reprocessed forever: {sql}"
        );
    }

    #[test]
    fn candidate_scan_sql_only_consumes_15m_rank_velocity_events() {
        let sql = market_velocity_live_candidate_events_sql();
        assert!(
            sql.contains("event_type = 'rank_velocity'"),
            "live handoff should not consume top-entry or slower rank events: {sql}"
        );
        assert!(
            sql.contains("COALESCE(timeframe, '') = '15分钟'"),
            "live handoff fast momentum candidates must be generated by the 15m rank window: {sql}"
        );
        assert!(
            !sql.contains("event_type IN ('rank_velocity', 'top_entry')"),
            "top_entry is a later Top50 boundary event and should stay out of fast momentum handoff: {sql}"
        );
    }

    #[test]
    fn candidate_scan_sql_applies_signal_range_filters_before_entry_candle_refresh() {
        let sql = market_velocity_live_candidate_events_sql();
        assert!(
            sql.contains("($2::int IS NULL OR delta_rank <= $2)"),
            "live handoff should not refresh candles for events above max_delta_rank: {sql}"
        );
        assert!(
            sql.contains(
                "($3::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $3)"
            ),
            "live handoff should not refresh candles before min price-change screening: {sql}"
        );
        assert!(
            sql.contains(
                "($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $4)"
            ),
            "live handoff should not refresh candles for events above max_price_change_pct: {sql}"
        );
    }

    #[test]
    fn candidate_scan_sql_uses_only_active_okx_symbols() {
        let sql = market_velocity_live_candidate_events_sql();
        assert!(
            sql.contains("exchange_symbols"),
            "live handoff must consult exchange_symbols before refreshing entry candles: {sql}"
        );
        assert!(
            sql.contains("available_okx_symbols"),
            "live handoff should use a dedicated available-symbol CTE before selecting candidates: {sql}"
        );
        let normalized_sql = sql.to_ascii_lowercase();
        assert!(
            normalized_sql.contains("lower(status) in ('trading', 'live')"),
            "deleted or unsupported OKX symbols must be excluded by status: {sql}"
        );
        assert!(
            sql.contains("JOIN available_okx_symbols"),
            "unavailable OKX symbols must not reach entry candle refresh or task handoff: {sql}"
        );
    }
}
