use crate::ema_indicator::EmaIndicator;
use crate::leg_detection_indicator::LegDetectionIndicator;
use crate::market_structure_indicator::MarketStructureIndicator;
use crate::momentum::rsi::RsiIndicator;
use crate::pattern::engulfing::KlineEngulfingIndicator;
use crate::pattern::hammer::KlineHammerIndicator;
use crate::volatility::bollinger::BollingBandsPlusIndicator;
use crate::volume::VolumeProfileIndicator;
use crate::volume_indicator::VolumeRatioIndicator;
/// 指标组合结构体
#[derive(Debug, Clone, Default)]
pub struct IndicatorCombine {
    /// EMA 指标实例；为空时表示未初始化。
    pub ema_indicator: Option<EmaIndicator>,
    /// RSI 指标实例；为空时表示未初始化。
    pub rsi_indicator: Option<RsiIndicator>,
    /// 成交量指标实例；为空时表示未初始化。
    pub volume_indicator: Option<VolumeRatioIndicator>,
    /// 成交量分布指标实例；为空时表示未初始化。
    pub volume_profile_indicator: Option<VolumeProfileIndicator>,
    /// 布林带指标实例；为空时表示未初始化。
    pub bollinger_indicator: Option<BollingBandsPlusIndicator>,
    /// 吞没形态指标实例；为空时表示未初始化。
    pub engulfing_indicator: Option<KlineEngulfingIndicator>,
    /// klinehammerindicator；为空时表示该值未提供。
    pub kline_hammer_indicator: Option<KlineHammerIndicator>,
    /// 波段识别指标实例；为空时表示未初始化。
    pub leg_detection_indicator: Option<LegDetectionIndicator>,
    /// 市场结构指标实例；为空时表示未初始化。
    pub market_structure_indicator: Option<MarketStructureIndicator>,
}
impl IndicatorCombine {
    /// 计算所有启用指标中的最大窗口周期，用于动态设置回看长度
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
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
        if let Some(profile) = &self.volume_profile_indicator {
            max_period = max_period.max(profile.lookback());
        }
        // 其他形态/结构类指标多为无窗口或小窗口，这里不计入
        max_period
    }
}
