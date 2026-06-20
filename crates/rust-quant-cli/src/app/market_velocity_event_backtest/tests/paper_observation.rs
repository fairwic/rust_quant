use super::super::{
    parse_paper_observation_args_from, parse_paper_observation_command_from,
    MarketVelocityPaperOutcomeSink, StopReentryMode,
};

#[test]
fn paper_observation_args_force_web_sink_and_production_entry_trigger_allowlist() {
    let args = parse_paper_observation_args_from([] as [&str; 0]).unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_timing_v1"
    );
}

#[test]
fn paper_observation_args_apply_stop_reentry_profit_preset() {
    let args =
        parse_paper_observation_args_from(["--paper-strategy-preset", "stop_reentry_025sl_24r_v1"])
            .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(args.stop_reentry_mode, StopReentryMode::BreakoutReclaim);
    assert_eq!(args.stop_loss_pct, 0.025);
    assert_eq!(args.target_rs, vec![2.4]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1"
    );
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, None);
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
        "stop_reentry_025sl_24r_v1",
        "--target-rs",
        "2.0",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --target-rs"));
}

#[test]
fn paper_observation_args_reject_preset_stop_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "stop_reentry_025sl_24r_v1",
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
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
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
        vec!["breakout_previous_high"]
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
        vec!["breakout_previous_high"]
    );
}

#[test]
fn paper_observation_command_rejects_zero_loop_interval() {
    let err = parse_paper_observation_command_from(["--loop-interval-seconds", "0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("--loop-interval-seconds must be greater than 0"));
}
