//! V20 组合合同仅放宽突破距离到 2.00 ATR 的 V21 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V21 独立候选身份，不覆盖 V20 的 2.50 ATR 组合结果。
pub const V21_CANDIDATE_KEY: &str =
    "market_momentum_ema576_cycle_fresh_breakout_200atr_acceptance8_ema144_retest_structural_stop_cost_cap050r_15m_v21";
/// V21 L1 只把组合突破距离从 2.50 ATR 放宽到 2.00 ATR。
pub const V21_L1_RULE_VERSION: &str = "l1_v21_v20_breakout200atr_only_composite_no_outcome_v1";
/// V21 L2 保留 V20 的接受、成交与风控合同。
pub const V21_L2_RULE_VERSION: &str =
    "l2_v21_v20_breakout200atr_ema144_retest_structural030_net200_cost050_v1";

const TARGETS: [TargetSample; 2] = [
    TargetSample {
        name: "ada_2026_07_19_stale_weak_short",
        symbol: "ADA-USDT-SWAP",
        direction: "short",
        signal_ts_ms: 1_784_394_900_000,
    },
    TargetSample {
        name: "ont_2026_07_19_weak_breakout_long",
        symbol: "ONT-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_784_425_500_000,
    },
];

/// 执行 V21 无标签覆盖，并仅在预注册门禁通过时附带一次冻结 L2。
pub async fn run_v21_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V21_CANDIDATE_KEY,
            l1_rule_version: V21_L1_RULE_VERSION,
            l2_rule_version: V21_L2_RULE_VERSION,
            machine_schema_version:
                "market_momentum_ema576_composite_breakout_quality_200atr_l1_l2_v21",
            l1_schema_version:
                "market_momentum_ema576_composite_breakout_quality_200atr_l1_v21",
            l2_schema_version:
                "market_momentum_ema576_composite_breakout_quality_200atr_l2_v21",
            only_variable: "relative to V20, relax only breakout confirmation close distance from 2.50 to 2.00 ATR14; qualification cycle, eight-close EMA144 early-retest veto, entry, stop, target, costs, and conflicts remain frozen",
            setup_consumption_policy: "a relation-cycle break consumes historical qualification; a distance or eight-close failure consumes the current breakout episode without reviving stale qualification",
            causal_field_boundary: "setup, breakout, and signal timestamps plus completed EMA144, EMA576, ATR14, and OHLC visible no later than the signal close",
            entry_policy: "unchanged V16 next-contiguous-open execution and 0.50R stop-cost gate after the V21 composite conditions pass",
            rule: QualityRule::CompositeCycleDistance2_0Acceptance,
            min_affected_ratio_pct: 93.5,
            max_affected_ratio_pct: 96.3,
            target_samples: &TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
