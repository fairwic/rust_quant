use super::super::{ComputedCandle, MarketVelocityEventBacktestArgs};
use super::weekly_p90_anchor_rsi_trend_managed_v12::signal_with_countertrend_target;
use super::FilteredVolumeRsiEmaMacdSignal;

/// V13 单变量消融把普通逆势交易的冻结目标提高到 1.5 ATR。
pub(crate) const COUNTERTREND_TARGET_ATR_MULTIPLIER: f64 = 1.5;

/// 完整复用 V12 入场、趋势和保护规则，仅替换普通逆势交易的目标距离。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_countertrend_target(
        candles,
        completed_count,
        args,
        COUNTERTREND_TARGET_ATR_MULTIPLIER,
        "countertrend_one_point_five_atr",
    )
}
