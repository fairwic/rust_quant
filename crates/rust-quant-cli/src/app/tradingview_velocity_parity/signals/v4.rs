use super::{round_down, round_up};
use crate::app::tradingview_velocity_parity::model::{Candle, Direction};
use crate::app::tradingview_velocity_parity::ranges::SidewaysZone;

const FRESH_BREAK_MAX_AGE: usize = 7;
const DISPLACEMENT_LOOKBACK: usize = 96;
const DISPLACEMENT_ATR: f64 = 6.0;
const DISPLACEMENT_RATIO: f64 = 0.05;

/// 冻结逆势结构目标：新破位默认只取回区间近边，大位移才允许瞄准远边。
pub(super) fn counter_trend_structure_target_v4(
    candles: &[Candle],
    index: usize,
    atr: f64,
    tick_size: f64,
    direction: Direction,
    zone: SidewaysZone,
) -> f64 {
    let fresh_break = fresh_sideways_break(candles, index, direction, zone);
    let expanded = fresh_break && excessive_96_bar_displacement(candles, index, atr, direction);
    let raw_target = match (direction, fresh_break, expanded) {
        (Direction::Long, true, false) => zone.low,
        (Direction::Short, true, false) => zone.high,
        (Direction::Long, _, _) => zone.high,
        (Direction::Short, _, _) => zone.low,
    };
    match direction {
        Direction::Long => round_down(raw_target, tick_size),
        Direction::Short => round_up(raw_target, tick_size),
    }
}

/// V4 空单目标优先级：fresh 横盘回归优先于同棒 transition sweep，最后才用普通横盘远边。
pub(super) fn select_short_structure_target_v4(
    fresh_sideways_target: Option<f64>,
    transition_target: Option<f64>,
    sideways_target: Option<f64>,
) -> Option<f64> {
    fresh_sideways_target
        .or(transition_target)
        .or(sideways_target)
}

/// 要求横盘结束后的首次收盘破位距信号不超过 7 根，且此后每根收盘都留在区间外。
pub(super) fn fresh_sideways_break(
    candles: &[Candle],
    index: usize,
    direction: Direction,
    zone: SidewaysZone,
) -> bool {
    let first_after_zone = zone.end_index.saturating_add(1);
    if first_after_zone == 0 || first_after_zone > index {
        return false;
    }
    let first_break = (first_after_zone..=index).find(|&candidate| match direction {
        Direction::Long => {
            candles[candidate].close < zone.low && candles[candidate - 1].close >= zone.low
        }
        Direction::Short => {
            candles[candidate].close > zone.high && candles[candidate - 1].close <= zone.high
        }
    });
    let Some(first_break) = first_break else {
        return false;
    };
    if index - first_break > FRESH_BREAK_MAX_AGE {
        return false;
    }
    candles[first_break..=index]
        .iter()
        .all(|candle| match direction {
            Direction::Long => candle.close < zone.low,
            Direction::Short => candle.close > zone.high,
        })
}

/// 用信号前 96 根的极值衡量趋势位移；ATR 与百分比门槛必须同时达到。
pub(super) fn excessive_96_bar_displacement(
    candles: &[Candle],
    index: usize,
    atr: f64,
    direction: Direction,
) -> bool {
    if index < DISPLACEMENT_LOOKBACK || atr <= 0.0 {
        return false;
    }
    let history = &candles[index - DISPLACEMENT_LOOKBACK..index];
    match direction {
        Direction::Long => {
            let highest = history
                .iter()
                .map(|candle| candle.high)
                .fold(f64::NEG_INFINITY, f64::max);
            highest - candles[index].close
                >= (DISPLACEMENT_ATR * atr).max(DISPLACEMENT_RATIO * highest)
        }
        Direction::Short => {
            let lowest = history
                .iter()
                .map(|candle| candle.low)
                .fold(f64::INFINITY, f64::min);
            candles[index].close - lowest
                >= (DISPLACEMENT_ATR * atr).max(DISPLACEMENT_RATIO * lowest)
        }
    }
}

/// V4 背离必须由方向实体或方向影线确认，影线阈值同时适配 tick 与 K 线振幅。
pub(super) fn divergence_candle_confirmed(
    candle: Candle,
    tick_size: f64,
    direction: Direction,
) -> bool {
    let range = candle.range();
    if range <= 0.0 || tick_size <= 0.0 {
        return false;
    }
    let wick_threshold = (2.0 * tick_size).max(0.25 * range);
    match direction {
        Direction::Long => {
            let lower_shadow = candle.open.min(candle.close) - candle.low;
            candle.close > candle.open || lower_shadow >= wick_threshold
        }
        Direction::Short => {
            let upper_shadow = candle.high - candle.open.max(candle.close);
            candle.close < candle.open || upper_shadow >= wick_threshold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{preserve_transition_stop, transition_entry_target};
    use super::*;
    use crate::app::tradingview_velocity_parity::model::ParityRuleVersion;

    fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn divergence_requires_direction_body_or_direction_wick() {
        let bullish_body = candle(0, 100.0, 102.0, 98.0, 101.0);
        let bearish_with_lower_wick = candle(0, 100.0, 101.0, 95.0, 98.0);
        let bearish_without_lower_wick = candle(0, 100.0, 101.0, 97.5, 98.0);
        assert!(divergence_candle_confirmed(
            bullish_body,
            0.1,
            Direction::Long
        ));
        assert!(divergence_candle_confirmed(
            bearish_with_lower_wick,
            0.1,
            Direction::Long
        ));
        assert!(!divergence_candle_confirmed(
            bearish_without_lower_wick,
            0.1,
            Direction::Long
        ));

        let bearish_body = candle(0, 100.0, 102.0, 98.0, 99.0);
        let bullish_with_upper_wick = candle(0, 98.0, 104.0, 97.0, 100.0);
        let bullish_without_upper_wick = candle(0, 98.0, 100.5, 97.0, 100.0);
        assert!(divergence_candle_confirmed(
            bearish_body,
            0.1,
            Direction::Short
        ));
        assert!(divergence_candle_confirmed(
            bullish_with_upper_wick,
            0.1,
            Direction::Short
        ));
        assert!(!divergence_candle_confirmed(
            bullish_without_upper_wick,
            0.1,
            Direction::Short
        ));
    }

    #[test]
    fn fresh_break_uses_near_edge_until_96_bar_displacement_is_large() {
        let mut candles = (0..106)
            .map(|index| candle(index, 100.0, 101.0, 99.0, 100.0))
            .collect::<Vec<_>>();
        for (index, current) in candles.iter_mut().enumerate().take(106).skip(100) {
            *current = candle(index, 98.5, 98.8, 97.8, 98.2);
        }
        let zone = SidewaysZone {
            high: 101.0,
            low: 99.0,
            end_index: 99,
        };

        assert!(fresh_sideways_break(&candles, 105, Direction::Long, zone));
        assert_eq!(
            counter_trend_structure_target_v4(&candles, 105, 1.0, 0.1, Direction::Long, zone),
            99.0
        );

        candles[20].high = 120.0;
        assert!(excessive_96_bar_displacement(
            &candles,
            105,
            1.0,
            Direction::Long
        ));
        assert_eq!(
            counter_trend_structure_target_v4(&candles, 105, 1.0, 0.1, Direction::Long, zone),
            101.0
        );
    }

    #[test]
    fn fresh_break_expires_after_seven_bars_and_rejects_reentry() {
        let mut candles = (0..109)
            .map(|index| candle(index, 100.0, 101.0, 99.0, 100.0))
            .collect::<Vec<_>>();
        for (index, current) in candles.iter_mut().enumerate().take(109).skip(100) {
            *current = candle(index, 98.5, 98.8, 97.8, 98.2);
        }
        let zone = SidewaysZone {
            high: 101.0,
            low: 99.0,
            end_index: 99,
        };
        assert!(!fresh_sideways_break(&candles, 108, Direction::Long, zone));

        candles[103] = candle(103, 98.5, 100.0, 98.0, 99.5);
        assert!(!fresh_sideways_break(&candles, 105, Direction::Long, zone));
    }

    #[test]
    fn second_break_does_not_refresh_an_expired_first_break() {
        let mut candles = (0..113)
            .map(|index| candle(index, 100.0, 101.0, 99.0, 100.0))
            .collect::<Vec<_>>();
        for (index, current) in candles.iter_mut().enumerate().take(109).skip(100) {
            *current = candle(index, 98.5, 98.8, 97.8, 98.2);
        }
        candles[109] = candle(109, 98.5, 100.0, 98.0, 99.5);
        for (index, current) in candles.iter_mut().enumerate().take(113).skip(110) {
            *current = candle(index, 98.5, 98.8, 97.8, 98.2);
        }
        let zone = SidewaysZone {
            high: 101.0,
            low: 99.0,
            end_index: 99,
        };

        assert!(!fresh_sideways_break(&candles, 112, Direction::Long, zone));
    }

    #[test]
    fn fresh_short_sideways_target_precedes_transition_sweep_target() {
        assert_eq!(
            select_short_structure_target_v4(Some(101.0), Some(95.0), Some(90.0)),
            Some(101.0)
        );
        assert_eq!(
            select_short_structure_target_v4(None, Some(95.0), Some(90.0)),
            Some(95.0)
        );
        assert_eq!(
            transition_entry_target(ParityRuleVersion::CandidateV4, Some(101.0), Some(95.0)),
            Some(101.0)
        );
        assert_eq!(
            transition_entry_target(ParityRuleVersion::CandidateV3, Some(101.0), Some(95.0)),
            Some(95.0)
        );
        assert!(preserve_transition_stop(
            ParityRuleVersion::CandidateV4,
            true
        ));
        assert!(!preserve_transition_stop(
            ParityRuleVersion::CandidateV3,
            true
        ));
    }
}
