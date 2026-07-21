/// 量价冲击后回踩确认候选返回的方向与结构保护止损。
#[derive(Debug, Clone, Copy, PartialEq)]
struct MomentumRetestDecision {
    /// 确认棒收盘后允许的方向。
    direction: SignalDirect,
    /// 冲击棒与确认棒共同极值外的保护价。
    protective_stop: f64,
}

impl VegasStrategy {
    /// 在基础 Vegas 信号之后统一执行因果动量窗口与可选 RSI 门禁，避免两者在不同调用链产生分歧。
    fn apply_candle_momentum_entry_gate(
        &self,
        data_items: &[CandleItem],
        fib_value: &FibRetracementSignalValue,
        valid_rsi_value: Option<f64>,
        signal_result: &mut SignalResult,
        dynamic_adjustments: &mut Vec<String>,
    ) {
        let has_entry_signal =
            signal_result.should_buy.unwrap_or(false) || signal_result.should_sell.unwrap_or(false);
        if !self.candle_momentum_activation.is_open || !has_entry_signal {
            return;
        }

        // Momentum retest 已独立验证同一量价冲击窗口，不要求 Fib 再次声明 delayed volume。
        if dynamic_adjustments
            .iter()
            .any(|adjustment| adjustment.starts_with("MOMENTUM_RETEST_"))
        {
            return;
        }

        if self
            .candle_momentum_activation
            .allow_delayed_fib_volume_confirmation
        {
            if !fib_value.used_delayed_volume_confirmation {
                return;
            }
            let Some(bars_ago) = fib_value.delayed_volume_activation_bars_ago else {
                signal_result.should_buy = Some(false);
                signal_result.should_sell = Some(false);
                signal_result
                    .filter_reasons
                    .push("FIB_DELAYED_VOLUME_ACTIVATION_REQUIRED".to_string());
                return;
            };
            dynamic_adjustments.push(format!(
                "FIB_DELAYED_VOLUME_ACTIVATION_PASS(bars_ago={})",
                bars_ago
            ));
            if !self.momentum_entry_rsi_allowed(valid_rsi_value) {
                signal_result.should_buy = Some(false);
                signal_result.should_sell = Some(false);
                signal_result
                    .filter_reasons
                    .push("CANDLE_MOMENTUM_RSI_RANGE_REQUIRED".to_string());
            }
            return;
        }

        let buy = signal_result.should_buy.unwrap_or(false);
        let sell = signal_result.should_sell.unwrap_or(false);
        let direction_mode = self.candle_momentum_activation.direction_mode;
        let activation_bars_ago =
            Self::required_momentum_trigger_bullish(direction_mode, buy, sell).and_then(
                |required_bullish| {
                    self.recent_candle_momentum_activation_bars_ago(data_items, required_bullish)
                },
            );
        if let Some(bars_ago) = activation_bars_ago {
            dynamic_adjustments.push(format!(
                "CANDLE_MOMENTUM_ACTIVATION_PASS(bars_ago={})",
                bars_ago
            ));
        } else {
            signal_result.should_buy = Some(false);
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("CANDLE_MOMENTUM_ACTIVATION_REQUIRED".to_string());
            return;
        }

        if !self.momentum_entry_rsi_allowed(valid_rsi_value) {
            signal_result.should_buy = Some(false);
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("CANDLE_MOMENTUM_RSI_RANGE_REQUIRED".to_string());
        }
    }

    /// 检查动量研究配置的无量纲 RSI 区间；配置错误或缺少 RSI 时保守拒绝。
    fn momentum_entry_rsi_allowed(&self, rsi: Option<f64>) -> bool {
        let config = self.candle_momentum_activation;
        if config.min_entry_rsi.is_none() && config.max_entry_rsi.is_none() {
            return true;
        }
        let Some(rsi) = rsi.filter(|value| value.is_finite()) else {
            return false;
        };
        let min = config.min_entry_rsi.unwrap_or(f64::NEG_INFINITY);
        let max = config.max_entry_rsi.unwrap_or(f64::INFINITY);
        min.is_finite() && max.is_finite() && min < max && rsi >= min && rsi < max
    }

    /// 将 Vegas 入场方向转换为触发 K 线必须满足的方向；外层 None 表示入场方向不唯一。
    fn required_momentum_trigger_bullish(
        direction_mode: CandleMomentumDirectionMode,
        buy: bool,
        sell: bool,
    ) -> Option<Option<bool>> {
        if direction_mode == CandleMomentumDirectionMode::Any {
            return Some(None);
        }
        let signal_bullish = match (buy, sell) {
            (true, false) => true,
            (false, true) => false,
            _ => return None,
        };
        Some(Some(match direction_mode {
            CandleMomentumDirectionMode::Same => signal_bullish,
            CandleMomentumDirectionMode::Opposite => !signal_bullish,
            CandleMomentumDirectionMode::Any => unreachable!("handled above"),
        }))
    }

    /// 返回最近一次有效动量事件距离当前已确认 4H K 线的根数。
    ///
    /// 基线只取触发 K 线之前的数据，且忽略未确认 K 线，避免历史回放和实盘出现前视偏差。
    fn recent_candle_momentum_activation_bars_ago(
        &self,
        data_items: &[CandleItem],
        required_bullish: Option<bool>,
    ) -> Option<usize> {
        let config = self.candle_momentum_activation;
        if config.baseline_bars == 0 || config.valid_for_bars == 0 {
            return None;
        }

        // 判定只依赖最近的基线和有效窗口，限制扫描长度可避免长回测反复分配全部 3,600 根输入。
        let required_items = config
            .baseline_bars
            .saturating_add(config.valid_for_bars)
            .saturating_add(1);
        let mut confirmed: Vec<_> = data_items
            .iter()
            .rev()
            .filter(|item| {
                item.confirm == 1
                    && item.v.is_finite()
                    && item.v > 0.0
                    && item.c.is_finite()
                    && item.c > 0.0
                    && item.h.is_finite()
                    && item.l.is_finite()
                    && item.h >= item.l
            })
            .take(required_items)
            .collect();
        confirmed.reverse();
        if confirmed.len() <= config.baseline_bars {
            return None;
        }

        let current_index = confirmed.len() - 1;
        let minimum_bars_ago = if config.allow_trigger_bar_entry {
            0
        } else {
            config.min_wait_bars.max(1)
        };
        let maximum_bars_ago = config.valid_for_bars.min(current_index);
        if minimum_bars_ago > maximum_bars_ago {
            return None;
        }

        for bars_ago in minimum_bars_ago..=maximum_bars_ago {
            let trigger_index = current_index - bars_ago;
            if trigger_index < config.baseline_bars {
                continue;
            }
            let baseline = &confirmed[trigger_index - config.baseline_bars..trigger_index];
            let average_volume =
                baseline.iter().map(|item| item.v).sum::<f64>() / config.baseline_bars as f64;
            let average_range_ratio = baseline
                .iter()
                .map(|item| (item.h - item.l) / item.c)
                .sum::<f64>()
                / config.baseline_bars as f64;
            if average_volume <= 0.0 || average_range_ratio <= 0.0 {
                continue;
            }

            let trigger = confirmed[trigger_index];
            if required_bullish.is_some_and(|is_bullish| {
                if is_bullish {
                    trigger.c <= trigger.o
                } else {
                    trigger.c >= trigger.o
                }
            }) {
                continue;
            }
            let volume_ratio = trigger.v / average_volume;
            let range_ratio = ((trigger.h - trigger.l) / trigger.c) / average_range_ratio;
            if volume_ratio >= config.min_volume_ratio.max(0.0)
                && range_ratio >= config.min_range_ratio.max(0.0)
            {
                return Some(bars_ago);
            }
        }
        None
    }

    /// 返回某一 Fib 入场方向可使用的前序量价冲击；未开启替代确认时不改变现有信号。
    fn delayed_fib_volume_activation_bars_ago(
        &self,
        data_items: &[CandleItem],
        is_long: bool,
    ) -> Option<usize> {
        let config = self.candle_momentum_activation;
        if !config.is_open || !config.allow_delayed_fib_volume_confirmation {
            return None;
        }
        Self::required_momentum_trigger_bullish(config.direction_mode, is_long, !is_long).and_then(
            |required_bullish| {
                self.recent_candle_momentum_activation_bars_ago(data_items, required_bullish)
            },
        )
    }

    /// 在量价冲击后的固定窗口内识别实体中位回踩与方向收复。
    ///
    /// 当前棒必须已经完成，且当前 leg/EMA 不能与候选方向冲突；未确认时不保存跨窗口承诺。
    fn momentum_retest_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<MomentumRetestDecision> {
        let config = self.candle_momentum_activation;
        if !config.is_open || !config.allow_momentum_retest_entry {
            return None;
        }
        let current = data_items.last()?;
        let stop_buffer = self
            .fib_retracement_signal
            .unwrap_or_default()
            .stop_loss_buffer_ratio
            .max(0.0);

        if current.c > current.o
            && values.leg_detection_value.is_bullish_leg
            && !values.ema_values.is_short_trend
        {
            if let Some(bars_ago) = self.delayed_fib_volume_activation_bars_ago(data_items, true) {
                let trigger = &data_items[data_items.len().checked_sub(bars_ago + 1)?];
                let body_midpoint = (trigger.o + trigger.c) / 2.0;
                if current.l <= body_midpoint && current.c > body_midpoint {
                    return Some(MomentumRetestDecision {
                        direction: SignalDirect::IsLong,
                        protective_stop: trigger.l.min(current.l) * (1.0 - stop_buffer).max(0.0),
                    });
                }
            }
        }

        if current.c < current.o
            && values.leg_detection_value.is_bearish_leg
            && !values.ema_values.is_long_trend
        {
            if let Some(bars_ago) = self.delayed_fib_volume_activation_bars_ago(data_items, false) {
                let trigger = &data_items[data_items.len().checked_sub(bars_ago + 1)?];
                let body_midpoint = (trigger.o + trigger.c) / 2.0;
                if current.h >= body_midpoint && current.c < body_midpoint {
                    return Some(MomentumRetestDecision {
                        direction: SignalDirect::IsShort,
                        protective_stop: trigger.h.max(current.h) * (1.0 + stop_buffer),
                    });
                }
            }
        }

        None
    }

    /// 延迟 Fib 确认可选择复用已有中波动带；关闭该限制时保持原激活语义。
    fn delayed_fib_atr_allowed(&self, atr_ratio: f64, is_long: bool) -> bool {
        if !self
            .candle_momentum_activation
            .restrict_delayed_fib_to_choppy_band
        {
            return true;
        }
        let adaptive = self.cross_asset_adaptive_threshold;
        let band = adaptive.choppy_volatility_filter;
        if !adaptive.is_open
            || !band.is_open
            || !atr_ratio.is_finite()
            || !band.min_atr_ratio.is_finite()
            || !band.max_atr_ratio.is_finite()
            || band.max_atr_ratio <= band.min_atr_ratio
        {
            return false;
        }
        let in_choppy_band = atr_ratio >= band.min_atr_ratio && atr_ratio < band.max_atr_ratio;
        let is_high_volatility_short = !is_long
            && self
                .candle_momentum_activation
                .allow_high_volatility_delayed_short
            && atr_ratio >= band.max_atr_ratio;
        let is_high_volatility_long = is_long
            && self
                .candle_momentum_activation
                .allow_high_volatility_delayed_long
            && atr_ratio >= band.max_atr_ratio;
        let is_low_volatility = self.candle_momentum_activation.allow_low_volatility_delayed
            && atr_ratio < band.min_atr_ratio;
        in_choppy_band || is_high_volatility_short || is_high_volatility_long || is_low_volatility
    }
}

#[cfg(test)]
mod momentum_activation_tests {
    use super::*;

    fn candle(ts: i64, volume: f64, range: f64, confirm: i32) -> CandleItem {
        CandleItem {
            o: 100.0,
            h: 100.0 + range / 2.0,
            l: 100.0 - range / 2.0,
            c: 100.0,
            v: volume,
            ts,
            confirm,
        }
    }

    fn strategy() -> VegasStrategy {
        let mut strategy = VegasStrategy::new("4H".to_string());
        strategy.candle_momentum_activation = CandleMomentumActivationConfig {
            is_open: true,
            allow_delayed_fib_volume_confirmation: false,
            restrict_delayed_fib_to_choppy_band: false,
            allow_high_volatility_delayed_short: false,
            allow_high_volatility_delayed_long: false,
            allow_low_volatility_delayed: false,
            allow_momentum_retest_entry: false,
            baseline_bars: 4,
            valid_for_bars: 3,
            min_wait_bars: 1,
            min_volume_ratio: 2.0,
            min_range_ratio: 1.5,
            allow_trigger_bar_entry: false,
            direction_mode: CandleMomentumDirectionMode::Any,
            min_entry_rsi: None,
            max_entry_rsi: None,
        };
        strategy
    }

    #[test]
    fn activates_only_after_a_confirmed_normalized_shock() {
        let strategy = strategy();
        let candles = vec![
            candle(1, 100.0, 2.0, 1),
            candle(2, 100.0, 2.0, 1),
            candle(3, 100.0, 2.0, 1),
            candle(4, 100.0, 2.0, 1),
            candle(5, 250.0, 4.0, 1),
            candle(6, 100.0, 2.0, 1),
        ];

        assert_eq!(
            strategy.recent_candle_momentum_activation_bars_ago(&candles, None),
            Some(1)
        );
        assert_eq!(
            strategy.recent_candle_momentum_activation_bars_ago(&candles[..5], None),
            None
        );
    }

    #[test]
    fn ignores_unconfirmed_shocks_and_expires_old_events() {
        let strategy = strategy();
        let mut candles = vec![
            candle(1, 100.0, 2.0, 1),
            candle(2, 100.0, 2.0, 1),
            candle(3, 100.0, 2.0, 1),
            candle(4, 100.0, 2.0, 1),
            candle(5, 250.0, 4.0, 0),
            candle(6, 100.0, 2.0, 1),
        ];
        assert_eq!(
            strategy.recent_candle_momentum_activation_bars_ago(&candles, None),
            None
        );

        candles[4].confirm = 1;
        candles.extend([
            candle(7, 100.0, 2.0, 1),
            candle(8, 100.0, 2.0, 1),
            candle(9, 100.0, 2.0, 1),
        ]);
        assert_eq!(
            strategy.recent_candle_momentum_activation_bars_ago(&candles, None),
            None
        );
    }

    #[test]
    fn can_require_a_specific_trigger_candle_direction() {
        let strategy = strategy();
        let candles = vec![
            candle(1, 100.0, 2.0, 1),
            candle(2, 100.0, 2.0, 1),
            candle(3, 100.0, 2.0, 1),
            candle(4, 100.0, 2.0, 1),
            CandleItem {
                o: 98.0,
                c: 102.0,
                ..candle(5, 250.0, 4.0, 1)
            },
            candle(6, 100.0, 2.0, 1),
        ];

        assert_eq!(
            strategy.recent_candle_momentum_activation_bars_ago(&candles, Some(true)),
            Some(1)
        );
        assert_eq!(
            strategy.recent_candle_momentum_activation_bars_ago(&candles, Some(false)),
            None
        );
    }

    #[test]
    fn maps_signal_direction_for_same_and_opposite_modes() {
        assert_eq!(
            VegasStrategy::required_momentum_trigger_bullish(
                CandleMomentumDirectionMode::Any,
                true,
                false
            ),
            Some(None)
        );
        assert_eq!(
            VegasStrategy::required_momentum_trigger_bullish(
                CandleMomentumDirectionMode::Same,
                true,
                false
            ),
            Some(Some(true))
        );
        assert_eq!(
            VegasStrategy::required_momentum_trigger_bullish(
                CandleMomentumDirectionMode::Opposite,
                true,
                false
            ),
            Some(Some(false))
        );
        assert_eq!(
            VegasStrategy::required_momentum_trigger_bullish(
                CandleMomentumDirectionMode::Opposite,
                true,
                true
            ),
            None
        );
    }

    #[test]
    fn optional_rsi_range_rejects_only_momentum_entries_outside_the_band() {
        let mut strategy = strategy();
        assert!(strategy.momentum_entry_rsi_allowed(None));

        strategy.candle_momentum_activation.min_entry_rsi = Some(25.0);
        strategy.candle_momentum_activation.max_entry_rsi = Some(55.0);
        assert!(strategy.momentum_entry_rsi_allowed(Some(25.0)));
        assert!(strategy.momentum_entry_rsi_allowed(Some(54.99)));
        assert!(!strategy.momentum_entry_rsi_allowed(Some(24.99)));
        assert!(!strategy.momentum_entry_rsi_allowed(Some(55.0)));
        assert!(!strategy.momentum_entry_rsi_allowed(None));
    }

    #[test]
    fn delayed_fib_mode_preserves_base_entries_without_a_prior_shock() {
        let mut strategy = strategy();
        strategy
            .candle_momentum_activation
            .allow_delayed_fib_volume_confirmation = true;
        let mut signal = SignalResult {
            should_buy: Some(true),
            ..SignalResult::empty()
        };
        let mut adjustments = Vec::new();

        strategy.apply_candle_momentum_entry_gate(
            &[],
            &FibRetracementSignalValue::default(),
            None,
            &mut signal,
            &mut adjustments,
        );

        assert_eq!(signal.should_buy, Some(true));
        assert!(signal.filter_reasons.is_empty());
        assert!(adjustments.is_empty());
    }

    #[test]
    fn delayed_fib_mode_audits_the_activation_used_by_the_signal() {
        let mut strategy = strategy();
        strategy
            .candle_momentum_activation
            .allow_delayed_fib_volume_confirmation = true;
        let mut signal = SignalResult {
            should_buy: Some(true),
            ..SignalResult::empty()
        };
        let fib_value = FibRetracementSignalValue {
            used_delayed_volume_confirmation: true,
            delayed_volume_activation_bars_ago: Some(2),
            ..FibRetracementSignalValue::default()
        };
        let mut adjustments = Vec::new();

        strategy.apply_candle_momentum_entry_gate(
            &[],
            &fib_value,
            None,
            &mut signal,
            &mut adjustments,
        );

        assert_eq!(signal.should_buy, Some(true));
        assert_eq!(
            adjustments,
            vec!["FIB_DELAYED_VOLUME_ACTIVATION_PASS(bars_ago=2)"]
        );
    }

    #[test]
    fn delayed_fib_can_reuse_the_existing_choppy_volatility_band() {
        let mut strategy = strategy();
        strategy
            .candle_momentum_activation
            .restrict_delayed_fib_to_choppy_band = true;
        strategy.cross_asset_adaptive_threshold.is_open = true;
        strategy
            .cross_asset_adaptive_threshold
            .choppy_volatility_filter = ChoppyVolatilityFilterConfig {
            is_open: true,
            min_atr_ratio: 0.018,
            max_atr_ratio: 0.032,
            ..ChoppyVolatilityFilterConfig::default()
        };

        assert!(!strategy.delayed_fib_atr_allowed(0.0179, true));
        assert!(!strategy.delayed_fib_atr_allowed(0.0179, false));
        assert!(strategy.delayed_fib_atr_allowed(0.018, true));
        assert!(strategy.delayed_fib_atr_allowed(0.018, false));
        assert!(strategy.delayed_fib_atr_allowed(0.0319, true));
        assert!(strategy.delayed_fib_atr_allowed(0.0319, false));
        assert!(!strategy.delayed_fib_atr_allowed(0.032, true));
        assert!(!strategy.delayed_fib_atr_allowed(0.032, false));

        strategy
            .candle_momentum_activation
            .allow_high_volatility_delayed_short = true;
        assert!(!strategy.delayed_fib_atr_allowed(0.032, true));
        assert!(strategy.delayed_fib_atr_allowed(0.032, false));

        strategy
            .candle_momentum_activation
            .allow_high_volatility_delayed_long = true;
        assert!(strategy.delayed_fib_atr_allowed(0.032, true));
        assert!(strategy.delayed_fib_atr_allowed(0.032, false));

        strategy
            .candle_momentum_activation
            .allow_low_volatility_delayed = true;
        assert!(strategy.delayed_fib_atr_allowed(0.0179, true));
        assert!(strategy.delayed_fib_atr_allowed(0.0179, false));
    }

    #[test]
    fn momentum_retest_waits_for_a_completed_mid_body_reclaim() {
        let mut strategy = strategy();
        strategy
            .candle_momentum_activation
            .allow_delayed_fib_volume_confirmation = true;
        strategy
            .candle_momentum_activation
            .allow_momentum_retest_entry = true;
        strategy.candle_momentum_activation.direction_mode = CandleMomentumDirectionMode::Same;
        strategy
            .fib_retracement_signal
            .as_mut()
            .expect("default Fib config")
            .stop_loss_buffer_ratio = 0.006;
        let mut candles = vec![
            candle(1, 10.0, 2.0, 1),
            candle(2, 10.0, 2.0, 1),
            candle(3, 10.0, 2.0, 1),
            candle(4, 10.0, 2.0, 1),
        ];
        candles.push(CandleItem {
            o: 100.0,
            h: 106.0,
            l: 99.0,
            c: 105.0,
            v: 40.0,
            ts: 5,
            confirm: 1,
        });
        candles.push(CandleItem {
            o: 102.0,
            h: 105.0,
            l: 101.0,
            c: 104.0,
            v: 12.0,
            ts: 6,
            confirm: 1,
        });
        let mut values = VegasIndicatorSignalValue::default();
        values.leg_detection_value.is_bullish_leg = true;

        let decision = strategy
            .momentum_retest_decision(&candles, &values)
            .expect("completed midpoint reclaim should produce a long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 98.406).abs() < 1e-9);

        candles.last_mut().expect("confirmation candle").l = 103.0;
        assert!(strategy
            .momentum_retest_decision(&candles, &values)
            .is_none());
    }
}
