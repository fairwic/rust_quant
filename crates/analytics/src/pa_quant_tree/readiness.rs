use rust_quant_strategies::implementations::pa_quant_tree::PaStrategyKey;
use serde::{Deserialize, Serialize};

/// 正式 PA 研究每个市场/周期最低需要的已确认 K 线数量；仅是研究准入线，不是统计证明。
pub const PA_MIN_RESEARCH_CANDLES: usize = 1_000;

/// 一个市场与周期的只读数据覆盖摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDataCoverage {
    /// 标准化交易对，例如 BTC-USDT-SWAP。
    pub symbol: String,
    /// K 线周期，单位为分钟。
    pub timeframe_minutes: u32,
    /// 已确认 K 线数量。
    pub confirmed_candles: usize,
    /// 最早已确认 K 线的 Unix 毫秒时间戳。
    pub first_ts: i64,
    /// 最新已确认 K 线的 Unix 毫秒时间戳。
    pub last_ts: i64,
}

/// 正式研究准备度的稳定阻塞原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchReadinessBlocker {
    /// Meta-filter 不使用独立 PA 周期数据 Gate。
    MetaFilterRequiresVegasCandidates,
    /// 目标周期缺少 BTC、ETH 或其他币种的合格覆盖。
    MissingMarketTierCoverage,
    /// 市场虽然存在但已确认 K 线数量低于研究准入线。
    InsufficientConfirmedCandles,
    /// 合格市场之间不存在共同历史窗口。
    NoCommonTimeWindow,
}

/// 研究开始前的多币种、周期和公共时间窗口检查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchReadinessReport {
    /// 被检查的独立 PA 策略标识。
    pub strategy_key: PaStrategyKey,
    /// 策略固定要求的 K 线周期，单位为分钟。
    pub timeframe_minutes: Option<u32>,
    /// 参与公共窗口计算的合格市场。
    pub eligible_symbols: Vec<String>,
    /// 合格市场共同窗口的起点；未准备好时为空。
    pub common_start_ts: Option<i64>,
    /// 合格市场共同窗口的终点；未准备好时为空。
    pub common_end_ts: Option<i64>,
    /// 阻塞列表为空时才允许创建正式训练批次。
    pub blockers: Vec<ResearchReadinessBlocker>,
}

impl ResearchReadinessReport {
    /// 返回是否满足正式训练的最低数据准备度，不代表 OOS 或 Paper 已通过。
    pub fn is_ready(&self) -> bool {
        self.blockers.is_empty()
    }
}

/// 校验独立 PA 策略是否具备 BTC、ETH、其他币种三层和公共时间窗口的研究数据。
pub fn assess_research_readiness(
    strategy_key: PaStrategyKey,
    coverage: &[MarketDataCoverage],
) -> ResearchReadinessReport {
    let Some(timeframe_minutes) = strategy_key.timeframe_minutes() else {
        return ResearchReadinessReport {
            strategy_key,
            timeframe_minutes: None,
            eligible_symbols: vec![],
            common_start_ts: None,
            common_end_ts: None,
            blockers: vec![ResearchReadinessBlocker::MetaFilterRequiresVegasCandidates],
        };
    };
    let eligible: Vec<_> = coverage
        .iter()
        .filter(|item| {
            item.timeframe_minutes == timeframe_minutes
                && item.confirmed_candles >= PA_MIN_RESEARCH_CANDLES
                && item.first_ts <= item.last_ts
        })
        .collect();
    let has_btc = eligible.iter().any(|item| item.symbol.starts_with("BTC-"));
    let has_eth = eligible.iter().any(|item| item.symbol.starts_with("ETH-"));
    let has_other = eligible
        .iter()
        .any(|item| !item.symbol.starts_with("BTC-") && !item.symbol.starts_with("ETH-"));
    let mut blockers = Vec::new();
    let target_timeframe: Vec<_> = coverage
        .iter()
        .filter(|item| item.timeframe_minutes == timeframe_minutes)
        .collect();
    let insufficient_required_tier = (!has_btc
        && target_timeframe
            .iter()
            .any(|item| item.symbol.starts_with("BTC-")))
        || (!has_eth
            && target_timeframe
                .iter()
                .any(|item| item.symbol.starts_with("ETH-")))
        || (!has_other
            && target_timeframe
                .iter()
                .any(|item| !item.symbol.starts_with("BTC-") && !item.symbol.starts_with("ETH-")));
    if insufficient_required_tier {
        blockers.push(ResearchReadinessBlocker::InsufficientConfirmedCandles);
    }
    if !has_btc || !has_eth || !has_other {
        blockers.push(ResearchReadinessBlocker::MissingMarketTierCoverage);
    }
    let common_start_ts = eligible.iter().map(|item| item.first_ts).max();
    let common_end_ts = eligible.iter().map(|item| item.last_ts).min();
    if !matches!(common_start_ts.zip(common_end_ts), Some((start, end)) if start <= end) {
        blockers.push(ResearchReadinessBlocker::NoCommonTimeWindow);
    }
    ResearchReadinessReport {
        strategy_key,
        timeframe_minutes: Some(timeframe_minutes),
        eligible_symbols: eligible.iter().map(|item| item.symbol.clone()).collect(),
        common_start_ts,
        common_end_ts,
        blockers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(symbol: &str, candles: usize, first_ts: i64, last_ts: i64) -> MarketDataCoverage {
        MarketDataCoverage {
            symbol: symbol.to_owned(),
            timeframe_minutes: 15,
            confirmed_candles: candles,
            first_ts,
            last_ts,
        }
    }

    #[test]
    fn requires_btc_eth_other_and_a_common_window() {
        let complete = assess_research_readiness(
            PaStrategyKey::PaTrend15m,
            &[
                coverage("BTC-USDT-SWAP", 1_000, 1, 3_000),
                coverage("ETH-USDT-SWAP", 1_000, 2, 3_000),
                coverage("BCH-USDT-SWAP", 1_000, 3, 3_000),
                coverage("SOL-USDT-SWAP", 200, 1, 3_000),
            ],
        );
        assert!(complete.is_ready());
        assert_eq!(complete.common_start_ts, Some(3));
        let incomplete = assess_research_readiness(
            PaStrategyKey::PaTrend15m,
            &[
                coverage("BTC-USDT-SWAP", 1_000, 1, 3_000),
                coverage("ETH-USDT-SWAP", 500, 1, 3_000),
            ],
        );
        assert!(!incomplete.is_ready());
        assert!(incomplete
            .blockers
            .contains(&ResearchReadinessBlocker::InsufficientConfirmedCandles));
    }
}
