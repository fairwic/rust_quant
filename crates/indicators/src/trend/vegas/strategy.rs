use std::env;

use crate::signal_weight::{SignalCondition, SignalDirect, SignalType, SignalWeightsConfig};
use crate::volatility::bollinger::BollingBandsSignalConfig;
use rust_quant_common::enums::common::{EnumAsStrTrait, PeriodEnum};
use rust_quant_common::utils::time as time_util;
use rust_quant_common::CandleItem;
use rust_quant_domain::Strategy;
use rust_quant_domain::{BacktestResult, BasicRiskStrategyConfig, SignalResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

use super::config::*;
use super::ema_filter::{self, EmaDistanceConfig, EmaDistanceState};
use super::fake_breakout::{self, FakeBreakoutConfig};
use super::indicator_combine::IndicatorCombine;
use super::signal::*;
use super::trend;
use super::utils;
use crate::trend::counter_trend;

/// Vegas综合策略配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VegasStrategy {
    /// 周期
    pub period: String,
    /// 最小需要的k线数量
    pub min_k_line_num: usize,
    /// EMA信号配置
    pub ema_signal: Option<EmaSignalConfig>,
    /// 成交量信号配置
    pub volume_signal: Option<VolumeSignalConfig>,
    /// EMA趋势配置
    pub ema_touch_trend_signal: Option<EmaTouchTrendSignalConfig>,
    /// RSI信号配置
    pub rsi_signal: Option<RsiSignalConfig>,
    /// 布林带信号配置
    pub bolling_signal: Option<BollingBandsSignalConfig>,
    /// 权重配置
    pub signal_weights: Option<SignalWeightsConfig>,
    /// 吞没形态指标
    pub engulfing_signal: Option<EngulfingSignalConfig>,
    /// 锤子形态指标
    pub kline_hammer_signal: Option<KlineHammerConfig>,
}

impl VegasStrategy {
    pub fn new(period: String) -> Self {
        Self {
            period: period,
            min_k_line_num: 7000,
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
            ema_touch_trend_signal: Some(EmaTouchTrendSignalConfig::default()),
            rsi_signal: Some(RsiSignalConfig::default()),
            bolling_signal: Some(BollingBandsSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            kline_hammer_signal: Some(KlineHammerConfig::default()),
        }
    }

    pub fn get_strategy_name() -> String {
        "vegas".to_string()
    }

    /// 获取最小数据长度
    pub fn get_min_data_length(&mut self) -> usize {
        self.min_k_line_num
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
                move_stop_open_price_when_touch_price: None,
                ts: Some(0),
                single_value: None,
                single_result: None,
                counter_trend_pullback_take_profit_price: None,
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
                    move_stop_open_price_when_touch_price: None,
                    counter_trend_pullback_take_profit_price: None,
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
            ts: Some(last_data_item.ts),
            single_value: None,
            single_result: None,
            counter_trend_pullback_take_profit_price: None,
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
            move_stop_open_price_when_touch_price: None,
        };

        let mut conditions = Vec::with_capacity(10);

        // 优先判断成交量
        if let Some(volume_signal) = &self.volume_signal {
            let is_than_vol_ratio =
                self.check_volume_trend(&vegas_indicator_signal_values.volume_value);
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
                    return signal_result;
                }
            };

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

        // ================================================================
        // 【新增】假突破信号检测
        // ================================================================
        let fake_breakout_config = FakeBreakoutConfig::default();
        let fake_breakout_signal = fake_breakout::detect_fake_breakout(data_items, &fake_breakout_config);
        vegas_indicator_signal_values.fake_breakout_value = fake_breakout_signal;

        // 假突破信号加入conditions
        if fake_breakout_signal.has_signal() {
            conditions.push((
                SignalType::FakeBreakout,
                SignalCondition::FakeBreakout {
                    is_bullish: fake_breakout_signal.is_bullish_fake_breakout,
                    is_bearish: fake_breakout_signal.is_bearish_fake_breakout,
                    strength: fake_breakout_signal.strength,
                },
            ));
        }

        // ================================================================
        // 【新增】EMA距离过滤
        // ================================================================
        let ema_distance_config = EmaDistanceConfig::default();
        let ema_distance_filter = ema_filter::apply_ema_distance_filter(
            last_data_item.c,
            &vegas_indicator_signal_values.ema_values,
            &ema_distance_config,
        );
        vegas_indicator_signal_values.ema_distance_filter = ema_distance_filter;

        // ================================================================
        // 计算得分
        // ================================================================
        let score = weights.calculate_score(conditions.clone());

        // 计算分数到达指定值
        if let Some(signal_direction) = weights.is_signal_valid(&score) {
            match signal_direction {
                SignalDirect::IsLong => {
                    signal_result.should_buy = Some(true);
                }
                SignalDirect::IsShort => {
                    signal_result.should_sell = Some(true);
                }
            }
        }

        // ================================================================
        // 【新增】假突破直接开仓逻辑（暂时禁用，改为权重计算）
        // 根据第一性原理：假突破信号直接市价开仓
        // 注意：此逻辑过于激进，导致盈利下降，暂时禁用
        // 假突破信号已经加入了权重计算，会影响最终得分
        // ================================================================
        // TODO: 需要更精细的假突破确认条件后再启用
        // if fake_breakout_signal.is_bullish_fake_breakout && fake_breakout_signal.volume_confirmed {
        //     signal_result.should_buy = Some(true);
        //     signal_result.should_sell = Some(false);
        // }
        // if fake_breakout_signal.is_bearish_fake_breakout && fake_breakout_signal.volume_confirmed {
        //     signal_result.should_sell = Some(true);
        //     signal_result.should_buy = Some(false);
        // }

        // ================================================================
        // 【新增】应用EMA距离过滤（暂时禁用）
        // 规则：
        // - 空头排列 + 距离过远 + 收盘价 > ema3 → 不做多
        // - 多头排列 + 距离过远 + 收盘价 < ema3 → 不做空
        // 注意：此过滤器可能过滤掉有效信号，暂时禁用
        // ================================================================
        // TODO: 调整EMA距离阈值后再启用
        // if ema_distance_filter.should_filter_long && signal_result.should_buy.unwrap_or(false) {
        //     if !fake_breakout_signal.is_bullish_fake_breakout {
        //         signal_result.should_buy = Some(false);
        //     }
        // }
        // if ema_distance_filter.should_filter_short && signal_result.should_sell.unwrap_or(false) {
        //     if !fake_breakout_signal.is_bearish_fake_breakout {
        //         signal_result.should_sell = Some(false);
        //     }
        // }

        // ================================================================
        // 【新增】成交量递减过滤（暂时禁用）
        // 规则：近3根K线成交量递减 Vol(n-2) > Vol(n-1) > Vol(n) → 忽略信号
        // 注意：此过滤器可能过于严格，暂时禁用以观察效果
        // ================================================================
        // TODO: 需要更精细的成交量过滤条件
        // let recent_volumes = ema_filter::extract_recent_volumes(data_items, 3);
        // if ema_filter::check_volume_decreasing_filter(&recent_volumes) {
        //     if signal_result.should_buy.unwrap_or(false) && !fake_breakout_signal.is_bullish_fake_breakout {
        //         signal_result.should_buy = Some(false);
        //     }
        //     if signal_result.should_sell.unwrap_or(false) && !fake_breakout_signal.is_bearish_fake_breakout {
        //         signal_result.should_sell = Some(false);
        //     }
        // }

        // 可选：添加详细信息到结果中
        if signal_result.should_buy.unwrap_or(false)
            || signal_result.should_sell.unwrap_or(false)
                && env::var("ENABLE_RANDOM_TEST").unwrap_or_default() != "true"
        {
            //如果有使用信号k线止损
            if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
                self.calculate_best_stop_loss_price(
                    last_data_item,
                    &mut signal_result,
                    &conditions,
                );
            }
            //如果有使用逆势回调止盈
            if risk_config
                .is_counter_trend_pullback_take_profit
                .unwrap_or(false)
            {
                counter_trend::calculate_counter_trend_pullback_take_profit_price(
                    &data_items,
                    &mut signal_result,
                    &conditions,
                    vegas_indicator_signal_values.ema_values.ema1_value,
                );
            }
            // TODO: 这些字段原本用于调试，现在类型不匹配，暂时注释
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }
        signal_result
    }

    /// 获取指标组合
    pub fn get_indicator_combine(&self) -> IndicatorCombine {
        use crate::ema_indicator::EmaIndicator;
        use crate::momentum::rsi::RsiIndicator;
        use crate::pattern::engulfing::KlineEngulfingIndicator;
        use crate::pattern::hammer::KlineHammerIndicator;
        use crate::volatility::bollinger::BollingBandsPlusIndicator;
        use crate::volume_indicator::VolumeRatioIndicator;

        let mut indicator_combine = IndicatorCombine::default();

        // 添加吞没形态
        if let Some(_engulfing_signal) = &self.engulfing_signal {
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

        indicator_combine
    }

    /// 运行回测
    ///
    /// 注意：此方法不能在 indicators 包中完整实现，因为 BacktestResult 在不同包中定义不同
    /// 实际回测逻辑应在 strategies 或 orchestration 包中调用，使用 get_indicator_combine() 和 get_trade_signal()
    pub fn run_test(
        &mut self,
        _candles: &Vec<CandleItem>,
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
    fn check_volume_trend(&self, volume_trend: &VolumeTrendSignalValue) -> bool {
        if let Some(volume_signal_config) = &self.volume_signal {
            return volume_trend.volume_ratio > volume_signal_config.volume_increase_ratio;
        }
        return false;
    }

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

    fn check_ema_touch_trend(
        &self,
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> EmaTouchTrendSignalValue {
        if let Some(ema_touch_trend_signal) = &self.ema_touch_trend_signal {
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

            let body_ratio = data_items.last().expect("数据不能为空").body_ratio();
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

    fn check_engulfing_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: &mut VegasIndicatorSignalValue,
        conditions: &mut Vec<(SignalType, SignalCondition)>,
        _ema_value: EmaSignalValue,
    ) {
        let mut is_engulfing = false;
        let last_data_item = data_items.last().expect("数据不能为空");

        if let Some(engulfing_signal) = &self.engulfing_signal {
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

    fn calculate_best_stop_loss_price(
        &self,
        last_data_item: &CandleItem,
        signal_result: &mut SignalResult,
        conditions: &Vec<(SignalType, SignalCondition)>,
    ) {
        // 检查是否有吞没形态信号
        let has_engulfing_signal = conditions
            .iter()
            .any(|(signal_type, _)| matches!(signal_type, SignalType::Engulfing));

        // 如果是吞没形态信号，使用开盘价作为止损价格
        if has_engulfing_signal {
            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o());
            return;
        }

        // 其他情况使用工具函数计算止损价格
        if let Some(stop_loss_price) = utils::calculate_best_stop_loss_price(
            last_data_item,
            signal_result.should_buy.unwrap_or(false),
            signal_result.should_sell.unwrap_or(false),
        ) {
            signal_result.signal_kline_stop_loss_price = Some(stop_loss_price);
        }
    }
}
