/// 冻结价值区上方突破失败后的反向做空决策。
#[derive(Debug, Clone, Copy)]
struct VolumeProfileFailedAuctionDecision {
    protective_stop: f64,
    point_of_control: f64,
}

impl VegasStrategy {
    /// 识别“2x 放量上破 VAH -> 下一根阴线收回 POC 与 VAH 之间”的失败拍卖。
    ///
    /// 价值区只使用突破棒之前的 48 根已完成 K 线，确认棒不会重算边界。
    fn volume_profile_failed_auction_decision(
        &self,
        data_items: &[CandleItem],
    ) -> Option<VolumeProfileFailedAuctionDecision> {
        const PROFILE_LOOKBACK: usize = 48;
        const PRICE_BINS: usize = 24;
        const VALUE_AREA_RATIO: f64 = 0.70;
        const BREAKOUT_VOLUME_RATIO: f64 = 2.0;

        let config = self.volume_profile_failed_auction;
        if !config.is_open || data_items.len() < PROFILE_LOOKBACK + 2 {
            return None;
        }
        let confirmation_index = data_items.len() - 1;
        let breakout_index = confirmation_index - 1;
        let profile_start = breakout_index - PROFILE_LOOKBACK;
        let profile_candles = &data_items[profile_start..breakout_index];
        let breakout = &data_items[breakout_index];
        let confirmation = &data_items[confirmation_index];
        if breakout.confirm != 1
            || confirmation.confirm != 1
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
        let point_of_control = profile.point_of_control;
        if !value_area_high.is_finite()
            || !value_area_low.is_finite()
            || !point_of_control.is_finite()
            || value_area_high <= value_area_low
            || point_of_control < value_area_low
            || point_of_control >= value_area_high
        {
            return None;
        }

        let opened_inside_value_area = breakout.o >= value_area_low && breakout.o <= value_area_high;
        let accepted_above_value_area = opened_inside_value_area
            && breakout.c > value_area_high
            && breakout.c > breakout.o;
        let failed_auction = confirmation.c < confirmation.o
            && confirmation.c <= value_area_high
            && confirmation.c > point_of_control;
        if !accepted_above_value_area || !failed_auction {
            return None;
        }

        let buffer = config
            .stop_loss_buffer_ratio
            .is_finite()
            .then_some(config.stop_loss_buffer_ratio.max(0.0))?;
        let protective_stop = breakout.h.max(confirmation.h) * (1.0 + buffer);
        if !protective_stop.is_finite()
            || protective_stop <= confirmation.c
            || point_of_control >= confirmation.c
            || point_of_control <= 0.0
        {
            return None;
        }
        Some(VolumeProfileFailedAuctionDecision {
            protective_stop,
            point_of_control,
        })
    }
}

#[cfg(test)]
mod volume_profile_failed_auction_tests {
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

    /// 构造只开启 V75 的研究策略。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            volume_profile_failed_auction: VolumeProfileFailedAuctionConfig {
                is_open: true,
                stop_loss_buffer_ratio: 0.006,
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn upper_failed_auction_enters_after_close_returns_inside_value_area() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 103.5, 99.8, 103.0, 20.0, 48));
        candles.push(candle(103.0, 103.2, 100.7, 100.98, 12.0, 49));

        let decision = strategy()
            .volume_profile_failed_auction_decision(&candles)
            .expect("failed auction should enter on the completed confirmation close");
        assert!(decision.protective_stop > candles[49].c);
        assert!(decision.point_of_control < candles[49].c);
    }

    #[test]
    fn failure_close_must_remain_above_frozen_poc() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 103.5, 99.8, 103.0, 20.0, 48));
        candles.push(candle(103.0, 103.2, 99.5, 100.5, 12.0, 49));

        assert!(strategy()
            .volume_profile_failed_auction_decision(&candles)
            .is_none());
    }

    #[test]
    fn breakout_volume_must_reach_two_times_frozen_average() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 103.5, 99.8, 103.0, 19.9, 48));
        candles.push(candle(103.0, 103.2, 100.7, 100.98, 12.0, 49));

        assert!(strategy()
            .volume_profile_failed_auction_decision(&candles)
            .is_none());
    }

    #[test]
    fn current_confirmation_cannot_change_frozen_profile() {
        let mut normal = profile_history();
        normal.push(candle(100.0, 103.5, 99.8, 103.0, 20.0, 48));
        normal.push(candle(103.0, 103.2, 100.7, 100.98, 12.0, 49));
        let mut extreme_volume = normal.clone();
        extreme_volume[49].v = 1_000_000.0;

        let normal_decision = strategy().volume_profile_failed_auction_decision(&normal);
        let extreme_decision = strategy().volume_profile_failed_auction_decision(&extreme_volume);
        assert_eq!(
            normal_decision.map(|decision| decision.point_of_control),
            extreme_decision.map(|decision| decision.point_of_control)
        );
    }
}
