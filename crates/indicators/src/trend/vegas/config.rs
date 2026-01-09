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
#[serde(default)]
pub struct MarketStructureConfig {
    /// 摆动结构长度
    pub swing_length: usize,
    /// 内部结构长度
    pub internal_length: usize,
    /// 触发摆动突破所需的相对幅度
    pub swing_threshold: f64,
    /// 触发内部突破所需的相对幅度
    pub internal_threshold: f64,
    /// 是否启用摆动结构信号
    pub enable_swing_signal: bool,
    /// 是否启用内部结构信号
    pub enable_internal_signal: bool,
    /// 是否开启整个市场结构信号
    pub is_open: bool,
}

impl Default for MarketStructureConfig {
    fn default() -> Self {
        Self {
            swing_length: 20,
            internal_length: 5,
            swing_threshold: 0.0,
            internal_threshold: 0.0,
            enable_swing_signal: true,
            enable_internal_signal: true,
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

/// 震荡/区间判断配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct RangeFilterConfig {
    /// 布林带宽度阈值 (upper-lower)/middle
    pub bb_width_threshold: f64,
    /// 震荡时使用的止盈倍数 (相对开仓K线振幅)
    pub tp_kline_ratio: f64,
    /// 是否启用
    pub is_open: bool,
}

impl Default for RangeFilterConfig {
    fn default() -> Self {
        Self {
            bb_width_threshold: 0.02,
            tp_kline_ratio: 0.6,
            is_open: false,
        }
    }
}

/// 极端K线过滤/放行配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ExtremeKFilterConfig {
    /// 是否启用
    pub is_open: bool,
    /// 极端K线最小实体占比（实体/整根振幅）
    pub min_body_ratio: f64,
    /// 极端K线最小实体涨跌幅（|收-开|/开）
    pub min_move_pct: f64,
    /// 至少跨越的EMA条数（例如同时穿过ema2/ema3/ema4）
    pub min_cross_ema_count: usize,
}

impl Default for ExtremeKFilterConfig {
    fn default() -> Self {
        Self {
            is_open: true,
            // 默认采用“宽松档”（5593方案）
            min_body_ratio: 0.65,
            min_move_pct: 0.010,
            min_cross_ema_count: 2,
        }
    }
}

pub fn default_extreme_k_filter() -> Option<ExtremeKFilterConfig> {
    Some(ExtremeKFilterConfig::default())
}

/// 追涨追跌确认配置
/// 当价格远离EMA144时，要求额外的确认条件才能开仓
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct ChaseConfirmConfig {
    /// 是否启用追涨追跌确认
    pub enabled: bool,
    /// 追涨阈值（价格高于EMA144的百分比，如0.18表示18%）
    pub long_threshold: f64,
    /// 追跌阈值（价格低于EMA144的百分比，如0.10表示10%）
    pub short_threshold: f64,
    /// 回调/反弹触碰阈值（K线high/low距离EMA144的百分比）
    pub pullback_touch_threshold: f64,
    /// 确认K线最小实体比
    pub min_body_ratio: f64,
    /// 贴线距离阈值（价格距离EMA4的百分比，如0.0025表示0.25%）
    pub close_to_ema_threshold: f64,
    /// 贴线止损系数（如0.998表示EMA4 * 0.998）
    pub tight_stop_loss_ratio: f64,
}

impl Default for ChaseConfirmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            long_threshold: 0.18,
            short_threshold: 0.10,
            pullback_touch_threshold: 0.05,
            min_body_ratio: 0.5,
            close_to_ema_threshold: 0.0025,
            tight_stop_loss_ratio: 0.998,
        }
    }
}

pub fn default_chase_confirm_config() -> Option<ChaseConfirmConfig> {
    Some(ChaseConfirmConfig::default())
}

/// MACD 信号配置
/// 用于过滤逆势交易，减少动量冲突的亏损
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct MacdSignalConfig {
    /// 是否启用 MACD 过滤
    pub is_open: bool,
    /// 快线周期（默认12）
    pub fast_period: usize,
    /// 慢线周期（默认26）
    pub slow_period: usize,
    /// 信号线周期（默认9）
    pub signal_period: usize,
    /// 是否仅作为过滤器（true: 仅过滤信号, false: 可作为独立信号）
    pub as_filter_only: bool,
    /// 是否要求动量确认（柱状图连续递增/递减）
    pub require_momentum_confirm: bool,
    /// 动量确认周期数（连续N根柱状图同向）
    pub momentum_confirm_bars: usize,
    /// 是否启用"接飞刀"保护 (默认 true)
    /// 当 MACD 与交易方向相反时，如果动量还在恶化则过滤；如果动量改善则放行（允许抄底）
    pub filter_falling_knife: bool,
}

impl Default for MacdSignalConfig {
    fn default() -> Self {
        Self {
            is_open: true,  // 默认开启，使用新的智能过滤逻辑
            fast_period: 6,   // 加速：12 -> 6
            slow_period: 13,  // 加速：26 -> 13
            signal_period: 4, // 加速：9 -> 4
            as_filter_only: true,
            require_momentum_confirm: false,  // 默认关闭，由 filter_falling_knife 接管主要的动量判断
            momentum_confirm_bars: 2,
            filter_falling_knife: true,
        }
    }
}

pub fn default_macd_signal_config() -> Option<MacdSignalConfig> {
    Some(MacdSignalConfig::default())
}
