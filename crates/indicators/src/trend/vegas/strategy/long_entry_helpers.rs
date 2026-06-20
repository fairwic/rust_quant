impl VegasStrategy {
    fn is_expansion_continuation_long_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let Some(last) = data_items.last() else {
            return false;
        };

        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd_val = &vegas_indicator_signal_values.macd_value;
        let leg_val = &vegas_indicator_signal_values.leg_detection_value;
        let structure_val = &vegas_indicator_signal_values.market_structure_value;
        let fib_val = &vegas_indicator_signal_values.fib_retracement_value;

        last.c > last.o
            && last.body_ratio() >= 0.65
            && volume_ratio >= 3.0
            && valid_rsi_value.is_some_and(|rsi| (55.0..=72.0).contains(&rsi))
            && macd_val.macd_line > 0.0
            && macd_val.signal_line > 0.0
            && macd_val.macd_line > macd_val.signal_line
            && macd_val.histogram > 0.0
            && macd_val.histogram_increasing
            && !vegas_indicator_signal_values.ema_values.is_short_trend
            && fib_val.in_zone
            && fib_val.volume_confirmed
            && (leg_val.is_bullish_leg || structure_val.internal_bullish_bos)
    }

    fn should_block_weak_breakout_no_trend_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_WEAK_BREAKOUT_NO_TREND_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && !engulfing.is_valid_engulfing
                    && !hammer.is_long_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && fib.in_zone
                    && fib.volume_confirmed
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && volume.volume_ratio >= 2.5
                    && rsi >= 58.0
                    && macd.above_zero
                    && macd.is_golden_cross
                    && macd.histogram > 0.0
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_ranging_no_trend_weak_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_RANGING_NO_TREND_WEAK_HAMMER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::Ranging
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && hammer.is_long_signal
                    && !hammer.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !fib.volume_confirmed
                    && !macd.above_zero
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && rsi < 45.0
                    && volume.volume_ratio < 1.5
            }
            _ => false,
        }
    }
}
