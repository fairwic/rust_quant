use serde::{Deserialize, Serialize};

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

/// 吞没形态指标配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EngulfingSignalConfig {
    /// 是否吞没
    pub is_engulfing: bool,
    /// 实体部分占比
    pub body_ratio: f64,
    /// 是否开仓
    pub is_open: bool,
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

/// 成交量信号配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct VolumeSignalConfig {
    /// 看前N根K线
    pub volume_bar_num: usize,
    /// 放量倍数
    pub volume_increase_ratio: f64,
    /// 缩量倍数
    pub volume_decrease_ratio: f64,
    /// 是否开启
    pub is_open: bool,
}

impl Default for VolumeSignalConfig {
    fn default() -> Self {
        Self {
            volume_bar_num: 4,
            volume_increase_ratio: 2.0,
            volume_decrease_ratio: 2.0,
            is_open: true,
        }
    }
}

/// EMA信号配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmaSignalConfig {
    pub ema1_length: usize,
    pub ema2_length: usize,
    pub ema3_length: usize,
    pub ema4_length: usize,
    pub ema5_length: usize,
    pub ema6_length: usize,
    pub ema7_length: usize,
    /// EMA突破价格的阈值
    pub ema_breakthrough_threshold: f64,
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

/// RSI信号配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct RsiSignalConfig {
    /// RSI周期
    pub rsi_length: usize,
    /// RSI超卖阈值
    pub rsi_oversold: f64,
    /// RSI超买阈值
    pub rsi_overbought: f64,
    /// 是否开启
    pub is_open: bool,
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

/// EMA趋势信号配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct EmaTouchTrendSignalConfig {
    /// EMA1与EMA2的相差幅度
    pub ema1_with_ema2_ratio: f64,
    /// EMA2与EMA3的相差幅度
    pub ema2_with_ema3_ratio: f64,
    /// EMA3与EMA4的相差幅度
    pub ema3_with_ema4_ratio: f64,
    /// EMA4与EMA5的相差幅度
    pub ema4_with_ema5_ratio: f64,
    /// EMA5与EMA7的相差幅度
    pub ema5_with_ema7_ratio: f64,
    /// 价格与EMA4的相差幅度(高位)
    pub price_with_ema_high_ratio: f64,
    /// 价格与EMA4的相差幅度(低位)
    pub price_with_ema_low_ratio: f64,
    /// 是否开启
    pub is_open: bool,
}

impl Default for EmaTouchTrendSignalConfig {
    fn default() -> Self {
        Self {
            ema1_with_ema2_ratio: 1.010,
            ema4_with_ema5_ratio: 1.006,
            ema3_with_ema4_ratio: 1.006,
            ema2_with_ema3_ratio: 1.012,
            ema5_with_ema7_ratio: 1.022,
            price_with_ema_high_ratio: 1.002,
            price_with_ema_low_ratio: 0.995,
            is_open: true,
        }
    }
}

/// 腿部识别系统配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct LegDetectionConfig {
    /// 用于识别腿部的bar数量
    pub size: usize,
    /// 是否启用腿部识别
    pub is_open: bool,
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
    /// 摆动结构长度
    pub swing_length: usize,
    /// 内部结构长度
    pub internal_length: usize,
    /// 是否启用
    pub is_open: bool,
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
    /// 阈值乘数
    pub threshold_multiplier: f64,
    /// 是否使用自动阈值
    pub auto_threshold: bool,
    /// 是否启用
    pub is_open: bool,
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
    /// 回看K线数量
    pub lookback: usize,
    /// 阈值百分比
    pub threshold_pct: f64,
    /// 是否启用
    pub is_open: bool,
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
    /// 溢价阈值
    pub premium_threshold: f64,
    /// 折扣阈值
    pub discount_threshold: f64,
    /// 回看K线数量
    pub lookback: usize,
    /// 是否启用
    pub is_open: bool,
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
