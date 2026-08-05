use super::{Candle, IndicatorPoint, IndicatorSeries};

const SLOW_EMA_BAND_RECLAIM_WINDOW: usize = 5;

/// V8 只记录实际从 V5 普通 RSI 长上影空单中移除的候选，便于删除集审计。
pub(super) const V8_BLOCK_REASON: &str =
    "V8_RSI_OVERBOUGHT_UPPER_WICK_FRESH_SLOW_EMA_BAND_RECLAIM_5";

/// 复用同一个冲量定义识别既有多头制度切换与 V8 慢均线带收复，避免两条保护线漂移。
pub(super) fn strong_bullish_volume_impulse(
    candle: Candle,
    point: &IndicatorPoint,
    atr: f64,
) -> bool {
    point.volume_event
        && point
            .filtered_volume_ratio
            .is_some_and(|ratio| ratio.is_finite() && ratio >= 6.0)
        && atr.is_finite()
        && atr > 0.0
        && candle.close > candle.open
        && candle.close - candle.open >= atr
}

/// 判断当前棒是否仍处于最近一次强势收复动态慢均线带后的五棒保护期。
///
/// 每根持有棒都重新使用当根 `max(EMA596, EMA696)`，避免冻结旧均线值后把已经
/// 跌回动态慢线下方的价格误判为仍被市场接受。
pub(super) fn fresh_slow_ema_band_reclaim(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
) -> bool {
    let first_candidate = index.saturating_sub(SLOW_EMA_BAND_RECLAIM_WINDOW - 1);
    (first_candidate..=index).rev().any(|reclaim_index| {
        let Some(previous_index) = reclaim_index.checked_sub(1) else {
            return false;
        };
        let Some(reclaim_band) = slow_band_upper(indicators, reclaim_index) else {
            return false;
        };
        let Some(previous_band) = slow_band_upper(indicators, previous_index) else {
            return false;
        };
        let Some(atr) = indicators.points[reclaim_index]
            .atr14
            .filter(|value| value.is_finite() && *value > 0.0)
        else {
            return false;
        };
        strong_bullish_volume_impulse(
            candles[reclaim_index],
            &indicators.points[reclaim_index],
            atr,
        ) && candles[reclaim_index].close > reclaim_band
            && candles[previous_index].close <= previous_band
            && (reclaim_index..=index).all(|held_index| {
                slow_band_upper(indicators, held_index)
                    .is_some_and(|band| candles[held_index].close > band)
            })
    })
}

fn slow_band_upper(indicators: &IndicatorSeries, index: usize) -> Option<f64> {
    let point = indicators.get(index)?;
    let (ema596, ema696) = point.ema596.zip(point.ema696)?;
    (ema596.is_finite() && ema696.is_finite()).then_some(ema596.max(ema696))
}

#[cfg(test)]
mod tests {
    use super::super::{candle_patterns, SignalState};
    use super::*;
    use crate::app::tradingview_velocity_parity::model::{ParityRuleVersion, SignalFamily};

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

    fn point(
        volume_event: bool,
        volume_ratio: Option<f64>,
        rsi: f64,
        atr: f64,
        ema596: f64,
        ema696: f64,
    ) -> IndicatorPoint {
        IndicatorPoint {
            filtered_volume_ratio: volume_ratio,
            volume_event,
            rsi14: Some(rsi),
            ema12: Some(46.10),
            ema144: Some(46.20),
            ema596: Some(ema596),
            ema696: Some(ema696),
            atr14: Some(atr),
            ..IndicatorPoint::default()
        }
    }

    fn flat_fixture(length: usize) -> (Vec<Candle>, IndicatorSeries) {
        let candles = (0..length)
            .map(|index| candle(index, 101.0, 101.4, 100.8, 101.2))
            .collect();
        let points = (0..length)
            .map(|_| point(false, None, 60.0, 1.0, 100.0, 99.0))
            .collect();
        (candles, IndicatorSeries { points })
    }

    #[test]
    fn ltc_target_is_v5_rsi_short_but_v8_blocks_the_age_zero_reclaim() {
        let candles = vec![
            candle(0, 46.50, 46.52, 46.40, 46.46),
            candle(1, 46.46, 46.80, 46.46, 46.58),
        ];
        let indicators = IndicatorSeries {
            points: vec![
                point(false, None, 50.0, 0.11, 46.50, 46.43),
                point(
                    true,
                    Some(9.490_995),
                    72.049,
                    0.109_79,
                    46.477_085,
                    46.425_704,
                ),
            ],
        };
        assert!(candle_patterns(&candles, 1).long_upper_shadow);
        assert!(fresh_slow_ema_band_reclaim(&candles, &indicators, 1));

        let mut v5 = SignalState::default();
        v5.evaluate(
            &candles,
            &indicators,
            0,
            0.01,
            None,
            false,
            ParityRuleVersion::CandidateV5,
        );
        let v5_result = v5.evaluate(
            &candles,
            &indicators,
            1,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV5,
        );
        assert!(v5_result
            .intent
            .is_some_and(|intent| { intent.families == vec![SignalFamily::RsiOverboughtPattern] }));

        let mut v8 = SignalState::default();
        v8.evaluate(
            &candles,
            &indicators,
            0,
            0.01,
            None,
            false,
            ParityRuleVersion::CandidateV8,
        );
        let v8_result = v8.evaluate(
            &candles,
            &indicators,
            1,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV8,
        );
        assert!(v8_result.intent.is_none());
        assert!(v8_result
            .blocked
            .iter()
            .any(|blocked| { blocked.reason == V8_BLOCK_REASON }));
    }

    #[test]
    fn bearish_engulfing_without_long_upper_shadow_remains_tradeable() {
        let candles = vec![
            candle(0, 100.0, 100.1, 99.7, 99.8),
            candle(1, 99.8, 101.3, 99.8, 101.2),
            candle(2, 101.0, 101.2, 100.9, 101.1),
            candle(3, 101.1, 101.2, 100.8, 100.9),
        ];
        let indicators = IndicatorSeries {
            points: vec![
                point(false, None, 50.0, 1.0, 100.0, 99.0),
                point(true, Some(6.0), 60.0, 1.0, 100.0, 99.0),
                point(false, None, 65.0, 1.0, 100.0, 99.0),
                point(true, Some(3.0), 72.0, 1.0, 100.0, 99.0),
            ],
        };
        let patterns = candle_patterns(&candles, 3);
        assert!(patterns.bearish_engulfing);
        assert!(!patterns.long_upper_shadow);
        assert!(fresh_slow_ema_band_reclaim(&candles, &indicators, 3));

        let mut state = SignalState::default();
        for index in 0..3 {
            state.evaluate(
                &candles,
                &indicators,
                index,
                0.1,
                None,
                false,
                ParityRuleVersion::CandidateV8,
            );
        }
        let result = state.evaluate(
            &candles,
            &indicators,
            3,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV8,
        );
        assert!(result
            .intent
            .is_some_and(|intent| { intent.families == vec![SignalFamily::RsiOverboughtPattern] }));
        assert!(!result
            .blocked
            .iter()
            .any(|blocked| blocked.reason == V8_BLOCK_REASON));
    }

    #[test]
    fn reclaim_age_four_is_protected_but_age_five_is_not() {
        let (mut candles, mut indicators) = flat_fixture(7);
        candles[0].close = 99.8;
        candles[1] = candle(1, 99.8, 101.5, 99.8, 101.2);
        indicators.points[1] = point(true, Some(6.0), 60.0, 1.0, 100.0, 99.0);

        assert!(fresh_slow_ema_band_reclaim(&candles, &indicators, 5));
        assert!(!fresh_slow_ema_band_reclaim(&candles, &indicators, 6));
    }

    #[test]
    fn equality_and_wick_only_boundaries_follow_the_manifest() {
        let candles = vec![
            candle(0, 100.0, 100.2, 99.8, 100.0),
            candle(1, 100.0, 102.0, 99.9, 101.0),
        ];
        let mut indicators = IndicatorSeries {
            points: vec![
                point(false, None, 60.0, 1.0, 100.0, 99.0),
                point(true, Some(6.0), 60.0, 1.0, 100.0, 99.0),
            ],
        };
        assert!(fresh_slow_ema_band_reclaim(&candles, &indicators, 1));

        indicators.points[1].ema596 = Some(101.0);
        assert!(!fresh_slow_ema_band_reclaim(&candles, &indicators, 1));

        let wick_only = vec![candles[0], candle(1, 99.8, 102.0, 99.7, 99.9)];
        indicators.points[1].ema596 = Some(100.0);
        assert!(!fresh_slow_ema_band_reclaim(&wick_only, &indicators, 1));
    }

    #[test]
    fn close_back_below_releases_until_a_new_strong_reclaim() {
        let (mut candles, mut indicators) = flat_fixture(5);
        candles[0].close = 99.8;
        candles[1] = candle(1, 99.8, 101.5, 99.8, 101.2);
        indicators.points[1] = point(true, Some(6.0), 60.0, 1.0, 100.0, 99.0);
        candles[3].close = 99.9;

        assert!(!fresh_slow_ema_band_reclaim(&candles, &indicators, 3));
        candles[4] = candle(4, 99.9, 101.6, 99.9, 101.2);
        indicators.points[4] = point(true, Some(6.0), 60.0, 1.0, 100.0, 99.0);
        assert!(fresh_slow_ema_band_reclaim(&candles, &indicators, 4));
    }
}
