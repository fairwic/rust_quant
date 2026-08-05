//! 用户明确指定的资格周期、2.50 ATR 与连续八收盘组合 V20 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V20 独立组合候选身份，不覆盖 V17～V19 的单项研究结果。
pub const V20_CANDIDATE_KEY: &str =
    "market_momentum_ema576_cycle_fresh_breakout_250atr_acceptance8_structural_stop_cost_cap050r_15m_v20";
/// V20 L1 同时执行三个用户冻结的信号时门禁。
pub const V20_L1_RULE_VERSION: &str =
    "l1_v20_v16_cycle_fresh_breakout250atr_acceptance8_composite_no_outcome_v1";
/// V20 L2 保留 V16 风控，只消费三个门禁的交集候选。
pub const V20_L2_RULE_VERSION: &str =
    "l2_v20_v16_cycle_fresh_breakout250atr_acceptance8_structural030_net200_cost050_v1";

const TARGETS: [TargetSample; 2] = [
    TargetSample {
        name: "ada_2026_07_19_stale_weak_short",
        symbol: "ADA-USDT-SWAP",
        direction: "short",
        signal_ts_ms: 1_784_394_900_000,
    },
    TargetSample {
        name: "ont_2026_07_19_weak_short_acceptance_long",
        symbol: "ONT-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_784_425_500_000,
    },
];

/// 执行 V20 组合 L1，并只在冻结覆盖门禁通过时附带一次 L2。
pub async fn run_v20_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V20_CANDIDATE_KEY,
            l1_rule_version: V20_L1_RULE_VERSION,
            l2_rule_version: V20_L2_RULE_VERSION,
            machine_schema_version: "market_momentum_ema576_composite_breakout_quality_l1_l2_v20",
            l1_schema_version: "market_momentum_ema576_composite_breakout_quality_l1_v20",
            l2_schema_version: "market_momentum_ema576_composite_breakout_quality_l2_v20",
            only_variable: "user-directed fixed composite contract: qualification relation cycle remains intact, breakout confirmation close distance is at least 2.50 ATR14, and eight breakout-side closes complete before the EMA144 retest",
            setup_consumption_policy: "an EMA144/576 relation-cycle break consumes the historical qualification; a distance or eight-close failure consumes the current breakout episode without reviving stale qualification",
            causal_field_boundary: "setup, breakout, and signal timestamps plus completed EMA144, EMA576, ATR14, and OHLC visible no later than the signal close",
            entry_policy: "unchanged V16 next-contiguous-open execution and 0.50R stop-cost gate after all three composite conditions pass",
            rule: QualityRule::CompositeCycleDistanceAcceptance,
            min_affected_ratio_pct: 93.5,
            max_affected_ratio_pct: 99.9,
            target_samples: &TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
