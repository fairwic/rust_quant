use super::super::{
    market_velocity_paper_observation_usage, parse_paper_observation_args_from,
    parse_paper_observation_command_from, FvgEntryMode, MarketVelocityEventSource,
    MarketVelocityPaperOutcomeSink, MarketVelocityPaperStrategySignalSink,
    MarketVelocityStopLossMode, MarketVelocityTradeDirection, MarketVelocityTrendTimeframe,
    StopReentryMode,
};
const STABLE_PRODUCTION_PRESET: &str =
    "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1";
const STABLE_PRODUCTION_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1";

#[test]
fn paper_observation_args_force_web_sink_and_production_entry_trigger_allowlist() {
    let args = parse_paper_observation_args_from([] as [&str; 0]).unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["reclaim_ema", "reclaim_ma", "pullback_hold_ema"]
    );
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![1.7]);
    assert_eq!(args.entry_max_distance_pct, 5.5);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 18);
    assert_eq!(args.max_delta_rank, Some(42));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.entry_max_signal_pullback_pct, None);
    assert!(!args.entry_retest_after_signal);
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
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
fn paper_observation_args_apply_stable_production_preset() {
    let args =
        parse_paper_observation_args_from(["--paper-strategy-preset", STABLE_PRODUCTION_PRESET])
            .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["reclaim_ema", "reclaim_ma", "pullback_hold_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![1.7]);
    assert_eq!(args.entry_max_distance_pct, 5.5);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 18);
    assert_eq!(args.max_delta_rank, Some(42));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.entry_max_signal_pullback_pct, None);
    assert!(!args.entry_retest_after_signal);
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
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
    assert_eq!(args.min_price_change_pct, Some(5.0));
}
#[test]
fn paper_observation_args_apply_reclaim_gap_retest_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r0375_26r_gap05_rt03_rcm13_22_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![2.6]);
    assert_eq!(args.entry_max_distance_pct, 5.5);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.entry_max_gap_without_retest_pct, Some(0.5));
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 13);
    assert_eq!(args.max_delta_rank, Some(75));
    assert_eq!(args.min_price_change_pct, Some(5.0));
}
#[test]
fn paper_observation_args_apply_signal_retest_no_new_rank_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r0375_15r_sigrt2_d24_34_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![1.5]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 2);
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, Some(0.0));
    assert_eq!(args.min_delta_rank, 24);
    assert_eq!(args.max_delta_rank, Some(34));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait5_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r0375_20r_rcm_fvg5_d20_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 5);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_fvg_wait10_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r0375_20r_brk_rcm_fvg10_d20_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_fvg_wait10_04sl_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d20_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_fvg_wait10_04sl_delta15_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_fvg_wait10_04sl_delta15_40_runner6r20_stop1_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_r6f20_s1_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
    assert_eq!(args.runner_target_r, Some(6.0));
    assert_eq!(args.runner_fraction, 0.2);
    assert_eq!(args.runner_stop_r, 1.0);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_fvg_wait10_04sl_delta15_40_runner8r20_stop1_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_r8f20_s1_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
    assert_eq!(args.runner_target_r, Some(8.0));
    assert_eq!(args.runner_fraction, 0.2);
    assert_eq!(args.runner_stop_r, 1.0);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait10_04sl_delta15_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_rcm_fvg10_d15_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait10_04sl_18r_delta15_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg10_d15_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait10_04sl_18r_delta20_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg10_d20_40_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait12_04sl_18r_delta20_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg12_d20_40_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 12);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait14_pullback3_04sl_18r_delta20_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg14_d3_pb3_vol11_fp10_d20_40_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 3.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 10.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 14);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait14_retest1_pullback3_04sl_18r_delta20_40_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_pb3_vol11_d20_40_p5_10_v2"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.stop_loss_mode, MarketVelocityStopLossMode::FixedPct);
    assert_eq!(args.structure_stop_min_pct, 0.0);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, None);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.runner_target_r, None);
    assert_eq!(args.runner_fraction, 0.0);
    assert_eq!(args.runner_stop_r, 0.0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait14_retest1_gap0_pullback3_04sl_18r_delta20_40_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_t2_gap0_pb3_vol11_d20_40_p5_10_v3"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 2.0);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, Some(0.0));
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_fvg_wait14_retest1_gap0_openfadevol2_pullback3_04sl_18r_delta20_40_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_t2_gap0_ofv2_pb3_v11_d20_40_p5_10_v4"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 2.0);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, Some(0.0));
    assert_eq!(args.entry_retest_open_fade_min_volume_ratio, Some(2.0));
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_retest1_pullback3_04sl_18r_delta20_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 3.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, None);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_reclaim_retest1_pullback3_04sl_20r_delta20_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 3.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, None);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_retest1_04sl_18r_delta20_40_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_breakout_reclaim_retest1_delta20_40_pchg5_10_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_brk_rcm_rt1_vol10_d20_40_p5_10_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.entry_retest_min_entry_open_gap_pct, None);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(10.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_hybrid_04sl_18r_delta20_40_pchg5_8_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_breakout_reclaim_fvg_retest1_delta20_40_pchg5_8_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_18r_brk_rcm_fvg_rt1_vol10_d20_40_p5_8_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.8]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.entry_max_signal_pullback_pct, Some(3.0));
    assert!(args.entry_retest_after_signal);
    assert_eq!(args.entry_retest_max_wait_candles, 1);
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
    assert_eq!(args.min_delta_rank, 20);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(8.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_lookback_candles, 40);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_fvg_wait10_minwait1_04sl_delta15_40_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_mw1_d15_40_p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 15);
    assert_eq!(args.max_delta_rank, Some(40));
    assert_eq!(args.min_price_change_pct, Some(5.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_max_wait_candles, 10);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 1);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_kline15m_breakout_fvg20_04sl_10r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_10r_kline15m_breakout_fvg20_vol13_dd35_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "kline15m_mom04_10r_brk_fvg20_vol13_dd35_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.3);
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
    assert_eq!(args.min_delta_rank, 0);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.max_price_change_pct, None);
    assert_eq!(args.sample_limit, 20);
    assert_eq!(args.sample_seed, "kline15m_fvg20_v1");
    assert_eq!(args.entry_min_rsi, Some(50.0));
    assert_eq!(args.entry_max_rsi, Some(90.0));
    assert!(args.entry_bollinger_breakout);
    assert_eq!(args.entry_min_recent_drawdown_pct, Some(3.5));
    assert_eq!(args.entry_recent_drawdown_lookback_candles, 12);
    assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_lookback_candles, 40);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_kline15m_breakout_fvg20_04sl_06r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_06r_kline15m_breakout_fvg20_vol13_dd35_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "kline15m_mom04_06r_brk_fvg20_vol13_dd35_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.6]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.3);
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
    assert_eq!(args.min_delta_rank, 0);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.max_price_change_pct, None);
    assert_eq!(args.sample_limit, 20);
    assert_eq!(args.sample_seed, "kline15m_fvg20_v1");
    assert_eq!(args.entry_min_rsi, Some(50.0));
    assert_eq!(args.entry_max_rsi, Some(90.0));
    assert!(args.entry_bollinger_breakout);
    assert_eq!(args.entry_min_recent_drawdown_pct, Some(3.5));
    assert_eq!(args.entry_recent_drawdown_lookback_candles, 12);
    assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_lookback_candles, 40);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_kline15m_breakout_fvg30_04sl_05r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_05r_kline15m_breakout_fvg30_vol13_dd35_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "kline15m_mom04_05r_brk_fvg30_vol13_dd35_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.5]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.3);
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
    assert_eq!(args.min_delta_rank, 0);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.max_price_change_pct, None);
    assert_eq!(args.sample_limit, 20);
    assert_eq!(args.sample_seed, "kline15m_fvg30_v1");
    assert_eq!(args.entry_min_rsi, Some(50.0));
    assert_eq!(args.entry_max_rsi, Some(90.0));
    assert!(args.entry_bollinger_breakout);
    assert_eq!(args.entry_min_recent_drawdown_pct, Some(3.5));
    assert_eq!(args.entry_recent_drawdown_lookback_candles, 12);
    assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_lookback_candles, 40);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 30.0);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_kline15m_breakout_fvg30_04sl_055r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_055r_kline15m_breakout_fvg30_vol13_dd35_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "kline15m_mom04_055r_brk_fvg30_vol13_dd35_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.55]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.3);
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
    assert_eq!(args.min_delta_rank, 0);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.max_price_change_pct, None);
    assert_eq!(args.sample_limit, 20);
    assert_eq!(args.sample_seed, "kline15m_fvg30_v1");
    assert_eq!(args.entry_min_rsi, Some(50.0));
    assert_eq!(args.entry_max_rsi, Some(90.0));
    assert!(args.entry_bollinger_breakout);
    assert_eq!(args.entry_min_recent_drawdown_pct, Some(3.5));
    assert_eq!(args.entry_recent_drawdown_lookback_candles, 12);
    assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_lookback_candles, 40);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 30.0);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_kline15m_breakout_fvg50_04sl_052r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.52]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.3);
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
    assert_eq!(args.min_delta_rank, 0);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.max_price_change_pct, None);
    assert_eq!(args.sample_limit, 20);
    assert_eq!(args.sample_seed, "kline15m_fvg50_v1");
    assert_eq!(args.entry_min_rsi, Some(50.0));
    assert_eq!(args.entry_max_rsi, Some(90.0));
    assert!(args.entry_bollinger_breakout);
    assert_eq!(args.entry_min_recent_drawdown_pct, Some(3.5));
    assert_eq!(args.entry_recent_drawdown_lookback_candles, 12);
    assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15ImpulseRetrace);
    assert_eq!(args.fvg_lookback_candles, 40);
    assert_eq!(args.fvg_max_wait_candles, 24);
    assert_eq!(args.fvg_impulse_retrace_fill_pct, 50.0);
    assert_eq!(args.fvg_impulse_retrace_min_wait_candles, 0);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_kline15m_direct_shape_04sl_10r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_10r_kline15m_direct_shape_reclaimema_vol12_body65_close80_rng15_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.entry_trigger_allowlist, vec!["reclaim_ema"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "kline15m_direct_rcmema_04sl_10r_v12_b65_c80_r15_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.2);
    assert_eq!(args.entry_min_body_ratio_pct, Some(65.0));
    assert_eq!(args.entry_min_close_position_pct, Some(80.0));
    assert_eq!(args.entry_min_range_expansion_ratio, Some(1.5));
    assert_eq!(args.trend_timeframe, MarketVelocityTrendTimeframe::Off);
    assert_eq!(args.min_delta_rank, 0);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, Some(0.8));
    assert_eq!(args.max_price_change_pct, Some(8.0));
    assert_eq!(args.sample_limit, 40);
    assert_eq!(args.sample_seed, "kline15m_direct_shape_v1");
    assert_eq!(args.entry_min_rsi, Some(50.0));
    assert_eq!(args.entry_max_rsi, Some(92.0));
    assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_10r_0375sl_delta11_72_dist14_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_10r_breakout_reclaim_delta11_72_pchg4_12_dist14_vol11_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r0375_10r_brk_rcm_d11_72_p4_12_dist14_vol11_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 11);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(4.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(!args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_breakout_reclaim_ma_10r_0375sl_delta11_72_dist14_ignore_research_preset(
) {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_10r_breakout_reclaim_ma_delta11_72_pchg4_12_dist14_vol11_ignore_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema", "reclaim_ma"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h15m_r0375_10r_brk_rcm_ma_ign_d11_72_p4_12_dist14_vol11_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 11);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(4.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_short_15m_support_breakdown_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakdown_range_low"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_15m_short_r0375_10r_15msup_brkdn_d5_72_p1p5_12_v1"
    );
    assert_eq!(args.stop_loss_pct, 0.0375);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 8.0);
    assert_eq!(args.entry_min_volume_ratio, 1.3);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(1.5));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_short_15m_support_breakdown_v2_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_short_04sl_10r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_prevlow_v2",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakdown_range_low", "breakdown_previous_low"]
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_15m_short_r04_10r_15msup_brkdn_d5_72_p1p5_12_vol11_prev_v2"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 8.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(1.5));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_args_apply_short_15m_support_breakdown_v3_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_short_04sl_06r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_dist5_v3",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakdown_range_low"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_15m_short_r04_06r_15msup_brkdn_d5_72_p1p5_12_vol11_d5_v3"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.6]);
    assert_eq!(args.entry_max_distance_pct, 5.0);
    assert_eq!(args.entry_min_volume_ratio, 1.1);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(1.5));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

// 验证 v4 破位做空 preset 仍只绑定动量异动 raw_state 与纸面观察参数。
#[test]
fn paper_observation_args_apply_short_15m_support_breakdown_v4_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_short_04sl_06r_15m_support_breakdown_d5_72_pchg1_12_vol10_dist8_v4",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakdown_range_low"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_15m_short_r04_06r_15msup_brkdn_d5_72_p1_12_vol10_d8_v4"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.6]);
    assert_eq!(args.entry_max_distance_pct, 8.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, Some(72));
    assert_eq!(args.min_price_change_pct, Some(1.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

// 验证 v5 破位做空 preset 放宽样本门槛但仍保持放量、raw_state 与横盘下沿破位约束。
#[test]
fn paper_observation_args_apply_short_15m_support_breakdown_v5_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_short_04sl_065r_15m_support_breakdown_d1_100_pchg0p5_12_vol10_dist14_v5",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakdown_range_low"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_15m_short_r04_065r_15msup_brkdn_d1_100_p0p5_12_vol10_d14_v5"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![0.65]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.min_delta_rank, 1);
    assert_eq!(args.max_delta_rank, Some(100));
    assert_eq!(args.min_price_change_pct, Some(0.5));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

// 验证 v6 破位做空 preset 收紧样本，同时保持 paper-only 观察语义。
#[test]
fn paper_observation_args_apply_short_15m_support_breakdown_v6_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::RawState);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Short);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakdown_range_low"]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_15m_short_r04_10r_15msup_brkdn_d5_100_p2_12_vol10_d14_v6"
    );
    assert_eq!(args.stop_loss_pct, 0.04);
    assert_eq!(args.target_rs, vec![1.0]);
    assert_eq!(args.entry_max_distance_pct, 14.0);
    assert_eq!(args.entry_min_volume_ratio, 1.0);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, Some(100));
    assert_eq!(args.min_price_change_pct, Some(2.0));
    assert_eq!(args.max_price_change_pct, Some(12.0));
    assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
    assert!(args.ignore_entry_signal_updates_while_open);
}

#[test]
fn paper_observation_usage_lists_short_15m_support_breakdown_v5_preset() {
    assert!(
        market_velocity_paper_observation_usage().contains(
            "research_momentum_short_04sl_065r_15m_support_breakdown_d1_100_pchg0p5_12_vol10_dist14_v5"
        ),
        "paper observation usage must list the production v5 breakdown-short preset"
    );
}

#[test]
fn paper_observation_usage_lists_short_15m_support_breakdown_v6_preset() {
    assert!(
        market_velocity_paper_observation_usage().contains(
            "research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6"
        ),
        "paper observation usage must list the production v6 breakdown-short preset"
    );
}

#[test]
fn paper_observation_usage_lists_kline15m_direct_shape_preset() {
    assert!(
        market_velocity_paper_observation_usage().contains(
            "research_momentum_04sl_10r_kline15m_direct_shape_reclaimema_vol12_body65_close80_rng15_v1"
        ),
        "paper observation usage must list the direct 15m kline shape research preset"
    );
}

#[test]
fn paper_observation_entry_rule_versions_fit_quant_web_contract() {
    let presets = [
        "momentum_03sl_20r_v5",
        "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1",
        "research_momentum_0375sl_27r_reclaim13_22_v1",
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1",
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
        "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1",
        "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1",
        "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4",
        "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_breakout_reclaim_retest1_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_breakout_reclaim_fvg_retest1_delta20_40_pchg5_8_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1",
        "research_momentum_0375sl_10r_breakout_reclaim_delta11_72_pchg4_12_dist14_vol11_v1",
        "research_momentum_0375sl_10r_breakout_reclaim_ma_delta11_72_pchg4_12_dist14_vol11_ignore_v1",
        "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1",
        "research_momentum_short_04sl_10r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_prevlow_v2",
        "research_momentum_short_04sl_06r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_dist5_v3",
        "research_momentum_short_04sl_06r_15m_support_breakdown_d5_72_pchg1_12_vol10_dist8_v4",
        "research_momentum_short_04sl_065r_15m_support_breakdown_d1_100_pchg0p5_12_vol10_dist14_v5",
        "research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6",
        "research_momentum_04sl_10r_kline15m_breakout_fvg20_vol13_dd35_v1",
        "research_momentum_04sl_06r_kline15m_breakout_fvg20_vol13_dd35_v1",
        "research_momentum_04sl_05r_kline15m_breakout_fvg30_vol13_dd35_v1",
        "research_momentum_04sl_10r_kline15m_direct_shape_reclaimema_vol12_body65_close80_rng15_v1",
        "research_episode_momentum_03sl_24r_rank5_30_v1",
        "research_episode_momentum_05sl_20r_rank5_v1",
        "research_episode_momentum_05sl_30r_rank5_v1",
        "research_episode_runner_03sl_24r_8r30_v1",
    ];
    for preset in presets {
        let args = parse_paper_observation_args_from(["--paper-strategy-preset", preset]).unwrap();
        assert!(
            args.paper_outcome_entry_rule_version.len() <= 80,
            "preset {} entry_rule_version too long for quant_web contract: {} ({})",
            preset,
            args.paper_outcome_entry_rule_version,
            args.paper_outcome_entry_rule_version.len()
        );
    }
}
#[test]
fn paper_observation_args_apply_episode_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_episode_momentum_03sl_24r_rank5_30_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Episodes);
    assert!(args.entry_trigger_allowlist.is_empty());
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.03);
    assert_eq!(args.target_rs, vec![2.4]);
    assert_eq!(args.entry_max_distance_pct, 7.0);
    assert_eq!(args.entry_min_volume_ratio, 0.8);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, None);
}
#[test]
fn paper_observation_args_apply_episode_05sl_20r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_episode_momentum_05sl_20r_rank5_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Episodes);
    assert!(args.entry_trigger_allowlist.is_empty());
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_episode_research_05sl_20r_rank5_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.05);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.entry_max_distance_pct, 7.0);
    assert_eq!(args.entry_min_volume_ratio, 0.8);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, None);
}
#[test]
fn paper_observation_args_apply_episode_05sl_30r_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_episode_momentum_05sl_30r_rank5_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Episodes);
    assert!(args.entry_trigger_allowlist.is_empty());
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_episode_research_05sl_30r_rank5_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.05);
    assert_eq!(args.target_rs, vec![3.0]);
    assert_eq!(args.entry_max_distance_pct, 7.0);
    assert_eq!(args.entry_min_volume_ratio, 0.8);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, None);
}
#[test]
fn paper_observation_args_apply_episode_runner_research_preset() {
    let args = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_episode_runner_03sl_24r_8r30_v1",
    ])
    .unwrap();
    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.event_source, MarketVelocityEventSource::Episodes);
    assert!(args.entry_trigger_allowlist.is_empty());
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_episode_runner_03sl_24r_8r30_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
    assert_eq!(args.stop_loss_pct, 0.03);
    assert_eq!(args.target_rs, vec![2.4]);
    assert_eq!(args.entry_max_distance_pct, 7.0);
    assert_eq!(args.entry_min_volume_ratio, 0.8);
    assert_eq!(args.trend_min_average_distance_pct, 0.0);
    assert_eq!(args.min_delta_rank, 5);
    assert_eq!(args.max_delta_rank, None);
    assert_eq!(args.min_price_change_pct, None);
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, Some(8.0));
    assert_eq!(args.runner_fraction, 0.3);
    assert_eq!(args.runner_stop_r, 0.0);
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
fn paper_observation_args_reject_preset_reclaim_gap_retest_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1",
        "--entry-max-gap-without-retest-pct",
        "0.8",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --entry-max-gap-without-retest-pct"));

    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1",
        "--entry-retest-tolerance-pct=0.2",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --entry-retest-tolerance-pct"));

    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
        "--entry-retest-after-signal",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --entry-retest-after-signal"));

    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
        "--entry-retest-max-wait-candles",
        "8",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --entry-retest-max-wait-candles"));

    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
        "--entry-retest-min-entry-open-gap-pct",
        "0.2",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --entry-retest-min-entry-open-gap-pct"));

    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1",
        "--entry-max-signal-pullback-pct",
        "2.5",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --entry-max-signal-pullback-pct"));
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
fn paper_observation_args_reject_preset_event_source_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_episode_momentum_03sl_24r_rank5_30_v1",
        "--event-source",
        "raw_state",
    ])
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --event-source"));
}
#[test]
fn paper_observation_args_reject_entry_trigger_filter_overrides() {
    let err = parse_paper_observation_args_from(["--entry-trigger-allowlist", "all"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --entry-trigger-allowlist"));
}
#[test]
fn paper_observation_args_reject_save_backtest_detail() {
    let err = parse_paper_observation_args_from(["--save-backtest-detail"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --save-backtest-detail"));
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
fn paper_observation_args_reject_impulse_retrace_fill_pct() {
    let err =
        parse_paper_observation_args_from(["--fvg-impulse-retrace-fill-pct", "10"]).unwrap_err();
    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --fvg-impulse-retrace-fill-pct"));
}
#[test]
fn paper_observation_args_reject_impulse_retrace_min_wait_candles() {
    let err = parse_paper_observation_args_from(["--fvg-impulse-retrace-min-wait-candles", "2"])
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --fvg-impulse-retrace-min-wait-candles"));
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
        vec!["reclaim_ema", "reclaim_ma", "pullback_hold_ema"]
    );
    assert_eq!(
        command.backtest_args.paper_outcome_entry_rule_version,
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(command.backtest_args.stop_loss_pct, 0.0375);
    assert_eq!(command.backtest_args.target_rs, vec![1.7]);
    assert_eq!(
        command.backtest_args.paper_strategy_signal_sink,
        MarketVelocityPaperStrategySignalSink::Off
    );
    assert_eq!(command.loop_interval_seconds, None);
}
#[test]
fn paper_observation_command_accepts_strategy_signal_web_sink() {
    let command = parse_paper_observation_command_from([
        "--paper-strategy-preset",
        "research_momentum_short_04sl_065r_15m_support_breakdown_d1_100_pchg0p5_12_vol10_dist14_v5",
        "--paper-strategy-signal-sink",
        "web",
    ])
    .unwrap();
    assert_eq!(
        command.backtest_args.paper_strategy_signal_sink,
        MarketVelocityPaperStrategySignalSink::Web
    );
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
