use crate::trading::indicator::equal_high_low_indicator::EqualHighLowValue;
use crate::trading::indicator::fair_value_gap_indicator::FairValueGapValue;
use crate::trading::indicator::leg_detection_indicator::LegDetectionValue;
use crate::trading::indicator::market_structure_indicator::MarketStructureValue;
use crate::trading::indicator::premium_discount_indicator::PremiumDiscountValue;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use serde::{Deserialize, Serialize};

/// 锤子形态信号值
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct KlineHammerSignalValue {
    /// 上影线比例
    pub up_shadow_ratio: f64,
    /// 下影线比例
    pub down_shadow_ratio: f64,
    /// 实体比例
    pub body_ratio: f64,
    /// 是否开多信号
    pub is_long_signal: bool,
    /// 是否开空信号
    pub is_short_signal: bool,
    /// 是否是锤子形态
    pub is_hammer: bool,
    /// 是否是上吊线形态
    pub is_hanging_man: bool,
}

/// 吞没形态指标值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct EngulfingSignalValue {
    /// 是否吞没形态
    pub is_engulfing: bool,
    /// 是否有效吞没形态
    pub is_valid_engulfing: bool,
    /// 实体比例
    pub body_ratio: f64,
}

/// 成交量趋势信号值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct VolumeTrendSignalValue {
    /// 是否增长,对比上一跟k线路
    pub is_increasing_than_pre: bool,
    /// 是否下降,对比上一跟k线路
    pub is_decreasing_than_pre: bool,
    /// 是否大于指标设置的成交量放大的比例
    pub is_increase_than_ratio: bool,
    /// 成交量比例(当前成交量/前N根K线成交量平均值)
    pub volume_ratio: f64,
    /// 成交量值
    pub volume_value: f64,
}


/// EMA信号值
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EmaSignalValue {
    pub ema1_value: f64,
    pub ema2_value: f64,
    pub ema3_value: f64,
    pub ema4_value: f64,
    pub ema5_value: f64,
    pub ema6_value: f64,
    pub ema7_value: f64,
    /// 是否多头排列
    pub is_long_trend: bool,
    /// 是否空头排列
    pub is_short_trend: bool,
}

/// 布林带信号值
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BollingerSignalValue {
    pub lower: f64,
    pub upper: f64,
    pub middle: f64,
    /// 连续触达上轨/下轨次数
    pub consecutive_touch_times: usize,
    pub is_long_signal: bool,
    pub is_short_signal: bool,
    pub is_close_signal: bool,
    /// 虽然触发了布林带开多，或者开空，但是被过滤了
    pub is_force_filter_signal: bool,
}

/// RSI信号值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct RsiSignalValue {
    /// RSI值
    pub rsi_value: f64,
    /// 是否超卖
    pub is_oversold: bool,
    /// 是否超买
    pub is_overbought: bool,
}

/// EMA趋势信号值
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EmaTouchTrendSignalValue {
    /// 是否多头趋势
    pub is_uptrend: bool,
    /// 是否空头趋势
    pub is_downtrend: bool,
    /// 是否在多头趋势触碰ema2
    pub is_in_uptrend_touch_ema2: bool,
    /// 是否在多头趋势触碰ema3
    pub is_in_uptrend_touch_ema3: bool,
    /// 当前多头趋势中触碰ema2和ema3的次数
    pub is_in_uptrend_touch_ema2_ema3_nums: usize,
    /// 是否在多头趋势触碰ema4
    pub is_in_uptrend_touch_ema4: bool,
    /// 是否在多头趋势触碰ema5
    pub is_in_uptrend_touch_ema5: bool,
    /// 当前多头趋势中触碰ema4和ema5的次数
    pub is_in_uptrend_touch_ema4_ema5_nums: usize,
    /// 是否在空头趋势触碰ema2
    pub is_touch_ema2: bool,
    /// 是否在空头趋势触碰ema3
    pub is_touch_ema3: bool,
    /// 当前空头趋势触碰ema2和ema3的次数
    pub is_ema2_ema3_nums: usize,
    /// 是否在空头趋势触碰ema4
    pub is_touch_ema4: bool,
    /// 是否在空头趋势触碰ema5
    pub is_touch_ema5: bool,
    /// 当前空头趋势中触碰ema4和ema5的次数
    pub is_touch_ema4_ema5_nums: usize,
    /// 是否在空头趋势触碰ema7
    pub is_touch_ema7: bool,
    /// 当前空头趋势中触碰ema7的次数
    pub is_touch_ema7_nums: usize,
    /// 是否多头开仓
    pub is_long_signal: bool,
    /// 是否空头开仓
    pub is_short_signal: bool,
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

/// Vegas指标综合信号值
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct VegasIndicatorSignalValue {
    /// EMA信号配置
    pub ema_values: EmaSignalValue,
    /// 成交量信号配置
    pub volume_value: VolumeTrendSignalValue,
    /// EMA趋势
    pub ema_touch_value: EmaTouchTrendSignalValue,
    /// RSI信号配置
    pub rsi_value: RsiSignalValue,
    /// 布林带信号配置
    pub bollinger_value: BollingerSignalValue,
    /// 权重配置
    pub signal_weights_value: SignalWeightsConfig,
    /// 吞没形态指标
    pub engulfing_value: EngulfingSignalValue,
    /// 锤子形态指标
    pub kline_hammer_value: KlineHammerSignalValue,
    /// Smart Money Concepts相关字段
    /// 腿部识别
    pub leg_detection_value: LegDetectionValue,
    /// 市场结构
    pub market_structure_value: MarketStructureValue,
    /// 公平价值缺口
    pub fair_value_gap_value: FairValueGapValue,
    /// 等高/等低点
    pub equal_high_low_value: EqualHighLowValue,
    /// 溢价/折扣区域
    pub premium_discount_value: PremiumDiscountValue,
}

/// 检查均线交叉
pub struct EmaCross {
    pub is_golden_cross: bool,
    pub is_death_cross: bool,
}
