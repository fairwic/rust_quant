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

/// 市场结构配置（SMC）
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct MarketStructureConfig {
    /// 摆动结构长度
    pub swing_length: usize,
    /// 内部结构长度
    pub internal_length: usize,
    /// 摆动突破阈值（相对 pivot 的百分比）
    pub swing_threshold: f64,
    /// 内部突破阈值（相对 pivot 的百分比）
    pub internal_threshold: f64,
    /// 是否启用摆动结构信号
    pub enable_swing_signal: bool,
    /// 是否启用内部结构信号
    pub enable_internal_signal: bool,
    /// 是否启用
    pub is_open: bool,
}

impl Default for MarketStructureConfig {
    fn default() -> Self {
        Self {
            swing_length: 12,
            internal_length: 2,
            swing_threshold: 0.015,
            internal_threshold: 0.015,
            enable_swing_signal: false,
            enable_internal_signal: true,
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
            bb_width_threshold: 0.03,
            tp_kline_ratio: 0.6,
            is_open: true,
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
    /// 是否启用"接飞刀"保护 (默认 true)
    /// 当 MACD 与交易方向相反时，如果动量还在恶化则过滤；如果动量改善则放行（允许抄底）
    pub filter_falling_knife: bool,
}

impl Default for MacdSignalConfig {
    fn default() -> Self {
        Self {
            is_open: true,                   // 默认开启，使用新的智能过滤逻辑
            fast_period: 12,                 // 标准 12
            slow_period: 26,                 // 标准 26
            signal_period: 9,                // 标准 9
            filter_falling_knife: true,
        }
    }
}

pub fn default_macd_signal_config() -> Option<MacdSignalConfig> {
    Some(MacdSignalConfig::default())
}

/// Fib 回撤入场配置（趋势回调/反弹入场）
///
/// 目标：只在“大小趋势一致 + 发生回撤/反弹 + 触达 Fib 区间 + 放量”时入场
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct FibRetracementSignalConfig {
    /// 是否启用
    pub is_open: bool,
    /// 是否只在 Fib 条件触发时才允许开仓
    /// - true: 只做 Fib 回撤入场（推荐用于严格顺势）
    /// - false: 作为过滤/辅助信号（保留原有入场）
    pub only_on_fib: bool,
    /// Swing 回看窗口（根数）
    pub swing_lookback: usize,
    /// 触发区间下边界（例如 0.328/0.382）
    pub fib_trigger_low: f64,
    /// 触发区间上边界（例如 0.618）
    pub fib_trigger_high: f64,
    /// 放量阈值（volume_ratio >= min_volume_ratio）
    pub min_volume_ratio: f64,
    /// 是否要求腿部方向确认（LegDetection）
    pub require_leg_confirmation: bool,
    /// 是否严格按大趋势方向过滤反向信号
    pub strict_major_trend: bool,
    /// Swing 止损缓冲（例如 0.01=1%）
    pub stop_loss_buffer_ratio: f64,
    /// 是否使用 Swing 结构止损
    pub use_swing_stop_loss: bool,
    /// 最小趋势波动幅度阈值（只有当 swing 范围 (high - low) / low 超过该阈值时，才应用 strict_major_trend 过滤）
    #[serde(default = "default_min_trend_move_pct")]
    pub min_trend_move_pct: f64,
}

impl Default for FibRetracementSignalConfig {
    fn default() -> Self {
        Self {
            is_open: false,
            only_on_fib: true,
            swing_lookback: 96,      // 4H≈16天
            fib_trigger_low: 0.328,  // 兼容用户口径（常见也可改为0.382）
            fib_trigger_high: 0.618, // 黄金分割区
            min_volume_ratio: 1.5,   // 放量确认
            require_leg_confirmation: true,
            strict_major_trend: true,
            stop_loss_buffer_ratio: 0.01,
            use_swing_stop_loss: true,
            min_trend_move_pct: 0.08,
        }
    }
}

pub fn default_fib_retracement_signal_config() -> Option<FibRetracementSignalConfig> {
    Some(FibRetracementSignalConfig::default())
}

fn default_min_trend_move_pct() -> f64 {
    0.08
}

/// 大实体止损配置
/// 当K线为大实体（强趋势）时，使用更紧的止损（假设回调不深）
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct LargeEntityStopLossConfig {
    /// 是否启用
    pub is_open: bool,
    /// 最小实体占比（实体/整根振幅），例如 0.6 表示实体占60%
    pub min_body_ratio: f64,
    /// 最小实体涨跌幅（|收-开|/开），例如 0.005 表示0.5%
    pub min_move_pct: f64,
    /// 回调比例阈值（Fibonacci），例如 0.382
    /// 做多止损 = High - (High - Low) * ratio
    /// 做空止损 = Low + (High - Low) * ratio
    pub retracement_ratio: f64,
}

impl Default for LargeEntityStopLossConfig {
    fn default() -> Self {
        Self {
            is_open: true,
            min_body_ratio: 0.6,
            min_move_pct: 0.005,    // 0.5%
            retracement_ratio: 0.5, // 允许回撤 50%
        }
    }
}

pub fn default_large_entity_stop_loss_config() -> Option<LargeEntityStopLossConfig> {
    Some(LargeEntityStopLossConfig::default())
}
