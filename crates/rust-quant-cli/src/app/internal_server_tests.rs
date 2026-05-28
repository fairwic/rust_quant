use super::{
    compute_rank_change_pct, finalize_market_rank_rows, kline_sync_request_from_body,
    market_rank_events_query_from_path, market_rank_sort_can_use_recent_query,
    market_rank_sort_requires_legacy_volume_before_limit, recent_market_rank_events_sql,
    MarketRankEventItem,
};
use chrono::{TimeZone, Utc};

fn rank_event_row(
    id: i64,
    symbol: &str,
    old_rank: i32,
    new_rank: i32,
    delta_rank: i32,
    volume_24h_quote: f64,
    volume_24h_change_pct: Option<f64>,
    detected_second: u32,
) -> MarketRankEventItem {
    MarketRankEventItem {
        id,
        exchange: "okx".to_string(),
        symbol: symbol.to_string(),
        event_type: "rank_velocity".to_string(),
        timeframe: Some("4小时".to_string()),
        old_rank: Some(old_rank),
        new_rank: Some(new_rank),
        delta_rank: Some(delta_rank),
        rank_change_pct: Some((delta_rank.abs() as f64 / old_rank as f64) * 100.0),
        volume_24h_quote: Some(volume_24h_quote),
        previous_volume_24h_quote: None,
        volume_24h_change_pct,
        volume_15m_quote: None,
        volume_15m_change_pct: None,
        current_price: Some(2200.0),
        previous_price: Some(2000.0),
        price_change_pct: Some(10.0),
        price_direction: "up".to_string(),
        price_change_24h_pct: None,
        detected_at: Utc
            .with_ymd_and_hms(2026, 5, 27, 10, 0, detected_second)
            .unwrap(),
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}

fn rank_event_row_with_type(
    id: i64,
    symbol: &str,
    event_type: &str,
    old_rank: Option<i32>,
    new_rank: Option<i32>,
    delta_rank: Option<i32>,
    detected_second: u32,
) -> MarketRankEventItem {
    let mut row = rank_event_row(
        id,
        symbol,
        old_rank.unwrap_or(100),
        new_rank.unwrap_or(100),
        delta_rank.unwrap_or(0),
        1_000_000.0,
        None,
        detected_second,
    );
    row.event_type = event_type.to_string();
    row.old_rank = old_rank;
    row.new_rank = new_rank;
    row.delta_rank = delta_rank;
    row.rank_change_pct = compute_rank_change_pct(row.old_rank, row.delta_rank);
    row
}

#[test]
fn market_rank_events_query_defaults_exchange_and_caps_limit() {
    let query = market_rank_events_query_from_path(
        "/internal/market-rank-events?symbol=btc-usdt-swap&sort=volume15m&limit=999",
    )
    .expect("market rank event query should parse");

    assert_eq!(query.exchange, "okx");
    assert_eq!(query.symbol.as_deref(), Some("BTC-USDT-SWAP"));
    assert_eq!(query.event_type, None);
    assert_eq!(query.timeframe, None);
    assert_eq!(query.sort.as_deref(), Some("volume_15m"));
    assert_eq!(query.limit, 200);
}

#[test]
fn market_rank_events_query_rejects_unknown_event_type() {
    let error =
        market_rank_events_query_from_path("/internal/market-rank-events?eventType=unknown")
            .expect_err("unknown rank event type should be rejected");

    assert_eq!(error, "unsupported eventType: unknown");
}

#[test]
fn market_rank_rows_only_need_prelimit_legacy_volume_for_volume_15m_sort() {
    assert!(market_rank_sort_requires_legacy_volume_before_limit(Some(
        "volume_15m"
    )));
    assert!(!market_rank_sort_requires_legacy_volume_before_limit(None));
    assert!(!market_rank_sort_requires_legacy_volume_before_limit(Some(
        "detected_at"
    )));
    assert!(!market_rank_sort_requires_legacy_volume_before_limit(Some(
        "delta_rank"
    )));
    assert!(!market_rank_sort_requires_legacy_volume_before_limit(Some(
        "volume_24h"
    )));
}

#[test]
fn market_rank_recent_query_is_used_for_sorts_that_do_not_need_volume_context() {
    assert!(market_rank_sort_can_use_recent_query(None));
    assert!(market_rank_sort_can_use_recent_query(Some("detected_at")));
    assert!(market_rank_sort_can_use_recent_query(Some("delta_rank")));
    assert!(!market_rank_sort_can_use_recent_query(Some("volume_24h")));
    assert!(!market_rank_sort_can_use_recent_query(Some("volume_15m")));
}

#[test]
fn market_rank_recent_query_filters_top50_before_symbol_dedup() {
    let sql = recent_market_rank_events_sql(Some("delta_rank"));
    let top50_filter = sql
        .find("AND (new_rank <= 50 OR old_rank <= 50)")
        .expect("recent market rank SQL should filter Top50 boundary rows");
    let symbol_dedup = sql
        .find("ORDER BY UPPER(symbol)")
        .expect("recent market rank SQL should dedupe by latest symbol rows");

    assert!(
        top50_filter < symbol_dedup,
        "Top50 filter must run before DISTINCT ON symbol picks the latest row"
    );
    assert!(
        !sql.contains("FROM latest\n        WHERE new_rank <= 50 OR old_rank <= 50"),
        "outer Top50 filter drops symbols whose latest non-Top50 event is newer"
    );
}

#[test]
fn kline_sync_request_normalizes_symbol_and_timeframe() {
    let request = kline_sync_request_from_body(
        br#"{"exchange":"OKX","symbol":"eth-usdt-swap","timeframe":"15m","limit":9999}"#,
    )
    .expect("kline sync request should parse");

    assert_eq!(request.exchange, "okx");
    assert_eq!(request.symbol, "ETH-USDT-SWAP");
    assert_eq!(request.timeframe, "15M");
    assert_eq!(request.limit, 2000);
}

#[test]
fn market_rank_rows_are_deduped_by_symbol_before_sorting() {
    let rows = vec![
        rank_event_row(
            1,
            "JELLYJELLY-USDT-SWAP",
            176,
            45,
            131,
            4_700_000.0,
            Some(12.0),
            1,
        ),
        rank_event_row(
            2,
            "JELLYJELLY-USDT-SWAP",
            176,
            45,
            131,
            4_710_000.0,
            Some(12.3),
            2,
        ),
        rank_event_row(3, "ONT-USDT-SWAP", 168, 49, 119, 4_100_000.0, Some(20.0), 3),
    ];

    let rows = finalize_market_rank_rows(rows, Some("delta_rank"), 10);

    let symbols: Vec<&str> = rows.iter().map(|item| item.symbol.as_str()).collect();
    assert_eq!(symbols, vec!["JELLYJELLY-USDT-SWAP", "ONT-USDT-SWAP"]);
    assert_eq!(rows[0].id, 2);
}

#[test]
fn market_rank_rows_sort_rank_movement_by_relative_change() {
    let rows = vec![
        rank_event_row(
            1,
            "LARGE-RANK-USDT-SWAP",
            176,
            49,
            127,
            4_700_000.0,
            Some(12.0),
            1,
        ),
        rank_event_row(
            2,
            "LOW-RANK-USDT-SWAP",
            21,
            1,
            20,
            116_000_000.0,
            Some(1.0),
            2,
        ),
    ];

    let rows = finalize_market_rank_rows(rows, Some("delta_rank"), 10);

    assert_eq!(rows[0].symbol, "LOW-RANK-USDT-SWAP");
    assert!(rows[0].rank_change_pct.unwrap() > rows[1].rank_change_pct.unwrap());
}

#[test]
fn market_rank_rows_default_to_rank_movement_sort() {
    let rows = vec![
        rank_event_row(1, "RECENT-USDT-SWAP", 80, 42, 38, 1_000_000.0, None, 2),
        rank_event_row(2, "FAST-USDT-SWAP", 20, 10, 10, 1_000_000.0, None, 1),
    ];

    let rows = finalize_market_rank_rows(rows, None, 10);

    assert_eq!(rows[0].symbol, "FAST-USDT-SWAP");
    assert!(rows[0].rank_change_pct.unwrap() > rows[1].rank_change_pct.unwrap());
}

#[test]
fn market_rank_rows_keep_only_top50_entries_and_exits() {
    let rows = vec![
        rank_event_row_with_type(
            1,
            "ENTERED-USDT-SWAP",
            "rank_velocity",
            Some(80),
            Some(44),
            Some(36),
            1,
        ),
        rank_event_row_with_type(
            2,
            "OUTSIDE-USDT-SWAP",
            "rank_velocity",
            Some(120),
            Some(90),
            Some(30),
            2,
        ),
        rank_event_row_with_type(
            3,
            "TOP-ENTRY-USDT-SWAP",
            "top_entry",
            None,
            Some(49),
            None,
            3,
        ),
        rank_event_row_with_type(
            4,
            "TOP150-ENTRY-USDT-SWAP",
            "top_entry",
            None,
            Some(144),
            None,
            4,
        ),
        rank_event_row_with_type(
            5,
            "EXITED-USDT-SWAP",
            "top_exit",
            Some(45),
            Some(60),
            Some(-15),
            5,
        ),
    ];

    let rows = finalize_market_rank_rows(rows, Some("delta_rank"), 10);
    let symbols: Vec<&str> = rows.iter().map(|item| item.symbol.as_str()).collect();

    assert!(symbols.contains(&"ENTERED-USDT-SWAP"));
    assert!(symbols.contains(&"TOP-ENTRY-USDT-SWAP"));
    assert!(symbols.contains(&"EXITED-USDT-SWAP"));
    assert!(!symbols.contains(&"OUTSIDE-USDT-SWAP"));
    assert!(!symbols.contains(&"TOP150-ENTRY-USDT-SWAP"));
}

#[test]
fn market_rank_rows_sort_volume_by_change_rate_not_absolute_volume() {
    let rows = vec![
        rank_event_row(
            1,
            "HIGH-VOLUME-USDT-SWAP",
            21,
            18,
            3,
            116_000_000.0,
            Some(1.0),
            1,
        ),
        rank_event_row(
            2,
            "FAST-VOLUME-USDT-SWAP",
            176,
            50,
            126,
            4_000_000.0,
            Some(35.0),
            2,
        ),
    ];

    let rows = finalize_market_rank_rows(rows, Some("volume_24h"), 10);

    assert_eq!(rows[0].symbol, "FAST-VOLUME-USDT-SWAP");
    assert!(rows[0].volume_24h_quote.unwrap() < rows[1].volume_24h_quote.unwrap());
}

#[test]
fn market_rank_rows_sort_volume_by_absolute_change_rate() {
    let rows = vec![
        rank_event_row(
            1,
            "SMALL-UP-USDT-SWAP",
            21,
            18,
            3,
            116_000_000.0,
            Some(10.0),
            1,
        ),
        rank_event_row(
            2,
            "LARGE-DOWN-USDT-SWAP",
            176,
            50,
            126,
            4_000_000.0,
            Some(-60.0),
            2,
        ),
    ];

    let rows = finalize_market_rank_rows(rows, Some("volume_24h"), 10);

    assert_eq!(rows[0].symbol, "LARGE-DOWN-USDT-SWAP");
}
