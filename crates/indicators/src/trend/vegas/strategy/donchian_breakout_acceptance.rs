/// Donchian 突破后紧邻确认棒继续接受通道外价格的方向与结构保护位。
#[derive(Debug, Clone, Copy)]
struct DonchianBreakoutAcceptanceDecision {
    direction: SignalDirect,
    protective_stop: f64,
}

impl VegasStrategy {
    /// 识别“前 20 根冻结通道 -> 2x 放量突破 -> 下一棒同向收在通道外”。
    fn donchian_breakout_acceptance_decision(
        &self,
        data_items: &[CandleItem],
    ) -> Option<DonchianBreakoutAcceptanceDecision> {
        const CHANNEL_LOOKBACK: usize = 20;
        const BREAKOUT_VOLUME_RATIO: f64 = 2.0;

        let config = self.donchian_breakout_acceptance;
        if !config.is_open || data_items.len() < CHANNEL_LOOKBACK + 2 {
            return None;
        }
        let confirmation_index = data_items.len() - 1;
        let seed_index = confirmation_index - 1;
        let history_start = seed_index - CHANNEL_LOOKBACK;
        let history = &data_items[history_start..seed_index];
        let seed = &data_items[seed_index];
        let confirmation = &data_items[confirmation_index];
        if seed.confirm != 1
            || confirmation.confirm != 1
            || history.iter().any(|candle| candle.confirm != 1)
        {
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
        if !channel_high.is_finite()
            || !channel_low.is_finite()
            || channel_high <= channel_low
            || !average_volume.is_finite()
            || average_volume <= 0.0
            || !seed.v.is_finite()
            || seed.v < average_volume * BREAKOUT_VOLUME_RATIO
        {
            return None;
        }

        let seed_opened_inside = seed.o >= channel_low && seed.o <= channel_high;
        if !seed_opened_inside {
            return None;
        }
        let buffer = config
            .stop_loss_buffer_ratio
            .is_finite()
            .then_some(config.stop_loss_buffer_ratio.max(0.0))?;

        let long_seed = seed.c > channel_high && seed.c > seed.o;
        let long_accepted = confirmation.c > channel_high && confirmation.c > confirmation.o;
        if config.enable_long && long_seed && long_accepted {
            let protective_stop = channel_high * (1.0 - buffer);
            if protective_stop.is_finite()
                && protective_stop > 0.0
                && protective_stop < confirmation.c
            {
                return Some(DonchianBreakoutAcceptanceDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop,
                });
            }
        }

        let short_seed = seed.c < channel_low && seed.c < seed.o;
        let short_accepted = confirmation.c < channel_low && confirmation.c < confirmation.o;
        if config.enable_short && short_seed && short_accepted {
            let protective_stop = channel_low * (1.0 + buffer);
            if protective_stop.is_finite() && protective_stop > confirmation.c {
                return Some(DonchianBreakoutAcceptanceDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod donchian_breakout_acceptance_tests {
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

    /// 构造只开启 V77 的研究策略。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            donchian_breakout_acceptance: DonchianBreakoutAcceptanceConfig {
                is_open: true,
                ..DonchianBreakoutAcceptanceConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn bullish_seed_requires_next_bar_acceptance_outside_frozen_channel() {
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 20.0, 20));
        candles.push(candle(101.8, 103.2, 101.5, 103.0, 9.0, 21));

        let decision = strategy()
            .donchian_breakout_acceptance_decision(&candles)
            .expect("next completed bullish bar should confirm acceptance");
        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!(decision.protective_stop < 101.0);
    }

    #[test]
    fn bearish_seed_requires_next_bar_acceptance_outside_frozen_channel() {
        let mut candles = channel_history();
        candles.push(candle(100.0, 100.2, 97.5, 98.0, 20.0, 20));
        candles.push(candle(98.2, 98.5, 96.8, 97.0, 9.0, 21));

        let decision = strategy()
            .donchian_breakout_acceptance_decision(&candles)
            .expect("next completed bearish bar should confirm acceptance");
        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.protective_stop > 99.0);
    }

    #[test]
    fn close_back_inside_channel_rejects_seed() {
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 20.0, 20));
        candles.push(candle(102.0, 102.2, 100.0, 100.5, 9.0, 21));

        assert!(strategy()
            .donchian_breakout_acceptance_decision(&candles)
            .is_none());
    }

    #[test]
    fn confirmation_does_not_need_second_volume_spike() {
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 20.0, 20));
        candles.push(candle(101.8, 103.2, 101.5, 103.0, 0.1, 21));

        assert!(strategy()
            .donchian_breakout_acceptance_decision(&candles)
            .is_some());
    }

    #[test]
    fn seed_below_two_times_volume_is_rejected() {
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 19.9, 20));
        candles.push(candle(101.8, 103.2, 101.5, 103.0, 9.0, 21));

        assert!(strategy()
            .donchian_breakout_acceptance_decision(&candles)
            .is_none());
    }

    #[test]
    fn only_immediately_adjacent_confirmation_is_eligible() {
        let mut candles = channel_history();
        candles.push(candle(100.0, 102.5, 99.8, 102.0, 20.0, 20));
        candles.push(candle(102.0, 102.2, 100.0, 100.5, 9.0, 21));
        candles.push(candle(100.5, 103.2, 100.4, 103.0, 9.0, 22));

        assert!(strategy()
            .donchian_breakout_acceptance_decision(&candles)
            .is_none());
    }
}
