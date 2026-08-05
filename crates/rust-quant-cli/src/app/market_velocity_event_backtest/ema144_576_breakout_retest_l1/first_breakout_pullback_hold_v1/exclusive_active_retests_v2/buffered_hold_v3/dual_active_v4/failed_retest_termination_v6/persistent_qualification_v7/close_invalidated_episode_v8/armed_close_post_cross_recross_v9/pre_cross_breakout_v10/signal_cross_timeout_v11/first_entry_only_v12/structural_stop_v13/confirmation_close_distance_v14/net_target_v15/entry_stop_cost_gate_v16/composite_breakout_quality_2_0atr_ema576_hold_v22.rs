//! V21 组合仅替换八根接受提前失效边界的 V22 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V22 独立候选身份，保留 V21 的 2.00 ATR 距离合同。
pub const V22_CANDIDATE_KEY: &str =
    "market_momentum_ema576_cycle_fresh_breakout_200atr_acceptance8_ema576_intrabar_hold_structural_stop_cost_cap050r_15m_v22";
/// V22 L1 只把提前回踩边界从 EMA144 区替换为 EMA576 盘中严格穿越。
pub const V22_L1_RULE_VERSION: &str =
    "l1_v22_v21_acceptance8_ema576_intrabar_hold_only_no_outcome_v1";
/// V22 L2 保留 V21 的成交、结构止损、净目标与成本门禁。
pub const V22_L2_RULE_VERSION: &str =
    "l2_v22_v21_acceptance8_ema576_intrabar_hold_structural030_net200_cost050_v1";

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

/// 执行 V22 无标签覆盖，并仅在预注册门禁通过时附带一次冻结 L2。
pub async fn run_v22_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V22_CANDIDATE_KEY,
            l1_rule_version: V22_L1_RULE_VERSION,
            l2_rule_version: V22_L2_RULE_VERSION,
            machine_schema_version:
                "market_momentum_ema576_composite_breakout_quality_200atr_ema576_hold_l1_l2_v22",
            l1_schema_version:
                "market_momentum_ema576_composite_breakout_quality_200atr_ema576_hold_l1_v22",
            l2_schema_version:
                "market_momentum_ema576_composite_breakout_quality_200atr_ema576_hold_l2_v22",
            only_variable: "relative to V21, replace only the pre-acceptance EMA144 plus or minus 0.30 ATR retest veto with strict intrabar crossing of EMA576; 2.00 ATR distance, eight closes, entry, stop, target, costs, and conflicts remain frozen",
            setup_consumption_policy: "a relation-cycle break consumes historical qualification; an EMA576 hold, distance, or close-acceptance failure consumes only the current breakout episode",
            causal_field_boundary: "setup, breakout, and signal timestamps plus completed EMA144, EMA576, ATR14, and OHLC visible no later than the signal close",
            entry_policy: "unchanged V16 next-contiguous-open execution and 0.50R stop-cost gate after the V22 composite conditions pass",
            rule: QualityRule::CompositeCycleDistance2_0AcceptanceEma576Hold,
            min_affected_ratio_pct: 92.0,
            max_affected_ratio_pct: 99.5,
            target_samples: &TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
