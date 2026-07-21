//! 策略相关枚举
use serde::{Deserialize, Serialize};
/// 策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrategyType {
    /// Vegas 策略
    Vegas,
    /// Vegas 全市场自适应 4H 策略
    VegasUniversal4h,
    /// Vegas MACD 背离 + fresh CHoCH 4H 研究策略
    VegasMacdDivergenceFreshChoch4hResearch,
    /// Vegas MACD 趋势复位 + fresh BOS 4H 研究策略
    VegasMacdTrendResetFreshBos4hResearch,
    /// Vegas 扫流动性、确认收回、首次回测 4H 研究策略
    VegasLiquiditySweepFirstRetest4hResearch,
    /// Vegas 扫低流动性后两棒确认多头 4H 研究策略
    VegasLowerLiquiditySweepConfirmation4hResearch,
    /// Vegas 跌破支撑后收回结构内的多头 4H 研究策略
    VegasFailedBreakdownCloseReentryLong4hResearch,
    /// Vegas 失败跌破后更高低点突破多头 4H 研究策略
    VegasFailedBreakdownHigherLowBreakoutLong4hResearch,
    /// Vegas 扫高收回后跌破确认棒低点空头 4H 研究策略
    VegasUpperSweepConfirmationLowBreakShort4hResearch,
    /// Vegas 仅保留冲击级量能压缩突破空头的 4H 研究策略
    VegasHighVolumeCompressedSweep4hResearch,
    /// Vegas 高量压缩基线 + 第二根首次回测的 4H 研究策略
    VegasHighVolumeCompressedDelayedRetest4hResearch,
    /// Vegas 使用既有 2.0x 放量标准的压缩突破 4H 研究策略
    VegasVolumeIncreaseCompressedSweep4hResearch,
    /// Vegas 压缩区间突破做空、结构止损的独立 4H 研究策略
    VegasCompressedRangeBreakoutShort4hResearch,
    /// Vegas 下方扫单收回后突破确认高点的 4H 研究策略
    VegasLowerSweepConfirmationHighBreakLong4hResearch,
    /// Vegas EMA144/169 隧道顺势回踩确认 4H 研究策略
    VegasEmaTunnelRetestConfirmation4hResearch,
    /// Vegas 固定成交量价值区突破回踩确认 4H 研究策略
    VegasVolumeProfileValueAreaRetest4hResearch,
    /// Vegas 固定成交量价值区即时突破 4H 研究策略
    VegasVolumeProfileValueAreaBreakout4hResearch,
    /// Vegas 成交量价值区上方失败拍卖做空 4H 研究策略
    VegasVolumeProfileFailedAuctionShort4hResearch,
    /// Vegas Donchian 放量通道突破 4H 研究策略
    VegasDonchianVolumeBreakout4hResearch,
    /// Vegas Donchian 突破后一棒接受 4H 研究策略
    VegasDonchianBreakoutAcceptance4hResearch,
    /// NWE 策略
    Nwe,
    /// MACD+KDJ 策略
    MacdKdj,
    /// 吞没形态策略
    Engulfing,
    /// 综合策略
    Comprehensive,
    /// 多因子组合策略
    MultCombine,
    /// Squeeze 动量策略
    Squeeze,
    /// UT Boot 策略
    UtBoot,
    /// 顶级合约策略
    TopContract,
    /// BSC 事件套利策略
    BscEventArb,
    /// Market Velocity 动量策略
    MarketVelocity,
    /// BTC/ETH 流动性剥头皮策略
    BtcEthLiquidityScalper,
    /// BTC/ETH 做空策略栈
    BearShortStack,
    /// BTC/ETH 短周期均值回归剥头皮策略
    RangeReversionScalper,
    /// BTC/ETH 短周期动量突破回踩策略
    MomentumBreakoutScalper,
    /// Smart Money Concepts 结构突破研究策略
    SmartMoneyConceptsV1Research,
    /// Keltner Channel 1m 剥头皮研究策略
    KeltnerChannelScalper1mV1Research,
    /// ETH 5m 放量反转研究策略
    EthVolumeReversal5mV1Research,
    /// ETH 5m 多空放量反转研究策略
    EthVolumeReversalDual5mV1Research,
    /// BTC 5m 多空放量反转研究策略
    BtcVolumeReversalDual5mV1Research,
    /// BTC 5m 放量反转混合研究策略
    BtcVolumeReversalHybrid5mV1Research,
    /// 自定义策略
    Custom(u32),
}
impl StrategyType {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    pub fn as_str(&self) -> &str {
        match self {
            StrategyType::Vegas => "vegas",
            StrategyType::VegasUniversal4h => "vegas_universal_4h",
            StrategyType::VegasMacdDivergenceFreshChoch4hResearch => {
                "vegas_macd_divergence_fresh_choch_reversal_4h_research"
            }
            StrategyType::VegasMacdTrendResetFreshBos4hResearch => {
                "vegas_macd_trend_reset_fresh_bos_4h_research"
            }
            StrategyType::VegasLiquiditySweepFirstRetest4hResearch => {
                "vegas_liquidity_sweep_first_retest_4h_research"
            }
            StrategyType::VegasLowerLiquiditySweepConfirmation4hResearch => {
                "vegas_lower_liquidity_sweep_confirmation_4h_research"
            }
            StrategyType::VegasFailedBreakdownCloseReentryLong4hResearch => {
                "vegas_failed_breakdown_close_reentry_long_4h_research"
            }
            StrategyType::VegasFailedBreakdownHigherLowBreakoutLong4hResearch => {
                "vegas_failed_breakdown_higher_low_breakout_long_4h_research"
            }
            StrategyType::VegasUpperSweepConfirmationLowBreakShort4hResearch => {
                "vegas_upper_sweep_confirmation_low_break_short_4h_research"
            }
            StrategyType::VegasHighVolumeCompressedSweep4hResearch => {
                "vegas_high_volume_compressed_sweep_4h_research"
            }
            StrategyType::VegasHighVolumeCompressedDelayedRetest4hResearch => {
                "vegas_high_volume_compressed_delayed_retest_4h_research"
            }
            StrategyType::VegasVolumeIncreaseCompressedSweep4hResearch => {
                "vegas_volume_increase_compressed_sweep_4h_research"
            }
            StrategyType::VegasCompressedRangeBreakoutShort4hResearch => {
                "vegas_compressed_range_breakout_short_4h_research"
            }
            StrategyType::VegasLowerSweepConfirmationHighBreakLong4hResearch => {
                "vegas_lower_sweep_confirmation_high_break_long_4h_research"
            }
            StrategyType::VegasEmaTunnelRetestConfirmation4hResearch => {
                "vegas_ema_tunnel_retest_confirmation_4h_research"
            }
            StrategyType::VegasVolumeProfileValueAreaRetest4hResearch => {
                "vegas_volume_profile_value_area_retest_4h_research"
            }
            StrategyType::VegasVolumeProfileValueAreaBreakout4hResearch => {
                "vegas_volume_profile_value_area_breakout_4h_research"
            }
            StrategyType::VegasVolumeProfileFailedAuctionShort4hResearch => {
                "vegas_volume_profile_failed_auction_short_4h_research"
            }
            StrategyType::VegasDonchianVolumeBreakout4hResearch => {
                "vegas_donchian_volume_breakout_4h_research"
            }
            StrategyType::VegasDonchianBreakoutAcceptance4hResearch => {
                "vegas_donchian_breakout_acceptance_4h_research"
            }
            StrategyType::Nwe => "nwe",
            StrategyType::MacdKdj => "macd_kdj",
            StrategyType::Engulfing => "engulfing",
            StrategyType::Comprehensive => "comprehensive",
            StrategyType::MultCombine => "mult_combine",
            StrategyType::Squeeze => "squeeze",
            StrategyType::UtBoot => "ut_boot",
            StrategyType::TopContract => "top_contract",
            StrategyType::BscEventArb => "bsc_event_arb",
            StrategyType::MarketVelocity => "market_velocity",
            StrategyType::BtcEthLiquidityScalper => "btc_eth_liquidity_scalper_v1",
            StrategyType::BearShortStack => "bear_short_stack_v1",
            StrategyType::RangeReversionScalper => "range_reversion_scalper_v1",
            StrategyType::MomentumBreakoutScalper => "momentum_breakout_scalper_v1",
            StrategyType::SmartMoneyConceptsV1Research => "smart_money_concepts_v1_research",
            StrategyType::KeltnerChannelScalper1mV1Research => {
                "keltner_channel_scalper_1m_v1_research"
            }
            StrategyType::EthVolumeReversal5mV1Research => "eth_volume_reversal_5m_v1_research",
            StrategyType::EthVolumeReversalDual5mV1Research => {
                "eth_volume_reversal_dual_5m_v1_research"
            }
            StrategyType::BtcVolumeReversalDual5mV1Research => {
                "btc_volume_reversal_dual_5m_v1_research"
            }
            StrategyType::BtcVolumeReversalHybrid5mV1Research => {
                "btc_volume_reversal_hybrid_5m_v1_research"
            }
            StrategyType::Custom(_) => "custom",
        }
    }
}
impl std::str::FromStr for StrategyType {
    type Err = String;
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vegas" => Ok(StrategyType::Vegas),
            "vegas_universal_4h" => Ok(StrategyType::VegasUniversal4h),
            "vegas_macd_divergence_fresh_choch_reversal_4h_research" => {
                Ok(StrategyType::VegasMacdDivergenceFreshChoch4hResearch)
            }
            "vegas_macd_trend_reset_fresh_bos_4h_research" => {
                Ok(StrategyType::VegasMacdTrendResetFreshBos4hResearch)
            }
            "vegas_liquidity_sweep_first_retest_4h_research" => {
                Ok(StrategyType::VegasLiquiditySweepFirstRetest4hResearch)
            }
            "vegas_lower_liquidity_sweep_confirmation_4h_research" => {
                Ok(StrategyType::VegasLowerLiquiditySweepConfirmation4hResearch)
            }
            "vegas_failed_breakdown_close_reentry_long_4h_research" => {
                Ok(StrategyType::VegasFailedBreakdownCloseReentryLong4hResearch)
            }
            "vegas_failed_breakdown_higher_low_breakout_long_4h_research" => {
                Ok(StrategyType::VegasFailedBreakdownHigherLowBreakoutLong4hResearch)
            }
            "vegas_upper_sweep_confirmation_low_break_short_4h_research" => {
                Ok(StrategyType::VegasUpperSweepConfirmationLowBreakShort4hResearch)
            }
            "vegas_high_volume_compressed_sweep_4h_research" => {
                Ok(StrategyType::VegasHighVolumeCompressedSweep4hResearch)
            }
            "vegas_high_volume_compressed_delayed_retest_4h_research" => {
                Ok(StrategyType::VegasHighVolumeCompressedDelayedRetest4hResearch)
            }
            "vegas_volume_increase_compressed_sweep_4h_research" => {
                Ok(StrategyType::VegasVolumeIncreaseCompressedSweep4hResearch)
            }
            "vegas_compressed_range_breakout_short_4h_research" => {
                Ok(StrategyType::VegasCompressedRangeBreakoutShort4hResearch)
            }
            "vegas_lower_sweep_confirmation_high_break_long_4h_research" => {
                Ok(StrategyType::VegasLowerSweepConfirmationHighBreakLong4hResearch)
            }
            "vegas_ema_tunnel_retest_confirmation_4h_research" => {
                Ok(StrategyType::VegasEmaTunnelRetestConfirmation4hResearch)
            }
            "vegas_volume_profile_value_area_retest_4h_research" => {
                Ok(StrategyType::VegasVolumeProfileValueAreaRetest4hResearch)
            }
            "vegas_volume_profile_value_area_breakout_4h_research" => {
                Ok(StrategyType::VegasVolumeProfileValueAreaBreakout4hResearch)
            }
            "vegas_volume_profile_failed_auction_short_4h_research" => {
                Ok(StrategyType::VegasVolumeProfileFailedAuctionShort4hResearch)
            }
            "vegas_donchian_volume_breakout_4h_research" => {
                Ok(StrategyType::VegasDonchianVolumeBreakout4hResearch)
            }
            "vegas_donchian_breakout_acceptance_4h_research" => {
                Ok(StrategyType::VegasDonchianBreakoutAcceptance4hResearch)
            }
            "nwe" => Ok(StrategyType::Nwe),
            "macd_kdj" => Ok(StrategyType::MacdKdj),
            "engulfing" => Ok(StrategyType::Engulfing),
            "comprehensive" => Ok(StrategyType::Comprehensive),
            "mult_combine" => Ok(StrategyType::MultCombine),
            "squeeze" => Ok(StrategyType::Squeeze),
            "ut_boot" => Ok(StrategyType::UtBoot),
            "top_contract" => Ok(StrategyType::TopContract),
            "bsc_event_arb" => Ok(StrategyType::BscEventArb),
            "market_velocity" => Ok(StrategyType::MarketVelocity),
            // 新策略只接受带版本的 key，避免回测、paper 和 live 结果混在同一个无版本标识下。
            "btc_eth_liquidity_scalper_v1" => Ok(StrategyType::BtcEthLiquidityScalper),
            "bear_short_stack_v1" | "bear_breakdown_short_v1" | "exhaustion_fade_short_v1" => {
                Ok(StrategyType::BearShortStack)
            }
            "range_reversion_scalper_v1" => Ok(StrategyType::RangeReversionScalper),
            "momentum_breakout_scalper_v1" => Ok(StrategyType::MomentumBreakoutScalper),
            "smart_money_concepts_v1_research" => Ok(StrategyType::SmartMoneyConceptsV1Research),
            "keltner_channel_scalper_1m_v1_research" => {
                Ok(StrategyType::KeltnerChannelScalper1mV1Research)
            }
            "eth_volume_reversal_5m_v1_research" => Ok(StrategyType::EthVolumeReversal5mV1Research),
            "eth_volume_reversal_dual_5m_v1_research" => {
                Ok(StrategyType::EthVolumeReversalDual5mV1Research)
            }
            "btc_volume_reversal_dual_5m_v1_research" => {
                Ok(StrategyType::BtcVolumeReversalDual5mV1Research)
            }
            "btc_volume_reversal_hybrid_5m_v1_research" => {
                Ok(StrategyType::BtcVolumeReversalHybrid5mV1Research)
            }
            _ => Err(format!("Unknown strategy type: {}", s)),
        }
    }
}
/// 策略状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrategyStatus {
    /// 未启动
    Stopped,
    /// 运行中
    #[default]
    Running,
    /// 暂停
    Paused,
    /// 错误
    Error,
}
impl StrategyStatus {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    pub fn as_str(&self) -> &'static str {
        match self {
            StrategyStatus::Stopped => "stopped",
            StrategyStatus::Running => "running",
            StrategyStatus::Paused => "paused",
            StrategyStatus::Error => "error",
        }
    }
}
/// 时间周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    /// 1分钟
    M1,
    /// 3分钟
    M3,
    /// 5分钟
    M5,
    /// 15分钟
    M15,
    /// 30分钟
    M30,
    /// 1小时
    H1,
    /// 2小时
    H2,
    /// 4小时
    H4,
    /// 6小时
    H6,
    /// 12小时
    H12,
    /// 1天
    D1,
    /// 1周
    W1,
    /// 1月
    MN1,
}
impl Timeframe {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::M1 => "1m",
            Timeframe::M3 => "3m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::M30 => "30m",
            Timeframe::H1 => "1H",
            Timeframe::H2 => "2H",
            Timeframe::H4 => "4H",
            Timeframe::H6 => "6H",
            Timeframe::H12 => "12H",
            Timeframe::D1 => "1Dutc",
            Timeframe::W1 => "1W",
            Timeframe::MN1 => "1M",
        }
    }
    /// 获取时间周期对应的分钟数
    pub fn to_minutes(&self) -> i64 {
        match self {
            Timeframe::M1 => 1,
            Timeframe::M3 => 3,
            Timeframe::M5 => 5,
            Timeframe::M15 => 15,
            Timeframe::M30 => 30,
            Timeframe::H1 => 60,
            Timeframe::H2 => 120,
            Timeframe::H4 => 240,
            Timeframe::H6 => 360,
            Timeframe::H12 => 720,
            Timeframe::D1 => 1440,
            Timeframe::W1 => 10080,
            Timeframe::MN1 => 43200,
        }
    }
}
impl std::str::FromStr for Timeframe {
    type Err = String;
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1m" => Ok(Timeframe::M1),
            "3m" => Ok(Timeframe::M3),
            "5m" => Ok(Timeframe::M5),
            "15m" => Ok(Timeframe::M15),
            "30m" => Ok(Timeframe::M30),
            "1H" | "1h" => Ok(Timeframe::H1),
            "2H" | "2h" => Ok(Timeframe::H2),
            "4H" | "4h" => Ok(Timeframe::H4),
            "6H" | "6h" => Ok(Timeframe::H6),
            "12H" | "12h" => Ok(Timeframe::H12),
            "1Dutc" | "1d" => Ok(Timeframe::D1),
            "1W" | "1w" => Ok(Timeframe::W1),
            "1M" => Ok(Timeframe::MN1),
            _ => Err(format!("Unknown timeframe: {}", s)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 提供test策略typefrom字符串的集中实现，避免回测策略调用方重复处理相同细节。
    fn test_strategy_type_from_str() {
        use std::str::FromStr;
        assert_eq!(StrategyType::from_str("vegas"), Ok(StrategyType::Vegas));
        assert_eq!(
            StrategyType::from_str("vegas_universal_4h"),
            Ok(StrategyType::VegasUniversal4h)
        );
        assert_eq!(
            StrategyType::from_str("vegas_macd_divergence_fresh_choch_reversal_4h_research"),
            Ok(StrategyType::VegasMacdDivergenceFreshChoch4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_macd_trend_reset_fresh_bos_4h_research"),
            Ok(StrategyType::VegasMacdTrendResetFreshBos4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_liquidity_sweep_first_retest_4h_research"),
            Ok(StrategyType::VegasLiquiditySweepFirstRetest4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_lower_liquidity_sweep_confirmation_4h_research"),
            Ok(StrategyType::VegasLowerLiquiditySweepConfirmation4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_failed_breakdown_close_reentry_long_4h_research"),
            Ok(StrategyType::VegasFailedBreakdownCloseReentryLong4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_failed_breakdown_higher_low_breakout_long_4h_research"),
            Ok(StrategyType::VegasFailedBreakdownHigherLowBreakoutLong4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_upper_sweep_confirmation_low_break_short_4h_research"),
            Ok(StrategyType::VegasUpperSweepConfirmationLowBreakShort4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_high_volume_compressed_sweep_4h_research"),
            Ok(StrategyType::VegasHighVolumeCompressedSweep4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_high_volume_compressed_delayed_retest_4h_research"),
            Ok(StrategyType::VegasHighVolumeCompressedDelayedRetest4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_volume_increase_compressed_sweep_4h_research"),
            Ok(StrategyType::VegasVolumeIncreaseCompressedSweep4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_compressed_range_breakout_short_4h_research"),
            Ok(StrategyType::VegasCompressedRangeBreakoutShort4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_lower_sweep_confirmation_high_break_long_4h_research"),
            Ok(StrategyType::VegasLowerSweepConfirmationHighBreakLong4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_ema_tunnel_retest_confirmation_4h_research"),
            Ok(StrategyType::VegasEmaTunnelRetestConfirmation4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_volume_profile_value_area_retest_4h_research"),
            Ok(StrategyType::VegasVolumeProfileValueAreaRetest4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_volume_profile_value_area_breakout_4h_research"),
            Ok(StrategyType::VegasVolumeProfileValueAreaBreakout4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_volume_profile_failed_auction_short_4h_research"),
            Ok(StrategyType::VegasVolumeProfileFailedAuctionShort4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_donchian_volume_breakout_4h_research"),
            Ok(StrategyType::VegasDonchianVolumeBreakout4hResearch)
        );
        assert_eq!(
            StrategyType::from_str("vegas_donchian_breakout_acceptance_4h_research"),
            Ok(StrategyType::VegasDonchianBreakoutAcceptance4hResearch)
        );
        assert_eq!(StrategyType::from_str("NWE"), Ok(StrategyType::Nwe));
        assert_eq!(
            StrategyType::from_str("market_velocity"),
            Ok(StrategyType::MarketVelocity)
        );
        assert_eq!(
            StrategyType::from_str("btc_eth_liquidity_scalper_v1"),
            Ok(StrategyType::BtcEthLiquidityScalper)
        );
        assert_eq!(
            StrategyType::from_str("exhaustion_fade_short_v1"),
            Ok(StrategyType::BearShortStack)
        );
        assert_eq!(
            StrategyType::from_str("eth_volume_reversal_5m_v1_research"),
            Ok(StrategyType::EthVolumeReversal5mV1Research)
        );
        assert!(StrategyType::from_str("unknown").is_err());
    }
    #[test]
    fn strategy_type_round_trips_eth_volume_reversal_research_key() {
        use std::str::FromStr;
        assert_eq!(
            StrategyType::EthVolumeReversal5mV1Research.as_str(),
            "eth_volume_reversal_5m_v1_research"
        );
        assert_eq!(
            StrategyType::from_str("eth_volume_reversal_5m_v1_research").unwrap(),
            StrategyType::EthVolumeReversal5mV1Research
        );
    }
    #[test]
    fn strategy_type_round_trips_btc_volume_reversal_research_key() {
        use std::str::FromStr;
        assert_eq!(
            StrategyType::BtcVolumeReversalDual5mV1Research.as_str(),
            "btc_volume_reversal_dual_5m_v1_research"
        );
        assert_eq!(
            StrategyType::from_str("btc_volume_reversal_dual_5m_v1_research").unwrap(),
            StrategyType::BtcVolumeReversalDual5mV1Research
        );
    }
    #[test]
    fn strategy_type_round_trips_btc_volume_reversal_hybrid_research_key() {
        use std::str::FromStr;
        assert_eq!(
            StrategyType::BtcVolumeReversalHybrid5mV1Research.as_str(),
            "btc_volume_reversal_hybrid_5m_v1_research"
        );
        assert_eq!(
            StrategyType::from_str("btc_volume_reversal_hybrid_5m_v1_research").unwrap(),
            StrategyType::BtcVolumeReversalHybrid5mV1Research
        );
    }
    #[test]
    fn test_timeframe_conversion() {
        use std::str::FromStr;
        assert_eq!(Timeframe::from_str("1H"), Ok(Timeframe::H1));
        assert_eq!(Timeframe::H1.to_minutes(), 60);
        assert_eq!(Timeframe::D1.to_minutes(), 1440);
    }
}
