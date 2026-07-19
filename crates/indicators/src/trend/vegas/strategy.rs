use super::config::*;
use super::ema_filter::{self, EmaDistanceConfig, EmaDistanceState};
use super::indicator_combine::IndicatorCombine;
use super::signal::*;
use super::trend;
use super::utils;
use crate::momentum::stc::StcIndicator;
use crate::signal_weight::{SignalCondition, SignalDirect, SignalType, SignalWeightsConfig};
use crate::volatility::atr::ATR;
use crate::volatility::bollinger::BollingBandsSignalConfig;
use rust_quant_common::enums::common::{EnumAsStrTrait, PeriodEnum};
use rust_quant_common::CandleItem;
use rust_quant_domain::{BacktestResult, BasicRiskStrategyConfig, SignalResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
/// Vegas综合策略配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VegasStrategy {
    /// 周期
    pub period: String,
    /// 回测首次允许产生信号、实盘重启预热及单次信号计算共用的当前周期 K 线根数。
    pub min_k_line_num: usize,
    /// EMA信号配置
    pub ema_signal: Option<EmaSignalConfig>,
    /// 成交量信号配置
    pub volume_signal: Option<VolumeSignalConfig>,
    /// 4H 动量事件激活代理；默认关闭，仅在显式配置后限制开仓窗口
    #[serde(default)]
    pub candle_momentum_activation: CandleMomentumActivationConfig,
    /// 跨币种 ATR 与成交量分位数阈值；默认关闭，研究版本显式开启。
    #[serde(default)]
    pub cross_asset_adaptive_threshold: CrossAssetAdaptiveThresholdConfig,
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
    /// 腿部识别配置
    pub leg_detection_signal: Option<LegDetectionConfig>,
    /// 市场结构配置
    pub market_structure_signal: Option<MarketStructureConfig>,
    /// 震荡过滤配置（仅调整止盈目标，不作为开仓信号）
    pub range_filter_signal: Option<RangeFilterConfig>,
    /// 极端K线过滤/放行配置
    #[serde(default = "default_extreme_k_filter")]
    pub extreme_k_filter_signal: Option<ExtremeKFilterConfig>,
    /// 大实体止损配置
    #[serde(default = "default_large_entity_stop_loss_config")]
    pub large_entity_stop_loss_config: Option<LargeEntityStopLossConfig>,
    /// 追涨追跌确认配置
    #[serde(default = "default_chase_confirm_config")]
    pub chase_confirm_config: Option<ChaseConfirmConfig>,
    /// MACD 信号配置
    #[serde(default = "default_macd_signal_config")]
    pub macd_signal: Option<MacdSignalConfig>,
    /// Fib 回撤入场配置（趋势回调/反弹入场）
    #[serde(default = "default_fib_retracement_signal_config")]
    pub fib_retracement_signal: Option<FibRetracementSignalConfig>,
    /// 入场硬拦截配置（默认全开，保持旧行为）
    #[serde(default = "default_entry_block_config")]
    pub entry_block_config: EntryBlockConfig,
    /// EMA 距离过滤配置（控制 TooFar/Ranging 等阈值）
    #[serde(default = "default_ema_distance_config")]
    pub ema_distance_config: EmaDistanceConfig,
    /// ATR 止损倍数（默认 1.5xATR）
    #[serde(default = "default_atr_stop_loss_multiplier")]
    pub atr_stop_loss_multiplier: f64,
    /// 是否输出信号调试信息（single_value/single_result）
    #[serde(default = "default_emit_debug")]
    pub emit_debug: bool,
}
fn default_ema_distance_config() -> EmaDistanceConfig {
    EmaDistanceConfig::default()
}
fn default_atr_stop_loss_multiplier() -> f64 {
    1.5
}
fn default_emit_debug() -> bool {
    true
}
/// 封装环境变量开关，减少回测策略调用方重复实现相同细节。
fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}
/// 封装环境变量字符串，减少回测策略调用方重复实现相同细节。
fn env_string(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
fn env_f64(name: &str) -> Option<f64> {
    std::env::var(name).ok()?.trim().parse().ok()
}
/// 计算 回测与策略研究 指标，保持公式和边界处理集中可审计。
fn compute_stc_pair(data_items: &[CandleItem]) -> Option<(f64, f64)> {
    if data_items.len() < 60 {
        return None;
    }
    let mut stc = StcIndicator::new(23, 50, 10, 3, 3);
    let mut prev = None;
    let mut current = None;
    for item in recent_indicator_replay_window(data_items, 50) {
        let value = stc.next(item.c);
        prev = current;
        current = Some(value);
    }
    Some((prev?, current?))
}

/// 为每根 K 线临时重建 MACD/STC 时，只回放最长周期的 10 倍数据，降低重复扫描成本。
/// 该窗口是动量过滤器的收敛近似，不可复用于 ATR、EMA 等会直接改变止损或入场边界的指标。
fn recent_indicator_replay_window(
    data_items: &[CandleItem],
    longest_period: usize,
) -> &[CandleItem] {
    let replay_bars = longest_period.max(1).saturating_mul(10);
    &data_items[data_items.len().saturating_sub(replay_bars)..]
}
impl VegasStrategy {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(period: String) -> Self {
        Self {
            period,
            min_k_line_num: 7000,
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
            candle_momentum_activation: CandleMomentumActivationConfig::default(),
            cross_asset_adaptive_threshold: CrossAssetAdaptiveThresholdConfig::default(),
            ema_touch_trend_signal: Some(EmaTouchTrendSignalConfig::default()),
            rsi_signal: Some(RsiSignalConfig::default()),
            bolling_signal: Some(BollingBandsSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            kline_hammer_signal: Some(KlineHammerConfig::default()),
            leg_detection_signal: Some(LegDetectionConfig {
                is_open: false,
                ..LegDetectionConfig::default()
            }),
            market_structure_signal: Some(MarketStructureConfig {
                is_open: false,
                ..MarketStructureConfig::default()
            }),
            range_filter_signal: Some(RangeFilterConfig::default()),
            extreme_k_filter_signal: default_extreme_k_filter(),
            large_entity_stop_loss_config: default_large_entity_stop_loss_config(),
            chase_confirm_config: default_chase_confirm_config(),
            macd_signal: default_macd_signal_config(),
            fib_retracement_signal: default_fib_retracement_signal_config(),
            entry_block_config: default_entry_block_config(),
            ema_distance_config: default_ema_distance_config(),
            atr_stop_loss_multiplier: default_atr_stop_loss_multiplier(),
            emit_debug: default_emit_debug(),
        }
    }
    pub fn get_strategy_name() -> String {
        "vegas".to_string()
    }
    pub fn get_min_data_length(&mut self) -> usize {
        self.min_k_line_num
    }
}
include!("strategy/short_rule_helpers.rs");
include!("strategy/long_rule_helpers.rs");
include!("strategy/long_entry_helpers.rs");
include!("strategy/momentum_activation.rs");
include!("strategy/adaptive_thresholds.rs");
include!("strategy/trade_signal_entry_filters.rs");
include!("strategy/trade_signal.rs");
include!("strategy/indicator_helpers.rs");
#[cfg(test)]
mod tests {
    include!("strategy/tests.rs");
}
