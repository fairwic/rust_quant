use crate::ema_indicator::EmaIndicator;
use crate::fair_value_gap_indicator::FairValueGapIndicator;
use crate::leg_detection_indicator::LegDetectionIndicator;
use crate::market_structure_indicator::MarketStructureIndicator;
use crate::momentum::rsi::RsiIndicator;
use crate::pattern::engulfing::KlineEngulfingIndicator;
use crate::pattern::hammer::KlineHammerIndicator;
use crate::premium_discount_indicator::PremiumDiscountIndicator;
use crate::volatility::bollinger::BollingBandsPlusIndicator;
use crate::volume_indicator::VolumeRatioIndicator;

/// 指标组合结构体
#[derive(Debug, Clone)]
pub struct IndicatorCombine {
    pub ema_indicator: Option<EmaIndicator>,
    pub rsi_indicator: Option<RsiIndicator>,
    pub volume_indicator: Option<VolumeRatioIndicator>,
    pub bollinger_indicator: Option<BollingBandsPlusIndicator>,
    pub engulfing_indicator: Option<KlineEngulfingIndicator>,
    pub kline_hammer_indicator: Option<KlineHammerIndicator>,
    // Smart Money Concepts相关指标
    pub leg_detection_indicator: Option<LegDetectionIndicator>,
    pub market_structure_indicator: Option<MarketStructureIndicator>,
    pub fair_value_gap_indicator: Option<FairValueGapIndicator>,
    pub premium_discount_indicator: Option<PremiumDiscountIndicator>,
}

impl Default for IndicatorCombine {
    fn default() -> Self {
        Self {
            ema_indicator: None,
            rsi_indicator: None,
            volume_indicator: None,
            bollinger_indicator: None,
            engulfing_indicator: None,
            kline_hammer_indicator: None,
            // Smart Money Concepts相关指标
            leg_detection_indicator: None,
            market_structure_indicator: None,
            fair_value_gap_indicator: None,
            premium_discount_indicator: None,
        }
    }
}

impl IndicatorCombine {
    /// 计算所有启用指标中的最大窗口周期，用于动态设置回看长度
    pub fn max_required_lookback(&self) -> usize {
        let mut max_period = 0usize;
        if let Some(ema) = &self.ema_indicator {
            max_period = max_period.max(ema.max_period());
        }
        if let Some(bb) = &self.bollinger_indicator {
            max_period = max_period.max(bb.period);
        }
        if let Some(_rsi) = &self.rsi_indicator {
            // RsiIndicator 的 length 字段是私有，这里按常规默认使用 14 作为保守值
            // 如果需要精确，请将 RsiIndicator 暴露 length 或提供 getter
            max_period = max_period.max(14);
        }
        if let Some(_vol) = &self.volume_indicator {
            // VolumeRatioIndicator 未暴露窗口，采用保守值，或在其结构体中暴露 length/getter
            max_period = max_period.max(20);
        }
        // 其他形态/结构类指标多为无窗口或小窗口，这里不计入
        max_period
    }
}
