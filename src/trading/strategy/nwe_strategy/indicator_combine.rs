use crate::trading::indicator::{atr::ATR, nwe_indicator::NweIndicator, rsi_rma_indicator::RsiIndicator, volume_indicator::VolumeRatioIndicator};

/// 指标组合结构体
#[derive(Debug, Clone)]
pub struct NweIndicatorCombine {
    // RSI指标
    pub rsi_indicator: Option<RsiIndicator>,
    // 成交量指标
    pub volume_indicator: Option<VolumeRatioIndicator>,
    // NWE指标
    pub nwe_indicator: Option<NweIndicator>,
    // ATR指标
    pub atr_indicator: Option<ATR>,
}

impl NweIndicatorCombine {
    pub fn new() -> Self {
        Self {
            rsi_indicator: None,
            volume_indicator: None,
            nwe_indicator: None,
            atr_indicator: None,
        }
    }
}
