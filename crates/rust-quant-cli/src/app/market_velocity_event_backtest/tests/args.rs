use super::super::parse_cli_args_from;

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
fn parses_tail_rank_min_price_change_research_filter() {
    let args = parse_cli_args_from([
        "--tail-new-rank-threshold",
        "21",
        "--tail-rank-min-price-change-pct",
        "10.0",
    ])
    .unwrap();

    assert_eq!(args.tail_new_rank_threshold, Some(21));
    assert_eq!(args.tail_rank_min_price_change_pct, Some(10.0));
}

#[test]
fn parses_entry_trigger_rank_blocklist_research_filter() {
    let args = parse_cli_args_from([
        "--entry-trigger-rank-blocklist",
        "reclaim_ema:11-20,breakout_previous_high:27-30",
    ])
    .unwrap();

    assert_eq!(args.entry_trigger_rank_blocklist.len(), 2);
    assert_eq!(args.entry_trigger_rank_blocklist[0].trigger, "reclaim_ema");
    assert_eq!(args.entry_trigger_rank_blocklist[0].min_new_rank, 11);
    assert_eq!(args.entry_trigger_rank_blocklist[0].max_new_rank, 20);
    assert_eq!(
        args.entry_trigger_rank_blocklist[1].trigger,
        "breakout_previous_high"
    );
    assert_eq!(args.entry_trigger_rank_blocklist[1].min_new_rank, 27);
    assert_eq!(args.entry_trigger_rank_blocklist[1].max_new_rank, 30);
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
fn rejects_tail_rank_threshold_without_min_price_change_pct() {
    let err = parse_cli_args_from(["--tail-new-rank-threshold", "21"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("--tail-new-rank-threshold requires --tail-rank-min-price-change-pct"));
}

#[test]
fn rejects_tail_rank_min_price_change_without_threshold() {
    let err = parse_cli_args_from(["--tail-rank-min-price-change-pct", "10.0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("--tail-rank-min-price-change-pct requires --tail-new-rank-threshold"));
}

#[test]
fn rejects_negative_tail_rank_min_price_change_pct() {
    let err = parse_cli_args_from([
        "--tail-new-rank-threshold",
        "21",
        "--tail-rank-min-price-change-pct",
        "-0.1",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--tail-rank-min-price-change-pct must be zero or greater"));
}

#[test]
fn rejects_invalid_entry_trigger_rank_blocklist_range() {
    let err =
        parse_cli_args_from(["--entry-trigger-rank-blocklist", "reclaim_ema:20-11"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("entry trigger rank block max rank must be >= min rank"));
}
