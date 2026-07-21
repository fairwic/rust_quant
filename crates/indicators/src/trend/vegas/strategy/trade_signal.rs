impl VegasStrategy {
    /// Fib-only 配置由 Fib 直接决定是否入场，不再执行失去决策权的加权评分。
    fn resolve_signal_direction(
        fib_cfg: FibRetracementSignalConfig,
        fib_direction: Option<SignalDirect>,
        weighted_direction: impl FnOnce() -> Option<SignalDirect>,
    ) -> Option<SignalDirect> {
        if !fib_cfg.is_open {
            return weighted_direction();
        }
        if fib_direction.is_some() {
            return fib_direction;
        }
        if fib_cfg.only_on_fib {
            None
        } else {
            weighted_direction()
        }
    }

    /// 获取交易信号
    /// data_items: 数据列表，在突破策略中要考虑到前一根k线
    pub fn get_trade_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &mut VegasIndicatorSignalValue,
        weights: &SignalWeightsConfig,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        // 输入验证
        if data_items.is_empty() {
            return SignalResult {
                should_buy: Some(false),
                should_sell: Some(false),
                open_price: Some(0.0),
                best_open_price: None,
                atr_take_profit_ratio_price: None,
                atr_stop_loss_price: None,
                long_signal_take_profit_price: None,
                short_signal_take_profit_price: None,
                signal_kline_stop_loss_price: None,
                stop_loss_source: None,
                ts: Some(0),
                single_value: None,
                single_result: None,
                // 填充新字段
                direction: rust_quant_domain::SignalDirection::None,
                strength: rust_quant_domain::SignalStrength::new(0.0),
                signals: vec![],
                can_open: false,
                should_close: false,
                entry_price: None,
                stop_loss_price: None,
                take_profit_price: None,
                position_time: None,
                signal_kline: None,
                filter_reasons: vec![],
                dynamic_adjustments: vec![],
                dynamic_config_snapshot: None,
            };
        }
        let last_data_item = match data_items.last() {
            Some(item) => item,
            None => {
                return SignalResult {
                    should_buy: Some(false),
                    should_sell: Some(false),
                    open_price: Some(0.0),
                    best_open_price: None,
                    atr_take_profit_ratio_price: None,
                    atr_stop_loss_price: None,
                    long_signal_take_profit_price: None,
                    short_signal_take_profit_price: None,
                    signal_kline_stop_loss_price: None,
                    stop_loss_source: None,
                    ts: Some(0),
                    single_value: None,
                    single_result: None,
                    // 填充新字段
                    direction: rust_quant_domain::SignalDirection::None,
                    strength: rust_quant_domain::SignalStrength::new(0.0),
                    signals: vec![],
                    can_open: false,
                    should_close: false,
                    entry_price: None,
                    stop_loss_price: None,
                    take_profit_price: None,
                    position_time: None,
                    signal_kline: None,
                    filter_reasons: vec![],
                    dynamic_adjustments: vec![],
                    dynamic_config_snapshot: None,
                };
            }
        };
        // 初始化交易信号
        let mut signal_result = SignalResult {
            should_buy: Some(false),
            should_sell: Some(false),
            open_price: Some(last_data_item.c),
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            signal_kline_stop_loss_price: None,
            stop_loss_source: None,
            ts: Some(last_data_item.ts),
            single_value: None,
            single_result: None,
            // 填充新字段
            direction: rust_quant_domain::SignalDirection::None,
            strength: rust_quant_domain::SignalStrength::new(0.0),
            signals: vec![],
            can_open: false,
            should_close: false,
            entry_price: None,
            stop_loss_price: None,
            take_profit_price: None,
            position_time: None,
            signal_kline: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
        };
        let mut conditions = Vec::with_capacity(10);
        let mut valid_rsi_value: Option<f64> = None;
        let mut dynamic_adjustments: Vec<String> = Vec::new();
        let mut range_snapshot: Option<serde_json::Value> = None;
        vegas_indicator_signal_values.cross_asset_adaptive_value =
            self.calculate_cross_asset_adaptive_value(data_items);
        // 优先判断成交量
        if let Some(_volume_signal) = &self.volume_signal {
            let is_than_vol_ratio = self.check_volume_trend(
                &vegas_indicator_signal_values.volume_value,
                &vegas_indicator_signal_values.cross_asset_adaptive_value,
            );
            conditions.push((
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: is_than_vol_ratio,
                    ratio: vegas_indicator_signal_values.volume_value.volume_ratio,
                },
            ));
        }
        // 检查EMA2被突破
        let (price_above, price_below) = self
            .check_breakthrough_conditions(data_items, vegas_indicator_signal_values.ema_values);
        if price_above || price_below {
            conditions.push((
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above,
                    price_below,
                },
            ));
        }
        // 检查EMA排列，回调触碰关键均线位置
        let ema_trend =
            self.check_ema_touch_trend(data_items, vegas_indicator_signal_values.ema_values);
        vegas_indicator_signal_values.ema_touch_value = ema_trend;
        if ema_trend.is_long_signal || ema_trend.is_short_signal {
            conditions.push((
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: ema_trend.is_long_signal,
                    is_short_signal: ema_trend.is_short_signal,
                },
            ));
        }
        // 计算RSI
        if let Some(rsi_signal) = &self.rsi_signal {
            let current_rsi_opt = self.get_valid_rsi(
                data_items,
                &vegas_indicator_signal_values.rsi_value,
                vegas_indicator_signal_values.ema_values,
            );
            // 如果返回 None，表示检测到极端行情（大利空/利多消息），跳过后续交易信号判断
            let current_rsi = match current_rsi_opt {
                Some(rsi) => rsi,
                None => {
                    // 极端行情，直接返回不交易的信号
                    signal_result
                        .filter_reasons
                        .push("RSI_EXTREME_EVENT".to_string());
                    dynamic_adjustments.push("RSI_EXTREME_EVENT".to_string());
                    signal_result.dynamic_adjustments = dynamic_adjustments.clone();
                    signal_result.dynamic_config_snapshot = Some(
                        json!({
                            "kline_ts": last_data_item.ts,
                            "adjustments": dynamic_adjustments,
                        })
                        .to_string(),
                    );
                    return signal_result;
                }
            };
            valid_rsi_value = Some(current_rsi);
            conditions.push((
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: current_rsi,
                    oversold: rsi_signal.rsi_oversold,
                    overbought: rsi_signal.rsi_overbought,
                    is_valid: true,
                },
            ));
        }
        // 判断布林带
        if let Some(_bollinger_signal) = &self.bolling_signal {
            let bollinger_value =
                self.check_bollinger_signal(data_items, vegas_indicator_signal_values.clone());
            vegas_indicator_signal_values.bollinger_value = bollinger_value;
            conditions.push((
                SignalType::Bolling,
                SignalCondition::Bolling {
                    is_long_signal: bollinger_value.is_long_signal,
                    is_short_signal: bollinger_value.is_short_signal,
                    is_close_signal: bollinger_value.is_close_signal,
                },
            ));
        }
        // 检查突破的持续性
        let _breakthrough_confirmed = self.check_breakthrough_confirmation(data_items, price_above);
        // 计算振幅
        let _k_line_amplitude = utils::calculate_k_line_amplitude(data_items);
        // 计算吞没形态
        self.check_engulfing_signal(
            data_items,
            vegas_indicator_signal_values,
            &mut conditions,
            vegas_indicator_signal_values.ema_values,
        );
        // 添加锤子形态
        self.check_kline_hammer_signal(
            data_items,
            vegas_indicator_signal_values,
            &mut conditions,
            vegas_indicator_signal_values.ema_values,
        );
        // 腿部识别（可选）：只在 is_open 时参与条件打分
        if let Some(leg_detection_signal) = &self.leg_detection_signal {
            if leg_detection_signal.is_open {
                let leg_value = vegas_indicator_signal_values.leg_detection_value;
                if leg_value.is_bullish_leg || leg_value.is_bearish_leg {
                    conditions.push((
                        SignalType::LegDetection,
                        SignalCondition::LegDetection {
                            is_bullish_leg: leg_value.is_bullish_leg,
                            is_bearish_leg: leg_value.is_bearish_leg,
                            is_new_leg: leg_value.is_new_leg,
                        },
                    ));
                }
            }
        }
        if let Some(market_structure_signal) = &self.market_structure_signal {
            if market_structure_signal.is_open {
                let structure_value = &vegas_indicator_signal_values.market_structure_value;
                let has_swing_signal = structure_value.swing_bullish_bos
                    || structure_value.swing_bearish_bos
                    || structure_value.swing_bullish_choch
                    || structure_value.swing_bearish_choch;
                let has_internal_signal = structure_value.internal_bullish_bos
                    || structure_value.internal_bearish_bos
                    || structure_value.internal_bullish_choch
                    || structure_value.internal_bearish_choch;
                let can_use_swing = market_structure_signal.enable_swing_signal && has_swing_signal;
                let can_use_internal = market_structure_signal.enable_internal_signal
                    && has_internal_signal
                    && (!market_structure_signal.enable_swing_signal || !has_swing_signal);
                if can_use_swing || can_use_internal {
                    let use_internal = !can_use_swing && can_use_internal;
                    let (bullish_bos, bearish_bos, bullish_choch, bearish_choch) = if use_internal {
                        (
                            structure_value.internal_bullish_bos,
                            structure_value.internal_bearish_bos,
                            structure_value.internal_bullish_choch,
                            structure_value.internal_bearish_choch,
                        )
                    } else {
                        (
                            structure_value.swing_bullish_bos,
                            structure_value.swing_bearish_bos,
                            structure_value.swing_bullish_choch,
                            structure_value.swing_bearish_choch,
                        )
                    };
                    conditions.push((
                        SignalType::MarketStructure,
                        SignalCondition::MarketStructure {
                            is_bullish_bos: bullish_bos,
                            is_bearish_bos: bearish_bos,
                            is_bullish_choch: bullish_choch,
                            is_bearish_choch: bearish_choch,
                            is_internal: use_internal,
                        },
                    ));
                }
            }
        }
        // ================================================================
        // 【新增】EMA距离过滤
        // ================================================================
        let ema_distance_config = self.ema_distance_config;
        let ema_distance_filter = ema_filter::apply_ema_distance_filter(
            last_data_item.c,
            &vegas_indicator_signal_values.ema_values,
            &ema_distance_config,
        );
        vegas_indicator_signal_values.ema_distance_filter = ema_distance_filter;
        // ================================================================
        // 【新增】MACD 计算
        // ================================================================
        if let Some(macd_cfg) = &self.macd_signal {
            if macd_cfg.is_open && data_items.len() > macd_cfg.slow_period + macd_cfg.signal_period
            {
                use ta::indicators::MovingAverageConvergenceDivergence;
                use ta::Next;
                let mut macd = MovingAverageConvergenceDivergence::new(
                    macd_cfg.fast_period,
                    macd_cfg.slow_period,
                    macd_cfg.signal_period,
                )
                .unwrap();
                let mut prev_macd = 0.0f64;
                let mut prev_signal = 0.0f64;
                let mut prev_histogram = 0.0f64;
                let mut prev_prev_histogram = 0.0f64;
                let replay_period = macd_cfg.slow_period.saturating_add(macd_cfg.signal_period);
                let replay_items = recent_indicator_replay_window(data_items, replay_period);
                let mut replay_histograms = Vec::with_capacity(replay_items.len());
                for item in replay_items {
                    let macd_output = macd.next(item.c);
                    prev_prev_histogram = prev_histogram;
                    prev_histogram = macd_output.macd - macd_output.signal;
                    replay_histograms.push(prev_histogram);
                    prev_signal = macd_output.signal;
                    prev_macd = macd_output.macd;
                }
                let histogram = prev_macd - prev_signal;
                // 判断金叉死叉：当前 histogram > 0 且前一根 < 0
                let is_golden_cross = histogram > 0.0 && prev_prev_histogram <= 0.0;
                let is_death_cross = histogram < 0.0 && prev_prev_histogram >= 0.0;
                // 判断柱状图趋势
                let histogram_increasing = histogram > prev_prev_histogram;
                let histogram_decreasing = histogram < prev_prev_histogram;
                // 判断动量是否正在改善（用于识别触底反弹）
                // 对于负区域：当前柱高于上一柱表示负值收缩；旧实现误与当前柱自身比较，恒为 false。
                let histogram_improving = histogram > prev_prev_histogram;
                vegas_indicator_signal_values.macd_value = super::signal::MacdSignalValue {
                    macd_line: prev_macd,
                    signal_line: prev_signal,
                    histogram,
                    is_golden_cross,
                    is_death_cross,
                    histogram_increasing,
                    histogram_decreasing,
                    above_zero: prev_macd > 0.0,
                    prev_histogram: prev_prev_histogram,
                    histogram_improving,
                };
                vegas_indicator_signal_values.macd_divergence_value =
                    Self::calculate_macd_divergence_value(replay_items, &replay_histograms);
            }
        }
        // ================================================================
        // 【新增】Fib 回撤入场信号（Swing + Fib + 放量）
        // ================================================================
        let fib_cfg = self.fib_retracement_signal.unwrap_or_default();
        if fib_cfg.is_open {
            let atr_ratio = vegas_indicator_signal_values
                .cross_asset_adaptive_value
                .atr_ratio;
            let delayed_long_volume_activation_bars_ago = self
                .delayed_fib_atr_allowed(atr_ratio, true)
                .then(|| self.delayed_fib_volume_activation_bars_ago(data_items, true))
                .flatten();
            let delayed_short_volume_activation_bars_ago = self
                .delayed_fib_atr_allowed(atr_ratio, false)
                .then(|| self.delayed_fib_volume_activation_bars_ago(data_items, false))
                .flatten();
            vegas_indicator_signal_values.fib_retracement_value =
                super::swing_fib::generate_fib_retracement_signal(
                    data_items,
                    &vegas_indicator_signal_values.ema_values,
                    &vegas_indicator_signal_values.leg_detection_value,
                    vegas_indicator_signal_values.volume_value.volume_ratio,
                    &vegas_indicator_signal_values.cross_asset_adaptive_value,
                    &self.cross_asset_adaptive_threshold,
                    delayed_long_volume_activation_bars_ago,
                    delayed_short_volume_activation_bars_ago,
                    &fib_cfg,
                );
        } else {
            vegas_indicator_signal_values
                .fib_retracement_value
                .volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        }
        let fib_val = vegas_indicator_signal_values.fib_retracement_value;
        let fib_direction = if fib_val.is_long_signal {
            Some(SignalDirect::IsLong)
        } else if fib_val.is_short_signal {
            Some(SignalDirect::IsShort)
        } else {
            None
        };
        let mut signal_direction = Self::resolve_signal_direction(fib_cfg, fib_direction, || {
            let score = weights.calculate_score(conditions.clone());
            weights.is_signal_valid(&score)
        });
        // V71 是独立策略家族：开启时先隔离旧入口，只产出价值区候选；组合层再以 V69
        // 为优先基线做容量与重叠裁决，避免新信号在单币回放中修改旧持仓的退出路径。
        let volume_profile_standalone = self.volume_profile_value_area_retest.is_open
            || self.volume_profile_value_area_breakout.is_open
            || self.volume_profile_failed_auction.is_open
            || self.donchian_volume_breakout.is_open
            || self.donchian_breakout_acceptance.is_open;
        let compressed_range_breakout_standalone =
            self.compressed_range_breakout.is_open && self.compressed_range_breakout.standalone;
        if volume_profile_standalone || compressed_range_breakout_standalone {
            signal_direction = None;
        }
        let liquidity_sweep_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.liquidity_sweep_reversal_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = liquidity_sweep_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(if decision.first_retest_confirmation {
                match (decision.direction, decision.first_retest_wait_bars) {
                    (SignalDirect::IsLong, 2) => {
                        "LIQUIDITY_SWEEP_FIRST_RETEST_DELAYED_2_LONG".to_string()
                    }
                    (SignalDirect::IsShort, 2) => {
                        "LIQUIDITY_SWEEP_FIRST_RETEST_DELAYED_2_SHORT".to_string()
                    }
                    (SignalDirect::IsLong, _) => "LIQUIDITY_SWEEP_FIRST_RETEST_LONG".to_string(),
                    (SignalDirect::IsShort, _) => "LIQUIDITY_SWEEP_FIRST_RETEST_SHORT".to_string(),
                }
            } else if decision.failed_breakout_close_reentry {
                "FAILED_BREAKOUT_CLOSE_REENTRY_SHORT".to_string()
            } else if decision.failed_breakdown_close_reentry {
                "FAILED_BREAKDOWN_CLOSE_REENTRY_LONG".to_string()
            } else if decision.failed_breakdown_higher_low_breakout {
                "FAILED_BREAKDOWN_HIGHER_LOW_BREAKOUT_LONG".to_string()
            } else if decision.upper_sweep_confirmation_low_break {
                "UPPER_SWEEP_CONFIRMATION_LOW_BREAK_SHORT".to_string()
            } else if decision.lower_sweep_confirmation_high_break {
                "LOWER_SWEEP_CONFIRMATION_HIGH_BREAK_LONG".to_string()
            } else {
                match decision.direction {
                    SignalDirect::IsLong => "LIQUIDITY_SWEEP_REVERSAL_LONG".to_string(),
                    SignalDirect::IsShort => "LIQUIDITY_SWEEP_REVERSAL_SHORT".to_string(),
                }
            });
            if decision.first_retest_confirmation {
                if let Some(take_profit_r) = self
                    .liquidity_sweep_reversal
                    .first_retest_take_profit_r
                    .filter(|value| value.is_finite() && *value > 0.0)
                {
                    // 信号层只携带冻结比率；有效止损会受最大损失门禁影响，
                    // 因此必须等持仓层选定初始止损后再换算目标价。
                    let prefix = if self
                        .liquidity_sweep_reversal
                        .first_retest_replace_existing_take_profit
                    {
                        "LIQUIDITY_SWEEP_FIRST_RETEST_TP_ONLY_R"
                    } else {
                        "LIQUIDITY_SWEEP_FIRST_RETEST_TP_R"
                    };
                    dynamic_adjustments.push(format!("{prefix}:{take_profit_r}"));
                }
            }
        }
        let momentum_retest_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.momentum_retest_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = momentum_retest_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(match decision.direction {
                SignalDirect::IsLong => "MOMENTUM_RETEST_LONG".to_string(),
                SignalDirect::IsShort => "MOMENTUM_RETEST_SHORT".to_string(),
            });
        }
        let compressed_range_breakout_decision =
            if !volume_profile_standalone && signal_direction.is_none() {
                self.compressed_range_breakout_decision(data_items, vegas_indicator_signal_values)
            } else {
                None
            };
        if let Some(decision) = compressed_range_breakout_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(if decision.delayed_confirmation {
                "COMPRESSED_RANGE_BREAKOUT_SHORT_ONE_BAR_CONFIRMATION".to_string()
            } else if decision.price_displacement_activation {
                "COMPRESSED_RANGE_BREAKOUT_SHORT_PRICE_DISPLACEMENT".to_string()
            } else {
                match decision.direction {
                    SignalDirect::IsLong => "COMPRESSED_RANGE_BREAKOUT_LONG".to_string(),
                    SignalDirect::IsShort => "COMPRESSED_RANGE_BREAKOUT_SHORT".to_string(),
                }
            });
        }
        let bos_fvg_retest_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.bos_fvg_retest_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = bos_fvg_retest_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(format!(
                "BOS_FVG_RETEST_MACD_SHORT(age_bars={})",
                decision.fvg_age_bars
            ));
        }
        let fvg_reclaim_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.fvg_reclaim_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = fvg_reclaim_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(format!(
                "FAILED_BEARISH_FVG_RECLAIM_MACD_LONG(age_bars={})",
                decision.fvg_age_bars
            ));
        }
        let macd_divergence_reversal_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.macd_divergence_reversal_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = macd_divergence_reversal_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(format!(
                "MACD_DIVERGENCE_FRESH_CHOCH_{}(shock_bars_ago={})",
                match decision.direction {
                    SignalDirect::IsLong => "LONG",
                    SignalDirect::IsShort => "SHORT",
                },
                decision.shock_bars_ago
            ));
        }
        let macd_trend_reset_bos_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.macd_trend_reset_bos_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = macd_trend_reset_bos_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(
                match decision.direction {
                    SignalDirect::IsLong => "MACD_TREND_RESET_FRESH_BOS_LONG",
                    SignalDirect::IsShort => "MACD_TREND_RESET_FRESH_BOS_SHORT",
                }
                .to_string(),
            );
        }
        let ema_tunnel_retest_confirmation_decision = if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
        {
            self.ema_tunnel_retest_confirmation_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = ema_tunnel_retest_confirmation_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(
                match decision.direction {
                    SignalDirect::IsLong => "EMA_TUNNEL_RETEST_CONFIRMATION_LONG",
                    SignalDirect::IsShort => "EMA_TUNNEL_RETEST_CONFIRMATION_SHORT",
                }
                .to_string(),
            );
        }
        let volume_profile_value_area_retest_decision = if !compressed_range_breakout_standalone
            && self.volume_profile_value_area_retest.is_open
        {
            self.volume_profile_value_area_retest_decision(data_items)
        } else {
            None
        };
        if let Some(decision) = volume_profile_value_area_retest_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(
                match decision.direction {
                    SignalDirect::IsLong => "VOLUME_PROFILE_VALUE_AREA_RETEST_LONG",
                    SignalDirect::IsShort => "VOLUME_PROFILE_VALUE_AREA_RETEST_SHORT",
                }
                .to_string(),
            );
        }
        let volume_profile_value_area_breakout_decision = if !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && self.volume_profile_value_area_breakout.is_open
        {
            self.volume_profile_value_area_breakout_decision(data_items)
        } else {
            None
        };
        if let Some(decision) = volume_profile_value_area_breakout_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(
                match decision.direction {
                    SignalDirect::IsLong => "VOLUME_PROFILE_VALUE_AREA_BREAKOUT_LONG",
                    SignalDirect::IsShort => "VOLUME_PROFILE_VALUE_AREA_BREAKOUT_SHORT",
                }
                .to_string(),
            );
            if let Some(take_profit_r) = self
                .volume_profile_value_area_breakout
                .fixed_take_profit_r
                .filter(|value| value.is_finite() && *value > 0.0)
            {
                dynamic_adjustments.push(format!(
                    "VOLUME_PROFILE_VALUE_AREA_BREAKOUT_TP_ONLY_R:{take_profit_r}"
                ));
            }
        }
        let volume_profile_failed_auction_decision = if !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && self.volume_profile_failed_auction.is_open
        {
            self.volume_profile_failed_auction_decision(data_items)
        } else {
            None
        };
        if volume_profile_failed_auction_decision.is_some() {
            signal_direction = Some(SignalDirect::IsShort);
            dynamic_adjustments.push("VOLUME_PROFILE_UPPER_FAILED_AUCTION_SHORT".to_string());
            dynamic_adjustments.push("VOLUME_PROFILE_FAILED_AUCTION_POC_ONLY".to_string());
        }
        let donchian_volume_breakout_decision = if !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && self.donchian_volume_breakout.is_open
        {
            self.donchian_volume_breakout_decision(data_items, vegas_indicator_signal_values)
        } else {
            None
        };
        if let Some(decision) = donchian_volume_breakout_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(
                match decision.direction {
                    SignalDirect::IsLong => "DONCHIAN_VOLUME_BREAKOUT_LONG",
                    SignalDirect::IsShort => "DONCHIAN_VOLUME_BREAKOUT_SHORT",
                }
                .to_string(),
            );
            dynamic_adjustments.push("DONCHIAN_VOLUME_BREAKOUT_TP_ONLY_R:2".to_string());
        }
        let donchian_breakout_acceptance_decision = if !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && self.donchian_breakout_acceptance.is_open
        {
            self.donchian_breakout_acceptance_decision(data_items)
        } else {
            None
        };
        if let Some(decision) = donchian_breakout_acceptance_decision {
            signal_direction = Some(decision.direction);
            dynamic_adjustments.push(
                match decision.direction {
                    SignalDirect::IsLong => "DONCHIAN_BREAKOUT_ACCEPTANCE_LONG",
                    SignalDirect::IsShort => "DONCHIAN_BREAKOUT_ACCEPTANCE_SHORT",
                }
                .to_string(),
            );
            dynamic_adjustments.push("DONCHIAN_BREAKOUT_ACCEPTANCE_TP_ONLY_R:2".to_string());
        }
        if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && env_flag("VEGAS_EXPERIMENT_EXPANSION_CONTINUATION_LONG")
            && Self::is_expansion_continuation_long_candidate(
                data_items,
                vegas_indicator_signal_values,
                valid_rsi_value,
            )
        {
            signal_direction = Some(SignalDirect::IsLong);
            dynamic_adjustments.push("EXPANSION_CONTINUATION_LONG".to_string());
        }
        if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && env_flag("VEGAS_EXPERIMENT_FAKE_BREAKOUT_REVERSAL_SHORT")
            && Self::is_fake_breakout_reversal_short_candidate(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_direction = Some(SignalDirect::IsShort);
            dynamic_adjustments.push("FAKE_BREAKOUT_REVERSAL_SHORT".to_string());
        }
        if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && signal_direction.is_none()
            && Self::is_above_zero_death_cross_range_break_short_candidate(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_direction = Some(SignalDirect::IsShort);
            dynamic_adjustments.push("ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT".to_string());
        }
        if !volume_profile_standalone
            && !compressed_range_breakout_standalone
            && env_flag("VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL")
        {
            let round_level_long_candidate = Self::is_round_level_reversal_long_candidate(
                data_items,
                vegas_indicator_signal_values,
            );
            let round_level_short_candidate = Self::is_round_level_reversal_short_candidate(
                data_items,
                vegas_indicator_signal_values,
            );
            if round_level_long_candidate && !round_level_short_candidate {
                signal_direction = Some(SignalDirect::IsLong);
                dynamic_adjustments.push("ROUND_LEVEL_REVERSAL_LONG".to_string());
            } else if round_level_short_candidate && !round_level_long_candidate {
                signal_direction = Some(SignalDirect::IsShort);
                dynamic_adjustments.push("ROUND_LEVEL_REVERSAL_SHORT".to_string());
            }
        }
        if let Some(signal_direction) = signal_direction {
            // 计算 ATR 用于止损价格
            let mut atr = ATR::new(14).unwrap();
            for item in data_items {
                atr.next(item.h, item.l, item.c);
            }
            let atr_value = atr.value();
            let atr_multiplier = self.atr_stop_loss_multiplier.max(0.0);
            // 检查大实体（Large Entity）状态
            let mut is_large_entity = false;
            let mut large_entity_retracement_sl: Option<f64> = None;
            if let Some(large_entity_cfg) = &self.large_entity_stop_loss_config {
                if large_entity_cfg.is_open {
                    let body_ratio = last_data_item.body_ratio();
                    let move_pct =
                        (last_data_item.c - last_data_item.o).abs() / last_data_item.o.max(1e-9);
                    let range = last_data_item.h - last_data_item.l;
                    if body_ratio >= large_entity_cfg.min_body_ratio
                        && move_pct >= large_entity_cfg.min_move_pct
                    {
                        is_large_entity = true;
                        // 计算基于回撤比例的止损
                        match signal_direction {
                            SignalDirect::IsLong => {
                                // 做多：High - Range * ratio (容忍从高点回撤一定比例)
                                let sl =
                                    last_data_item.h - range * large_entity_cfg.retracement_ratio;
                                // 确保止损不高于入场价(Close) - 保护性
                                large_entity_retracement_sl = Some(sl.min(last_data_item.c));
                            }
                            SignalDirect::IsShort => {
                                // 做空：Low + Range * ratio (容忍从低点反弹一定比例)
                                let sl =
                                    last_data_item.l + range * large_entity_cfg.retracement_ratio;
                                // 确保止损不低于入场价(Close) - 保护性
                                large_entity_retracement_sl = Some(sl.max(last_data_item.c));
                            }
                        }
                    }
                }
            }
            match signal_direction {
                SignalDirect::IsLong => {
                    signal_result.should_buy = Some(true);
                    signal_result.direction = rust_quant_domain::SignalDirection::Long;
                    // 做多止损: 入场价 - ATR * multiplier
                    if atr_value > 0.0 {
                        signal_result.atr_stop_loss_price =
                            Some(last_data_item.c - atr_value * atr_multiplier);
                    }
                    // Fib 回撤入场：优先写入 swing 止损（可配置）
                    if fib_cfg.is_open
                        && fib_cfg.use_swing_stop_loss
                        && vegas_indicator_signal_values
                            .fib_retracement_value
                            .is_long_signal
                        && signal_result.signal_kline_stop_loss_price.is_none()
                    {
                        let sl = vegas_indicator_signal_values
                            .fib_retracement_value
                            .suggested_stop_loss;
                        if sl > 0.0 {
                            signal_result.signal_kline_stop_loss_price =
                                Some(sl.min(last_data_item.c));
                            signal_result.stop_loss_source = Some("FibRetracement".to_string());
                        }
                    }
                    // 【成交量确认形态止损】只在成交量放大时启用形态止损
                    let volume_confirmed =
                        vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;
                    // 1. 优先检查大实体止损（强趋势保护）
                    // 用户规则优化：如果macd是绿柱（histogram > 0），且快线大于慢线（macd > signal），就不启用大实体止损
                    let macd_val = &vegas_indicator_signal_values.macd_value;
                    let macd_strong_bullish =
                        macd_val.histogram > 0.0 && macd_val.macd_line > macd_val.signal_line;
                    let is_repair_long = Self::is_repair_long_candidate(
                        vegas_indicator_signal_values,
                        valid_rsi_value,
                    );
                    if is_repair_long {
                        // 暴跌后的修复 long 更容易被后续信号止损过早打掉，
                        // 用标记交给持仓层忽略后续信号止损更新，保留 ATR/最大亏损止损。
                        signal_result.signal_kline_stop_loss_price = None;
                        signal_result.stop_loss_source =
                            Some("RepairLong_NoSignalKline".to_string());
                    } else if is_large_entity
                        && large_entity_retracement_sl.is_some()
                        && !macd_strong_bullish
                    {
                        signal_result.signal_kline_stop_loss_price = large_entity_retracement_sl;
                        signal_result.stop_loss_source =
                            Some("LargeEntity_Retracement".to_string());
                    }
                    // 2. 其次检查吞没形态 + 成交量确认
                    else if vegas_indicator_signal_values.engulfing_value.is_engulfing {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Rejected".to_string());
                        }
                    }
                    // 3. 最后检查锤子线形态 + 成交量确认(如果还没有设置止损)
                    if !is_repair_long
                        && signal_result.signal_kline_stop_loss_price.is_none()
                        && vegas_indicator_signal_values
                            .kline_hammer_value
                            .is_long_signal
                    {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.l);
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Rejected".to_string());
                        }
                    }
                }
                SignalDirect::IsShort => {
                    signal_result.should_sell = Some(true);
                    signal_result.direction = rust_quant_domain::SignalDirection::Short;
                    // 做空止损: 入场价 + ATR * multiplier
                    if atr_value > 0.0 {
                        signal_result.atr_stop_loss_price =
                            Some(last_data_item.c + atr_value * atr_multiplier);
                    }
                    // Fib 回撤入场：优先写入 swing 止损（可配置）
                    if fib_cfg.is_open
                        && fib_cfg.use_swing_stop_loss
                        && vegas_indicator_signal_values
                            .fib_retracement_value
                            .is_short_signal
                        && signal_result.signal_kline_stop_loss_price.is_none()
                    {
                        let sl = vegas_indicator_signal_values
                            .fib_retracement_value
                            .suggested_stop_loss;
                        if sl > 0.0 {
                            signal_result.signal_kline_stop_loss_price =
                                Some(sl.max(last_data_item.c));
                            signal_result.stop_loss_source = Some("FibRetracement".to_string());
                        }
                    }
                    // 【成交量确认形态止损】只在成交量放大时启用形态止损
                    let volume_confirmed =
                        vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;
                    // 1. 优先检查大实体止损（强趋势保护）
                    // if is_large_entity && large_entity_retracement_sl.is_some() {
                    //    signal_result.signal_kline_stop_loss_price = large_entity_retracement_sl;
                    //    signal_result.stop_loss_source =
                    //        Some("LargeEntity_Retracement".to_string());
                    // }
                    // 2. 其次检查吞没形态 + 成交量确认
                    if vegas_indicator_signal_values.engulfing_value.is_engulfing {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Rejected".to_string());
                        }
                    }
                    // 3. 最后检查锤子线形态 + 成交量确认(如果还没有设置止损)
                    if signal_result.signal_kline_stop_loss_price.is_none()
                        && vegas_indicator_signal_values
                            .kline_hammer_value
                            .is_short_signal
                    {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.h);
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Rejected".to_string());
                        }
                    }
                }
            }
            if let Some(decision) = liquidity_sweep_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source = Some(
                    if decision.first_retest_confirmation {
                        if decision.first_retest_wait_bars == 2 {
                            "LiquiditySweepFirstRetestDelayed2_FourBarExtreme"
                        } else {
                            "LiquiditySweepFirstRetest_ThreeBarExtreme"
                        }
                    } else if decision.failed_breakout_close_reentry {
                        "FailedBreakoutCloseReentry_TwoBarExtreme"
                    } else if decision.failed_breakdown_close_reentry {
                        "FailedBreakdownCloseReentry_TwoBarExtreme"
                    } else if decision.failed_breakdown_higher_low_breakout {
                        "FailedBreakdownHigherLowBreakout_PullbackLow"
                    } else if decision.upper_sweep_confirmation_low_break {
                        "UpperSweepConfirmationLowBreak_ConfirmationHigh"
                    } else if decision.lower_sweep_confirmation_high_break {
                        "LowerSweepConfirmationHighBreak_ConfirmationLow"
                    } else {
                        "LiquiditySweepReversal_TwoBarExtreme"
                    }
                    .to_string(),
                );
            }
            if let Some(decision) = momentum_retest_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("MomentumRetest_ShockConfirmationExtreme".to_string());
            }
            if let Some(protective_stop) =
                compressed_range_breakout_decision.and_then(|decision| decision.protective_stop)
            {
                signal_result.signal_kline_stop_loss_price = Some(protective_stop);
                signal_result.stop_loss_source = Some(
                    if compressed_range_breakout_decision
                        .is_some_and(|decision| decision.delayed_confirmation)
                    {
                        "CompressedRangeBreakout_WeakVolumeOneBarConfirmation"
                    } else {
                        "CompressedRangeBreakout_PriorRangeInvalidation"
                    }
                    .to_string(),
                );
            }
            if let Some(decision) = bos_fvg_retest_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("BosFvgRetest_FvgInvalidationHigh".to_string());
            }
            if let Some(decision) = fvg_reclaim_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("FailedBearishFvgReclaim_FvgInvalidationLow".to_string());
            }
            if let Some(decision) = macd_divergence_reversal_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("MacdDivergenceFreshChoch_ShockConfirmationExtreme".to_string());
            }
            if let Some(decision) = macd_trend_reset_bos_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("MacdTrendResetFreshBos_ThreeBarExtreme".to_string());
            }
            if let Some(decision) = ema_tunnel_retest_confirmation_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("EmaTunnelRetestConfirmation_ThreeBarExtreme".to_string());
            }
            if let Some(decision) = volume_profile_value_area_retest_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("VolumeProfileValueAreaRetest_RetestExtreme".to_string());
            }
            if let Some(decision) = volume_profile_value_area_breakout_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("VolumeProfileValueAreaBreakout_ValueAreaBoundary".to_string());
            }
            if let Some(decision) = volume_profile_failed_auction_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("VolumeProfileFailedAuction_TwoBarHigh".to_string());
                signal_result.short_signal_take_profit_price = Some(decision.point_of_control);
            }
            if let Some(decision) = donchian_volume_breakout_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source = Some("DonchianVolumeBreakout_TwoAtr".to_string());
            }
            if let Some(decision) = donchian_breakout_acceptance_decision {
                signal_result.signal_kline_stop_loss_price = Some(decision.protective_stop);
                signal_result.stop_loss_source =
                    Some("DonchianBreakoutAcceptance_FrozenBoundary".to_string());
            }
            // 信号产生时立即记录指标快照（在过滤逻辑之前）
            // 这样即使信号后续被过滤，filtered_signal_log 也能记录当时的指标状态
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }
        self.apply_post_signal_entry_filters(
            data_items,
            last_data_item,
            vegas_indicator_signal_values,
            &conditions,
            fib_cfg,
            ema_distance_filter,
            valid_rsi_value,
            &mut signal_result,
            &mut dynamic_adjustments,
            &mut range_snapshot,
        );
        self.apply_bullish_rejection_momentum_recovery_short_block(
            vegas_indicator_signal_values,
            &mut signal_result,
        );
        self.apply_short_lower_rejection_block(vegas_indicator_signal_values, &mut signal_result);
        self.apply_new_leg_activation_guard(vegas_indicator_signal_values, &mut signal_result);
        if self.short_profit_protection.is_open && signal_result.should_sell == Some(true) {
            // 只把版本化的出场语义写入信号；实际价格必须等开仓层冻结最终初始止损后计算。
            dynamic_adjustments.push(
                match self.short_profit_protection.mode {
                    ShortProfitProtectionMode::BreakevenAfter1p5R => "SHORT_PROFIT_PROTECTION_1_5R",
                    ShortProfitProtectionMode::Lock1rAfter2r => "SHORT_PROFIT_LOCK_2R_TO_1R",
                }
                .to_string(),
            );
        }
        // 吞没形态与 Vegas 最终方向不一致时，信号棒开盘价会落在止损错误一侧；
        // 若不剔除，下一棒可能以行情范围外的价格产生虚假盈利平仓。
        if Self::reject_non_protective_signal_stop(&mut signal_result) {
            dynamic_adjustments.push("STOP_LOSS_SIGNAL_DIRECTION_REJECTED".to_string());
        }
        if signal_result.signal_kline_stop_loss_price.is_some() {
            dynamic_adjustments.push("STOP_LOSS_SIGNAL_KLINE".to_string());
        }
        if signal_result.atr_stop_loss_price.is_some() {
            dynamic_adjustments.push("STOP_LOSS_ATR".to_string());
        }
        if signal_result.long_signal_take_profit_price.is_some() {
            dynamic_adjustments.push("TP_DYNAMIC_LONG".to_string());
        }
        if signal_result.short_signal_take_profit_price.is_some() {
            dynamic_adjustments.push("TP_DYNAMIC_SHORT".to_string());
        }
        signal_result.dynamic_adjustments = dynamic_adjustments.clone();
        signal_result.dynamic_config_snapshot = Some(
            json!({
                "kline_ts": last_data_item.ts,
                "adjustments": dynamic_adjustments,
                "range_tp": range_snapshot,
                "stop_loss": {
                    "signal_kline": signal_result.signal_kline_stop_loss_price,
                    "atr": signal_result.atr_stop_loss_price,
                    "source": signal_result.stop_loss_source.clone(),
                },
                "take_profit": {
                    "long": signal_result.long_signal_take_profit_price,
                    "short": signal_result.short_signal_take_profit_price,
                    "atr_ratio": signal_result.atr_take_profit_ratio_price
                }
            })
            .to_string(),
        );
        // 可选：添加详细信息到结果中
        if self.emit_debug
            && (signal_result.should_buy.unwrap_or(false)
                || signal_result.should_sell.unwrap_or(false))
        {
            //如果有使用信号k线止损
            if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
                self.calculate_best_stop_loss_price(
                    last_data_item,
                    &mut signal_result,
                    &conditions,
                    vegas_indicator_signal_values,
                );
                if signal_result.direction == rust_quant_domain::SignalDirection::Short
                    && matches!(
                        signal_result.stop_loss_source.as_deref(),
                        Some("Engulfing_Volume_Confirmed") | Some("KlineHammer_Volume_Confirmed")
                    )
                {
                    if let Some(current_stop) = signal_result.signal_kline_stop_loss_price {
                        let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
                        if let Some(tightened_stop) = Self::tighten_short_signal_stop_near_zero_macd(
                            entry_price,
                            current_stop,
                            &vegas_indicator_signal_values.macd_value,
                        ) {
                            signal_result.signal_kline_stop_loss_price = Some(tightened_stop);
                            signal_result
                                .dynamic_adjustments
                                .push("MACD_NEAR_ZERO_TIGHTEN_SHORT_STOP".to_string());
                        }
                    }
                }
            }
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }
        signal_result
    }
}
