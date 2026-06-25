use super::{
    account_snapshot_sync_credential_ref, backtest_detail_list_query_from_path,
    backtest_log_list_query_from_path, compute_rank_change_pct,
    core_backtest_run_list_query_from_path, exchange_account_snapshot_sync_request_from_body,
    finalize_market_rank_rows, handle_exchange_account_snapshot_sync_body,
    handle_market_velocity_paper_strategy_preset_manifest_path, kline_sync_request_from_body,
    market_rank_events_query_from_path, market_rank_sort_can_use_recent_query,
    market_rank_sort_requires_legacy_volume_before_limit, recent_market_rank_events_sql,
    strategy_config_list_query_from_path, strategy_config_risk_config_update_value,
    strategy_config_upsert_request_from_body, BacktestLogListQuery, MarketRankEventItem,
};
use chrono::{TimeZone, Utc};
use serde_json::json;

#[test]
fn strategy_catalog_exposes_market_velocity_as_standard_core_strategy() {
    let items = super::standard_strategy_catalog_items();
    let market_velocity = items
        .iter()
        .find(|item| item.strategy_key == "market_velocity")
        .expect("market_velocity strategy catalog item");

    assert_eq!(market_velocity.product_slug, "market-velocity-radar");
    assert_eq!(market_velocity.display_name, "市场动能雷达");
    assert!(market_velocity.supported_symbols.contains(&"ALL"));
    assert!(market_velocity.timeframes.contains(&"15m"));
}
#[test]
fn strategy_catalog_exposes_display_defaults_for_admin_product_form() {
    let items = super::standard_strategy_catalog_items();
    let market_velocity = items
        .iter()
        .find(|item| item.strategy_key == "market_velocity")
        .expect("market_velocity strategy catalog item");

    assert_eq!(market_velocity.risk_level, "中高");
    assert!(market_velocity.description.contains("全市场"));
    assert!(market_velocity.detail.contains("成交额跃迁"));
    assert_eq!(
        market_velocity.cover_image,
        "/strategy-covers/strategy-quant-core.svg"
    );
    assert_eq!(market_velocity.display_total_return_pct, Some(118.40));
    assert_eq!(market_velocity.display_sharpe_ratio, Some(2.18));
    assert_eq!(market_velocity.display_trade_count, Some(204));
    assert_eq!(market_velocity.display_max_drawdown_pct, Some(20.30));
}
#[tokio::test]
async fn market_velocity_preset_manifest_endpoint_returns_canonical_manifest_hash() {
    let response = handle_market_velocity_paper_strategy_preset_manifest_path(
        "/api/internal/market-velocity/paper-strategy-preset-manifest?preset=research_momentum_0375sl_27r_reclaim13_22_v1",
    )
    .await;

    assert_eq!(response.status_code, 200);
    assert_eq!(response.body["productSlug"], "market-velocity-radar");
    assert_eq!(response.body["symbol"], "ALL");
    assert_eq!(response.body["channel"], "production_default");
    assert_eq!(response.body["strategyKey"], "market_velocity");
    assert_eq!(
        response.body["manifestJson"]["preset"],
        "research_momentum_0375sl_27r_reclaim13_22_v1"
    );
    assert!(response.body["canonicalJson"]
        .as_str()
        .unwrap()
        .contains("\"manifest_schema_version\":1"));
    assert!(response.body["manifestHash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
}
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
        technical_timeframe: None,
        technical_period: None,
        technical_close_price: None,
        technical_ma_value: None,
        technical_ema_value: None,
        technical_ma_distance_pct: None,
        technical_ema_distance_pct: None,
        technical_ma_state: None,
        technical_ema_state: None,
        technical_candle_count: None,
        technical_snapshot_at: None,
        technical_snapshot_status: None,
        technical_context: None,
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
    assert_eq!(query.lookback_minutes, 120);
}
#[test]
fn exchange_account_snapshot_sync_request_normalizes_symbols_and_defaults() {
    let body = json!({
        "buyerEmail": " Trader@Example.COM ",
        "exchange": "okx",
        "credentialId": 8801,
        "combos": [
            {"comboId": 42, "symbol": " btc-usdt-swap "}
        ],
        "triggerSource": "api credential checked"
    })
    .to_string();
    let request =
        exchange_account_snapshot_sync_request_from_body(body.as_bytes()).expect("sync request");
    assert_eq!(request.buyer_email, "Trader@Example.COM");
    assert_eq!(request.exchange.as_str(), "okx");
    assert_eq!(request.combos.len(), 1);
    assert_eq!(request.combos[0].combo_id, 42);
    assert_eq!(request.combos[0].symbol, "BTC-USDT-SWAP");
    assert_eq!(request.credential_id, 8801);
    assert!(!request.account_wide);
    assert!(request.include_fills);
    assert!(!request.report_reconciliation);
    assert_eq!(request.trigger_source, "api credential checked");
}
#[test]
fn exchange_account_snapshot_sync_request_accepts_account_wide_without_combos() {
    let body = json!({
        "buyerEmail": "buyer@example.com",
        "exchange": "okx",
        "credentialId": 8801,
        "accountWide": true,
        "triggerSource": "asset overview refresh"
    })
    .to_string();
    let request = exchange_account_snapshot_sync_request_from_body(body.as_bytes())
        .expect("account-wide sync request");
    assert!(request.account_wide);
    assert!(request.combos.is_empty());
    assert_eq!(request.credential_id, 8801);
    assert_eq!(request.trigger_source, "asset overview refresh");
}
#[test]
fn exchange_account_snapshot_sync_request_requires_exact_credential_id() {
    let body = json!({
        "buyerEmail": "buyer@example.com",
        "exchange": "okx",
        "combos": [
            {"comboId": 42, "symbol": "BTC-USDT-SWAP"}
        ]
    })
    .to_string();
    let error = exchange_account_snapshot_sync_request_from_body(body.as_bytes())
        .expect_err("missing exact credential id must be rejected");
    assert_eq!(error, "credential_id is required");
}
#[tokio::test]
async fn exchange_account_snapshot_sync_handler_rejects_missing_exact_credential_id() {
    let body = json!({
        "buyerEmail": "buyer@example.com",
        "exchange": "okx",
        "combos": [
            {"comboId": 42, "symbol": "BTC-USDT-SWAP"}
        ]
    })
    .to_string();
    let response = handle_exchange_account_snapshot_sync_body(body.as_bytes()).await;
    assert_eq!(response.status_code, 400);
    assert_eq!(response.body["error"], "credential_id is required");
}
#[test]
fn account_snapshot_sync_credential_ref_prefers_exact_credential_id() {
    assert_eq!(
        account_snapshot_sync_credential_ref(8801, "asset overview refresh", "account-wide"),
        "web_api_credential_id_8801"
    );
}
#[test]
fn exchange_account_snapshot_sync_request_rejects_invalid_combos() {
    let body = json!({
        "buyerEmail": "buyer@example.com",
        "exchange": "okx",
        "credentialId": 8801,
        "combos": [
            {"comboId": 0, "symbol": "BTC-USDT-SWAP"}
        ]
    })
    .to_string();
    let error = exchange_account_snapshot_sync_request_from_body(body.as_bytes())
        .expect_err("invalid combo id");
    assert_eq!(error, "combo_id must be a positive integer");
}
#[test]
fn market_rank_events_query_caps_recent_lookback_window() {
    let query = market_rank_events_query_from_path(
        "/internal/market-rank-events?lookbackMinutes=99999&limit=20",
    )
    .expect("market rank event query should parse");
    assert_eq!(query.lookback_minutes, 1440);
}
#[test]
fn market_rank_events_query_rejects_unknown_event_type() {
    let error =
        market_rank_events_query_from_path("/internal/market-rank-events?eventType=unknown")
            .expect_err("unknown rank event type should be rejected");
    assert_eq!(error, "unsupported eventType: unknown");
}
#[test]
fn strategy_config_list_query_accepts_api_internal_prefix_and_caps_page_size() {
    let query = strategy_config_list_query_from_path(
        "/api/internal/strategy-configs?page=2&pageSize=999&keyword=vegas&exchange=binance&symbol=eth-usdt-swap",
    )
    .expect("strategy config query should parse");
    assert_eq!(query.page, 2);
    assert_eq!(query.page_size, 200);
    assert_eq!(query.keyword.as_deref(), Some("vegas"));
    assert_eq!(query.exchange.as_deref(), Some("binance"));
    assert_eq!(query.symbol.as_deref(), Some("ETH-USDT-SWAP"));
}
#[test]
fn strategy_config_upsert_request_accepts_admin_payload() {
    let request = strategy_config_upsert_request_from_body(
        json!({
            "legacyId": 42,
            "strategyKey": "vegas",
            "strategyName": "Vegas 4H",
            "version": "admin-upsert",
            "exchange": "okx",
            "symbol": "btc-usdt-swap",
            "timeframe": "4H",
            "enabled": false,
            "config": {"ema": 144},
            "riskConfig": {"maxLossPercent": 0.02},
            "riskLevel": "高",
            "description": "运营简介",
            "detail": "运营详情",
            "coverImage": "/strategy-covers/custom.svg",
            "displayTotalReturnPct": 120.5,
            "displaySharpeRatio": 2.34,
            "displayTradeCount": 321,
            "displayMaxDrawdownPct": 12.8,
            "updatedBy": "strategy-auditor"
        })
        .to_string()
        .as_bytes(),
    )
    .expect("strategy config upsert payload should parse");
    assert_eq!(request.legacy_id, Some(42));
    assert_eq!(request.strategy_key, "vegas");
    assert_eq!(request.strategy_name.as_deref(), Some("Vegas 4H"));
    assert_eq!(request.version.as_deref(), Some("admin-upsert"));
    assert_eq!(request.exchange.as_deref(), Some("okx"));
    assert_eq!(request.symbol, "BTC-USDT-SWAP");
    assert_eq!(request.timeframe, "4H");
    assert!(!request.enabled);
    assert_eq!(request.config["ema"], 144);
    assert_eq!(request.risk_config["maxLossPercent"], 0.02);
    assert_eq!(request.risk_level.as_deref(), Some("高"));
    assert_eq!(request.description.as_deref(), Some("运营简介"));
    assert_eq!(request.detail.as_deref(), Some("运营详情"));
    assert_eq!(
        request.cover_image.as_deref(),
        Some("/strategy-covers/custom.svg")
    );
    assert_eq!(request.display_total_return_pct, Some(120.5));
    assert_eq!(request.display_sharpe_ratio, Some(2.34));
    assert_eq!(request.display_trade_count, Some(321));
    assert_eq!(request.display_max_drawdown_pct, Some(12.8));
    assert_eq!(request.updated_by.as_deref(), Some("strategy-auditor"));
}
#[test]
fn strategy_config_upsert_omits_absent_risk_config_from_updates() {
    let request = strategy_config_upsert_request_from_body(
        json!({
            "strategyKey": "vegas",
            "strategyName": "Vegas 4H",
            "version": "admin-upsert",
            "exchange": "okx",
            "symbol": "btc-usdt-swap",
            "timeframe": "4H",
            "enabled": false,
            "config": {"ema": 144},
            "updatedBy": "strategy-auditor"
        })
        .to_string()
        .as_bytes(),
    )
    .expect("strategy config upsert payload should parse without riskConfig");

    assert!(strategy_config_risk_config_update_value(&request).is_none());
}
#[test]
fn backtest_log_list_query_accepts_api_internal_prefix_and_filters() {
    let query = backtest_log_list_query_from_path(
        "/api/internal/backtests/logs?page=2&pageSize=999&keyword=vegas&status=success&exchange=okx&symbol=eth-usdt-swap",
    )
    .expect("backtest log query should parse");
    assert_eq!(query.page, 2);
    assert_eq!(query.page_size, 200);
    assert_eq!(query.keyword.as_deref(), Some("vegas"));
    assert_eq!(query.status.as_deref(), Some("success"));
    assert_eq!(query.exchange.as_deref(), Some("okx"));
    assert_eq!(query.symbol.as_deref(), Some("ETH-USDT-SWAP"));
}
#[test]
fn backtest_log_list_query_defaults_page_fields() {
    let query = backtest_log_list_query_from_path("/api/internal/backtests/logs")
        .expect("default backtest log query should parse");
    assert_eq!(
        query,
        BacktestLogListQuery {
            page: 1,
            page_size: 20,
            keyword: None,
            status: None,
            exchange: None,
            symbol: None,
            start_time: None,
            end_time: None,
        }
    );
}
#[test]
fn backtest_detail_list_query_accepts_api_internal_prefix_and_filters() {
    let query = backtest_detail_list_query_from_path(
        "/api/internal/backtests/details?page=2&pageSize=999&keyword=vegas&status=closed&backTestId=42&symbol=eth-usdt-swap&side=long",
    )
    .expect("backtest detail query should parse");
    assert_eq!(query.page, 2);
    assert_eq!(query.page_size, 200);
    assert_eq!(query.keyword.as_deref(), Some("vegas"));
    assert_eq!(query.status.as_deref(), Some("closed"));
    assert_eq!(query.back_test_id.as_deref(), Some("42"));
    assert_eq!(query.symbol.as_deref(), Some("ETH-USDT-SWAP"));
    assert_eq!(query.side.as_deref(), Some("long"));
}
#[test]
fn core_backtest_run_list_query_accepts_api_internal_prefix_and_filters() {
    let query = core_backtest_run_list_query_from_path(
        "/api/internal/core/backtest-runs?page=2&pageSize=999&keyword=vegas&status=success&exchange=okx&symbol=eth-usdt-swap",
    )
    .expect("core backtest run query should parse");
    assert_eq!(query.page, 2);
    assert_eq!(query.page_size, 200);
    assert_eq!(query.keyword.as_deref(), Some("vegas"));
    assert_eq!(query.status.as_deref(), Some("success"));
    assert_eq!(query.exchange.as_deref(), Some("okx"));
    assert_eq!(query.symbol.as_deref(), Some("ETH-USDT-SWAP"));
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
    let recency_filter = sql
        .find("detected_at >= NOW() - ($5::INTEGER * INTERVAL '1 minute')")
        .expect("recent market rank SQL should keep only a fresh time window");
    let top50_filter = sql
        .find("AND (new_rank <= 50 OR old_rank <= 50)")
        .expect("recent market rank SQL should filter Top50 boundary rows");
    let symbol_dedup = sql
        .find("ORDER BY UPPER(symbol)")
        .expect("recent market rank SQL should dedupe by latest symbol rows");
    assert!(
        recency_filter < symbol_dedup,
        "freshness filter must run before DISTINCT ON symbol picks rows"
    );
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
fn market_rank_recent_query_exposes_persisted_technical_snapshot_columns() {
    let sql = recent_market_rank_events_sql(Some("delta_rank"));
    for fragment in [
        "technical_timeframe",
        "technical_period",
        "technical_close_price::FLOAT8 AS technical_close_price",
        "technical_ma_value::FLOAT8 AS technical_ma_value",
        "technical_ema_value::FLOAT8 AS technical_ema_value",
        "technical_ma_distance_pct::FLOAT8 AS technical_ma_distance_pct",
        "technical_ema_distance_pct::FLOAT8 AS technical_ema_distance_pct",
        "technical_ma_state",
        "technical_ema_state",
        "technical_candle_count",
        "technical_snapshot_at",
        "technical_snapshot_status",
    ] {
        assert!(
            sql.contains(fragment),
            "recent market rank SQL should expose persisted technical snapshot fragment: {fragment}"
        );
    }
}
#[test]
fn market_rank_recent_query_exposes_24h_volume_delta_fields() {
    let sql = recent_market_rank_events_sql(Some("delta_rank"));
    assert!(
        sql.contains("previous.previous_volume_24h_quote"),
        "recent market rank SQL should expose previous 24h volume for delta display"
    );
    assert!(
        sql.contains("AS volume_24h_change_pct"),
        "recent market rank SQL should expose computed 24h volume change percentage"
    );
    assert!(
        !sql.contains("NULL::FLOAT8 AS previous_volume_24h_quote"),
        "recent market rank SQL must not return a fixed empty previous 24h volume"
    );
    assert!(
        !sql.contains("NULL::FLOAT8 AS volume_24h_change_pct"),
        "recent market rank SQL must not return a fixed empty 24h volume change"
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
