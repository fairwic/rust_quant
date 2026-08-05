use super::super::model::{Candle, EmaShortResearchVariant, IndicatorPoint, IndicatorSeries};

const STRUCTURE_LOOKBACK: usize = 20;
const REGIME_SLOPE_LOOKBACK: usize = 20;
const SLOPE_LOOKBACK: usize = 3;
const MAX_DISTANCE_ATR: f64 = 0.8;
const EXTREME_VOLUME_RATIO: f64 = 10.0;
const RETEST_MIN_AGE: usize = 1;
const RETEST_MAX_AGE: usize = 3;
const RETEST_TOUCH_DISTANCE_ATR: f64 = 0.15;
const RETEST_INVALIDATION_ATR: f64 = 0.20;

/// EMA 空头家族生成订单时必须沿用的来源棒快照。
#[derive(Debug, Clone, Copy)]
pub(super) struct EmaShortSignal {
    /// 来源棒 ATR；延迟确认不能用后续波动重新缩放初始风险。
    pub(super) source_atr: f64,
    /// 来源棒量比决定的 ATR 目标档位。
    pub(super) source_take_profit_atr: f64,
    /// 来源棒过滤量比；延迟接受棒不需要再次放量。
    pub(super) source_volume_ratio: f64,
    /// 来源棒 RSI；`None` 仅表示指标数据缺失。
    pub(super) source_rsi: Option<f64>,
    /// `true` 表示信号来自后续回抽失败，而不是来源棒立即确认。
    pub(super) deferred: bool,
}

/// 右侧回抽期间冻结的来源事实；后续 K 线只能确认或取消，不能移动阈值。
#[derive(Debug, Clone, Copy)]
struct PendingRetest {
    /// 原始 EMA 空头来源棒位置，用于限制确认只能发生在后续 1～3 根。
    source_index: usize,
    /// 来源棒 EMA12；回抽阈值在等待期间保持冻结。
    source_ema12: f64,
    /// 来源棒订单参数，确认棒不能重新计算风险和目标。
    signal: EmaShortSignal,
}

/// 单变量消融状态；非右侧版本保持无状态并与 V19 同棒确认。
#[derive(Debug)]
pub(super) struct EmaShortAblationState {
    /// 本次回放唯一启用的单变量版本。
    variant: EmaShortResearchVariant,
    /// 仅右侧回抽版本使用的未决来源棒。
    pending_retest: Option<PendingRetest>,
}

impl Default for EmaShortAblationState {
    fn default() -> Self {
        Self::new(EmaShortResearchVariant::Baseline)
    }
}

impl EmaShortAblationState {
    /// 绑定一次回放的唯一消融变量，避免同一报告混入多个假设。
    pub(super) const fn new(variant: EmaShortResearchVariant) -> Self {
        Self {
            variant,
            pending_retest: None,
        }
    }

    /// 评估原始 EMA 空头来源或有限窗口回抽；只读取 `index` 及之前的已完成 K 线。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        baseline_source: bool,
        source_atr: f64,
        source_take_profit_atr: Option<f64>,
        source_enabled: bool,
    ) -> Option<EmaShortSignal> {
        if self.variant == EmaShortResearchVariant::RightSideRetest {
            return self.evaluate_retest(
                candles,
                indicators,
                index,
                baseline_source,
                source_atr,
                source_take_profit_atr,
                source_enabled,
            );
        }
        if !baseline_source {
            return None;
        }
        let source_take_profit_atr = source_take_profit_atr?;
        let point = indicators.get(index)?;
        let accepted = match self.variant {
            EmaShortResearchVariant::Baseline => true,
            EmaShortResearchVariant::SlopeSpread => {
                has_falling_expanding_ema_stack(indicators, index)
            }
            EmaShortResearchVariant::StructureBreak => {
                closes_below_prior_structure_by_atr(candles, index, source_atr, 0.0)
            }
            EmaShortResearchVariant::StructureBreakDepth10 => {
                closes_below_prior_structure_by_atr(candles, index, source_atr, 0.10)
            }
            EmaShortResearchVariant::StructureBreakDepth20 => {
                closes_below_prior_structure_by_atr(candles, index, source_atr, 0.20)
            }
            EmaShortResearchVariant::StructureBreakEma676Falling20 => {
                closes_below_prior_structure_by_atr(candles, index, source_atr, 0.0)
                    && ema676_is_falling(indicators, index)
            }
            EmaShortResearchVariant::DistanceGuard => {
                is_within_ema12_distance(point, candles[index].close, source_atr)
            }
            EmaShortResearchVariant::ExtremeVolumeAcceptance => {
                point.filtered_volume_ratio.is_some_and(|ratio| {
                    ratio < EXTREME_VOLUME_RATIO
                        || closes_below_prior_structure_by_atr(candles, index, source_atr, 0.0)
                })
            }
            EmaShortResearchVariant::RightSideRetest => unreachable!("handled above"),
        };
        accepted.then(|| source_signal(point, source_atr, source_take_profit_atr, false))
    }

    /// 推进冻结来源棒的 1～3 根确认窗口，并在接受、失效或到期时清理状态。
    #[allow(clippy::too_many_arguments)]
    fn evaluate_retest(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        baseline_source: bool,
        source_atr: f64,
        source_take_profit_atr: Option<f64>,
        source_enabled: bool,
    ) -> Option<EmaShortSignal> {
        let candle = candles[index];
        if let Some(pending) = self.pending_retest {
            let age = index.saturating_sub(pending.source_index);
            let accepted = (RETEST_MIN_AGE..=RETEST_MAX_AGE).contains(&age)
                && candle.high
                    >= pending.source_ema12 - RETEST_TOUCH_DISTANCE_ATR * pending.signal.source_atr
                && candle.close < pending.source_ema12
                && candle.close < candle.open;
            let invalidated = candle.close
                > pending.source_ema12 + RETEST_INVALIDATION_ATR * pending.signal.source_atr;
            if accepted {
                self.pending_retest = None;
                return Some(pending.signal);
            }
            if invalidated || age >= RETEST_MAX_AGE {
                self.pending_retest = None;
            } else {
                return None;
            }
        }

        if baseline_source && source_enabled {
            let point = indicators.get(index)?;
            self.pending_retest = Some(PendingRetest {
                source_index: index,
                source_ema12: point.ema12?,
                signal: source_signal(point, source_atr, source_take_profit_atr?, true),
            });
        }
        None
    }
}

/// 从来源棒冻结订单所需指标，防止延迟确认引入后续信息。
fn source_signal(
    point: &IndicatorPoint,
    source_atr: f64,
    source_take_profit_atr: f64,
    deferred: bool,
) -> EmaShortSignal {
    EmaShortSignal {
        source_atr,
        source_take_profit_atr,
        source_volume_ratio: point
            .filtered_volume_ratio
            .expect("EMA short source requires a filtered volume ratio"),
        source_rsi: point.rsi14,
        deferred,
    }
}

/// 要求当前与三根前均为空头排列，且四线下斜、三段间距同步扩大。
fn has_falling_expanding_ema_stack(indicators: &IndicatorSeries, index: usize) -> bool {
    let Some(previous_index) = index.checked_sub(SLOPE_LOOKBACK) else {
        return false;
    };
    let Some(current) = ema_stack(indicators.get(index)) else {
        return false;
    };
    let Some(previous) = ema_stack(indicators.get(previous_index)) else {
        return false;
    };
    current.is_bearish()
        && previous.is_bearish()
        && current.all_below(previous)
        && current.gaps_all_greater_than(previous)
}

/// 判断信号收盘是否至少向下突破前 20 根低点指定 ATR 深度。
///
/// `minimum_depth_atr = 0` 仍要求严格破位，用于保持上一轮 D0 基线逐笔不变。
fn closes_below_prior_structure_by_atr(
    candles: &[Candle],
    index: usize,
    atr: f64,
    minimum_depth_atr: f64,
) -> bool {
    let Some(start) = index.checked_sub(STRUCTURE_LOOKBACK) else {
        return false;
    };
    let prior_low = candles[start..index]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let break_distance = prior_low - candles[index].close;
    break_distance > 0.0
        && (minimum_depth_atr == 0.0 || (atr > 0.0 && break_distance / atr >= minimum_depth_atr))
}

/// 要求 V19 实际 EMA676 低于 20 根前；历史字段 `ema696` 只保留兼容命名。
fn ema676_is_falling(indicators: &IndicatorSeries, index: usize) -> bool {
    let Some(previous_index) = index.checked_sub(REGIME_SLOPE_LOOKBACK) else {
        return false;
    };
    indicators
        .get(index)
        .and_then(|point| point.ema696)
        .zip(
            indicators
                .get(previous_index)
                .and_then(|point| point.ema696),
        )
        .is_some_and(|(current, previous)| current < previous)
}

/// 仅接受 EMA12 下方 0～0.8 ATR 内的空头信号，拒绝已经延伸的追空。
fn is_within_ema12_distance(point: &IndicatorPoint, close: f64, atr: f64) -> bool {
    point
        .ema12
        .map(|ema12| (ema12 - close) / atr)
        .is_some_and(|distance| (0.0..=MAX_DISTANCE_ATR).contains(&distance))
}

#[derive(Debug, Clone, Copy)]
struct EmaStack {
    /// EMA12，代表短线价格反应。
    fast: f64,
    /// EMA144，代表中期趋势。
    medium: f64,
    /// EMA576，代表慢结构。
    structure: f64,
    /// EMA676，代表最慢市场状态。
    regime: f64,
}

impl EmaStack {
    /// 四线是否按快到慢严格空头排列。
    fn is_bearish(self) -> bool {
        self.fast < self.medium && self.medium < self.structure && self.structure < self.regime
    }

    /// 四条均线是否都低于三根前各自的位置。
    fn all_below(self, previous: Self) -> bool {
        self.fast < previous.fast
            && self.medium < previous.medium
            && self.structure < previous.structure
            && self.regime < previous.regime
    }

    /// 三段相邻 EMA 空头间距是否都比三根前扩大。
    fn gaps_all_greater_than(self, previous: Self) -> bool {
        self.medium - self.fast > previous.medium - previous.fast
            && self.structure - self.medium > previous.structure - previous.medium
            && self.regime - self.structure > previous.regime - previous.structure
    }
}

/// 只有四条 EMA 均已预热完成时才构造可比较的排列快照。
fn ema_stack(point: Option<&IndicatorPoint>) -> Option<EmaStack> {
    let point = point?;
    Some(EmaStack {
        fast: point.ema12?,
        medium: point.ema144?,
        structure: point.ema596?,
        regime: point.ema696?,
    })
}

/// 把过滤量比分档为 Pine 的 ATR 目标倍数；不足 3 倍不生成普通入场。
pub fn take_profit_tier(volume_ratio: f64) -> Option<f64> {
    if volume_ratio >= 6.0 {
        Some(4.5)
    } else if volume_ratio >= 4.0 {
        Some(3.6)
    } else if volume_ratio >= 3.0 {
        Some(2.7)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(index: usize, close: f64) -> Candle {
        Candle {
            timestamp_ms: index as i64,
            open: close + 1.0,
            high: close + 2.0,
            low: close - 2.0,
            close,
            volume: 10.0,
        }
    }

    fn point(fast: f64, medium: f64, structure: f64, regime: f64) -> IndicatorPoint {
        IndicatorPoint {
            filtered_volume_ratio: Some(6.0),
            volume_event: true,
            rsi14: Some(45.0),
            ema12: Some(fast),
            ema144: Some(medium),
            ema596: Some(structure),
            ema696: Some(regime),
            atr14: Some(10.0),
            ..IndicatorPoint::default()
        }
    }

    #[test]
    fn slope_spread_requires_all_four_emas_to_fall_and_every_gap_to_expand() {
        let mut points = vec![IndicatorPoint::default(); 4];
        points[0] = point(100.0, 102.0, 105.0, 109.0);
        points[3] = point(96.0, 99.0, 103.0, 108.0);
        let indicators = IndicatorSeries { points };
        assert!(has_falling_expanding_ema_stack(&indicators, 3));

        let mut rejected = indicators.clone();
        rejected.points[3].ema696 = Some(109.5);
        assert!(!has_falling_expanding_ema_stack(&rejected, 3));
    }

    #[test]
    fn structure_break_excludes_the_signal_candle_from_prior_low() {
        let mut candles = (0..20)
            .map(|index| candle(index, 100.0))
            .collect::<Vec<_>>();
        candles.push(candle(20, 97.9));
        assert!(closes_below_prior_structure_by_atr(&candles, 20, 10.0, 0.0));

        candles[20].close = 98.0;
        assert!(!closes_below_prior_structure_by_atr(
            &candles, 20, 10.0, 0.0
        ));
    }

    #[test]
    fn structure_break_depth_includes_exact_atr_boundary() {
        let mut candles = (0..20)
            .map(|index| candle(index, 100.0))
            .collect::<Vec<_>>();
        candles.push(candle(20, 97.0));
        assert!(closes_below_prior_structure_by_atr(
            &candles, 20, 10.0, 0.10
        ));
        assert!(!closes_below_prior_structure_by_atr(
            &candles, 20, 10.0, 0.20
        ));

        candles[20].close = 97.001;
        assert!(!closes_below_prior_structure_by_atr(
            &candles, 20, 10.0, 0.10
        ));

        candles[20].close = 96.0;
        assert!(closes_below_prior_structure_by_atr(
            &candles, 20, 10.0, 0.20
        ));
    }

    #[test]
    fn ema676_slope_uses_only_the_current_and_twenty_completed_bars_ago() {
        let mut points = vec![point(100.0, 102.0, 105.0, 110.0); 21];
        points[20].ema696 = Some(109.0);
        let indicators = IndicatorSeries {
            points: points.clone(),
        };
        assert!(ema676_is_falling(&indicators, 20));
        assert!(!ema676_is_falling(&indicators, 19));

        points[20].ema696 = Some(110.0);
        let flat = IndicatorSeries {
            points: points.clone(),
        };
        assert!(!ema676_is_falling(&flat, 20));

        points[20].ema696 = Some(111.0);
        let rising = IndicatorSeries { points };
        assert!(!ema676_is_falling(&rising, 20));
    }

    #[test]
    fn ema676_slope_fails_closed_when_either_endpoint_is_missing() {
        let mut points = vec![point(100.0, 102.0, 105.0, 110.0); 21];
        points[20].ema696 = None;
        let missing_current = IndicatorSeries {
            points: points.clone(),
        };
        assert!(!ema676_is_falling(&missing_current, 20));

        points[20].ema696 = Some(109.0);
        points[0].ema696 = None;
        let missing_previous = IndicatorSeries { points };
        assert!(!ema676_is_falling(&missing_previous, 20));
    }

    #[test]
    fn distance_guard_keeps_exact_boundary_and_rejects_farther_close() {
        let point = point(100.0, 102.0, 105.0, 109.0);
        assert!(is_within_ema12_distance(&point, 92.0, 10.0));
        assert!(!is_within_ema12_distance(&point, 91.9, 10.0));
    }

    #[test]
    fn right_side_retest_never_signals_on_source_and_freezes_source_values() {
        let candles = vec![
            candle(0, 90.0),
            Candle {
                open: 98.0,
                high: 99.0,
                low: 94.0,
                close: 96.0,
                ..candle(1, 96.0)
            },
        ];
        let indicators = IndicatorSeries {
            points: vec![
                point(100.0, 102.0, 105.0, 109.0),
                point(98.0, 101.0, 104.0, 108.0),
            ],
        };
        let mut state = EmaShortAblationState::new(EmaShortResearchVariant::RightSideRetest);
        assert!(state
            .evaluate(&candles, &indicators, 0, true, 10.0, Some(4.5), true)
            .is_none());
        let accepted = state
            .evaluate(&candles, &indicators, 1, false, 8.0, Some(2.7), true)
            .expect("first completed retest should confirm");
        assert_eq!(accepted.source_atr, 10.0);
        assert_eq!(accepted.source_take_profit_atr, 4.5);
        assert!(accepted.deferred);
    }

    #[test]
    fn right_side_retest_cannot_use_a_fourth_future_candle() {
        let mut candles = vec![candle(0, 90.0)];
        for index in 1..=3 {
            candles.push(Candle {
                open: 94.0,
                high: 95.0,
                low: 92.0,
                close: 93.0,
                ..candle(index, 93.0)
            });
        }
        candles.push(Candle {
            open: 98.0,
            high: 99.0,
            low: 94.0,
            close: 96.0,
            ..candle(4, 96.0)
        });
        let indicators = IndicatorSeries {
            points: (0..=4).map(|_| point(100.0, 102.0, 105.0, 109.0)).collect(),
        };
        let mut state = EmaShortAblationState::new(EmaShortResearchVariant::RightSideRetest);
        assert!(state
            .evaluate(&candles, &indicators, 0, true, 10.0, Some(4.5), true)
            .is_none());
        for index in 1..=4 {
            assert!(state
                .evaluate(&candles, &indicators, index, false, 10.0, Some(4.5), true)
                .is_none());
        }
    }

    #[test]
    fn extreme_volume_only_adds_structure_acceptance_at_ten_times() {
        let mut candles = (0..20)
            .map(|index| candle(index, 100.0))
            .collect::<Vec<_>>();
        candles.push(candle(20, 99.0));
        let mut points = vec![point(100.0, 102.0, 105.0, 109.0); 21];
        points[20].filtered_volume_ratio = Some(9.99);
        let indicators = IndicatorSeries {
            points: points.clone(),
        };
        let mut state =
            EmaShortAblationState::new(EmaShortResearchVariant::ExtremeVolumeAcceptance);
        assert!(state
            .evaluate(&candles, &indicators, 20, true, 10.0, Some(4.5), true)
            .is_some());

        points[20].filtered_volume_ratio = Some(10.0);
        let indicators = IndicatorSeries { points };
        let mut state =
            EmaShortAblationState::new(EmaShortResearchVariant::ExtremeVolumeAcceptance);
        assert!(state
            .evaluate(&candles, &indicators, 20, true, 10.0, Some(4.5), true)
            .is_none());

        candles[20].close = 97.9;
        let mut state =
            EmaShortAblationState::new(EmaShortResearchVariant::ExtremeVolumeAcceptance);
        assert!(state
            .evaluate(&candles, &indicators, 20, true, 10.0, Some(4.5), true)
            .is_some());
    }
}
