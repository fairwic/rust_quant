//! 在 V24 上只把 EMA576 接受与顺势极值窗口从八根缩短为六根的 V25 L1 入口。

use super::breakout_entry_quality_common::{
    run_quality_l1_only, QualityRule, QualitySpec, TargetSample,
};
use anyhow::Result;
use std::path::Path;

/// V25 独立候选标识，禁止覆盖 V24 的八根窗口行为。
pub const V25_CANDIDATE_KEY: &str =
    "market_momentum_ema576_relation_intact_until_signal_acceptance_window_extreme_200atr_acceptance6_ema576_intrabar_hold_structural_stop_cost_cap050r_15m_v25";
/// V25 L1 只读取第一根越线收盘起六根完成 K 的因果字段。
pub const V25_L1_RULE_VERSION: &str =
    "l1_v25_v24_acceptance_window_six_closes_directional_extreme_200atr_no_outcome_v1";

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

/// 运行 V25 无 outcome L1；本入口不读取止损、退出或收益结果。
pub async fn run_v25_l1(v14_source: &Path, v16_source: &Path, output: &Path) -> Result<()> {
    run_quality_l1_only(
        QualitySpec {
            candidate_key: V25_CANDIDATE_KEY,
            l1_rule_version: V25_L1_RULE_VERSION,
            l2_rule_version: "l2_v25_not_registered",
            machine_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_six_close_l1_v25",
            l1_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_six_close_l1_v25",
            l2_schema_version:
                "market_momentum_ema576_acceptance_window_extreme_2_0atr_six_close_l2_v25_not_run",
            only_variable: "relative to V24, shorten both the required same-side EMA576 closes and the maximum directional 2.00 ATR extreme window from the first eight breakout closes to the first six breakout closes",
            setup_consumption_policy: "unchanged V24: the first EMA144/EMA576 relation cross before signal consumes the old qualification and breakout episode; a later candidate must earn a new long-term qualification",
            causal_field_boundary: "completed close, high/low, EMA576, and ATR14 values from the first crossing close through the sixth close plus the unchanged V24 relation-cycle, entry, and risk fields",
            entry_policy: "unchanged V24/V16 next-contiguous-open execution and 0.50R stop-cost gate; L2 is not executed by this L1-only entry",
            rule: QualityRule::CompositeCycleAcceptanceWindowExtreme2_0SixCloseEma576HoldRelationUntilSignal,
            min_affected_ratio_pct: 82.0,
            max_affected_ratio_pct: 94.0,
            target_samples: &TARGETS,
        },
        v14_source,
        v16_source,
        output,
    )
    .await
    .map(|_| ())
}
