/// MACD 在既有 EMA 趋势侧复位，并由当前完成柱 fresh internal BOS 确认后的入场决策。
#[derive(Debug, Clone, Copy, PartialEq)]
struct MacdTrendResetBosDecision {
    /// 顺势交易方向。
    direction: SignalDirect,
    /// 最近三根完成柱极值之外的结构失效止损。
    protective_stop: f64,
}

const MACD_TREND_RESET_STOP_LOOKBACK_BARS: usize = 3;
const MACD_TREND_RESET_STOP_BUFFER_RATIO: f64 = 0.006;
const MACD_TREND_RESET_SWING_LENGTH: usize = 12;
const MACD_TREND_RESET_INTERNAL_LENGTH: usize = 2;
const MACD_TREND_RESET_SWING_THRESHOLD: f64 = 0.015;
const MACD_TREND_RESET_INTERNAL_THRESHOLD: f64 = 0.015;

impl VegasStrategy {
    /// 仅在 DIF 保持零轴趋势侧、柱体刚完成同向交叉且当前柱 fresh BOS 时补充机会。
    ///
    /// `internal_*_bos_active` 不能替代当前柱的 fresh BOS，否则历史结构状态会在后续
    /// K 线重复触发；TooFar 也直接拒绝，避免把趋势延续误用成末端追涨追跌。
    fn macd_trend_reset_bos_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<MacdTrendResetBosDecision> {
        let config = self.macd_trend_reset_bos;
        if !config.is_open
            || !self.macd_signal.is_some_and(|macd| macd.is_open)
            || values.ema_distance_filter.state == EmaDistanceState::TooFar
            || data_items.len() < MACD_TREND_RESET_STOP_LOOKBACK_BARS
        {
            return None;
        }

        let current = data_items.last()?;
        let ema = values.ema_values;
        let macd = values.macd_value;
        let structure = &values.macd_trend_reset_structure_value;
        let recent = &data_items[data_items.len() - MACD_TREND_RESET_STOP_LOOKBACK_BARS..];

        let long_decision = config.enable_long
            && ema.is_long_trend
            && macd.macd_line > 0.0
            && macd.is_golden_cross
            && structure.internal_bullish_bos
            && current.c > current.o;
        let short_decision = config.enable_short
            && ema.is_short_trend
            && macd.macd_line < 0.0
            && macd.is_death_cross
            && structure.internal_bearish_bos
            && current.c < current.o;

        match (long_decision, short_decision) {
            (true, false) => {
                let invalidation_low = recent.iter().map(|item| item.l).min_by(f64::total_cmp)?;
                Some(MacdTrendResetBosDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop: (invalidation_low
                        * (1.0 - MACD_TREND_RESET_STOP_BUFFER_RATIO))
                        .max(0.0),
                })
            }
            (false, true) => {
                let invalidation_high = recent.iter().map(|item| item.h).max_by(f64::total_cmp)?;
                Some(MacdTrendResetBosDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop: invalidation_high * (1.0 + MACD_TREND_RESET_STOP_BUFFER_RATIO),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod macd_trend_reset_bos_tests {
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
            macd_trend_reset_bos: MacdTrendResetBosConfig {
                is_open: true,
                enable_long: true,
                enable_short: true,
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn long_requires_zero_side_cross_and_fresh_bullish_bos() {
        let candles = vec![
            candle(100.0, 102.0, 99.0, 101.0, 1),
            candle(101.0, 103.0, 100.0, 102.0, 2),
            candle(102.0, 104.0, 101.0, 103.0, 3),
        ];
        let mut values = VegasIndicatorSignalValue::default();
        values.ema_values.is_long_trend = true;
        values.macd_value.macd_line = 1.0;
        values.macd_value.is_golden_cross = true;
        values.macd_trend_reset_structure_value.internal_bullish_bos = true;

        let decision = strategy()
            .macd_trend_reset_bos_decision(&candles, &values)
            .expect("fresh bullish BOS should confirm the trend reset");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 98.406).abs() < 1e-9);
    }

    #[test]
    fn short_uses_three_bar_high_as_structural_invalidation() {
        let candles = vec![
            candle(103.0, 105.0, 102.0, 103.0, 1),
            candle(103.0, 104.0, 100.0, 101.0, 2),
            candle(101.0, 102.0, 98.0, 99.0, 3),
        ];
        let mut values = VegasIndicatorSignalValue::default();
        values.ema_values.is_short_trend = true;
        values.macd_value.macd_line = -1.0;
        values.macd_value.is_death_cross = true;
        values.macd_trend_reset_structure_value.internal_bearish_bos = true;

        let decision = strategy()
            .macd_trend_reset_bos_decision(&candles, &values)
            .expect("fresh bearish BOS should confirm the trend reset");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!((decision.protective_stop - 105.63).abs() < 1e-9);
    }

    #[test]
    fn stale_bos_state_or_too_far_distance_cannot_trigger() {
        let candles = vec![
            candle(100.0, 102.0, 99.0, 101.0, 1),
            candle(101.0, 103.0, 100.0, 102.0, 2),
            candle(102.0, 104.0, 101.0, 103.0, 3),
        ];
        let mut values = VegasIndicatorSignalValue::default();
        values.ema_values.is_long_trend = true;
        values.macd_value.macd_line = 1.0;
        values.macd_value.is_golden_cross = true;
        values
            .macd_trend_reset_structure_value
            .internal_bullish_bos_active = true;

        assert!(strategy()
            .macd_trend_reset_bos_decision(&candles, &values)
            .is_none());

        values.macd_trend_reset_structure_value.internal_bullish_bos = true;
        values.ema_distance_filter.state = EmaDistanceState::TooFar;
        assert!(strategy()
            .macd_trend_reset_bos_decision(&candles, &values)
            .is_none());
    }
}
