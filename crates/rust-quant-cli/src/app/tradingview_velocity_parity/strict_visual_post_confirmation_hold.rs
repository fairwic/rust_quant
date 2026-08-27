use super::model::{Candle, Direction, StrictVisualBreakoutResearchVariant};
use super::strict_visual_breakout::StrictVisualBreakoutSignal;

const MINIMUM_ACCEPTANCE_RETENTION_RATIO: f64 = 0.75;

/// 额外一棒守稳在当前完成棒产生的冻结决策。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StrictVisualPostConfirmationHoldDecision {
    /// 当前棒守住冻结接受价后形成的新信号；非 V28 直接透传原确认信号。
    pub confirmed: Option<StrictVisualBreakoutSignal>,
    /// 当前棒未守住冻结接受价而永久消费的来源。
    pub rejected: Option<StrictVisualBreakoutSignal>,
}

/// V28 在首次确认与紧邻完成棒之间保存的唯一来源状态。
#[derive(Debug, Default)]
pub struct StrictVisualPostConfirmationHoldState {
    pending: Option<StrictVisualBreakoutSignal>,
}

impl StrictVisualPostConfirmationHoldState {
    /// 先解决上一棒挂起的来源，再允许当前首次确认挂起为下一棒来源。
    ///
    /// 该状态只改变信号时间；突破边界、止损、目标和来源 ATR 均保持首次确认时的冻结值。
    pub fn update(
        &mut self,
        index: usize,
        candle: Candle,
        newly_confirmed: Option<StrictVisualBreakoutSignal>,
        variant: StrictVisualBreakoutResearchVariant,
    ) -> StrictVisualPostConfirmationHoldDecision {
        if !variant.requires_next_completed_bar_hold() {
            self.pending = None;
            return StrictVisualPostConfirmationHoldDecision {
                confirmed: newly_confirmed,
                rejected: None,
            };
        }

        let mut decision = StrictVisualPostConfirmationHoldDecision::default();
        if let Some(source) = self.pending.take() {
            let decided = StrictVisualBreakoutSignal {
                signal_index: index,
                ..source
            };
            let adjacent = index == source.signal_index.saturating_add(1);
            if adjacent && strict_visual_next_bar_holds_acceptance(source, candle.close) {
                decision.confirmed = Some(decided);
            } else {
                decision.rejected = Some(decided);
            }
        }
        self.pending = newly_confirmed;
        decision
    }
}

/// 返回 V28 紧邻完成棒必须守住的 75% 绝对接受价，多空完全镜像。
pub fn strict_visual_next_bar_hold_required_close(source: StrictVisualBreakoutSignal) -> f64 {
    match source.direction {
        Direction::Long => {
            source.execution_boundary + source.breakout_excess * MINIMUM_ACCEPTANCE_RETENTION_RATIO
        }
        Direction::Short => {
            source.execution_boundary - source.breakout_excess * MINIMUM_ACCEPTANCE_RETENTION_RATIO
        }
    }
}

/// 判断紧邻完成棒是否仍保留首次突破越界幅度的至少 75%。
fn strict_visual_next_bar_holds_acceptance(source: StrictVisualBreakoutSignal, close: f64) -> bool {
    let required = strict_visual_next_bar_hold_required_close(source);
    match source.direction {
        Direction::Long => close >= required,
        Direction::Short => close <= required,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::strict_visual_breakout::StrictVisualRangeEvidence;

    #[test]
    fn required_close_formula_is_mirrored_around_the_frozen_boundary() {
        assert!(
            (strict_visual_next_bar_hold_required_close(signal(Direction::Long)) - 106.0).abs()
                < 1e-12
        );
        assert!(
            (strict_visual_next_bar_hold_required_close(signal(Direction::Short)) - 84.0).abs()
                < 1e-12
        );
    }

    #[test]
    fn candidate_waits_exactly_one_completed_bar_before_confirming() {
        let source = signal(Direction::Long);
        let mut state = StrictVisualPostConfirmationHoldState::default();
        let armed = state.update(
            source.signal_index,
            candle(108.0),
            Some(source),
            StrictVisualBreakoutResearchVariant::V28AcceptanceRetention75NextBarHold,
        );
        assert_eq!(armed, StrictVisualPostConfirmationHoldDecision::default());

        let confirmed = state.update(
            source.signal_index + 1,
            candle(106.0),
            None,
            StrictVisualBreakoutResearchVariant::V28AcceptanceRetention75NextBarHold,
        );
        assert_eq!(
            confirmed.confirmed.unwrap().signal_index,
            source.signal_index + 1
        );
        assert!(confirmed.rejected.is_none());
    }

    /// 构造只携带守稳状态所需价格身份的冻结来源。
    fn signal(direction: Direction) -> StrictVisualBreakoutSignal {
        StrictVisualBreakoutSignal {
            range: StrictVisualRangeEvidence {
                start_index: 0,
                first_confirmation_index: 7,
                boundary_confirmation_index: 9,
                length_bars: 10,
                upper: 100.0,
                lower: 90.0,
                containment_ratio: 0.8,
                direction_efficiency: 0.2,
                edge_transition_count: 3,
                upper_touch_groups: 2,
                lower_touch_groups: 2,
                upper_drift_ratio: 0.1,
                lower_drift_ratio: 0.1,
            },
            execution_boundary: if direction == Direction::Long {
                100.0
            } else {
                90.0
            },
            direction,
            breakout_index: 10,
            signal_index: 11,
            breakout_open: 99.0,
            breakout_close: if direction == Direction::Long {
                108.0
            } else {
                82.0
            },
            breakout_candle_extreme_stop_price: Some(if direction == Direction::Long {
                98.0
            } else {
                102.0
            }),
            breakout_candle_extreme_stop_min_atr_multiple: Some(1.0),
            breakout_body_midpoint: 103.5,
            source_atr: 2.0,
            source_volume_ratio: 0.0,
            source_take_profit_atr: 0.0,
            volume_gate_applied: false,
            short_range_one_r_target: false,
            retest_band: 0.5,
            breakout_excess: 8.0,
            required_acceptance_close: if direction == Direction::Long {
                102.0
            } else {
                88.0
            },
            measured_move_target_price: Some(if direction == Direction::Long {
                110.0
            } else {
                80.0
            }),
            external_structure: None,
        }
    }

    /// 构造有效完成棒；本状态只读取收盘价。
    fn candle(close: f64) -> Candle {
        Candle {
            timestamp_ms: 0,
            open: close,
            high: close,
            low: close,
            close,
            volume: 1.0,
        }
    }
}
