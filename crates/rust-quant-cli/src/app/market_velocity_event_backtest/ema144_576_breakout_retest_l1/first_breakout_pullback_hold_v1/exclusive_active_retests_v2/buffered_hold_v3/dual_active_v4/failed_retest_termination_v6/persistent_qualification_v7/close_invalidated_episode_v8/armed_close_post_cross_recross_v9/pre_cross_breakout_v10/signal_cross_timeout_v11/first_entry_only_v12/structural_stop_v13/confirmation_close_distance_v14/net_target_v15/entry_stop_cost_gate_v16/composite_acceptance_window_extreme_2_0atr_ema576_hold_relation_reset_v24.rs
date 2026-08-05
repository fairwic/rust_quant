//! 基于 V23 把固定确认棒收盘距离改为八根接受窗口顺势极值的 V24 L1 入口。

use super::breakout_entry_quality_common::{
    run_frozen_quality_l2, run_quality_l1_only, FrozenQualityL2Spec, QualityRule, QualitySpec,
    TargetSample,
};
use super::V10L2Report;
use anyhow::Result;
use std::path::Path;

/// V24 独立候选标识，冻结 V23 的第二根确认收盘距离行为。
pub const V24_CANDIDATE_KEY: &str =
    "market_momentum_ema576_relation_intact_until_signal_acceptance_window_extreme_200atr_acceptance8_ema576_intrabar_hold_structural_stop_cost_cap050r_15m_v24";
/// V24 L1 只读取第 1～8 根完成 K 的顺势极值、EMA576 与 ATR14。
pub const V24_L1_RULE_VERSION: &str =
    "l1_v24_v23_acceptance_window_directional_extreme_200atr_no_outcome_v1";
/// V24 独立 L2 身份，只消费 SHA 固定的 715 个 L1 合格候选。
pub const V24_L2_RULE_VERSION: &str = "l2_v24_frozen715_next_open_structural030_net200_cost050_v1";

const EXPECTED_V24_L1_REPORT_SHA256: &str =
    "e0c3ed91ba38ca6782d2e45bef35461898ca888d08ab2de1daa93f28a329055d";
const EXPECTED_V24_L1_PAYLOAD_SHA256: &str =
    "c62d10e5cf841ac8c2bc88934b5a45ac7c1bfefb410f592da668b3c85578081d";
const EXPECTED_V24_CANDIDATE_SET_SHA256: &str =
    "484a8541113a3275c4bf2adb2ad1d79e5dbcc51d2b0b057100634250c29b3630";

const TARGETS: [TargetSample; 3] = [
    TargetSample {
        name: "woo_2026_07_16_post_cross_long_remains_invalid",
        symbol: "WOO-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_784_143_800_000,
    },
    TargetSample {
        name: "ltc_2026_07_14_post_cross_short_remains_invalid",
        symbol: "LTC-USDT-SWAP",
        direction: "short",
        signal_ts_ms: 1_783_987_200_000,
    },
    TargetSample {
        name: "act_2026_07_11_post_cross_long_remains_invalid",
        symbol: "ACT-USDT-SWAP",
        direction: "long",
        signal_ts_ms: 1_783_736_100_000,
    },
];

/// 运行 V24 无 outcome L1；覆盖通过后仍需另行预注册 L2。
pub async fn run_v24_l1(v14_source: &Path, v16_source: &Path, output: &Path) -> Result<()> {
    run_quality_l1_only(
        QualitySpec {
            candidate_key: V24_CANDIDATE_KEY,
            l1_rule_version: V24_L1_RULE_VERSION,
            l2_rule_version: V24_L2_RULE_VERSION,
            machine_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_l1_v24",
            l1_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_l1_v24",
            l2_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_l2_v24_not_run",
            only_variable: "relative to V23, replace only the second confirmation candle close distance from EMA576 with the maximum same-candle directional high/low distance across the first eight breakout-side closes; the 2.00 ATR threshold is unchanged",
            setup_consumption_policy: "unchanged V23: the first EMA144/EMA576 relation cross before signal consumes the old qualification and breakout episode; a later candidate must earn a new long-term qualification",
            causal_field_boundary: "completed high/low, EMA576, and ATR14 values from the first crossing close through the eighth close plus the unchanged V23 relation-cycle, acceptance, intrabar-hold, entry, and risk fields",
            entry_policy: "unchanged V23/V16 next-contiguous-open execution and 0.50R stop-cost gate; L2 is not executed by this L1-only entry",
            rule: QualityRule::CompositeCycleAcceptanceWindowExtreme2_0Ema576HoldRelationUntilSignal,
            min_affected_ratio_pct: 75.0,
            max_affected_ratio_pct: 98.5,
            target_samples: &TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}

/// 对冻结 V24 L1 的 715 个候选执行唯一一次成本后 L2 回放。
pub async fn run_v24_l2(
    l1_source: &Path,
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<V10L2Report> {
    run_frozen_quality_l2(
        FrozenQualityL2Spec {
            source_machine_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_l1_v24",
            source_l1_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_l1_v24",
            expected_l1_report_sha256: EXPECTED_V24_L1_REPORT_SHA256,
            expected_l1_payload_sha256: EXPECTED_V24_L1_PAYLOAD_SHA256,
            candidate_key: V24_CANDIDATE_KEY,
            source_l1_rule_version: V24_L1_RULE_VERSION,
            l2_rule_version: V24_L2_RULE_VERSION,
            l2_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_l2_v24",
            only_variable: "read the post-entry outcomes of the exact SHA-frozen 715 V24 L1 candidates under the unchanged V16 entry, structural-stop, net-2R target, cost, conflict, setup-lock, symbol-lock, and 24-hour contracts",
            entry_policy: "the next contiguous 15m open after the completed signal; a rejected or unresolved opportunity does not consume its setup, and only the first later qualifying real fill may consume it",
            expected_candidate_count: 715,
            expected_long_count: 470,
            expected_short_count: 245,
            expected_candidate_set_sha256: EXPECTED_V24_CANDIDATE_SET_SHA256,
        },
        l1_source,
        v14_source,
        v16_source,
        output,
    )
    .await
}
