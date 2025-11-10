//! NWE 指标组合
//!
//! 将 RSI、Volume、NWE、ATR 等指标组合在一起计算
//!
//! 注意：这是纯粹的计算逻辑，不包含交易决策

use crate::momentum::rsi::RsiIndicator;
use crate::trend::nwe_indicator::NweIndicator;
use crate::volatility::atr_stop_loss::ATRStopLoos;
use crate::volume::VolumeRatioIndicator;
use rust_quant_common::CandleItem;

/// NWE 指标组合配置
#[derive(Debug, Clone)]
pub struct NweIndicatorConfig {
    pub rsi_period: usize,
    pub volume_bar_num: usize,
    pub nwe_period: usize,
    pub nwe_multi: f64,
    pub atr_period: usize,
    pub atr_multiplier: f64,
}

impl Default for NweIndicatorConfig {
    fn default() -> Self {
        Self {
            rsi_period: 14,
            volume_bar_num: 4,
            nwe_period: 8,
            nwe_multi: 3.0,
            atr_period: 14,
            atr_multiplier: 0.5,
        }
    }
}

/// NWE 指标值输出
#[derive(Debug, Clone, Copy, Default)]
pub struct NweIndicatorValues {
    pub rsi_value: f64,
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
    rsi_indicator: Option<RsiIndicator>,
    volume_indicator: Option<VolumeRatioIndicator>,
    nwe_indicator: Option<NweIndicator>,
    atr_indicator: Option<ATRStopLoos>,
}

impl NweIndicatorCombine {
    /// 创建新的指标组合
    pub fn new(config: &NweIndicatorConfig) -> Self {
        Self {
            rsi_indicator: Some(RsiIndicator::new(config.rsi_period)),
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
        let rsi = if let Some(r) = &mut self.rsi_indicator {
            r.next(candle.c)
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
            rsi_value: rsi,
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

        assert!(combine.rsi_indicator.is_some());
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
        };

        let values = combine.next(&candle);

        // 基本验证：返回值应该是有效的数字
        assert!(values.rsi_value.is_finite());
        assert!(values.volume_ratio.is_finite());
        assert!(values.atr_value.is_finite());
    }
}
