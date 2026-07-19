use serde::{Deserialize, Serialize};
/// 锤子形态配置
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct KlineHammerConfig {
    /// upshadow 比例。
    pub up_shadow_ratio: f64,
    /// downshadow 比例。
    pub down_shadow_ratio: f64,
}
impl Default for KlineHammerConfig {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            volume_bar_num: 4,
            volume_increase_ratio: 2.0,
            volume_decrease_ratio: 2.0,
            is_open: true,
        }
    }
}

/// 中间震荡波动带过滤，只在 ATR 比率落入指定区间时提高成交量确认门槛。
/// 默认关闭，避免现有研究与生产配置因新增字段改变结果。
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct ChoppyVolatilityFilterConfig {
    /// true 只在指定 ATR 区间提高成交量门槛；false 不改变基础分位数。
    pub is_open: bool,
    /// 震荡区间下界，使用 ATR/收盘价的无量纲比率，包含该边界。
    pub min_atr_ratio: f64,
    /// 震荡区间上界，使用 ATR/收盘价的无量纲比率，不包含该边界。
    pub max_atr_ratio: f64,
    /// 震荡区间内要求的最低滚动相对成交量分位数，取值范围为 0 到 1。
    pub min_volume_percentile: f64,
}

impl Default for ChoppyVolatilityFilterConfig {
    fn default() -> Self {
        Self {
            is_open: false,
            min_atr_ratio: 0.018,
            max_atr_ratio: 0.032,
            min_volume_percentile: 119.0 / 120.0,
        }
    }
}

/// 跨币种自适应阈值配置。
///
/// 该配置默认关闭，避免改变现有生产版本；研究版本显式开启后，使用 ATR 倍数和
/// 滚动成交量分位数替代依赖币价、成交量绝对量级或固定涨跌幅的判断。
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct CrossAssetAdaptiveThresholdConfig {
    /// true 使用 ATR 与成交量分位数；false 保留旧版本固定阈值行为。
    pub is_open: bool,
    /// ATR 计算周期。
    pub atr_period: usize,
    /// 成交量分位数使用的前序已确认 K 线数量。
    pub volume_lookback_bars: usize,
    /// 当前相对成交量至少达到的滚动分位数，取值范围为 0 到 1。
    pub min_volume_percentile: f64,
    /// Fib swing 振幅至少达到的 ATR 倍数。
    pub min_swing_atr_multiple: f64,
    /// 中间震荡波动带的额外成交量确认；默认关闭以保持现有策略结果不变。
    pub choppy_volatility_filter: ChoppyVolatilityFilterConfig,
}

impl Default for CrossAssetAdaptiveThresholdConfig {
    fn default() -> Self {
        Self {
            is_open: false,
            atr_period: 14,
            volume_lookback_bars: 120,
            min_volume_percentile: 0.95,
            min_swing_atr_multiple: 4.0,
            choppy_volatility_filter: ChoppyVolatilityFilterConfig::default(),
        }
    }
}

impl CrossAssetAdaptiveThresholdConfig {
    /// 返回当前 ATR 波动状态对应的成交量门槛；区间无效时回退基础值，避免错误配置扩大开仓范围。
    pub fn effective_min_volume_percentile(&self, atr_ratio: f64) -> f64 {
        let base = self.min_volume_percentile.clamp(0.0, 1.0);
        let filter = self.choppy_volatility_filter;
        if !filter.is_open
            || !atr_ratio.is_finite()
            || !filter.min_atr_ratio.is_finite()
            || !filter.max_atr_ratio.is_finite()
            || filter.max_atr_ratio <= filter.min_atr_ratio
            || atr_ratio < filter.min_atr_ratio
            || atr_ratio >= filter.max_atr_ratio
        {
            return base;
        }
        base.max(filter.min_volume_percentile.clamp(0.0, 1.0))
    }
}
/// Vegas 信号与触发动量 K 线的方向关系。
#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandleMomentumDirectionMode {
    /// 不限制 Vegas 信号与触发 K 线的方向关系。
    #[default]
    Any,
    /// Vegas 信号必须与触发 K 线同向。
    Same,
    /// Vegas 信号必须与触发 K 线反向，用于验证放量冲击后的反转假设。
    Opposite,
}

/// 基于已确认 4H K 线生成动量激活窗口的研究配置。
///
/// 该窗口是历史回放时对上游动量事件的因果代理；默认关闭，不改变现有 Vegas 行为。
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct CandleMomentumActivationConfig {
    /// 是否要求近期出现过归一化的量价波动事件。
    pub is_open: bool,
    /// 当前 Fib K 线量能不足时，是否允许近期激活事件替代当根成交量确认。
    ///
    /// 开启后不再把动量窗口作为所有 Vegas 信号的总门禁，只补充 Fib 入场；默认关闭以保持旧语义。
    #[serde(default)]
    pub allow_delayed_fib_volume_confirmation: bool,
    /// 是否只在现有中波动带内允许延迟 Fib 确认，复用 ATR 边界而不新增数值参数。
    #[serde(default)]
    pub restrict_delayed_fib_to_choppy_band: bool,
    /// 中波动带限制开启时，是否额外允许高波动环境中的延迟做空；延迟做多仍受上界约束。
    #[serde(default)]
    pub allow_high_volatility_delayed_short: bool,
    /// 计算成交量和振幅基线使用的已确认 K 线数量。
    pub baseline_bars: usize,
    /// 激活事件产生后允许 Vegas 寻找入场的 4H K 线数量。
    pub valid_for_bars: usize,
    /// 触发后至少等待的完整 4H K 线数量；默认 1 表示不在冲击当根追单。
    #[serde(default = "default_candle_momentum_min_wait_bars")]
    pub min_wait_bars: usize,
    /// 触发 K 线成交量相对前序基线均量的最小倍数。
    pub min_volume_ratio: f64,
    /// 触发 K 线振幅相对前序基线平均振幅的最小倍数。
    pub min_range_ratio: f64,
    /// true 允许在触发 K 线收盘时开仓；false 从下一根已确认 K 线开始等待 Vegas 信号。
    pub allow_trigger_bar_entry: bool,
    /// Vegas 信号与触发动量 K 线之间允许的方向关系。
    #[serde(default)]
    pub direction_mode: CandleMomentumDirectionMode,
    /// 动量激活后允许开仓的 RSI 下界（含边界、无量纲）；None 表示不限制下界。
    #[serde(default)]
    pub min_entry_rsi: Option<f64>,
    /// 动量激活后允许开仓的 RSI 上界（不含边界、无量纲）；None 表示不限制上界。
    #[serde(default)]
    pub max_entry_rsi: Option<f64>,
}
fn default_candle_momentum_min_wait_bars() -> usize {
    1
}
impl Default for CandleMomentumActivationConfig {
    fn default() -> Self {
        Self {
            is_open: false,
            allow_delayed_fib_volume_confirmation: false,
            restrict_delayed_fib_to_choppy_band: false,
            allow_high_volatility_delayed_short: false,
            baseline_bars: 20,
            valid_for_bars: 6,
            min_wait_bars: default_candle_momentum_min_wait_bars(),
            min_volume_ratio: 2.0,
            min_range_ratio: 1.5,
            allow_trigger_bar_entry: false,
            direction_mode: CandleMomentumDirectionMode::Any,
            min_entry_rsi: None,
            max_entry_rsi: None,
        }
    }
}
/// EMA信号配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmaSignalConfig {
    /// 第 1 条 EMA 的计算周期。
    pub ema1_length: usize,
    /// 第 2 条 EMA 的计算周期。
    pub ema2_length: usize,
    /// 第 3 条 EMA 的计算周期。
    pub ema3_length: usize,
    /// 第 4 条 EMA 的计算周期。
    pub ema4_length: usize,
    /// 第 5 条 EMA 的计算周期。
    pub ema5_length: usize,
    /// 第 6 条 EMA 的计算周期。
    pub ema6_length: usize,
    /// 第 7 条 EMA 的计算周期。
    pub ema7_length: usize,
    /// EMA突破价格的阈值
    pub ema_breakthrough_threshold: f64,
    /// 是否处于打开状态。
    pub is_open: bool,
}
impl Default for EmaSignalConfig {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// 是否启用做多接飞刀保护
    pub filter_falling_knife_long: bool,
    /// 是否启用做空接飞刀保护
    pub filter_falling_knife_short: bool,
}
impl Default for MacdSignalConfig {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            is_open: true,    // 默认开启，使用新的智能过滤逻辑
            fast_period: 12,  // 标准 12
            slow_period: 26,  // 标准 26
            signal_period: 9, // 标准 9
            filter_falling_knife: true,
            filter_falling_knife_long: true,
            filter_falling_knife_short: true,
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
/// 入场硬拦截配置
///
/// 默认保持既有基线；实验性拦截必须显式开启后再做回测验证。
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct EntryBlockConfig {
    /// TooFar 空头且不在 Fib 区间时拦截追空
    pub block_too_far_outside_fib_short: bool,
    /// TooFar 空头趋势下反趋势锤子线做多拦截
    pub block_counter_trend_hammer_long: bool,
    /// 弱 EMA 趋势且缺少形态确认时拦截入场
    pub block_weak_ema_trend_entry: bool,
    /// TooFar + Fib 区间 + 低量冲突新空腿时拦截做空
    pub block_conflicting_too_far_new_bear_leg_short: bool,
    /// 缩量 + RSI 中性 + MACD 零轴上方转弱时拦截做多
    pub block_low_volume_neutral_rsi_macd_weakening_long: bool,
    /// 实验性：空头浅反弹里的缩量震荡多单拦截，默认关闭
    pub block_low_volume_inside_range_entry: bool,
    /// 实验性：做多低于 VAL、做空高于 VAH 时拦截入场，默认关闭
    pub block_opposite_value_area_entry: bool,
    /// 实验性：价格在 VAH 上方但处于低成交量节点时拦截入场，默认关闭
    pub block_low_volume_above_value_area_entry: bool,
    /// 实验性：价格在价值区内且处于低成交量节点时拦截做空，默认关闭
    pub block_short_inside_low_volume_node_entry: bool,
    /// EMA 距离过滤的空头分支
    pub block_ema_distance_short: bool,
    /// 做空入场时允许的最大 EMA2/EMA4 距离；None 表示不启用
    pub max_short_ema_distance_ratio: Option<f64>,
    /// ETH 4H id102：做多打入上方 bearish FVG 压力区但未收复时拦截
    pub block_bearish_fvg_pressure_long: bool,
    /// 布林方向缺少入场支持时拦截多空追单
    pub block_weak_bollinger_context_entry: bool,
    /// 仅拦截布林方向缺少支持的做多信号
    pub block_weak_bollinger_context_long: bool,
    /// 仅拦截布林方向缺少支持的做空信号
    pub block_weak_bollinger_context_short: bool,
    /// 启用弱布林过滤所需的最小 ATR/收盘价；0 表示不限制波动
    pub weak_bollinger_min_atr_ratio: f64,
    /// 普通距离、牛腿但缺少布林和成交量确认时拦截做多
    pub block_normal_bull_leg_no_confirm_long: bool,
    /// 深负 MACD 环境下的弱锤子线做多拦截
    pub block_deep_negative_hammer_long: bool,
    /// 零轴上方、无趋势且缩量的上吊线做空拦截
    pub block_above_zero_low_volume_no_trend_hanging_short: bool,
    /// 多头趋势深回调但缺少做空结构确认时拦截做空
    pub block_long_trend_pullback_short: bool,
}
impl Default for EntryBlockConfig {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            block_too_far_outside_fib_short: true,
            block_counter_trend_hammer_long: true,
            block_weak_ema_trend_entry: true,
            block_conflicting_too_far_new_bear_leg_short: true,
            block_low_volume_neutral_rsi_macd_weakening_long: true,
            block_low_volume_inside_range_entry: false,
            block_opposite_value_area_entry: false,
            block_low_volume_above_value_area_entry: false,
            block_short_inside_low_volume_node_entry: false,
            block_ema_distance_short: true,
            max_short_ema_distance_ratio: None,
            block_bearish_fvg_pressure_long: false,
            block_weak_bollinger_context_entry: false,
            block_weak_bollinger_context_long: false,
            block_weak_bollinger_context_short: false,
            weak_bollinger_min_atr_ratio: 0.0,
            block_normal_bull_leg_no_confirm_long: false,
            block_deep_negative_hammer_long: false,
            block_above_zero_low_volume_no_trend_hanging_short: false,
            block_long_trend_pullback_short: false,
        }
    }
}
pub fn default_entry_block_config() -> EntryBlockConfig {
    EntryBlockConfig::default()
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
#[cfg(test)]
mod tests {
    use super::{
        ChoppyVolatilityFilterConfig, CrossAssetAdaptiveThresholdConfig, EntryBlockConfig,
        MacdSignalConfig,
    };

    #[test]
    fn choppy_volatility_filter_only_raises_threshold_inside_configured_band() {
        let config = CrossAssetAdaptiveThresholdConfig {
            min_volume_percentile: 0.95,
            choppy_volatility_filter: ChoppyVolatilityFilterConfig {
                is_open: true,
                min_atr_ratio: 0.018,
                max_atr_ratio: 0.032,
                min_volume_percentile: 119.0 / 120.0,
            },
            ..Default::default()
        };

        assert_eq!(config.effective_min_volume_percentile(0.0179), 0.95);
        assert_eq!(config.effective_min_volume_percentile(0.018), 119.0 / 120.0);
        assert_eq!(
            config.effective_min_volume_percentile(0.0319),
            119.0 / 120.0
        );
        assert_eq!(config.effective_min_volume_percentile(0.032), 0.95);
    }
    #[test]
    fn entry_block_config_defaults_keep_existing_baseline_stable() {
        let config = EntryBlockConfig::default();
        assert!(config.block_ema_distance_short);
        assert!(config.block_too_far_outside_fib_short);
        assert!(config.block_counter_trend_hammer_long);
        assert!(config.block_conflicting_too_far_new_bear_leg_short);
        assert!(config.block_low_volume_neutral_rsi_macd_weakening_long);
        assert!(!config.block_low_volume_inside_range_entry);
        assert!(!config.block_opposite_value_area_entry);
        assert!(!config.block_low_volume_above_value_area_entry);
        assert!(!config.block_short_inside_low_volume_node_entry);
        assert!(!config.block_bearish_fvg_pressure_long);
        assert!(!config.block_weak_bollinger_context_entry);
        assert!(!config.block_weak_bollinger_context_long);
        assert!(!config.block_weak_bollinger_context_short);
        assert_eq!(config.weak_bollinger_min_atr_ratio, 0.0);
        assert!(!config.block_normal_bull_leg_no_confirm_long);
        assert!(!config.block_deep_negative_hammer_long);
        assert!(!config.block_above_zero_low_volume_no_trend_hanging_short);
        assert!(!config.block_long_trend_pullback_short);
        assert!(config.block_weak_ema_trend_entry);
    }
    #[test]
    fn entry_block_config_can_override_specific_filter_from_json() {
        let config: EntryBlockConfig = serde_json::from_value(serde_json::json!({
            "block_ema_distance_short": false,
            "block_counter_trend_hammer_long": false,
            "block_low_volume_neutral_rsi_macd_weakening_long": false,
            "block_low_volume_inside_range_entry": true,
            "block_opposite_value_area_entry": true,
            "block_low_volume_above_value_area_entry": true,
            "block_short_inside_low_volume_node_entry": true,
            "block_bearish_fvg_pressure_long": true,
            "block_weak_bollinger_context_entry": true,
            "block_weak_bollinger_context_long": true,
            "block_weak_bollinger_context_short": true,
            "weak_bollinger_min_atr_ratio": 0.02,
            "block_normal_bull_leg_no_confirm_long": true,
            "block_deep_negative_hammer_long": true,
            "block_above_zero_low_volume_no_trend_hanging_short": true,
            "block_long_trend_pullback_short": true
        }))
        .expect("entry block config should deserialize");
        assert!(!config.block_ema_distance_short);
        assert!(!config.block_counter_trend_hammer_long);
        assert!(!config.block_low_volume_neutral_rsi_macd_weakening_long);
        assert!(config.block_low_volume_inside_range_entry);
        assert!(config.block_opposite_value_area_entry);
        assert!(config.block_low_volume_above_value_area_entry);
        assert!(config.block_short_inside_low_volume_node_entry);
        assert!(config.block_bearish_fvg_pressure_long);
        assert!(config.block_weak_bollinger_context_entry);
        assert!(config.block_weak_bollinger_context_long);
        assert!(config.block_weak_bollinger_context_short);
        assert_eq!(config.weak_bollinger_min_atr_ratio, 0.02);
        assert!(config.block_normal_bull_leg_no_confirm_long);
        assert!(config.block_deep_negative_hammer_long);
        assert!(config.block_above_zero_low_volume_no_trend_hanging_short);
        assert!(config.block_long_trend_pullback_short);
        assert!(config.block_too_far_outside_fib_short);
        assert!(config.block_conflicting_too_far_new_bear_leg_short);
        assert!(config.block_weak_ema_trend_entry);
    }
    #[test]
    fn macd_signal_config_defaults_keep_directional_falling_knife_filters_enabled() {
        let config = MacdSignalConfig::default();
        assert!(config.filter_falling_knife);
        assert!(config.filter_falling_knife_long);
        assert!(config.filter_falling_knife_short);
    }
    #[test]
    fn macd_signal_config_can_disable_only_long_falling_knife_filter_from_json() {
        let config: MacdSignalConfig = serde_json::from_value(serde_json::json!({
            "filter_falling_knife": true,
            "filter_falling_knife_long": false
        }))
        .expect("macd signal config should deserialize");
        assert!(config.filter_falling_knife);
        assert!(!config.filter_falling_knife_long);
        assert!(config.filter_falling_knife_short);
    }
}
