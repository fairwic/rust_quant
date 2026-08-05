use super::*;

/// 入场构造所需的同棒候选集合；只携带信号时已知信息，禁止读取下一根开盘。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct IntentContext {
    pub(super) patterns: CandlePatterns,
    pub(super) divergence: Divergence,
    pub(super) rsi_pattern_long: bool,
    pub(super) rsi_pattern_short: bool,
    pub(super) ema_long: bool,
    pub(super) ema_trend_long_v6: Option<EmaTrendLongAcceptanceV6>,
    pub(super) ema_short: Option<EmaShortSignal>,
    pub(super) three_bar_long: bool,
    pub(super) bollinger_lower_reclaim: Option<BollingerLowerReclaimLongResult>,
    pub(super) ema596_reclaim_departure: Option<Ema596ReclaimDepartureLongResult>,
    pub(super) accepted_range: Option<AcceptedRangeSignal>,
    pub(super) large_horizontal_line: Option<f64>,
    pub(super) strict_visual_breakout: Option<StrictVisualBreakoutSignal>,
    pub(super) large_triangle_line: Option<f64>,
    pub(super) false_breakout: Option<FalseBreakoutSignal>,
    pub(super) upthrust_failed_acceptance: Option<UpthrustFailedAcceptanceSignal>,
    pub(super) transition_sweep: Option<TransitionSweepSignal>,
    pub(super) ema_expansion_long: bool,
    pub(super) ema_expansion_short: bool,
    pub(super) effort_no_result: Option<EffortNoResultShortResult>,
    pub(super) divergence_reversal_long: bool,
    pub(super) divergence_reversal_short: bool,
    pub(super) short_trend_extension: bool,
    pub(super) counter_trend_long: bool,
    pub(super) counter_trend_short: bool,
    pub(super) long_structure_target: Option<f64>,
    pub(super) short_structure_target: Option<f64>,
    /// V4/V5 纯 RSI 严格逆势共用的只读年龄；不得参与 V4 的信号、目标或退出判断。
    pub(super) counter_trend_ema_age_audit: Option<usize>,
    pub(super) v5_counter_trend_plan: Option<RsiCounterTrendPlanV5>,
    pub(super) take_profit_atr: Option<f64>,
}

impl IntentContext {
    /// 返回信号时已冻结的视觉横盘高度，避免退出研究重新读取或推导后续区间。
    pub(super) fn strict_visual_range_height(&self) -> Option<f64> {
        self.strict_visual_breakout
            .map(|signal| signal.range.upper - signal.range.lower)
    }
}
