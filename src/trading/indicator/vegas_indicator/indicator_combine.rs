use crate::trading::indicator::ema_indicator::EmaIndicator;
use crate::trading::indicator::rsi_rma_indicator::RsiIndicator;
use crate::trading::indicator::volume_indicator::VolumeRatioIndicator;
use crate::trading::indicator::bollings::BollingBandsPlusIndicator;
use crate::trading::indicator::k_line_engulfing_indicator::KlineEngulfingIndicator;
use crate::trading::indicator::k_line_hammer_indicator::KlineHammerIndicator;
use crate::trading::indicator::equal_high_low_indicator::EqualHighLowIndicator;
use crate::trading::indicator::fair_value_gap_indicator::FairValueGapIndicator;
use crate::trading::indicator::leg_detection_indicator::LegDetectionIndicator;
use crate::trading::indicator::market_structure_indicator::MarketStructureIndicator;
use crate::trading::indicator::premium_discount_indicator::PremiumDiscountIndicator;

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
    pub equal_high_low_indicator: Option<EqualHighLowIndicator>,
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
            equal_high_low_indicator: None,
            premium_discount_indicator: None,
        }
    }
} 