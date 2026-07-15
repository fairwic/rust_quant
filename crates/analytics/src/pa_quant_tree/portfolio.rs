use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 共享组合回放的冻结风险约束，所有比例均以当前权益为基准。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioRiskPolicy {
    /// 初始权益，单位为 U。
    pub initial_equity: f64,
    /// 单笔目标风险占权益比例；v1 固定为 0.5%。
    pub target_risk_fraction: f64,
    /// 所有未结算交易的最大风险占权益比例；v1 为 2%。
    pub max_open_risk_fraction: f64,
    /// 单笔最大名义价值占权益比例；v1 为 25%。
    pub max_trade_notional_fraction: f64,
    /// 全部未结算交易最大名义价值占权益比例；v1 为 100%。
    pub max_total_notional_fraction: f64,
}

/// 一个已经按成本结算、可用于资金回放的候选交易路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioTradeCandidate {
    /// 与研究样本对应的唯一候选 ID。
    pub candidate_id: String,
    /// 交易对；同一交易对不能重叠持仓。
    pub symbol: String,
    /// 实际下一棒开盘入场的 Unix 毫秒时间戳。
    pub entry_ts: i64,
    /// 交易路径结束的 Unix 毫秒时间戳。
    pub exit_ts: i64,
    /// 实际入场价格，用于由止损距离推导名义价值。
    pub entry_price: f64,
    /// 冻结的结构止损价格。
    pub stop_price: f64,
    /// 扣除手续费、滑点和资金费率后的 R 倍数。
    pub net_r: f64,
}

/// 一个被组合约束拒绝的候选及原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioSkippedTrade {
    /// 被拒绝的候选 ID。
    pub candidate_id: String,
    /// 稳定拒绝代码，供报告聚合。
    pub reason: PortfolioSkipReason,
}

/// 组合层拒绝原因，不改变策略层候选或其 shadow outcome。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortfolioSkipReason {
    /// 入场、止损、时间或 R 数值不满足回放合同。
    InvalidTrade,
    /// 已有同交易对未结算持仓。
    SymbolAlreadyOpen,
    /// 当前权益已耗尽，无法分配正风险。
    NoRiskBudget,
}

/// 已按共享权益结算的交易结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioSettledTrade {
    /// 原始候选 ID。
    pub candidate_id: String,
    /// 交易对。
    pub symbol: String,
    /// 组合实际承担的 U 风险。
    pub allocated_risk: f64,
    /// 入场时实际分配的 U 名义价值。
    pub allocated_notional: f64,
    /// 成本后实际 U 损益，等于 allocated_risk × net_r。
    pub pnl: f64,
    /// 结算后的共享组合权益，单位为 U。
    pub equity_after_settlement: f64,
}

/// 共享组合权益回放结果；最大回撤只从这条权益曲线计算。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioReplay {
    /// 起始权益，单位为 U。
    pub initial_equity: f64,
    /// 所有持仓结算后的最终权益，单位为 U。
    pub final_equity: f64,
    /// 共享权益曲线的最大回撤比例。
    pub max_drawdown_ratio: f64,
    /// 已实际分配并结算的交易。
    pub settled_trades: Vec<PortfolioSettledTrade>,
    /// 受到组合约束拒绝的候选。
    pub skipped_trades: Vec<PortfolioSkippedTrade>,
}

/// 使用方案定义的 100U 初始权益和组合限制。
pub fn default_portfolio_risk_policy() -> PortfolioRiskPolicy {
    PortfolioRiskPolicy {
        initial_equity: 100.0,
        target_risk_fraction: 0.005,
        max_open_risk_fraction: 0.02,
        max_trade_notional_fraction: 0.25,
        max_total_notional_fraction: 1.0,
    }
}

/// 在同一共享权益曲线上回放所有交易，并按同刻信号同比缩放风险和名义价值。
pub fn replay_shared_portfolio(
    candidates: &[PortfolioTradeCandidate],
    policy: &PortfolioRiskPolicy,
) -> Result<PortfolioReplay, String> {
    validate_policy(policy)?;
    let mut ordered = candidates.to_vec();
    ordered.sort_by_key(|trade| (trade.entry_ts, trade.candidate_id.clone()));
    let mut equity = policy.initial_equity;
    let mut peak = equity;
    let mut max_drawdown_ratio: f64 = 0.0;
    let mut open = Vec::new();
    let mut settled_trades = Vec::new();
    let mut skipped_trades = Vec::new();
    let mut index = 0;

    while index < ordered.len() {
        let entry_ts = ordered[index].entry_ts;
        settle_until(
            entry_ts,
            &mut open,
            &mut settled_trades,
            &mut equity,
            &mut peak,
            &mut max_drawdown_ratio,
        );
        let group_end = ordered[index..]
            .iter()
            .position(|trade| trade.entry_ts != entry_ts)
            .map(|offset| index + offset)
            .unwrap_or(ordered.len());
        allocate_group(
            &ordered[index..group_end],
            policy,
            equity,
            &mut open,
            &mut skipped_trades,
        );
        index = group_end;
    }
    settle_until(
        i64::MAX,
        &mut open,
        &mut settled_trades,
        &mut equity,
        &mut peak,
        &mut max_drawdown_ratio,
    );
    Ok(PortfolioReplay {
        initial_equity: policy.initial_equity,
        final_equity: equity,
        max_drawdown_ratio,
        settled_trades,
        skipped_trades,
    })
}

#[derive(Debug, Clone)]
struct OpenTrade {
    candidate: PortfolioTradeCandidate,
    allocated_risk: f64,
    allocated_notional: f64,
}

fn validate_policy(policy: &PortfolioRiskPolicy) -> Result<(), String> {
    if !policy.initial_equity.is_finite()
        || policy.initial_equity <= 0.0
        || [
            policy.target_risk_fraction,
            policy.max_open_risk_fraction,
            policy.max_trade_notional_fraction,
            policy.max_total_notional_fraction,
        ]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err("portfolio risk policy must contain positive finite values".to_owned());
    }
    Ok(())
}

fn settle_until(
    timestamp: i64,
    open: &mut Vec<OpenTrade>,
    settled: &mut Vec<PortfolioSettledTrade>,
    equity: &mut f64,
    peak: &mut f64,
    max_drawdown_ratio: &mut f64,
) {
    let mut remaining = Vec::with_capacity(open.len());
    for position in open.drain(..) {
        if position.candidate.exit_ts <= timestamp {
            let pnl = position.allocated_risk * position.candidate.net_r;
            *equity += pnl;
            *peak = peak.max(*equity);
            *max_drawdown_ratio = (*max_drawdown_ratio).max((*peak - *equity) / *peak);
            settled.push(PortfolioSettledTrade {
                candidate_id: position.candidate.candidate_id,
                symbol: position.candidate.symbol,
                allocated_risk: position.allocated_risk,
                allocated_notional: position.allocated_notional,
                pnl,
                equity_after_settlement: *equity,
            });
        } else {
            remaining.push(position);
        }
    }
    *open = remaining;
}

fn allocate_group(
    group: &[PortfolioTradeCandidate],
    policy: &PortfolioRiskPolicy,
    equity: f64,
    open: &mut Vec<OpenTrade>,
    skipped: &mut Vec<PortfolioSkippedTrade>,
) {
    if equity <= 0.0 {
        skipped.extend(group.iter().map(|trade| PortfolioSkippedTrade {
            candidate_id: trade.candidate_id.clone(),
            reason: PortfolioSkipReason::NoRiskBudget,
        }));
        return;
    }
    let open_symbols: HashSet<_> = open
        .iter()
        .map(|position| position.candidate.symbol.as_str())
        .collect();
    let mut group_symbols = HashSet::new();
    let eligible: Vec<_> = group
        .iter()
        .filter_map(|trade| {
            if !is_valid_trade(trade) {
                skipped.push(PortfolioSkippedTrade {
                    candidate_id: trade.candidate_id.clone(),
                    reason: PortfolioSkipReason::InvalidTrade,
                });
                None
            } else if open_symbols.contains(trade.symbol.as_str())
                || !group_symbols.insert(trade.symbol.as_str())
            {
                skipped.push(PortfolioSkippedTrade {
                    candidate_id: trade.candidate_id.clone(),
                    reason: PortfolioSkipReason::SymbolAlreadyOpen,
                });
                None
            } else {
                Some(trade)
            }
        })
        .collect();
    let current_risk: f64 = open.iter().map(|position| position.allocated_risk).sum();
    let current_notional: f64 = open
        .iter()
        .map(|position| position.allocated_notional)
        .sum();
    let available_risk = (equity * policy.max_open_risk_fraction - current_risk).max(0.0);
    let available_notional =
        (equity * policy.max_total_notional_fraction - current_notional).max(0.0);
    let requested: Vec<_> = eligible
        .into_iter()
        .map(|trade| requested_allocation(trade, policy, equity))
        .collect();
    let requested_risk: f64 = requested.iter().map(|(_, risk, _)| risk).sum();
    let requested_notional: f64 = requested.iter().map(|(_, _, notional)| notional).sum();
    let scale = if requested_risk == 0.0 || requested_notional == 0.0 {
        0.0
    } else {
        (available_risk / requested_risk)
            .min(available_notional / requested_notional)
            .min(1.0)
    };
    for (trade, risk, notional) in requested {
        if scale == 0.0 {
            skipped.push(PortfolioSkippedTrade {
                candidate_id: trade.candidate_id.clone(),
                reason: PortfolioSkipReason::NoRiskBudget,
            });
        } else {
            open.push(OpenTrade {
                candidate: trade.clone(),
                allocated_risk: risk * scale,
                allocated_notional: notional * scale,
            });
        }
    }
}

fn is_valid_trade(trade: &PortfolioTradeCandidate) -> bool {
    trade.exit_ts >= trade.entry_ts
        && !trade.candidate_id.is_empty()
        && !trade.symbol.is_empty()
        && [trade.entry_price, trade.stop_price, trade.net_r]
            .iter()
            .all(|value| value.is_finite())
        && trade.entry_price > 0.0
        && (trade.entry_price - trade.stop_price).abs() > f64::EPSILON
}

fn requested_allocation<'a>(
    trade: &'a PortfolioTradeCandidate,
    policy: &PortfolioRiskPolicy,
    equity: f64,
) -> (&'a PortfolioTradeCandidate, f64, f64) {
    let stop_distance = (trade.entry_price - trade.stop_price).abs();
    let risk_cap_by_notional =
        equity * policy.max_trade_notional_fraction * stop_distance / trade.entry_price;
    let risk = (equity * policy.target_risk_fraction).min(risk_cap_by_notional);
    (trade, risk, risk * trade.entry_price / stop_distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(
        id: &str,
        symbol: &str,
        entry_ts: i64,
        exit_ts: i64,
        net_r: f64,
    ) -> PortfolioTradeCandidate {
        PortfolioTradeCandidate {
            candidate_id: id.to_owned(),
            symbol: symbol.to_owned(),
            entry_ts,
            exit_ts,
            entry_price: 100.0,
            stop_price: 99.0,
            net_r,
        }
    }

    #[test]
    fn simultaneous_signals_share_the_risk_budget_proportionally() {
        let policy = PortfolioRiskPolicy {
            max_open_risk_fraction: 0.005,
            ..default_portfolio_risk_policy()
        };
        let replay = replay_shared_portfolio(
            &[trade("a", "BTC", 1, 2, 1.0), trade("b", "ETH", 1, 2, 1.0)],
            &policy,
        )
        .unwrap();
        assert_eq!(replay.settled_trades.len(), 2);
        assert!((replay.settled_trades[0].allocated_risk - 0.25).abs() < 1e-12);
        assert!((replay.settled_trades[1].allocated_risk - 0.25).abs() < 1e-12);
    }

    #[test]
    fn same_symbol_overlap_is_rejected_and_drawdown_uses_shared_equity() {
        let replay = replay_shared_portfolio(
            &[trade("a", "BTC", 1, 4, -1.0), trade("b", "BTC", 2, 3, 1.0)],
            &default_portfolio_risk_policy(),
        )
        .unwrap();
        assert_eq!(replay.settled_trades.len(), 1);
        assert_eq!(
            replay.skipped_trades[0].reason,
            PortfolioSkipReason::SymbolAlreadyOpen
        );
        // 100U 入场、1U 止损时，25U 单笔名义上限把风险压缩为 0.25U。
        assert!((replay.max_drawdown_ratio - 0.0025).abs() < 1e-12);
    }
}
