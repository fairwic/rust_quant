/// 冻结 Donchian 通道放量突破的方向与 ATR 保护位。
#[derive(Debug, Clone, Copy)]
struct DonchianVolumeBreakoutDecision {
    direction: SignalDirect,
    protective_stop: f64,
}

impl VegasStrategy {
    /// 识别当前完成 K 线对前 20 根冻结通道的 2x 放量收盘突破。
    fn donchian_volume_breakout_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<DonchianVolumeBreakoutDecision> {
        const CHANNEL_LOOKBACK: usize = 20;
        const BREAKOUT_VOLUME_RATIO: f64 = 2.0;
        const STOP_ATR_MULTIPLIER: f64 = 2.0;

        let config = self.donchian_volume_breakout;
        if !config.is_open || data_items.len() < CHANNEL_LOOKBACK + 1 {
            return None;
        }
        let current_index = data_items.len() - 1;
        let history_start = current_index - CHANNEL_LOOKBACK;
        let history = &data_items[history_start..current_index];
        let current = data_items.last()?;
        if current.confirm != 1 || history.iter().any(|candle| candle.confirm != 1) {
            return None;
        }

        let channel_high = history
            .iter()
            .map(|candle| candle.h)
            .fold(f64::NEG_INFINITY, f64::max);
        let channel_low = history
            .iter()
            .map(|candle| candle.l)
            .fold(f64::INFINITY, f64::min);
        let average_volume = history.iter().map(|candle| candle.v).sum::<f64>()
            / CHANNEL_LOOKBACK as f64;
        let atr = values.cross_asset_adaptive_value.atr_value;
        if !channel_high.is_finite()
            || !channel_low.is_finite()
            || channel_high <= channel_low
            || !average_volume.is_finite()
            || average_volume <= 0.0
            || !current.v.is_finite()
            || current.v < average_volume * BREAKOUT_VOLUME_RATIO
            || !atr.is_finite()
            || atr <= 0.0
        {
            return None;
        }

        let opened_inside_channel = current.o >= channel_low && current.o <= channel_high;
        if !opened_inside_channel {
            return None;
        }
        if config.enable_long && current.c > channel_high && current.c > current.o {
            let protective_stop = current.c - atr * STOP_ATR_MULTIPLIER;
            if protective_stop.is_finite() && protective_stop > 0.0 && protective_stop < current.c {
                return Some(DonchianVolumeBreakoutDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop,
                });
            }
        }
        if config.enable_short && current.c < channel_low && current.c < current.o {
            let protective_stop = current.c + atr * STOP_ATR_MULTIPLIER;
            if protective_stop.is_finite() && protective_stop > current.c {
                return Some(DonchianVolumeBreakoutDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod donchian_volume_breakout_tests {
    use super::*;

    /// 构造已确认测试 K 线。
    fn candle(o: f64, h: f64, l: f64, c: f64, v: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            v,
            ts,
            confirm: 1,
        }
    }

    /// 构造冻结 20 根、上下边界为 101/99 的通道。
    fn channel_history() -> Vec<CandleItem> {
        (0..20)
            .map(|index| candle(100.0, 101.0, 99.0, 100.0, 10.0, index))
            .collect()
    }

    /// 构造只开启 V76 且 ATR 已就绪的策略与指标快照。
    fn strategy_and_values() -> (VegasStrategy, VegasIndicatorSignalValue) {
        let strategy = VegasStrategy {
            donchian_volume_breakout: DonchianVolumeBreakoutConfig {
                is_open: true,
                ..DonchianVolumeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };
        let mut values = VegasIndicatorSignalValue::default();
        values.cross_asset_adaptive_value.atr_value = 1.0;
        (strategy, values)
    }

    #[test]
    fn bullish_close_breakout_uses_two_atr_protective_stop() {
        let (strategy, values) = strategy_and_values();
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 20.0, 20));

        let decision = strategy
            .donchian_volume_breakout_decision(&candles, &values)
            .expect("completed 2x close breakout should enter long");
        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert_eq!(decision.protective_stop, 100.0);
    }

    #[test]
    fn bearish_close_breakout_is_symmetric() {
        let (strategy, values) = strategy_and_values();
        let mut candles = channel_history();
        candles.push(candle(100.0, 100.2, 97.5, 98.0, 20.0, 20));

        let decision = strategy
            .donchian_volume_breakout_decision(&candles, &values)
            .expect("completed 2x close breakout should enter short");
        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert_eq!(decision.protective_stop, 100.0);
    }

    #[test]
    fn breakout_rejects_below_two_times_volume() {
        let (strategy, values) = strategy_and_values();
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 19.9, 20));

        assert!(strategy
            .donchian_volume_breakout_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn current_high_cannot_raise_frozen_channel_boundary() {
        let (strategy, values) = strategy_and_values();
        let mut first = channel_history();
        first.push(candle(100.0, 102.5, 99.8, 102.0, 20.0, 20));
        let mut second = first.clone();
        second[20].h = 200.0;

        let first_decision = strategy.donchian_volume_breakout_decision(&first, &values);
        let second_decision = strategy.donchian_volume_breakout_decision(&second, &values);
        assert_eq!(
            first_decision.map(|decision| decision.direction),
            second_decision.map(|decision| decision.direction)
        );
    }

    #[test]
    fn gap_open_outside_channel_is_not_backfilled_at_close() {
        let (strategy, values) = strategy_and_values();
        let mut candles = channel_history();
        candles.push(candle(101.5, 103.0, 101.2, 102.5, 20.0, 20));

        assert!(strategy
            .donchian_volume_breakout_decision(&candles, &values)
            .is_none());
    }
}
