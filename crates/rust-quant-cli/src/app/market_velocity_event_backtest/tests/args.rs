use super::super::{
    market_velocity_paper_strategy_preset_manifest, parse_cli_args_from,
    parse_paper_observation_args_from, FvgEntryMode, MarketVelocityEventSource,
    MarketVelocityPaperOutcomeSink, MarketVelocityTradeDirection, MarketVelocityTrendTimeframe,
};

const RECLAIM_ONLY_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_20r_reclaim_delta13_72_pchg5_v1";
const RECLAIM_ONLY_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r0375_20r_rcm_d13_72_p5_v1";

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
fn defaults_to_4h_trend_timeframe() {
    let args = parse_cli_args_from([] as [&str; 0]).unwrap();
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::FourHour);
}
#[test]
fn parses_1h_trend_timeframe() {
    let args = parse_cli_args_from(["--trend-timeframe", "1h"]).unwrap();
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::OneHour);
}
#[test]
fn parses_off_trend_timeframe() {
    let args = parse_cli_args_from(["--trend-timeframe", "off"]).unwrap();
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
}
#[test]
fn kline_15m_event_source_defaults_to_no_higher_timeframe_trend() {
    let args = parse_cli_args_from(["--event-source", "kline_15m"]).unwrap();
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
}
#[test]
fn kline_15m_event_source_preserves_explicit_4h_trend() {
    let args =
        parse_cli_args_from(["--event-source", "kline_15m", "--trend-timeframe", "4h"]).unwrap();
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::FourHour);
}
#[test]
fn kline_15m_event_source_preserves_default_4h_when_trend_threshold_is_explicit() {
    let args = parse_cli_args_from([
        "--event-source",
        "kline_15m",
        "--trend-min-average-distance-pct",
        "0.5",
    ])
    .unwrap();
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::FourHour);
}
#[test]
fn rejects_unknown_trend_timeframe() {
    let err = parse_cli_args_from(["--trend-timeframe", "2h"]).unwrap_err();
    assert!(err.to_string().contains("unknown --trend-timeframe"));
}
#[test]
fn parses_fast_momentum_entry_filters() {
    let args = parse_cli_args_from([
        "--entry-min-rsi",
        "55",
        "--entry-max-rsi",
        "78",
        "--entry-min-rsi-delta",
        "3",
        "--entry-rsi-delta-lookback-candles",
        "3",
        "--entry-bollinger-breakout",
        "--entry-min-bollinger-bandwidth-expansion-pct",
        "12",
        "--entry-min-recent-drawdown-pct",
        "3.5",
        "--entry-recent-drawdown-lookback-candles",
        "12",
    ])
    .unwrap();
    assert_eq!(args.entry_min_rsi, Some(55.0));
    assert_eq!(args.entry_max_rsi, Some(78.0));
    assert_eq!(args.entry_min_rsi_delta, Some(3.0));
    assert_eq!(args.entry_rsi_delta_lookback_candles, 3);
    assert!(args.entry_bollinger_breakout);
    assert_eq!(args.entry_min_bollinger_bandwidth_expansion_pct, Some(12.0));
    assert_eq!(args.entry_min_recent_drawdown_pct, Some(3.5));
    assert_eq!(args.entry_recent_drawdown_lookback_candles, 12);
}
#[test]
fn parses_entry_symbol_cooldown_filter() {
    let args = parse_cli_args_from(["--entry-symbol-cooldown-candles", "8"]).unwrap();
    assert_eq!(args.entry_symbol_cooldown_candles, Some(8));
}
#[test]
fn parses_opposite_duration_filter() {
    let args = parse_cli_args_from(["--entry-min-opposite-duration-candles", "96"]).unwrap();
    assert_eq!(args.entry_min_opposite_duration_candles, Some(96));
}

#[test]
fn parses_btc_flat_regime_filter_and_rejects_zero() {
    let args = parse_cli_args_from(["--entry-btc-96-max-abs-net-move-pct", "2.0"]).unwrap();
    assert_eq!(args.entry_btc_96_max_abs_net_move_pct, Some(2.0));

    let error = parse_cli_args_from(["--entry-btc-96-max-abs-net-move-pct", "0"]).unwrap_err();
    assert!(error
        .to_string()
        .contains("--entry-btc-96-max-abs-net-move-pct must be greater than 0"));
}
#[test]
fn parses_btc_broad_direction_filter_and_rejects_invalid_values() {
    let zero = parse_cli_args_from(["--entry-btc-384-min-directional-net-move-pct", "0"]).unwrap();
    assert_eq!(zero.entry_btc_384_min_directional_net_move_pct, Some(0.0));

    let positive =
        parse_cli_args_from(["--entry-btc-384-min-directional-net-move-pct", "1.5"]).unwrap();
    assert_eq!(
        positive.entry_btc_384_min_directional_net_move_pct,
        Some(1.5)
    );

    for invalid in ["-0.1", "NaN"] {
        let error = parse_cli_args_from(["--entry-btc-384-min-directional-net-move-pct", invalid])
            .unwrap_err();
        assert!(error.to_string().contains(
            "--entry-btc-384-min-directional-net-move-pct must be finite and non-negative"
        ));
    }
}
#[test]
fn parses_btc_current_directional_candle_filter() {
    let args = parse_cli_args_from(["--entry-btc-require-current-directional-candle"]).unwrap();
    assert!(args.entry_btc_require_current_directional_candle);
}
#[test]
fn parses_exhaustion_volume_dominance_filter() {
    let args =
        parse_cli_args_from(["--entry-min-exhaustion-volume-dominance-ratio", "1.2"]).unwrap();
    assert_eq!(args.entry_min_exhaustion_volume_dominance_ratio, Some(1.2));
}
#[test]
fn rejects_invalid_fast_momentum_entry_filters() {
    let err = parse_cli_args_from(["--entry-min-rsi", "80", "--entry-max-rsi", "60"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-max-rsi must be greater than or equal to --entry-min-rsi"));
    let err = parse_cli_args_from(["--entry-rsi-delta-lookback-candles", "0"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-rsi-delta-lookback-candles must be greater than 0"));
    let err = parse_cli_args_from(["--entry-recent-drawdown-lookback-candles", "0"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-recent-drawdown-lookback-candles must be greater than 0"));
    let err = parse_cli_args_from(["--entry-symbol-cooldown-candles", "0"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-symbol-cooldown-candles must be greater than 0"));
    let err = parse_cli_args_from(["--entry-min-opposite-duration-candles", "3"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-min-opposite-duration-candles must be at least 4"));
    let err =
        parse_cli_args_from(["--entry-min-exhaustion-volume-dominance-ratio", "0"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("--entry-min-exhaustion-volume-dominance-ratio must be greater than 0"));
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
fn parses_kline_15m_event_source_for_signal_logic_research() {
    let args = parse_cli_args_from(["--event-source", "kline_15m"]).unwrap();
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
}

#[test]
fn enables_historical_volume_rank_velocity_only_for_kline_source() {
    let args = parse_cli_args_from([
        "--event-source",
        "kline_15m",
        "--kline-volume-rank-velocity",
    ])
    .unwrap();

    assert!(args.kline_volume_rank_velocity);
}

#[test]
fn rejects_historical_volume_rank_velocity_for_rank_event_sources() {
    let error = parse_cli_args_from(["--kline-volume-rank-velocity"]).unwrap_err();

    assert!(error
        .to_string()
        .contains("requires --event-source kline_15m"));
}

#[test]
fn requires_volume_rank_mode_before_enabling_turnover_growth_gate() {
    let error = parse_cli_args_from([
        "--event-source",
        "kline_15m",
        "--kline-volume-rank-require-turnover-growth",
    ])
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("requires --kline-volume-rank-velocity"));
}

#[test]
fn requires_volume_rank_mode_before_enabling_consecutive_rank_gate() {
    let error = parse_cli_args_from([
        "--event-source",
        "kline_15m",
        "--kline-volume-rank-require-consecutive-improvement",
    ])
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("requires --kline-volume-rank-velocity"));
}
#[test]
fn parses_kline_15m_sample_seed_for_reproducible_random_samples() {
    let args = parse_cli_args_from([
        "--event-source",
        "kline_15m",
        "--sample-limit",
        "20",
        "--sample-seed",
        "batch_a",
    ])
    .unwrap();
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.sample_limit, 20);
    assert_eq!(args.sample_seed, "batch_a");
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
fn paper_observation_args_apply_reclaim_only_0375sl_20r_delta13_72_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        RECLAIM_ONLY_RESEARCH_PRESET,
    ])
    .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        RECLAIM_ONLY_RESEARCH_ENTRY_RULE_VERSION
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.5);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 13);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, None);
    assert!(!args.entry_retest_after_signal);
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
}

#[test]
fn paper_observation_reclaim_only_entry_rule_version_fits_quant_web_contract() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        RECLAIM_ONLY_RESEARCH_PRESET,
    ])
    .unwrap();

    assert!(
        args.paper_outcome_entry_rule_version.len() <= 80,
        "preset {} entry_rule_version too long for quant_web contract: {} ({})",
        RECLAIM_ONLY_RESEARCH_PRESET,
        args.paper_outcome_entry_rule_version,
        args.paper_outcome_entry_rule_version.len()
    );
}

#[test]
fn paper_observation_reclaim_only_preset_manifest_is_canonical_and_hashable() {
    let manifest =
        market_velocity_paper_strategy_preset_manifest(RECLAIM_ONLY_RESEARCH_PRESET).unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 2.0R reclaim delta13-72 pchg5 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        RECLAIM_ONLY_RESEARCH_PRESET
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["stop_loss_pct"],
        0.0375
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 13);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 72);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "off"
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
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
