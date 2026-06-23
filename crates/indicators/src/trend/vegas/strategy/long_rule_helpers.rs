impl VegasStrategy {
    /// 判断shouldblock上边界成交量toofarbollingershortlong，为回测策略流程提供明确的布尔结果。
    fn should_block_high_volume_too_far_bollinger_short_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && leg.is_bullish_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_macd_near_zero_weak_hammer_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let volume = &vegas_indicator_signal_values.volume_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        ema_distance.state == EmaDistanceState::TooFar
            && ema_values.is_short_trend
            && hammer.is_short_signal
            && !engulfing.is_valid_engulfing
            && macd.histogram.abs() < 2.0
            && volume.volume_ratio < 1.0
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_too_far_uptrend_opposing_hammer_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        ema_distance.state == EmaDistanceState::TooFar
            && ema_touch.is_uptrend
            && ema_values.is_long_trend
            && !ema_values.is_short_trend
            && !fib.in_zone
            && boll.is_short_signal
            && !boll.is_long_signal
            && leg.is_bullish_leg
            && !leg.is_bearish_leg
            && !leg.is_new_leg
            && hammer.is_short_signal
            && !engulfing.is_valid_engulfing
            && macd.histogram > 0.0
            && rsi >= 55.0
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_high_volume_no_trend_bollinger_long_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::Normal
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && leg.is_bearish_leg
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_high_volume_conflicting_bollinger_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                boll.is_long_signal
                    && boll.is_short_signal
                    && leg.is_bullish_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_high_volume_internal_down_counter_trend_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                !ema_values.is_short_trend
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && market.internal_trend == -1
                    && !macd.above_zero
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_high_volume_ranging_recovery_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_RANGING_RECOVERY_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::Ranging
                    && engulfing.is_valid_engulfing
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && !macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_high_volume_high_rsi_bollinger_short_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && matches!(
                        ema_distance.state,
                        EmaDistanceState::Normal | EmaDistanceState::Ranging
                    )
                    && boll.is_short_signal
                    && macd.above_zero
                    && leg.is_bullish_leg
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && volume.volume_ratio >= 4.0
                    && rsi >= 65.0
                    && !engulfing.is_valid_engulfing
                    && !hammer.is_long_signal
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_deep_negative_no_trend_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_NO_TREND_HAMMER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                boll.is_long_signal
                    && hammer.is_long_signal
                    && !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && volume.volume_ratio < 2.1
                    && macd.macd_line < -60.0
                    && macd.signal_line < -60.0
                    && macd.histogram < 0.0
                    && macd.histogram_increasing
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_short_trend_too_far_bollinger_short_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_SHORT_TREND_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_long_signal
                    && boll.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.2
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_short_trend_new_bull_leg_counter_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
    ) -> bool {
        let mode = env_string("VEGAS_SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let histogram_ratio = if signal_price > 0.0 {
            macd.histogram.abs() / signal_price
        } else {
            0.0
        };
        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && leg.is_bullish_leg
                    && leg.is_new_leg
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !fib.volume_confirmed
                    && !boll.is_long_signal
                    && volume.volume_ratio < 1.5
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            "v2" => {
                ema_values.is_short_trend
                    && leg.is_bullish_leg
                    && leg.is_new_leg
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !fib.volume_confirmed
                    && !boll.is_long_signal
                    && volume.volume_ratio < 1.5
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && macd.histogram > 0.0
                    && histogram_ratio >= 0.0015
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_short_trend_no_bollinger_rebound_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && leg.is_bullish_leg
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !fib.volume_confirmed
                    && !boll.is_long_signal
                    && !boll.is_short_signal
                    && macd.above_zero
                    && volume.volume_ratio < 1.5
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_normal_bull_leg_no_confirm_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::Normal
                    && leg.is_bullish_leg
                    && !leg.is_bearish_leg
                    && !boll.is_long_signal
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.5
                    && macd.histogram > 0.0
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_above_zero_no_trend_engulfing_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.5
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            "v2" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && !ema_touch.is_long_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.0
                    && rsi >= 70.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_protect_long_trend_deep_negative_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_DEEP_NEGATIVE_HAMMER_PROTECT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && hammer.is_long_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.6
                    && rsi < 40.0
                    && macd.macd_line < -30.0
                    && macd.signal_line < 0.0
                    && macd.histogram < -20.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_long_trend_below_zero_fib_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_BELOW_ZERO_FIB_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && !ema_touch.is_long_signal
                    && !boll.is_long_signal
                    && !boll.is_short_signal
                    && !engulfing.is_valid_engulfing
                    && !hammer.is_long_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && fib.is_long_signal
                    && fib.retracement_ratio < 0.5
                    && !macd.above_zero
                    && market.internal_trend < 0
                    && volume.volume_ratio < 2.1
                    && (40.0..46.0).contains(&rsi)
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_high_level_sideways_chase_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode =
            env_string("VEGAS_HIGH_LEVEL_SIDEWAYS_LONG_BLOCK").unwrap_or_else(|| "off".to_string());
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
        ema_values.is_long_trend
            && !ema_values.is_short_trend
            && !ema_touch.is_long_signal
            && engulfing.is_valid_engulfing
            && leg.is_bullish_leg
            && !leg.is_new_leg
            && !fib.in_zone
            && !fib.volume_confirmed
            && fib.retracement_ratio >= 0.75
            && volume.volume_ratio < 1.6
            && hammer.body_ratio < 0.55
            && boll.is_short_signal
            && !boll.is_long_signal
            && macd.above_zero
            && macd.histogram > 0.0
            && rsi < 60.0
            && !market.internal_bullish_bos
            && !market.swing_bullish_bos
            && !market
                .internal_high
                .as_ref()
                .is_some_and(|pivot| pivot.crossed)
            && !market
                .swing_high
                .as_ref()
                .is_some_and(|pivot| pivot.crossed)
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_above_zero_high_level_chase_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && fib.retracement_ratio >= 0.9
                    && volume.volume_ratio < 1.2
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && rsi >= 68.0
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.histogram > 0.0
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && !market
                        .internal_high
                        .as_ref()
                        .is_some_and(|pivot| pivot.crossed)
                    && !market
                        .swing_high
                        .as_ref()
                        .is_some_and(|pivot| pivot.crossed)
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_protect_above_zero_high_level_chase_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_PROTECT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && !ema_values.is_long_trend
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !fib.in_zone
                    && fib.retracement_ratio >= 0.9
                    && volume.volume_ratio < 1.0
                    && boll.is_short_signal
                    && rsi >= 70.0
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.histogram > 0.0
            }
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn is_deep_negative_hammer_long_candidate(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        boll.is_long_signal
            && hammer.is_long_signal
            && !ema_touch.is_long_signal
            && !engulfing.is_valid_engulfing
            && !fib.volume_confirmed
            && volume.volume_ratio < 1.5
            && rsi < 40.0
            && macd.macd_line < -30.0
            && macd.signal_line < -10.0
            && macd.histogram < -20.0
            && (ema_values.is_short_trend || ema_values.is_long_trend)
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_deep_negative_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_HAMMER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        match mode.as_str() {
            "v1" => Self::is_deep_negative_hammer_long_candidate(vegas_indicator_signal_values),
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_protect_deep_negative_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_HAMMER_LONG_PROTECT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }
        match mode.as_str() {
            "v1" => Self::is_deep_negative_hammer_long_candidate(vegas_indicator_signal_values),
            _ => false,
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn is_repair_long_candidate(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        vegas_indicator_signal_values.ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !vegas_indicator_signal_values.fib_retracement_value.in_zone
            && vegas_indicator_signal_values
                .kline_hammer_value
                .is_long_signal
            && valid_rsi_value.is_some_and(|rsi| rsi < 45.0)
            && vegas_indicator_signal_values.macd_value.histogram < 0.0
            && vegas_indicator_signal_values
                .macd_value
                .histogram_increasing
            && vegas_indicator_signal_values.volume_value.volume_ratio <= 1.6
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let histogram = vegas_indicator_signal_values.macd_value.histogram;
        let hammer_body_ratio = vegas_indicator_signal_values.kline_hammer_value.body_ratio;
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        vegas_indicator_signal_values.ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !vegas_indicator_signal_values.fib_retracement_value.in_zone
            && vegas_indicator_signal_values
                .kline_hammer_value
                .is_long_signal
            && vegas_indicator_signal_values
                .leg_detection_value
                .is_bearish_leg
            && vegas_indicator_signal_values.leg_detection_value.is_new_leg
            && valid_rsi_value.is_some_and(|rsi| rsi < 45.0)
            && (0.0..=3.0).contains(&histogram)
            && hammer_body_ratio >= 0.15
            && (1.5..=3.0).contains(&volume_ratio)
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_recent_upper_shadow_pressure_long(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode =
            env_string("VEGAS_RECENT_UPPER_SHADOW_LONG_BLOCK").unwrap_or_else(|| "off".to_string());
        if mode == "off" || data_items.len() < 4 {
            return false;
        }
        let current = data_items.last().expect("数据不能为空");
        let prev_1 = &data_items[data_items.len() - 2];
        let prev_2 = &data_items[data_items.len() - 3];
        let prev_3 = &data_items[data_items.len() - 4];
        let has_recent_upper_shadow_pressure = [(prev_2, prev_3), (prev_1, prev_2)]
            .into_iter()
            .any(|(candidate, prev)| {
                candidate.up_shadow_ratio() >= 0.18
                    && candidate.v > prev.v * 1.2
                    && candidate.body_ratio() < 0.75
            });
        if !has_recent_upper_shadow_pressure {
            return false;
        }
        let current_is_strong_breakout = current.c > current.o
            && current.body_ratio() >= 0.65
            && current.v > prev_1.v.max(prev_2.v);
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        match mode.as_str() {
            "v3" => {
                has_recent_upper_shadow_pressure
                    && !current_is_strong_breakout
                    && current.v < prev_1.v.max(prev_2.v)
                    && !ema_values.is_long_trend
                    && ema_distance.should_filter_long
                    && boll.is_short_signal
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && fib.retracement_ratio > 0.90
                    && rsi > 60.0
            }
            "v2" => {
                has_recent_upper_shadow_pressure
                    && !current_is_strong_breakout
                    && current.v < prev_1.v.max(prev_2.v)
                    && !ema_values.is_long_trend
                    && ema_distance.should_filter_long
                    && boll.is_short_signal
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && rsi > 60.0
            }
            _ => {
                !current_is_strong_breakout
                    && (boll.is_short_signal || !fib.in_zone || !fib.volume_confirmed)
            }
        }
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn is_rebound_protect_long_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let Some(last) = data_items.last() else {
            return false;
        };
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        if !hammer.is_hammer || !hammer.is_long_signal || !boll.is_long_signal || last.c <= last.o {
            return false;
        }
        let strong_hammer = hammer.down_shadow_ratio >= 0.70 && hammer.body_ratio <= 0.12;
        if !strong_hammer {
            return false;
        }
        let recent_high = data_items
            .iter()
            .rev()
            .skip(1)
            .take(6)
            .map(|c| c.h)
            .fold(last.h, f64::max);
        let pullback_pct = if recent_high > 0.0 {
            (recent_high - last.l) / recent_high
        } else {
            0.0
        };
        if pullback_pct < 0.01 {
            return false;
        }
        true
    }
}
