use super::*;
#[test]
fn parse_symbols_normalizes_and_deduplicates() {
    assert_eq!(
        parse_symbol_list(" xag-usdt-swap, BTC-USDT-SWAP, xag-USDT-swap "),
        vec!["BTC-USDT-SWAP".to_string(), "XAG-USDT-SWAP".to_string()]
    );
}
#[test]
fn cli_args_support_proxy_and_all_radar_symbols() {
    let args = parse_cli_args_from([
        "--symbols",
        "xag-usdt-swap",
        "--days",
        "30",
        "--proxy-url",
        "http://127.0.0.1:7897",
        "--all-radar-symbols",
        "--dry-run",
    ])
    .unwrap();
    assert_eq!(args.symbols, Some(vec!["XAG-USDT-SWAP".to_string()]));
    assert_eq!(args.days, Some(30));
    assert_eq!(
        args.proxy_url,
        Some(Some("http://127.0.0.1:7897".to_string()))
    );
    assert_eq!(args.require_4h, Some(false));
    assert_eq!(args.dry_run, Some(true));
}
#[test]
fn cli_args_support_fail_fast_for_bulk_backfill() {
    let args = parse_cli_args_from(["--fail-fast"]).unwrap();
    assert_eq!(args.continue_on_error, Some(false));
}
#[test]
fn cli_args_support_rust_native_scheduler_loop_interval() {
    let args = parse_cli_args_from(["--loop-interval-seconds", "300"]).unwrap();
    assert_eq!(args.loop_interval_seconds, Some(300));
    let args = parse_cli_args_from(["--loop-interval-seconds=600"]).unwrap();
    assert_eq!(args.loop_interval_seconds, Some(600));
    let error = parse_cli_args_from(["--loop-interval-seconds", "0"]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--loop-interval-seconds must be greater than 0"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn cli_args_support_multiple_scheduler_timeframes() {
    let args = parse_cli_args_from(["--timeframes", "1m, 5m,15m"]).unwrap();
    assert_eq!(
        args.timeframes,
        Some(vec!["1m".to_string(), "5m".to_string(), "15m".to_string()])
    );
}
#[test]
fn cli_args_support_enabled_strategy_symbol_source() {
    let args = parse_cli_args_from([
        "--enabled-strategy-symbols",
        "--timeframe",
        "4h",
        "--days",
        "60",
    ])
    .unwrap();
    assert_eq!(args.enabled_strategy_symbols, Some(true));
    assert_eq!(args.timeframe, Some("4h".to_string()));
    assert_eq!(args.days, Some(60));
}
#[test]
fn history_url_paginates_to_older_candles_with_after() {
    let url = build_okx_history_candles_url(
        "https://www.okx.com/",
        "XAG-USDT-SWAP",
        "15m",
        Some(1_781_500_000_000),
        100,
    )
    .unwrap();
    assert_eq!(
        url.as_str(),
        "https://www.okx.com/api/v5/market/history-candles?instId=XAG-USDT-SWAP&bar=15m&limit=100&after=1781500000000"
    );
}
#[test]
fn parse_okx_candle_row_matches_existing_dto_mapping() {
    let candle = parse_okx_candle_row(vec![
        "1781503200000".to_string(),
        "1".to_string(),
        "2".to_string(),
        "0.9".to_string(),
        "1.5".to_string(),
        "10".to_string(),
        "11".to_string(),
        "12".to_string(),
        "1".to_string(),
    ])
    .unwrap();
    assert_eq!(candle.ts, "1781503200000");
    assert_eq!(candle.v, "10");
    assert_eq!(candle.vol_ccy, "11");
    assert_eq!(candle.vol_ccy_quote, "12");
    assert_eq!(candle.confirm, "1");
}
#[test]
fn parse_okx_candle_row_rejects_short_rows() {
    let error = parse_okx_candle_row(vec!["1781503200000".to_string()]).unwrap_err();
    assert!(error.to_string().contains("expected at least 9"));
}
#[test]
fn max_history_pages_has_buffer_for_60_days_of_15m_candles() {
    let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, CANDLE_15M_MS, 100);
    assert_eq!(pages, 65);
}
#[test]
fn backfill_window_uses_full_range_when_no_local_candles_exist() {
    let window = resolve_incremental_backfill_window(
        1_000_000,
        2_000_000,
        CANDLE_15M_MS,
        CandleContinuityStatus::default(),
    );
    assert_eq!(window.fetch_start_ms, 1_000_000);
    assert_eq!(window.reason, BackfillWindowReason::EmptyOrMissingTable);
}
#[test]
fn backfill_window_repairs_earliest_detected_gap_with_overlap() {
    let window = resolve_incremental_backfill_window(
        1_000_000,
        5_000_000,
        CANDLE_15M_MS,
        CandleContinuityStatus {
            earliest_ts: Some(1_000_000),
            latest_ts: Some(4_800_000),
            actual_count: 4,
            expected_count: 5,
            earliest_gap_start_ts: Some(3_000_000),
        },
    );
    assert_eq!(window.fetch_start_ms, 3_000_000 - CANDLE_15M_MS);
    assert_eq!(window.reason, BackfillWindowReason::GapRepair);
}
#[test]
fn backfill_window_repairs_leading_gap_from_configured_start() {
    let window = resolve_incremental_backfill_window(
        1_000_000,
        5_000_000,
        CANDLE_15M_MS,
        CandleContinuityStatus {
            earliest_ts: Some(3_000_000),
            latest_ts: Some(4_800_000),
            actual_count: 3,
            expected_count: 3,
            earliest_gap_start_ts: None,
        },
    );
    assert_eq!(window.fetch_start_ms, 1_000_000);
    assert_eq!(window.reason, BackfillWindowReason::GapRepair);
}
#[test]
fn backfill_window_uses_latest_candle_overlap_when_local_series_is_continuous() {
    let window = resolve_incremental_backfill_window(
        1_000_000,
        5_000_000,
        CANDLE_15M_MS,
        CandleContinuityStatus {
            earliest_ts: Some(1_000_000),
            latest_ts: Some(4_800_000),
            actual_count: 5,
            expected_count: 5,
            earliest_gap_start_ts: None,
        },
    );
    assert_eq!(window.fetch_start_ms, 4_800_000 - CANDLE_15M_MS);
    assert_eq!(window.reason, BackfillWindowReason::IncrementalTail);
}
#[test]
fn symbol_start_aligns_configured_time_to_the_next_candle_boundary() {
    assert_eq!(
        aligned_symbol_start_ms(1_000_001, None, CANDLE_15M_MS),
        1_800_000
    );
}
#[test]
fn symbol_start_clamps_to_listing_time_before_alignment() {
    assert_eq!(
        aligned_symbol_start_ms(1_000_000, Some(2_000_001), CANDLE_15M_MS),
        2_700_000
    );
}
#[test]
fn unaligned_scheduler_window_does_not_trigger_false_gap_repair() {
    let configured_start_ms = 1_784_300_865_335;
    let aligned_start_ms = aligned_symbol_start_ms(configured_start_ms, None, CANDLE_1M_MS);
    let latest_ts = aligned_start_ms + CANDLE_1M_MS * 2_879;
    let window = resolve_incremental_backfill_window(
        aligned_start_ms,
        latest_ts + CANDLE_1M_MS,
        CANDLE_1M_MS,
        CandleContinuityStatus {
            earliest_ts: Some(aligned_start_ms),
            latest_ts: Some(latest_ts),
            actual_count: 2_880,
            expected_count: 2_880,
            earliest_gap_start_ts: None,
        },
    );
    assert_eq!(window.reason, BackfillWindowReason::IncrementalTail);
    assert_eq!(window.fetch_start_ms, latest_ts - CANDLE_1M_MS);
}
#[test]
fn candle_continuity_uses_bounds_and_count_to_detect_missing_rows() {
    let status = CandleContinuityStatus {
        earliest_ts: Some(1_000_000),
        latest_ts: Some(1_000_000 + CANDLE_15M_MS * 4),
        actual_count: 4,
        expected_count: 5,
        earliest_gap_start_ts: None,
    };
    assert!(status.has_missing_candles());
}
#[test]
fn candle_continuity_treats_matching_bounds_and_count_as_continuous() {
    let status = CandleContinuityStatus {
        earliest_ts: Some(1_000_000),
        latest_ts: Some(1_000_000 + CANDLE_15M_MS * 4),
        actual_count: 5,
        expected_count: 5,
        earliest_gap_start_ts: None,
    };
    assert!(!status.has_missing_candles());
}
#[test]
fn candle_interval_ms_supports_4h_trend_backfill() {
    assert_eq!(candle_interval_ms("4h").unwrap(), 4 * 60 * 60 * 1_000);
}
#[test]
fn candle_interval_ms_supports_1h_fvg_backfill() {
    assert_eq!(candle_interval_ms("1h").unwrap(), 60 * 60 * 1_000);
}
#[test]
fn candle_interval_ms_supports_1m_scalper_backfill() {
    assert_eq!(candle_interval_ms("1m").unwrap(), 60 * 1_000);
    assert_eq!(okx_bar_for_timeframe("1m").unwrap(), "1m");
}
#[test]
fn okx_bar_for_timeframe_uses_okx_hour_case() {
    assert_eq!(okx_bar_for_timeframe("5m").unwrap(), "5m");
    assert_eq!(okx_bar_for_timeframe("15m").unwrap(), "15m");
    assert_eq!(okx_bar_for_timeframe("1h").unwrap(), "1H");
    assert_eq!(okx_bar_for_timeframe("4h").unwrap(), "4H");
}
#[test]
fn max_history_pages_has_buffer_for_60_days_of_5m_candles() {
    let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, CANDLE_5M_MS, 100);
    assert_eq!(pages, 180);
}
#[test]
fn max_history_pages_has_buffer_for_60_days_of_1h_candles() {
    let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, CANDLE_1H_MS, 100);
    assert_eq!(pages, 22);
}
#[test]
fn max_history_pages_has_buffer_for_60_days_of_4h_candles() {
    let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, 4 * 60 * 60 * 1_000, 100);
    assert_eq!(pages, 11);
}
#[test]
fn backfill_symbol_scan_uses_only_active_okx_symbols() {
    let sql = load_market_velocity_backfill_symbols_sql(false);
    assert!(
        sql.contains("exchange_symbols"),
        "backfill must consult exchange_symbols before requesting OKX candles: {sql}"
    );
    assert!(
        sql.contains("available_okx_symbols"),
        "backfill should use a dedicated available-symbol CTE before selecting candidates: {sql}"
    );
    let normalized_sql = sql.to_ascii_lowercase();
    assert!(
        normalized_sql.contains("lower(status) in ('trading', 'live')"),
        "deleted or unsupported OKX symbols must be excluded by status: {sql}"
    );
    assert!(
        sql.contains("JOIN available_okx_symbols USING (symbol)"),
        "unavailable OKX symbols must not reach history-candles requests: {sql}"
    );
}

#[test]
fn enabled_strategy_symbol_scan_is_timeframe_scoped_and_exchange_safe() {
    let sql = load_enabled_strategy_backfill_symbols_sql().to_ascii_lowercase();
    assert!(sql.contains("from strategy_configs"));
    assert!(sql.contains("config.enabled = true"));
    assert!(sql.contains("lower(config.timeframe) = lower($1)"));
    assert!(sql.contains("lower(config.exchange) = 'okx'"));
    assert!(sql.contains("exchange_symbol.market_type = 'perpetual'"));
    assert!(sql.contains("lower(exchange_symbol.status) in ('trading', 'live')"));
}

#[test]
fn okx_51001_is_missing_instrument_error() {
    let error = anyhow!(okx_history_candles_api_error(
        "51001",
        "Instrument ID doesn't exist.",
        "IP-USDT-SWAP"
    ));
    assert!(is_okx_missing_instrument_error(&error));
    let transient = anyhow!(okx_history_candles_api_error(
        "50011",
        "Rate limit reached.",
        "BTC-USDT-SWAP"
    ));
    assert!(!is_okx_missing_instrument_error(&transient));
}

#[test]
fn dry_run_does_not_mark_missing_okx_instrument_deleted() {
    let missing = anyhow!(
        "OKX history-candles returned code={} msg=instrument missing",
        OKX_MISSING_INSTRUMENT_CODE
    );

    assert!(!should_mark_okx_exchange_symbol_deleted(true, &missing));
    assert!(should_mark_okx_exchange_symbol_deleted(false, &missing));
}

#[test]
fn okx_missing_instrument_mark_sql_sets_deleted_status() {
    let sql = mark_okx_exchange_symbol_deleted_sql().to_ascii_lowercase();
    assert!(sql.contains("update exchange_symbols"));
    assert!(sql.contains("set status = 'deleted'"));
    assert!(sql.contains("exchange = 'okx'"));
    assert!(sql.contains("market_type = 'perpetual'"));
    assert!(sql.contains("upper(exchange_symbol) = upper($1)"));
    assert!(sql.contains("upper(normalized_symbol) = upper($1)"));
}
