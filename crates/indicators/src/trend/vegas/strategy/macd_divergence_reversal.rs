/// MACD 背离经 fresh internal CHoCH 确认后交给主流程的方向与结构止损。
#[derive(Debug, Clone, Copy, PartialEq)]
struct MacdDivergenceReversalDecision {
    /// 背离与 CHoCH 共同确认的交易方向。
    direction: SignalDirect,
    /// 冲击柱和确认柱极值之外的失效止损。
    protective_stop: f64,
    /// 背离冲击柱距离当前确认柱的已完成 K 线数量。
    shock_bars_ago: usize,
}

const MACD_DIVERGENCE_REFERENCE_BARS: usize = 20;
const MACD_DIVERGENCE_MAX_CONFIRMATION_BARS: usize = 2;
const MACD_DIVERGENCE_MIN_PRICE_EXTENSION_RATIO: f64 = 0.001;
const MACD_DIVERGENCE_MAX_HISTOGRAM_RATIO: f64 = 0.80;
const MACD_DIVERGENCE_SHOCK_CLOSE_BOUNDARY: f64 = 0.40;
const MACD_DIVERGENCE_STOP_BUFFER_RATIO: f64 = 0.006;
const MACD_DIVERGENCE_SWING_LENGTH: usize = 12;
const MACD_DIVERGENCE_INTERNAL_LENGTH: usize = 2;
const MACD_DIVERGENCE_SWING_THRESHOLD: f64 = 0.015;
const MACD_DIVERGENCE_INTERNAL_THRESHOLD: f64 = 0.015;

impl VegasStrategy {
    /// 从与 MACD 完全对齐的已完成 K 线中提取背离快照。
    ///
    /// 冲击柱必须先于当前确认柱，且只与冲击柱之前的 20 根比较；这样新高/新低与
    /// 动量收缩都在 CHoCH 发生前已经成立，不会把确认后的路径反写成入场特征。
    fn calculate_macd_divergence_value(
        data_items: &[CandleItem],
        histograms: &[f64],
    ) -> MacdDivergenceSignalValue {
        let mut value = MacdDivergenceSignalValue::default();
        if data_items.len() != histograms.len()
            || data_items.len()
                < MACD_DIVERGENCE_REFERENCE_BARS + MACD_DIVERGENCE_MAX_CONFIRMATION_BARS
        {
            return value;
        }

        let current_index = data_items.len() - 1;
        for shock_bars_ago in 1..=MACD_DIVERGENCE_MAX_CONFIRMATION_BARS {
            let Some(shock_index) = current_index.checked_sub(shock_bars_ago) else {
                continue;
            };
            let Some(reference_start) = shock_index.checked_sub(MACD_DIVERGENCE_REFERENCE_BARS)
            else {
                continue;
            };
            let shock = &data_items[shock_index];
            let shock_range = shock.h - shock.l;
            if !shock_range.is_finite() || shock_range <= 0.0 {
                continue;
            }
            let close_position = (shock.c - shock.l) / shock_range;
            let reference_indices = reference_start..shock_index;
            let reference_high_index = reference_indices
                .clone()
                .max_by(|left, right| data_items[*left].h.total_cmp(&data_items[*right].h));
            let reference_low_index = reference_indices
                .min_by(|left, right| data_items[*left].l.total_cmp(&data_items[*right].l));

            if !value.bearish_divergence {
                if let Some(reference_index) = reference_high_index {
                    let reference_high = data_items[reference_index].h;
                    let reference_histogram = histograms[reference_index];
                    let shock_histogram = histograms[shock_index];
                    let price_extended = shock.h
                        > reference_high * (1.0 + MACD_DIVERGENCE_MIN_PRICE_EXTENSION_RATIO);
                    let momentum_not_confirmed = reference_histogram > 0.0
                        && shock_histogram
                            <= reference_histogram * MACD_DIVERGENCE_MAX_HISTOGRAM_RATIO;
                    let high_close = close_position >= 1.0 - MACD_DIVERGENCE_SHOCK_CLOSE_BOUNDARY;
                    if price_extended && momentum_not_confirmed && high_close {
                        value.bearish_divergence = true;
                        value.bearish_shock_bars_ago = Some(shock_bars_ago);
                        value.bearish_reference_high = reference_high;
                        value.bearish_shock_high = shock.h;
                        value.bearish_reference_histogram = reference_histogram;
                        value.bearish_shock_histogram = shock_histogram;
                    }
                }
            }

            if !value.bullish_divergence {
                if let Some(reference_index) = reference_low_index {
                    let reference_low = data_items[reference_index].l;
                    let reference_histogram = histograms[reference_index];
                    let shock_histogram = histograms[shock_index];
                    let price_extended =
                        shock.l < reference_low * (1.0 - MACD_DIVERGENCE_MIN_PRICE_EXTENSION_RATIO);
                    let momentum_not_confirmed = reference_histogram < 0.0
                        && shock_histogram
                            >= reference_histogram * MACD_DIVERGENCE_MAX_HISTOGRAM_RATIO;
                    let low_close = close_position <= MACD_DIVERGENCE_SHOCK_CLOSE_BOUNDARY;
                    if price_extended && momentum_not_confirmed && low_close {
                        value.bullish_divergence = true;
                        value.bullish_shock_bars_ago = Some(shock_bars_ago);
                        value.bullish_reference_low = reference_low;
                        value.bullish_shock_low = shock.l;
                        value.bullish_reference_histogram = reference_histogram;
                        value.bullish_shock_histogram = shock_histogram;
                    }
                }
            }

            if value.bullish_divergence && value.bearish_divergence {
                break;
            }
        }
        value
    }

    /// 只在背离之后的当前完成柱产生 fresh internal CHoCH 时确认反转。
    fn macd_divergence_reversal_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<MacdDivergenceReversalDecision> {
        let config = self.macd_divergence_reversal;
        if !config.is_open || !self.macd_signal.is_some_and(|macd| macd.is_open) {
            return None;
        }
        let current = data_items.last()?;
        let divergence = values.macd_divergence_value;
        let structure = &values.macd_divergence_structure_value;

        let long_decision = config
            .enable_long
            .then(|| {
                let shock_bars_ago = divergence.bullish_shock_bars_ago?;
                let shock_index = data_items.len().checked_sub(1 + shock_bars_ago)?;
                let shock = data_items.get(shock_index)?;
                let shock_midpoint = (shock.h + shock.l) / 2.0;
                if !divergence.bullish_divergence
                    || !structure.internal_bullish_choch
                    || current.c <= current.o
                    || current.c <= shock_midpoint
                    || !values.macd_value.histogram_increasing
                    || values.macd_value.histogram <= divergence.bullish_shock_histogram
                {
                    return None;
                }
                let invalidation_low = shock.l.min(current.l);
                Some(MacdDivergenceReversalDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop: (invalidation_low * (1.0 - MACD_DIVERGENCE_STOP_BUFFER_RATIO))
                        .max(0.0),
                    shock_bars_ago,
                })
            })
            .flatten();

        let short_decision = config
            .enable_short
            .then(|| {
                let shock_bars_ago = divergence.bearish_shock_bars_ago?;
                let shock_index = data_items.len().checked_sub(1 + shock_bars_ago)?;
                let shock = data_items.get(shock_index)?;
                let shock_midpoint = (shock.h + shock.l) / 2.0;
                if !divergence.bearish_divergence
                    || !structure.internal_bearish_choch
                    || current.c >= current.o
                    || current.c >= shock_midpoint
                    || !values.macd_value.histogram_decreasing
                    || values.macd_value.histogram >= divergence.bearish_shock_histogram
                {
                    return None;
                }
                let invalidation_high = shock.h.max(current.h);
                Some(MacdDivergenceReversalDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop: invalidation_high * (1.0 + MACD_DIVERGENCE_STOP_BUFFER_RATIO),
                    shock_bars_ago,
                })
            })
            .flatten();

        // 同一确认柱若产生方向冲突，宁可拒绝也不依赖调用顺序隐式选边。
        match (long_decision, short_decision) {
            (Some(decision), None) | (None, Some(decision)) => Some(decision),
            _ => None,
        }
    }
}

#[cfg(test)]
mod macd_divergence_reversal_tests {
    use super::*;

    fn candle(o: f64, h: f64, l: f64, c: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            v: 10.0,
            ts,
            confirm: 1,
        }
    }

    fn strategy() -> VegasStrategy {
        VegasStrategy {
            macd_signal: Some(MacdSignalConfig::default()),
            macd_divergence_reversal: MacdDivergenceReversalConfig {
                is_open: true,
                enable_long: true,
                enable_short: true,
            },
            ..VegasStrategy::default()
        }
    }

    fn bearish_case() -> (Vec<CandleItem>, Vec<f64>) {
        let mut candles = (0..20)
            .map(|index| candle(100.0, 101.0, 99.0, 100.0, index))
            .collect::<Vec<_>>();
        let mut histograms = vec![0.5; 20];
        candles[10] = candle(108.0, 110.0, 107.0, 109.0, 10);
        histograms[10] = 2.0;
        candles.push(candle(108.0, 111.0, 107.0, 110.0, 20));
        histograms.push(1.0);
        candles.push(candle(110.0, 110.5, 103.0, 104.0, 21));
        histograms.push(0.4);
        (candles, histograms)
    }

    fn bullish_case() -> (Vec<CandleItem>, Vec<f64>) {
        let mut candles = (0..20)
            .map(|index| candle(100.0, 101.0, 99.0, 100.0, index))
            .collect::<Vec<_>>();
        let mut histograms = vec![-0.5; 20];
        candles[10] = candle(92.0, 93.0, 90.0, 91.0, 10);
        histograms[10] = -2.0;
        candles.push(candle(92.0, 93.0, 89.0, 90.0, 20));
        histograms.push(-1.0);
        candles.push(candle(90.0, 96.5, 89.5, 96.0, 21));
        histograms.push(-0.4);
        (candles, histograms)
    }

    #[test]
    fn bearish_divergence_requires_new_high_with_weaker_positive_histogram() {
        let (candles, histograms) = bearish_case();
        let value = VegasStrategy::calculate_macd_divergence_value(&candles, &histograms);

        assert!(value.bearish_divergence);
        assert_eq!(value.bearish_shock_bars_ago, Some(1));
        assert_eq!(value.bearish_reference_high, 110.0);
        assert_eq!(value.bearish_shock_high, 111.0);
        assert_eq!(value.bearish_reference_histogram, 2.0);
        assert_eq!(value.bearish_shock_histogram, 1.0);
    }

    #[test]
    fn bullish_divergence_requires_new_low_with_contracting_negative_histogram() {
        let (candles, histograms) = bullish_case();
        let value = VegasStrategy::calculate_macd_divergence_value(&candles, &histograms);

        assert!(value.bullish_divergence);
        assert_eq!(value.bullish_shock_bars_ago, Some(1));
        assert_eq!(value.bullish_reference_low, 90.0);
        assert_eq!(value.bullish_shock_low, 89.0);
        assert_eq!(value.bullish_reference_histogram, -2.0);
        assert_eq!(value.bullish_shock_histogram, -1.0);
    }

    #[test]
    fn fresh_bearish_choch_confirms_only_after_the_divergent_high() {
        let (candles, histograms) = bearish_case();
        let mut values = VegasIndicatorSignalValue {
            macd_divergence_value: VegasStrategy::calculate_macd_divergence_value(
                &candles,
                &histograms,
            ),
            ..VegasIndicatorSignalValue::default()
        };
        values
            .macd_divergence_structure_value
            .internal_bearish_choch = true;
        values.macd_value.histogram = 0.4;
        values.macd_value.histogram_decreasing = true;

        let decision = strategy()
            .macd_divergence_reversal_decision(&candles, &values)
            .expect("fresh bearish CHoCH should confirm the prior divergence");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert_eq!(decision.shock_bars_ago, 1);
        assert!((decision.protective_stop - 111.666).abs() < 1e-9);
    }

    #[test]
    fn fresh_bullish_choch_confirms_only_after_the_divergent_low() {
        let (candles, histograms) = bullish_case();
        let mut values = VegasIndicatorSignalValue {
            macd_divergence_value: VegasStrategy::calculate_macd_divergence_value(
                &candles,
                &histograms,
            ),
            ..VegasIndicatorSignalValue::default()
        };
        values
            .macd_divergence_structure_value
            .internal_bullish_choch = true;
        values.macd_value.histogram = -0.4;
        values.macd_value.histogram_increasing = true;

        let decision = strategy()
            .macd_divergence_reversal_decision(&candles, &values)
            .expect("fresh bullish CHoCH should confirm the prior divergence");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert_eq!(decision.shock_bars_ago, 1);
        assert!((decision.protective_stop - 88.466).abs() < 1e-9);
    }

    #[test]
    fn active_bos_or_missing_midpoint_reclaim_cannot_replace_fresh_choch() {
        let (mut candles, histograms) = bearish_case();
        let mut values = VegasIndicatorSignalValue {
            macd_divergence_value: VegasStrategy::calculate_macd_divergence_value(
                &candles,
                &histograms,
            ),
            ..VegasIndicatorSignalValue::default()
        };
        values
            .macd_divergence_structure_value
            .internal_bearish_bos_active = true;
        values.macd_value.histogram = 0.4;
        values.macd_value.histogram_decreasing = true;
        assert!(strategy()
            .macd_divergence_reversal_decision(&candles, &values)
            .is_none());

        values
            .macd_divergence_structure_value
            .internal_bearish_choch = true;
        candles.last_mut().expect("confirmation candle").c = 109.5;
        assert!(strategy()
            .macd_divergence_reversal_decision(&candles, &values)
            .is_none());
    }
}
