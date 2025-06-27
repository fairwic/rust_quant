use crate::trading::strategy::strategy_common::{
    self, BackTestResult, BasicRiskStrategyConfig, SignalResult,
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
use tracing::{debug, error};

use super::bollings::{BollingBandsPlusIndicator, BollingBandsSignalConfig};
use super::candle;
use super::ema_indicator::EmaIndicator;
use super::is_big_kline::IsBigKLineIndicator;
use super::k_line_engulfing_indicator::KlineEngulfingIndicator;
use super::k_line_hammer_indicator::KlineHammerIndicator;
use super::rsi_rma_indicator::RsiIndicator;
use super::signal_weight::{SignalCondition, SignalDirect, SignalType, SignalWeightsConfig};
use super::volume_indicator::VolumeRatioIndicator;
use crate::trading::indicator::equal_high_low_indicator::{
    EqualHighLowIndicator, EqualHighLowValue,
};
use crate::trading::indicator::fair_value_gap_indicator::{
    FairValueGapIndicator, FairValueGapValue,
};
use crate::trading::indicator::leg_detection_indicator::{
    LegDetectionIndicator, LegDetectionValue,
};
use crate::trading::indicator::market_structure_indicator::{
    MarketStructureIndicator, MarketStructureValue,
};
use crate::trading::indicator::premium_discount_indicator::{
    PremiumDiscountIndicator, PremiumDiscountValue,
};
use crate::trading::strategy::arc::indicator_values::ema_indicator_values;
use crate::trading::utils;
use crate::trading::utils::fibonacci::{
    FIBONACCI_ZERO_POINT_FIVE, FIBONACCI_ZERO_POINT_THREE_EIGHT_TWO,
    FIBONACCI_ZERO_POINT_TWO_THREE_SIX,
};
use uuid::fmt::Braced;

/// 锤子形态配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct KlineHammerConfig {
    pub up_shadow_ratio: f64,
    pub down_shadow_ratio: f64,
}
impl Default for KlineHammerConfig {
    fn default() -> Self {
        Self {
            up_shadow_ratio: 0.6,
            down_shadow_ratio: 0.6,
        }
    }
}
/// 锤子形态信号值
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct KlineHammerSignalValue {
    //上影线比例
    pub up_shadow_ratio: f64,
    //下影线比例
    pub down_shadow_ratio: f64,
    //实体比例
    pub body_ratio: f64,
    //是否开多信号
    pub is_long_signal: bool,
    //是否开空信号
    pub is_short_signal: bool,
    //是否是锤子形态
    pub is_hammer: bool,
    //是否是上吊线形态
    pub is_hanging_man: bool,
}

//吞没形态指标
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EngulfingSignalConfig {
    //吞没形态指标
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
    pub is_valid_engulfing: bool,
    pub body_ratio: f64,
}

impl Default for EngulfingSignalConfig {
    fn default() -> Self {
        Self {
            is_engulfing: true,
            body_ratio: 0.4,
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
    pub volume_ratio: f64, // 成交量比例
    pub volume_value: f64, // 成交量值
}

// 成交量信号配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct VolumeSignalConfig {
    pub volume_bar_num: usize,      // 看前10根K线
    pub volume_increase_ratio: f64, // 放量倍数
    pub volume_decrease_ratio: f64, // 缩量倍数
    pub is_open: bool,              // 是否开启
    pub is_force_dependent: bool,   // 是否是必要的指标
}
impl Default for VolumeSignalConfig {
    fn default() -> Self {
        Self {
            volume_bar_num: 6,
            volume_increase_ratio: 2.0,
            volume_decrease_ratio: 2.4,
            is_open: true,
            is_force_dependent: false,
        }
    }
}

// ema信号配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmaSignalConfig {
    pub ema1_length: usize,
    pub ema2_length: usize,
    pub ema3_length: usize,
    pub ema4_length: usize,
    pub ema5_length: usize,
    pub ema6_length: usize,
    pub ema7_length: usize,
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
            ema6_length: 2304,
            ema7_length: 2704,
            ema_breakthrough_threshold: 0.003,
            is_open: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndicatorCombine {
    pub ema_indicator: Option<EmaIndicator>,
    pub rsi_indicator: Option<RsiIndicator>,
    pub volume_indicator: Option<VolumeRatioIndicator>,
    pub bollinger_indicator: Option<BollingBandsPlusIndicator>,
    pub engulfing_indicator: Option<KlineEngulfingIndicator>,
    pub kline_hammer_indicator: Option<KlineHammerIndicator>,
    // 新增Smart Money Concepts相关指标
    pub leg_detection_indicator: Option<LegDetectionIndicator>,
    pub market_structure_indicator: Option<MarketStructureIndicator>,
    pub fair_value_gap_indicator: Option<FairValueGapIndicator>,
    pub equal_high_low_indicator: Option<EqualHighLowIndicator>,
    pub premium_discount_indicator: Option<PremiumDiscountIndicator>,
}

impl Default for IndicatorCombine {
    fn default() -> Self {
        Self {
            ema_indicator: None,
            rsi_indicator: None,
            volume_indicator: None,
            bollinger_indicator: None,
            engulfing_indicator: None,
            kline_hammer_indicator: None,
            // 新增Smart Money Concepts相关指标
            leg_detection_indicator: None,
            market_structure_indicator: None,
            fair_value_gap_indicator: None,
            equal_high_low_indicator: None,
            premium_discount_indicator: None,
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
    pub ema6_value: f64,
    pub ema7_value: f64,

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
    //连续触达上轨/下轨次数
    pub consecutive_touch_times: usize,
    pub is_long_signal: bool,
    pub is_short_signal: bool,
    pub is_close_signal: bool,
    //虽然触发了布林带开多，或者开空，但是被过滤了
    pub is_force_filter_signal: bool,
}

// rsi信号配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct RsiSignalConfig {
    pub rsi_length: usize,   // rsi周期
    pub rsi_oversold: f64,   // rsi超卖阈值
    pub rsi_overbought: f64, // rsi超买阈值
    pub is_open: bool,       // 是否开启
}
impl Default for RsiSignalConfig {
    fn default() -> Self {
        Self {
            rsi_length: 9,
            rsi_oversold: 15.0,
            rsi_overbought: 85.0,
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
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EmaTouchTrendSignalConfig {
    pub ema1_with_ema2_ratio: f64,      //eam2与eam3的相差幅度
    pub ema2_with_ema3_ratio: f64,      //eam2与eam3的相差幅度
    pub ema3_with_ema4_ratio: f64,      //eam2与eam3的相差幅度
    pub ema4_with_ema5_ratio: f64,      //eam2与eam3的相差幅度
    pub ema5_with_ema7_ratio: f64,      //eam2与eam3的相差幅度
    pub price_with_ema_high_ratio: f64, //价格与ema4的相差幅度
    pub price_with_ema_low_ratio: f64,  //价格与ema4的相差幅度
    pub is_open: bool,                  //是否开启
}
impl Default for EmaTouchTrendSignalConfig {
    fn default() -> Self {
        Self {
            ema1_with_ema2_ratio: 1.010,      //ema1与ema2的相差幅度
            ema4_with_ema5_ratio: 1.006,      //ema4与ema5的相差幅度
            ema3_with_ema4_ratio: 1.006,      //ema3与ema4的相差幅度
            ema2_with_ema3_ratio: 1.012,      //ema2与ema3的相差幅度
            ema5_with_ema7_ratio: 1.022,      //ema5与ema7的相差幅度
            price_with_ema_high_ratio: 1.002, //价格与ema4的相差幅度
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

    pub is_touch_ema2: bool,      //是否在空头趋势触碰ema2
    pub is_touch_ema3: bool,      //是否在空头趋势触碰ema3
    pub is_ema2_ema3_nums: usize, //当前空头趋势触碰ema2和ema3的次数

    pub is_touch_ema4: bool,            //是否在空头趋势触碰ema4
    pub is_touch_ema5: bool,            //是否在空头趋势触碰ema5
    pub is_touch_ema4_ema5_nums: usize, //当前空头趋势中触碰ema4和ema5的次数

    pub is_touch_ema7: bool,       //是否在空头趋势触碰ema7
    pub is_touch_ema7_nums: usize, //当前空头趋势中触碰ema7的次数

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
            is_touch_ema2: false,
            is_touch_ema3: false,
            is_ema2_ema3_nums: 0,
            is_touch_ema4: false,
            is_touch_ema5: false,
            is_touch_ema4_ema5_nums: 0,
            is_touch_ema7: false,
            is_touch_ema7_nums: 0,
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
    pub ema_values: EmaSignalValue,                 // ema信号配置
    pub volume_value: VolumeTrendSignalValue,       // 新增：成交量信号配置
    pub ema_touch_value: EmaTouchTrendSignalValue,  // ema趋势
    pub rsi_value: RsiSignalValue,                  //rsi信号配置
    pub bollinger_value: BollingerSignalValue,      //bollinger信号配置
    pub signal_weights_value: SignalWeightsConfig,  // 新增权重配置
    pub engulfing_value: EngulfingSignalValue,      //吞没形态指标
    pub kline_hammer_value: KlineHammerSignalValue, //锤子形态指标
    // 新增Smart Money Concepts相关字段
    pub leg_detection_value: LegDetectionValue, // 腿部识别
    pub market_structure_value: MarketStructureValue, // 市场结构
    pub fair_value_gap_value: FairValueGapValue, // 公平价值缺口
    pub equal_high_low_value: EqualHighLowValue, // 等高/等低点
    pub premium_discount_value: PremiumDiscountValue, // 溢价/折扣区域
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
    pub min_k_line_num: usize,                     //最小需要的k线数量
    pub ema_signal: Option<EmaSignalConfig>,       // ema信号配置
    pub volume_signal: Option<VolumeSignalConfig>, // 新增：成交量信号配置
    pub ema_touch_trend_signal: Option<EmaTouchTrendSignalConfig>, // ema趋势
    pub rsi_signal: Option<RsiSignalConfig>,       //rsi信号配置
    pub bolling_signal: Option<BollingBandsSignalConfig>, //bollinger信号配置
    pub signal_weights: Option<SignalWeightsConfig>, // 新增权重配置
    //新增吞没形态指标
    pub engulfing_signal: Option<EngulfingSignalConfig>, // 新增吞没形态指标
    //新增锤子形态指标
    pub kline_hammer_signal: Option<KlineHammerConfig>, // 新增锤子形态指标
                                                        // 新增Smart Money Concepts相关配置
                                                        // pub leg_detection_signal: Option<LegDetectionConfig>,           // 腿部识别系统
                                                        // pub market_structure_signal: Option<MarketStructureConfig>,     // 市场结构识别
                                                        // pub fair_value_gap_signal: Option<FairValueGapConfig>,          // 公平价值缺口
                                                        // pub equal_high_low_signal: Option<EqualHighLowConfig>,          // 等高/等低点识别
                                                        // pub premium_discount_signal: Option<PremiumDiscountConfig>,     // 溢价/折扣区域
}

impl Default for VegasStrategy {
    fn default() -> Self {
        Self {
            min_k_line_num: 3600,
            ema_touch_trend_signal: Some(EmaTouchTrendSignalConfig::default()),
            bolling_signal: Some(BollingBandsSignalConfig::default()),
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
            rsi_signal: Some(RsiSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            kline_hammer_signal: Some(KlineHammerConfig::default()),
            // // 新增Smart Money Concepts相关配置默认值
            // leg_detection_signal: Some(LegDetectionConfig::default()),
            // market_structure_signal: Some(MarketStructureConfig::default()),
            // fair_value_gap_signal: Some(FairValueGapConfig::default()),
            // equal_high_low_signal: Some(EqualHighLowConfig::default()),
            // premium_discount_signal: Some(PremiumDiscountConfig::default()),
        }
    }
}

impl VegasStrategy {
    pub fn get_min_data_length(&mut self) -> usize {
        self.min_k_line_num
    }

    /// 获取交易信号
    ///  data_items 数据列表,在突破策略中要考虑到前一根k线
    pub fn get_trade_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &mut VegasIndicatorSignalValue,
        weights: &SignalWeightsConfig,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        // 初始化交易信号
        let last_data_item = data_items.last().unwrap();
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
        //优先判断成交量
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
            self.check_ema_touch_trend(data_items, vegas_indicator_signal_values.ema_values);
        vegas_indicator_signal_values.ema_touch_value = ema_trend.clone();
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

        //判断布林带
        if let Some(bollinger_signal) = &self.bolling_signal {
            let bollinger_value =
                self.check_bollinger_signal(data_items, vegas_indicator_signal_values.clone());
            vegas_indicator_signal_values.bollinger_value = bollinger_value.clone();
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
        let breakthrough_confirmed = self.check_breakthrough_confirmation(data_items, price_above);

        //计算振幅
        let k_line_amplitude = self.calculate_k_line_amplitude(data_items);

        //计算吞没形态
        self.check_engulfing_signal(
            data_items,
            vegas_indicator_signal_values,
            &mut conditions,
            vegas_indicator_signal_values.ema_values,
        );

        //添加锤子形态
        self.check_kline_hammer_signal(
            data_items,
            vegas_indicator_signal_values,
            &mut conditions,
            vegas_indicator_signal_values.ema_values,
        );

        // 新增Smart Money Concepts相关信号检查
        // // 检查腿部识别信号
        // if let Some(leg_config) = &self.leg_detection_signal {
        //     if leg_config.is_open {
        //         conditions.push((
        //             SignalType::LegDetection,
        //             SignalCondition::LegDetection {
        //                 is_bullish_leg: vegas_indicator_signal_values.leg_detection_value.is_bullish_leg,
        //                 is_bearish_leg: vegas_indicator_signal_values.leg_detection_value.is_bearish_leg,
        //                 is_new_leg: vegas_indicator_signal_values.leg_detection_value.is_new_leg,
        //             },
        //         ));
        //     }
        // }
        // // 检查市场结构信号
        // if let Some(structure_config) = &self.market_structure_signal {
        //     if structure_config.is_open {
        //         // 检查摆动结构信号
        //         let ms_value = &vegas_indicator_signal_values.market_structure_value;

        //         if ms_value.swing_bullish_bos || ms_value.swing_bearish_bos ||
        //            ms_value.swing_bullish_choch || ms_value.swing_bearish_choch {
        //             conditions.push((
        //                 SignalType::MarketStructure,
        //                 SignalCondition::MarketStructure {
        //                     is_bullish_bos: ms_value.swing_bullish_bos,
        //                     is_bearish_bos: ms_value.swing_bearish_bos,
        //                     is_bullish_choch: ms_value.swing_bullish_choch,
        //                     is_bearish_choch: ms_value.swing_bearish_choch,
        //                     is_internal: false,
        //                 },
        //             ));
        //         }

        //         // 检查内部结构信号
        //         if ms_value.internal_bullish_bos || ms_value.internal_bearish_bos ||
        //            ms_value.internal_bullish_choch || ms_value.internal_bearish_choch {
        //             conditions.push((
        //                 SignalType::MarketStructure,
        //                 SignalCondition::MarketStructure {
        //                     is_bullish_bos: ms_value.internal_bullish_bos,
        //                     is_bearish_bos: ms_value.internal_bearish_bos,
        //                     is_bullish_choch: ms_value.internal_bullish_choch,
        //                     is_bearish_choch: ms_value.internal_bearish_choch,
        //                     is_internal: true,
        //                 },
        //             ));
        //         }
        //     }
        // }

        // // 检查公平价值缺口信号
        // if let Some(fvg_config) = &self.fair_value_gap_signal {
        //     if fvg_config.is_open {
        //         let fvg_value = &vegas_indicator_signal_values.fair_value_gap_value;

        //         if fvg_value.current_bullish_fvg || fvg_value.current_bearish_fvg {
        //             conditions.push((
        //                 SignalType::FairValueGap,
        //                 SignalCondition::FairValueGap {
        //                     is_bullish_fvg: fvg_value.current_bullish_fvg,
        //                     is_bearish_fvg: fvg_value.current_bearish_fvg,
        //                 },
        //             ));
        //         }
        //     }
        // }

        // // 检查等高/等低点信号
        // if let Some(ehl_config) = &self.equal_high_low_signal {
        //     if ehl_config.is_open {
        //         let ehl_value = &vegas_indicator_signal_values.equal_high_low_value;

        //         if ehl_value.current_equal_high || ehl_value.current_equal_low {
        //             conditions.push((
        //                 SignalType::EqualHighLow,
        //                 SignalCondition::EqualHighLow {
        //                     is_equal_high: ehl_value.current_equal_high,
        //                     is_equal_low: ehl_value.current_equal_low,
        //                 },
        //             ));
        //         }
        //     }
        // }

        // // 检查溢价/折扣区域信号
        // if let Some(pd_config) = &self.premium_discount_signal {
        //     if pd_config.is_open {
        //         let pd_value = &vegas_indicator_signal_values.premium_discount_value;

        //         if pd_value.in_premium_zone || pd_value.in_discount_zone {
        //             conditions.push((
        //                 SignalType::PremiumDiscount,
        //                 SignalCondition::PremiumDiscount {
        //                     in_premium_zone: pd_value.in_premium_zone,
        //                     in_discount_zone: pd_value.in_discount_zone,
        //                 },
        //             ));
        //         }
        //     }
        // }
        // println!("conditions: {:#?}", conditions);
        // 计算得分
        let score = weights.calculate_score(conditions.clone());
        //计算分数到达指定值
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
        if false {
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }

        // self.calculate_best_open_price(data_items, &mut signal_result);
        //设置止盈比例为1:2
        // self.calculate_best_take_profit_price(last_data_item, &mut signal_result);
        signal_result
    }

    //计算最佳止损价格
    fn calculate_best_stop_loss_price(
        &self,
        last_data_item: &CandleItem,
        signal_result: &mut SignalResult,
        conditions: &Vec<(SignalType, SignalCondition)>,
    ) {
        //todo 可以做调整，如果出现吞没形态，则止损价格为前一个k线的最高价的0.382位置,
        //todo 可以做调整，如果出现锤子形态，则止损价格为最高价和开盘价的0.382位置,
        for (signal_type, signal_condition) in conditions {
            let mut signal_kline_stop_loss_price: Option<f64> = None;
            // match signal_type {
            //     //吞没形态
            //     SignalType::Engulfing => {
            //         signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
            //     }
            //     SignalType::Bolling => {
            //         if signal_result.should_buy {
            //             signal_kline_stop_loss_price = Some(last_data_item.l());
            //         } else if signal_result.should_sell {
            //             //如果止损价格为空，或者止损价格>当前最新止损价格，则更新止损价格
            //             signal_kline_stop_loss_price = Some(last_data_item.h());
            //         }
            //     }
            //     _ => {}
            // }
            if signal_result.should_buy && signal_kline_stop_loss_price.is_some() {
                //如果止损价格为空，或者止损价格<当前最新止损价格，则更新止损价格
                if signal_result.signal_kline_stop_loss_price.is_none()
                    || signal_result.signal_kline_stop_loss_price.unwrap()
                        < signal_kline_stop_loss_price.unwrap()
                {
                    signal_result.signal_kline_stop_loss_price = signal_kline_stop_loss_price;
                }
            } else if signal_result.should_sell {
                //如果止损价格为空，或者止损价格>当前最新止损价格，则更新止损价格
                let signal_kline_stop_loss_price = last_data_item.h();
                if signal_result.signal_kline_stop_loss_price.is_none()
                    || signal_result.signal_kline_stop_loss_price.unwrap()
                        > signal_kline_stop_loss_price
                {
                    signal_result.signal_kline_stop_loss_price = Some(signal_kline_stop_loss_price);
                }
            }
        }

        //如果条件中没有出现吞没形态和锤子形态，则使用默认止损价格
        if signal_result.signal_kline_stop_loss_price.is_none() {
            if signal_result.should_buy {
                let amplitude = last_data_item.h() - last_data_item.l();
                let best_stop_loss_price =
                    last_data_item.l() + (amplitude * FIBONACCI_ZERO_POINT_TWO_THREE_SIX);
                signal_result.signal_kline_stop_loss_price = Some(best_stop_loss_price);
            } else if signal_result.should_sell {
                let amplitude = last_data_item.h() - last_data_item.l();
                let best_stop_loss_price =
                    last_data_item.h() - (amplitude * FIBONACCI_ZERO_POINT_TWO_THREE_SIX);
                signal_result.signal_kline_stop_loss_price = Some(best_stop_loss_price);
            }
        }
    }

    //计算最优止盈价格
    fn calculate_best_take_profit_price(
        &self,
        last_data_item: &CandleItem,
        signal_result: &mut SignalResult,
    ) {
        if signal_result.should_buy {
            let amplitude = last_data_item.c() - last_data_item.l();
            let best_take_profit_price = last_data_item.c() + (amplitude * 4.0);
            signal_result.best_take_profit_price = Some(best_take_profit_price);
        } else if signal_result.should_sell {
            let amplitude = last_data_item.c() - last_data_item.l();
            let best_take_profit_price = last_data_item.c() - (amplitude * 4.0);
            signal_result.best_take_profit_price = Some(best_take_profit_price);
        }
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

    //计算最优开仓价格
    fn calculate_best_open_price(
        &self,
        data_items: &[CandleItem],
        signal_result: &mut SignalResult,
    ) {
        //判断最新的k线是否跌了超过1.5个点
        let last_data_item = data_items.last().unwrap();
        let amplitude = self.calculate_k_line_amplitude(data_items);
        if amplitude <= 1.2 {
            debug!("k线振幅小于1.5个点，不计算最优开仓价格");
            return;
        }
        //没有出现特别的利空消息，因为出现大的利空消息会一直下跌不会触发反弹
        if true {
            if signal_result.should_sell {
                //判断是否使用最优开仓价格,如果k线是下跌，且跌幅较大，且没有利空消息，则使用最优开仓价格(当前k线最高价格-当前k线最低价格)的38.2%作为最优开仓价格
                let high_price = last_data_item.h();
                let low_price = last_data_item.l();
                let diff = high_price - low_price;
                let best_open_price =
                    low_price + diff * utils::fibonacci::FIBONACCI_ZERO_POINT_THREE_EIGHT_TWO;
                signal_result.best_open_price = Some(best_open_price);
                signal_result.signal_kline_stop_loss_price = Some(high_price);

                // //找到之前的一根不连续上涨或下跌的k线的，最低价格或者最高价格作为信号止损线路
                // for i in (0..data_items.len()).rev() {
                //     if data_items[i].c() > data_items[i].o() {
                //         let high_price = data_items[i].h();
                //         signal_result.tp_price = Some(high_price);
                //         break;
                //     }
                // }
            } else if signal_result.should_buy {
                //判断是否使用最优开仓价格,如果k线是上涨，且涨幅较大，且没有利好消息，则使用最优开仓价格,(当前k线最高价格-当前k线最低价格)的23.6%作为最优开仓价格
                let high_price = last_data_item.h();
                let low_price = last_data_item.l();
                let diff = high_price - low_price;
                let best_open_price =
                    high_price - (diff * utils::fibonacci::FIBONACCI_ZERO_POINT_THREE_EIGHT_TWO);
                signal_result.best_open_price = Some(best_open_price);
                signal_result.signal_kline_stop_loss_price = Some(low_price);
                // //找到之前的一根不连续上涨或下跌的k线的，最低价格或者最高价格作为信号止损线路
                // for i in (0..data_items.len()).rev() {
                //     if data_items[i].c() < data_items[i].o() {
                //         let low_price = data_items[i].l();
                //         signal_result.tp_price = Some(low_price);
                //         break;
                //     }
                // }
            }
        }
    }

    //获取有效的rsi
    fn get_valid_rsi(
        &self,
        data_items: &[CandleItem],
        rsi_value: &RsiSignalValue,
        ema_value: EmaSignalValue,
    ) -> f64 {
        // 如果当前k线价格波动比较大，且k线路的实体部分占比大于80%,表明当前k线为大阳线或者大阴线，则不使用rsi指标,因为大概率趋势还会继续  2025-03-03 00:00:00,2025-03-09 18:00:00
        if true {
            let is_big_k_line =
                IsBigKLineIndicator::new(70.0).is_big_k_line(data_items.last().unwrap());
            if is_big_k_line {
                return 50.0;
            }
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
    fn check_ema_touch_trend(
        &self,
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> EmaTouchTrendSignalValue {
        //判断ema 是否空头排列，或者多头排列或者多头排列
        let mut ema_touch_trend_value = EmaTouchTrendSignalValue::default();
        let last_data_item = data_items.last().unwrap();

        if let Some(ema_touch_trend_signal) = &self.ema_touch_trend_signal {
            //todo  优化时间点 2024-12-09 08:00:00
            // println!("111111111 ema_value: {:#?}", ema_value);
            if ema_value.ema2_value > ema_value.ema3_value
                && ema_value.ema3_value > ema_value.ema4_value
            {
                ema_touch_trend_value.is_uptrend = true;
                //当前ema_value_1 >ema_value_2 的时候， 且要求开盘价>ema2,价格最低下跌到em2附近的时候，且ema1 与 ema2 相差幅度大于0.012
                if ema_value.ema1_value > ema_value.ema2_value
                    && data_items.last().unwrap().l()
                        <= ema_value.ema2_value * ema_touch_trend_signal.price_with_ema_high_ratio
                    && ema_value.ema1_value
                        > ema_value.ema2_value * ema_touch_trend_signal.ema1_with_ema2_ratio
                    && data_items.last().unwrap().o() > ema_value.ema2_value
                    && data_items.last().unwrap().c() > ema_value.ema2_value
                {
                    ema_touch_trend_value.is_long_signal = true;
                } else {
                    // 当开盘价格大于ema4的时候， 当价格下跌接近ema4或者ema5位置时候=>价格接近ema4,ema5均线附近 ,且ema4 乘以一定比例依旧<于ema3=> 说明价格下跌幅度较大
                    let condition_1 = data_items.last().unwrap().o() > ema_value.ema4_value;
                    let condition_2 = data_items.last().unwrap().l()
                        <= ema_value.ema4_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                        || data_items.last().unwrap().l()
                            <= ema_value.ema5_value * ema_touch_trend_signal.ema4_with_ema5_ratio;
                    // println!(
                    //     "ema_value.ema3_value: {}, ema_value.ema4_value: {}, ema_value.ema5_value: {}",
                    //     ema_value.ema3_value, ema_value.ema4_value, ema_value.ema5_value
                    // );
                    // println!(
                    //     "ema_value.ema4_value * ema_touch_trend_signal.ema3_with_ema4_ratio: {}, ema_value.ema4_value * ema_touch_trend_signal.ema4_with_ema5_ratio: {}",
                    //     ema_value.ema4_value * ema_touch_trend_signal.ema3_with_ema4_ratio,
                    //     ema_value.ema5_value * ema_touch_trend_signal.ema4_with_ema5_ratio
                    // );

                    let condition_3 = ema_value.ema4_value
                        * ema_touch_trend_signal.ema3_with_ema4_ratio
                        <= ema_value.ema3_value
                        || ema_value.ema4_value * ema_touch_trend_signal.ema4_with_ema5_ratio
                            <= ema_value.ema3_value;
                    // println!(
                    //     "condition_1: {}, condition_2: {}, condition_3: {}",
                    //     condition_1, condition_2, condition_3
                    // );
                    if condition_1 && condition_2 && condition_3 {
                        ema_touch_trend_value.is_in_uptrend_touch_ema4_ema5_nums += 1;
                        if data_items.last().unwrap().l() <= ema_value.ema4_value {
                            ema_touch_trend_value.is_in_uptrend_touch_ema4 = true;
                        } else {
                            ema_touch_trend_value.is_in_uptrend_touch_ema5 = true;
                        }
                        ema_touch_trend_value.is_long_signal = true;
                    }
                }

                //短期多头趋势
                if ema_value.ema1_value > ema_value.ema2_value
                    && ema_value.ema2_value > ema_value.ema3_value
                    && ema_value.ema3_value > ema_value.ema4_value
                    //长期空头趋势
                    && ema_value.ema4_value < ema_value.ema5_value
                    && ema_value.ema5_value < ema_value.ema6_value
                    && ema_value.ema6_value < ema_value.ema7_value
                {
                    //case 3当价格到达接近ema7位置时候,且ema5 与 ema7相差幅度大于0.09,则开始短多
                    if last_data_item.h() >= ema_value.ema7_value
                        && ema_value.ema5_value * ema_touch_trend_signal.ema5_with_ema7_ratio
                            > ema_value.ema7_value
                    {
                        ema_touch_trend_value.is_touch_ema7_nums += 1;
                        ema_touch_trend_value.is_touch_ema7 = true;
                        ema_touch_trend_value.is_short_signal = true;
                        ema_touch_trend_value.is_long_signal = false;
                    }
                }
            } else if ema_value.ema1_value < ema_value.ema2_value
                && ema_value.ema2_value < ema_value.ema3_value
                && ema_value.ema3_value < ema_value.ema4_value
            {
                ema_touch_trend_value.is_downtrend = true;

                //case 1当前ema_vaue_1 <emalue_2 的时候， 且要求开盘价<ema2,价格最高上涨到em2附近的时候且ema1 与 ema2 相差幅度大于0.012
                if data_items.last().unwrap().h()
                    >= ema_value.ema2_value * ema_touch_trend_signal.price_with_ema_low_ratio
                    && ema_value.ema2_value
                        > ema_value.ema1_value * ema_touch_trend_signal.ema1_with_ema2_ratio
                    && data_items.last().unwrap().o() < ema_value.ema2_value
                    && data_items.last().unwrap().c() < ema_value.ema2_value
                {
                    // println!("data.last().unwrap(): {:#?}", data_itms.last().unwrap());
                    // println!("ema2_value: {:#?}", ema_value.ema2_value);
                    ema_touch_trend_value.is_short_signal = true;
                    ema_touch_trend_value.is_touch_ema2 = true;
                }
                //case 2当价格到达接近ema4或者ema5位置时候,且ema3 与 ema4 或 ema5 相差幅度大于0.09
                if (data_items.last().unwrap().o() < ema_value.ema4_value
                    && ((data_items.last().unwrap().h()
                        * ema_touch_trend_signal.price_with_ema_high_ratio
                        >= ema_value.ema4_value)
                        || (data_items.last().unwrap().h()
                            * ema_touch_trend_signal.price_with_ema_high_ratio
                            >= ema_value.ema5_value)))
                    && ((ema_value.ema3_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                        < ema_value.ema4_value)
                        || (ema_value.ema3_value * ema_touch_trend_signal.ema3_with_ema4_ratio
                            < ema_value.ema5_value))
                {
                    ema_touch_trend_value.is_touch_ema4_ema5_nums += 1;
                    if data_items.last().unwrap().h()
                        * ema_touch_trend_signal.price_with_ema_high_ratio
                        >= ema_value.ema4_value
                    {
                        ema_touch_trend_value.is_touch_ema4 = true;
                    } else {
                        ema_touch_trend_value.is_touch_ema5 = true;
                    }
                    ema_touch_trend_value.is_short_signal = true;
                }

                //短期空头趋势
                if ema_value.ema1_value < ema_value.ema2_value
                    && ema_value.ema2_value < ema_value.ema3_value
                    && ema_value.ema3_value < ema_value.ema4_value
                    //长期多头趋势
                    && ema_value.ema4_value > ema_value.ema5_value
                    && ema_value.ema5_value > ema_value.ema6_value
                    && ema_value.ema6_value > ema_value.ema7_value
                {
                    //case 3当价格到达接近ema7位置时候,且ema5 与 ema7相差幅度大于0.09,则开始短多
                    if last_data_item.l() <= ema_value.ema7_value
                        && ema_value.ema7_value * ema_touch_trend_signal.ema5_with_ema7_ratio
                            < ema_value.ema5_value
                    {
                        ema_touch_trend_value.is_touch_ema7_nums += 1;
                        ema_touch_trend_value.is_touch_ema7 = true;
                        ema_touch_trend_value.is_long_signal = true;
                        ema_touch_trend_value.is_short_signal = false;
                    }
                }
            }
            // println!(
            //     "2222222 ema_touch_trend_value: {:#?}",
            //     ema_touch_trend_value
            // );
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
            indicator_combine.engulfing_indicator = Some(KlineEngulfingIndicator::new());
        }
        //添加ema
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
        //添加成交量
        if let Some(volume_signal) = &self.volume_signal {
            indicator_combine.volume_indicator = Some(VolumeRatioIndicator::new(
                volume_signal.volume_bar_num,
                true,
            ));
        }
        //添加rsi
        if let Some(rsi_signal) = &self.rsi_signal {
            indicator_combine.rsi_indicator = Some(RsiIndicator::new(rsi_signal.rsi_length));
        }
        //添加bolling
        if let Some(bolling_signal) = &self.bolling_signal {
            indicator_combine.bollinger_indicator = Some(BollingBandsPlusIndicator::new(
                bolling_signal.period,
                bolling_signal.multiplier,
                bolling_signal.consecutive_touch_times,
            ));
        }
        //添加锤子形态
        if let Some(kline_hammer_signal) = &self.kline_hammer_signal {
            indicator_combine.kline_hammer_indicator = Some(KlineHammerIndicator::new(
                kline_hammer_signal.up_shadow_ratio,
                kline_hammer_signal.down_shadow_ratio,
            ));
        }
        // 新增Smart Money Concepts相关指标

        // // 添加腿部识别指标
        // if let Some(leg_config) = &self.leg_detection_signal {
        //     if leg_config.is_open {
        //         indicator_combine.leg_detection_indicator = Some(
        //             LegDetectionIndicator::new(leg_config.size)
        //         );
        //     }
        // }

        // // 添加市场结构指标
        // if let Some(structure_config) = &self.market_structure_signal {
        //     if structure_config.is_open {
        //         indicator_combine.market_structure_indicator = Some(
        //             MarketStructureIndicator::new(
        //                 structure_config.swing_length,
        //                 structure_config.internal_length
        //             )
        //         );
        //     }
        // }
        // // 添加公平价值缺口指标
        // if let Some(fvg_config) = &self.fair_value_gap_signal {
        //     if fvg_config.is_open {
        //         indicator_combine.fair_value_gap_indicator = Some(
        //             FairValueGapIndicator::new(
        //                 fvg_config.threshold_multiplier,
        //                 fvg_config.auto_threshold
        //             )
        //         );
        //     }
        // }
        // // 添加等高/等低点指标
        // if let Some(ehl_config) = &self.equal_high_low_signal {
        //     if ehl_config.is_open {
        //         indicator_combine.equal_high_low_indicator = Some(
        //             EqualHighLowIndicator::new(
        //                 ehl_config.lookback,
        //                 ehl_config.threshold_pct
        //             )
        //         );
        //     }
        // }
        // // 添加溢价/折扣区域指标
        // if let Some(pd_config) = &self.premium_discount_signal {
        //     if pd_config.is_open {
        //         indicator_combine.premium_discount_indicator = Some(
        //             PremiumDiscountIndicator::new(
        //                 pd_config.premium_threshold,
        //                 pd_config.discount_threshold,
        //                 pd_config.lookback
        //             )
        //         );
        //     }
        // }
        // println!("indicator_combine: {:#?}", indicator_combine);
        indicator_combine
    }

    /// Runs the backtest asynchronously.
    pub fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        let min_length = self.get_min_data_length();

        //获取组合配置策略
        let mut indicator_combine = self.get_indicator_combine();
        strategy_common::run_back_test(
            {
                let signal_weights = self.signal_weights.as_ref().unwrap().clone();
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

    // 新增：检查突破确认
    fn check_breakthrough_confirmation(&self, data_items: &[CandleItem], is_upward: bool) -> bool {
        // 实现突破确认逻辑
        // 可以检查:
        // 1. 突破后的持续性
        // 2. 回测支撑/阻力的表现
        // 3. 成交量配合
        true // 临时返回值
    }

    // 新增：计算动态回调幅度
    fn calculate_dynamic_pullback_threshold(&self, _data_items: &[CandleItem]) -> f64 {
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
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> (bool, bool) {
        if data_items.len() < 2 {
            return (false, false);
        }
        let current_price = data_items.last().unwrap().c;
        let prev_price = data_items[data_items.len() - 2].c;
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
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: VegasIndicatorSignalValue,
    ) -> BollingerSignalValue {
        //todo 成功示例  2024-11-30 14:00:00, 2024-11-30 14:00:00, 2024-12-07 22:00:00
        //todo 错误示例  2024-12-06 21:00:00
        //todo 考虑在上升时期的时候，因为布林带做空，止盈到价格触碰到布林带中轨平仓
        //todo 考虑在k线收盘之后，价格依然低于布林带下轨位置，如果有多单则平仓

        let mut bolling_bands = vegas_indicator_signal_value.bollinger_value.clone();
        if let Some(bollinger_signal) = &self.bolling_signal {
            let ema_signal_values = vegas_indicator_signal_value.ema_values;
            //如果ema是多头排列 则当触达ema下轨的时候可以开多，当触达ema上轨的时候可以平仓，但是不能开空单
            // if ema_signal_values.ema1_value > ema_signal_values.ema2_value
            //     && ema_signal_values.ema2_value > ema_signal_values.ema3_value
            //     && ema_signal_values.ema3_value > ema_signal_values.ema4_value
            // {

            let data_item = data_items.last().unwrap();

            if bolling_bands.lower > data_item.l() {
                bolling_bands.is_long_signal = true;
            }
            if bolling_bands.upper < data_item.h() {
                bolling_bands.is_short_signal = true;
            }

            //如果连续触达上轨/下轨次数小于4次,则需要进行额外过滤
            // if bolling_bands.consecutive_touch_times < bollinger_signal.consecutive_touch_times
            // && (bolling_bands.is_long_signal || bolling_bands.is_short_signal)
            if (bolling_bands.is_long_signal || bolling_bands.is_short_signal) {
                //如果ema多头排列，且收盘价格大于ema 则不能做空，
                //如果ema空头排列，且收盘价格小于ema 则不能做空
                if bolling_bands.is_long_signal
                // && ema_signal_values.ema1_value < ema_signal_values.ema2_value
                // && ema_signal_values.ema2_value < ema_signal_values.ema3_value
                && data_items.last().unwrap().c < ema_signal_values.ema1_value
                {
                    bolling_bands.is_long_signal = false;
                    bolling_bands.is_force_filter_signal = true;
                }

                if bolling_bands.is_short_signal
                // && ema_signal_values.ema1_value > ema_signal_values.ema2_value
                // && ema_signal_values.ema2_value > ema_signal_values.ema3_value
                && data_items.last().unwrap().c > ema_signal_values.ema1_value
                {
                    bolling_bands.is_short_signal = false;
                    bolling_bands.is_force_filter_signal = true;
                }
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
        conditions: &mut Vec<(SignalType, SignalCondition)>,
        ema_value: EmaSignalValue,
    ) {
        // todo 如果是吞没形态，开仓几个大概率是在趋势的末端，所以需要判断当前k线是否是趋势的末端，且开仓挂单价格最好小于当前k线路的最高价70% ，或者小于当前k线路的最低价30%
        let mut is_engulfing = false;
        let last_data_item = data_items.last().unwrap();
        //判断如果是吞没形态，且要求实体部分要大于指定的配置值
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
            let mut is_long_signal = false;
            let mut is_short_signal = false;
            //但出现吞没形态，且如果当前k线收盘价大于开盘价，则认为是多头吞没形态，否则为空头吞没形态
            if last_data_item.c() > last_data_item.o() {
                is_long_signal = true;
                //且当ema均线是空头排列，且收盘小于于ema1均线，则多头形态失效
                // if ema_value.is_short_trend == true
                //     && data_items.last().unwrap().c < ema_value.ema1_value
                // {
                //     is_long_signal = false;
                // }
            } else {
                is_short_signal = true;
                //且当ema均线是多头排列，且收盘大于ema1均线，则空头形态失效
                // if ema_value.is_long_trend == true
                //     && data_items.last().unwrap().c > ema_value.ema1_value
                // {
                //     is_short_signal = false;
                // }
            }
            //如果吞没形态跌幅
            conditions.push((
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: is_long_signal,
                    is_short_signal: is_short_signal,
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
        if let Some(kline_hammer_signal) = &self.kline_hammer_signal {
            let is_hammer = vegas_indicator_signal_values.kline_hammer_value.is_hammer;
            let is_hanging_man = vegas_indicator_signal_values
                .kline_hammer_value
                .is_hanging_man;

            //如果上有长上影线，且振幅>0.5,则才能判断是有效的
            if is_hammer && self.calculate_k_line_amplitude(data_items) >= 0.6 {
                vegas_indicator_signal_values
                    .kline_hammer_value
                    .is_long_signal = true;
                //如果ema均线是空头排列，且收盘价<于ema1均线，则不能开多单,且成交量<5000
                if ema_value.is_short_trend == true
                    && data_items.last().unwrap().c < ema_value.ema1_value
                    && data_items.last().unwrap().v < 5000.0
                {
                    vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_long_signal = false;
                }
            }

            //如果ema均线是空头排列，且收盘价小于ema1均线，则才能是下跌信号,且成交量<5000
            if is_hanging_man && self.calculate_k_line_amplitude(data_items) >= 0.6 {
                vegas_indicator_signal_values
                    .kline_hammer_value
                    .is_short_signal = true;
                //case 1如果ema均线是空头排列，且收盘价小于ema1均线，则不能开空单,且成交量<5000
                if ema_value.is_long_trend == true
                    && data_items.last().unwrap().c > ema_value.ema1_value
                    && data_items.last().unwrap().v < 5000.0
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
}

/// 腿部识别系统配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct LegDetectionConfig {
    pub size: usize,   // 用于识别腿部的bar数量
    pub is_open: bool, // 是否启用腿部识别
}

impl Default for LegDetectionConfig {
    fn default() -> Self {
        Self {
            size: 5,
            is_open: true,
        }
    }
}

/// 市场结构识别配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct MarketStructureConfig {
    pub swing_length: usize,    // 摆动结构长度
    pub internal_length: usize, // 内部结构长度
    pub is_open: bool,          // 是否启用
}

impl Default for MarketStructureConfig {
    fn default() -> Self {
        Self {
            swing_length: 20,
            internal_length: 5,
            is_open: true,
        }
    }
}

/// 公平价值缺口配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct FairValueGapConfig {
    pub threshold_multiplier: f64, // 阈值乘数
    pub auto_threshold: bool,      // 是否使用自动阈值
    pub is_open: bool,             // 是否启用
}

impl Default for FairValueGapConfig {
    fn default() -> Self {
        Self {
            threshold_multiplier: 1.0,
            auto_threshold: true,
            is_open: true,
        }
    }
}

/// 等高/等低点识别配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EqualHighLowConfig {
    pub lookback: usize,    // 回看K线数量
    pub threshold_pct: f64, // 阈值百分比
    pub is_open: bool,      // 是否启用
}

impl Default for EqualHighLowConfig {
    fn default() -> Self {
        Self {
            lookback: 10,
            threshold_pct: 0.1,
            is_open: true,
        }
    }
}

/// 溢价/折扣区域配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct PremiumDiscountConfig {
    pub premium_threshold: f64,  // 溢价阈值
    pub discount_threshold: f64, // 折扣阈值
    pub lookback: usize,         // 回看K线数量
    pub is_open: bool,           // 是否启用
}

impl Default for PremiumDiscountConfig {
    fn default() -> Self {
        Self {
            premium_threshold: 0.05,
            discount_threshold: 0.05,
            lookback: 20,
            is_open: true,
        }
    }
}
