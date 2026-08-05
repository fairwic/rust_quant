//! EMA576 突破先完成连续八收盘接受的独立 V19 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V19 独立候选身份。
pub const V19_CANDIDATE_KEY: &str =
    "market_momentum_ema576_breakout_acceptance_8close_structural_stop_cost_cap050r_15m_v19";
/// V19 L1 只增加突破侧连续八收盘接受。
pub const V19_L1_RULE_VERSION: &str =
    "l1_v19_v16_breakout_side_8_completed_closes_before_retest_no_outcome_v1";
/// V19 L2 保留全部 V16 风控合同。
pub const V19_L2_RULE_VERSION: &str =
    "l2_v19_v16_breakout_acceptance8_structural030_net200_cost050_v1";

const ACCEPTANCE_TARGETS: [TargetSample; 2] = [
    TargetSample {
        name: "ont_2026_07_19_only_6_breakout_side_closes",
        symbol: "ONT-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_784_425_500_000,
    },
    TargetSample {
        name: "ada_2026_07_19_only_2_breakout_side_closes",
        symbol: "ADA-USDT-SWAP",
        direction: "short",
        signal_ts_ms: 1_784_394_900_000,
    },
];

/// 执行 V19 L1，并只在无标签门禁通过时附带一次冻结 L2。
pub async fn run_v19_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V19_CANDIDATE_KEY,
            l1_rule_version: V19_L1_RULE_VERSION,
            l2_rule_version: V19_L2_RULE_VERSION,
            machine_schema_version: "market_momentum_ema576_breakout_acceptance_8bar_l1_l2_v19",
            l1_schema_version: "market_momentum_ema576_breakout_acceptance_8bar_l1_v19",
            l2_schema_version: "market_momentum_ema576_breakout_acceptance_8bar_l2_v19",
            only_variable: "require eight consecutive completed closes on the EMA576 breakout side before any EMA144 retest may signal",
            setup_consumption_policy: "a return across EMA576 or EMA144 retest before the eighth accepted close invalidates that breakout episode; a new breakout is required",
            causal_field_boundary: "only completed bars from the first breakout close through the signal-exclusive acceptance window; V16 entry risk fields remain frozen",
            entry_policy: "unchanged V16 next-contiguous-open execution and 0.50R cost gate after non-accepted breakout episodes are removed",
            rule: QualityRule::BreakoutAcceptance8Bars,
            min_affected_ratio_pct: 50.0,
            max_affected_ratio_pct: 95.0,
            target_samples: &ACCEPTANCE_TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
