use super::model::{Candle, Direction, StrictVisualBreakoutResearchVariant};
use super::strict_visual_breakout::{
    StrictVisualBreakoutSignal, StrictVisualLongEntryEvent, StrictVisualLongEntryState,
    StrictVisualNestedChildState, StrictVisualRangeEvent, StrictVisualRangeEvidence,
};
use serde::Serialize;
use std::collections::VecDeque;

/// V30 只查看突破棒左侧固定 96 根 15m 完成 K 线，即 24 小时。
pub const STRICT_VISUAL_PRIOR_RANGE_LOOKBACK_BARS: usize = 96;

/// 一个此前已确认、随后被结构离开事件结束的严格视觉横盘。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualCompletedRange {
    /// 旧横盘冻结证据。
    pub range: StrictVisualRangeEvidence,
    /// 首根离开旧横盘的完成棒索引。
    pub departure_index: usize,
}

/// 突破棒完成时冻结的前方旧横盘净空证据。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualPriorRangeObstruction {
    /// 实际阻挡当前 measured move 的旧横盘。
    pub prior: StrictVisualCompletedRange,
    /// 当前横盘高度。
    pub current_range_height: f64,
    /// 当前方向边界到旧横盘最近前沿的绝对价格距离；重叠时为 0。
    pub clearance: f64,
    /// `clearance / current_range_height`；V30 在该值不大于 1 时拒绝。
    pub clearance_in_range_heights: f64,
}

/// V36b 在突破棒完成时冻结的视觉边界与横盘前外部结构边界。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualTradeBoundaryEvidence {
    /// 横盘开始前 96 根窗口的起点。
    pub external_window_start_index: usize,
    /// 实际外部方向极值所属完成棒；历史不足时为 `None`。
    pub external_extreme_index: Option<usize>,
    /// 多头为外部最高价，空头为外部最低价；已经解决时为 `None`。
    pub unresolved_external_extreme: Option<f64>,
    /// 当前子横盘 P90/P10 视觉边界。
    pub visual_boundary: f64,
    /// 实际要求突破收盘越过的外部交易边界。
    pub trade_breakout_boundary: f64,
    /// 突破棒是否已沿方向越过交易边界至少一个 tick。
    pub crossed_by_breakout_close: bool,
}

/// 组合状态一次推进后公开的核心事件、可交易确认或 V30 拒绝证据。
#[derive(Debug, Clone, Copy, Default)]
pub struct StrictVisualPriorRangeUpdate {
    /// 原严格横盘状态机事件，用于 L1 审计；它不含成交后结果。
    pub event: Option<StrictVisualLongEntryEvent>,
    /// 通过当前版本全部门禁的确认来源。
    pub confirmed: Option<StrictVisualBreakoutSignal>,
    /// V30 因前方净空不足拒绝的来源与冻结证据。
    pub rejected: Option<(
        StrictVisualBreakoutSignal,
        StrictVisualPriorRangeObstruction,
    )>,
    /// V36b 未收盘越过横盘前方向外部极值时的冻结拒绝证据。
    pub trade_boundary_rejected: Option<(
        StrictVisualBreakoutSignal,
        StrictVisualTradeBoundaryEvidence,
    )>,
}

/// 在原严格横盘状态机外维护已完成旧横盘；非 V30 版本只透传原确认行为。
#[derive(Debug, Default)]
pub struct StrictVisualPriorRangeClearanceState {
    core: StrictVisualLongEntryState,
    nested_child: StrictVisualNestedChildState,
    completed: VecDeque<StrictVisualCompletedRange>,
    pending_obstruction: Option<(usize, Option<StrictVisualPriorRangeObstruction>)>,
}

impl StrictVisualPriorRangeClearanceState {
    /// 逐根推进并在突破棒完成时冻结 V30 旧横盘净空，确认棒不得重算。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        source_atr: Option<f64>,
        volume_event: bool,
        volume_ratio: Option<f64>,
        take_profit_atr: Option<f64>,
        variant: StrictVisualBreakoutResearchVariant,
    ) -> StrictVisualPriorRangeUpdate {
        let parent_event = self.core.update(
            candles,
            index,
            tick_size,
            source_atr,
            volume_event,
            volume_ratio,
            take_profit_atr,
            variant,
        );
        let child_event = variant.uses_nested_child_range().then(|| {
            self.nested_child.update(
                candles,
                index,
                tick_size,
                source_atr,
                volume_event,
                volume_ratio,
                take_profit_atr,
                variant,
                parent_event,
            )
        });
        let event = parent_event.or(child_event.flatten());
        let mut update = StrictVisualPriorRangeUpdate {
            event,
            ..StrictVisualPriorRangeUpdate::default()
        };
        let Some(event) = event else {
            return update;
        };

        match event {
            StrictVisualLongEntryEvent::AcceptanceArmed(source) => {
                if variant.uses_split_trade_breakout_boundary() {
                    let evidence = self.trade_boundary_for(candles, source, tick_size);
                    if !evidence.crossed_by_breakout_close {
                        self.core.discard_pending_acceptance();
                        update.trade_boundary_rejected = Some((source, evidence));
                        self.record_completed(source.range, source.breakout_index);
                        return update;
                    }
                }
                let obstruction = variant
                    .requires_prior_valid_range_clearance()
                    .then(|| self.obstruction_for(candles, source, tick_size))
                    .flatten();
                self.pending_obstruction = Some((source.breakout_index, obstruction));
                self.record_completed(source.range, source.breakout_index);
            }
            StrictVisualLongEntryEvent::DirectSignal(source) => {
                if variant.uses_split_trade_breakout_boundary() {
                    let evidence = self.trade_boundary_for(candles, source, tick_size);
                    if !evidence.crossed_by_breakout_close {
                        update.trade_boundary_rejected = Some((source, evidence));
                        self.record_completed(source.range, source.breakout_index);
                        return update;
                    }
                }
                let obstruction = variant
                    .requires_prior_valid_range_clearance()
                    .then(|| self.obstruction_for(candles, source, tick_size))
                    .flatten();
                self.record_completed(source.range, source.breakout_index);
                if let Some(obstruction) = obstruction {
                    update.rejected = Some((source, obstruction));
                } else {
                    update.confirmed = Some(source);
                }
            }
            StrictVisualLongEntryEvent::AcceptanceConfirmed(source) => {
                let obstruction = self
                    .pending_obstruction
                    .take()
                    .filter(|(breakout_index, _)| *breakout_index == source.breakout_index)
                    .and_then(|(_, obstruction)| obstruction);
                if variant.requires_prior_valid_range_clearance() {
                    if let Some(obstruction) = obstruction {
                        update.rejected = Some((source, obstruction));
                    } else {
                        update.confirmed = Some(source);
                    }
                } else {
                    update.confirmed = Some(source);
                }
            }
            StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(_)
            | StrictVisualLongEntryEvent::AcceptanceMarginRejected(_)
            | StrictVisualLongEntryEvent::ExternalStructureRejected(_)
            | StrictVisualLongEntryEvent::AcceptanceInvalidated(_)
            | StrictVisualLongEntryEvent::AcceptanceExpired(_) => {
                self.pending_obstruction = None;
            }
            StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(range))
            | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(range)) => {
                self.record_completed(range, index);
            }
            StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureConsumed(
                evidence,
            )) => {
                self.record_completed(evidence.range, evidence.departure_index);
            }
            StrictVisualLongEntryEvent::Range(_) => {}
        }
        update
    }

    /// 保留足够覆盖下一次 96 根审计的已完成横盘，并避免重复记录同一次离开。
    fn record_completed(&mut self, range: StrictVisualRangeEvidence, departure_index: usize) {
        if self.completed.back().is_some_and(|last| {
            last.departure_index == departure_index
                && last.range.start_index == range.start_index
                && last.range.length_bars == range.length_bars
        }) {
            return;
        }
        self.completed.push_back(StrictVisualCompletedRange {
            range,
            departure_index,
        });
        while self.completed.front().is_some_and(|oldest| {
            departure_index.saturating_sub(oldest.departure_index)
                > STRICT_VISUAL_PRIOR_RANGE_LOOKBACK_BARS * 2
        }) {
            self.completed.pop_front();
        }
    }

    /// 从突破棒当时可见的旧横盘中选择价格方向最近且 measured move 会触达的障碍。
    fn obstruction_for(
        &self,
        candles: &[Candle],
        source: StrictVisualBreakoutSignal,
        tick_size: f64,
    ) -> Option<StrictVisualPriorRangeObstruction> {
        let current_height = source.range.upper - source.range.lower;
        if current_height <= 0.0 || tick_size <= 0.0 {
            return None;
        }
        self.completed
            .iter()
            .filter(|prior| {
                prior.departure_index < source.range.start_index
                    && source.breakout_index.saturating_sub(prior.departure_index)
                        <= STRICT_VISUAL_PRIOR_RANGE_LOOKBACK_BARS
            })
            .filter_map(|prior| {
                let unresolved = !prior_range_resolved(
                    candles,
                    *prior,
                    source.breakout_index,
                    source.direction,
                    tick_size,
                );
                if !unresolved {
                    return None;
                }
                let clearance = match source.direction {
                    Direction::Long if prior.range.upper >= source.range.upper + tick_size => {
                        (prior.range.lower - source.range.upper).max(0.0)
                    }
                    Direction::Short if prior.range.lower <= source.range.lower - tick_size => {
                        (source.range.lower - prior.range.upper).max(0.0)
                    }
                    _ => return None,
                };
                let clearance_in_range_heights = clearance / current_height;
                (clearance_in_range_heights <= 1.0).then_some(StrictVisualPriorRangeObstruction {
                    prior: *prior,
                    current_range_height: current_height,
                    clearance,
                    clearance_in_range_heights,
                })
            })
            .min_by(|left, right| {
                left.clearance
                    .total_cmp(&right.clearance)
                    .then_with(|| right.prior.departure_index.cmp(&left.prior.departure_index))
            })
    }

    /// 选择当前横盘开始前 96 根的方向外部极值；已解决或历史不足时退化为视觉边界。
    fn trade_boundary_for(
        &self,
        candles: &[Candle],
        source: StrictVisualBreakoutSignal,
        tick_size: f64,
    ) -> StrictVisualTradeBoundaryEvidence {
        let external_window_start_index = source
            .range
            .start_index
            .saturating_sub(STRICT_VISUAL_PRIOR_RANGE_LOOKBACK_BARS);
        let external = candles
            .get(external_window_start_index..source.range.start_index)
            .and_then(|history| match source.direction {
                Direction::Long => history
                    .iter()
                    .enumerate()
                    .max_by(|left, right| {
                        left.1
                            .high
                            .total_cmp(&right.1.high)
                            .then_with(|| left.0.cmp(&right.0))
                    })
                    .map(|(offset, candle)| (external_window_start_index + offset, candle.high)),
                Direction::Short => history
                    .iter()
                    .enumerate()
                    .min_by(|left, right| {
                        left.1
                            .low
                            .total_cmp(&right.1.low)
                            .then_with(|| right.0.cmp(&left.0))
                    })
                    .map(|(offset, candle)| (external_window_start_index + offset, candle.low)),
            });
        let visual_boundary = match source.direction {
            Direction::Long => source.range.upper,
            Direction::Short => source.range.lower,
        };
        let unresolved_external = external.filter(|(_, extreme)| {
            let history = candles
                .get(source.range.start_index..source.breakout_index)
                .unwrap_or_default();
            match source.direction {
                Direction::Long => !history
                    .iter()
                    .any(|candle| candle.close >= *extreme + tick_size),
                Direction::Short => !history
                    .iter()
                    .any(|candle| candle.close <= *extreme - tick_size),
            }
        });
        let trade_breakout_boundary =
            unresolved_external.map_or(visual_boundary, |(_, extreme)| match source.direction {
                Direction::Long => visual_boundary.max(extreme),
                Direction::Short => visual_boundary.min(extreme),
            });
        let crossed_by_breakout_close = match source.direction {
            Direction::Long => source.breakout_close >= trade_breakout_boundary + tick_size,
            Direction::Short => source.breakout_close <= trade_breakout_boundary - tick_size,
        };
        StrictVisualTradeBoundaryEvidence {
            external_window_start_index,
            external_extreme_index: unresolved_external.map(|(index, _)| index),
            unresolved_external_extreme: unresolved_external.map(|(_, extreme)| extreme),
            visual_boundary,
            trade_breakout_boundary,
            crossed_by_breakout_close,
        }
    }
}

/// 旧横盘若已被同方向完成收盘穿越一个 tick，则不再作为后续突破障碍。
fn prior_range_resolved(
    candles: &[Candle],
    prior: StrictVisualCompletedRange,
    breakout_index: usize,
    direction: Direction,
    tick_size: f64,
) -> bool {
    let start = prior.departure_index.saturating_add(1);
    let Some(history) = candles.get(start..breakout_index) else {
        return true;
    };
    match direction {
        Direction::Long => history
            .iter()
            .any(|candle| candle.close >= prior.range.upper + tick_size),
        Direction::Short => history
            .iter()
            .any(|candle| candle.close <= prior.range.lower - tick_size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造只关注收盘解决语义的有效测试 K 线。
    fn candle(close: f64) -> Candle {
        Candle {
            timestamp_ms: 0,
            open: close,
            high: close + 0.2,
            low: close - 0.2,
            close,
            volume: 1.0,
        }
    }

    /// 构造边界与索引明确的冻结横盘，其他质量字段不参与本模块决策。
    fn range(start_index: usize, upper: f64, lower: f64) -> StrictVisualRangeEvidence {
        StrictVisualRangeEvidence {
            start_index,
            first_confirmation_index: start_index + 7,
            boundary_confirmation_index: start_index + 7,
            length_bars: 8,
            upper,
            lower,
            containment_ratio: 1.0,
            direction_efficiency: 0.0,
            edge_transition_count: 3,
            upper_touch_groups: 2,
            lower_touch_groups: 2,
            upper_drift_ratio: 0.0,
            lower_drift_ratio: 0.0,
        }
    }

    /// 构造 V30 只需读取的突破冻结字段。
    fn source(
        direction: Direction,
        current: StrictVisualRangeEvidence,
        breakout_index: usize,
    ) -> StrictVisualBreakoutSignal {
        StrictVisualBreakoutSignal {
            range: current,
            execution_boundary: match direction {
                Direction::Long => current.upper,
                Direction::Short => current.lower,
            },
            direction,
            breakout_index,
            signal_index: breakout_index,
            breakout_open: 100.0,
            breakout_close: 101.0,
            breakout_candle_extreme_stop_price: None,
            breakout_candle_extreme_stop_min_atr_multiple: None,
            breakout_body_midpoint: 100.5,
            source_atr: 1.0,
            source_volume_ratio: 0.0,
            source_take_profit_atr: 0.0,
            volume_gate_applied: false,
            short_range_one_r_target: false,
            retest_band: 0.1,
            breakout_excess: 0.5,
            required_acceptance_close: 100.5,
            measured_move_target_price: None,
            external_structure: None,
        }
    }

    #[test]
    fn unresolved_long_range_within_one_height_blocks() {
        let candles = vec![candle(100.0); 40];
        let mut state = StrictVisualPriorRangeClearanceState::default();
        state.completed.push_back(StrictVisualCompletedRange {
            range: range(0, 103.0, 101.5),
            departure_index: 8,
        });
        let obstruction = state.obstruction_for(
            &candles,
            source(Direction::Long, range(20, 101.0, 100.0), 30),
            0.1,
        );
        assert_eq!(obstruction.map(|item| item.clearance), Some(0.5));
    }

    #[test]
    fn completed_close_above_prior_upper_resolves_long_obstruction() {
        let mut candles = vec![candle(100.0); 40];
        candles[15] = candle(103.2);
        let mut state = StrictVisualPriorRangeClearanceState::default();
        state.completed.push_back(StrictVisualCompletedRange {
            range: range(0, 103.0, 101.5),
            departure_index: 8,
        });
        let obstruction = state.obstruction_for(
            &candles,
            source(Direction::Long, range(20, 101.0, 100.0), 30),
            0.1,
        );
        assert!(obstruction.is_none());
    }

    #[test]
    fn unresolved_short_range_uses_mirrored_front_edge() {
        let candles = vec![candle(100.0); 40];
        let mut state = StrictVisualPriorRangeClearanceState::default();
        state.completed.push_back(StrictVisualCompletedRange {
            range: range(0, 99.0, 97.0),
            departure_index: 8,
        });
        let obstruction = state.obstruction_for(
            &candles,
            source(Direction::Short, range(20, 101.0, 100.0), 30),
            0.1,
        );
        assert_eq!(obstruction.map(|item| item.clearance), Some(1.0));
    }

    #[test]
    fn split_short_trade_boundary_uses_pre_range_external_low() {
        let mut candles = vec![candle(100.0); 40];
        candles[8] = Candle {
            low: 97.0,
            ..candle(98.0)
        };
        let state = StrictVisualPriorRangeClearanceState::default();
        let mut short = source(Direction::Short, range(20, 101.0, 100.0), 30);
        short.breakout_close = 99.0;
        let evidence = state.trade_boundary_for(&candles, short, 0.1);
        assert_eq!(evidence.visual_boundary, 100.0);
        assert_eq!(evidence.trade_breakout_boundary, 97.0);
        assert_eq!(evidence.external_extreme_index, Some(8));
        assert!(!evidence.crossed_by_breakout_close);
    }
}
