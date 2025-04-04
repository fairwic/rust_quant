use crate::trading::indicator::rsi_rma_indicator::RsiIndicator;
use crate::trading::indicator::signal_weight::{
    SignalCondition, SignalDeriect, SignalScoreWithDeriact, SignalType, SignalWeightsConfig,
};
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common;
use crate::trading::strategy::strategy_common::{
    BackTestResult, BasicRiskStrategyConfig, SignalResult,
};
use crate::CandleItem;
use fast_log::print;
use futures::io::sink;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::f32::consts::E;
use std::fmt::Display;
use std::sync::Arc;
use ta::indicators::{BollingerBands, BollingerBandsOutput, ExponentialMovingAverage};
use ta::indicators::{MovingAverageConvergenceDivergence, RelativeStrengthIndex};
use ta::{Close, DataItem, High, Low, Next, Open, Volume};
use tracing::error;

use super::bollings::BollingerBandsSignalConfig;
use super::ema_indicator::EmaIndicator;
use super::engulfing_indicator::EngulfingIndicator;
use super::is_big_kline::IsBigKLineIndicator;
use super::volume_indicator::VolumeRatioIndicator;

//吞没形态指标
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EngulfingSignalConfig {
    //吞没形态指标
    //是否有上影线
    pub is_upper_shadow: bool,
    //是否有下影线
    pub is_lower_shadow: bool,
    //是否吞没
    pub is_engulfing: bool,
    //实体部分占比
    pub body_ratio: f64,
    //是否开仓
    pub is_open: bool,
}
//k线路形态指标值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct EngulfingSignalValue {
    //是否吞没形态
    pub is_engulfing: bool,
    //是否是锤子形态
    pub is_hammer: bool,
    //是否是上吊线形态
    pub is_hanging_man: bool,
    //是否是流星形态
    pub is_shooting_star: bool,
    //是否是十字星形态
    pub is_doji: bool,
    pub is_valid_engulfing: bool,
    pub body_ratio: f64,
}

impl Default for EngulfingSignalConfig {
    fn default() -> Self {
        Self {
            is_upper_shadow: true,
            is_lower_shadow: true,
            is_engulfing: true,
            body_ratio: 0.5,
            is_open: true,
        }
    }
}

// 成交量趋势
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct VolumeTrendSignalValue {
    pub is_increasing: bool,
    pub is_decreasing: bool,
    pub is_stable: bool,   // 是否稳定
    pub volume_ratio: f64, // 添加 volume_ratio 字段
    pub volume_value: f64, // 添加 volume_value 字段
}

// 成交量信号配置
#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeSignalConfig {
    pub volume_bar_num: usize,      // 看前10根K线
    pub volume_increase_ratio: f64, // 放量倍数
    pub volume_decrease_ratio: f64, // 缩量倍数
    pub is_open: bool,              // 是否开启
}
impl Default for VolumeSignalConfig {
    fn default() -> Self {
        Self {
            volume_bar_num: 3,
            volume_increase_ratio: 2.5,
            volume_decrease_ratio: 2.5,
            is_open: true,
        }
    }
}

// ema信号配置
#[derive(Debug, Serialize, Deserialize)]
pub struct EmaSignalConfig {
    pub ema1_length: usize,
    pub ema2_length: usize,
    pub ema3_length: usize,
    pub ema4_length: usize,
    pub ema5_length: usize,
    pub ema_breakthrough_threshold: f64, // 新增：ema突破价格的阈值
    pub is_open: bool,
}
impl Default for EmaSignalConfig {
    fn default() -> Self {
        Self {
            ema1_length: 12,
            ema2_length: 144,
            ema3_length: 169,
            ema4_length: 576,
            ema5_length: 676,
            ema_breakthrough_threshold: 0.003,
            is_open: true,
        }
    }
}

#[derive(Debug)]
pub struct IndicatorCombine {
    pub ema_indicator: Option<EmaIndicator>,
    pub rsi_indicator: Option<RsiIndicator>,
    pub volume_indicator: Option<VolumeRatioIndicator>,
    pub bollinger_indicator: Option<BollingerBands>,
    pub engulfing_indicator: Option<EngulfingIndicator>,
}
impl Default for IndicatorCombine {
    fn default() -> Self {
        Self {
            ema_indicator: None,
            rsi_indicator: None,
            volume_indicator: None,
            bollinger_indicator: None,
            engulfing_indicator: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EmaSignalValue {
    pub ema1_value: f64,
    pub ema2_value: f64,
    pub ema3_value: f64,
    pub ema4_value: f64,
    pub ema5_value: f64,
    //是否多头排列
    pub is_long_trend: bool,
    //是否空头排列
    pub is_short_trend: bool,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BollingerSignalValue {
    pub lower: f64,
    pub upper: f64,
    pub middle: f64,
    pub is_long_signal: bool,
    pub is_short_signal: bool,
    pub is_close_signal: bool,
    //虽然触发了布林带开多，或者开空，但是被过滤了
    pub is_foce_filter_signal: bool,
}

// rsi信号配置
#[derive(Debug, Serialize, Deserialize)]
pub struct RsiSignalConfig {
    pub rsi_length: usize,   // rsi周期
    pub rsi_oversold: f64,   // rsi超卖阈值
    pub rsi_overbought: f64, // rsi超买阈值
    pub is_open: bool,       // 是否开启
}
impl Default for RsiSignalConfig {
    fn default() -> Self {
        Self {
            rsi_length: 12,
            rsi_oversold: 25.0,
            rsi_overbought: 75.0,
            is_open: true,
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct RsiSignalValue {
    pub rsi_value: f64,      // rsi值
    pub is_oversold: bool,   // 是否超卖
    pub is_overbought: bool, // 是否超买
}

impl VolumeTrendSignalValue {
    pub fn new(
        is_increasing: bool,
        is_decreasing: bool,
        is_stable: bool,
        volume_ratio: f64,
        volume_value: f64,
    ) -> Self {
        Self {
            is_increasing,
            is_decreasing,
            is_stable,
            volume_ratio,
            volume_value,
        }
    }
}

// ema趋势
#[derive(Debug, Serialize, Deserialize)]
pub struct EmaTouchTrendSignalConfig {
    pub ema1_with_ema2_ratio: f64,      //eam2与eam3的相差幅度
    pub ema2_with_ema3_ratio: f64,      //eam2与eam3的相差幅度
    pub ema3_with_ema4_ratio: f64,      //eam2与eam3的相差幅度
    pub ema4_with_ema5_ratio: f64,      //eam2与eam3的相差幅度
    pub price_with_ema_high_ratio: f64, //价格与ema4的相差幅度
    pub price_with_ema_low_ratio: f64,  //价格与ema4的相差幅度
    pub is_open: bool,                  //是否开启
}
impl Default for EmaTouchTrendSignalConfig {
    fn default() -> Self {
        Self {
            ema1_with_ema2_ratio: 1.010,      //ema1与ema2的相差幅度
            ema4_with_ema5_ratio: 1.012,      //ema4与ema5的相差幅度
            ema3_with_ema4_ratio: 1.012,      //ema3与ema4的相差幅度
            ema2_with_ema3_ratio: 1.012,      //ema2与ema3的相差幅度
            price_with_ema_high_ratio: 1.005, //价格与ema4的相差幅度
            price_with_ema_low_ratio: 0.995,  //价格与ema4的相差幅度
            is_open: true,                    //是否开启
        }
    }
}
// ema趋势
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EmaTouchTrendSignalValue {
    pub is_uptrend: bool,                          //是否多头趋势
    pub is_downtrend: bool,                        //是否空头趋势
    pub is_in_uptrend_touch_ema2: bool,            //是否在多头趋势触碰ema2
    pub is_in_uptrend_touch_ema3: bool,            //是否在多头趋势触碰ema3
    pub is_in_uptrend_touch_ema2_ema3_nums: usize, //当前多头趋势中触碰ema2和ema3的次数

    pub is_in_uptrend_touch_ema4: bool, //是否在多头趋势触碰ema4
    pub is_in_uptrend_touch_ema5: bool, //是否在多头趋势触碰ema4
    pub is_in_uptrend_touch_ema4_ema5_nums: usize, //当前多头趋势中触碰ema4和ema5的次数

    pub is_in_downtrend_touch_ema2: bool, //是否在空头趋势触碰ema2
    pub is_in_downtrend_touch_ema3: bool, //是否在空头趋势触碰ema3
    pub is_in_downtrend_touch_ema2_ema3_nums: usize, //当前空头趋势触碰ema2和ema3的次数

    pub is_in_downtrend_touch_ema4: bool, //是否在空头趋势触碰ema4
    pub is_in_downtrend_touch_ema5: bool, //是否在空头趋势触碰ema5
    pub is_in_downtrend_touch_ema4_ema5_nums: usize, //当前空头趋势中触碰ema4和ema5的次数

    pub is_long_signal: bool,  //是否多头开仓
    pub is_short_signal: bool, //是否空头开仓
}
impl Default for EmaTouchTrendSignalValue {
    fn default() -> Self {
        Self {
            is_uptrend: false,
            is_downtrend: false,
            is_in_uptrend_touch_ema2: false,
            is_in_uptrend_touch_ema3: false,
            is_in_uptrend_touch_ema2_ema3_nums: 0,
            is_in_uptrend_touch_ema4: false,
            is_in_uptrend_touch_ema5: false,
            is_in_uptrend_touch_ema4_ema5_nums: 0,
            is_in_downtrend_touch_ema2: false,
            is_in_downtrend_touch_ema3: false,
            is_in_downtrend_touch_ema2_ema3_nums: 0,
            is_in_downtrend_touch_ema4: false,
            is_in_downtrend_touch_ema5: false,
            is_in_downtrend_touch_ema4_ema5_nums: 0,
            is_long_signal: false,
            is_short_signal: false,
        }
    }
}

// 新增：检查均线交叉
pub struct EmaCross {
    pub is_golden_cross: bool,
    pub is_death_cross: bool,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct VegasIndicatorSignalValue {
    pub ema_values: EmaSignalValue,                // ema信号配置
    pub volume_value: VolumeTrendSignalValue,      // 新增：成交量信号配置
    pub ema_touch_value: EmaTouchTrendSignalValue, // ema趋势
    pub rsi_value: RsiSignalValue,                 //rsi信号配置
    pub bollinger_value: BollingerSignalValue,     //bollinger信号配置
    pub signal_weights_value: SignalWeightsConfig, // 新增权重配置
    pub engulfing_value: EngulfingSignalValue,     //吞没形态指标
}

/// vegae 综合策略配置
/// 1. ema信号配置
/// 2. 成交量信号配置
/// 3. ema趋势
/// 4. rsi信号配置
/// 5. bollinger信号配置
/// 6. 新增权重配置
/// 7. 新增吞没形态指标
#[derive(Debug, Serialize, Deserialize)]
pub struct VegasStrategy {
    pub ema_signal: Option<EmaSignalConfig>,       // ema信号配置
    pub volume_signal: Option<VolumeSignalConfig>, // 新增：成交量信号配置
    pub ema_touch_trend_signal: Option<EmaTouchTrendSignalConfig>, // ema趋势
    pub rsi_signal: Option<RsiSignalConfig>,       //rsi信号配置
    pub bollinger_signal: Option<BollingerBandsSignalConfig>, //bollinger信号配置
    pub signal_weights: Option<SignalWeightsConfig>, // 新增权重配置
    //新增吞没形态指标
    pub engulfing_signal: Option<EngulfingSignalConfig>, // 新增吞没形态指标
}

impl Default for VegasStrategy {
    fn default() -> Self {
        Self {
            ema_touch_trend_signal: Some(EmaTouchTrendSignalConfig::default()),
            bollinger_signal: Some(BollingerBandsSignalConfig::default()),
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
            rsi_signal: Some(RsiSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig {
                weights: vec![
                    (SignalType::SimpleBreakEma2through, 2.0),
                    (SignalType::VolumeTrend, 1.5),
                    (SignalType::Rsi, 1.0),
                    (SignalType::TrendStrength, 1.5),
                    (SignalType::EmaDivergence, 1.8),
                    (SignalType::PriceLevel, 1.2),
                ],
                min_total_weight: 3.0, // 需要至少3分才触发信号
            }),
            engulfing_signal: Some(EngulfingSignalConfig::default()),
        }
    }
}

impl VegasStrategy {
    pub fn get_min_data_length(&mut self) -> usize {
        3400
    }

    /// 获取交易信号
    pub fn get_trade_signal(
        &mut self,
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &mut VegasIndicatorSignalValue,
        weights: &SignalWeightsConfig,
    ) -> SignalResult {
        // 初始化交易信号
        let mut signal_result = SignalResult {
            should_buy: false,
            should_sell: false,
            price: data_items.last().unwrap().c,
            ts: data_items.last().unwrap().ts,
            single_value: None,
            single_result: None,
        };

        let last_data_item = data_items.last().unwrap();

        let mut conditions = vec![];
        // 检查ema2被突破
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

        //新增ema排列，回调触碰关键均线位置
        let ema_trend =
            self.calculate_ema_touch_trend(data_items, vegas_indicator_signal_values.ema_values);
        if ema_trend.is_long_signal || ema_trend.is_short_signal {
            conditions.push((
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: ema_trend.is_long_signal,
                    is_short_signal: ema_trend.is_short_signal,
                },
            ));
        }

        //成交量
        if let Some(volume_signal) = &self.volume_signal {
            let res = self.check_volume_trend(&vegas_indicator_signal_values.volume_value);
            conditions.push((
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: res.is_increasing,
                    ratio: res.volume_ratio,
                },
            ))
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

        //判断布林带
        if let Some(bollinger_signal) = &self.bollinger_signal {
            let bollinger_value =
                self.check_bollinger_signal(data_items, vegas_indicator_signal_values.clone());
            conditions.push((
                SignalType::Bollinger,
                SignalCondition::Bollinger {
                    is_long_signal: bollinger_value.is_long_signal,
                    is_short_signal: bollinger_value.is_short_signal,
                    is_close_signal: bollinger_value.is_close_signal,
                },
            ));
        }

        // 检查突破的持续性
        let breakthrough_confirmed = self.check_breakthrough_confirmation(data_items, price_above);

        //计算振幅
        let k_line_amplitude = self.calculate_k_line_amplitude(data_items);

        //计算吞没形态
        let is_engulfing = self.check_engulfing_signal(data_items, vegas_indicator_signal_values);
        if is_engulfing {
            if data_items.last().unwrap().c > data_items.last().unwrap().o() {
                conditions.push((
                    SignalType::Engulfing,
                    SignalCondition::Engulfing {
                        is_long_engulfing: true,
                        is_short_engulfing: false,
                    },
                ));
            } else {
                conditions.push((
                    SignalType::Engulfing,
                    SignalCondition::Engulfing {
                        is_long_engulfing: false,
                        is_short_engulfing: true,
                    },
                ));
            }
        }


        // println!("vegas_indicator_signal_values: {:#?}", vegas_indicator_signal_values);
        //todo 可以考虑在出现上影线，且价格在恰好收盘在均线下方，可以开空，反之可以开多
        // 计算得分
        let score = weights.calculate_score(conditions.clone());
        //计算分数到达指定值
        if let Some(signal_direction) = weights.is_signal_valid(&score) {
            match signal_direction {
                SignalDeriect::IsLong => {
                    signal_result.should_buy = true;
                    signal_result.single_value =
                        Some(json!(vegas_indicator_signal_values).to_string());
                    signal_result.single_result = Some(json!(conditions).to_string());
                }
                SignalDeriect::IsShort => {
                    signal_result.should_sell = true;
                    signal_result.single_value =
                        Some(json!(vegas_indicator_signal_values).to_string());
                    signal_result.single_result = Some(json!(conditions).to_string());
                }
            }
        };

        signal_result
    }

    //新增函数计算当前k线价格的振幅
    fn calculate_k_line_amplitude(&self, data_items: &[CandleItem]) -> f64 {
        let mut amplitude = 0.0;
        if let Some(last_item) = data_items.last() {
            // 计算最高价和最低价之间的差异
            let high = last_item.h();
            let low = last_item.l();
            // 使用开盘价作为基准计算振幅百分比
            let open = last_item.o();
            if open != 0.0 {
                // 振幅计算: (最高价 - 最低价) / 开盘价 * 100
                amplitude = (high - low) / open * 100.0;
            }
        }
        amplitude
    }

    //获取有效的rsi
    fn get_valid_rsi(
        &self,
        data_items: &[CandleItem],
        rsi_value: &RsiSignalValue,
        ema_value: EmaSignalValue,
    ) -> f64 {
        // 如果当前k线价格波动比较大，且k线路的实体部分占比大于80%,表明当前k线为大阳线或者大阴线，则不使用rsi指标,因为大概率趋势还会继续  2025-03-03 00:00:00,2025-03-09 18:00:00
        let is_big_k_line =
            IsBigKLineIndicator::new(70.0).is_big_k_line(data_items.last().unwrap());
        if is_big_k_line {
            return 50.0;
        }
        let current_rsi = rsi_value.rsi_value;
        return current_rsi;
        // todo 12/19 04:00 rsi超卖，但是不在ema4附近，所以无效
        // todo 12/19 23:00
        // todo 2025-02-25 06:00:00 rsi超卖，但是价格开盘和收盘都低于ema1，所以无法买入
        // todo 2025 03-10 06:00:00 确实有效的rsi id=103
        // todo 2025-01-13 18:00:00
        //如果当前价格是下跌,判断是不是ema4附近，否则为无效rsi
        // if data_items.last().unwrap().close() < ema_value.ema4_value {
        //     current_rsi
        // } else {
        //     return 0.0;
        // }
    }

    // 辅助方法：检查成交量是否显著增加
    // fn check_volume_increase(&self, data: &[DataItem]) -> bool {
    //     if data.len() < 5 { return false; }
    //
    //     let current_volume = data.last().unwrap().volume();  // 使用真实成交量
    //     let avg_volume: f64 = data[data.len() - 6..data.len() - 1].iter().map(|x| x.volume())  // 使用真实成交量.sum::<f64>() / 5.0;
    //
    //     // println!("成交量检查 - 当前: {}, 平均: {}, 倍数: {}", current_volume, avg_volume, current_volume / avg_volume);
    //     current_volume > avg_volume * self.volume_signal.volume_increase_ratio // 倍数大于1.5
    // }
    // 辅助方法：计算EMA趋势
    fn calculate_ema_touch_trend(
        &mut self,
        data_itms: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> EmaTouchTrendSignalValue {
        //判断ema 是否空头排列，或者多头排列或者多头排列
        let mut ema_touch_trend_value = EmaTouchTrendSignalValue::default();

        if let Some(ema_touch_trend_signal) = &self.ema_touch_trend_signal {
            //todo  优化时间点 2024-12-09 08:00:00
            if ema_value.ema2_value > ema_value.ema3_value
                && ema_value.ema3_value > ema_value.ema4_value
            {
                ema_touch_trend_value.is_uptrend = true;
                //当前ema_vaue_1 >emalue_2 的时候， 价格最低下跌到em2附近的时候，且ema1 与 ema2 相差幅度大于0.012
                if ema_value.ema1_value > ema_value.ema2_value
                    && data_itms.last().unwrap().l()
                        <= ema_value.ema2_value * ema_touch_trend_signal.price_with_ema_high_ratio
                    && ema_value.ema1_value
                        > ema_value.ema2_value * ema_touch_trend_signal.ema1_with_ema2_ratio
                {
                    ema_touch_trend_value.is_long_signal = true;
                } else {
                    // 当开盘价格大于ema4的时候， 当价格下跌接近ema4或者ema5位置时候=>价格接近ema4,ema5均线附近 ,且ema4 乘以一定比例依旧<于ema3=> 说明价格下跌幅度较大
                    if ((data_itms.last().unwrap().o() > ema_value.ema4_value)
                        && data_itms.last().unwrap().l()
                            <= ema_value.ema4_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                        || data_itms.last().unwrap().l()
                            <= ema_value.ema5_value * ema_touch_trend_signal.ema4_with_ema5_ratio)
                        && (ema_value.ema4_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                            <= ema_value.ema3_value
                            || ema_value.ema4_value * ema_touch_trend_signal.ema4_with_ema5_ratio
                                <= ema_value.ema3_value)
                    {
                        ema_touch_trend_value.is_in_uptrend_touch_ema4_ema5_nums += 1;
                        if data_itms.last().unwrap().l() <= ema_value.ema4_value {
                            ema_touch_trend_value.is_in_uptrend_touch_ema4 = true;
                        } else {
                            ema_touch_trend_value.is_in_uptrend_touch_ema5 = true;
                        }
                        ema_touch_trend_value.is_long_signal = true;
                    }
                }
            } else if ema_value.ema1_value < ema_value.ema2_value
                && ema_value.ema2_value < ema_value.ema3_value
                && ema_value.ema3_value < ema_value.ema4_value
            {
                ema_touch_trend_value.is_downtrend = true;

                //当前ema_vaue_1 <emalue_2 的时候，价格最高上涨到em2附近的时候且ema1 与 ema2 相差幅度大于0.012
                if data_itms.last().unwrap().h()
                    >= ema_value.ema2_value * ema_touch_trend_signal.price_with_ema_low_ratio
                    && ema_value.ema2_value
                        > ema_value.ema1_value * ema_touch_trend_signal.ema1_with_ema2_ratio
                {
                    ema_touch_trend_value.is_short_signal = true;
                } else {
                    //当价格到达接近ema4或者ema5位置时候,且ema3 与 ema4 或 ema5 相差幅度大于0.09

                    //当价格到达接近ema4或者ema5位置时候,且ema3 与 ema4 或 ema5 相差幅度大于0.09
                    if ((data_itms.last().unwrap().h()
                        * ema_touch_trend_signal.price_with_ema_high_ratio
                        >= ema_value.ema4_value)
                        || (data_itms.last().unwrap().h()
                            * ema_touch_trend_signal.price_with_ema_high_ratio
                            >= ema_value.ema5_value))
                        && ((ema_value.ema3_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                            < ema_value.ema4_value)
                            || (ema_value.ema3_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                                < ema_value.ema5_value))
                    {
                        ema_touch_trend_value.is_in_downtrend_touch_ema4_ema5_nums += 1;
                        if data_itms.last().unwrap().h()
                            * ema_touch_trend_signal.price_with_ema_high_ratio
                            >= ema_value.ema4_value
                        {
                            ema_touch_trend_value.is_in_downtrend_touch_ema4 = true;
                        } else {
                            ema_touch_trend_value.is_in_downtrend_touch_ema5 = true;
                        }
                        ema_touch_trend_value.is_short_signal = true;
                    }
                }
            }
            // println!("ema_touch_trend_value: {:#?}", ema_touch_trend_value);
        }

        ema_touch_trend_value
    }

    // 检查突破信号
    fn check_breakout_signals(
        &self,
        price: f64,
        ema2: f64,
        ema3: f64,
        trend: &EmaTouchTrendSignalValue,
        volume_increase: bool,
    ) -> bool {
        let price_above_ema2 = price > ema2;
        let price_below_ema3 = price < ema3;
        // 简化判断条件
        price_above_ema2 || price_below_ema3
    }

    /// 获取指标组合
    pub fn get_indicator_combine(&self) -> IndicatorCombine {
        let mut indicator_combine = IndicatorCombine::default();
        //添加吞没形态
        if let Some(engulfing_signal) = &self.engulfing_signal {
            indicator_combine.engulfing_indicator = Some(EngulfingIndicator::new());
        }
        //添加ema
        if let Some(ema_signal) = &self.ema_signal {
            indicator_combine.ema_indicator = Some(EmaIndicator::new(
                ema_signal.ema1_length,
                ema_signal.ema2_length,
                ema_signal.ema3_length,
                ema_signal.ema4_length,
                ema_signal.ema5_length,
            ));
        }
        //添加成交量
        if let Some(volume_signal) = &self.volume_signal {
            indicator_combine.volume_indicator =
                Some(VolumeRatioIndicator::new(volume_signal.volume_bar_num));
        }
        //添加rsi
        if let Some(rsi_signal) = &self.rsi_signal {
            indicator_combine.rsi_indicator = Some(RsiIndicator::new(rsi_signal.rsi_length));
        }
        //添加bollinger
        if let Some(bollinger_signal) = &self.bollinger_signal {
            indicator_combine.bollinger_indicator = Some(
                BollingerBands::new(bollinger_signal.period, bollinger_signal.multiplier).unwrap(),
            );
        }
        // println!("indicator_combine: {:#?}", indicator_combine);
        indicator_combine
    }

    /// Runs the backtest asynchronously.
    pub fn run_test(
        &mut self,
        candles: &Vec<CandlesEntity>,
        strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        let min_length = self.get_min_data_length();

        //获取组合配置策略
        let indicator_combine = self.get_indicator_combine();
        strategy_common::run_test(
            {
                let signal_weights = self.signal_weights.as_ref().unwrap().clone();
                move |candles, multi_indicator_values| {
                    self.get_trade_signal(candles, multi_indicator_values, &signal_weights)
                }
            },
            candles,
            strategy_config,
            min_length,
            indicator_combine,
        )
    }

    // 新增：检查突破确认
    fn check_breakthrough_confirmation(&self, data_itms: &[CandleItem], is_upward: bool) -> bool {
        // 实现突破确认逻辑
        // 可以检查:
        // 1. 突破后的持续性
        // 2. 回测支撑/阻力的表现
        // 3. 成交量配合
        true // 临时返回值
    }

    // 新增：计算动态回调幅度
    fn calculate_dynamic_pullback_threshold(&self, _data_itms: &[CandleItem]) -> f64 {
        // 实现动态回调幅度计算逻辑
        // 可以考虑:
        // 1. 价格波动性
        // 2. 均线角度
        // 3. 成交量变化
        // 返回回调幅度
        0.005 // 临时返回值
    }

    // 修改成交量趋势判断
    fn check_volume_trend(&self, volume_trend: &VolumeTrendSignalValue) -> VolumeTrendSignalValue {
        if let Some(volume_signal) = &self.volume_signal {
            VolumeTrendSignalValue {
                is_increasing: volume_trend.volume_ratio > volume_signal.volume_increase_ratio, // 放量
                is_decreasing: volume_trend.volume_ratio < volume_signal.volume_decrease_ratio, // 缩量
                is_stable: volume_trend.volume_ratio >= volume_signal.volume_decrease_ratio
                    && volume_trend.volume_ratio <= volume_signal.volume_increase_ratio, // 稳定
                volume_ratio: volume_trend.volume_ratio,
                volume_value: volume_trend.volume_value,
            }
        } else {
            VolumeTrendSignalValue::default()
        }
    }

    // todo 优化：检查关键价位买入和卖出信号
    fn check_key_price_level_sell(
        &self,
        current_price: f64,
        volume_trend: &VolumeTrendSignalValue,
    ) -> Option<String> {
        // 定义价位级别和对应的提前预警距离
        const PRICE_LEVELS: [(f64, f64, f64, &str); 8] = [
            // (价位区间, 提前预警百分比, 建议回撤百分比, 级别描述)
            (10000.0, 0.02, 0.015, "万元"), // 万元级别
            (1000.0, 0.015, 0.01, "千元"),  // 千元级别
            (100.0, 0.01, 0.008, "百元"),   // 百元级别
            (10.0, 0.008, 0.005, "十元"),   // 十元级别
            (1.0, 0.005, 0.003, "元"),      // 1元级别
            (0.1, 0.003, 0.002, "角"),      // 0.1元级别
            (0.01, 0.002, 0.001, "分"),     // 0.01元级别
            (0.001, 0.001, 0.0005, "厘"),   // 0.001元级别
        ];

        // 修改：从大到小遍历找到第一个小于等于当前价格的级别
        let (interval, alert_percent, pullback_percent, level_name) = PRICE_LEVELS
            .iter()
            .find(|&&(level, _, _, _)| current_price >= level)
            .unwrap_or(&(0.001, 0.001, 0.0005, "微"));

        // 计算下一个关键价位（根据价格级别调整精度）
        let price_unit = if *interval >= 1.0 {
            *interval / 10.0 // 对于大于1元的价格，使用十分之一作为单位
        } else {
            *interval // 对于小于1元的价格，使用当前区间作为单位
        };

        let next_key_level = if *interval >= 1.0 {
            let magnitude = 10f64.powi((*interval as f64).log10().floor() as i32);
            (*interval / magnitude).floor() * magnitude
        } else {
            let magnitude = 10f64.powi((1.0 / *interval as f64).log10().ceil() as i32);
            (*interval * magnitude).floor() / magnitude
        };
        let distance_to_key = next_key_level - current_price;
        let alert_distance = next_key_level * alert_percent;

        println!(
            "价位分析 - 当前价格: {:.4}, 下一关键位: {:.4}, 距离: {:.4}, 预警距离: {:.4} [{}级别]",
            current_price, next_key_level, distance_to_key, alert_distance, level_name
        );

        // 如果接近关键价位且成交量增加，生成卖出信号
        if distance_to_key > 0.0 && distance_to_key < alert_distance && volume_trend.is_increasing {
            // 动态计算建议卖出价格
            let suggested_sell_price = if *interval >= 1.0 {
                // 大额价格使用百分比回撤
                next_key_level * (1.0 - pullback_percent)
            } else {
                // 小额价格使用固定点位回撤
                next_key_level - (price_unit * pullback_percent)
            };

            // 根据价格级别确定信号类型
            let signal_type = if *interval >= 100.0 {
                "重要"
            } else {
                "普通"
            };

            println!("价位分析详情:");
            println!("  价格级别: {} (区间: {:.4})", level_name, interval);
            println!("  预警比例: {:.2}%", alert_percent * 100.0);
            println!("  建议回撤: {:.2}%", pullback_percent * 100.0);
            println!("  建议卖价: {:.4}", suggested_sell_price);

            let format_str = if *interval >= 1.0 {
                format!(
                        "{}价位卖出信号: 当前价格({:.2})接近{}级别关键位({:.2})，建议在{:.2}卖出 [回撤{:.1}%]",
                        signal_type, current_price, level_name, next_key_level, suggested_sell_price,
                        pullback_percent * 100.0
                    )
            } else {
                format!(
                        "{}价位卖出信号: 当前价格({:.4})接近{}级别关键位({:.4})，建议在{:.4}卖出 [回撤{:.2}%]",
                        signal_type, current_price, level_name, next_key_level, suggested_sell_price,
                        pullback_percent * 100.0
                    )
            };

            return Some(format_str);
        }

        None
    }

    // 新增方法：检查突破条件
    fn check_breakthrough_conditions(
        &self,
        data_itms: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> (bool, bool) {
        if data_itms.len() < 2 {
            return (false, false);
        }
        let current_price = data_itms.last().unwrap().c;
        let prev_price = data_itms[data_itms.len() - 2].c;
        if let Some(ema_signal) = &self.ema_signal {
            // 向上突破条件：当前价格突破ema2上轨，且前一根K线价格低于EMA2
            let price_above = current_price
                > ema_value.ema2_value * (1.0 + ema_signal.ema_breakthrough_threshold)
                && prev_price < ema_value.ema2_value;

            // 向下突破条件：当前价格突破ema2下轨，且前一根K线价格高于EMA2
            //todo  优化时间点k线：2025-02-19 22:00:00
            //todo  优化时间点k线路 2025-03-07 08:00:00
            let mut price_below = false;
            if (current_price < ema_value.ema1_value
                && current_price
                    < ema_value.ema2_value * (1.0 - ema_signal.ema_breakthrough_threshold)
                && prev_price > ema_value.ema2_value)
                || (current_price
                    < ema_value.ema5_value * (1.0 - ema_signal.ema_breakthrough_threshold)
                    && prev_price > ema_value.ema5_value)
            {
                price_below = true;
            }
            (price_above, price_below)
        } else {
            (false, false)
        }
    }

    //检查布林带信号
    fn check_bollinger_signal(
        &mut self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: VegasIndicatorSignalValue,
    ) -> BollingerSignalValue {
        //todo 成功示例  2024-11-30 14:00:00, 2024-11-30 14:00:00, 2024-12-07 22:00:00
        //todo 错误示例  2024-12-06 21:00:00
        //todo 考虑在上升时期的时候，因为布林带做空，止盈到价格触碰到布林带中轨平仓
        //todo 考虑在k线收盘之后，价格依然低于布林带下轨位置，如果有多单则平仓

        let mut bolling_bands = vegas_indicator_signal_value.bollinger_value;
        if let Some(bollinger_signal) = &self.bollinger_signal {
            let ema_signal_values = vegas_indicator_signal_value.ema_values;
            //如果ema是多头排列 则当触达ema下轨的时候可以开多，当触达ema上轨的时候可以平仓，但是不能开空单
            // if ema_signal_values.ema1_value > ema_signal_values.ema2_value
            //     && ema_signal_values.ema2_value > ema_signal_values.ema3_value
            //     && ema_signal_values.ema3_value > ema_signal_values.ema4_value
            // {
            if bolling_bands.lower > data_items.last().unwrap().l() {
                bolling_bands.is_long_signal = true;
            }
            if bolling_bands.upper < data_items.last().unwrap().h() {
                bolling_bands.is_short_signal = true;
            }
            // }
            //如果ema是空头排列 则当触达ema上轨的时候可以开空，当触达ema下轨的时候可以平仓，但是不能开多单
            // if ema_signal_values.ema2_value < ema_signal_values.ema3_value
            //     && ema_signal_values.ema3_value < ema_signal_values.ema4_value
            // {
            //示例2025/02/15 01:00:00
            if bolling_bands.lower > data_items.last().unwrap().l() {
                bolling_bands.is_long_signal = true;
            }
            if bolling_bands.upper < data_items.last().unwrap().h() {
                bolling_bands.is_short_signal = true;
            }

            //如果ema多头排列，且收盘价格大于ema 则不能做空，
            //如果ema空头排列，且收盘价格小于ema 则不能做空
            if bolling_bands.is_long_signal
                && ema_signal_values.ema1_value < ema_signal_values.ema2_value
                && ema_signal_values.ema2_value < ema_signal_values.ema3_value
                && data_items.last().unwrap().c < ema_signal_values.ema1_value
            {
                bolling_bands.is_long_signal = false;
                bolling_bands.is_foce_filter_signal = true;
            }

            if bolling_bands.is_short_signal
                && ema_signal_values.ema1_value > ema_signal_values.ema2_value
                && ema_signal_values.ema2_value > ema_signal_values.ema3_value
                && data_items.last().unwrap().c > ema_signal_values.ema1_value
            {
                bolling_bands.is_short_signal = false;
                bolling_bands.is_foce_filter_signal = true;
            }
            bolling_bands
        } else {
            BollingerSignalValue::default()
        }
    }

    //检查吞没形态信号
    fn check_engulfing_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: &mut VegasIndicatorSignalValue,
    ) -> bool {
        //todo 如果是吞没形态，开仓几个大概率是在趋势的末端，所以需要判断当前k线是否是趋势的末端，且开仓挂单价格最好小于当前k线路的最高价70% ，或者小于当前k线路的最低价30%

        if let Some(engulfing_signal) = &self.engulfing_signal {
            if vegas_indicator_signal_value.engulfing_value.is_engulfing
                && vegas_indicator_signal_value.engulfing_value.body_ratio
                    > engulfing_signal.body_ratio
            {
                vegas_indicator_signal_value
                    .engulfing_value
                    .is_valid_engulfing = true;
                return true;
            }
        }
        false
    }
}
