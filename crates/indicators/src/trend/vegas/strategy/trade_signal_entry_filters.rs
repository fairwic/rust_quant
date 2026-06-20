impl VegasStrategy {
    fn apply_post_signal_entry_filters(
        &self,
        data_items: &[CandleItem],
        last_data_item: &CandleItem,
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        conditions: &[(SignalType, SignalCondition)],
        fib_cfg: FibRetracementSignalConfig,
        ema_distance_filter: ema_filter::EmaDistanceFilter,
        valid_rsi_value: Option<f64>,
        signal_result: &mut SignalResult,
        dynamic_adjustments: &mut Vec<String>,
        range_snapshot: &mut Option<serde_json::Value>,
    ) {
        // ================================================================
        // Fib 严格大趋势过滤：禁开反向仓
        // 只有当 swing 波动幅度足够大时，才应用此过滤，避免窄幅震荡中过度过滤
        // ================================================================
        if fib_cfg.is_open && fib_cfg.strict_major_trend {
            let major_bull =
                trend::is_major_bullish_trend(&vegas_indicator_signal_values.ema_values);
            let major_bear =
                trend::is_major_bearish_trend(&vegas_indicator_signal_values.ema_values);

            // 计算 swing 波动幅度
            let swing_high = vegas_indicator_signal_values
                .fib_retracement_value
                .swing_high;
            let swing_low = vegas_indicator_signal_values
                .fib_retracement_value
                .swing_low;
            let swing_move_pct = if swing_low > 0.0 {
                (swing_high - swing_low) / swing_low
            } else {
                0.0
            };

            // 只有在 swing 数据有效且波动幅度足够大时才应用过滤
            let is_trend_move_significant =
                swing_low > 0.0 && swing_move_pct >= fib_cfg.min_trend_move_pct;

            // 注意：这里仅记录"禁止开仓"的原因，不直接清空 should_buy/should_sell。
            // 这样回测/实盘可以在 backtest/position 层实现"反向信号仅平仓，不反手开仓"的行为。
            if is_trend_move_significant {
                if major_bear && signal_result.should_buy.unwrap_or(false) {
                    signal_result.filter_reasons.push(format!(
                        "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG(swing_pct={:.2}%)",
                        swing_move_pct * 100.0
                    ));
                }
                if major_bull && signal_result.should_sell.unwrap_or(false) {
                    signal_result.filter_reasons.push(format!(
                        "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT(swing_pct={:.2}%)",
                        swing_move_pct * 100.0
                    ));
                }
            }
        }

        // 高波动下跌阶段容易出现"低位追空"，
        // 当空头排列已经显著远离均线且不在 Fib 回撤区间时，直接拦截做空。
        // 但对极少数"放量新腿破位延续"场景保留例外，避免错杀有效突破空单。
        let fib_val = vegas_indicator_signal_values.fib_retracement_value;
        let allow_breakdown_short = vegas_indicator_signal_values.leg_detection_value.is_new_leg
            && fib_val.retracement_ratio <= 0.10
            && fib_val.volume_ratio >= 3.0
            && vegas_indicator_signal_values.macd_value.histogram < 0.0;
        if self.entry_block_config.block_too_far_outside_fib_short
            && signal_result.should_sell.unwrap_or(false)
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && ema_distance_filter.state == EmaDistanceState::TooFar
            && !fib_val.in_zone
            && !allow_breakdown_short
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TOO_FAR_OUTSIDE_FIB_ZONE_BLOCK_SHORT".to_string());
        }

        let allow_repair_long = signal_result.should_buy.unwrap_or(false)
            && Self::is_repair_long_candidate(vegas_indicator_signal_values, valid_rsi_value);
        let allow_new_leg_positive_macd_long = signal_result.should_buy.unwrap_or(false)
            && Self::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                vegas_indicator_signal_values,
                valid_rsi_value,
            );

        // TooFar 反趋势做多里，锤子线抄底在空头排列且 Fib 未回到理想区间时表现较差。
        // 这类单常由局部反转信号触发，但仍处于空头主导阶段，优先拦截低 RSI 的接飞刀做多。
        let should_block_counter_trend_hammer_long = signal_result.should_buy.unwrap_or(false)
            && ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !fib_val.in_zone
            && vegas_indicator_signal_values
                .kline_hammer_value
                .is_long_signal
            && valid_rsi_value.is_some_and(|rsi| rsi < 45.0)
            && !allow_repair_long;
        let should_block_counter_trend_hammer_long =
            should_block_counter_trend_hammer_long && !allow_new_leg_positive_macd_long;
        if self.entry_block_config.block_counter_trend_hammer_long
            && should_block_counter_trend_hammer_long
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG".to_string());
        }

        let should_block_counter_trend_chase_long = signal_result.should_buy.unwrap_or(false)
            && ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !fib_val.in_zone
            && (vegas_indicator_signal_values
                .engulfing_value
                .is_valid_engulfing
                || vegas_indicator_signal_values.bollinger_value.is_long_signal)
            && valid_rsi_value.is_some_and(|rsi| rsi >= 50.0)
            && fib_val.volume_ratio >= 2.5
            && vegas_indicator_signal_values.macd_value.histogram >= 0.0;
        if should_block_counter_trend_chase_long {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TOO_FAR_COUNTER_TREND_CHASE_LONG".to_string());
        }

        let should_block_weak_ema_trend_entry =
            Self::should_block_weak_ema_trend_entry(&conditions, &fib_val, fib_cfg.is_open);
        if self.entry_block_config.block_weak_ema_trend_entry
            && signal_result.should_buy.unwrap_or(false)
            && should_block_weak_ema_trend_entry
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_LONG".to_string());
        }
        if self.entry_block_config.block_weak_ema_trend_entry
            && signal_result.should_sell.unwrap_or(false)
            && should_block_weak_ema_trend_entry
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_SHORT".to_string());
        }

        let should_block_weak_structure_breakout_long =
            Self::should_block_weak_structure_breakout_long(&conditions, valid_rsi_value);
        if signal_result.should_buy.unwrap_or(false) && should_block_weak_structure_breakout_long {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SIMPLE_BREAK_CHOCH_NO_BOS_LONG".to_string());
        }

        let should_block_conflicting_structure_breakout_short =
            Self::should_block_conflicting_structure_breakout_short(
                &conditions,
                ema_distance_filter.state,
            );
        if signal_result.should_sell.unwrap_or(false)
            && should_block_conflicting_structure_breakout_short
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("SIMPLE_BREAK_BULLISH_STRUCTURE_SHORT".to_string());
        }

        let should_block_shallow_fib_breakdown_short =
            Self::should_block_shallow_fib_breakdown_short(
                &conditions,
                ema_distance_filter.state,
                &fib_val,
            );
        if signal_result.should_sell.unwrap_or(false) && should_block_shallow_fib_breakdown_short {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("SIMPLE_BREAK_TOO_FAR_SHALLOW_FIB_SHORT".to_string());
        }

        let should_block_conflicting_too_far_new_bear_leg_short =
            Self::should_block_conflicting_too_far_new_bear_leg_short(
                &conditions,
                vegas_indicator_signal_values,
            );
        if self
            .entry_block_config
            .block_conflicting_too_far_new_bear_leg_short
            && signal_result.should_sell.unwrap_or(false)
            && should_block_conflicting_too_far_new_bear_leg_short
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("CONFLICTING_TOO_FAR_NEW_BEAR_LEG_SHORT".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_macd_near_zero_weak_hammer_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("MACD_NEAR_ZERO_WEAK_HAMMER_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_too_far_uptrend_opposing_hammer_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("TOO_FAR_UPTREND_OPPOSING_HAMMER_SHORT_BLOCK".to_string());
        }

        // ================================================================
        // 应用EMA距离过滤（仅空头分支）
        // - 过远状态且空头排列：拒绝做空
        // ================================================================
        if self.entry_block_config.block_ema_distance_short
            && ema_distance_filter.should_filter_short
            && signal_result.should_sell.unwrap_or(false)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_DISTANCE_FILTER_SHORT".to_string());
        }

        // ================================================================
        // 【追涨/追跌确认K线条件】
        // 当价格远离EMA144时，要求额外的确认条件才能开仓
        // 回测验证: ID 5988, profit +57%, sharpe 1.53→1.89, max_dd 57.7%→55.5%
        // ================================================================
        let chase_cfg = self.chase_confirm_config.unwrap_or_default();
        if chase_cfg.enabled {
            let ema144 = vegas_indicator_signal_values.ema_values.ema2_value;
            if ema144 > 0.0 {
                let price_vs_ema144 = (last_data_item.c - ema144) / ema144;

                // 追涨确认：price > EMA144*(1+threshold) 时要求额外确认
                if price_vs_ema144 > chase_cfg.long_threshold
                    && signal_result.should_buy.unwrap_or(false)
                {
                    let body_ratio = last_data_item.body_ratio();
                    let is_bullish = last_data_item.c > last_data_item.o;

                    // 确认条件（任一满足）
                    let pullback_touch = {
                        let low_vs_ema144 = (last_data_item.l - ema144) / ema144;
                        low_vs_ema144.abs() <= chase_cfg.pullback_touch_threshold
                    };
                    let bullish_close = is_bullish && body_ratio > chase_cfg.min_body_ratio;
                    let has_engulfing = vegas_indicator_signal_values
                        .engulfing_value
                        .is_valid_engulfing;

                    let confirmed = pullback_touch || bullish_close || has_engulfing;
                    if !confirmed {
                        signal_result.should_buy = Some(false);
                        signal_result
                            .filter_reasons
                            .push("CHASE_CONFIRM_FILTER_LONG".to_string());
                    }
                }

                // 追跌确认：price < EMA144*(1-threshold) 时要求额外确认
                if price_vs_ema144 < -chase_cfg.short_threshold
                    && signal_result.should_sell.unwrap_or(false)
                {
                    let body_ratio = last_data_item.body_ratio();
                    let is_bearish = last_data_item.c < last_data_item.o;

                    // 确认条件（任一满足）
                    let bounce_touch = {
                        let high_vs_ema144 = (last_data_item.h - ema144) / ema144;
                        high_vs_ema144.abs() <= chase_cfg.pullback_touch_threshold
                    };
                    let bearish_close = is_bearish && body_ratio > chase_cfg.min_body_ratio;
                    let has_engulfing = vegas_indicator_signal_values
                        .engulfing_value
                        .is_valid_engulfing;

                    let confirmed = bounce_touch || bearish_close || has_engulfing;
                    if !confirmed {
                        signal_result.should_sell = Some(false);
                        signal_result
                            .filter_reasons
                            .push("CHASE_CONFIRM_FILTER_SHORT".to_string());
                    }
                }
            }
        }

        // ================================================================
        // 【新增】极端K线过滤/放行：
        // - 大实体且一次跨越多条EMA时，仅顺势放行；反向信号直接过滤
        // - 方向冲突时撤销信号，避免追入假突破
        // ================================================================
        if let Some(extreme_cfg) = self.extreme_k_filter_signal.as_ref() {
            if extreme_cfg.is_open {
                let body_ratio = last_data_item.body_ratio();
                let body_move_pct =
                    ((last_data_item.c - last_data_item.o).abs()) / last_data_item.o.max(1e-9);
                let cross_count = Self::count_crossed_emas(
                    last_data_item.o,
                    last_data_item.c,
                    &vegas_indicator_signal_values.ema_values,
                );

                let is_extreme = body_ratio >= extreme_cfg.min_body_ratio
                    && body_move_pct >= extreme_cfg.min_move_pct
                    && cross_count >= extreme_cfg.min_cross_ema_count;

                if is_extreme {
                    let is_bull = last_data_item.c > last_data_item.o;
                    let is_bear = last_data_item.c < last_data_item.o;

                    if is_bull && signal_result.should_sell.unwrap_or(false) {
                        signal_result.should_sell = Some(false);
                        signal_result
                            .filter_reasons
                            .push("EXTREME_K_FILTER_CONFLICT_SHORT".to_string());
                    }
                    if is_bear && signal_result.should_buy.unwrap_or(false) {
                        signal_result.should_buy = Some(false);
                        signal_result
                            .filter_reasons
                            .push("EXTREME_K_FILTER_CONFLICT_LONG".to_string());
                    }

                    // 仅顺势放行，逆势则拦截
                    if signal_result.should_buy.unwrap_or(false) {
                        // 如果是大趋势多头且极端K线也是多头，则放行（忽略小趋势）
                        let allow_by_major = trend::is_major_bullish_trend(
                            &vegas_indicator_signal_values.ema_values,
                        ) && is_bull;

                        if !allow_by_major {
                            // 否则必须满足小趋势多头
                            if !trend::is_bullish_trend(&vegas_indicator_signal_values.ema_values) {
                                signal_result.should_buy = Some(false);
                                signal_result
                                    .filter_reasons
                                    .push("EXTREME_K_FILTER_TREND_LONG".to_string());
                            }
                        }
                    }

                    if signal_result.should_sell.unwrap_or(false) {
                        // 如果是大趋势空头且极端K线也是空头，则放行（忽略小趋势）
                        let allow_by_major = trend::is_major_bearish_trend(
                            &vegas_indicator_signal_values.ema_values,
                        ) && is_bear;

                        if !allow_by_major {
                            // 否则必须满足小趋势空头
                            if !trend::is_bearish_trend(&vegas_indicator_signal_values.ema_values) {
                                signal_result.should_sell = Some(false);
                                signal_result
                                    .filter_reasons
                                    .push("EXTREME_K_FILTER_TREND_SHORT".to_string());
                            }
                        }
                    }
                }
            }
        }

        super::entry_blocks::apply_entry_block_reasons(
            signal_result,
            &self.entry_block_config,
            last_data_item,
            vegas_indicator_signal_values,
            self.range_filter_signal.as_ref(),
        );

        // ================================================================
        // 震荡过滤：震荡时降低止盈目标（不影响开仓，只影响 TP）
        // 震荡区间: RSI 中性 + 缩量或 MACD 近零轴 -> 1:1 止盈
        // ================================================================
        if let Some(range_filter_signal) = &self.range_filter_signal {
            if range_filter_signal.is_open && self.bolling_signal.is_some() {
                let bb_value = &vegas_indicator_signal_values.bollinger_value;
                let mid = bb_value.middle;
                let width = bb_value.upper - bb_value.lower;
                if mid > 0.0 && width > 0.0 {
                    let bb_width_ratio = width / mid;
                    if bb_width_ratio <= range_filter_signal.bb_width_threshold {
                        let k_range = (last_data_item.h - last_data_item.l)
                            .abs()
                            .max(last_data_item.c * 0.001);
                        let tp_ratio = range_filter_signal.tp_kline_ratio.max(0.0);
                        let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
                        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
                        let rsi_in_range = valid_rsi_value
                            .map(|rsi| (46.0..=54.0).contains(&rsi))
                            .unwrap_or(false);
                        let macd_near_zero = self.macd_signal.as_ref().is_some_and(|macd_cfg| {
                            if !macd_cfg.is_open {
                                return false;
                            }
                            let macd_val = &vegas_indicator_signal_values.macd_value;
                            macd_val.macd_line.abs() <= entry_price * 0.001
                        });
                        let is_ultra_narrow =
                            bb_width_ratio <= range_filter_signal.bb_width_threshold * 0.85;
                        let is_indecision = last_data_item.is_small_body_and_big_up_down_shadow();
                        let use_one_to_one = rsi_in_range
                            && (volume_ratio < 1.05 || macd_near_zero || is_indecision)
                            && is_ultra_narrow;
                        *range_snapshot = Some(json!({
                            "enabled": true,
                            "bb_width_ratio": bb_width_ratio,
                            "bb_width_threshold": range_filter_signal.bb_width_threshold,
                            "tp_ratio": tp_ratio,
                            "use_one_to_one": use_one_to_one,
                            "volume_ratio": volume_ratio,
                            "rsi": valid_rsi_value,
                            "macd_near_zero": macd_near_zero,
                            "is_indecision": is_indecision,
                        }));
                        if use_one_to_one {
                            dynamic_adjustments.push("RANGE_TP_ONE_TO_ONE".to_string());
                        } else {
                            dynamic_adjustments.push("RANGE_TP_RATIO".to_string());
                        }

                        let take_profit_diff = if use_one_to_one {
                            let stop_price = signal_result
                                .signal_kline_stop_loss_price
                                .or(signal_result.atr_stop_loss_price);
                            let diff = stop_price
                                .map(|price| (entry_price - price).abs())
                                .unwrap_or(0.0);
                            if diff > 0.0 {
                                diff
                            } else {
                                k_range * tp_ratio
                            }
                        } else {
                            k_range * tp_ratio
                        };

                        if signal_result.should_buy.unwrap_or(false) {
                            signal_result.long_signal_take_profit_price =
                                Some(entry_price + take_profit_diff);
                        }
                        if signal_result.should_sell.unwrap_or(false) {
                            signal_result.short_signal_take_profit_price =
                                Some(entry_price - take_profit_diff);
                        }
                    }
                }
            }
        }

        // ================================================================
        // 【新增】MACD 动量反转过滤 (Momentum Turn Filter)
        // 核心逻辑：允许 MACD 反向入场（抄底/摸顶），但要求动量必须改善（拐点已现）
        // 1. 如果 MACD 与交易方向一致 -> 放行（顺势）
        // 2. 如果 MACD 与交易方向相反（逆势）：
        //    - 柱状图继续恶化（接飞刀） -> 过滤
        //    - 柱状图开始改善（企稳） -> 放行
        // ================================================================
        if let Some(macd_cfg) = &self.macd_signal {
            if macd_cfg.is_open {
                let macd_val = &vegas_indicator_signal_values.macd_value;

                // 做多过滤
                if signal_result.should_buy.unwrap_or(false) {
                    let mut should_filter = false;
                    let rebound_protect_long = Self::is_rebound_protect_long_candidate(
                        data_items,
                        vegas_indicator_signal_values,
                    );

                    if macd_cfg.filter_falling_knife && macd_cfg.filter_falling_knife_long {
                        // 如果 MACD 柱状图为负（处于空头动量区）
                        if macd_val.histogram < 0.0 {
                            // 且 柱状图在递减（负值变更大，动量加速向下）
                            if macd_val.histogram_decreasing {
                                should_filter = true; // 正在接飞刀，过滤
                                if rebound_protect_long {
                                    signal_result
                                        .filter_reasons
                                        .push("REBOUND_HAMMER_LONG_PROTECT".to_string());
                                }
                                signal_result
                                    .filter_reasons
                                    .push("MACD_FALLING_KNIFE_LONG".to_string());
                            }
                        }
                    }

                    if should_filter {
                        signal_result.should_buy = Some(false);
                    }
                }

                // 做空过滤
                if signal_result.should_sell.unwrap_or(false) {
                    let mut should_filter = false;

                    if macd_cfg.filter_falling_knife && macd_cfg.filter_falling_knife_short {
                        // 如果 MACD 柱状图为正（处于多头动量区）
                        if macd_val.histogram > 0.0 {
                            // 且 柱状图在递增（正值变更大，动量加速向上）
                            if macd_val.histogram_increasing {
                                should_filter = true; // 正在逆势摸顶（涨势未尽），过滤
                                signal_result
                                    .filter_reasons
                                    .push("MACD_FALLING_KNIFE_SHORT".to_string());
                            }
                        }
                    }

                    if should_filter {
                        signal_result.should_sell = Some(false);
                    }
                }
            }
        }

        // 缩量 + RSI 中性 + MACD 零轴下方修复时，避免过早反手做空。
        // 典型场景是大跌后的修复反抽，趋势仍偏空，但动量和参与度都不支持立即追空。
        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_low_volume_neutral_rsi_macd_recovery_short(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
                valid_rsi_value,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_BLOCK_SHORT".to_string());
        }

        // 极端低位放量砸盘时，避免在旧空头腿末端继续追空。
        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_exhaustion_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EXHAUSTION_SHORT_NEAR_SWING_LOW_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_bullish_leg_mean_reversion_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("BULLISH_LEG_MEAN_REVERSION_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_deep_negative_macd_recovery_short(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_stc_early_weakening_short(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("STC_EARLY_WEAKENING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_weakening_no_structure_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("WEAKENING_NO_STRUCTURE_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_deep_negative_weak_breakdown_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_WEAK_BREAKDOWN_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_shallow_weakening_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_SHALLOW_WEAKENING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_panic_breakdown_short(data_items, vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("PANIC_BREAKDOWN_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_no_trend_hanging_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_NO_TREND_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_below_zero_weakening_hanging_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_no_trend_too_far_hanging_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_low_volume_no_trend_hanging_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_long_trend_pullback_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_PULLBACK_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_long_trend_above_zero_low_volume_weakening_short(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_long_trend_above_zero_high_rsi_early_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_ABOVE_ZERO_HIGH_RSI_EARLY_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_high_volume_no_trend_bollinger_long_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_high_volume_ranging_recovery_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_RANGING_RECOVERY_SHORT_BLOCK".to_string());
        }

        // 缩量 + RSI 中性 + MACD 零轴上方转弱时，避免过早逆势做多。
        // 典型场景是上涨后的回落修复，参与度不足且死叉刚开始，不适合抢多。
        if self
            .entry_block_config
            .block_low_volume_neutral_rsi_macd_weakening_long
            && signal_result.should_buy.unwrap_or(false)
        {
            let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
            let macd_val = &vegas_indicator_signal_values.macd_value;
            let rsi_is_neutral = valid_rsi_value
                .map(|rsi| (47.0..=53.0).contains(&rsi))
                .unwrap_or(false);
            let macd_weakening_above_zero = macd_val.macd_line > 0.0
                && macd_val.signal_line > 0.0
                && macd_val.macd_line < macd_val.signal_line
                && macd_val.histogram < 0.0;

            if volume_ratio < 1.0 && rsi_is_neutral && macd_weakening_above_zero {
                signal_result.should_buy = Some(false);
                signal_result
                    .filter_reasons
                    .push("LOW_VOLUME_NEUTRAL_RSI_MACD_WEAKENING_BLOCK_LONG".to_string());
            }
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_deep_negative_hammer_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_HAMMER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_recent_upper_shadow_pressure_long(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("RECENT_UPPER_SHADOW_PRESSURE_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_weak_breakout_no_trend_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("WEAK_BREAKOUT_NO_TREND_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_ranging_no_trend_weak_hammer_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("RANGING_NO_TREND_WEAK_HAMMER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_too_far_bollinger_short_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_conflicting_bollinger_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_internal_down_counter_trend_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_high_rsi_bollinger_short_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_deep_negative_no_trend_hammer_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_NO_TREND_HAMMER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_short_trend_too_far_bollinger_short_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_TREND_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_short_trend_new_bull_leg_counter_long(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_short_trend_no_bollinger_rebound_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_normal_bull_leg_no_confirm_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_above_zero_no_trend_engulfing_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_long_trend_below_zero_fib_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_BELOW_ZERO_FIB_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_level_sideways_chase_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_LEVEL_SIDEWAYS_CHASE_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_above_zero_high_level_chase_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_protect_deep_negative_hammer_long(vegas_indicator_signal_values)
        {
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
            let protective_stop = last_data_item.l.max(entry_price * 0.98);
            signal_result.signal_kline_stop_loss_price = Some(
                signal_result
                    .signal_kline_stop_loss_price
                    .map(|existing| existing.max(protective_stop))
                    .unwrap_or(protective_stop),
            );
            signal_result.stop_loss_source = Some("DeepNegativeHammer_Long_Protect".to_string());
            dynamic_adjustments.push("DEEP_NEGATIVE_HAMMER_LONG_PROTECT".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_protect_long_trend_deep_negative_hammer_long(
                vegas_indicator_signal_values,
            )
        {
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
            let protective_stop = last_data_item.l.max(entry_price * 0.975);
            signal_result.signal_kline_stop_loss_price = Some(
                signal_result
                    .signal_kline_stop_loss_price
                    .map(|existing| existing.max(protective_stop))
                    .unwrap_or(protective_stop),
            );
            signal_result.stop_loss_source =
                Some("LongTrendDeepNegativeHammer_Protect".to_string());
            dynamic_adjustments.push("LONG_TREND_DEEP_NEGATIVE_HAMMER_PROTECT".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_protect_above_zero_high_level_chase_long(vegas_indicator_signal_values)
        {
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
            let protective_stop = last_data_item.l.max(entry_price * 0.985);
            signal_result.signal_kline_stop_loss_price = Some(
                signal_result
                    .signal_kline_stop_loss_price
                    .map(|existing| existing.max(protective_stop))
                    .unwrap_or(protective_stop),
            );
            signal_result.stop_loss_source =
                Some("AboveZeroHighLevelChaseLong_Protect".to_string());
            dynamic_adjustments.push("ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_PROTECT".to_string());
        }

    }
}
