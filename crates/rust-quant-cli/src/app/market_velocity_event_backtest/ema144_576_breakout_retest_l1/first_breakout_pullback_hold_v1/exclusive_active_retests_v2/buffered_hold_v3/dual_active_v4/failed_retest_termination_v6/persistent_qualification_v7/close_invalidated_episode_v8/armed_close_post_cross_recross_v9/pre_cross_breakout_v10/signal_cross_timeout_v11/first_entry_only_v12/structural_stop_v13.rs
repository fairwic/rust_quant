//! V12 首笔成交生命周期下，使用 EMA144 ATR 缓冲结构止损的 L2 诊断。
//!
//! 初始止损只读取信号 K 已完成的 EMA144/ATR14，并在入场时永久冻结；后续均线变化
//! 不能放宽风险。本版保持 0.52R、24 小时、成本和冲突顺序不变。

pub mod confirmation_close_distance_v14;

use super::*;

/// V13 独立研究身份；结构止损不能覆盖固定 4% 的 V12。
pub const V13_CANDIDATE_KEY: &str =
    "market_momentum_ema576_first_entry_ema144_structural_stop_15m_v13";
/// V13 精确风险版本；`030` 表示信号 EMA144 外侧 0.30 ATR14。
pub const V13_L2_RULE_VERSION: &str =
    "l2_v13_v12_first_filled_signal_ema144_buffer030_r052_hold24h_cost8bps_v1";

const STRUCTURAL_STOP_BUFFER_ATR: f64 = 0.30;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;

/// 执行 V13 EMA144 结构止损回放，并保持 V12 首笔真实成交限制。
pub async fn run_v13_l2_replay(l1_source: &Path, output: &Path) -> Result<V10L2Report> {
    let report = replay_v11_variant(
        l1_source,
        "market_momentum_ema576_first_entry_ema144_structural_stop_l2_v13",
        V10L2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: V13_CANDIDATE_KEY,
            source_l1_rule_version: V11_RULE_VERSION,
            rule_version: V13_L2_RULE_VERSION,
            only_variable: "replace V12 fixed 4 percent initial stop with the signal-time EMA144 plus or minus a mirrored 0.30 ATR14 structural invalidation stop",
            entry_policy: "unchanged V12 next-contiguous-15m-open entry and first real fill per symbol x direction x long-term qualification setup",
            initial_stop_policy: "freeze long stop at signal EMA144 minus 0.30 ATR14 and short stop at signal EMA144 plus 0.30 ATR14; block entries already beyond the structural stop; never loosen after entry",
            target_policy: "unchanged fixed 0.52R from actual entry, where R is the actual entry-to-structural-stop distance; no break-even, trailing, partial, runner, or reversal",
            intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
            symbol_position_policy: "unchanged V12 one open trade per symbol and one filled trade per symbol x direction x setup_ts",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            funding_modeled: false,
            outcome_evaluation_performed: true,
            runtime_boundary: "research-only V13 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        SetupEntryPolicy::FirstFilledPerSetup,
        InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR),
    )
    .await?;
    write_report(output, &report, "V13")?;
    Ok(report)
}
