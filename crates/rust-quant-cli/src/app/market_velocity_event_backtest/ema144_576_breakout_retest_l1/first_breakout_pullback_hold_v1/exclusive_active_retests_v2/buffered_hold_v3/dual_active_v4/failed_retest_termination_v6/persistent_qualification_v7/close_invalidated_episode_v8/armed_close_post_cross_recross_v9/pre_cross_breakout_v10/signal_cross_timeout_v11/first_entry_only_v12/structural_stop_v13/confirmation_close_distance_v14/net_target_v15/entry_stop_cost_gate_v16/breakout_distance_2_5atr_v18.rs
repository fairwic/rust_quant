//! EMA576 突破确认收盘至少离开 2.50 ATR14 的独立 V18 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V18 独立候选身份。
pub const V18_CANDIDATE_KEY: &str =
    "market_momentum_ema576_breakout_confirmation_distance_250atr_structural_stop_cost_cap050r_15m_v18";
/// V18 L1 只增加突破确认收盘的 2.50 ATR 距离。
pub const V18_L1_RULE_VERSION: &str =
    "l1_v18_v16_breakout_confirmation_close_distance_min250atr_no_outcome_v1";
/// V18 L2 保留全部 V16 风控合同。
pub const V18_L2_RULE_VERSION: &str =
    "l2_v18_v16_breakout_distance250atr_structural030_net200_cost050_v1";

const BREAKOUT_TARGETS: [TargetSample; 2] = [
    TargetSample {
        name: "ont_2026_07_19_weak_breakout",
        symbol: "ONT-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_784_425_500_000,
    },
    TargetSample {
        name: "ada_2026_07_19_weak_breakdown",
        symbol: "ADA-USDT-SWAP",
        direction: "short",
        signal_ts_ms: 1_784_394_900_000,
    },
];

/// 执行 V18 L1，并只在无标签门禁通过时附带一次冻结 L2。
pub async fn run_v18_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V18_CANDIDATE_KEY,
            l1_rule_version: V18_L1_RULE_VERSION,
            l2_rule_version: V18_L2_RULE_VERSION,
            machine_schema_version: "market_momentum_ema576_breakout_distance_250atr_l1_l2_v18",
            l1_schema_version: "market_momentum_ema576_breakout_distance_250atr_l1_v18",
            l2_schema_version: "market_momentum_ema576_breakout_distance_250atr_l2_v18",
            only_variable: "require the second completed EMA576 breakout close to be at least 2.50 ATR14 beyond EMA576 in trade direction",
            setup_consumption_policy: "a sub-2.50ATR two-close breakout does not create a valid episode; a later independent two-close breakout may qualify",
            causal_field_boundary: "breakout confirmation close, same-bar EMA576 and same-bar Wilder ATR14 only; V16 entry risk fields remain frozen",
            entry_policy: "unchanged V16 next-contiguous-open execution and 0.50R cost gate after weak-distance breakouts are removed",
            rule: QualityRule::BreakoutDistance2_5Atr,
            min_affected_ratio_pct: 70.0,
            max_affected_ratio_pct: 98.0,
            target_samples: &BREAKOUT_TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
