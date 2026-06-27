use super::super::{
    market_velocity_paper_strategy_preset_manifest, parse_paper_observation_args_from,
    parse_paper_observation_command_from, FvgEntryMode, MarketVelocityEventSource,
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
fn paper_observation_entry_rule_versions_fit_quant_web_contract() {
    let presets = [
        "momentum_03sl_20r_v5",
        "research_momentum_0375sl_27r_reclaim13_22_v1",
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1",
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
        "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1",
        "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
        "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3",
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4",
        "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1",
        "research_episode_momentum_03sl_24r_rank5_30_v1",
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
fn paper_observation_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_27r_reclaim13_22_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(manifest.symbol, "ALL");
    assert_eq!(manifest.channel, "production_default");
    assert_eq!(manifest.strategy_key, "market_velocity");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 2.7R reclaim13-22 v1"
    );
    assert_eq!(manifest.risk_level, "high");
    assert_eq!(manifest.manifest_json["strategy_key"], "market_velocity");
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_0375sl_27r_reclaim13_22_v1"
    );
    assert_eq!(
        manifest.manifest_json["execution"]["service_mode"],
        "signal_only"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["stop_loss_pct"],
        0.0375
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.7);
    assert!(manifest.manifest_json["parameters"]
        .get("max_new_rank")
        .is_none());
    assert!(manifest.manifest_json["filters"]
        .get("entry_trigger_rank_blocklist")
        .is_none());
    assert!(manifest
        .canonical_json
        .contains("\"manifest_schema_version\":1"));
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}
#[test]
fn paper_observation_reclaim_gap_retest_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 2.6R gap0.5 retest0.3 reclaim13-22 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1"
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.6);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_gap_without_retest_pct"],
        0.5
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_tolerance_pct"],
        0.3
    );
    assert!(manifest.manifest_json["parameters"]
        .get("max_new_rank")
        .is_none());
    assert!(manifest.manifest_json["filters"]
        .get("entry_trigger_rank_blocklist")
        .is_none());
    assert!(manifest
        .canonical_json
        .contains("\"entry_max_gap_without_retest_pct\":0.5"));
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}
#[test]
fn paper_observation_signal_retest_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 1.5R signal retest2 delta24-34 pchg5-10 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1"
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.5);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_after_signal"],
        true
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_max_wait_candles"],
        2
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_min_entry_open_gap_pct"],
        0.0
    );
    assert!(manifest.manifest_json["parameters"]
        .get("max_new_rank")
        .is_none());
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        10.0
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait5_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 2.0R reclaim fvg wait5 delta20-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        12.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_breakout_reclaim_fvg_wait10_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 2.0R breakout reclaim fvg wait10 delta20-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        12.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high", "reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_breakout_reclaim_fvg_wait10_04sl_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta20-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high", "reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_breakout_reclaim_fvg_wait10_04sl_delta15_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta15-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 15);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high", "reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait10_04sl_delta15_40_preset_manifest_is_canonical_and_hashable()
{
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R reclaim fvg wait10 delta15-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 15);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait10_04sl_18r_delta15_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg wait10 delta15-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.8);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 15);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait10_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg wait10 delta20-40 pchg5-10 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.8);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 20);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        10.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait12_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg wait12 delta20-40 pchg5-10 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.8);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 20);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        10.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_max_wait_candles"],
        12
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait14_pullback3_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg wait14 dist3 pullback3 vol11 fill10 delta20-40 pchg5-10 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_signal_pullback_pct"],
        3.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_max_wait_candles"],
        14
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait14_retest1_pullback3_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg retest1 pullback3 vol11 delta20-40 pchg5-10 v2"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_signal_pullback_pct"],
        3.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_after_signal"],
        true
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_max_wait_candles"],
        1
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_max_wait_candles"],
        24
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_distance_pct"],
        5.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        20.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait14_retest1_gap0_pullback3_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg retest1 gap0 pullback3 vol11 delta20-40 pchg5-10 v3"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_signal_pullback_pct"],
        3.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_after_signal"],
        true
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_max_wait_candles"],
        1
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_tolerance_pct"],
        2.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_min_entry_open_gap_pct"],
        0.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_max_wait_candles"],
        24
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_distance_pct"],
        5.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        20.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_fvg_wait14_retest1_gap0_openfadevol2_pullback3_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim fvg retest1 gap0 open-fade-vol2 pullback3 vol11 delta20-40 pchg5-10 v4"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_signal_pullback_pct"],
        3.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_after_signal"],
        true
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_max_wait_candles"],
        1
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_tolerance_pct"],
        2.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_min_entry_open_gap_pct"],
        0.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_open_fade_min_volume_ratio"],
        2.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_max_wait_candles"],
        24
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_distance_pct"],
        5.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        20.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_reclaim_retest1_pullback3_04sl_18r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 1.8R reclaim retest1 dist3 pullback3 vol11 delta20-40 pchg5-10 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_signal_pullback_pct"],
        3.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_after_signal"],
        true
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_retest_max_wait_candles"],
        1
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["reclaim_ema"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_breakout_reclaim_fvg_wait10_minwait1_04sl_delta15_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 minwait1 delta15-40 pchg5-12 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_min_wait_candles"],
        1
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
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
