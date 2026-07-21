impl VegasStrategy {
    /// 获取指标组合
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    pub fn get_indicator_combine(&self) -> IndicatorCombine {
        use crate::ema_indicator::EmaIndicator;
        use crate::leg_detection_indicator::LegDetectionIndicator;
        use crate::market_structure_indicator::MarketStructureIndicator;
        use crate::momentum::rsi::RsiIndicator;
        use crate::pattern::engulfing::KlineEngulfingIndicator;
        use crate::pattern::hammer::KlineHammerIndicator;
        use crate::volatility::bollinger::BollingBandsPlusIndicator;
        use crate::volume::VolumeProfileIndicator;
        use crate::volume_indicator::VolumeRatioIndicator;
        let mut indicator_combine = IndicatorCombine::default();
        // 添加吞没形态
        if self
            .engulfing_signal
            .as_ref()
            .is_some_and(|config| config.is_open)
        {
            indicator_combine.engulfing_indicator = Some(KlineEngulfingIndicator::new());
        }
        // 添加EMA
        if let Some(ema_signal) = &self.ema_signal {
            indicator_combine.ema_indicator = Some(EmaIndicator::new(
                ema_signal.ema1_length,
                ema_signal.ema2_length,
                ema_signal.ema3_length,
                ema_signal.ema4_length,
                ema_signal.ema5_length,
                ema_signal.ema6_length,
                ema_signal.ema7_length,
            ));
        }
        // 添加成交量
        if let Some(volume_signal) = &self.volume_signal {
            indicator_combine.volume_indicator = Some(VolumeRatioIndicator::new(
                volume_signal.volume_bar_num,
                true,
            ));
        }
        // 添加RSI
        if let Some(rsi_signal) = &self.rsi_signal {
            indicator_combine.rsi_indicator = Some(RsiIndicator::new(rsi_signal.rsi_length));
        }
        // 添加布林带
        if let Some(bolling_signal) = &self.bolling_signal {
            indicator_combine.bollinger_indicator = Some(BollingBandsPlusIndicator::new(
                bolling_signal.period,
                bolling_signal.multiplier,
                bolling_signal.consecutive_touch_times,
            ));
        }
        // 添加锤子形态
        if let Some(kline_hammer_signal) = &self.kline_hammer_signal {
            indicator_combine.kline_hammer_indicator = Some(KlineHammerIndicator::new(
                kline_hammer_signal.up_shadow_ratio,
                kline_hammer_signal.down_shadow_ratio,
            ));
        }
        // 添加腿部识别（可选）
        if let Some(leg_detection_signal) = &self.leg_detection_signal {
            if leg_detection_signal.is_open {
                indicator_combine.leg_detection_indicator =
                    Some(LegDetectionIndicator::new(leg_detection_signal.size));
            }
        }
        // 添加市场结构（可选）
        if let Some(market_structure_signal) = &self.market_structure_signal {
            if market_structure_signal.is_open {
                indicator_combine.market_structure_indicator =
                    Some(MarketStructureIndicator::new_with_thresholds(
                        market_structure_signal.swing_length,
                        market_structure_signal.internal_length,
                        market_structure_signal.swing_threshold,
                        market_structure_signal.internal_threshold,
                    ));
            }
        }
        // 背离研究的 CHoCH 不能借用公共结构快照，否则旧规则会在未投票时改变基线入场。
        if self.macd_divergence_reversal.is_open {
            indicator_combine.macd_divergence_structure_indicator =
                Some(MarketStructureIndicator::new_with_thresholds(
                    MACD_DIVERGENCE_SWING_LENGTH,
                    MACD_DIVERGENCE_INTERNAL_LENGTH,
                    MACD_DIVERGENCE_SWING_THRESHOLD,
                    MACD_DIVERGENCE_INTERNAL_THRESHOLD,
                ));
        }
        // 研究结构必须使用独立实例；复用公共快照会让旧规则在配置未投票时仍改变行为。
        if self.macd_trend_reset_bos.is_open {
            indicator_combine.macd_trend_reset_structure_indicator =
                Some(MarketStructureIndicator::new_with_thresholds(
                    MACD_TREND_RESET_SWING_LENGTH,
                    MACD_TREND_RESET_INTERNAL_LENGTH,
                    MACD_TREND_RESET_SWING_THRESHOLD,
                    MACD_TREND_RESET_INTERNAL_THRESHOLD,
                ));
        }
        if self.entry_block_config.block_opposite_value_area_entry
            || self
                .entry_block_config
                .block_low_volume_above_value_area_entry
            || self
                .entry_block_config
                .block_short_inside_low_volume_node_entry
        {
            indicator_combine.volume_profile_indicator = Some(VolumeProfileIndicator::default());
        }
        indicator_combine
    }
    /// 运行回测
    /// 注意：此方法不能在 indicators 包中完整实现，因为 BacktestResult 在不同包中定义不同
    /// 实际回测逻辑应在 strategies 或 orchestration 包中调用，使用 get_indicator_combine() 和 get_trade_signal()
    pub fn run_test(
        &mut self,
        _candles: &[CandleItem],
        _risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BacktestResult {
        // 由于架构分层，indicators 包的 BacktestResult 与 strategies 包不同
        // 此方法仅作占位，实际回测在 orchestration/backtest_executor.rs 中实现
        unimplemented!(
            "VegasStrategy::run_test 应在 orchestration 包中调用，\
            使用 get_indicator_combine() 和 get_trade_signal() 方法"
        )
    }
    // 私有辅助方法
    /// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
    fn check_volume_trend(
        &self,
        volume_trend: &VolumeTrendSignalValue,
        adaptive_value: &CrossAssetAdaptiveThresholdValue,
    ) -> bool {
        if let Some(volume_signal_config) = &self.volume_signal {
            return self.adaptive_volume_confirmed(
                adaptive_value,
                volume_trend.volume_ratio > volume_signal_config.volume_increase_ratio,
            );
        }
        false
    }
    /// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
    fn check_breakthrough_conditions(
        &self,
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> (bool, bool) {
        if let Some(ema_signal) = &self.ema_signal {
            trend::check_breakthrough_conditions(
                data_items,
                ema_value,
                ema_signal.ema_breakthrough_threshold,
            )
        } else {
            (false, false)
        }
    }
    /// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
    fn check_ema_touch_trend(
        &self,
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> EmaTouchTrendSignalValue {
        if let Some(ema_touch_trend_signal) = self
            .ema_touch_trend_signal
            .as_ref()
            .filter(|config| config.is_open)
        {
            trend::check_ema_touch_trend(data_items, ema_value, ema_touch_trend_signal)
        } else {
            EmaTouchTrendSignalValue::default()
        }
    }
    fn get_valid_rsi(
        &self,
        data_items: &[CandleItem],
        rsi_value: &RsiSignalValue,
        ema_value: EmaSignalValue,
    ) -> Option<f64> {
        trend::get_valid_rsi(data_items, rsi_value.rsi_value, ema_value)
    }
    fn check_breakthrough_confirmation(&self, data_items: &[CandleItem], is_upward: bool) -> bool {
        trend::check_breakthrough_confirmation(data_items, is_upward)
    }
    /// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
    fn check_bollinger_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: VegasIndicatorSignalValue,
    ) -> BollingerSignalValue {
        let mut bolling_bands = vegas_indicator_signal_value.bollinger_value;
        // if data_items.last().expect("数据不能为空").ts == 1756051200000 {
        //     print!("bolling_bands: {:?}", bolling_bands);
        //     print!("data_items: {:?}", data_items.last());
        // }
        if let Some(_bollinger_signal) = &self.bolling_signal {
            let ema_signal_values = vegas_indicator_signal_value.ema_values;
            let data_item = data_items.last().expect("数据不能为空");
            if bolling_bands.lower > data_item.l() {
                bolling_bands.is_long_signal = true;
            }
            if bolling_bands.upper < data_item.h() {
                bolling_bands.is_short_signal = true;
            }
            //过滤逻辑,如果虽然触发了bollinger的信号，但是k线的收盘价，依然大于em1值,则认为bollinger的信号是无效的(除了对4H周期，其他的周期的提升非常大,特别是日线级别)
            if (bolling_bands.is_long_signal || bolling_bands.is_short_signal)
                && self.period != PeriodEnum::FourHour.as_str()
            {
                if bolling_bands.is_long_signal
                    && data_items.last().expect("数据不能为空").c < ema_signal_values.ema1_value
                {
                    bolling_bands.is_long_signal = false;
                    bolling_bands.is_force_filter_signal = true;
                }
                if bolling_bands.is_short_signal
                    && data_items.last().expect("数据不能为空").c > ema_signal_values.ema1_value
                {
                    bolling_bands.is_short_signal = false;
                    bolling_bands.is_force_filter_signal = true;
                }
            }
            //todo 加入过滤逻辑，如果出发点了布林带低点或者高点，但是k线是大阳线或者大阴线(实体站百分60以上)&&且刚开始形成死叉或者金叉的 表示很强势，不能直接做多，或者做空
            //todo 如何收盘价在支撑位置的下方，则不能做多，反之不能做空
            //todo 当均线空头排列时候。止盈 eth止盈为之前n根下跌k线的30%的位置，而且从最低点到最高点不能超过12%的收益
            //todo 如果上下引线都大于实体部分，说明此时不能开仓，因为此时趋势不明显，而且容易亏损
            //如果价格
            //判断k线的实体部分占比是否大于60%
            let _body_ratio = data_items.last().expect("数据不能为空").body_ratio();
            if bolling_bands.is_long_signal || bolling_bands.is_short_signal {
                // if data_items.last().unwrap().ts == 1763049600000 {
                //     println!("data_items: {:?}", data_items.last().unwrap());
                //    println!("body_ratio: {:?}", data_items.last().unwrap().body_ratio());
                // }
                // if body_ratio > 0.8 {
                //     bolling_bands.is_force_filter_signal = true;
                //     bolling_bands.is_long_signal = false;
                //     bolling_bands.is_short_signal = false;
                // }
                if data_items
                    .last()
                    .expect("数据不能为空")
                    .is_small_body_and_big_up_down_shadow()
                {
                    bolling_bands.is_force_filter_signal = true;
                    bolling_bands.is_long_signal = false;
                    bolling_bands.is_short_signal = false;
                }
            }
        }
        bolling_bands
    }
    /// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
    fn check_engulfing_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: &mut VegasIndicatorSignalValue,
        conditions: &mut Vec<(SignalType, SignalCondition)>,
        _ema_value: EmaSignalValue,
    ) {
        let mut is_engulfing = false;
        let last_data_item = data_items.last().expect("数据不能为空");
        if let Some(engulfing_signal) = self
            .engulfing_signal
            .as_ref()
            .filter(|config| config.is_open)
        {
            if vegas_indicator_signal_value.engulfing_value.is_engulfing
                && vegas_indicator_signal_value.engulfing_value.body_ratio
                    > engulfing_signal.body_ratio
            {
                vegas_indicator_signal_value
                    .engulfing_value
                    .is_valid_engulfing = true;
                is_engulfing = true;
            }
        }
        if is_engulfing {
            let is_long_signal = last_data_item.c() > last_data_item.o();
            let is_short_signal = !is_long_signal;
            conditions.push((
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal,
                    is_short_signal,
                },
            ));
        }
    }
    /// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
    fn check_kline_hammer_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &mut VegasIndicatorSignalValue,
        conditions: &mut Vec<(SignalType, SignalCondition)>,
        ema_value: EmaSignalValue,
    ) {
        if let Some(_kline_hammer_signal) = &self.kline_hammer_signal {
            let is_hammer = vegas_indicator_signal_values.kline_hammer_value.is_hammer;
            let is_hanging_man = vegas_indicator_signal_values
                .kline_hammer_value
                .is_hanging_man;
            // 如果有长上影线，且振幅>0.5，则才能判断是有效的
            if is_hammer && utils::calculate_k_line_amplitude(data_items) >= 0.6 {
                vegas_indicator_signal_values
                    .kline_hammer_value
                    .is_long_signal = true;
                // 过滤条件
                if ema_value.is_short_trend
                    && data_items.last().expect("数据不能为空").c < ema_value.ema1_value
                    && data_items.last().expect("数据不能为空").v < 5000.0
                {
                    vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_long_signal = false;
                }
            }
            if is_hanging_man && utils::calculate_k_line_amplitude(data_items) >= 0.6 {
                vegas_indicator_signal_values
                    .kline_hammer_value
                    .is_short_signal = true;
                // 过滤条件
                if ema_value.is_long_trend
                    && data_items.last().expect("数据不能为空").c > ema_value.ema1_value
                    && data_items.last().expect("数据不能为空").v < 5000.0
                {
                    vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_short_signal = false;
                }
            }
            // //如何没有长上影线和长下影线的长影线，但是此时如何实体特别大，且是放量的大实体，则标记为上涨
            // if !is_hanging_man
            //     && !is_hammer
            //     && vegas_indicator_signal_values.kline_hammer_value.body_ratio > 0.9
            //     && vegas_indicator_signal_values.volume_value.volume_ratio > 1.7
            // {
            //     println!("time:{}",time_util::mill_time_to_datetime_shanghai(data_items.last().unwrap().ts).unwrap());
            //     if data_items.last().unwrap().c > data_items.last().unwrap().o() {
            //         vegas_indicator_signal_values
            //             .kline_hammer_value
            //             .is_long_signal = true;
            //     } else {
            //         vegas_indicator_signal_values
            //             .kline_hammer_value
            //             .is_long_signal = false;
            //     }
            // }
        }
        if vegas_indicator_signal_values
            .kline_hammer_value
            .is_long_signal
            || vegas_indicator_signal_values
                .kline_hammer_value
                .is_short_signal
        {
            conditions.push((
                SignalType::KlineHammer,
                SignalCondition::KlineHammer {
                    is_long_signal: vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_long_signal,
                    is_short_signal: vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_short_signal,
                },
            ));
        }
    }
    /// 统计极端K线一次跨越的EMA条数（开盘价与收盘价之间包含的EMA数量）
    fn count_crossed_emas(open: f64, close: f64, ema_values: &EmaSignalValue) -> usize {
        let (low, high) = if open < close {
            (open, close)
        } else {
            (close, open)
        };
        let emas = [
            ema_values.ema1_value,
            ema_values.ema2_value,
            ema_values.ema3_value,
            ema_values.ema4_value,
            ema_values.ema5_value,
        ];
        emas.iter()
            .filter(|ema| **ema >= low && **ema <= high)
            .count()
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn has_signal_type(conditions: &[(SignalType, SignalCondition)], target: SignalType) -> bool {
        conditions
            .iter()
            .any(|(signal_type, _)| *signal_type == target)
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_weak_ema_trend_entry(
        conditions: &[(SignalType, SignalCondition)],
        fib_value: &FibRetracementSignalValue,
        fib_enabled: bool,
    ) -> bool {
        fib_enabled
            && fib_value.swing_high > 0.0
            && fib_value.swing_low > 0.0
            && fib_value.retracement_ratio <= 0.5
            && Self::has_signal_type(conditions, SignalType::EmaTrend)
            && !Self::has_signal_type(conditions, SignalType::Engulfing)
            && !Self::has_signal_type(conditions, SignalType::KlineHammer)
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_weak_structure_breakout_long(
        conditions: &[(SignalType, SignalCondition)],
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let Some(rsi) = valid_rsi_value else {
            return false;
        };
        if rsi >= 60.0
            || !Self::has_signal_type(conditions, SignalType::SimpleBreakEma2through)
            || !Self::has_signal_type(conditions, SignalType::LegDetection)
            || !Self::has_signal_type(conditions, SignalType::MarketStructure)
            || Self::has_signal_type(conditions, SignalType::EmaTrend)
        {
            return false;
        }
        conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::MarketStructure
                && matches!(
                    condition,
                    SignalCondition::MarketStructure {
                        is_bullish_bos: false,
                        is_bullish_choch: true,
                        ..
                    }
                )
        })
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_conflicting_structure_breakout_short(
        conditions: &[(SignalType, SignalCondition)],
        ema_distance_state: EmaDistanceState,
    ) -> bool {
        if ema_distance_state != EmaDistanceState::TooFar
            || !Self::has_signal_type(conditions, SignalType::SimpleBreakEma2through)
            || !Self::has_signal_type(conditions, SignalType::LegDetection)
            || !Self::has_signal_type(conditions, SignalType::MarketStructure)
            || Self::has_signal_type(conditions, SignalType::EmaTrend)
        {
            return false;
        }
        let has_upside_breakout = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::SimpleBreakEma2through
                && matches!(
                    condition,
                    SignalCondition::PriceBreakout {
                        price_above: true,
                        price_below: false,
                    }
                )
        });
        let has_bullish_structure = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::MarketStructure
                && matches!(
                    condition,
                    SignalCondition::MarketStructure {
                        is_bullish_bos: true,
                        ..
                    } | SignalCondition::MarketStructure {
                        is_bullish_choch: true,
                        ..
                    }
                )
        });
        has_upside_breakout && has_bullish_structure
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_shallow_fib_breakdown_short(
        conditions: &[(SignalType, SignalCondition)],
        ema_distance_state: EmaDistanceState,
        fib_value: &FibRetracementSignalValue,
    ) -> bool {
        if ema_distance_state != EmaDistanceState::TooFar
            || fib_value.in_zone
            || fib_value.retracement_ratio > 0.3
            || !Self::has_signal_type(conditions, SignalType::SimpleBreakEma2through)
            || !Self::has_signal_type(conditions, SignalType::LegDetection)
            || !Self::has_signal_type(conditions, SignalType::MarketStructure)
            || Self::has_signal_type(conditions, SignalType::EmaTrend)
        {
            return false;
        }
        let has_downside_breakout = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::SimpleBreakEma2through
                && matches!(
                    condition,
                    SignalCondition::PriceBreakout {
                        price_above: false,
                        price_below: true,
                    }
                )
        });
        let has_bearish_structure = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::MarketStructure
                && matches!(
                    condition,
                    SignalCondition::MarketStructure {
                        is_bearish_bos: true,
                        ..
                    } | SignalCondition::MarketStructure {
                        is_bearish_choch: true,
                        ..
                    }
                )
        });
        has_downside_breakout && has_bearish_structure
    }
    /// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
    fn should_block_conflicting_too_far_new_bear_leg_short(
        conditions: &[(SignalType, SignalCondition)],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if vegas_indicator_signal_values.ema_distance_filter.state != EmaDistanceState::TooFar
            || !vegas_indicator_signal_values.fib_retracement_value.in_zone
            || vegas_indicator_signal_values.volume_value.volume_ratio >= 1.5
        {
            return false;
        }
        let has_bolling_long = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::Bolling
                && matches!(
                    condition,
                    SignalCondition::Bolling {
                        is_long_signal: true,
                        is_short_signal: false,
                        ..
                    }
                )
        });
        let has_engulfing_short = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::Engulfing
                && matches!(
                    condition,
                    SignalCondition::Engulfing {
                        is_long_signal: false,
                        is_short_signal: true,
                    }
                )
        });
        let has_new_bearish_leg = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::LegDetection
                && matches!(
                    condition,
                    SignalCondition::LegDetection {
                        is_bullish_leg: false,
                        is_bearish_leg: true,
                        is_new_leg: true,
                    }
                )
        });
        has_bolling_long && has_engulfing_short && has_new_bearish_leg
    }
    /// 封装收紧空头信号止损靠近零轴MACD，减少回测策略调用方重复实现相同细节。
    fn tighten_short_signal_stop_near_zero_macd(
        entry_price: f64,
        current_stop: f64,
        macd_value: &MacdSignalValue,
    ) -> Option<f64> {
        if current_stop <= entry_price || macd_value.histogram.abs() >= 2.0 {
            return None;
        }
        Some(entry_price + (current_stop - entry_price) * 0.5)
    }
    /// 移除位于入场价错误一侧的信号止损，保留 ATR 与最大亏损保护作为后备。
    fn reject_non_protective_signal_stop(signal_result: &mut SignalResult) -> bool {
        let (Some(entry_price), Some(stop_price)) = (
            signal_result.open_price,
            signal_result.signal_kline_stop_loss_price,
        ) else {
            return false;
        };
        let is_protective = match signal_result.direction {
            rust_quant_domain::SignalDirection::Long => stop_price < entry_price,
            rust_quant_domain::SignalDirection::Short => stop_price > entry_price,
            rust_quant_domain::SignalDirection::None
            | rust_quant_domain::SignalDirection::Close => true,
        };
        if is_protective {
            return false;
        }
        signal_result.signal_kline_stop_loss_price = None;
        signal_result.stop_loss_source = Some("SignalStop_DirectionRejected".to_string());
        true
    }
    /// 计算 回测与策略研究 指标，保持公式和边界处理集中可审计。
    fn calculate_best_stop_loss_price(
        &self,
        last_data_item: &CandleItem,
        signal_result: &mut SignalResult,
        conditions: &[(SignalType, SignalCondition)],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) {
        // 检查是否有吞没形态信号
        let has_engulfing_signal = Self::has_signal_type(conditions, SignalType::Engulfing);
        let disable_long_engulfing_stop_raise =
            std::env::var("VEGAS_DISABLE_LONG_ENGULFING_STOP_RAISE")
                .ok()
                .as_deref()
                == Some("1");
        let disable_conflicting_long_engulfing_stop_raise =
            std::env::var("VEGAS_DISABLE_CONFLICTING_LONG_ENGULFING_STOP_RAISE")
                .ok()
                .as_deref()
                == Some("1");
        let conflicting_long_engulfing_stop_raise = disable_conflicting_long_engulfing_stop_raise
            && signal_result.direction == rust_quant_domain::SignalDirection::Long
            && !vegas_indicator_signal_values.fib_retracement_value.in_zone
            && vegas_indicator_signal_values
                .bollinger_value
                .is_short_signal
            && vegas_indicator_signal_values.ema_distance_filter.state == EmaDistanceState::TooFar;
        // 如果是吞没形态信号，使用开盘价作为止损价格
        if has_engulfing_signal
            && !(disable_long_engulfing_stop_raise
                && signal_result.direction == rust_quant_domain::SignalDirection::Long)
            && !conflicting_long_engulfing_stop_raise
        {
            let candidate = last_data_item.o();
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c());
            let is_protective = match signal_result.direction {
                rust_quant_domain::SignalDirection::Long => candidate < entry_price,
                rust_quant_domain::SignalDirection::Short => candidate > entry_price,
                rust_quant_domain::SignalDirection::None
                | rust_quant_domain::SignalDirection::Close => false,
            };
            if is_protective {
                signal_result.signal_kline_stop_loss_price = Some(candidate);
            }
        }
        // 【已禁用】只保留吞没形态止损，其他情况不设置信号线止损
        // if let Some(stop_loss_price) = utils::calculate_best_stop_loss_price(
        //     last_data_item,
        //     signal_result.should_buy.unwrap_or(false),
        //     signal_result.should_sell.unwrap_or(false),
        // ) {
        //     signal_result.signal_kline_stop_loss_price = Some(stop_loss_price);
        // }
    }
}
