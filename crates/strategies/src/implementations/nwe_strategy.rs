// ⭐ 指标组合已移至 indicators 包
// pub mod indicator_combine;  // 已废弃

use core::time;

use rust_quant_domain::entities::candle;
use rust_quant_indicators::KlineHammerIndicator;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use rust_quant_indicators::trend::ema_indicator::EmaIndicator;
use rust_quant_indicators::trend::nwe_indicator::NweIndicator;
use rust_quant_indicators::volatility::ATRStopLoos;
use rust_quant_indicators::volume::VolumeRatioIndicator;
use ta::Next;
// ⭐ 使用新的 indicators::nwe 模块
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::{risk, time_util, CandleItem};
use rust_quant_indicators::trend::nwe::{
    NweIndicatorCombine, NweIndicatorConfig, NweIndicatorValues,
};

/// NWE 策略配置与执行器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NweStrategyConfig {
    pub period: String,

    #[serde(default = "default_stc_fast_length")]
    pub stc_fast_length: usize,
    #[serde(
        default = "default_stc_slow_length",
        alias = "stc_period",
        alias = "rsi_period"
    )]
    pub stc_slow_length: usize,
    #[serde(default = "default_stc_cycle_length")]
    pub stc_cycle_length: usize,
    #[serde(default = "default_stc_d1_length")]
    pub stc_d1_length: usize,
    #[serde(default = "default_stc_d2_length")]
    pub stc_d2_length: usize,

    /// STC 超买阈值（兼容 rsi_overbought 字段）
    #[serde(default = "default_stc_overbought", alias = "stc_overbought")]
    pub stc_overbought: f64,
    /// STC 超卖阈值（兼容 rsi_oversold 字段）
    #[serde(default = "default_stc_oversold", alias = "stc_oversold")]
    pub stc_oversold: f64,

    pub atr_period: usize,
    pub atr_multiplier: f64,

    pub nwe_period: usize,
    pub nwe_multi: f64,
    pub volume_bar_num: usize,
    pub volume_ratio: f64,
    pub min_k_line_num: usize,
    pub k_line_hammer_shadow_ratio: f64,

    /// 是否启用动态波动率调整 (根据近期波动率自动调整带宽和止损)
    /// 波动率敏感度 (0.0 ~ 2.0)，建议 0.5。值越大，通道随波动率变化越剧烈
    #[serde(default = "default_use_dynamic_adjustment")]
    pub use_dynamic_adjustment: bool,
    /// 波动率敏感度 (0.0 ~ 2.0)，建议 0.5。值越大，通道随波动率变化越剧烈
    #[serde(default = "default_sensitivity")]
    pub volatility_sensitivity: f64,

    /// 是否启用放宽入场条件（移除前一根K线方向限制）
    #[serde(default = "default_relax_entry")]
    pub relax_entry_conditions: bool,

    /// 动态STC阈值调整系数 (0.5 ~ 2.0)，1.0表示不调整
    #[serde(default = "default_dynamic_stc_adjustment")]
    pub dynamic_stc_adjustment: f64,

    /// 动态ATR倍数调整系数 (0.5 ~ 3.0)，1.0表示不调整
    #[serde(default = "default_dynamic_atr_adjustment")]
    pub dynamic_atr_adjustment: f64,
}

fn default_sensitivity() -> f64 {
    0.5
}

fn default_use_dynamic_adjustment() -> bool {
    false
}

fn default_relax_entry() -> bool {
    true
}

fn default_dynamic_stc_adjustment() -> f64 {
    1.0
}

fn default_dynamic_atr_adjustment() -> f64 {
    1.0
}
fn default_stc_fast_length() -> usize {
    23
}

fn default_stc_slow_length() -> usize {
    50
}

fn default_stc_cycle_length() -> usize {
    10
}

fn default_stc_d1_length() -> usize {
    3
}

fn default_stc_d2_length() -> usize {
    3
}

fn default_stc_overbought() -> f64 {
    75.0
}

fn default_stc_oversold() -> f64 {
    25.0
}

impl Default for NweStrategyConfig {
    fn default() -> Self {
        Self {
            period: "5m".to_string(),
            stc_fast_length: default_stc_fast_length(),
            stc_slow_length: default_stc_slow_length(),
            stc_cycle_length: default_stc_cycle_length(),
            stc_d1_length: default_stc_d1_length(),
            stc_d2_length: default_stc_d2_length(),
            stc_overbought: default_stc_overbought(),
            stc_oversold: default_stc_oversold(),

            atr_period: 14,
            atr_multiplier: 0.5,

            nwe_period: 8,
            nwe_multi: 3.0,

            volume_bar_num: 4,
            volume_ratio: 0.9,

            min_k_line_num: 500,
            k_line_hammer_shadow_ratio: 0.45,

            use_dynamic_adjustment: false,
            volatility_sensitivity: default_sensitivity(),
            relax_entry_conditions: default_relax_entry(),
            dynamic_stc_adjustment: default_dynamic_stc_adjustment(),
            dynamic_atr_adjustment: default_dynamic_atr_adjustment(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NweStrategy {
    pub config: NweStrategyConfig,
    pub combine_indicator: NweIndicatorCombine,
    /// Vegas 通道 EMA 过滤器（12, 144, 169）
    pub vegas_ema_indicator: Option<EmaIndicator>,
}
impl NweStrategy {}

impl NweStrategy {
    /// 创建 Nwe 策略实例（零 clone 优化）✨
    pub fn new(config: NweStrategyConfig) -> Self {
        // ⭐ 转换为 NweIndicatorConfig
        let indicator_config = NweIndicatorConfig {
            stc_fast_length: config.stc_fast_length,
            stc_slow_length: config.stc_slow_length,
            stc_cycle_length: config.stc_cycle_length,
            stc_d1_length: config.stc_d1_length,
            stc_d2_length: config.stc_d2_length,
            volume_bar_num: config.volume_bar_num,
            nwe_period: config.nwe_period,
            nwe_multi: config.nwe_multi,
            atr_period: config.atr_period,
            atr_multiplier: config.atr_multiplier,
            k_line_hammer_shadow_ratio: config.k_line_hammer_shadow_ratio,
            min_k_line_num: config.min_k_line_num,
        };

        Self {
            combine_indicator: NweIndicatorCombine::new(&indicator_config),
            // Vegas EMA 默认使用 12,144,169，其余周期按 Vegas 典型配置占位
            vegas_ema_indicator: Some(EmaIndicator::new(169, 576, 676, 2304, 2704, 2704, 2704)),
            config,
        }
    }
    pub fn get_strategy_name() -> String {
        "nwe".to_string()
    }

    pub fn get_min_data_length(&self) -> usize {
        self.config.min_k_line_num.max(self.config.nwe_period)
    }

    /// 根据市场趋势动态调整STC超卖阈值
    /// 在强势市场中，降低超卖阈值（更容易触发做多）
    /// 在弱势市场中，提高超卖阈值（更谨慎做多）
    fn calculate_dynamic_stc_oversold(&self, candles: &[CandleItem]) -> f64 {
        if self.config.dynamic_stc_adjustment == 1.0 {
            return self.config.stc_oversold;
        }

        let len = candles.len();
        if len < 20 {
            return self.config.stc_oversold;
        }

        // 计算近期价格趋势
        let recent_candles = &candles[len - 20..];
        let first_price = recent_candles.first().map(|c| c.c).unwrap_or(0.0);
        let last_price = recent_candles.last().map(|c| c.c).unwrap_or(0.0);

        if first_price == 0.0 {
            return self.config.stc_oversold;
        }

        let trend_ratio = (last_price - first_price) / first_price;

        // 上涨趋势：降低超卖阈值（更容易触发）
        // 下跌趋势：提高超卖阈值（更谨慎）
        let adjustment = trend_ratio * 10.0 * self.config.dynamic_stc_adjustment;
        let adjusted = self.config.stc_oversold - adjustment;

        // 限制范围在 15-35 之间
        adjusted.clamp(15.0, 35.0)
    }

    /// 根据市场趋势动态调整STC超买阈值
    fn calculate_dynamic_stc_overbought(&self, candles: &[CandleItem]) -> f64 {
        if self.config.dynamic_stc_adjustment == 1.0 {
            return self.config.stc_overbought;
        }

        let len = candles.len();
        if len < 20 {
            return self.config.stc_overbought;
        }

        let recent_candles = &candles[len - 20..];
        let first_price = recent_candles.first().map(|c| c.c).unwrap_or(0.0);
        let last_price = recent_candles.last().map(|c| c.c).unwrap_or(0.0);

        if first_price == 0.0 {
            return self.config.stc_overbought;
        }

        let trend_ratio = (last_price - first_price) / first_price;

        // 下跌趋势：降低超买阈值（更容易触发做空）
        // 上涨趋势：提高超买阈值（更谨慎做空）
        let adjustment = trend_ratio * 10.0 * self.config.dynamic_stc_adjustment;
        let adjusted = self.config.stc_overbought + adjustment;

        // 限制范围在 65-85 之间
        adjusted.clamp(65.0, 85.0)
    }

    /// Parkinson 波动率：利用高低点信息，比收盘价波动率更高效
    /// σ = sqrt(1/(4*n*ln(2)) * Σ(ln(H/L))²)
    fn calculate_parkinson_volatility(candles: &[CandleItem]) -> f64 {
        let n = candles.len() as f64;
        if n < 2.0 {
            return 0.0;
        }

        let sum_sq: f64 = candles
            .iter()
            .filter(|c| c.l > 0.0)
            .map(|c| {
                let ln_hl = (c.h / c.l).ln();
                ln_hl * ln_hl
            })
            .sum();

        let coefficient = 1.0 / (4.0 * n * 2.0_f64.ln());
        (coefficient * sum_sq).sqrt()
    }

    /// 根据市场波动率动态调整ATR倍数（使用 Parkinson 波动率）
    /// 5分钟级别优化：近期4小时（48根），历史24小时（288根）
    fn calculate_dynamic_atr_multiplier(&self, candles: &[CandleItem]) -> f64 {
        if self.config.dynamic_atr_adjustment == 1.0 {
            return self.config.atr_multiplier;
        }

        let len = candles.len();
        // 5分钟级别：近期4小时（48根），历史24小时（288根）
        let short_lookback = 48;
        let long_lookback = 288.min(len);

        if len < short_lookback {
            return self.config.atr_multiplier;
        }

        // 使用 Parkinson 波动率计算近期波动率
        let recent_candles = &candles[len - short_lookback..];
        let recent_vol = Self::calculate_parkinson_volatility(recent_candles);

        // 计算历史波动率
        let historical_candles = &candles[len - long_lookback..];
        let historical_vol = Self::calculate_parkinson_volatility(historical_candles);

        if historical_vol == 0.0 {
            return self.config.atr_multiplier;
        }

        // 波动率比率
        let volatility_ratio = recent_vol / historical_vol;

        // 使用 tanh 平滑因子避免剧烈跳变
        let smoothed_ratio =
            1.0 + (volatility_ratio - 1.0).tanh() * self.config.dynamic_atr_adjustment;

        // 波动率高时，增加ATR倍数（放宽止损）
        // 波动率低时，降低ATR倍数（收紧止损）
        let adjusted_multiplier = self.config.atr_multiplier * smoothed_ratio;

        // 限制范围在 0.3-3.0 之间
        adjusted_multiplier.clamp(0.3, 3.0)
    }

    /// 根据市场波动率动态调整 ATR 和 NWE 带宽
    fn calculate_dynamic_values(
        &self,
        candles: &[CandleItem],
        base_values: &NweSignalValues,
    ) -> NweSignalValues {
        let current_candle = candles.last().unwrap();
        let current_price = current_candle.c;

        // 计算动态ATR倍数
        let dynamic_atr_multiplier = self.calculate_dynamic_atr_multiplier(candles);

        // 如果不启用动态调整，只应用动态ATR倍数
        if !self.config.use_dynamic_adjustment {
            let new_long_stop = current_price - (base_values.atr_value * dynamic_atr_multiplier);
            let new_short_stop = current_price + (base_values.atr_value * dynamic_atr_multiplier);

            return NweSignalValues {
                stc_value: base_values.stc_value,
                volume_ratio: base_values.volume_ratio,
                atr_value: base_values.atr_value,
                atr_short_stop: new_short_stop,
                atr_long_stop: new_long_stop,
                nwe_upper: base_values.nwe_upper,
                nwe_lower: base_values.nwe_lower,
            };
        }

        let len = candles.len();
        let lookback = 20;
        if len < lookback {
            return *base_values;
        }

        // 1. 计算过去 N 根 K 线的平均波动范围 (High - Low)
        let sum_range: f64 = candles[len - lookback..].iter().map(|c| c.h - c.l).sum();
        let avg_range = sum_range / lookback as f64;

        // 2. 计算波动率比率 (当前 ATR / 历史平均 Range)
        let volatility_ratio = if avg_range > 0.0 {
            base_values.atr_value / avg_range
        } else {
            1.0
        };

        // 3. 计算缩放系数 (Scalar)
        // 限制在 0.6 ~ 2.0 之间，防止极端变形
        let scalar =
            (1.0 + (volatility_ratio - 1.0) * self.config.volatility_sensitivity).clamp(0.6, 2.0);
        // let scalar = 1.0;

        // 4. 调整 NWE 带宽
        let nwe_middle = (base_values.nwe_upper + base_values.nwe_lower) / 2.0;
        let original_half_width = (base_values.nwe_upper - base_values.nwe_lower) / 2.0;
        let new_half_width = original_half_width * scalar;

        // 5. 调整 ATR 值和止损位（同时应用波动率调整和动态ATR倍数）
        let adjusted_atr = base_values.atr_value * scalar;
        let new_long_stop = current_price - (adjusted_atr * dynamic_atr_multiplier);
        let new_short_stop = current_price + (adjusted_atr * dynamic_atr_multiplier);

        // 6. 使用动态调整后的带宽重新计算上下轨
        let adjusted_nwe_upper = nwe_middle + new_half_width;
        let adjusted_nwe_lower = nwe_middle - new_half_width;

        // 6. 使用动态调整后的带宽重新计算上下轨
        let adjusted_nwe_upper = nwe_middle + new_half_width;
        let adjusted_nwe_lower = nwe_middle - new_half_width;

        NweSignalValues {
            stc_value: base_values.stc_value,
            volume_ratio: base_values.volume_ratio,
            atr_value: adjusted_atr,
            atr_short_stop: new_short_stop,
            atr_long_stop: new_long_stop,
            nwe_upper: adjusted_nwe_upper,
            nwe_lower: adjusted_nwe_lower,
        }
    }

    /// 检查NWE通道突破信号
    ///
    /// 优化后的入场条件：
    /// 1. 如果启用relax_entry_conditions，移除前一根K线方向限制
    /// 2. 使用动态调整的STC阈值
    pub fn check_nwe(&self, candles: &[CandleItem], values: &NweSignalValues) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;

        let middle = (values.nwe_upper + values.nwe_lower) / 2.0;
        let previous_candle = &candles[candles.len() - 2];
        let current_candle = candles.last().unwrap();

        // K线形态过滤（可选）
        let kline_hammer_indicator_output = KlineHammerIndicator::new(
            self.config.k_line_hammer_shadow_ratio,
            self.config.k_line_hammer_shadow_ratio,
        )
        .next(current_candle);

        let is_hanging_man = kline_hammer_indicator_output.is_hanging_man;
        let is_hammer = kline_hammer_indicator_output.is_hammer;

        // 使用动态调整的STC阈值
        let adjusted_oversold = self.calculate_dynamic_stc_oversold(candles);
        let adjusted_overbought = self.calculate_dynamic_stc_overbought(candles);

        let (is_stc_buy, is_stc_sell) =
            Self::check_stc(values.stc_value, adjusted_oversold, adjusted_overbought);

        // 做多信号判断
        if (previous_candle.c < values.nwe_lower || current_candle.l < values.nwe_lower)
            && current_candle.c >= values.nwe_lower
            && current_candle.c < middle
        {
            // 放宽条件：不要求前一根K线方向
            is_buy = true;
        }

        // 做空信号判断
        if (previous_candle.c > values.nwe_upper || current_candle.h > values.nwe_upper)
            && current_candle.c < values.nwe_upper
            && current_candle.c > middle
        {
            is_sell = true;
        }

        (is_buy, is_sell)
    }

    /// 使用 Vegas 通道的 EMA 排列（12,144,169）过滤方向：
    /// - ema12 > ema144 > ema169 → 只做多
    /// - ema12 < ema144 < ema169 → 只做空
    /// - 其他情况 → 不启用 Vegas 方向过滤
    fn apply_vegas_trend_filter(
        &mut self,
        candles: &[CandleItem],
        signal_result: &mut SignalResult,
    ) -> Option<f64> {
        let ema_indicator = match self.vegas_ema_indicator.as_mut() {
            Some(indicator) => indicator,
            None => {
                return None;
            }
        };

        let last_candle = match candles.last() {
            Some(candle) => candle,
            None => {
                return None;
            }
        };

        let close_price = last_candle.c;

        let ema12 = ema_indicator.ema1_indicator.next(close_price);
        let ema144 = ema_indicator.ema2_indicator.next(close_price);
        let ema169 = ema_indicator.ema3_indicator.next(close_price);

        let is_bull_trend = ema12 > ema144 && ema144 > ema169;
        let is_bear_trend = ema12 < ema144 && ema144 < ema169;

        if !is_bull_trend && !is_bear_trend {
            // 其他情况：不启用 Vegas 过滤
            return Some(ema12);
        }

        // 只在已有 NWE 信号的基础上做方向过滤
        if is_bull_trend && !is_bear_trend {
            if !signal_result.should_buy {
                signal_result.should_sell = false;
            }
        } else if is_bear_trend && !is_bull_trend && !signal_result.should_sell {
            signal_result.should_buy = false;
        }
        Some(ema12)
    }

    /**
     * 检查rsi是否超卖或超买
     */
    fn check_rsi(rsi: f64, rsi_oversold: f64, rsi_overbought: f64) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;
        if rsi < rsi_oversold {
            is_buy = true;
        } else if rsi > rsi_overbought {
            is_sell = true;
        }
        (is_buy, is_sell)
    }
    /**
     * 检查成交量比率是否超卖或超买
     */
    fn check_volume_ratio(volume_ratio: f64, volume_ratio_threshold: f64) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;
        if volume_ratio < volume_ratio_threshold {
            is_buy = true;
        } else if volume_ratio > volume_ratio_threshold {
            is_sell = true;
        }
        (is_buy, is_sell)
    }

    fn check_stc(stc: f64, stc_oversold: f64, stc_overbought: f64) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;
        if stc < stc_oversold {
            is_buy = true;
        } else if stc > stc_overbought {
            is_sell = true;
        }
        (is_buy, is_sell)
    }
    pub fn get_indicator_combine(&self) -> NweIndicatorCombine {
        self.combine_indicator.clone()
    }

    /// 生成信号：
    /// - close 下穿 lower → 做多（结合 STC/Volume/ATR 过滤）
    /// - close 上穿 upper → 做空（结合 STC/Volume/ATR 过滤）
    ///
    /// 优化点：
    /// 1. 动态调整止损和通道宽度
    /// 2. 多级止盈机制
    /// 3. 改进的移动止损逻辑
    pub fn get_trade_signal(
        &mut self,
        candles: &[CandleItem],
        raw_values: &NweSignalValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        // ⭐ 动态调整：根据波动率重新计算阈值
        let values = self.calculate_dynamic_values(candles, raw_values);

        let mut signal_result = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts: 0,
            single_value: None,
            single_result: None,
            signal_kline_stop_loss_price: None,
            stop_loss_source: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::None,
        };

        let current_price = candles.last().unwrap().c;
        let o = candles.last().unwrap().o;
        let l = candles.last().unwrap().l;
        let atr = values.atr_value;
        let upper = values.nwe_upper;
        let lower = values.nwe_lower;

        //检查nwe是否超卖或超买
        let (is_nwe_buy, is_nwe_sell) = self.check_nwe(candles, &values);

        if is_nwe_buy || is_nwe_sell {
            if is_nwe_buy {
                signal_result.should_buy = true;

                // 基于ATR的止损
                signal_result.atr_stop_loss_price = Some(values.atr_long_stop);

                let stop_distance = current_price - values.atr_long_stop;

                // 三级止盈系统
                if let Some(atr_ratio) = risk_config.atr_take_profit_ratio {
                    if atr_ratio > 0.0 {
                        // 第一级：1.5倍ATR
                        let level_1 = current_price + stop_distance * 1.5;
                        signal_result.atr_take_profit_level_1 = Some(level_1);

                        // 第二级：2倍ATR
                        let level_2 = current_price + stop_distance * 2.0;
                        signal_result.atr_take_profit_level_2 = Some(level_2);

                        // 第三级：5倍ATR
                        let level_3 = current_price + stop_distance * 5.0;
                        signal_result.atr_take_profit_level_3 = Some(level_3);
                    }
                } else {
                    // 传统单级止盈
                    if let Some(atr_ratio) = risk_config.atr_take_profit_ratio {
                        if atr_ratio > 0.0 {
                            let atr_take_profit = current_price + stop_distance * atr_ratio;
                            signal_result.atr_take_profit_ratio_price = Some(atr_take_profit);
                        }
                    }
                }
                if let Some(is_used_signal_k_line_stop_loss) =
                    risk_config.is_used_signal_k_line_stop_loss
                {
                    if is_used_signal_k_line_stop_loss {
                        signal_result.signal_kline_stop_loss_price = Some(o);
                    }
                }
            }

            if is_nwe_sell {
                signal_result.should_sell = true;

                // 基于ATR的止损
                signal_result.atr_stop_loss_price = Some(values.atr_short_stop);

                let stop_distance = values.atr_short_stop - current_price;

                // 三级止盈系统
                if let Some(atr_ratio) = risk_config.atr_take_profit_ratio {
                    if atr_ratio > 0.0 {
                        let level_1 = current_price - stop_distance * 1.5;
                        signal_result.atr_take_profit_level_1 = Some(level_1);

                        let level_2 = current_price - stop_distance * 2.0;
                        signal_result.atr_take_profit_level_2 = Some(level_2);

                        let level_3 = current_price - stop_distance * 5.0;
                        signal_result.atr_take_profit_level_3 = Some(level_3);
                    }
                } else {
                    // 传统单级止盈
                    if let Some(atr_ratio) = risk_config.atr_take_profit_ratio {
                        if atr_ratio > 0.0 {
                            let atr_take_profit = current_price - stop_distance * atr_ratio;
                            signal_result.atr_take_profit_ratio_price = Some(atr_take_profit);
                        }
                    }
                }
            }
        }

        //计算是否k线的高点
        // let is_kline_high_point = Self::check_kline_high_point(candles, values);
        // if is_kline_high_point {
        //     signal_result.long_signal_take_profit_price = Some(candles.last().unwrap().c);
        // }
        // //计算是否k线的低点
        // let is_kline_low_point = Self::check_kline_low_point(candles, values);
        // if is_kline_low_point {
        //     signal_result.short_signal_take_profit_price = Some(candles.last().unwrap().c);
        // }

        // 使用 Vegas EMA 排列进行方向过滤
        let ema1_value = self.apply_vegas_trend_filter(candles, &mut signal_result);

        signal_result.ts = candles.last().unwrap().ts;
        signal_result.open_price = candles.last().unwrap().c;
        // if candles.last().unwrap().ts == 1765566000000 {
        //     println!("values: {:#?}", values);
        //     println!("signal_result: {:#?}", signal_result);
        // }

        // info!("NWE signal values: {:#?}", values);
        // info!(
        // "ts : {:#?}",
        //     rust_quant_common::utils::time::mill_time_to_datetime_shanghai(signal_result.ts)
        // );
        signal_result.single_value = Some(json!(values.clone()).to_string());
        signal_result.single_result = Some(json!(signal_result.clone()).to_string());

        signal_result
    }

    fn check_kline_high_point(candles: &[CandleItem], values: &NweSignalValues) -> bool {
        let last_candle = candles.last().unwrap();
        last_candle.h > values.nwe_upper
    }

    fn check_kline_low_point(candles: &[CandleItem], values: &NweSignalValues) -> bool {
        let last_candle = candles.last().unwrap();
        last_candle.l < values.nwe_lower
    }

    /// 运行回测：仅使用 RSI、Volume、NWE、ATR 指标（复用可插拔的 indicator_combine）
    pub fn run_test(
        mut self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(inst_id, self, candles, risk)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NweSignalValues {
    pub stc_value: f64,

    pub volume_ratio: f64,

    pub atr_value: f64,
    pub atr_short_stop: f64,
    pub atr_long_stop: f64,

    pub nwe_upper: f64,
    pub nwe_lower: f64,
}

impl From<NweIndicatorValues> for NweSignalValues {
    fn from(value: NweIndicatorValues) -> Self {
        Self {
            stc_value: value.stc_value,
            volume_ratio: value.volume_ratio,
            atr_value: value.atr_value,
            atr_short_stop: value.atr_short_stop,
            atr_long_stop: value.atr_long_stop,
            nwe_upper: value.nwe_upper,
            nwe_lower: value.nwe_lower,
        }
    }
}

impl IndicatorStrategyBacktest for NweStrategy {
    type IndicatorCombine = NweIndicatorCombine;
    type IndicatorValues = NweSignalValues;

    fn min_data_length(&self) -> usize {
        self.get_min_data_length()
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {
        self.combine_indicator.clone()
    }

    fn build_indicator_values(
        indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues {
        indicator_combine.next(candle).into()
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        self.get_trade_signal(candles, values, risk_config)
    }
}
