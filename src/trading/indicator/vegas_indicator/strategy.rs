use crate::trading::indicator::bollings::BollingBandsSignalConfig;
use crate::trading::indicator::signal_weight::{
    SignalCondition, SignalDirect, SignalType, SignalWeightsConfig,
};
use crate::trading::strategy::strategy_common::{
    BackTestResult, BasicRiskStrategyConfig, SignalResult,
};
use crate::CandleItem;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

use super::config::*;
use super::indicator_combine::IndicatorCombine;
use super::signal::*;
use super::trend;
use super::utils;
use crate::enums::common::EnumAsStrTrait;
use crate::enums::common::PeriodEnum;

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
                should_buy: false,
                should_sell: false,
                open_price: 0.0,
                best_open_price: None,
                best_take_profit_price: None,
                signal_kline_stop_loss_price: None,
                ts: 0,
                single_value: None,
                single_result: None,
            };
        }

        let last_data_item = match data_items.last() {
            Some(item) => item,
            None => {
                return SignalResult {
                    should_buy: false,
                    should_sell: false,
                    open_price: 0.0,
                    best_open_price: None,
                    best_take_profit_price: None,
                    signal_kline_stop_loss_price: None,
                    ts: 0,
                    single_value: None,
                    single_result: None,
                }
            }
        };

        // 初始化交易信号
        let mut signal_result = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: last_data_item.c,
            best_open_price: None,
            best_take_profit_price: None,
            signal_kline_stop_loss_price: None,
            ts: last_data_item.ts,
            single_value: None,
            single_result: None,
        };

        let mut conditions = Vec::with_capacity(10);

        // 优先判断成交量
        if let Some(volume_signal) = &self.volume_signal {
            let res = self.check_volume_trend(&vegas_indicator_signal_values.volume_value);
            if volume_signal.is_force_dependent
                && res.volume_ratio < volume_signal.volume_decrease_ratio
            {
                return signal_result;
            }
            conditions.push((
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: res.is_increasing,
                    ratio: res.volume_ratio,
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
            let current_rsi = self.get_valid_rsi(
                data_items,
                &vegas_indicator_signal_values.rsi_value,
                vegas_indicator_signal_values.ema_values,
            );
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

        // 计算得分
        let score = weights.calculate_score(conditions.clone());
        // 计算分数到达指定值
        if let Some(signal_direction) = weights.is_signal_valid(&score) {
            match signal_direction {
                SignalDirect::IsLong => {
                    signal_result.should_buy = true;
                    if risk_config.is_used_signal_k_line_stop_loss {
                        self.calculate_best_stop_loss_price(
                            last_data_item,
                            &mut signal_result,
                            &conditions,
                        );
                    }
                }
                SignalDirect::IsShort => {
                    signal_result.should_sell = true;
                    if risk_config.is_used_signal_k_line_stop_loss {
                        self.calculate_best_stop_loss_price(
                            last_data_item,
                            &mut signal_result,
                            &conditions,
                        );
                    }
                }
            }
        }

        // 可选：添加详细信息到结果中
        if signal_result.should_buy || signal_result.should_sell {
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }

        signal_result
    }

    /// 获取指标组合
    pub fn get_indicator_combine(&self) -> IndicatorCombine {
        use crate::trading::indicator::bollings::BollingBandsPlusIndicator;
        use crate::trading::indicator::ema_indicator::EmaIndicator;
        use crate::trading::indicator::k_line_engulfing_indicator::KlineEngulfingIndicator;
        use crate::trading::indicator::k_line_hammer_indicator::KlineHammerIndicator;
        use crate::trading::indicator::rsi_rma_indicator::RsiIndicator;
        use crate::trading::indicator::volume_indicator::VolumeRatioIndicator;

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
    pub fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        use crate::trading::strategy::strategy_common;

        let min_length = self.get_min_data_length();
        let mut indicator_combine = self.get_indicator_combine();
        strategy_common::run_back_test(
            {
                let signal_weights = self
                    .signal_weights
                    .as_ref()
                    .expect("信号权重配置不能为空")
                    .clone();
                move |candles, multi_indicator_values| {
                    self.get_trade_signal(
                        candles,
                        multi_indicator_values,
                        &signal_weights,
                        &risk_strategy_config,
                    )
                }
            },
            candles,
            risk_strategy_config,
            min_length,
            &mut indicator_combine,
        )
    }

    // 私有辅助方法
    fn check_volume_trend(&self, volume_trend: &VolumeTrendSignalValue) -> VolumeTrendSignalValue {
        if let Some(volume_signal) = &self.volume_signal {
            VolumeTrendSignalValue {
                is_increasing: volume_trend.volume_ratio > volume_signal.volume_increase_ratio,
                is_decreasing: volume_trend.volume_ratio < volume_signal.volume_decrease_ratio,
                is_stable: volume_trend.volume_ratio >= volume_signal.volume_decrease_ratio
                    && volume_trend.volume_ratio <= volume_signal.volume_increase_ratio,
                volume_ratio: volume_trend.volume_ratio,
                volume_value: volume_trend.volume_value,
            }
        } else {
            VolumeTrendSignalValue::default()
        }
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
    ) -> f64 {
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

            // // 过滤逻辑
            if (bolling_bands.is_long_signal || bolling_bands.is_short_signal)
                && self.period == PeriodEnum::OneDayUtc.as_str()
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
                    if data_items.last().expect("数据不能为空").ts == 1756051200000 {
                        println!(
                            "bolling_bands.is_short_signal: {:?}",
                            bolling_bands.is_short_signal
                        );
                        println!(
                            "ema_signal_values.ema1_value: {:?}",
                            ema_signal_values.ema1_value
                        );
                    }
                    bolling_bands.is_short_signal = false;
                    bolling_bands.is_force_filter_signal = true;
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
        _conditions: &Vec<(SignalType, SignalCondition)>,
    ) {
        // 使用工具函数计算止损价格
        if let Some(stop_loss_price) = utils::calculate_best_stop_loss_price(
            last_data_item,
            signal_result.should_buy,
            signal_result.should_sell,
        ) {
            signal_result.signal_kline_stop_loss_price = Some(stop_loss_price);
        }
    }
}
