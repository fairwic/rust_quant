use super::super::model::{Candle, EmaTrendLongResearchVariant, IndicatorPoint};
use super::prior_extremes;

const STRUCTURE_LOOKBACK: usize = 20;

/// 判断 Research 补充来源是否满足目标缺口与本轮冻结突破深度。
///
/// 结构高点只读取信号 K 之前的 20 根已完成 K 线，禁止用确认棒重算边界。
#[allow(clippy::too_many_arguments)]
pub(super) fn research_source_constraints_pass(
    variant: EmaTrendLongResearchVariant,
    candles: &[Candle],
    index: usize,
    candle: Candle,
    point: &IndicatorPoint,
    ema12: f64,
    atr: f64,
    body_open_ratio: f64,
) -> bool {
    if variant.requires_all_target_gaps() {
        let volume_in_gap = point
            .weekly_volume_p80
            .zip(point.weekly_volume_p90)
            .is_some_and(|(p80, p90)| candle.volume >= p80 && candle.volume < p90);
        let ratio_in_gap = point
            .filtered_volume_ratio
            .is_some_and(|ratio| (2.5..3.0).contains(&ratio));
        let body_in_gap = body_open_ratio > 0.003 && body_open_ratio <= 0.01;
        let distance_atr = (candle.close - ema12) / atr;
        if !(volume_in_gap
            && ratio_in_gap
            && body_in_gap
            && distance_atr > 1.25
            && distance_atr <= 1.50)
        {
            return false;
        }
    }

    let minimum_depth = variant.source_break_depth_atr_min();
    minimum_depth <= 0.0
        || (atr > 0.0
            && prior_extremes(candles, index, STRUCTURE_LOOKBACK)
                .is_some_and(|(prior_high, _)| (candle.close - prior_high) / atr >= minimum_depth))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conservative_target_gap_requires_all_four_original_threshold_gaps() {
        let candle = Candle {
            timestamp_ms: 0,
            open: 100.0,
            high: 101.0,
            low: 99.8,
            close: 100.4,
            volume: 50.0,
        };
        let point = IndicatorPoint {
            filtered_volume_ratio: Some(2.6),
            weekly_volume_p80: Some(40.0),
            weekly_volume_p90: Some(60.0),
            ..IndicatorPoint::default()
        };

        assert!(research_source_constraints_pass(
            EmaTrendLongResearchVariant::ConservativeTargetGap,
            &[],
            0,
            candle,
            &point,
            99.0,
            1.0,
            0.004,
        ));
        assert!(!research_source_constraints_pass(
            EmaTrendLongResearchVariant::ConservativeTargetGap,
            &[],
            0,
            Candle {
                volume: 60.0,
                ..candle
            },
            &point,
            99.0,
            1.0,
            0.004,
        ));
    }

    #[test]
    fn breakout_depth_variants_read_only_the_frozen_prior_high() {
        let prior = Candle {
            timestamp_ms: 0,
            open: 99.8,
            high: 100.0,
            low: 99.5,
            close: 99.9,
            volume: 10.0,
        };
        let signal = Candle {
            timestamp_ms: 20,
            open: 100.0,
            high: 100.6,
            low: 99.9,
            close: 100.35,
            volume: 50.0,
        };
        let mut candles = vec![prior; 21];
        candles[20] = signal;
        let point = IndicatorPoint::default();

        assert!(research_source_constraints_pass(
            EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth20,
            &candles,
            20,
            signal,
            &point,
            99.0,
            1.0,
            0.0035,
        ));
        assert!(research_source_constraints_pass(
            EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30,
            &candles,
            20,
            signal,
            &point,
            99.0,
            1.0,
            0.0035,
        ));
        assert!(!research_source_constraints_pass(
            EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth40,
            &candles,
            20,
            signal,
            &point,
            99.0,
            1.0,
            0.0035,
        ));
    }
}
