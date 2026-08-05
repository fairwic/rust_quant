//! 基于 V22 增加“EMA144/576 原关系保持到信号”的 V23 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V23 独立候选标识，避免覆盖冻结的 V22 行为。
pub const V23_CANDIDATE_KEY: &str =
    "market_momentum_ema576_relation_intact_until_signal_200atr_acceptance8_ema576_intrabar_hold_structural_stop_cost_cap050r_15m_v23";
/// L1 只读信号时可见字段的规则版本。
pub const V23_L1_RULE_VERSION: &str = "l1_v23_v22_relation_cycle_intact_until_signal_no_outcome_v1";
/// 仅当 L1 覆盖门禁通过时才允许使用的 L2 回放版本。
pub const V23_L2_RULE_VERSION: &str =
    "l2_v23_v22_relation_cycle_intact_until_signal_structural030_net200_cost050_v1";

const TARGETS: [TargetSample; 3] = [
    TargetSample {
        name: "woo_2026_07_16_post_cross_long",
        symbol: "WOO-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_784_143_800_000,
    },
    TargetSample {
        name: "ltc_2026_07_14_post_cross_short",
        symbol: "LTC-USDT-SWAP",
        direction: "short",
        signal_ts_ms: 1_783_987_200_000,
    },
    TargetSample {
        name: "act_2026_07_11_post_cross_long",
        symbol: "ACT-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_783_736_100_000,
    },
];

/// 运行 V23 L1，且只在无 outcome 覆盖门禁通过后继续 L2。
pub async fn run_v23_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V23_CANDIDATE_KEY,
            l1_rule_version: V23_L1_RULE_VERSION,
            l2_rule_version: V23_L2_RULE_VERSION,
            machine_schema_version:
                "market_momentum_ema576_relation_reset_before_signal_l1_l2_v23",
            l1_schema_version: "market_momentum_ema576_relation_reset_before_signal_l1_v23",
            l2_schema_version: "market_momentum_ema576_relation_reset_before_signal_l2_v23",
            only_variable: "relative to V22, extend the original EMA144/EMA576 qualification relation from breakout confirmation through the retest signal close; any cross consumes the old qualification and breakout episode",
            setup_consumption_policy: "the first EMA144/EMA576 relation cross before signal consumes the old qualification and breakout episode; a later candidate must earn a new long-term qualification in the new relation cycle",
            causal_field_boundary: "completed EMA144 and EMA576 values from setup through signal plus the unchanged V22 breakout distance, eight-close acceptance, and EMA576 intrabar hold fields",
            entry_policy: "unchanged V22/V16 next-contiguous-open execution and 0.50R stop-cost gate, but only when the original EMA relation remains intact through signal close",
            rule: QualityRule::CompositeCycleDistance2_0AcceptanceEma576HoldRelationUntilSignal,
            min_affected_ratio_pct: 98.0,
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
