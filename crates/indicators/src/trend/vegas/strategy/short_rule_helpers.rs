impl VegasStrategy {
    fn is_fake_breakout_reversal_short_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let Some(last) = data_items.last() else {
            return false;
        };

        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd_val = &vegas_indicator_signal_values.macd_value;
        let leg_val = &vegas_indicator_signal_values.leg_detection_value;
        let structure_val = &vegas_indicator_signal_values.market_structure_value;
        let fib_val = &vegas_indicator_signal_values.fib_retracement_value;
        let hammer_val = &vegas_indicator_signal_values.kline_hammer_value;

        last.c < last.o
            && volume_ratio >= 1.8
            && (hammer_val.is_short_signal || hammer_val.up_shadow_ratio >= 0.5)
            && leg_val.is_bearish_leg
            && leg_val.is_new_leg
            && fib_val.in_zone
            && fib_val.volume_confirmed
            && structure_val
                .swing_high
                .map(|pivot| pivot.crossed)
                .unwrap_or(false)
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && macd_val.macd_line > 0.0
            && macd_val.signal_line > 0.0
            && macd_val.macd_line < macd_val.signal_line
            && macd_val.histogram < 0.0
            && macd_val.histogram_decreasing
    }

    fn is_above_zero_death_cross_range_break_short_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if data_items.len() < 7 {
            return false;
        }

        let mode = env_string("VEGAS_EXPERIMENT_ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prior_window = &data_items[data_items.len() - 6..data_items.len() - 1];
        let prior_range_high = prior_window
            .iter()
            .map(|item| item.h())
            .fold(f64::MIN, f64::max);
        let prior_range_low = prior_window
            .iter()
            .map(|item| item.l())
            .fold(f64::MAX, f64::min);
        let prior_range_width = (prior_range_high - prior_range_low) / current.c().max(1e-9);
        let close_break_pct = (prior_range_low - current.c()).max(0.0) / current.c().max(1e-9);
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd_val = &vegas_indicator_signal_values.macd_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let structure = &vegas_indicator_signal_values.market_structure_value;

        let base_match = current.c() < current.o()
            && current.body_ratio() >= 0.6
            && macd_val.above_zero
            && macd_val.is_death_cross
            && macd_val.histogram < 0.0
            && structure.swing_trend == 1
            && !structure.internal_bearish_bos
            && !structure.swing_bearish_bos;

        match mode.as_str() {
            "v3" => {
                base_match
                    && volume_ratio >= 1.3
                    && !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && matches!(
                        ema_distance.state,
                        EmaDistanceState::TooFar | EmaDistanceState::Normal
                    )
                    && prior_range_width <= 0.025
                    && close_break_pct >= 0.012
            }
            "v2" => {
                base_match
                    && volume_ratio >= 1.3
                    && !ema_values.is_long_trend
                    && !matches!(ema_distance.state, EmaDistanceState::Tangled)
                    && prior_range_width <= 0.04
                    && close_break_pct >= 0.0075
            }
            "v1" | "1" | "true" | "yes" | "on" => {
                base_match
                    && volume_ratio >= 1.5
                    && !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && prior_range_width <= 0.03
                    && close_break_pct >= 0.01
            }
            _ => false,
        }
    }

    fn round_level_step(price: f64) -> f64 {
        if price >= 10_000.0 {
            1_000.0
        } else if price >= 1_000.0 {
            100.0
        } else if price >= 100.0 {
            10.0
        } else if price >= 10.0 {
            1.0
        } else {
            0.1
        }
    }

    fn is_round_level_reversal_long_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if data_items.len() < 10 {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prev = &data_items[data_items.len() - 2];
        let prior = &data_items[data_items.len() - 10..data_items.len() - 1];
        let step = Self::round_level_step(prev.c());
        let level = (prev.c() / step).floor() * step;
        let touch_tol = step * 0.05;
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let shock_drop_pct = ((prev.c() - current.l()) / prev.c().max(1e-9)).max(0.0);

        let held_above = prior.iter().all(|item| item.l() > level + touch_tol);
        let first_touch = prev.l() > level + touch_tol && current.l() <= level + touch_tol;
        let reclaim_close = current.c() >= level - touch_tol;
        let reversal_shape = current.down_shadow_ratio() >= 0.45
            && (current.c() >= current.o() || current.body_ratio() <= 0.45);

        held_above
            && first_touch
            && shock_drop_pct >= 0.025
            && volume_ratio >= 3.0
            && reclaim_close
            && reversal_shape
            && !vegas_indicator_signal_values
                .market_structure_value
                .internal_bearish_bos
            && !vegas_indicator_signal_values
                .market_structure_value
                .swing_bearish_bos
    }

    fn is_round_level_reversal_short_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if data_items.len() < 10 {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prev = &data_items[data_items.len() - 2];
        let prior = &data_items[data_items.len() - 10..data_items.len() - 1];
        let step = Self::round_level_step(prev.c());
        let level = (prev.c() / step).ceil() * step;
        let touch_tol = step * 0.05;
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        let shock_rise_pct = ((current.h() - prev.c()) / prev.c().max(1e-9)).max(0.0);
        let mode = env_string("VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL_SHORT_MODE")
            .unwrap_or_else(|| "v1".to_string());

        let held_below = prior.iter().all(|item| item.h() < level - touch_tol);
        let first_touch = prev.h() < level - touch_tol && current.h() >= level - touch_tol;
        let reject_close = current.c() <= level + touch_tol;
        let reversal_shape = current.up_shadow_ratio() >= 0.45
            && (current.c() <= current.o() || current.body_ratio() <= 0.45);

        let base_match = held_below
            && first_touch
            && shock_rise_pct >= 0.025
            && volume_ratio >= 3.0
            && reject_close
            && reversal_shape
            && !vegas_indicator_signal_values
                .market_structure_value
                .internal_bullish_bos
            && !vegas_indicator_signal_values
                .market_structure_value
                .swing_bullish_bos;

        match mode.as_str() {
            "v2" => {
                base_match
                    && !ema_values.is_short_trend
                    && fib.retracement_ratio >= 0.5
                    && (rsi >= 65.0 || ema_distance.state == EmaDistanceState::TooFar)
            }
            _ => base_match,
        }
    }

    fn should_block_exhaustion_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        rsi < 25.0
            && volume.volume_ratio >= 5.0
            && !fib.in_zone
            && fib.retracement_ratio <= 0.05
            && boll.is_long_signal
            && ema_touch.is_short_signal
            && ema_values.is_short_trend
            && !leg.is_new_leg
            && macd.macd_line < 0.0
            && macd.signal_line < 0.0
    }

    fn should_block_bullish_leg_mean_reversion_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        volume.volume_ratio >= 1.8
            && !ema_values.is_short_trend
            && leg.is_bullish_leg
            && !leg.is_new_leg
            && fib.in_zone
            && fib.volume_confirmed
            && fib.leg_bullish
            && boll.is_short_signal
            && !ema_touch.is_short_signal
            && (45.0..=50.0).contains(&rsi)
            && macd.macd_line < 0.0
            && macd.signal_line < 0.0
            && macd.histogram > 0.0
            && macd.histogram_decreasing
    }

    fn should_block_deep_negative_macd_recovery_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        let mode = env_string("VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE")
            .unwrap_or_else(|| "v1".to_string());

        let macd_recovery_core =
            !ema_touch.is_short_signal && macd.histogram > 0.0 && macd.histogram_decreasing;
        let macd_depth_ratio = if signal_price > 0.0 {
            macd.macd_line.abs() / signal_price
        } else {
            0.0
        };
        let signal_depth_ratio = if signal_price > 0.0 {
            macd.signal_line.abs() / signal_price
        } else {
            0.0
        };

        match mode.as_str() {
            "off" => false,
            "v2" => {
                macd_recovery_core
                    && ema_values.is_short_trend
                    && boll.is_long_signal
                    && (engulfing.is_valid_engulfing || leg.is_bearish_leg)
                    && (!fib.in_zone || !fib.volume_confirmed)
                    && volume.volume_ratio < 2.0
                    && rsi < 42.0
                    && macd.macd_line < -60.0
                    && macd.signal_line < -60.0
            }
            "v3" => {
                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 2.2
                    && rsi < 45.0
                    && macd.macd_line < -50.0
                    && macd.signal_line < -50.0
            }
            "v5" => {
                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 2.2
                    && rsi < 50.0
                    && macd.macd_line < -50.0
                    && macd.signal_line < -50.0
            }
            "v6" => {
                engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 2.2
                    && rsi < 45.0
                    && macd.macd_line < -50.0
                    && macd.signal_line < -50.0
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
            }
            "v7" => {
                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.6
                    && (34.0..=43.0).contains(&rsi)
                    && macd_depth_ratio >= 0.007
                    && signal_depth_ratio >= 0.0085
            }
            "v8" => {
                let use_absolute_thresholds = signal_price >= 10_000.0;

                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && if use_absolute_thresholds {
                        volume.volume_ratio < 2.2
                            && rsi < 45.0
                            && macd.macd_line < -50.0
                            && macd.signal_line < -50.0
                    } else {
                        volume.volume_ratio < 1.6
                            && (34.0..=43.0).contains(&rsi)
                            && macd_depth_ratio >= 0.007
                            && signal_depth_ratio >= 0.0085
                    }
            }
            _ => {
                ema_values.is_short_trend
                    && engulfing.is_valid_engulfing
                    && boll.is_long_signal
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && (1.0..=1.5).contains(&volume.volume_ratio)
                    && (30.0..=38.0).contains(&rsi)
                    && macd.macd_line < -80.0
                    && macd.signal_line < -80.0
                    && macd_recovery_core
            }
        }
    }

    fn should_block_stc_early_weakening_short(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_STC_EARLY_WEAKENING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let Some((prev_stc, current_stc)) = compute_stc_pair(data_items) else {
            return false;
        };

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && !ema_touch.is_short_signal
                    && volume.volume_ratio < 2.5
                    && (45.0..=52.0).contains(&rsi)
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.macd_line < macd.signal_line
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && prev_stc >= 60.0
                    && current_stc >= 45.0
                    && current_stc < prev_stc
            }
            _ => false,
        }
    }

    fn should_block_weakening_no_structure_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_WEAKENING_NO_STRUCTURE_SHORT_BLOCK")
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
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && !hammer.is_short_signal
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && !ema_touch.is_short_signal
                    && volume.volume_ratio < 2.5
                    && (45.0..=52.0).contains(&rsi)
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.macd_line < macd.signal_line
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
            }
            _ => false,
        }
    }

    fn should_block_deep_negative_weak_breakdown_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_WEAK_BREAKDOWN_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                engulfing.is_valid_engulfing
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !ema_values.is_short_trend
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && fib.retracement_ratio < 0.08
                    && volume.volume_ratio < 2.0
                    && rsi < 30.0
                    && ema_touch.is_short_signal
                    && macd.macd_line < -60.0
                    && macd.signal_line < -50.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_shallow_weakening_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_SHALLOW_WEAKENING_SHORT_BLOCK")
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
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && !ema_touch.is_short_signal
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
                    && volume.volume_ratio < 2.5
                    && (44.0..=50.0).contains(&rsi)
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.macd_line < macd.signal_line
                    && (-2.0..0.0).contains(&macd.histogram)
                    && macd.histogram_decreasing
            }
            _ => false,
        }
    }

    fn should_block_panic_breakdown_short(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode =
            env_string("VEGAS_PANIC_BREAKDOWN_SHORT_BLOCK").unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let Some(last) = data_items.last() else {
            return false;
        };

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
                last.c < last.o
                    && last.body_ratio() >= 0.8
                    && ema_distance.state == EmaDistanceState::Ranging
                    && !ema_values.is_short_trend
                    && !ema_touch.is_short_signal
                    && boll.is_long_signal
                    && boll.is_short_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && volume.volume_ratio >= 4.5
                    && fib.volume_confirmed
                    && !fib.in_zone
                    && fib.retracement_ratio >= 0.6
                    && (38.0..=45.0).contains(&rsi)
                    && macd.macd_line < 0.0
                    && macd.signal_line < 0.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && market.internal_bearish_bos
                    && market
                        .internal_low
                        .as_ref()
                        .is_some_and(|pivot| pivot.crossed)
            }
            _ => false,
        }
    }

    fn should_block_above_zero_no_trend_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_NO_TREND_HANGING_SHORT_BLOCK")
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
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_short_trend
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && fib.retracement_ratio >= 0.85
                    && volume.volume_ratio < 1.0
                    && rsi >= 68.0
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_below_zero_weakening_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK")
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
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_short_trend
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.8
                    && (42.0..=50.0).contains(&rsi)
                    && macd.macd_line < 0.0
                    && macd.signal_line < 0.0
                    && macd.histogram < 0.0
                    && macd.histogram_increasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_no_trend_too_far_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK")
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
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.5
                    && rsi >= 55.0
                    && macd.above_zero
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_low_volume_no_trend_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK")
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
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.0
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_long_trend_pullback_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_PULLBACK_SHORT_BLOCK")
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
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && ema_touch.is_uptrend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_short_signal
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 2.0
                    && rsi <= 45.0
                    && macd.macd_line < 0.0
                    && macd.signal_line < 0.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_long_trend_above_zero_low_volume_weakening_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK")
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
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        let histogram_ratio = if signal_price > 0.0 {
            macd.histogram.abs() / signal_price
        } else {
            0.0
        };

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.2
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            "v2" => {
                ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.2
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
                    && histogram_ratio >= 0.002
            }
            _ => false,
        }
    }

    fn should_block_long_trend_above_zero_high_rsi_early_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_ABOVE_ZERO_HIGH_RSI_EARLY_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && volume.volume_ratio >= 1.5
                    && rsi >= 65.0
                    && macd.above_zero
                    && macd.histogram < 0.0
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_low_volume_neutral_rsi_macd_recovery_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let mode = env_string("VEGAS_LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_SHORT_BLOCK")
            .unwrap_or_else(|| "v1".to_string());
        if mode == "off" {
            return false;
        }

        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd = &vegas_indicator_signal_values.macd_value;
        let signal_line_ratio = if signal_price > 0.0 {
            macd.signal_line.abs() / signal_price
        } else {
            0.0
        };
        let rsi_is_neutral = valid_rsi_value
            .map(|rsi| (47.0..=53.0).contains(&rsi))
            .unwrap_or(false);
        let macd_recovering_below_zero = macd.macd_line < 0.0
            && macd.signal_line < 0.0
            && macd.macd_line > macd.signal_line
            && macd.histogram > 0.0;

        match mode.as_str() {
            "v1" => volume_ratio < 1.0 && rsi_is_neutral && macd_recovering_below_zero,
            "v2" => {
                volume_ratio < 1.0
                    && rsi_is_neutral
                    && macd_recovering_below_zero
                    && signal_line_ratio >= 0.002
            }
            _ => false,
        }
    }
}
