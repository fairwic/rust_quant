use super::{round_down, round_up};
use crate::app::tradingview_velocity_parity::model::{Direction, IndicatorPoint, IndicatorSeries};
use crate::app::tradingview_velocity_parity::ranges::SidewaysZone;

pub(super) const EMA_ALIGNMENT_AGE_CAP: usize = 600;

/// V5 在信号收盘冻结的年龄化结构退出参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RsiCounterTrendPlanV5 {
    /// 包含信号棒的连续严格逆势 EMA 排列年龄，最多冻结为 600。
    pub(super) ema_alignment_age: usize,
    /// 年轻分支取横盘近边，600 根成熟分支取横盘远边。
    pub(super) target_price: f64,
    /// 信号时冻结的横盘近边；成熟分支必须由后续完成棒严格穿过。
    pub(super) structure_breakout_line: f64,
}

/// 计算 V4/V5 审计共用的严格 EMA 排列年龄；包含信号棒，缺值或排列断开即停止。
pub(super) fn counter_trend_ema_age_capped_600(
    indicators: &IndicatorSeries,
    index: usize,
    direction: Direction,
) -> usize {
    let mut age = 0;
    for offset in 0..EMA_ALIGNMENT_AGE_CAP {
        let Some(candidate) = index.checked_sub(offset) else {
            break;
        };
        if !strict_counter_trend_alignment(&indicators.points[candidate], direction) {
            break;
        }
        age += 1;
    }
    age
}

/// 纯 RSI 中性或过渡排列在 V5 中被阻止；重叠独立家族由调用方在传入前排除。
pub(super) fn blocks_pure_rsi_neutral_v5(
    pure_rsi_divergence: bool,
    counter_trend: bool,
    trend_aligned: bool,
) -> bool {
    pure_rsi_divergence && !counter_trend && !trend_aligned
}

/// 使用同一个已确认横盘区冻结 V5 目标与近边结构线，避免持仓后重新识别边界。
pub(super) fn rsi_counter_trend_plan_v5(
    indicators: &IndicatorSeries,
    index: usize,
    direction: Direction,
    zone: SidewaysZone,
    tick_size: f64,
) -> Option<RsiCounterTrendPlanV5> {
    let ema_alignment_age = counter_trend_ema_age_capped_600(indicators, index, direction);
    if ema_alignment_age == 0 {
        return None;
    }

    let mature = ema_alignment_age >= EMA_ALIGNMENT_AGE_CAP;
    let (raw_target, raw_breakout_line) = match direction {
        Direction::Long => (if mature { zone.high } else { zone.low }, zone.low),
        Direction::Short => (if mature { zone.low } else { zone.high }, zone.high),
    };
    let (target_price, structure_breakout_line) = match direction {
        Direction::Long => (
            round_down(raw_target, tick_size),
            round_down(raw_breakout_line, tick_size),
        ),
        Direction::Short => (
            round_up(raw_target, tick_size),
            round_up(raw_breakout_line, tick_size),
        ),
    };
    Some(RsiCounterTrendPlanV5 {
        ema_alignment_age,
        target_price,
        structure_breakout_line,
    })
}

/// V5 的多单逆势对应严格空头排列，空单逆势对应严格多头排列。
fn strict_counter_trend_alignment(point: &IndicatorPoint, direction: Direction) -> bool {
    let Some((ema12, ema144, ema696)) = point
        .ema12
        .zip(point.ema144)
        .zip(point.ema696)
        .map(|((ema12, ema144), ema696)| (ema12, ema144, ema696))
    else {
        return false;
    };
    match direction {
        Direction::Long => ema12 < ema144 && ema144 < ema696,
        Direction::Short => ema12 > ema144 && ema144 > ema696,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(ema12: f64, ema144: f64, ema696: f64) -> IndicatorPoint {
        IndicatorPoint {
            ema12: Some(ema12),
            ema144: Some(ema144),
            ema696: Some(ema696),
            ..IndicatorPoint::default()
        }
    }

    #[test]
    fn ema_alignment_age_includes_signal_bar_resets_and_caps_at_six_hundred() {
        let mut points = vec![point(1.0, 2.0, 3.0); 605];
        let series = IndicatorSeries {
            points: points.clone(),
        };
        assert_eq!(
            counter_trend_ema_age_capped_600(&series, 0, Direction::Long),
            1
        );
        assert_eq!(
            counter_trend_ema_age_capped_600(&series, 604, Direction::Long),
            600
        );

        points[601] = point(3.0, 2.0, 1.0);
        let reset = IndicatorSeries { points };
        assert_eq!(
            counter_trend_ema_age_capped_600(&reset, 604, Direction::Long),
            3
        );
        assert_eq!(
            counter_trend_ema_age_capped_600(&reset, 601, Direction::Short),
            1
        );
    }

    #[test]
    fn age_five_hundred_ninety_nine_uses_near_edge_and_six_hundred_uses_far_edge() {
        let zone = SidewaysZone {
            high: 110.04,
            low: 100.06,
            end_index: 0,
        };
        let young = IndicatorSeries {
            points: vec![point(1.0, 2.0, 3.0); 599],
        };
        let mature = IndicatorSeries {
            points: vec![point(3.0, 2.0, 1.0); 600],
        };

        let long = rsi_counter_trend_plan_v5(&young, 598, Direction::Long, zone, 0.1)
            .expect("strict bearish alignment");
        assert_eq!(long.ema_alignment_age, 599);
        assert!((long.target_price - 100.0).abs() < 1e-9);
        assert!((long.structure_breakout_line - 100.0).abs() < 1e-9);

        let short = rsi_counter_trend_plan_v5(&mature, 599, Direction::Short, zone, 0.1)
            .expect("strict bullish alignment");
        assert_eq!(short.ema_alignment_age, 600);
        assert!((short.target_price - 100.1).abs() < 1e-9);
        assert!((short.structure_breakout_line - 110.1).abs() < 1e-9);
    }

    #[test]
    fn only_pure_rsi_neutral_or_transition_is_blocked() {
        assert!(blocks_pure_rsi_neutral_v5(true, false, false));
        assert!(!blocks_pure_rsi_neutral_v5(true, true, false));
        assert!(!blocks_pure_rsi_neutral_v5(true, false, true));
        assert!(!blocks_pure_rsi_neutral_v5(false, false, false));
    }
}
