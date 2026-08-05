//! EMA144/576 关系周期结束即消费旧资格的独立 V17 研究入口。

use super::breakout_entry_quality_common::{
    run_quality_research, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V17 独立候选身份。
pub const V17_CANDIDATE_KEY: &str =
    "market_momentum_ema576_qualification_cycle_fresh_structural_stop_cost_cap050r_15m_v17";
/// V17 L1 只改变旧资格跨关系周期复用。
pub const V17_L1_RULE_VERSION: &str =
    "l1_v17_v16_setup_relation_cycle_must_remain_intact_no_outcome_v1";
/// V17 L2 保留 V16 风控，只消费跨周期旧资格。
pub const V17_L2_RULE_VERSION: &str =
    "l2_v17_v16_setup_relation_cycle_fresh_structural030_net200_cost050_v1";

const ADA_TARGETS: [TargetSample; 1] = [TargetSample {
    name: "ada_2026_07_19_stale_short_qualification",
    symbol: "ADA-USDT-SWAP",
    direction: "short",
    signal_ts_ms: 1_784_394_900_000,
}];

/// 执行 V17 L1，并只在无标签门禁通过时附带一次冻结 L2。
pub async fn run_v17_l1_l2_replay(
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<()> {
    run_quality_research(
        QualitySpec {
            candidate_key: V17_CANDIDATE_KEY,
            l1_rule_version: V17_L1_RULE_VERSION,
            l2_rule_version: V17_L2_RULE_VERSION,
            machine_schema_version: "market_momentum_ema576_qualification_cycle_reset_l1_l2_v17",
            l1_schema_version: "market_momentum_ema576_qualification_cycle_reset_l1_v17",
            l2_schema_version: "market_momentum_ema576_qualification_cycle_reset_l2_v17",
            only_variable: "invalidate a historical qualification as soon as any completed bar leaves the EMA144/576 relation cycle that originally created it",
            setup_consumption_policy: "a relation-cycle break consumes the historical qualification; only a newly completed 144-bar qualification may support a later breakout",
            causal_field_boundary: "setup and breakout timestamps plus completed EMA144/576 values between them; V16 entry risk fields remain frozen",
            entry_policy: "unchanged V16 next-contiguous-open execution and 0.50R cost gate after stale qualification candidates are removed",
            rule: QualityRule::QualificationCycleFresh,
            min_affected_ratio_pct: 10.0,
            max_affected_ratio_pct: 60.0,
            target_samples: &ADA_TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
