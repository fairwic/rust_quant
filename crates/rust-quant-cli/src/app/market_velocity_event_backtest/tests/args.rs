use super::super::parse_cli_args_from;
use super::super::{MarketVelocityEventSource, MarketVelocityTradeDirection};
#[test]
fn defaults_to_episode_event_source_for_clean_backtests() {
    let args = parse_cli_args_from([] as [&str; 0]).unwrap();
    assert_eq!(args.event_source, MarketVelocityEventSource::Episodes);
}
#[test]
fn defaults_to_long_trade_direction() {
    let args = parse_cli_args_from([] as [&str; 0]).unwrap();
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Long);
}
#[test]
fn parses_short_trade_direction() {
    let args = parse_cli_args_from(["--trade-direction", "short"]).unwrap();
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
}
#[test]
fn parses_both_trade_direction() {
    let args = parse_cli_args_from(["--trade-direction", "both"]).unwrap();
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Both);
}
#[test]
fn rejects_unknown_trade_direction() {
    let err = parse_cli_args_from(["--trade-direction", "inverse"]).unwrap_err();
    assert!(err.to_string().contains("unknown --trade-direction"));
}
#[test]
fn parses_raw_event_source_for_legacy_research() {
    let args = parse_cli_args_from(["--event-source", "raw_events"]).unwrap();
    assert_eq!(args.event_source, MarketVelocityEventSource::RawEvents);
}
#[test]
fn parses_raw_state_event_source_for_signal_state_research() {
    let args = parse_cli_args_from(["--event-source", "raw_state"]).unwrap();
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
}
#[test]
fn rejects_unknown_event_source() {
    let err = parse_cli_args_from(["--event-source", "market_rank_events"]).unwrap_err();
    assert!(err.to_string().contains("unknown --event-source"));
}
#[test]
fn parses_optional_max_delta_rank_research_filter() {
    let args = parse_cli_args_from(["--min-delta-rank", "15", "--max-delta-rank", "79"]).unwrap();
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(79));
}
#[test]
fn parses_optional_min_price_change_pct_research_filter() {
    let args = parse_cli_args_from(["--min-price-change-pct", "5.0"]).unwrap();
    assert_eq!(args.min_price_change_pct, Some(5.0));
}
#[test]
fn parses_optional_max_price_change_pct_research_filter() {
    let args = parse_cli_args_from(["--max-price-change-pct", "15.0"]).unwrap();
    assert_eq!(args.max_price_change_pct, Some(15.0));
}
#[test]
fn parses_event_time_window_filters() {
    let args = parse_cli_args_from([
        "--event-start-ms",
        "1717200000000",
        "--event-end-ms",
        "1719791999999",
    ])
    .unwrap();
    assert_eq!(args.event_start_ms, Some(1717200000000));
    assert_eq!(args.event_end_ms, Some(1719791999999));
}
#[test]
fn parses_optional_entry_max_signal_pullback_pct() {
    let args = parse_cli_args_from(["--entry-max-signal-pullback-pct", "3.0"]).unwrap();
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
}
#[test]
fn rejects_removed_new_rank_strategy_filters() {
    for flag in [
        "--max-new-rank",
        "--tail-new-rank-threshold",
        "--tail-rank-min-price-change-pct",
        "--chase-top-rank",
        "--chase-price-change-pct",
        "--entry-trigger-rank-blocklist",
    ] {
        let err = parse_cli_args_from([flag, "1"]).unwrap_err();
        assert!(err.to_string().contains("unknown argument"));
    }
}
#[test]
fn parses_equity_quartile_report() {
    let args = parse_cli_args_from(["--equity-quartile-report"]).unwrap();
    assert!(args.equity_quartile_report);
}
#[test]
fn parses_equity_trigger_report() {
    let args = parse_cli_args_from(["--equity-trigger-report"]).unwrap();
    assert!(args.equity_trigger_report);
}
#[test]
fn parses_equity_concentration_report() {
    let args = parse_cli_args_from(["--equity-concentration-report"]).unwrap();
    assert!(args.equity_concentration_report);
}
#[test]
fn parses_equity_feature_report() {
    let args = parse_cli_args_from(["--equity-feature-report"]).unwrap();
    assert!(args.equity_feature_report);
}
#[test]
fn parses_equity_symbol_window_report() {
    let args = parse_cli_args_from(["--equity-symbol-window-report"]).unwrap();
    assert!(args.equity_symbol_window_report);
}
#[test]
fn parses_equity_trade_report() {
    let args = parse_cli_args_from(["--equity-trade-report"]).unwrap();
    assert!(args.equity_trade_report);
}
#[test]
fn parses_save_backtest_detail() {
    let args = parse_cli_args_from(["--save-backtest-detail"]).unwrap();
    assert!(args.save_backtest_detail);
}
#[test]
fn rejects_max_delta_rank_below_min_delta_rank() {
    let err =
        parse_cli_args_from(["--min-delta-rank", "80", "--max-delta-rank", "79"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--max-delta-rank must be greater than or equal to --min-delta-rank"));
}
#[test]
fn rejects_negative_min_price_change_pct() {
    let err = parse_cli_args_from(["--min-price-change-pct", "-0.1"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--min-price-change-pct must be zero or greater"));
}
#[test]
fn rejects_negative_max_price_change_pct() {
    let err = parse_cli_args_from(["--max-price-change-pct", "-0.1"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--max-price-change-pct must be zero or greater"));
}
#[test]
fn rejects_event_end_ms_before_event_start_ms() {
    let err = parse_cli_args_from([
        "--event-start-ms",
        "1719791999999",
        "--event-end-ms",
        "1717200000000",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--event-end-ms must be greater than or equal to --event-start-ms"));
}
#[test]
fn rejects_negative_entry_max_signal_pullback_pct() {
    let err = parse_cli_args_from(["--entry-max-signal-pullback-pct", "-0.1"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-max-signal-pullback-pct must be zero or greater"));
}
#[test]
fn rejects_max_price_change_below_min_price_change() {
    let err = parse_cli_args_from([
        "--min-price-change-pct",
        "15.0",
        "--max-price-change-pct",
        "10.0",
    ])
    .unwrap_err();
    assert!(err.to_string().contains(
        "--max-price-change-pct must be greater than or equal to --min-price-change-pct"
    ));
}
#[test]
fn parses_impulse_retrace_fill_pct() {
    let args = parse_cli_args_from(["--fvg-impulse-retrace-fill-pct", "10"]).unwrap();
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 10.0);
}
#[test]
fn rejects_impulse_retrace_fill_pct_above_100() {
    let err = parse_cli_args_from(["--fvg-impulse-retrace-fill-pct", "120"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--fvg-impulse-retrace-fill-pct must be greater than 0 and at most 100"));
}
#[test]
fn parses_impulse_retrace_min_wait_candles() {
    let args = parse_cli_args_from(["--fvg-impulse-retrace-min-wait-candles", "2"]).unwrap();
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 2);
}
