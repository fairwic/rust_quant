use super::super::market_velocity_paper_strategy_preset_manifest;
use serde_json::Value;

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
fn paper_observation_breakout_reclaim_fvg_wait10_04sl_delta15_40_runner6r20_stop1_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta15-40 pchg5-12 runner6R20 stop1 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 15);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(manifest.manifest_json["parameters"]["runner_target_r"], 6.0);
    assert_eq!(manifest.manifest_json["parameters"]["runner_fraction"], 0.2);
    assert_eq!(manifest.manifest_json["parameters"]["runner_stop_r"], 1.0);
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
fn paper_observation_breakout_reclaim_fvg_wait10_04sl_delta15_40_runner8r20_stop1_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta15-40 pchg5-12 runner8R20 stop1 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 15);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 40);
    assert_eq!(manifest.manifest_json["parameters"]["runner_target_r"], 8.0);
    assert_eq!(manifest.manifest_json["parameters"]["runner_fraction"], 0.2);
    assert_eq!(manifest.manifest_json["parameters"]["runner_stop_r"], 1.0);
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
        manifest.manifest_json["parameters"]["stop_loss_mode"],
        "fixed_pct"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["structure_stop_min_pct"],
        0.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["runner_target_r"],
        Value::Null
    );
    assert_eq!(manifest.manifest_json["parameters"]["runner_fraction"], 0.0);
    assert_eq!(manifest.manifest_json["parameters"]["runner_stop_r"], 0.0);
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
fn paper_observation_reclaim_retest1_pullback3_04sl_20r_delta20_40_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.04SL 2.0R reclaim retest1 dist3 pullback3 vol11 delta20-40 pchg5-10 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1"
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
    assert_eq!(
        manifest.manifest_json["parameters"]["stop_loss_mode"],
        "fixed_pct"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["structure_stop_min_pct"],
        0.0
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_min_wait_candles"],
        1
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_kline15m_breakout_fvg20_04sl_10r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_10r_kline15m_breakout_fvg20_vol13_dd35_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 15m kline 0.04SL 1.0R breakout fvg20 vol13 dd35 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "kline_15m"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["trend_timeframe"],
        "off"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.3
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        20.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_kline15m_breakout_fvg20_04sl_06r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_06r_kline15m_breakout_fvg20_vol13_dd35_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 15m kline 0.04SL 0.6R breakout fvg20 vol13 dd35 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "kline_15m"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["trend_timeframe"],
        "off"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 0.6);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.3
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        20.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_kline15m_breakout_fvg30_04sl_05r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_05r_kline15m_breakout_fvg30_vol13_dd35_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 15m kline 0.04SL 0.5R breakout fvg30 vol13 dd35 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "kline_15m"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["trend_timeframe"],
        "off"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 0.5);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.3
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        30.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_kline15m_breakout_fvg30_04sl_055r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_055r_kline15m_breakout_fvg30_vol13_dd35_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 15m kline 0.04SL 0.55R breakout fvg30 vol13 dd35 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "kline_15m"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["trend_timeframe"],
        "off"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 0.55);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.3
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        30.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_kline15m_breakout_fvg50_04sl_052r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 15m kline 0.04SL 0.52R breakout fvg50 vol13 dd35 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "kline_15m"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["trend_timeframe"],
        "off"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.04);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 0.52);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.3
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fvg_impulse_retrace_fill_pct"],
        50.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_breakout_reclaim_10r_0375sl_delta11_72_dist14_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_10r_breakout_reclaim_delta11_72_pchg4_12_dist14_vol11_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 1.0R breakout reclaim delta11-72 pchg4-12 dist14 vol11 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["stop_loss_pct"],
        0.0375
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_distance_pct"],
        14.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.1
    );
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 11);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 72);
    assert_eq!(
        manifest.manifest_json["parameters"]["min_price_change_pct"],
        4.0
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
fn paper_observation_breakout_reclaim_ma_10r_0375sl_delta11_72_dist14_ignore_preset_manifest_is_canonical_and_hashable(
) {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_0375sl_10r_breakout_reclaim_ma_delta11_72_pchg4_12_dist14_vol11_ignore_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity 0.0375SL 1.0R breakout reclaim_ma ignore delta11-72 pchg4-12 dist14 vol11 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["stop_loss_pct"],
        0.0375
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_distance_pct"],
        14.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.1
    );
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 11);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 72);
    assert_eq!(
        manifest.manifest_json["parameters"]["min_price_change_pct"],
        4.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        12.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["ignore_entry_signal_updates_while_open"],
        true
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high", "reclaim_ema", "reclaim_ma"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_short_15m_support_breakdown_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-breakdown-short");
    assert_eq!(manifest.strategy_key, "market_velocity_breakdown_short");
    assert_eq!(
        manifest.human_label,
        "Market Velocity short 0.0375SL 1.0R 15m support breakdown delta5-72 pchg1.5-12 vol13 v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "raw_state"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["trade_direction"],
        "short"
    );
    assert_eq!(
        manifest.manifest_json["strategy_key"],
        "market_velocity_breakdown_short"
    );
    assert_eq!(
        manifest.manifest_json["strategy_family"],
        "market_velocity_breakdown_short"
    );
    assert_eq!(
        manifest.manifest_json["product"]["slug"],
        "market-velocity-breakdown-short"
    );
    assert_eq!(
        manifest.manifest_json["execution"]["source_signal_type"],
        "market_velocity_breakdown_short"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["stop_loss_pct"],
        0.0375
    );
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 1.0);
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_max_distance_pct"],
        8.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["entry_min_volume_ratio"],
        1.3
    );
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 5);
    assert_eq!(manifest.manifest_json["parameters"]["max_delta_rank"], 72);
    assert_eq!(
        manifest.manifest_json["parameters"]["min_price_change_pct"],
        1.5
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["max_price_change_pct"],
        12.0
    );
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!(["breakdown_range_low"])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}

#[test]
fn paper_observation_episode_05sl_20r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_episode_momentum_05sl_20r_rank5_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity episode 0.05SL 2.0R rank5 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_episode_momentum_05sl_20r_rank5_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "episodes"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.05);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 2.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 5);
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!([])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}
#[test]
fn paper_observation_episode_05sl_30r_preset_manifest_is_canonical_and_hashable() {
    let manifest = market_velocity_paper_strategy_preset_manifest(
        "research_episode_momentum_05sl_30r_rank5_v1",
    )
    .unwrap();

    assert_eq!(manifest.product_slug, "market-velocity-radar");
    assert_eq!(
        manifest.human_label,
        "Market Velocity episode 0.05SL 3.0R rank5 v1"
    );
    assert_eq!(
        manifest.manifest_json["preset"],
        "research_episode_momentum_05sl_30r_rank5_v1"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["event_source"],
        "episodes"
    );
    assert_eq!(manifest.manifest_json["parameters"]["stop_loss_pct"], 0.05);
    assert_eq!(manifest.manifest_json["parameters"]["target_r"], 3.0);
    assert_eq!(manifest.manifest_json["parameters"]["min_delta_rank"], 5);
    assert_eq!(
        manifest.manifest_json["filters"]["entry_trigger_allowlist"],
        serde_json::json!([])
    );
    assert!(manifest.manifest_hash.starts_with("sha256:"));
    assert_eq!(manifest.manifest_hash.len(), "sha256:".len() + 64);
}
