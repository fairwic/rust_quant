use crate::{trading::{indicator::{atr::ATR, atr_stop_loos::ATRStopLoos, nwe_indicator::NweIndicator, rsi_rma_indicator::RsiIndicator, volume_indicator::VolumeRatioIndicator}, strategy::nwe_strategy::{NweSignalValues, NweStrategyConfig}}, CandleItem};

/// 指标组合结构体
#[derive(Debug, Clone)]
pub struct NweIndicatorCombine {
    // RSI指标
    pub rsi_indicator: Option<RsiIndicator>,   // 成交量指标
    pub volume_indicator: Option<VolumeRatioIndicator>,
    // NWE指标
    pub nwe_indicator: Option<NweIndicator>,
    // ATR指标
    pub atr_indicator: Option<ATRStopLoos>,
}

impl NweIndicatorCombine {
    pub fn new(config: NweStrategyConfig) -> Self {
        Self {
            rsi_indicator: Some(RsiIndicator::new(config.rsi_period)),
            volume_indicator: Some(VolumeRatioIndicator::new(config.volume_bar_num, true)),
            // Use NWE-specific config values for the envelope
            nwe_indicator: Some(NweIndicator::new(
                config.nwe_period as f64,
                config.nwe_multi,
                500,
            )),
            atr_indicator: Some(ATRStopLoos::new(config.atr_period, config.atr_multiplier).expect("ATR period must be > 0")),
        }
    }

    pub fn get_indicator_values(&mut self, nwe_signal_values: &mut NweSignalValues, data_item: &CandleItem) -> NweSignalValues {
        nwe_signal_values.rsi_value = self.rsi_indicator.as_mut().unwrap().next(data_item.c);
        nwe_signal_values.volume_ratio = self.volume_indicator.as_mut().unwrap().next(data_item.v);
        let (short_stop, long_stop, atr_value) = self.atr_indicator.as_mut().unwrap().next(data_item.h, data_item.l, data_item.c);
        nwe_signal_values.atr_value = atr_value;
        nwe_signal_values.atr_short_stop = short_stop;
        nwe_signal_values.atr_long_stop = long_stop;
        let (nwe_upper, nwe_lower) = self.nwe_indicator.as_mut().unwrap().next(data_item.c);
        nwe_signal_values.nwe_upper = nwe_upper;
        nwe_signal_values.nwe_lower = nwe_lower;
        *nwe_signal_values
    }
}

impl Default for NweIndicatorCombine {
    fn default() -> Self {
        Self::new(NweStrategyConfig::default())
    }
}
