use crate::momentum::stc::StcIndicator;
use crate::signal_weight::{SignalCondition, SignalDirect, SignalType, SignalWeightsConfig};
use crate::volatility::atr::ATR;
use crate::volatility::bollinger::BollingBandsSignalConfig;
use rust_quant_common::enums::common::{EnumAsStrTrait, PeriodEnum};
use rust_quant_common::CandleItem;
use rust_quant_domain::{BacktestResult, BasicRiskStrategyConfig, SignalResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::config::*;
use super::ema_filter::{self, EmaDistanceConfig, EmaDistanceState};
use super::indicator_combine::IndicatorCombine;
use super::signal::*;
use super::trend;
use super::utils;

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

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_string(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn compute_stc_pair(data_items: &[CandleItem]) -> Option<(f64, f64)> {
    if data_items.len() < 60 {
        return None;
    }

    let mut stc = StcIndicator::new(23, 50, 10, 3, 3);
    let mut prev = None;
    let mut current = None;

    for item in data_items {
        let value = stc.next(item.c);
        prev = current;
        current = Some(value);
    }

    Some((prev?, current?))
}

impl VegasStrategy {
    fn is_expansion_continuation_long_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let Some(last) = data_items.last() else {
            return false;
        };

        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd_val = &vegas_indicator_signal_values.macd_value;
        let leg_val = &vegas_indicator_signal_values.leg_detection_value;
        let structure_val = &vegas_indicator_signal_values.market_structure_value;
        let fib_val = &vegas_indicator_signal_values.fib_retracement_value;

        last.c > last.o
            && last.body_ratio() >= 0.65
            && volume_ratio >= 3.0
            && valid_rsi_value.is_some_and(|rsi| (55.0..=72.0).contains(&rsi))
            && macd_val.macd_line > 0.0
            && macd_val.signal_line > 0.0
            && macd_val.macd_line > macd_val.signal_line
            && macd_val.histogram > 0.0
            && macd_val.histogram_increasing
            && !vegas_indicator_signal_values.ema_values.is_short_trend
            && fib_val.in_zone
            && fib_val.volume_confirmed
            && (leg_val.is_bullish_leg || structure_val.internal_bullish_bos)
    }

    fn is_fake_breakout_reversal_short_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let Some(last) = data_items.last() else {
            return false;
        };

        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd_val = &vegas_indicator_signal_values.macd_value;
        let leg_val = &vegas_indicator_signal_values.leg_detection_value;
        let structure_val = &vegas_indicator_signal_values.market_structure_value;
        let fib_val = &vegas_indicator_signal_values.fib_retracement_value;
        let hammer_val = &vegas_indicator_signal_values.kline_hammer_value;

        last.c < last.o
            && volume_ratio >= 1.8
            && (hammer_val.is_short_signal || hammer_val.up_shadow_ratio >= 0.5)
            && leg_val.is_bearish_leg
            && leg_val.is_new_leg
            && fib_val.in_zone
            && fib_val.volume_confirmed
            && structure_val
                .swing_high
                .map(|pivot| pivot.crossed)
                .unwrap_or(false)
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && macd_val.macd_line > 0.0
            && macd_val.signal_line > 0.0
            && macd_val.macd_line < macd_val.signal_line
            && macd_val.histogram < 0.0
            && macd_val.histogram_decreasing
    }

    fn is_above_zero_death_cross_range_break_short_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if data_items.len() < 7 {
            return false;
        }

        let mode = env_string("VEGAS_EXPERIMENT_ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prior_window = &data_items[data_items.len() - 6..data_items.len() - 1];
        let prior_range_high = prior_window
            .iter()
            .map(|item| item.h())
            .fold(f64::MIN, f64::max);
        let prior_range_low = prior_window
            .iter()
            .map(|item| item.l())
            .fold(f64::MAX, f64::min);
        let prior_range_width = (prior_range_high - prior_range_low) / current.c().max(1e-9);
        let close_break_pct = (prior_range_low - current.c()).max(0.0) / current.c().max(1e-9);
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd_val = &vegas_indicator_signal_values.macd_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let structure = &vegas_indicator_signal_values.market_structure_value;

        let base_match = current.c() < current.o()
            && current.body_ratio() >= 0.6
            && macd_val.above_zero
            && macd_val.is_death_cross
            && macd_val.histogram < 0.0
            && structure.swing_trend == 1
            && !structure.internal_bearish_bos
            && !structure.swing_bearish_bos;

        match mode.as_str() {
            "v3" => {
                base_match
                    && volume_ratio >= 1.3
                    && !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && matches!(
                        ema_distance.state,
                        EmaDistanceState::TooFar | EmaDistanceState::Normal
                    )
                    && prior_range_width <= 0.025
                    && close_break_pct >= 0.012
            }
            "v2" => {
                base_match
                    && volume_ratio >= 1.3
                    && !ema_values.is_long_trend
                    && !matches!(ema_distance.state, EmaDistanceState::Tangled)
                    && prior_range_width <= 0.04
                    && close_break_pct >= 0.0075
            }
            "v1" | "1" | "true" | "yes" | "on" => {
                base_match
                    && volume_ratio >= 1.5
                    && !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && prior_range_width <= 0.03
                    && close_break_pct >= 0.01
            }
            _ => false,
        }
    }

    fn round_level_step(price: f64) -> f64 {
        if price >= 10_000.0 {
            1_000.0
        } else if price >= 1_000.0 {
            100.0
        } else if price >= 100.0 {
            10.0
        } else if price >= 10.0 {
            1.0
        } else {
            0.1
        }
    }

    fn is_round_level_reversal_long_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if data_items.len() < 10 {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prev = &data_items[data_items.len() - 2];
        let prior = &data_items[data_items.len() - 10..data_items.len() - 1];
        let step = Self::round_level_step(prev.c());
        let level = (prev.c() / step).floor() * step;
        let touch_tol = step * 0.05;
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let shock_drop_pct = ((prev.c() - current.l()) / prev.c().max(1e-9)).max(0.0);

        let held_above = prior.iter().all(|item| item.l() > level + touch_tol);
        let first_touch = prev.l() > level + touch_tol && current.l() <= level + touch_tol;
        let reclaim_close = current.c() >= level - touch_tol;
        let reversal_shape = current.down_shadow_ratio() >= 0.45
            && (current.c() >= current.o() || current.body_ratio() <= 0.45);

        held_above
            && first_touch
            && shock_drop_pct >= 0.025
            && volume_ratio >= 3.0
            && reclaim_close
            && reversal_shape
            && !vegas_indicator_signal_values
                .market_structure_value
                .internal_bearish_bos
            && !vegas_indicator_signal_values
                .market_structure_value
                .swing_bearish_bos
    }

    fn is_round_level_reversal_short_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if data_items.len() < 10 {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prev = &data_items[data_items.len() - 2];
        let prior = &data_items[data_items.len() - 10..data_items.len() - 1];
        let step = Self::round_level_step(prev.c());
        let level = (prev.c() / step).ceil() * step;
        let touch_tol = step * 0.05;
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        let shock_rise_pct = ((current.h() - prev.c()) / prev.c().max(1e-9)).max(0.0);
        let mode = env_string("VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL_SHORT_MODE")
            .unwrap_or_else(|| "v1".to_string());

        let held_below = prior.iter().all(|item| item.h() < level - touch_tol);
        let first_touch = prev.h() < level - touch_tol && current.h() >= level - touch_tol;
        let reject_close = current.c() <= level + touch_tol;
        let reversal_shape = current.up_shadow_ratio() >= 0.45
            && (current.c() <= current.o() || current.body_ratio() <= 0.45);

        let base_match = held_below
            && first_touch
            && shock_rise_pct >= 0.025
            && volume_ratio >= 3.0
            && reject_close
            && reversal_shape
            && !vegas_indicator_signal_values
                .market_structure_value
                .internal_bullish_bos
            && !vegas_indicator_signal_values
                .market_structure_value
                .swing_bullish_bos;

        match mode.as_str() {
            "v2" => {
                base_match
                    && !ema_values.is_short_trend
                    && fib.retracement_ratio >= 0.5
                    && (rsi >= 65.0 || ema_distance.state == EmaDistanceState::TooFar)
            }
            _ => base_match,
        }
    }

    fn should_block_exhaustion_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        rsi < 25.0
            && volume.volume_ratio >= 5.0
            && !fib.in_zone
            && fib.retracement_ratio <= 0.05
            && boll.is_long_signal
            && ema_touch.is_short_signal
            && ema_values.is_short_trend
            && !leg.is_new_leg
            && macd.macd_line < 0.0
            && macd.signal_line < 0.0
    }

    fn should_block_bullish_leg_mean_reversion_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        volume.volume_ratio >= 1.8
            && !ema_values.is_short_trend
            && leg.is_bullish_leg
            && !leg.is_new_leg
            && fib.in_zone
            && fib.volume_confirmed
            && fib.leg_bullish
            && boll.is_short_signal
            && !ema_touch.is_short_signal
            && (45.0..=50.0).contains(&rsi)
            && macd.macd_line < 0.0
            && macd.signal_line < 0.0
            && macd.histogram > 0.0
            && macd.histogram_decreasing
    }

    fn should_block_deep_negative_macd_recovery_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        let mode = env_string("VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE")
            .unwrap_or_else(|| "v1".to_string());

        let macd_recovery_core =
            !ema_touch.is_short_signal && macd.histogram > 0.0 && macd.histogram_decreasing;
        let macd_depth_ratio = if signal_price > 0.0 {
            macd.macd_line.abs() / signal_price
        } else {
            0.0
        };
        let signal_depth_ratio = if signal_price > 0.0 {
            macd.signal_line.abs() / signal_price
        } else {
            0.0
        };

        match mode.as_str() {
            "off" => false,
            "v2" => {
                macd_recovery_core
                    && ema_values.is_short_trend
                    && boll.is_long_signal
                    && (engulfing.is_valid_engulfing || leg.is_bearish_leg)
                    && (!fib.in_zone || !fib.volume_confirmed)
                    && volume.volume_ratio < 2.0
                    && rsi < 42.0
                    && macd.macd_line < -60.0
                    && macd.signal_line < -60.0
            }
            "v3" => {
                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 2.2
                    && rsi < 45.0
                    && macd.macd_line < -50.0
                    && macd.signal_line < -50.0
            }
            "v5" => {
                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 2.2
                    && rsi < 50.0
                    && macd.macd_line < -50.0
                    && macd.signal_line < -50.0
            }
            "v6" => {
                engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 2.2
                    && rsi < 45.0
                    && macd.macd_line < -50.0
                    && macd.signal_line < -50.0
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
            }
            "v7" => {
                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.6
                    && (34.0..=43.0).contains(&rsi)
                    && macd_depth_ratio >= 0.007
                    && signal_depth_ratio >= 0.0085
            }
            "v8" => {
                let use_absolute_thresholds = signal_price >= 10_000.0;

                macd_recovery_core
                    && engulfing.is_valid_engulfing
                    && !fib.volume_confirmed
                    && if use_absolute_thresholds {
                        volume.volume_ratio < 2.2
                            && rsi < 45.0
                            && macd.macd_line < -50.0
                            && macd.signal_line < -50.0
                    } else {
                        volume.volume_ratio < 1.6
                            && (34.0..=43.0).contains(&rsi)
                            && macd_depth_ratio >= 0.007
                            && signal_depth_ratio >= 0.0085
                    }
            }
            _ => {
                ema_values.is_short_trend
                    && engulfing.is_valid_engulfing
                    && boll.is_long_signal
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && (1.0..=1.5).contains(&volume.volume_ratio)
                    && (30.0..=38.0).contains(&rsi)
                    && macd.macd_line < -80.0
                    && macd.signal_line < -80.0
                    && macd_recovery_core
            }
        }
    }

    fn should_block_stc_early_weakening_short(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_STC_EARLY_WEAKENING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let Some((prev_stc, current_stc)) = compute_stc_pair(data_items) else {
            return false;
        };

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && !ema_touch.is_short_signal
                    && volume.volume_ratio < 2.5
                    && (45.0..=52.0).contains(&rsi)
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.macd_line < macd.signal_line
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && prev_stc >= 60.0
                    && current_stc >= 45.0
                    && current_stc < prev_stc
            }
            _ => false,
        }
    }

    fn should_block_weakening_no_structure_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_WEAKENING_NO_STRUCTURE_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && !hammer.is_short_signal
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && !ema_touch.is_short_signal
                    && volume.volume_ratio < 2.5
                    && (45.0..=52.0).contains(&rsi)
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.macd_line < macd.signal_line
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
            }
            _ => false,
        }
    }

    fn should_block_deep_negative_weak_breakdown_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_WEAK_BREAKDOWN_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                engulfing.is_valid_engulfing
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !ema_values.is_short_trend
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && fib.retracement_ratio < 0.08
                    && volume.volume_ratio < 2.0
                    && rsi < 30.0
                    && ema_touch.is_short_signal
                    && macd.macd_line < -60.0
                    && macd.signal_line < -50.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_shallow_weakening_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_SHALLOW_WEAKENING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && !ema_touch.is_short_signal
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
                    && volume.volume_ratio < 2.5
                    && (44.0..=50.0).contains(&rsi)
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.macd_line < macd.signal_line
                    && (-2.0..0.0).contains(&macd.histogram)
                    && macd.histogram_decreasing
            }
            _ => false,
        }
    }

    fn should_block_panic_breakdown_short(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode =
            env_string("VEGAS_PANIC_BREAKDOWN_SHORT_BLOCK").unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let Some(last) = data_items.last() else {
            return false;
        };

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                last.c < last.o
                    && last.body_ratio() >= 0.8
                    && ema_distance.state == EmaDistanceState::Ranging
                    && !ema_values.is_short_trend
                    && !ema_touch.is_short_signal
                    && boll.is_long_signal
                    && boll.is_short_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && volume.volume_ratio >= 4.5
                    && fib.volume_confirmed
                    && !fib.in_zone
                    && fib.retracement_ratio >= 0.6
                    && (38.0..=45.0).contains(&rsi)
                    && macd.macd_line < 0.0
                    && macd.signal_line < 0.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && market.internal_bearish_bos
                    && market
                        .internal_low
                        .as_ref()
                        .is_some_and(|pivot| pivot.crossed)
            }
            _ => false,
        }
    }

    fn should_block_above_zero_no_trend_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_NO_TREND_HANGING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_short_trend
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && fib.retracement_ratio >= 0.85
                    && volume.volume_ratio < 1.0
                    && rsi >= 68.0
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_below_zero_weakening_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_short_trend
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.8
                    && (42.0..=50.0).contains(&rsi)
                    && macd.macd_line < 0.0
                    && macd.signal_line < 0.0
                    && macd.histogram < 0.0
                    && macd.histogram_increasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_no_trend_too_far_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.5
                    && rsi >= 55.0
                    && macd.above_zero
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_low_volume_no_trend_hanging_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_short_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && hammer.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.0
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_long_trend_pullback_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_PULLBACK_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && ema_touch.is_uptrend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_short_signal
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 2.0
                    && rsi <= 45.0
                    && macd.macd_line < 0.0
                    && macd.signal_line < 0.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_long_trend_above_zero_low_volume_weakening_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;
        let histogram_ratio = if signal_price > 0.0 {
            macd.histogram.abs() / signal_price
        } else {
            0.0
        };

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.2
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            "v2" => {
                ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.2
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
                    && histogram_ratio >= 0.002
            }
            _ => false,
        }
    }

    fn should_block_long_trend_above_zero_high_rsi_early_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_ABOVE_ZERO_HIGH_RSI_EARLY_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && volume.volume_ratio >= 1.5
                    && rsi >= 65.0
                    && macd.above_zero
                    && macd.histogram < 0.0
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_low_volume_neutral_rsi_macd_recovery_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let mode = env_string("VEGAS_LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_SHORT_BLOCK")
            .unwrap_or_else(|| "v1".to_string());
        if mode == "off" {
            return false;
        }

        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        let macd = &vegas_indicator_signal_values.macd_value;
        let signal_line_ratio = if signal_price > 0.0 {
            macd.signal_line.abs() / signal_price
        } else {
            0.0
        };
        let rsi_is_neutral = valid_rsi_value
            .map(|rsi| (47.0..=53.0).contains(&rsi))
            .unwrap_or(false);
        let macd_recovering_below_zero = macd.macd_line < 0.0
            && macd.signal_line < 0.0
            && macd.macd_line > macd.signal_line
            && macd.histogram > 0.0;

        match mode.as_str() {
            "v1" => volume_ratio < 1.0 && rsi_is_neutral && macd_recovering_below_zero,
            "v2" => {
                volume_ratio < 1.0
                    && rsi_is_neutral
                    && macd_recovering_below_zero
                    && signal_line_ratio >= 0.002
            }
            _ => false,
        }
    }

    fn should_block_weak_breakout_no_trend_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_WEAK_BREAKOUT_NO_TREND_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && !engulfing.is_valid_engulfing
                    && !hammer.is_long_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && fib.in_zone
                    && fib.volume_confirmed
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && volume.volume_ratio >= 2.5
                    && rsi >= 58.0
                    && macd.above_zero
                    && macd.is_golden_cross
                    && macd.histogram > 0.0
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_ranging_no_trend_weak_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_RANGING_NO_TREND_WEAK_HAMMER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::Ranging
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && hammer.is_long_signal
                    && !hammer.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !fib.volume_confirmed
                    && !macd.above_zero
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && rsi < 45.0
                    && volume.volume_ratio < 1.5
            }
            _ => false,
        }
    }

    fn should_block_high_volume_too_far_bollinger_short_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && leg.is_bullish_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_macd_near_zero_weak_hammer_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let volume = &vegas_indicator_signal_values.volume_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;

        ema_distance.state == EmaDistanceState::TooFar
            && ema_values.is_short_trend
            && hammer.is_short_signal
            && !engulfing.is_valid_engulfing
            && macd.histogram.abs() < 2.0
            && volume.volume_ratio < 1.0
    }

    fn should_block_too_far_uptrend_opposing_hammer_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        ema_distance.state == EmaDistanceState::TooFar
            && ema_touch.is_uptrend
            && ema_values.is_long_trend
            && !ema_values.is_short_trend
            && !fib.in_zone
            && boll.is_short_signal
            && !boll.is_long_signal
            && leg.is_bullish_leg
            && !leg.is_bearish_leg
            && !leg.is_new_leg
            && hammer.is_short_signal
            && !engulfing.is_valid_engulfing
            && macd.histogram > 0.0
            && rsi >= 55.0
    }

    fn should_block_high_volume_no_trend_bollinger_long_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::Normal
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && leg.is_bearish_leg
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && macd.histogram < 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_high_volume_conflicting_bollinger_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                boll.is_long_signal
                    && boll.is_short_signal
                    && leg.is_bullish_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_high_volume_internal_down_counter_trend_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_short_trend
                    && boll.is_long_signal
                    && !boll.is_short_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && market.internal_trend == -1
                    && !macd.above_zero
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_high_volume_ranging_recovery_short(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_RANGING_RECOVERY_SHORT_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::Ranging
                    && engulfing.is_valid_engulfing
                    && fib.volume_confirmed
                    && volume.volume_ratio >= 3.0
                    && !macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_decreasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && !market.internal_bearish_bos
                    && !market.swing_bearish_bos
            }
            _ => false,
        }
    }

    fn should_block_high_volume_high_rsi_bollinger_short_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && matches!(
                        ema_distance.state,
                        EmaDistanceState::Normal | EmaDistanceState::Ranging
                    )
                    && boll.is_short_signal
                    && macd.above_zero
                    && leg.is_bullish_leg
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && volume.volume_ratio >= 4.0
                    && rsi >= 65.0
                    && !engulfing.is_valid_engulfing
                    && !hammer.is_long_signal
            }
            _ => false,
        }
    }

    fn should_block_deep_negative_no_trend_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_NO_TREND_HAMMER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                boll.is_long_signal
                    && hammer.is_long_signal
                    && !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && volume.volume_ratio < 2.1
                    && macd.macd_line < -60.0
                    && macd.signal_line < -60.0
                    && macd.histogram < 0.0
                    && macd.histogram_increasing
            }
            _ => false,
        }
    }

    fn should_block_short_trend_too_far_bollinger_short_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_SHORT_TREND_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !ema_touch.is_long_signal
                    && boll.is_short_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.2
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
            }
            _ => false,
        }
    }

    fn should_block_short_trend_new_bull_leg_counter_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        signal_price: f64,
    ) -> bool {
        let mode = env_string("VEGAS_SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let histogram_ratio = if signal_price > 0.0 {
            macd.histogram.abs() / signal_price
        } else {
            0.0
        };

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && leg.is_bullish_leg
                    && leg.is_new_leg
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !fib.volume_confirmed
                    && !boll.is_long_signal
                    && volume.volume_ratio < 1.5
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            "v2" => {
                ema_values.is_short_trend
                    && leg.is_bullish_leg
                    && leg.is_new_leg
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !fib.volume_confirmed
                    && !boll.is_long_signal
                    && volume.volume_ratio < 1.5
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && macd.histogram > 0.0
                    && histogram_ratio >= 0.0015
            }
            _ => false,
        }
    }

    fn should_block_short_trend_no_bollinger_rebound_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_short_trend
                    && leg.is_bullish_leg
                    && ema_distance.state == EmaDistanceState::TooFar
                    && !fib.volume_confirmed
                    && !boll.is_long_signal
                    && !boll.is_short_signal
                    && macd.above_zero
                    && volume.volume_ratio < 1.5
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_normal_bull_leg_no_confirm_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;

        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::Normal
                    && leg.is_bullish_leg
                    && !leg.is_bearish_leg
                    && !boll.is_long_signal
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.5
                    && macd.histogram > 0.0
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_above_zero_no_trend_engulfing_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.5
                    && rsi >= 60.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            "v2" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && !ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && !ema_touch.is_long_signal
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.0
                    && rsi >= 70.0
                    && macd.above_zero
                    && macd.histogram > 0.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_protect_long_trend_deep_negative_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_DEEP_NEGATIVE_HAMMER_PROTECT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_values.is_long_trend
                    && ema_distance.state == EmaDistanceState::TooFar
                    && hammer.is_long_signal
                    && leg.is_bearish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && !fib.volume_confirmed
                    && volume.volume_ratio < 1.6
                    && rsi < 40.0
                    && macd.macd_line < -30.0
                    && macd.signal_line < 0.0
                    && macd.histogram < -20.0
                    && macd.histogram_increasing
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
            }
            _ => false,
        }
    }

    fn should_block_long_trend_below_zero_fib_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_LONG_TREND_BELOW_ZERO_FIB_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && ema_values.is_long_trend
                    && !ema_values.is_short_trend
                    && !ema_touch.is_long_signal
                    && !boll.is_long_signal
                    && !boll.is_short_signal
                    && !engulfing.is_valid_engulfing
                    && !hammer.is_long_signal
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && fib.in_zone
                    && fib.volume_confirmed
                    && fib.is_long_signal
                    && fib.retracement_ratio < 0.5
                    && !macd.above_zero
                    && market.internal_trend < 0
                    && volume.volume_ratio < 2.1
                    && (40.0..46.0).contains(&rsi)
            }
            _ => false,
        }
    }

    fn should_block_high_level_sideways_chase_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode =
            env_string("VEGAS_HIGH_LEVEL_SIDEWAYS_LONG_BLOCK").unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        ema_values.is_long_trend
            && !ema_values.is_short_trend
            && !ema_touch.is_long_signal
            && engulfing.is_valid_engulfing
            && leg.is_bullish_leg
            && !leg.is_new_leg
            && !fib.in_zone
            && !fib.volume_confirmed
            && fib.retracement_ratio >= 0.75
            && volume.volume_ratio < 1.6
            && hammer.body_ratio < 0.55
            && boll.is_short_signal
            && !boll.is_long_signal
            && macd.above_zero
            && macd.histogram > 0.0
            && rsi < 60.0
            && !market.internal_bullish_bos
            && !market.swing_bullish_bos
            && !market
                .internal_high
                .as_ref()
                .is_some_and(|pivot| pivot.crossed)
            && !market
                .swing_high
                .as_ref()
                .is_some_and(|pivot| pivot.crossed)
    }

    fn should_block_above_zero_high_level_chase_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let market = &vegas_indicator_signal_values.market_structure_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && !ema_values.is_long_trend
                    && !ema_touch.is_long_signal
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !leg.is_new_leg
                    && !fib.in_zone
                    && fib.retracement_ratio >= 0.9
                    && volume.volume_ratio < 1.2
                    && boll.is_short_signal
                    && !boll.is_long_signal
                    && rsi >= 68.0
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.histogram > 0.0
                    && !market.internal_bullish_bos
                    && !market.swing_bullish_bos
                    && !market
                        .internal_high
                        .as_ref()
                        .is_some_and(|pivot| pivot.crossed)
                    && !market
                        .swing_high
                        .as_ref()
                        .is_some_and(|pivot| pivot.crossed)
            }
            _ => false,
        }
    }

    fn should_protect_above_zero_high_level_chase_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_PROTECT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let leg = &vegas_indicator_signal_values.leg_detection_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v1" => {
                ema_distance.state == EmaDistanceState::TooFar
                    && !ema_values.is_long_trend
                    && engulfing.is_valid_engulfing
                    && leg.is_bullish_leg
                    && !fib.in_zone
                    && fib.retracement_ratio >= 0.9
                    && volume.volume_ratio < 1.0
                    && boll.is_short_signal
                    && rsi >= 70.0
                    && macd.macd_line > 0.0
                    && macd.signal_line > 0.0
                    && macd.histogram > 0.0
            }
            _ => false,
        }
    }

    fn is_deep_negative_hammer_long_candidate(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let volume = &vegas_indicator_signal_values.volume_value;
        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_touch = &vegas_indicator_signal_values.ema_touch_value;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let engulfing = &vegas_indicator_signal_values.engulfing_value;
        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let macd = &vegas_indicator_signal_values.macd_value;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        boll.is_long_signal
            && hammer.is_long_signal
            && !ema_touch.is_long_signal
            && !engulfing.is_valid_engulfing
            && !fib.volume_confirmed
            && volume.volume_ratio < 1.5
            && rsi < 40.0
            && macd.macd_line < -30.0
            && macd.signal_line < -10.0
            && macd.histogram < -20.0
            && (ema_values.is_short_trend || ema_values.is_long_trend)
    }

    fn should_block_deep_negative_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_HAMMER_LONG_BLOCK")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        match mode.as_str() {
            "v1" => Self::is_deep_negative_hammer_long_candidate(vegas_indicator_signal_values),
            _ => false,
        }
    }

    fn should_protect_deep_negative_hammer_long(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode = env_string("VEGAS_DEEP_NEGATIVE_HAMMER_LONG_PROTECT")
            .unwrap_or_else(|| "off".to_string());
        if mode == "off" {
            return false;
        }

        match mode.as_str() {
            "v1" => Self::is_deep_negative_hammer_long_candidate(vegas_indicator_signal_values),
            _ => false,
        }
    }

    fn is_repair_long_candidate(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        vegas_indicator_signal_values.ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !vegas_indicator_signal_values.fib_retracement_value.in_zone
            && vegas_indicator_signal_values
                .kline_hammer_value
                .is_long_signal
            && valid_rsi_value.is_some_and(|rsi| rsi < 45.0)
            && vegas_indicator_signal_values.macd_value.histogram < 0.0
            && vegas_indicator_signal_values
                .macd_value
                .histogram_increasing
            && vegas_indicator_signal_values.volume_value.volume_ratio <= 1.6
    }

    fn is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let histogram = vegas_indicator_signal_values.macd_value.histogram;
        let hammer_body_ratio = vegas_indicator_signal_values.kline_hammer_value.body_ratio;
        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;

        vegas_indicator_signal_values.ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !vegas_indicator_signal_values.fib_retracement_value.in_zone
            && vegas_indicator_signal_values
                .kline_hammer_value
                .is_long_signal
            && vegas_indicator_signal_values
                .leg_detection_value
                .is_bearish_leg
            && vegas_indicator_signal_values.leg_detection_value.is_new_leg
            && valid_rsi_value.is_some_and(|rsi| rsi < 45.0)
            && histogram >= 0.0
            && histogram <= 3.0
            && hammer_body_ratio >= 0.15
            && volume_ratio >= 1.5
            && volume_ratio <= 3.0
    }

    fn should_block_recent_upper_shadow_pressure_long(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let mode =
            env_string("VEGAS_RECENT_UPPER_SHADOW_LONG_BLOCK").unwrap_or_else(|| "off".to_string());
        if mode == "off" || data_items.len() < 4 {
            return false;
        }

        let current = data_items.last().expect("数据不能为空");
        let prev_1 = &data_items[data_items.len() - 2];
        let prev_2 = &data_items[data_items.len() - 3];
        let prev_3 = &data_items[data_items.len() - 4];

        let has_recent_upper_shadow_pressure = [(prev_2, prev_3), (prev_1, prev_2)]
            .into_iter()
            .any(|(candidate, prev)| {
                candidate.up_shadow_ratio() >= 0.18
                    && candidate.v > prev.v * 1.2
                    && candidate.body_ratio() < 0.75
            });

        if !has_recent_upper_shadow_pressure {
            return false;
        }

        let current_is_strong_breakout = current.c > current.o
            && current.body_ratio() >= 0.65
            && current.v > prev_1.v.max(prev_2.v);

        let fib = &vegas_indicator_signal_values.fib_retracement_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        let ema_distance = &vegas_indicator_signal_values.ema_distance_filter;
        let ema_values = &vegas_indicator_signal_values.ema_values;
        let rsi = vegas_indicator_signal_values.rsi_value.rsi_value;

        match mode.as_str() {
            "v3" => {
                has_recent_upper_shadow_pressure
                    && !current_is_strong_breakout
                    && current.v < prev_1.v.max(prev_2.v)
                    && !ema_values.is_long_trend
                    && ema_distance.should_filter_long
                    && boll.is_short_signal
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && fib.retracement_ratio > 0.90
                    && rsi > 60.0
            }
            "v2" => {
                has_recent_upper_shadow_pressure
                    && !current_is_strong_breakout
                    && current.v < prev_1.v.max(prev_2.v)
                    && !ema_values.is_long_trend
                    && ema_distance.should_filter_long
                    && boll.is_short_signal
                    && !fib.in_zone
                    && !fib.volume_confirmed
                    && rsi > 60.0
            }
            _ => {
                !current_is_strong_breakout
                    && (boll.is_short_signal || !fib.in_zone || !fib.volume_confirmed)
            }
        }
    }

    fn is_rebound_protect_long_candidate(
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        let Some(last) = data_items.last() else {
            return false;
        };

        let hammer = &vegas_indicator_signal_values.kline_hammer_value;
        let boll = &vegas_indicator_signal_values.bollinger_value;
        if !hammer.is_hammer || !hammer.is_long_signal || !boll.is_long_signal || last.c <= last.o {
            return false;
        }

        let strong_hammer = hammer.down_shadow_ratio >= 0.70 && hammer.body_ratio <= 0.12;
        if !strong_hammer {
            return false;
        }

        let recent_high = data_items
            .iter()
            .rev()
            .skip(1)
            .take(6)
            .map(|c| c.h)
            .fold(last.h, f64::max);
        let pullback_pct = if recent_high > 0.0 {
            (recent_high - last.l) / recent_high
        } else {
            0.0
        };
        if pullback_pct < 0.01 {
            return false;
        }
        true
    }

    pub fn new(period: String) -> Self {
        Self {
            period,
            min_k_line_num: 7000,
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
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
            ema_distance_config: default_ema_distance_config(),
            atr_stop_loss_multiplier: default_atr_stop_loss_multiplier(),
            emit_debug: default_emit_debug(),
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
                stop_loss_source: None,
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
                filter_reasons: vec![],
                dynamic_adjustments: vec![],
                dynamic_config_snapshot: None,
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
                    stop_loss_source: None,
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
                    filter_reasons: vec![],
                    dynamic_adjustments: vec![],
                    dynamic_config_snapshot: None,
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
            stop_loss_source: None,
            ts: Some(last_data_item.ts),
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
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
        };

        let mut conditions = Vec::with_capacity(10);
        let mut valid_rsi_value: Option<f64> = None;
        let mut dynamic_adjustments: Vec<String> = Vec::new();
        let mut range_snapshot: Option<serde_json::Value> = None;

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
                    signal_result
                        .filter_reasons
                        .push("RSI_EXTREME_EVENT".to_string());
                    dynamic_adjustments.push("RSI_EXTREME_EVENT".to_string());
                    signal_result.dynamic_adjustments = dynamic_adjustments.clone();
                    signal_result.dynamic_config_snapshot = Some(
                        json!({
                            "kline_ts": last_data_item.ts,
                            "adjustments": dynamic_adjustments,
                        })
                        .to_string(),
                    );
                    return signal_result;
                }
            };

            valid_rsi_value = Some(current_rsi);

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

        // 腿部识别（可选）：只在 is_open 时参与条件打分
        if let Some(leg_detection_signal) = &self.leg_detection_signal {
            if leg_detection_signal.is_open {
                let leg_value = vegas_indicator_signal_values.leg_detection_value;
                if leg_value.is_bullish_leg || leg_value.is_bearish_leg {
                    conditions.push((
                        SignalType::LegDetection,
                        SignalCondition::LegDetection {
                            is_bullish_leg: leg_value.is_bullish_leg,
                            is_bearish_leg: leg_value.is_bearish_leg,
                            is_new_leg: leg_value.is_new_leg,
                        },
                    ));
                }
            }
        }

        if let Some(market_structure_signal) = &self.market_structure_signal {
            if market_structure_signal.is_open {
                let structure_value = &vegas_indicator_signal_values.market_structure_value;
                let has_swing_signal = structure_value.swing_bullish_bos
                    || structure_value.swing_bearish_bos
                    || structure_value.swing_bullish_choch
                    || structure_value.swing_bearish_choch;
                let has_internal_signal = structure_value.internal_bullish_bos
                    || structure_value.internal_bearish_bos
                    || structure_value.internal_bullish_choch
                    || structure_value.internal_bearish_choch;

                let can_use_swing = market_structure_signal.enable_swing_signal && has_swing_signal;
                let can_use_internal = market_structure_signal.enable_internal_signal
                    && has_internal_signal
                    && (!market_structure_signal.enable_swing_signal || !has_swing_signal);

                if can_use_swing || can_use_internal {
                    let use_internal = !can_use_swing && can_use_internal;
                    let (bullish_bos, bearish_bos, bullish_choch, bearish_choch) = if use_internal {
                        (
                            structure_value.internal_bullish_bos,
                            structure_value.internal_bearish_bos,
                            structure_value.internal_bullish_choch,
                            structure_value.internal_bearish_choch,
                        )
                    } else {
                        (
                            structure_value.swing_bullish_bos,
                            structure_value.swing_bearish_bos,
                            structure_value.swing_bullish_choch,
                            structure_value.swing_bearish_choch,
                        )
                    };

                    conditions.push((
                        SignalType::MarketStructure,
                        SignalCondition::MarketStructure {
                            is_bullish_bos: bullish_bos,
                            is_bearish_bos: bearish_bos,
                            is_bullish_choch: bullish_choch,
                            is_bearish_choch: bearish_choch,
                            is_internal: use_internal,
                        },
                    ));
                }
            }
        }

        // ================================================================
        // 【新增】EMA距离过滤
        // ================================================================
        let ema_distance_config = self.ema_distance_config;
        let ema_distance_filter = ema_filter::apply_ema_distance_filter(
            last_data_item.c,
            &vegas_indicator_signal_values.ema_values,
            &ema_distance_config,
        );
        vegas_indicator_signal_values.ema_distance_filter = ema_distance_filter;

        // ================================================================
        // 【新增】MACD 计算
        // ================================================================
        if let Some(macd_cfg) = &self.macd_signal {
            if macd_cfg.is_open && data_items.len() > macd_cfg.slow_period + macd_cfg.signal_period
            {
                use ta::indicators::MovingAverageConvergenceDivergence;
                use ta::Next;

                let mut macd = MovingAverageConvergenceDivergence::new(
                    macd_cfg.fast_period,
                    macd_cfg.slow_period,
                    macd_cfg.signal_period,
                )
                .unwrap();

                let mut prev_macd = 0.0f64;
                let mut prev_signal = 0.0f64;
                let mut prev_histogram = 0.0f64;
                let mut prev_prev_histogram = 0.0f64;

                // 计算所有 K 线的 MACD
                for item in data_items.iter() {
                    let macd_output = macd.next(item.c);
                    prev_prev_histogram = prev_histogram;
                    prev_histogram = macd_output.macd - macd_output.signal;
                    prev_signal = macd_output.signal;
                    prev_macd = macd_output.macd;
                }

                let histogram = prev_macd - prev_signal;

                // 判断金叉死叉：当前 histogram > 0 且前一根 < 0
                let is_golden_cross = histogram > 0.0 && prev_prev_histogram <= 0.0;
                let is_death_cross = histogram < 0.0 && prev_prev_histogram >= 0.0;

                // 判断柱状图趋势
                let histogram_increasing = histogram > prev_prev_histogram;
                let histogram_decreasing = histogram < prev_prev_histogram;
                // 判断动量是否正在改善（用于识别触底反弹）
                // 对于负区域：histogram > prev_histogram 表示负值在变小，动量改善
                let histogram_improving = histogram > prev_histogram;

                vegas_indicator_signal_values.macd_value = super::signal::MacdSignalValue {
                    macd_line: prev_macd,
                    signal_line: prev_signal,
                    histogram,
                    is_golden_cross,
                    is_death_cross,
                    histogram_increasing,
                    histogram_decreasing,
                    above_zero: prev_macd > 0.0,
                    prev_histogram: prev_prev_histogram,
                    histogram_improving,
                };
            }
        }

        // ================================================================
        // 【新增】Fib 回撤入场信号（Swing + Fib + 放量）
        // ================================================================
        let fib_cfg = self.fib_retracement_signal.unwrap_or_default();
        if fib_cfg.is_open {
            vegas_indicator_signal_values.fib_retracement_value =
                super::swing_fib::generate_fib_retracement_signal(
                    data_items,
                    &vegas_indicator_signal_values.ema_values,
                    &vegas_indicator_signal_values.leg_detection_value,
                    vegas_indicator_signal_values.volume_value.volume_ratio,
                    &fib_cfg,
                );
        } else {
            vegas_indicator_signal_values
                .fib_retracement_value
                .volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        }

        // ================================================================
        // 计算得分
        // ================================================================
        let score = weights.calculate_score(conditions.clone());

        // 计算分数到达指定值
        // 计算分数到达指定值
        let mut signal_direction = weights.is_signal_valid(&score);
        if fib_cfg.is_open {
            let fib_val = vegas_indicator_signal_values.fib_retracement_value;
            let fib_direction = if fib_val.is_long_signal {
                Some(SignalDirect::IsLong)
            } else if fib_val.is_short_signal {
                Some(SignalDirect::IsShort)
            } else {
                None
            };

            // Fib 触发时优先使用 Fib 方向（即使原权重系统没有达到阈值）
            if fib_direction.is_some() {
                signal_direction = fib_direction;
            } else if fib_cfg.only_on_fib {
                // 仅Fib模式：未触发Fib则不允许开仓
                signal_direction = None;
            }
        }

        if signal_direction.is_none()
            && env_flag("VEGAS_EXPERIMENT_EXPANSION_CONTINUATION_LONG")
            && Self::is_expansion_continuation_long_candidate(
                data_items,
                vegas_indicator_signal_values,
                valid_rsi_value,
            )
        {
            signal_direction = Some(SignalDirect::IsLong);
            dynamic_adjustments.push("EXPANSION_CONTINUATION_LONG".to_string());
        }

        if signal_direction.is_none()
            && env_flag("VEGAS_EXPERIMENT_FAKE_BREAKOUT_REVERSAL_SHORT")
            && Self::is_fake_breakout_reversal_short_candidate(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_direction = Some(SignalDirect::IsShort);
            dynamic_adjustments.push("FAKE_BREAKOUT_REVERSAL_SHORT".to_string());
        }

        if signal_direction.is_none()
            && Self::is_above_zero_death_cross_range_break_short_candidate(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_direction = Some(SignalDirect::IsShort);
            dynamic_adjustments.push("ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT".to_string());
        }

        if env_flag("VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL") {
            let round_level_long_candidate = Self::is_round_level_reversal_long_candidate(
                data_items,
                vegas_indicator_signal_values,
            );
            let round_level_short_candidate = Self::is_round_level_reversal_short_candidate(
                data_items,
                vegas_indicator_signal_values,
            );

            if round_level_long_candidate && !round_level_short_candidate {
                signal_direction = Some(SignalDirect::IsLong);
                dynamic_adjustments.push("ROUND_LEVEL_REVERSAL_LONG".to_string());
            } else if round_level_short_candidate && !round_level_long_candidate {
                signal_direction = Some(SignalDirect::IsShort);
                dynamic_adjustments.push("ROUND_LEVEL_REVERSAL_SHORT".to_string());
            }
        }

        if let Some(signal_direction) = signal_direction {
            // 计算 ATR 用于止损价格
            let mut atr = ATR::new(14).unwrap();
            for item in data_items.iter() {
                atr.next(item.h, item.l, item.c);
            }
            let atr_value = atr.value();
            let atr_multiplier = self.atr_stop_loss_multiplier.max(0.0);

            // 检查大实体（Large Entity）状态
            let mut is_large_entity = false;
            let mut large_entity_retracement_sl: Option<f64> = None;

            if let Some(large_entity_cfg) = &self.large_entity_stop_loss_config {
                if large_entity_cfg.is_open {
                    let body_ratio = last_data_item.body_ratio();
                    let move_pct =
                        (last_data_item.c - last_data_item.o).abs() / last_data_item.o.max(1e-9);
                    let range = last_data_item.h - last_data_item.l;

                    if body_ratio >= large_entity_cfg.min_body_ratio
                        && move_pct >= large_entity_cfg.min_move_pct
                    {
                        is_large_entity = true;
                        // 计算基于回撤比例的止损
                        match signal_direction {
                            SignalDirect::IsLong => {
                                // 做多：High - Range * ratio (容忍从高点回撤一定比例)
                                let sl =
                                    last_data_item.h - range * large_entity_cfg.retracement_ratio;
                                // 确保止损不高于入场价(Close) - 保护性
                                large_entity_retracement_sl = Some(sl.min(last_data_item.c));
                            }
                            SignalDirect::IsShort => {
                                // 做空：Low + Range * ratio (容忍从低点反弹一定比例)
                                let sl =
                                    last_data_item.l + range * large_entity_cfg.retracement_ratio;
                                // 确保止损不低于入场价(Close) - 保护性
                                large_entity_retracement_sl = Some(sl.max(last_data_item.c));
                            }
                        }
                    }
                }
            }

            match signal_direction {
                SignalDirect::IsLong => {
                    signal_result.should_buy = Some(true);
                    signal_result.direction = rust_quant_domain::SignalDirection::Long;
                    // 做多止损: 入场价 - ATR * multiplier
                    if atr_value > 0.0 {
                        signal_result.atr_stop_loss_price =
                            Some(last_data_item.c - atr_value * atr_multiplier);
                    }

                    // Fib 回撤入场：优先写入 swing 止损（可配置）
                    if fib_cfg.is_open
                        && fib_cfg.use_swing_stop_loss
                        && vegas_indicator_signal_values
                            .fib_retracement_value
                            .is_long_signal
                        && signal_result.signal_kline_stop_loss_price.is_none()
                    {
                        let sl = vegas_indicator_signal_values
                            .fib_retracement_value
                            .suggested_stop_loss;
                        if sl > 0.0 {
                            signal_result.signal_kline_stop_loss_price =
                                Some(sl.min(last_data_item.c));
                            signal_result.stop_loss_source = Some("FibRetracement".to_string());
                        }
                    }

                    // 【成交量确认形态止损】只在成交量放大时启用形态止损
                    let volume_confirmed =
                        vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;

                    // 1. 优先检查大实体止损（强趋势保护）
                    // 用户规则优化：如果macd是绿柱（histogram > 0），且快线大于慢线（macd > signal），就不启用大实体止损
                    let macd_val = &vegas_indicator_signal_values.macd_value;
                    let macd_strong_bullish =
                        macd_val.histogram > 0.0 && macd_val.macd_line > macd_val.signal_line;
                    let is_repair_long = Self::is_repair_long_candidate(
                        vegas_indicator_signal_values,
                        valid_rsi_value,
                    );

                    if is_repair_long {
                        // 暴跌后的修复 long 更容易被后续信号止损过早打掉，
                        // 用标记交给持仓层忽略后续信号止损更新，保留 ATR/最大亏损止损。
                        signal_result.signal_kline_stop_loss_price = None;
                        signal_result.stop_loss_source =
                            Some("RepairLong_NoSignalKline".to_string());
                    } else if is_large_entity
                        && large_entity_retracement_sl.is_some()
                        && !macd_strong_bullish
                    {
                        signal_result.signal_kline_stop_loss_price = large_entity_retracement_sl;
                        signal_result.stop_loss_source =
                            Some("LargeEntity_Retracement".to_string());
                    }
                    // 2. 其次检查吞没形态 + 成交量确认
                    else if vegas_indicator_signal_values.engulfing_value.is_engulfing {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Rejected".to_string());
                        }
                    }

                    // 3. 最后检查锤子线形态 + 成交量确认(如果还没有设置止损)
                    if signal_result.signal_kline_stop_loss_price.is_none()
                        && vegas_indicator_signal_values
                            .kline_hammer_value
                            .is_long_signal
                    {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.l);
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Rejected".to_string());
                        }
                    }
                }
                SignalDirect::IsShort => {
                    signal_result.should_sell = Some(true);
                    signal_result.direction = rust_quant_domain::SignalDirection::Short;
                    // 做空止损: 入场价 + ATR * multiplier
                    if atr_value > 0.0 {
                        signal_result.atr_stop_loss_price =
                            Some(last_data_item.c + atr_value * atr_multiplier);
                    }

                    // Fib 回撤入场：优先写入 swing 止损（可配置）
                    if fib_cfg.is_open
                        && fib_cfg.use_swing_stop_loss
                        && vegas_indicator_signal_values
                            .fib_retracement_value
                            .is_short_signal
                        && signal_result.signal_kline_stop_loss_price.is_none()
                    {
                        let sl = vegas_indicator_signal_values
                            .fib_retracement_value
                            .suggested_stop_loss;
                        if sl > 0.0 {
                            signal_result.signal_kline_stop_loss_price =
                                Some(sl.max(last_data_item.c));
                            signal_result.stop_loss_source = Some("FibRetracement".to_string());
                        }
                    }

                    // 【成交量确认形态止损】只在成交量放大时启用形态止损
                    let volume_confirmed =
                        vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;

                    // 1. 优先检查大实体止损（强趋势保护）
                    // if is_large_entity && large_entity_retracement_sl.is_some() {
                    //    signal_result.signal_kline_stop_loss_price = large_entity_retracement_sl;
                    //    signal_result.stop_loss_source =
                    //        Some("LargeEntity_Retracement".to_string());
                    // }
                    // 2. 其次检查吞没形态 + 成交量确认
                    if vegas_indicator_signal_values.engulfing_value.is_engulfing {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Rejected".to_string());
                        }
                    }

                    // 3. 最后检查锤子线形态 + 成交量确认(如果还没有设置止损)
                    if signal_result.signal_kline_stop_loss_price.is_none()
                        && vegas_indicator_signal_values
                            .kline_hammer_value
                            .is_short_signal
                    {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.h);
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Rejected".to_string());
                        }
                    }
                }
            }

            // 信号产生时立即记录指标快照（在过滤逻辑之前）
            // 这样即使信号后续被过滤，filtered_signal_log 也能记录当时的指标状态

            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }

        // ================================================================
        // Fib 严格大趋势过滤：禁开反向仓
        // 只有当 swing 波动幅度足够大时，才应用此过滤，避免窄幅震荡中过度过滤
        // ================================================================
        if fib_cfg.is_open && fib_cfg.strict_major_trend {
            let major_bull =
                trend::is_major_bullish_trend(&vegas_indicator_signal_values.ema_values);
            let major_bear =
                trend::is_major_bearish_trend(&vegas_indicator_signal_values.ema_values);

            // 计算 swing 波动幅度
            let swing_high = vegas_indicator_signal_values
                .fib_retracement_value
                .swing_high;
            let swing_low = vegas_indicator_signal_values
                .fib_retracement_value
                .swing_low;
            let swing_move_pct = if swing_low > 0.0 {
                (swing_high - swing_low) / swing_low
            } else {
                0.0
            };

            // 只有在 swing 数据有效且波动幅度足够大时才应用过滤
            let is_trend_move_significant =
                swing_low > 0.0 && swing_move_pct >= fib_cfg.min_trend_move_pct;

            // 注意：这里仅记录"禁止开仓"的原因，不直接清空 should_buy/should_sell。
            // 这样回测/实盘可以在 backtest/position 层实现"反向信号仅平仓，不反手开仓"的行为。
            if is_trend_move_significant {
                if major_bear && signal_result.should_buy.unwrap_or(false) {
                    signal_result.filter_reasons.push(format!(
                        "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG(swing_pct={:.2}%)",
                        swing_move_pct * 100.0
                    ));
                }
                if major_bull && signal_result.should_sell.unwrap_or(false) {
                    signal_result.filter_reasons.push(format!(
                        "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT(swing_pct={:.2}%)",
                        swing_move_pct * 100.0
                    ));
                }
            }
        }

        // 高波动下跌阶段容易出现"低位追空"，
        // 当空头排列已经显著远离均线且不在 Fib 回撤区间时，直接拦截做空。
        // 但对极少数"放量新腿破位延续"场景保留例外，避免错杀有效突破空单。
        let fib_val = vegas_indicator_signal_values.fib_retracement_value;
        let allow_breakdown_short = vegas_indicator_signal_values.leg_detection_value.is_new_leg
            && fib_val.retracement_ratio <= 0.10
            && fib_val.volume_ratio >= 3.0
            && vegas_indicator_signal_values.macd_value.histogram < 0.0;
        if signal_result.should_sell.unwrap_or(false)
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && ema_distance_filter.state == EmaDistanceState::TooFar
            && !fib_val.in_zone
            && !allow_breakdown_short
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TOO_FAR_OUTSIDE_FIB_ZONE_BLOCK_SHORT".to_string());
        }

        let allow_repair_long = signal_result.should_buy.unwrap_or(false)
            && Self::is_repair_long_candidate(vegas_indicator_signal_values, valid_rsi_value);
        let allow_new_leg_positive_macd_long = signal_result.should_buy.unwrap_or(false)
            && Self::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                vegas_indicator_signal_values,
                valid_rsi_value,
            );

        // TooFar 反趋势做多里，锤子线抄底在空头排列且 Fib 未回到理想区间时表现较差。
        // 这类单常由局部反转信号触发，但仍处于空头主导阶段，优先拦截低 RSI 的接飞刀做多。
        let should_block_counter_trend_hammer_long = signal_result.should_buy.unwrap_or(false)
            && ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !fib_val.in_zone
            && vegas_indicator_signal_values
                .kline_hammer_value
                .is_long_signal
            && valid_rsi_value.is_some_and(|rsi| rsi < 45.0)
            && !allow_repair_long;
        let should_block_counter_trend_hammer_long =
            should_block_counter_trend_hammer_long && !allow_new_leg_positive_macd_long;
        if should_block_counter_trend_hammer_long {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG".to_string());
        }

        let should_block_counter_trend_chase_long = signal_result.should_buy.unwrap_or(false)
            && ema_distance_filter.state == EmaDistanceState::TooFar
            && !vegas_indicator_signal_values.ema_touch_value.is_uptrend
            && !vegas_indicator_signal_values.ema_values.is_long_trend
            && vegas_indicator_signal_values.ema_values.is_short_trend
            && !fib_val.in_zone
            && (vegas_indicator_signal_values
                .engulfing_value
                .is_valid_engulfing
                || vegas_indicator_signal_values.bollinger_value.is_long_signal)
            && valid_rsi_value.is_some_and(|rsi| rsi >= 50.0)
            && fib_val.volume_ratio >= 2.5
            && vegas_indicator_signal_values.macd_value.histogram >= 0.0;
        if should_block_counter_trend_chase_long {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TOO_FAR_COUNTER_TREND_CHASE_LONG".to_string());
        }

        let should_block_weak_ema_trend_entry =
            Self::should_block_weak_ema_trend_entry(&conditions, &fib_val, fib_cfg.is_open);
        if signal_result.should_buy.unwrap_or(false) && should_block_weak_ema_trend_entry {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_LONG".to_string());
        }
        if signal_result.should_sell.unwrap_or(false) && should_block_weak_ema_trend_entry {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_SHORT".to_string());
        }

        let should_block_weak_structure_breakout_long =
            Self::should_block_weak_structure_breakout_long(&conditions, valid_rsi_value);
        if signal_result.should_buy.unwrap_or(false) && should_block_weak_structure_breakout_long {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SIMPLE_BREAK_CHOCH_NO_BOS_LONG".to_string());
        }

        let should_block_conflicting_structure_breakout_short =
            Self::should_block_conflicting_structure_breakout_short(
                &conditions,
                ema_distance_filter.state,
            );
        if signal_result.should_sell.unwrap_or(false)
            && should_block_conflicting_structure_breakout_short
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("SIMPLE_BREAK_BULLISH_STRUCTURE_SHORT".to_string());
        }

        let should_block_shallow_fib_breakdown_short =
            Self::should_block_shallow_fib_breakdown_short(
                &conditions,
                ema_distance_filter.state,
                &fib_val,
            );
        if signal_result.should_sell.unwrap_or(false) && should_block_shallow_fib_breakdown_short {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("SIMPLE_BREAK_TOO_FAR_SHALLOW_FIB_SHORT".to_string());
        }

        let should_block_conflicting_too_far_new_bear_leg_short =
            Self::should_block_conflicting_too_far_new_bear_leg_short(
                &conditions,
                vegas_indicator_signal_values,
            );
        if signal_result.should_sell.unwrap_or(false)
            && should_block_conflicting_too_far_new_bear_leg_short
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("CONFLICTING_TOO_FAR_NEW_BEAR_LEG_SHORT".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_macd_near_zero_weak_hammer_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("MACD_NEAR_ZERO_WEAK_HAMMER_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_too_far_uptrend_opposing_hammer_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("TOO_FAR_UPTREND_OPPOSING_HAMMER_SHORT_BLOCK".to_string());
        }

        // ================================================================
        // 应用EMA距离过滤（仅空头分支）
        // - 过远状态且空头排列：拒绝做空
        // ================================================================
        if ema_distance_filter.should_filter_short && signal_result.should_sell.unwrap_or(false) {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_DISTANCE_FILTER_SHORT".to_string());
        }

        // ================================================================
        // 【追涨/追跌确认K线条件】
        // 当价格远离EMA144时，要求额外的确认条件才能开仓
        // 回测验证: ID 5988, profit +57%, sharpe 1.53→1.89, max_dd 57.7%→55.5%
        // ================================================================
        let chase_cfg = self.chase_confirm_config.unwrap_or_default();
        if chase_cfg.enabled {
            let ema144 = vegas_indicator_signal_values.ema_values.ema2_value;
            if ema144 > 0.0 {
                let price_vs_ema144 = (last_data_item.c - ema144) / ema144;

                // 追涨确认：price > EMA144*(1+threshold) 时要求额外确认
                if price_vs_ema144 > chase_cfg.long_threshold
                    && signal_result.should_buy.unwrap_or(false)
                {
                    let body_ratio = last_data_item.body_ratio();
                    let is_bullish = last_data_item.c > last_data_item.o;

                    // 确认条件（任一满足）
                    let pullback_touch = {
                        let low_vs_ema144 = (last_data_item.l - ema144) / ema144;
                        low_vs_ema144.abs() <= chase_cfg.pullback_touch_threshold
                    };
                    let bullish_close = is_bullish && body_ratio > chase_cfg.min_body_ratio;
                    let has_engulfing = vegas_indicator_signal_values
                        .engulfing_value
                        .is_valid_engulfing;

                    let confirmed = pullback_touch || bullish_close || has_engulfing;
                    if !confirmed {
                        signal_result.should_buy = Some(false);
                        signal_result
                            .filter_reasons
                            .push("CHASE_CONFIRM_FILTER_LONG".to_string());
                    }
                }

                // 追跌确认：price < EMA144*(1-threshold) 时要求额外确认
                if price_vs_ema144 < -chase_cfg.short_threshold
                    && signal_result.should_sell.unwrap_or(false)
                {
                    let body_ratio = last_data_item.body_ratio();
                    let is_bearish = last_data_item.c < last_data_item.o;

                    // 确认条件（任一满足）
                    let bounce_touch = {
                        let high_vs_ema144 = (last_data_item.h - ema144) / ema144;
                        high_vs_ema144.abs() <= chase_cfg.pullback_touch_threshold
                    };
                    let bearish_close = is_bearish && body_ratio > chase_cfg.min_body_ratio;
                    let has_engulfing = vegas_indicator_signal_values
                        .engulfing_value
                        .is_valid_engulfing;

                    let confirmed = bounce_touch || bearish_close || has_engulfing;
                    if !confirmed {
                        signal_result.should_sell = Some(false);
                        signal_result
                            .filter_reasons
                            .push("CHASE_CONFIRM_FILTER_SHORT".to_string());
                    }
                }
            }
        }

        // ================================================================
        // 【新增】极端K线过滤/放行：
        // - 大实体且一次跨越多条EMA时，仅顺势放行；反向信号直接过滤
        // - 方向冲突时撤销信号，避免追入假突破
        // ================================================================
        if let Some(extreme_cfg) = self.extreme_k_filter_signal.as_ref() {
            if extreme_cfg.is_open {
                let body_ratio = last_data_item.body_ratio();
                let body_move_pct =
                    ((last_data_item.c - last_data_item.o).abs()) / last_data_item.o.max(1e-9);
                let cross_count = Self::count_crossed_emas(
                    last_data_item.o,
                    last_data_item.c,
                    &vegas_indicator_signal_values.ema_values,
                );

                let is_extreme = body_ratio >= extreme_cfg.min_body_ratio
                    && body_move_pct >= extreme_cfg.min_move_pct
                    && cross_count >= extreme_cfg.min_cross_ema_count;

                if is_extreme {
                    let is_bull = last_data_item.c > last_data_item.o;
                    let is_bear = last_data_item.c < last_data_item.o;

                    if is_bull && signal_result.should_sell.unwrap_or(false) {
                        signal_result.should_sell = Some(false);
                        signal_result
                            .filter_reasons
                            .push("EXTREME_K_FILTER_CONFLICT_SHORT".to_string());
                    }
                    if is_bear && signal_result.should_buy.unwrap_or(false) {
                        signal_result.should_buy = Some(false);
                        signal_result
                            .filter_reasons
                            .push("EXTREME_K_FILTER_CONFLICT_LONG".to_string());
                    }

                    // 仅顺势放行，逆势则拦截
                    if signal_result.should_buy.unwrap_or(false) {
                        // 如果是大趋势多头且极端K线也是多头，则放行（忽略小趋势）
                        let allow_by_major = trend::is_major_bullish_trend(
                            &vegas_indicator_signal_values.ema_values,
                        ) && is_bull;

                        if !allow_by_major {
                            // 否则必须满足小趋势多头
                            if !trend::is_bullish_trend(&vegas_indicator_signal_values.ema_values) {
                                signal_result.should_buy = Some(false);
                                signal_result
                                    .filter_reasons
                                    .push("EXTREME_K_FILTER_TREND_LONG".to_string());
                            }
                        }
                    }

                    if signal_result.should_sell.unwrap_or(false) {
                        // 如果是大趋势空头且极端K线也是空头，则放行（忽略小趋势）
                        let allow_by_major = trend::is_major_bearish_trend(
                            &vegas_indicator_signal_values.ema_values,
                        ) && is_bear;

                        if !allow_by_major {
                            // 否则必须满足小趋势空头
                            if !trend::is_bearish_trend(&vegas_indicator_signal_values.ema_values) {
                                signal_result.should_sell = Some(false);
                                signal_result
                                    .filter_reasons
                                    .push("EXTREME_K_FILTER_TREND_SHORT".to_string());
                            }
                        }
                    }
                }
            }
        }

        // ================================================================
        // 震荡过滤：震荡时降低止盈目标（不影响开仓，只影响 TP）
        // 震荡区间: RSI 中性 + 缩量或 MACD 近零轴 -> 1:1 止盈
        // ================================================================
        if let Some(range_filter_signal) = &self.range_filter_signal {
            if range_filter_signal.is_open && self.bolling_signal.is_some() {
                let bb_value = &vegas_indicator_signal_values.bollinger_value;
                let mid = bb_value.middle;
                let width = bb_value.upper - bb_value.lower;
                if mid > 0.0 && width > 0.0 {
                    let bb_width_ratio = width / mid;
                    if bb_width_ratio <= range_filter_signal.bb_width_threshold {
                        let k_range = (last_data_item.h - last_data_item.l)
                            .abs()
                            .max(last_data_item.c * 0.001);
                        let tp_ratio = range_filter_signal.tp_kline_ratio.max(0.0);
                        let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
                        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
                        let rsi_in_range = valid_rsi_value
                            .map(|rsi| (46.0..=54.0).contains(&rsi))
                            .unwrap_or(false);
                        let macd_near_zero = self.macd_signal.as_ref().is_some_and(|macd_cfg| {
                            if !macd_cfg.is_open {
                                return false;
                            }
                            let macd_val = &vegas_indicator_signal_values.macd_value;
                            macd_val.macd_line.abs() <= entry_price * 0.001
                        });
                        let is_ultra_narrow =
                            bb_width_ratio <= range_filter_signal.bb_width_threshold * 0.85;
                        let is_indecision = last_data_item.is_small_body_and_big_up_down_shadow();
                        let use_one_to_one = rsi_in_range
                            && (volume_ratio < 1.05 || macd_near_zero || is_indecision)
                            && is_ultra_narrow;
                        range_snapshot = Some(json!({
                            "enabled": true,
                            "bb_width_ratio": bb_width_ratio,
                            "bb_width_threshold": range_filter_signal.bb_width_threshold,
                            "tp_ratio": tp_ratio,
                            "use_one_to_one": use_one_to_one,
                            "volume_ratio": volume_ratio,
                            "rsi": valid_rsi_value,
                            "macd_near_zero": macd_near_zero,
                            "is_indecision": is_indecision,
                        }));
                        if use_one_to_one {
                            dynamic_adjustments.push("RANGE_TP_ONE_TO_ONE".to_string());
                        } else {
                            dynamic_adjustments.push("RANGE_TP_RATIO".to_string());
                        }

                        let take_profit_diff = if use_one_to_one {
                            let stop_price = signal_result
                                .signal_kline_stop_loss_price
                                .or(signal_result.atr_stop_loss_price);
                            let diff = stop_price
                                .map(|price| (entry_price - price).abs())
                                .unwrap_or(0.0);
                            if diff > 0.0 {
                                diff
                            } else {
                                k_range * tp_ratio
                            }
                        } else {
                            k_range * tp_ratio
                        };

                        if signal_result.should_buy.unwrap_or(false) {
                            signal_result.long_signal_take_profit_price =
                                Some(entry_price + take_profit_diff);
                        }
                        if signal_result.should_sell.unwrap_or(false) {
                            signal_result.short_signal_take_profit_price =
                                Some(entry_price - take_profit_diff);
                        }
                    }
                }
            }
        }

        // ================================================================
        // 【新增】MACD 动量反转过滤 (Momentum Turn Filter)
        // 核心逻辑：允许 MACD 反向入场（抄底/摸顶），但要求动量必须改善（拐点已现）
        // 1. 如果 MACD 与交易方向一致 -> 放行（顺势）
        // 2. 如果 MACD 与交易方向相反（逆势）：
        //    - 柱状图继续恶化（接飞刀） -> 过滤
        //    - 柱状图开始改善（企稳） -> 放行
        // ================================================================
        if let Some(macd_cfg) = &self.macd_signal {
            if macd_cfg.is_open {
                let macd_val = &vegas_indicator_signal_values.macd_value;

                // 做多过滤
                if signal_result.should_buy.unwrap_or(false) {
                    let mut should_filter = false;
                    let rebound_protect_long = Self::is_rebound_protect_long_candidate(
                        data_items,
                        vegas_indicator_signal_values,
                    );

                    if macd_cfg.filter_falling_knife {
                        // 如果 MACD 柱状图为负（处于空头动量区）
                        if macd_val.histogram < 0.0 {
                            // 且 柱状图在递减（负值变更大，动量加速向下）
                            if macd_val.histogram_decreasing {
                                should_filter = true; // 正在接飞刀，过滤
                                if rebound_protect_long {
                                    signal_result
                                        .filter_reasons
                                        .push("REBOUND_HAMMER_LONG_PROTECT".to_string());
                                }
                                signal_result
                                    .filter_reasons
                                    .push("MACD_FALLING_KNIFE_LONG".to_string());
                            }
                        }
                    }

                    if should_filter {
                        signal_result.should_buy = Some(false);
                    }
                }

                // 做空过滤
                if signal_result.should_sell.unwrap_or(false) {
                    let mut should_filter = false;

                    if macd_cfg.filter_falling_knife {
                        // 如果 MACD 柱状图为正（处于多头动量区）
                        if macd_val.histogram > 0.0 {
                            // 且 柱状图在递增（正值变更大，动量加速向上）
                            if macd_val.histogram_increasing {
                                should_filter = true; // 正在逆势摸顶（涨势未尽），过滤
                                signal_result
                                    .filter_reasons
                                    .push("MACD_FALLING_KNIFE_SHORT".to_string());
                            }
                        }
                    }

                    if should_filter {
                        signal_result.should_sell = Some(false);
                    }
                }
            }
        }

        // 缩量 + RSI 中性 + MACD 零轴下方修复时，避免过早反手做空。
        // 典型场景是大跌后的修复反抽，趋势仍偏空，但动量和参与度都不支持立即追空。
        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_low_volume_neutral_rsi_macd_recovery_short(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
                valid_rsi_value,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_BLOCK_SHORT".to_string());
        }

        // 极端低位放量砸盘时，避免在旧空头腿末端继续追空。
        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_exhaustion_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EXHAUSTION_SHORT_NEAR_SWING_LOW_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_bullish_leg_mean_reversion_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("BULLISH_LEG_MEAN_REVERSION_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_deep_negative_macd_recovery_short(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_stc_early_weakening_short(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("STC_EARLY_WEAKENING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_weakening_no_structure_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("WEAKENING_NO_STRUCTURE_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_deep_negative_weak_breakdown_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_WEAK_BREAKDOWN_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_shallow_weakening_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_SHALLOW_WEAKENING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_panic_breakdown_short(data_items, vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("PANIC_BREAKDOWN_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_no_trend_hanging_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_NO_TREND_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_below_zero_weakening_hanging_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_no_trend_too_far_hanging_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_above_zero_low_volume_no_trend_hanging_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_long_trend_pullback_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_PULLBACK_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_long_trend_above_zero_low_volume_weakening_short(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_long_trend_above_zero_high_rsi_early_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_ABOVE_ZERO_HIGH_RSI_EARLY_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_high_volume_no_trend_bollinger_long_short(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK".to_string());
        }

        if signal_result.should_sell.unwrap_or(false)
            && Self::should_block_high_volume_ranging_recovery_short(vegas_indicator_signal_values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_RANGING_RECOVERY_SHORT_BLOCK".to_string());
        }

        // 缩量 + RSI 中性 + MACD 零轴上方转弱时，避免过早逆势做多。
        // 典型场景是上涨后的回落修复，参与度不足且死叉刚开始，不适合抢多。
        if signal_result.should_buy.unwrap_or(false) {
            let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
            let macd_val = &vegas_indicator_signal_values.macd_value;
            let rsi_is_neutral = valid_rsi_value
                .map(|rsi| (47.0..=53.0).contains(&rsi))
                .unwrap_or(false);
            let macd_weakening_above_zero = macd_val.macd_line > 0.0
                && macd_val.signal_line > 0.0
                && macd_val.macd_line < macd_val.signal_line
                && macd_val.histogram < 0.0;

            if volume_ratio < 1.0 && rsi_is_neutral && macd_weakening_above_zero {
                signal_result.should_buy = Some(false);
                signal_result
                    .filter_reasons
                    .push("LOW_VOLUME_NEUTRAL_RSI_MACD_WEAKENING_BLOCK_LONG".to_string());
            }
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_deep_negative_hammer_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_HAMMER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_recent_upper_shadow_pressure_long(
                data_items,
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("RECENT_UPPER_SHADOW_PRESSURE_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_weak_breakout_no_trend_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("WEAK_BREAKOUT_NO_TREND_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_ranging_no_trend_weak_hammer_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("RANGING_NO_TREND_WEAK_HAMMER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_too_far_bollinger_short_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_conflicting_bollinger_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_internal_down_counter_trend_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_volume_high_rsi_bollinger_short_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_deep_negative_no_trend_hammer_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("DEEP_NEGATIVE_NO_TREND_HAMMER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_short_trend_too_far_bollinger_short_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_TREND_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_short_trend_new_bull_leg_counter_long(
                vegas_indicator_signal_values,
                signal_result.open_price.unwrap_or(last_data_item.c),
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_short_trend_no_bollinger_rebound_long(
                vegas_indicator_signal_values,
            )
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_normal_bull_leg_no_confirm_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_above_zero_no_trend_engulfing_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_long_trend_below_zero_fib_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("LONG_TREND_BELOW_ZERO_FIB_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_high_level_sideways_chase_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("HIGH_LEVEL_SIDEWAYS_CHASE_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_block_above_zero_high_level_chase_long(vegas_indicator_signal_values)
        {
            signal_result.should_buy = Some(false);
            signal_result
                .filter_reasons
                .push("ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_BLOCK".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_protect_deep_negative_hammer_long(vegas_indicator_signal_values)
        {
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
            let protective_stop = last_data_item.l.max(entry_price * 0.98);
            signal_result.signal_kline_stop_loss_price = Some(
                signal_result
                    .signal_kline_stop_loss_price
                    .map(|existing| existing.max(protective_stop))
                    .unwrap_or(protective_stop),
            );
            signal_result.stop_loss_source = Some("DeepNegativeHammer_Long_Protect".to_string());
            dynamic_adjustments.push("DEEP_NEGATIVE_HAMMER_LONG_PROTECT".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_protect_long_trend_deep_negative_hammer_long(
                vegas_indicator_signal_values,
            )
        {
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
            let protective_stop = last_data_item.l.max(entry_price * 0.975);
            signal_result.signal_kline_stop_loss_price = Some(
                signal_result
                    .signal_kline_stop_loss_price
                    .map(|existing| existing.max(protective_stop))
                    .unwrap_or(protective_stop),
            );
            signal_result.stop_loss_source =
                Some("LongTrendDeepNegativeHammer_Protect".to_string());
            dynamic_adjustments.push("LONG_TREND_DEEP_NEGATIVE_HAMMER_PROTECT".to_string());
        }

        if signal_result.should_buy.unwrap_or(false)
            && Self::should_protect_above_zero_high_level_chase_long(vegas_indicator_signal_values)
        {
            let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
            let protective_stop = last_data_item.l.max(entry_price * 0.985);
            signal_result.signal_kline_stop_loss_price = Some(
                signal_result
                    .signal_kline_stop_loss_price
                    .map(|existing| existing.max(protective_stop))
                    .unwrap_or(protective_stop),
            );
            signal_result.stop_loss_source =
                Some("AboveZeroHighLevelChaseLong_Protect".to_string());
            dynamic_adjustments.push("ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_PROTECT".to_string());
        }

        if signal_result.signal_kline_stop_loss_price.is_some() {
            dynamic_adjustments.push("STOP_LOSS_SIGNAL_KLINE".to_string());
        }
        if signal_result.atr_stop_loss_price.is_some() {
            dynamic_adjustments.push("STOP_LOSS_ATR".to_string());
        }
        if signal_result.long_signal_take_profit_price.is_some() {
            dynamic_adjustments.push("TP_DYNAMIC_LONG".to_string());
        }
        if signal_result.short_signal_take_profit_price.is_some() {
            dynamic_adjustments.push("TP_DYNAMIC_SHORT".to_string());
        }
        signal_result.dynamic_adjustments = dynamic_adjustments.clone();
        signal_result.dynamic_config_snapshot = Some(
            json!({
                "kline_ts": last_data_item.ts,
                "adjustments": dynamic_adjustments,
                "range_tp": range_snapshot,
                "stop_loss": {
                    "signal_kline": signal_result.signal_kline_stop_loss_price,
                    "atr": signal_result.atr_stop_loss_price,
                    "source": signal_result.stop_loss_source.clone(),
                },
                "take_profit": {
                    "long": signal_result.long_signal_take_profit_price,
                    "short": signal_result.short_signal_take_profit_price,
                    "atr_ratio": signal_result.atr_take_profit_ratio_price
                }
            })
            .to_string(),
        );

        // 可选：添加详细信息到结果中
        if self.emit_debug
            && (signal_result.should_buy.unwrap_or(false)
                || signal_result.should_sell.unwrap_or(false))
        {
            //如果有使用信号k线止损
            if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
                self.calculate_best_stop_loss_price(
                    last_data_item,
                    &mut signal_result,
                    &conditions,
                    vegas_indicator_signal_values,
                );

                if signal_result.direction == rust_quant_domain::SignalDirection::Short
                    && matches!(
                        signal_result.stop_loss_source.as_deref(),
                        Some("Engulfing_Volume_Confirmed") | Some("KlineHammer_Volume_Confirmed")
                    )
                {
                    if let Some(current_stop) = signal_result.signal_kline_stop_loss_price {
                        let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
                        if let Some(tightened_stop) = Self::tighten_short_signal_stop_near_zero_macd(
                            entry_price,
                            current_stop,
                            &vegas_indicator_signal_values.macd_value,
                        ) {
                            signal_result.signal_kline_stop_loss_price = Some(tightened_stop);
                            signal_result
                                .dynamic_adjustments
                                .push("MACD_NEAR_ZERO_TIGHTEN_SHORT_STOP".to_string());
                        }
                    }
                }
            }
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }

        signal_result
    }

    /// 获取指标组合
    pub fn get_indicator_combine(&self) -> IndicatorCombine {
        use crate::ema_indicator::EmaIndicator;
        use crate::leg_detection_indicator::LegDetectionIndicator;
        use crate::market_structure_indicator::MarketStructureIndicator;
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

        // 添加腿部识别（可选）
        if let Some(leg_detection_signal) = &self.leg_detection_signal {
            if leg_detection_signal.is_open {
                indicator_combine.leg_detection_indicator =
                    Some(LegDetectionIndicator::new(leg_detection_signal.size));
            }
        }

        // 添加市场结构（可选）
        if let Some(market_structure_signal) = &self.market_structure_signal {
            if market_structure_signal.is_open {
                indicator_combine.market_structure_indicator =
                    Some(MarketStructureIndicator::new_with_thresholds(
                        market_structure_signal.swing_length,
                        market_structure_signal.internal_length,
                        market_structure_signal.swing_threshold,
                        market_structure_signal.internal_threshold,
                    ));
            }
        }

        indicator_combine
    }

    /// 运行回测
    ///
    /// 注意：此方法不能在 indicators 包中完整实现，因为 BacktestResult 在不同包中定义不同
    /// 实际回测逻辑应在 strategies 或 orchestration 包中调用，使用 get_indicator_combine() 和 get_trade_signal()
    pub fn run_test(
        &mut self,
        _candles: &[CandleItem],
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
        false
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

    /// 统计极端K线一次跨越的EMA条数（开盘价与收盘价之间包含的EMA数量）
    fn count_crossed_emas(open: f64, close: f64, ema_values: &EmaSignalValue) -> usize {
        let (low, high) = if open < close {
            (open, close)
        } else {
            (close, open)
        };
        let emas = [
            ema_values.ema1_value,
            ema_values.ema2_value,
            ema_values.ema3_value,
            ema_values.ema4_value,
            ema_values.ema5_value,
        ];
        emas.iter()
            .filter(|ema| **ema >= low && **ema <= high)
            .count()
    }

    fn has_signal_type(conditions: &[(SignalType, SignalCondition)], target: SignalType) -> bool {
        conditions
            .iter()
            .any(|(signal_type, _)| *signal_type == target)
    }

    fn should_block_weak_ema_trend_entry(
        conditions: &[(SignalType, SignalCondition)],
        fib_value: &FibRetracementSignalValue,
        fib_enabled: bool,
    ) -> bool {
        fib_enabled
            && fib_value.swing_high > 0.0
            && fib_value.swing_low > 0.0
            && fib_value.retracement_ratio <= 0.5
            && Self::has_signal_type(conditions, SignalType::EmaTrend)
            && !Self::has_signal_type(conditions, SignalType::Engulfing)
            && !Self::has_signal_type(conditions, SignalType::KlineHammer)
    }

    fn should_block_weak_structure_breakout_long(
        conditions: &[(SignalType, SignalCondition)],
        valid_rsi_value: Option<f64>,
    ) -> bool {
        let Some(rsi) = valid_rsi_value else {
            return false;
        };

        if rsi >= 60.0
            || !Self::has_signal_type(conditions, SignalType::SimpleBreakEma2through)
            || !Self::has_signal_type(conditions, SignalType::LegDetection)
            || !Self::has_signal_type(conditions, SignalType::MarketStructure)
            || Self::has_signal_type(conditions, SignalType::EmaTrend)
        {
            return false;
        }

        conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::MarketStructure
                && matches!(
                    condition,
                    SignalCondition::MarketStructure {
                        is_bullish_bos: false,
                        is_bullish_choch: true,
                        ..
                    }
                )
        })
    }

    fn should_block_conflicting_structure_breakout_short(
        conditions: &[(SignalType, SignalCondition)],
        ema_distance_state: EmaDistanceState,
    ) -> bool {
        if ema_distance_state != EmaDistanceState::TooFar
            || !Self::has_signal_type(conditions, SignalType::SimpleBreakEma2through)
            || !Self::has_signal_type(conditions, SignalType::LegDetection)
            || !Self::has_signal_type(conditions, SignalType::MarketStructure)
            || Self::has_signal_type(conditions, SignalType::EmaTrend)
        {
            return false;
        }

        let has_upside_breakout = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::SimpleBreakEma2through
                && matches!(
                    condition,
                    SignalCondition::PriceBreakout {
                        price_above: true,
                        price_below: false,
                    }
                )
        });

        let has_bullish_structure = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::MarketStructure
                && matches!(
                    condition,
                    SignalCondition::MarketStructure {
                        is_bullish_bos: true,
                        ..
                    } | SignalCondition::MarketStructure {
                        is_bullish_choch: true,
                        ..
                    }
                )
        });

        has_upside_breakout && has_bullish_structure
    }

    fn should_block_shallow_fib_breakdown_short(
        conditions: &[(SignalType, SignalCondition)],
        ema_distance_state: EmaDistanceState,
        fib_value: &FibRetracementSignalValue,
    ) -> bool {
        if ema_distance_state != EmaDistanceState::TooFar
            || fib_value.in_zone
            || fib_value.retracement_ratio > 0.3
            || !Self::has_signal_type(conditions, SignalType::SimpleBreakEma2through)
            || !Self::has_signal_type(conditions, SignalType::LegDetection)
            || !Self::has_signal_type(conditions, SignalType::MarketStructure)
            || Self::has_signal_type(conditions, SignalType::EmaTrend)
        {
            return false;
        }

        let has_downside_breakout = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::SimpleBreakEma2through
                && matches!(
                    condition,
                    SignalCondition::PriceBreakout {
                        price_above: false,
                        price_below: true,
                    }
                )
        });

        let has_bearish_structure = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::MarketStructure
                && matches!(
                    condition,
                    SignalCondition::MarketStructure {
                        is_bearish_bos: true,
                        ..
                    } | SignalCondition::MarketStructure {
                        is_bearish_choch: true,
                        ..
                    }
                )
        });

        has_downside_breakout && has_bearish_structure
    }

    fn should_block_conflicting_too_far_new_bear_leg_short(
        conditions: &[(SignalType, SignalCondition)],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) -> bool {
        if vegas_indicator_signal_values.ema_distance_filter.state != EmaDistanceState::TooFar
            || !vegas_indicator_signal_values.fib_retracement_value.in_zone
            || vegas_indicator_signal_values.volume_value.volume_ratio >= 1.5
        {
            return false;
        }

        let has_bolling_long = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::Bolling
                && matches!(
                    condition,
                    SignalCondition::Bolling {
                        is_long_signal: true,
                        is_short_signal: false,
                        ..
                    }
                )
        });

        let has_engulfing_short = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::Engulfing
                && matches!(
                    condition,
                    SignalCondition::Engulfing {
                        is_long_signal: false,
                        is_short_signal: true,
                    }
                )
        });

        let has_new_bearish_leg = conditions.iter().any(|(signal_type, condition)| {
            *signal_type == SignalType::LegDetection
                && matches!(
                    condition,
                    SignalCondition::LegDetection {
                        is_bullish_leg: false,
                        is_bearish_leg: true,
                        is_new_leg: true,
                    }
                )
        });

        has_bolling_long && has_engulfing_short && has_new_bearish_leg
    }

    fn tighten_short_signal_stop_near_zero_macd(
        entry_price: f64,
        current_stop: f64,
        macd_value: &MacdSignalValue,
    ) -> Option<f64> {
        if current_stop <= entry_price || macd_value.histogram.abs() >= 2.0 {
            return None;
        }

        Some(entry_price + (current_stop - entry_price) * 0.5)
    }

    fn calculate_best_stop_loss_price(
        &self,
        last_data_item: &CandleItem,
        signal_result: &mut SignalResult,
        conditions: &[(SignalType, SignalCondition)],
        vegas_indicator_signal_values: &VegasIndicatorSignalValue,
    ) {
        // 检查是否有吞没形态信号
        let has_engulfing_signal = Self::has_signal_type(conditions, SignalType::Engulfing);
        let disable_long_engulfing_stop_raise =
            std::env::var("VEGAS_DISABLE_LONG_ENGULFING_STOP_RAISE")
                .ok()
                .as_deref()
                == Some("1");
        let disable_conflicting_long_engulfing_stop_raise =
            std::env::var("VEGAS_DISABLE_CONFLICTING_LONG_ENGULFING_STOP_RAISE")
                .ok()
                .as_deref()
                == Some("1");
        let conflicting_long_engulfing_stop_raise = disable_conflicting_long_engulfing_stop_raise
            && signal_result.direction == rust_quant_domain::SignalDirection::Long
            && !vegas_indicator_signal_values.fib_retracement_value.in_zone
            && vegas_indicator_signal_values
                .bollinger_value
                .is_short_signal
            && vegas_indicator_signal_values.ema_distance_filter.state == EmaDistanceState::TooFar;

        // 如果是吞没形态信号，使用开盘价作为止损价格
        if has_engulfing_signal
            && !(disable_long_engulfing_stop_raise
                && signal_result.direction == rust_quant_domain::SignalDirection::Long)
            && !conflicting_long_engulfing_stop_raise
        {
            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o());
        }

        // 【已禁用】只保留吞没形态止损，其他情况不设置信号线止损
        // if let Some(stop_loss_price) = utils::calculate_best_stop_loss_price(
        //     last_data_item,
        //     signal_result.should_buy.unwrap_or(false),
        //     signal_result.should_sell.unwrap_or(false),
        // ) {
        //     signal_result.signal_kline_stop_loss_price = Some(stop_loss_price);
        // }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ema_filter::EmaDistanceFilter;
    use super::super::signal::{
        BollingerSignalValue, EmaTouchTrendSignalValue, EngulfingSignalValue,
        KlineHammerSignalValue, MacdSignalValue, RsiSignalValue, VolumeTrendSignalValue,
    };
    use super::{
        EmaDistanceState, EmaSignalValue, FibRetracementSignalConfig, FibRetracementSignalValue,
        RsiSignalConfig, SignalCondition, SignalType, SignalWeightsConfig,
        VegasIndicatorSignalValue, VegasStrategy, VolumeSignalConfig,
    };
    use crate::leg_detection_indicator::LegDetectionValue;
    use rust_quant_common::CandleItem;
    use rust_quant_domain::BasicRiskStrategyConfig;

    fn candle(o: f64, h: f64, l: f64, c: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            ts,
            v: 1.0,
            confirm: 1,
        }
    }

    #[test]
    fn fib_strict_reason_includes_swing_pct_suffix() {
        let mut strategy = VegasStrategy {
            period: "4H".to_string(),
            volume_signal: Some(VolumeSignalConfig {
                volume_bar_num: 4,
                volume_increase_ratio: 2.0,
                volume_decrease_ratio: 2.0,
                is_open: true,
            }),
            rsi_signal: Some(RsiSignalConfig {
                rsi_length: 14,
                rsi_oversold: 15.0,
                rsi_overbought: 85.0,
                is_open: true,
            }),
            fib_retracement_signal: Some(FibRetracementSignalConfig {
                is_open: true,
                only_on_fib: false,
                swing_lookback: 5,
                fib_trigger_low: 0.328,
                fib_trigger_high: 0.618,
                min_volume_ratio: 10.0,
                require_leg_confirmation: false,
                strict_major_trend: true,
                stop_loss_buffer_ratio: 0.01,
                use_swing_stop_loss: false,
                min_trend_move_pct: 0.1,
            }),
            ..VegasStrategy::default()
        };

        let candles = vec![
            candle(10.0, 10.0, 9.0, 9.5, 1),
            candle(9.5, 9.7, 8.5, 9.0, 2),
            candle(9.0, 9.2, 8.0, 8.4, 3),
            candle(8.4, 8.8, 8.2, 8.6, 4),
            candle(8.6, 9.0, 8.4, 8.8, 5),
        ];

        let mut indicator_values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                ema1_value: 90.0,
                ema2_value: 95.0,
                ema3_value: 96.0,
                ema4_value: 100.0,
                ..EmaSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        indicator_values.volume_value.volume_ratio = 3.0;
        indicator_values.rsi_value.rsi_value = 10.0;

        let weights = SignalWeightsConfig {
            weights: vec![(SignalType::VolumeTrend, 1.0), (SignalType::Rsi, 1.0)],
            min_total_weight: 2.0,
        };

        let result = strategy.get_trade_signal(
            &candles,
            &mut indicator_values,
            &weights,
            &BasicRiskStrategyConfig::default(),
        );

        let reason = result
            .filter_reasons
            .iter()
            .find(|r| r.starts_with("FIB_STRICT_MAJOR_BEAR_BLOCK_LONG"))
            .expect("expected fib strict reason");
        assert!(
            reason.contains("swing_pct="),
            "reason should include swing_pct suffix, got: {}",
            reason
        );
    }

    #[test]
    fn deep_negative_hammer_long_candidate_helper_matches_expected_shape() {
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                is_long_signal: true,
                ..BollingerSignalValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                ..KlineHammerSignalValue::default()
            },
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.4,
                ..VolumeTrendSignalValue::default()
            },
            rsi_value: RsiSignalValue {
                rsi_value: 39.0,
                ..RsiSignalValue::default()
            },
            macd_value: MacdSignalValue {
                macd_line: -31.0,
                signal_line: -11.0,
                histogram: -21.0,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(VegasStrategy::is_deep_negative_hammer_long_candidate(
            &values
        ));
    }

    #[test]
    fn repair_long_candidate_helper_matches_expected_shape() {
        let values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.6,
                ..VolumeTrendSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: -1.0,
                histogram_increasing: true,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(VegasStrategy::is_repair_long_candidate(&values, Some(44.0)));
        assert!(!VegasStrategy::is_repair_long_candidate(
            &values,
            Some(46.0)
        ));
    }

    #[test]
    fn counter_trend_hammer_long_new_leg_positive_macd_candidate_matches_expected_shape() {
        let values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                body_ratio: 0.16,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 2.86,
                ..VolumeTrendSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 1.95,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &values,
                Some(36.0)
            )
        );
    }

    #[test]
    fn counter_trend_hammer_long_new_leg_positive_macd_candidate_requires_non_negative_histogram() {
        let values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                ..KlineHammerSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: -0.1,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &values,
                Some(36.0)
            )
        );
    }

    #[test]
    fn counter_trend_hammer_long_new_leg_positive_macd_candidate_rejects_extreme_histogram_or_weak_body(
    ) {
        let extreme_hist_values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                body_ratio: 0.18,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 2.0,
                ..VolumeTrendSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 12.0,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &extreme_hist_values,
                Some(36.0)
            )
        );

        let weak_body_values = VegasIndicatorSignalValue {
            kline_hammer_value: KlineHammerSignalValue {
                body_ratio: 0.08,
                ..extreme_hist_values.kline_hammer_value
            },
            macd_value: MacdSignalValue {
                histogram: 1.5,
                ..MacdSignalValue::default()
            },
            ..extreme_hist_values
        };

        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &weak_body_values,
                Some(36.0)
            )
        );

        let extreme_volume_values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                body_ratio: 0.18,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 4.2,
                ..VolumeTrendSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 1.5,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &extreme_volume_values,
                Some(36.0)
            )
        );
    }

    #[test]
    fn weak_ema_trend_entry_without_pattern_below_fib_midline_should_be_blocked() {
        let conditions = vec![
            (
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: true,
                    ratio: 2.8,
                },
            ),
            (
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: true,
                    is_short_signal: false,
                },
            ),
            (
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: 48.0,
                    oversold: 15.0,
                    overbought: 85.0,
                    is_valid: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: true,
                    is_bearish_leg: false,
                    is_new_leg: false,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            is_long_signal: true,
            in_zone: true,
            retracement_ratio: 0.49,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };

        assert!(VegasStrategy::should_block_weak_ema_trend_entry(
            &conditions,
            &fib_value,
            true,
        ));
    }

    #[test]
    fn weak_ema_trend_entry_with_engulfing_confirmation_should_stay_allowed() {
        let conditions = vec![
            (
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: true,
                    is_short_signal: false,
                },
            ),
            (
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: true,
                    is_short_signal: false,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            is_long_signal: true,
            in_zone: true,
            retracement_ratio: 0.42,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };

        assert!(!VegasStrategy::should_block_weak_ema_trend_entry(
            &conditions,
            &fib_value,
            true,
        ));
    }

    #[test]
    fn weak_structure_breakout_long_without_bos_should_be_blocked() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: 55.0,
                    oversold: 15.0,
                    overbought: 85.0,
                    is_valid: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: true,
                    is_bearish_leg: false,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: false,
                    is_bullish_choch: true,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];

        assert!(VegasStrategy::should_block_weak_structure_breakout_long(
            &conditions,
            Some(55.0),
        ));
    }

    #[test]
    fn weak_structure_breakout_long_with_bos_should_stay_allowed() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: true,
                    is_bearish_leg: false,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: true,
                    is_bearish_bos: false,
                    is_bullish_choch: true,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];

        assert!(!VegasStrategy::should_block_weak_structure_breakout_long(
            &conditions,
            Some(58.0),
        ));
    }

    #[test]
    fn conflicting_bullish_structure_short_should_be_blocked_when_too_far() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: false,
                    is_bullish_choch: true,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];

        assert!(
            VegasStrategy::should_block_conflicting_structure_breakout_short(
                &conditions,
                EmaDistanceState::TooFar,
            )
        );
    }

    #[test]
    fn conflicting_bullish_structure_short_should_stay_allowed_when_not_too_far() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: true,
                    is_bearish_bos: false,
                    is_bullish_choch: false,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];

        assert!(
            !VegasStrategy::should_block_conflicting_structure_breakout_short(
                &conditions,
                EmaDistanceState::Normal,
            )
        );
    }

    #[test]
    fn shallow_fib_breakdown_short_should_be_blocked_when_too_far() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: false,
                    price_below: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: true,
                    is_bullish_choch: false,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            in_zone: false,
            retracement_ratio: 0.26,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };

        assert!(VegasStrategy::should_block_shallow_fib_breakdown_short(
            &conditions,
            EmaDistanceState::TooFar,
            &fib_value,
        ));
    }

    #[test]
    fn shallow_fib_breakdown_short_should_stay_allowed_in_fib_zone() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: false,
                    price_below: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: false,
                    is_bullish_choch: false,
                    is_bearish_choch: true,
                    is_internal: true,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            in_zone: true,
            retracement_ratio: 0.26,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };

        assert!(!VegasStrategy::should_block_shallow_fib_breakdown_short(
            &conditions,
            EmaDistanceState::TooFar,
            &fib_value,
        ));
    }

    #[test]
    fn conflicting_too_far_new_bear_leg_short_should_be_blocked_with_low_volume() {
        let conditions = vec![
            (
                SignalType::Bolling,
                SignalCondition::Bolling {
                    is_long_signal: true,
                    is_short_signal: false,
                    is_close_signal: false,
                },
            ),
            (
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: false,
                    is_short_signal: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: true,
                },
            ),
        ];
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: true,
                ..FibRetracementSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.06,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            VegasStrategy::should_block_conflicting_too_far_new_bear_leg_short(
                &conditions,
                &signal_values,
            )
        );
    }

    #[test]
    fn conflicting_too_far_new_bear_leg_short_should_stay_allowed_with_high_volume() {
        let conditions = vec![
            (
                SignalType::Bolling,
                SignalCondition::Bolling {
                    is_long_signal: true,
                    is_short_signal: false,
                    is_close_signal: false,
                },
            ),
            (
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: false,
                    is_short_signal: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: true,
                },
            ),
        ];
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: true,
                ..FibRetracementSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 2.3,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            !VegasStrategy::should_block_conflicting_too_far_new_bear_leg_short(
                &conditions,
                &signal_values,
            )
        );
    }

    #[test]
    fn macd_near_zero_short_stop_should_tighten_to_midpoint() {
        let macd = MacdSignalValue {
            histogram: 1.2,
            ..MacdSignalValue::default()
        };

        let tightened =
            VegasStrategy::tighten_short_signal_stop_near_zero_macd(100.0, 110.0, &macd);

        assert_eq!(Some(105.0), tightened);
    }

    #[test]
    fn macd_far_from_zero_short_stop_should_stay_unadjusted() {
        let macd = MacdSignalValue {
            histogram: 3.5,
            ..MacdSignalValue::default()
        };

        let tightened =
            VegasStrategy::tighten_short_signal_stop_near_zero_macd(100.0, 110.0, &macd);

        assert_eq!(None, tightened);
    }

    #[test]
    fn macd_near_zero_weak_hammer_short_should_be_blocked_when_too_far_and_low_volume() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 0.85,
                ..VolumeTrendSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 0.63,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(VegasStrategy::should_block_macd_near_zero_weak_hammer_short(&signal_values));
    }

    #[test]
    fn macd_near_zero_weak_hammer_short_should_stay_allowed_with_higher_volume() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.05,
                ..VolumeTrendSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 0.63,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!VegasStrategy::should_block_macd_near_zero_weak_hammer_short(&signal_values));
    }

    #[test]
    fn too_far_uptrend_opposing_hammer_short_should_be_blocked() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_touch_value: EmaTouchTrendSignalValue {
                is_uptrend: true,
                ..EmaTouchTrendSignalValue::default()
            },
            ema_values: EmaSignalValue {
                is_long_trend: true,
                is_short_trend: false,
                ..EmaSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: false,
                ..FibRetracementSignalValue::default()
            },
            bollinger_value: BollingerSignalValue {
                is_long_signal: false,
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                is_bearish_leg: false,
                is_new_leg: false,
                ..LegDetectionValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: false,
                ..EngulfingSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 2.4,
                ..MacdSignalValue::default()
            },
            rsi_value: RsiSignalValue {
                rsi_value: 62.0,
                ..RsiSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(VegasStrategy::should_block_too_far_uptrend_opposing_hammer_short(&signal_values));
    }

    #[test]
    fn too_far_uptrend_opposing_hammer_short_should_stay_allowed_in_fib_zone() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_touch_value: EmaTouchTrendSignalValue {
                is_uptrend: true,
                ..EmaTouchTrendSignalValue::default()
            },
            ema_values: EmaSignalValue {
                is_long_trend: true,
                is_short_trend: false,
                ..EmaSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: true,
                ..FibRetracementSignalValue::default()
            },
            bollinger_value: BollingerSignalValue {
                is_long_signal: false,
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                is_bearish_leg: false,
                is_new_leg: false,
                ..LegDetectionValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: false,
                ..EngulfingSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 2.4,
                ..MacdSignalValue::default()
            },
            rsi_value: RsiSignalValue {
                rsi_value: 62.0,
                ..RsiSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(
            !VegasStrategy::should_block_too_far_uptrend_opposing_hammer_short(&signal_values,)
        );
    }
}
