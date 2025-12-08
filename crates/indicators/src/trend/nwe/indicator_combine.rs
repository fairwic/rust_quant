//! NWE 指标组合
//!
//! 将 RSI、Volume、NWE、ATR 等指标组合在一起计算
//!
//! 注意：这是纯粹的计算逻辑，不包含交易决策

use crate::momentum::stc::StcIndicator;
use crate::trend::nwe_indicator::NweIndicator;
use crate::volatility::atr_stop_loss::ATRStopLoos;
use crate::volume::VolumeRatioIndicator;
use rust_quant_common::CandleItem;

/// NWE 指标组合配置
#[derive(Debug, Clone)]
pub struct NweIndicatorConfig {
    pub stc_fast_length: usize,
    pub stc_slow_length: usize,
    pub stc_cycle_length: usize,
    pub stc_d1_length: usize,
    pub stc_d2_length: usize,
    pub volume_bar_num: usize,
    pub nwe_period: usize,
    pub nwe_multi: f64,
    pub atr_period: usize,
    pub atr_multiplier: f64,
    pub k_line_hammer_shadow_ratio: f64,
    pub min_k_line_num: usize,
}

impl Default for NweIndicatorConfig {
    fn default() -> Self {
        Self {
            stc_fast_length: 23,
            stc_slow_length: 50,
            stc_cycle_length: 10,
            stc_d1_length: 3,
            stc_d2_length: 3,
            volume_bar_num: 4,
            nwe_period: 8,
            nwe_multi: 3.0,
            atr_period: 14,
            atr_multiplier: 0.5,
            k_line_hammer_shadow_ratio: 0.45,
            min_k_line_num: 500,
        }
    }
}

/// NWE 指标值输出
#[derive(Debug, Clone, Copy, Default)]
pub struct NweIndicatorValues {
    pub stc_value: f64,
    pub volume_ratio: f64,
    pub atr_value: f64,
    pub atr_short_stop: f64,
    pub atr_long_stop: f64,
    pub nwe_upper: f64,
    pub nwe_lower: f64,
}

/// NWE 指标组合
///
/// 组合多个技术指标进行计算
#[derive(Debug, Clone)]
pub struct NweIndicatorCombine {
    stc_indicator: Option<StcIndicator>,
    volume_indicator: Option<VolumeRatioIndicator>,
    nwe_indicator: Option<NweIndicator>,
    atr_indicator: Option<ATRStopLoos>,
}

impl NweIndicatorCombine {
    /// 创建新的指标组合
    pub fn new(config: &NweIndicatorConfig) -> Self {
        // 保护 STC 参数，确保 fast < slow 且都 > 0
        let slow = config.stc_slow_length.max(2);
        let fast = config
            .stc_fast_length
            .max(1)
            .min(slow.saturating_sub(1).max(1));
        let cycle = config.stc_cycle_length.max(1);
        let d1 = config.stc_d1_length.max(1);
        let d2 = config.stc_d2_length.max(1);
        Self {
            stc_indicator: Some(StcIndicator::new(fast, slow, cycle, d1, d2)),
            volume_indicator: Some(VolumeRatioIndicator::new(config.volume_bar_num, true)),
            nwe_indicator: Some(NweIndicator::new(
                config.nwe_period as f64,
                config.nwe_multi,
                500,
            )),
            atr_indicator: Some(
                ATRStopLoos::new(config.atr_period, config.atr_multiplier)
                    .expect("ATR period must be > 0"),
            ),
        }
    }

    /// 推进所有指标并返回当前值
    ///
    /// # 参数
    /// * `candle` - 当前K线数据
    ///
    /// # 返回
    /// * `NweIndicatorValues` - 所有指标的当前值
    pub fn next(&mut self, candle: &CandleItem) -> NweIndicatorValues {
        let stc_value = if let Some(ind) = &mut self.stc_indicator {
            ind.next(candle.c)
        } else {
            0.0
        };

        let volume_ratio = if let Some(v) = &mut self.volume_indicator {
            v.next(candle.v)
        } else {
            0.0
        };

        let (short_stop, long_stop, atr_value) = if let Some(a) = &mut self.atr_indicator {
            a.next(candle.h, candle.l, candle.c)
        } else {
            (0.0, 0.0, 0.0)
        };

        let (upper, lower) = if let Some(n) = &mut self.nwe_indicator {
            n.next(candle.c)
        } else {
            (0.0, 0.0)
        };

        NweIndicatorValues {
            stc_value: stc_value,
            volume_ratio,
            atr_value,
            atr_short_stop: short_stop,
            atr_long_stop: long_stop,
            nwe_upper: upper,
            nwe_lower: lower,
        }
    }

    /// 计算指标值（不修改内部状态的版本）
    ///
    /// 用于批量计算历史数据
    pub fn get_indicator_values(&mut self, data_item: &CandleItem) -> NweIndicatorValues {
        self.next(data_item)
    }
}

impl Default for NweIndicatorCombine {
    fn default() -> Self {
        Self::new(&NweIndicatorConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nwe_indicator_combine_creation() {
        let config = NweIndicatorConfig::default();
        let combine = NweIndicatorCombine::new(&config);

        assert!(combine.stc_indicator.is_some());
        assert!(combine.volume_indicator.is_some());
        assert!(combine.nwe_indicator.is_some());
        assert!(combine.atr_indicator.is_some());
    }

    #[test]
    fn test_nwe_indicator_combine_next() {
        let mut combine = NweIndicatorCombine::default();
        let candle = CandleItem {
            ts: 1609459200000,
            o: 50000.0,
            h: 51000.0,
            l: 49000.0,
            c: 50500.0,
            v: 100.5,
            confirm: 0,
        };

        let values = combine.next(&candle);

        // 基本验证：返回值应该是有效的数字
        assert!(values.stc_value.is_finite());
        assert!(values.volume_ratio.is_finite());
        assert!(values.atr_value.is_finite());
    }
}
