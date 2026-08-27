use super::consolidation_quality::strict_visual_parent_for_range;
use super::{Candle, StrictVisualBreakoutResearchVariant, StrictVisualRangeEvidence};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StrictVisualExecutionBounds {
    pub upper: f64,
    pub lower: f64,
}

/// V40 只替换执行边界；其他版本继续精确返回核心边界。
pub(super) fn frozen_execution_bounds(
    candles: &[Candle],
    range: StrictVisualRangeEvidence,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<StrictVisualExecutionBounds> {
    if variant == StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary {
        let parent = strict_visual_parent_for_range(candles, range)?;
        Some(StrictVisualExecutionBounds {
            upper: parent.upper,
            lower: parent.lower,
        })
    } else {
        Some(StrictVisualExecutionBounds {
            upper: range.upper,
            lower: range.lower,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::strict_visual_breakout::{
        StrictVisualConsolidationState, StrictVisualLongEntryEvent, StrictVisualLongEntryState,
        StrictVisualRangeEvent,
    };

    #[test]
    fn breakout_bar_cannot_expand_the_frozen_parent() {
        let mut candles = fixture();
        let range = range();
        let before = frozen_execution_bounds(
            &candles,
            range,
            StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary,
        )
        .unwrap();
        candles.push(candle(8, 101.0, 110.0, 100.9, 109.0));
        let after = frozen_execution_bounds(
            &candles,
            range,
            StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary,
        )
        .unwrap();
        assert_eq!(before, after);
        assert_eq!(before.upper, 101.4);
        assert_eq!(before.lower, 98.6);
    }

    #[test]
    fn close_outside_core_but_inside_parent_does_not_break_the_range() {
        let mut candles = fixture();
        candles.push(candle(8, 101.1, 101.3, 101.0, 101.2));
        let mut state = StrictVisualConsolidationState {
            active: Some(range()),
            ..Default::default()
        };
        assert_eq!(
            state.update_for_variant(
                &candles,
                8,
                0.1,
                StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary,
            ),
            None
        );
        assert_eq!(state.active, Some(range()));
    }

    #[test]
    fn weak_departure_can_return_inside_parent_while_still_outside_core() {
        let mut candles = fixture();
        candles.push(candle(8, 101.55, 101.7, 101.5, 101.6));
        candles.push(candle(9, 101.3, 101.35, 101.15, 101.2));
        let mut state = StrictVisualConsolidationState {
            active: Some(range()),
            ..Default::default()
        };
        assert!(matches!(
            state.update_for_variant(
                &candles,
                8,
                0.1,
                StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary,
            ),
            Some(StrictVisualRangeEvent::WeakDeparturePending(_))
        ));
        assert!(matches!(
            state.update_for_variant(
                &candles,
                9,
                0.1,
                StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary,
            ),
            Some(StrictVisualRangeEvent::WeakDepartureReturned(_))
        ));
    }

    #[test]
    fn short_breakout_and_retest_share_the_frozen_parent_lower_boundary() {
        let mut candles = fixture();
        candles.push(candle(8, 99.1, 99.2, 98.0, 98.1));
        let mut state = StrictVisualLongEntryState {
            range_state: StrictVisualConsolidationState {
                active: Some(range()),
                ..Default::default()
            },
            ..Default::default()
        };
        let variant = StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary;
        let Some(StrictVisualLongEntryEvent::AcceptanceArmed(source)) =
            state.update(&candles, 8, 0.1, Some(1.0), false, None, None, variant)
        else {
            panic!("short parent-boundary breakout must arm");
        };
        assert_eq!(source.range.lower, 99.0);
        assert_eq!(source.execution_boundary, 98.6);
        assert!((source.breakout_excess - 0.5).abs() < 1e-12);

        candles.push(candle(9, 98.3, 98.55, 98.1, 98.2));
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(confirmed)) =
            state.update(&candles, 9, 0.1, Some(1.0), false, None, None, variant)
        else {
            panic!("short retest must confirm against the same frozen parent boundary");
        };
        assert_eq!(confirmed.execution_boundary, 98.6);
        assert_eq!(confirmed.range.lower, 99.0);
    }

    fn fixture() -> Vec<Candle> {
        vec![
            candle(0, 100.0, 100.4, 99.7, 100.2),
            candle(1, 100.1, 101.5, 100.0, 101.4),
            candle(2, 100.4, 100.6, 99.9, 100.1),
            candle(3, 100.1, 100.3, 99.8, 100.0),
            candle(4, 100.0, 100.2, 98.5, 98.6),
            candle(5, 99.0, 100.1, 98.9, 99.8),
            candle(6, 99.8, 100.3, 99.6, 100.0),
            candle(7, 100.0, 100.2, 99.8, 100.1),
        ]
    }

    fn range() -> StrictVisualRangeEvidence {
        StrictVisualRangeEvidence {
            start_index: 0,
            first_confirmation_index: 7,
            boundary_confirmation_index: 7,
            length_bars: 8,
            upper: 101.0,
            lower: 99.0,
            containment_ratio: 0.8,
            direction_efficiency: 0.2,
            edge_transition_count: 3,
            upper_touch_groups: 2,
            lower_touch_groups: 2,
            upper_drift_ratio: 0.1,
            lower_drift_ratio: 0.1,
        }
    }

    fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 100.0,
        }
    }
}
