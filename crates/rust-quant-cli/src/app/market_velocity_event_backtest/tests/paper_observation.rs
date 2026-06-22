use super::super::{
    parse_paper_observation_args_from, parse_paper_observation_command_from,
    MarketVelocityPaperOutcomeSink, StopReentryMode,
};

#[test]
fn paper_observation_args_force_web_sink_and_production_entry_trigger_allowlist() {
    let args = parse_paper_observation_args_from([] as [&str; 0]).unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_timing_v1"
    );
    assert!(
        args.symbol_blocklist.is_empty(),
        "production paper observation must not default to a historical symbol blocklist"
    );
}

#[test]
fn paper_observation_args_apply_momentum_profit_preset() {
    let args =
        parse_paper_observation_args_from(["--paper-strategy-preset", "momentum_03sl_20r_v5"])
            .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.03);
    assert!(
        args.symbol_blocklist.is_empty(),
        "anti-overfit production preset must not carry historical symbol blocklist"
    );
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 4.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_momentum_03sl_20r_v5"
    );
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, None);
}

#[test]
fn paper_observation_args_apply_reclaim_midrank_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_27r_reclaim13_22_v1",
    ])
    .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_research_0375sl_27r_dist55_reclaim13_22_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![2.7]);
    assert_eq!(args.entry_max_distance_pct, 5.5);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 13);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.max_new_rank, 30);
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.chase_top_rank, 5);
    assert_eq!(args.chase_price_change_pct, 80.0);
    assert_eq!(args.entry_trigger_rank_blocklist.len(), 1);
    assert_eq!(args.entry_trigger_rank_blocklist[0].trigger, "reclaim_ema");
    assert_eq!(args.entry_trigger_rank_blocklist[0].min_new_rank, 13);
    assert_eq!(args.entry_trigger_rank_blocklist[0].max_new_rank, 22);
}

#[test]
fn paper_observation_args_reject_unknown_strategy_preset() {
    let err =
        parse_paper_observation_args_from(["--paper-strategy-preset", "unknown"]).unwrap_err();

    assert!(err.to_string().contains("unknown --paper-strategy-preset"));
}

#[test]
fn paper_observation_args_reject_preset_target_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "momentum_03sl_20r_v5",
        "--target-rs",
        "2.0",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --target-rs"));
}

#[test]
fn paper_observation_args_reject_preset_max_delta_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "momentum_03sl_20r_v5",
        "--max-delta-rank",
        "79",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --max-delta-rank"));
}

#[test]
fn paper_observation_args_reject_preset_min_price_change_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "momentum_03sl_20r_v5",
        "--min-price-change-pct",
        "5.0",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --min-price-change-pct"));
}

#[test]
fn paper_observation_args_reject_preset_stop_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "momentum_03sl_20r_v5",
        "--stop-loss-pct",
        "0.03",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --stop-loss-pct"));
}

#[test]
fn paper_observation_args_reject_entry_trigger_filter_overrides() {
    let err = parse_paper_observation_args_from(["--entry-trigger-allowlist", "all"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --entry-trigger-allowlist"));
}

#[test]
fn paper_observation_args_reject_entry_trigger_rank_blocklist_override() {
    let err =
        parse_paper_observation_args_from(["--entry-trigger-rank-blocklist", "reclaim_ema:13-22"])
            .unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --entry-trigger-rank-blocklist"));
}

#[test]
fn paper_observation_args_reject_stop_reentry_mode() {
    let err =
        parse_paper_observation_args_from(["--stop-reentry-mode", "breakout_reclaim"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --stop-reentry-mode"));
}

#[test]
fn paper_observation_args_reject_fvg_entry_mode() {
    let err = parse_paper_observation_args_from(["--fvg-entry-mode", "15m_to_1h"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --fvg-entry-mode"));
}

#[test]
fn paper_observation_args_reject_profit_protection() {
    let err = parse_paper_observation_args_from(["--profit-protect-after-r", "1.0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --profit-protect-after-r"));
}

#[test]
fn paper_observation_args_reject_runner_exit() {
    let err = parse_paper_observation_args_from(["--runner-target-r", "4.0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --runner-target-r"));
}

#[test]
fn paper_observation_args_keep_backtest_tunables() {
    let args =
        parse_paper_observation_args_from(["--target-rs", "2.0", "--stop-loss-pct", "0.025"])
            .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.stop_loss_pct, 0.025);
}

#[test]
fn paper_observation_command_defaults_to_one_shot() {
    let command = parse_paper_observation_command_from([] as [&str; 0]).unwrap();

    assert_eq!(
        command.backtest_args.paper_outcome_sink,
        MarketVelocityPaperOutcomeSink::Web
    );
    assert_eq!(
        command.backtest_args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(command.loop_interval_seconds, None);
}

#[test]
fn paper_observation_command_parses_loop_interval_without_losing_tunables() {
    let command = parse_paper_observation_command_from([
        "--loop-interval-seconds",
        "21600",
        "--target-rs",
        "2.0",
        "--stop-loss-pct",
        "0.025",
    ])
    .unwrap();

    assert_eq!(command.loop_interval_seconds, Some(21_600));
    assert_eq!(command.backtest_args.target_rs, vec![2.0]);
    assert_eq!(command.backtest_args.stop_loss_pct, 0.025);
    assert_eq!(
        command.backtest_args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
}

#[test]
fn paper_observation_command_rejects_zero_loop_interval() {
    let err = parse_paper_observation_command_from(["--loop-interval-seconds", "0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("--loop-interval-seconds must be greater than 0"));
}
