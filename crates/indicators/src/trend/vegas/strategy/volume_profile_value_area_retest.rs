/// 固定历史成交量价值区突破回踩生成的方向与结构保护位。
#[derive(Debug, Clone, Copy)]
struct VolumeProfileValueAreaRetestDecision {
    direction: SignalDirect,
    protective_stop: f64,
}

impl VegasStrategy {
    /// 识别“冻结 48 根成交量价值区 -> 2x 放量突破 -> 下一棒回踩并在区外接受”。
    ///
    /// 成交量分布严格截止于突破棒之前，突破棒和当前回踩棒不会反向改变 VAH/VAL。
    fn volume_profile_value_area_retest_decision(
        &self,
        data_items: &[CandleItem],
    ) -> Option<VolumeProfileValueAreaRetestDecision> {
        const PROFILE_LOOKBACK: usize = 48;
        const PRICE_BINS: usize = 24;
        const VALUE_AREA_RATIO: f64 = 0.70;
        const BREAKOUT_VOLUME_RATIO: f64 = 2.0;

        let config = self.volume_profile_value_area_retest;
        if !config.is_open || data_items.len() < PROFILE_LOOKBACK + 2 {
            return None;
        }

        let breakout_index = data_items.len() - 2;
        let profile_start = breakout_index - PROFILE_LOOKBACK;
        let profile_candles = &data_items[profile_start..breakout_index];
        let breakout = &data_items[breakout_index];
        let retest = data_items.last()?;
        if breakout.confirm != 1
            || retest.confirm != 1
            || profile_candles.iter().any(|candle| candle.confirm != 1)
        {
            return None;
        }

        let average_volume = profile_candles
            .iter()
            .map(|candle| candle.v)
            .sum::<f64>()
            / PROFILE_LOOKBACK as f64;
        if !average_volume.is_finite()
            || average_volume <= 0.0
            || !breakout.v.is_finite()
            || breakout.v < average_volume * BREAKOUT_VOLUME_RATIO
        {
            return None;
        }

        let mut indicator = crate::volume::VolumeProfileIndicator::new(
            PROFILE_LOOKBACK,
            PRICE_BINS,
            VALUE_AREA_RATIO,
        );
        let mut profile = None;
        for candle in profile_candles {
            profile = Some(indicator.next(candle));
        }
        let profile = profile?;
        let value_area_high = profile.value_area_high;
        let value_area_low = profile.value_area_low;
        if !value_area_high.is_finite()
            || !value_area_low.is_finite()
            || value_area_high <= value_area_low
        {
            return None;
        }

        let buffer = config
            .stop_loss_buffer_ratio
            .is_finite()
            .then_some(config.stop_loss_buffer_ratio.max(0.0))?;
        let long_confirmed = config.enable_long
            && breakout.o <= value_area_high
            && breakout.c > value_area_high
            && breakout.c > breakout.o
            && retest.l <= value_area_high
            && retest.c > value_area_high
            && retest.c > retest.o;
        if long_confirmed {
            let protective_stop = retest.l * (1.0 - buffer);
            if protective_stop.is_finite()
                && protective_stop > 0.0
                && protective_stop < retest.c
            {
                return Some(VolumeProfileValueAreaRetestDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop,
                });
            }
        }

        let short_confirmed = config.enable_short
            && breakout.o >= value_area_low
            && breakout.c < value_area_low
            && breakout.c < breakout.o
            && retest.h >= value_area_low
            && retest.c < value_area_low
            && retest.c < retest.o;
        if short_confirmed {
            let protective_stop = retest.h * (1.0 + buffer);
            if protective_stop.is_finite() && protective_stop > retest.c {
                return Some(VolumeProfileValueAreaRetestDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop,
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod volume_profile_value_area_retest_tests {
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

    /// 构造价格位于 99—101 的冻结 48 根成交量分布。
    fn profile_history() -> Vec<CandleItem> {
        (0..48)
            .map(|index| candle(100.0, 101.0, 99.0, 100.0, 10.0, index))
            .collect()
    }

    /// 构造只开启 V71 的研究策略。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            volume_profile_value_area_retest: VolumeProfileValueAreaRetestConfig {
                is_open: true,
                ..VolumeProfileValueAreaRetestConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn bullish_value_area_breakout_retest_is_confirmed() {
        let mut candles = profile_history();
        candles.push(candle(100.5, 103.5, 100.0, 103.0, 20.0, 48));
        candles.push(candle(102.0, 103.0, 100.5, 102.5, 10.0, 49));

        let decision = strategy()
            .volume_profile_value_area_retest_decision(&candles)
            .expect("fresh VAH breakout and retest should confirm");
        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!(decision.protective_stop < candles[49].c);
    }

    #[test]
    fn bearish_value_area_breakout_retest_is_symmetric() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 100.5, 97.5, 98.0, 20.0, 48));
        candles.push(candle(98.8, 100.0, 97.8, 98.2, 10.0, 49));

        let decision = strategy()
            .volume_profile_value_area_retest_decision(&candles)
            .expect("fresh VAL breakdown and retest should confirm");
        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.protective_stop > candles[49].c);
    }

    #[test]
    fn setup_requires_full_frozen_profile_window() {
        let mut candles = profile_history();
        candles.remove(0);
        candles.push(candle(100.5, 103.5, 100.0, 103.0, 20.0, 48));
        candles.push(candle(102.0, 103.0, 100.5, 102.5, 10.0, 49));

        assert!(strategy()
            .volume_profile_value_area_retest_decision(&candles)
            .is_none());
    }

    #[test]
    fn breakout_below_two_times_average_volume_is_rejected() {
        let mut candles = profile_history();
        candles.push(candle(100.5, 103.5, 100.0, 103.0, 19.9, 48));
        candles.push(candle(102.0, 103.0, 100.5, 102.5, 10.0, 49));

        assert!(strategy()
            .volume_profile_value_area_retest_decision(&candles)
            .is_none());
    }

    #[test]
    fn breakout_must_start_from_inside_the_frozen_value_area() {
        let mut candles = profile_history();
        candles.push(candle(102.0, 103.5, 101.5, 103.0, 20.0, 48));
        candles.push(candle(102.0, 103.0, 100.5, 102.5, 10.0, 49));

        assert!(strategy()
            .volume_profile_value_area_retest_decision(&candles)
            .is_none());
    }

    #[test]
    fn retest_must_touch_and_close_back_outside_value_area() {
        let mut no_touch = profile_history();
        no_touch.push(candle(100.5, 103.5, 100.0, 103.0, 20.0, 48));
        no_touch.push(candle(102.0, 103.0, 101.5, 102.5, 10.0, 49));
        assert!(strategy()
            .volume_profile_value_area_retest_decision(&no_touch)
            .is_none());

        let mut closed_inside = profile_history();
        closed_inside.push(candle(100.5, 103.5, 100.0, 103.0, 20.0, 48));
        closed_inside.push(candle(101.5, 102.0, 100.0, 100.5, 10.0, 49));
        assert!(strategy()
            .volume_profile_value_area_retest_decision(&closed_inside)
            .is_none());
    }

    #[test]
    fn current_retest_volume_does_not_change_frozen_profile_boundary() {
        let mut normal = profile_history();
        normal.push(candle(100.5, 103.5, 100.0, 103.0, 20.0, 48));
        normal.push(candle(102.0, 103.0, 100.5, 102.5, 10.0, 49));
        let mut extreme_current_volume = normal.clone();
        extreme_current_volume[49].v = 1_000_000.0;

        let normal_decision = strategy().volume_profile_value_area_retest_decision(&normal);
        let extreme_decision = strategy()
            .volume_profile_value_area_retest_decision(&extreme_current_volume);
        assert_eq!(normal_decision.map(|value| value.direction), Some(SignalDirect::IsLong));
        assert_eq!(
            extreme_decision.map(|value| value.direction),
            Some(SignalDirect::IsLong)
        );
    }

    #[test]
    fn unconfirmed_retest_is_rejected() {
        let mut candles = profile_history();
        candles.push(candle(100.5, 103.5, 100.0, 103.0, 20.0, 48));
        let mut retest = candle(102.0, 103.0, 100.5, 102.5, 10.0, 49);
        retest.confirm = 0;
        candles.push(retest);

        assert!(strategy()
            .volume_profile_value_area_retest_decision(&candles)
            .is_none());
    }
}
