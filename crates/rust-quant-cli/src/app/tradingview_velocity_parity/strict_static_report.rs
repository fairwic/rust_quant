//! TradingView parity 严格静态币池的纯报告计算。
//!
//! 本模块只消费已经完成的 [`ReplayReport`]，不依赖币池 manifest、数据库或加载器。
//! 60/60 数据门禁仍由调用方先完成；这里负责验证零成本与压力成本路径一致，并生成
//! 可安全序列化的指标、收益集中度和 60 分钟同向事件簇。

use super::{BlockedSignal, Direction, Metrics, ReplayReport, Trade};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// 标准压力口径的单边手续费，单位为基点。
pub const STRICT_STATIC_FEE_BPS_PER_SIDE: f64 = 5.0;
/// 标准压力口径的单边滑点，单位为基点。
pub const STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE: f64 = 3.0;
/// 同方向市场事件使用相邻信号不超过 60 分钟的 single-linkage 规则。
pub const STRICT_STATIC_EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;

/// 避免把无限 Profit Factor 写成非法 JSON 数值的安全表示。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SerializableProfitFactor {
    /// 有有限亏损分母时的 Profit Factor；无有限值时为 `None`。
    pub value: Option<f64>,
    /// `true` 表示有正收益但没有亏损，因此 Profit Factor 为正无穷。
    pub is_infinite: bool,
}

/// 一次回放的可安全序列化指标。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SerializableMetricSnapshot {
    /// 已平仓交易数；样本末未平仓仓位不计入。
    pub trades: usize,
    /// 净收益大于零的已平仓交易数。
    pub wins: usize,
    /// 净收益小于零的已平仓交易数。
    pub losses: usize,
    /// 固定一单位仓位的净价格收益。
    pub net_pnl: f64,
    /// 所有正净收益之和。
    pub gross_profit: f64,
    /// 所有负净收益绝对值之和。
    pub gross_loss: f64,
    /// 可安全写入 JSON 的 Profit Factor。
    pub profit_factor: SerializableProfitFactor,
    /// 已平仓交易胜率，单位为百分比。
    pub win_rate_percent: f64,
    /// 以入场时初始止损风险为分母的平均净 R。
    pub average_net_r: f64,
    /// 只按已平仓权益更新的最大回撤。
    pub closed_equity_max_drawdown: f64,
    /// 包含单币持仓期间不利价格路径的 TradingView 口径最大回撤。
    pub max_drawdown: f64,
}

/// 单币回放的报告摘要；blocked signal 只统计正式评价窗口。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SerializableSymbolSnapshot {
    /// OKX 原始交易对标识。
    pub symbol: String,
    /// 本次回放实际消费的冻结价格步长。
    pub tick_size: f64,
    /// 已平仓交易指标。
    pub metrics: SerializableMetricSnapshot,
    /// 该币种所有已平仓交易净 R 之和。
    pub net_r: f64,
    /// 正式评价窗口内被策略显式阻断或冲突取消的信号数。
    pub blocked_signal_count: usize,
    /// `true` 表示评价末仍有未平仓仓位，且未被强制结算。
    pub open_position_at_end: bool,
    /// `true` 表示末根确认棒产生信号，但窗口内没有下一根开盘可成交。
    pub pending_entry_at_end: bool,
}

/// 多币独立单元回放的汇总；该结构不宣称统一资金或容量约束组合。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SerializableAggregateSnapshot {
    /// 纳入汇总的回放报告数量；symbol 唯一性由严格币池门禁保证。
    pub symbols: usize,
    /// 所有币种的已平仓交易数。
    pub trades: usize,
    /// 所有币种的盈利交易数。
    pub wins: usize,
    /// 所有币种的亏损交易数。
    pub losses: usize,
    /// 固定一单位仓位的跨币净价格收益之和。
    pub net_pnl: f64,
    /// 所有已平仓交易净 R 之和。
    pub net_r: f64,
    /// 所有正净收益之和。
    pub gross_profit: f64,
    /// 所有负净收益绝对值之和。
    pub gross_loss: f64,
    /// 可安全写入 JSON 的跨币 Profit Factor。
    pub profit_factor: SerializableProfitFactor,
    /// 所有已平仓交易胜率，单位为百分比。
    pub win_rate_percent: f64,
    /// 所有已平仓交易的平均净 R。
    pub average_net_r: f64,
    /// 按退出时间顺序累计固定一单位收益得到的已平仓权益最大回撤。
    pub chronological_closed_equity_max_drawdown: f64,
    /// 所有单币回放中最大的 TradingView 口径盘中回撤。
    pub max_single_symbol_intrabar_drawdown: f64,
    /// 已平仓净收益为正的币种数。
    pub profitable_symbols: usize,
    /// 已平仓净收益为负的币种数。
    pub losing_symbols: usize,
    /// 已平仓净收益为零的币种数。
    pub flat_symbols: usize,
    /// 评价末仍持仓的币种数。
    pub open_positions_at_end: usize,
    /// 评价末仍等待下一根开盘的币种数。
    pub pending_entries_at_end: usize,
}

/// 一笔正收益交易在集中度排序中的稳定身份与贡献。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RankedTradeContribution {
    /// 交易所属 symbol。
    pub symbol: String,
    /// 交易方向。
    pub direction: Direction,
    /// 产生信号的 K 线开盘时间，Unix 毫秒。
    pub signal_time_ms: i64,
    /// 下一根开盘成交时间，Unix 毫秒。
    pub entry_time_ms: i64,
    /// 平仓时间，Unix 毫秒。
    pub exit_time_ms: i64,
    /// 扣除当前报告成本后的净 R。
    pub net_r: f64,
    /// 固定一单位仓位的净价格收益。
    pub net_pnl: f64,
}

/// 一个正收益币种在集中度排序中的聚合贡献。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RankedSymbolContribution {
    /// 聚合使用的 symbol。
    pub symbol: String,
    /// 该币种已平仓交易数。
    pub trades: usize,
    /// 该币种所有已平仓交易净 R 之和。
    pub net_r: f64,
    /// 该币种固定一单位仓位净价格收益之和。
    pub net_pnl: f64,
}

/// 移除预先指定的头部贡献后，剩余交易集合的指标。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemovalMetricSnapshot {
    /// 移除后剩余的已平仓交易数。
    pub remaining_trades: usize,
    /// 移除后剩余交易净 R 之和。
    pub net_r: f64,
    /// 移除后固定一单位仓位净价格收益之和。
    pub net_pnl: f64,
    /// 移除后所有正净收益之和。
    pub gross_profit: f64,
    /// 移除后所有负净收益绝对值之和。
    pub gross_loss: f64,
    /// 移除后的可安全序列化 Profit Factor。
    pub profit_factor: SerializableProfitFactor,
}

/// 正收益交易与正收益币种的 Top1/Top5 集中度审计。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConcentrationAudit {
    /// 净 R 大于零的交易数。
    pub profitable_trade_count: usize,
    /// 所有正收益交易净 R 之和，作为交易集中度分母。
    pub gross_positive_trade_net_r: f64,
    /// 净 R 排名第一的正收益交易。
    pub top1_trade: Option<RankedTradeContribution>,
    /// 按净 R 降序稳定排序的前五笔正收益交易；不足五笔时保留全部。
    pub top5_trades: Vec<RankedTradeContribution>,
    /// Top1 交易占全部正收益交易净 R 的百分比。
    pub top1_trade_positive_net_r_share_percent: Option<f64>,
    /// Top5 交易占全部正收益交易净 R 的百分比。
    pub top5_trade_positive_net_r_share_percent: Option<f64>,
    /// 移除 Top5 正收益交易后的剩余结果。
    pub after_removing_top5_trades: RemovalMetricSnapshot,
    /// 聚合净 R 大于零的币种数。
    pub profitable_symbol_count: usize,
    /// 所有正收益币种净 R 之和，作为币种集中度分母。
    pub gross_positive_symbol_net_r: f64,
    /// 聚合净 R 排名第一的正收益币种。
    pub top1_symbol: Option<RankedSymbolContribution>,
    /// 按聚合净 R 降序稳定排序的前五个正收益币种。
    pub top5_symbols: Vec<RankedSymbolContribution>,
    /// Top1 币种占全部正收益币种净 R 的百分比。
    pub top1_symbol_positive_net_r_share_percent: Option<f64>,
    /// Top5 币种占全部正收益币种净 R 的百分比。
    pub top5_symbol_positive_net_r_share_percent: Option<f64>,
    /// 移除 Top5 正收益币种及其全部交易后的剩余结果。
    pub after_removing_top5_symbols: RemovalMetricSnapshot,
}

/// 一个 60 分钟同向 single-linkage 事件簇的摘要。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventClusterSummary {
    /// 事件簇方向；多空始终分开聚类。
    pub direction: Direction,
    /// 事件簇首笔交易的信号时间，Unix 毫秒。
    pub first_signal_time_ms: i64,
    /// 事件簇末笔交易的信号时间，Unix 毫秒。
    pub last_signal_time_ms: i64,
    /// 该事件簇包含的已平仓交易数。
    pub trades: usize,
    /// 该事件簇净 R 之和。
    pub net_r: f64,
    /// 该事件簇固定一单位仓位净价格收益之和。
    pub net_pnl: f64,
}

/// 60 分钟同向 single-linkage 有效市场事件审计。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventClusterAudit {
    /// 聚类窗口，固定为 3,600,000 毫秒。
    pub cluster_window_ms: i64,
    /// 聚类语义；相邻事件间隔不超过窗口即链接，不要求簇首尾也在一个窗口内。
    pub linkage_rule: &'static str,
    /// 参与聚类的全部已平仓交易数。
    pub raw_trade_count: usize,
    /// 做多交易数。
    pub long_trade_count: usize,
    /// 做空交易数。
    pub short_trade_count: usize,
    /// 多空合计事件簇数。
    pub total_clusters: usize,
    /// 做多事件簇数。
    pub long_clusters: usize,
    /// 做空事件簇数。
    pub short_clusters: usize,
    /// 交易数最多的事件簇；并列时按首个信号时间和方向稳定选择。
    pub largest_trade_cluster: Option<EventClusterSummary>,
    /// 最大交易数事件簇占全部交易的百分比。
    pub largest_trade_cluster_share_percent: Option<f64>,
    /// 最大交易数事件簇占全部正收益事件簇净 R 的百分比；该簇不盈利时为空。
    pub largest_trade_cluster_positive_net_r_share_percent: Option<f64>,
    /// 净 R 最大的正收益事件簇。
    pub largest_positive_net_r_cluster: Option<EventClusterSummary>,
    /// 最大正收益事件簇占所有正收益事件簇净 R 的百分比。
    pub largest_positive_net_r_cluster_share_percent: Option<f64>,
    /// 全部事件簇明细，按方向、首个信号时间稳定排列。
    pub clusters: Vec<EventClusterSummary>,
}

#[derive(Debug, Clone, Copy)]
struct TradeRef<'a> {
    report_index: usize,
    trade_index: usize,
    symbol: &'a str,
    trade: &'a Trade,
}

#[derive(Debug, Default)]
struct SymbolAccumulator {
    trades: usize,
    net_r: f64,
    net_pnl: f64,
}

/// 验证零成本和压力成本回放的执行路径完全相同。
///
/// 成本模式只允许改变报告成本字段、汇总指标以及每笔交易的 `net_pnl`/`net_r`；
/// 信号、成交、保护价、退出原因和未完成状态发生任何漂移都返回错误。
pub fn assert_cost_path_parity(zero: &ReplayReport, stress: &ReplayReport) -> Result<()> {
    if !same_f64(zero.fee_bps_per_side, 0.0)
        || !same_f64(zero.slippage_bps_per_side, 0.0)
        || !same_f64(stress.fee_bps_per_side, STRICT_STATIC_FEE_BPS_PER_SIDE)
        || !same_f64(
            stress.slippage_bps_per_side,
            STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE,
        )
    {
        bail!("{} 的零成本或压力成本口径不是冻结值", zero.symbol);
    }
    if zero.strategy_version != stress.strategy_version
        || zero.pine_source_fnv1a32 != stress.pine_source_fnv1a32
        || zero.symbol != stress.symbol
        || zero.tick_size.to_bits() != stress.tick_size.to_bits()
        || zero.evaluation_start_ms != stress.evaluation_start_ms
        || zero.evaluation_end_ms != stress.evaluation_end_ms
        || zero.open_position_at_end != stress.open_position_at_end
        || zero.pending_entry_at_end != stress.pending_entry_at_end
    {
        bail!("{} 的零成本与压力成本回放身份或末态发生漂移", zero.symbol);
    }
    if zero.blocked_signals.len() != stress.blocked_signals.len() {
        bail!("{} 的零成本与压力成本 blocked signal 数量不同", zero.symbol);
    }
    for (index, (left, right)) in zero
        .blocked_signals
        .iter()
        .zip(&stress.blocked_signals)
        .enumerate()
    {
        if !blocked_signal_path_eq(left, right) {
            bail!(
                "{} 的第 {} 个 blocked signal 在成本模式下发生漂移",
                zero.symbol,
                index + 1
            );
        }
    }
    if zero.trades.len() != stress.trades.len() {
        bail!("{} 的零成本与压力成本交易数量不同", zero.symbol);
    }
    for (index, (left, right)) in zero.trades.iter().zip(&stress.trades).enumerate() {
        if !trade_path_eq(left, right) {
            bail!(
                "{} 的第 {} 笔交易路径在成本模式下发生漂移",
                zero.symbol,
                index + 1
            );
        }
    }
    Ok(())
}

/// 把单次回放指标转换为不会序列化非有限 PF 的稳定结构。
pub fn metric_snapshot(metrics: &Metrics) -> SerializableMetricSnapshot {
    SerializableMetricSnapshot {
        trades: metrics.trades,
        wins: metrics.wins,
        losses: metrics.losses,
        net_pnl: metrics.net_pnl,
        gross_profit: metrics.gross_profit,
        gross_loss: metrics.gross_loss,
        profit_factor: serializable_profit_factor(metrics.gross_profit, metrics.gross_loss),
        win_rate_percent: metrics.win_rate_percent,
        average_net_r: metrics.average_net_r,
        closed_equity_max_drawdown: metrics.closed_equity_max_drawdown,
        max_drawdown: metrics.max_drawdown,
    }
}

/// 生成一个单币摘要，并把预热段 blocked signal 排除在正式计数之外。
pub fn symbol_snapshot(
    report: &ReplayReport,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
) -> SerializableSymbolSnapshot {
    SerializableSymbolSnapshot {
        symbol: report.symbol.clone(),
        tick_size: report.tick_size,
        metrics: metric_snapshot(&report.metrics),
        net_r: report.trades.iter().map(|trade| trade.net_r).sum(),
        blocked_signal_count: blocked_signal_count_in_window(
            report,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        open_position_at_end: report.open_position_at_end,
        pending_entry_at_end: report.pending_entry_at_end,
    }
}

/// 只统计正式评价闭区间内的 blocked signal，避免 60 天指标预热污染诊断数量。
pub fn blocked_signal_count_in_window(
    report: &ReplayReport,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
) -> usize {
    blocked_signals_in_window(report, evaluation_start_ms, evaluation_end_ms).len()
}

/// 返回正式评价闭区间内的 blocked signal 引用，供报告明细和数量使用同一过滤口径。
pub fn blocked_signals_in_window(
    report: &ReplayReport,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
) -> Vec<&BlockedSignal> {
    report
        .blocked_signals
        .iter()
        .filter(|signal| (evaluation_start_ms..=evaluation_end_ms).contains(&signal.signal_time_ms))
        .collect()
}

/// 汇总多个独立单币回放；固定一单位结果不等价于统一资金组合。
pub fn aggregate_metric_snapshot(reports: &[ReplayReport]) -> SerializableAggregateSnapshot {
    let mut trades = flatten_trades(reports);
    let trade_count = trades.len();
    let wins = trades
        .iter()
        .filter(|item| item.trade.net_pnl > 0.0)
        .count();
    let losses = trades
        .iter()
        .filter(|item| item.trade.net_pnl < 0.0)
        .count();
    let net_pnl = trades.iter().map(|item| item.trade.net_pnl).sum();
    let net_r = trades.iter().map(|item| item.trade.net_r).sum();
    let gross_profit = trades.iter().map(|item| item.trade.net_pnl.max(0.0)).sum();
    let gross_loss = trades
        .iter()
        .map(|item| (-item.trade.net_pnl).max(0.0))
        .sum();
    let average_net_r = if trade_count > 0 {
        trades.iter().map(|item| item.trade.net_r).sum::<f64>() / trade_count as f64
    } else {
        0.0
    };
    let profitable_symbols = reports
        .iter()
        .filter(|report| report.metrics.net_pnl > 0.0)
        .count();
    let losing_symbols = reports
        .iter()
        .filter(|report| report.metrics.net_pnl < 0.0)
        .count();

    trades.sort_by(|left, right| {
        left.trade
            .exit_time_ms
            .cmp(&right.trade.exit_time_ms)
            .then_with(|| left.symbol.cmp(right.symbol))
            .then_with(|| left.trade.signal_time_ms.cmp(&right.trade.signal_time_ms))
    });

    SerializableAggregateSnapshot {
        symbols: reports.len(),
        trades: trade_count,
        wins,
        losses,
        net_pnl,
        net_r,
        gross_profit,
        gross_loss,
        profit_factor: serializable_profit_factor(gross_profit, gross_loss),
        win_rate_percent: if trade_count > 0 {
            wins as f64 / trade_count as f64 * 100.0
        } else {
            0.0
        },
        average_net_r,
        chronological_closed_equity_max_drawdown: chronological_closed_equity_drawdown(&trades),
        max_single_symbol_intrabar_drawdown: reports
            .iter()
            .map(|report| report.metrics.max_drawdown)
            .fold(0.0_f64, f64::max),
        profitable_symbols,
        losing_symbols,
        flat_symbols: reports.len() - profitable_symbols - losing_symbols,
        open_positions_at_end: reports
            .iter()
            .filter(|report| report.open_position_at_end)
            .count(),
        pending_entries_at_end: reports
            .iter()
            .filter(|report| report.pending_entry_at_end)
            .count(),
    }
}

/// 按净 R 预先固定排序，审计头部盈利交易和盈利币种集中度。
pub fn concentration_audit(reports: &[ReplayReport]) -> ConcentrationAudit {
    let trades = flatten_trades(reports);
    let mut profitable_trades = trades
        .iter()
        .copied()
        .filter(|item| item.trade.net_r > 0.0)
        .collect::<Vec<_>>();
    profitable_trades.sort_by(stable_trade_net_r_order);
    let gross_positive_trade_net_r = profitable_trades
        .iter()
        .map(|item| item.trade.net_r)
        .sum::<f64>();
    let top5_trade_refs = profitable_trades
        .iter()
        .copied()
        .take(5)
        .collect::<Vec<_>>();
    let top5_trade_keys = top5_trade_refs
        .iter()
        .map(|item| (item.report_index, item.trade_index))
        .collect::<BTreeSet<_>>();
    let top5_trade_net_r = top5_trade_refs
        .iter()
        .map(|item| item.trade.net_r)
        .sum::<f64>();
    let top5_trades = top5_trade_refs
        .iter()
        .copied()
        .map(ranked_trade_contribution)
        .collect::<Vec<_>>();

    let mut by_symbol = BTreeMap::<&str, SymbolAccumulator>::new();
    for item in &trades {
        let aggregate = by_symbol.entry(item.symbol).or_default();
        aggregate.trades += 1;
        aggregate.net_r += item.trade.net_r;
        aggregate.net_pnl += item.trade.net_pnl;
    }
    let mut profitable_symbols = by_symbol
        .into_iter()
        .filter(|(_, aggregate)| aggregate.net_r > 0.0)
        .map(|(symbol, aggregate)| RankedSymbolContribution {
            symbol: symbol.to_owned(),
            trades: aggregate.trades,
            net_r: aggregate.net_r,
            net_pnl: aggregate.net_pnl,
        })
        .collect::<Vec<_>>();
    profitable_symbols.sort_by(|left, right| {
        right
            .net_r
            .total_cmp(&left.net_r)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let gross_positive_symbol_net_r = profitable_symbols
        .iter()
        .map(|item| item.net_r)
        .sum::<f64>();
    let top5_symbols = profitable_symbols
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    let top5_symbol_names = top5_symbols
        .iter()
        .map(|item| item.symbol.clone())
        .collect::<BTreeSet<_>>();
    let top5_symbol_net_r = top5_symbols.iter().map(|item| item.net_r).sum::<f64>();

    ConcentrationAudit {
        profitable_trade_count: profitable_trades.len(),
        gross_positive_trade_net_r,
        top1_trade: profitable_trades
            .first()
            .copied()
            .map(ranked_trade_contribution),
        top5_trades,
        top1_trade_positive_net_r_share_percent: profitable_trades
            .first()
            .and_then(|item| positive_share_percent(item.trade.net_r, gross_positive_trade_net_r)),
        top5_trade_positive_net_r_share_percent: positive_share_percent(
            top5_trade_net_r,
            gross_positive_trade_net_r,
        ),
        after_removing_top5_trades: removal_metrics(
            trades
                .iter()
                .filter(|item| !top5_trade_keys.contains(&(item.report_index, item.trade_index)))
                .map(|item| item.trade),
        ),
        profitable_symbol_count: profitable_symbols.len(),
        gross_positive_symbol_net_r,
        top1_symbol: profitable_symbols.first().cloned(),
        top5_symbols,
        top1_symbol_positive_net_r_share_percent: profitable_symbols
            .first()
            .and_then(|item| positive_share_percent(item.net_r, gross_positive_symbol_net_r)),
        top5_symbol_positive_net_r_share_percent: positive_share_percent(
            top5_symbol_net_r,
            gross_positive_symbol_net_r,
        ),
        after_removing_top5_symbols: removal_metrics(
            trades
                .iter()
                .filter(|item| !top5_symbol_names.contains(item.symbol))
                .map(|item| item.trade),
        ),
    }
}

/// 将已平仓交易按方向和相邻信号间隔聚合为 60 分钟 single-linkage 事件。
pub fn event_cluster_audit(reports: &[ReplayReport]) -> EventClusterAudit {
    let trades = flatten_trades(reports);
    let mut clusters = Vec::new();
    append_direction_clusters(&trades, Direction::Long, &mut clusters);
    append_direction_clusters(&trades, Direction::Short, &mut clusters);
    clusters.sort_by(|left, right| {
        direction_order(left.direction)
            .cmp(&direction_order(right.direction))
            .then_with(|| left.first_signal_time_ms.cmp(&right.first_signal_time_ms))
            .then_with(|| left.last_signal_time_ms.cmp(&right.last_signal_time_ms))
    });

    let raw_trade_count = trades.len();
    let long_trade_count = trades
        .iter()
        .filter(|item| item.trade.direction == Direction::Long)
        .count();
    let short_trade_count = raw_trade_count - long_trade_count;
    let long_clusters = clusters
        .iter()
        .filter(|cluster| cluster.direction == Direction::Long)
        .count();
    let short_clusters = clusters.len() - long_clusters;
    let largest_trade_cluster = clusters.iter().cloned().min_by(|left, right| {
        right
            .trades
            .cmp(&left.trades)
            .then_with(|| left.first_signal_time_ms.cmp(&right.first_signal_time_ms))
            .then_with(|| direction_order(left.direction).cmp(&direction_order(right.direction)))
    });
    let mut positive_clusters = clusters
        .iter()
        .filter(|cluster| cluster.net_r > 0.0)
        .cloned()
        .collect::<Vec<_>>();
    positive_clusters.sort_by(|left, right| {
        right
            .net_r
            .total_cmp(&left.net_r)
            .then_with(|| left.first_signal_time_ms.cmp(&right.first_signal_time_ms))
            .then_with(|| direction_order(left.direction).cmp(&direction_order(right.direction)))
    });
    let positive_cluster_net_r = positive_clusters
        .iter()
        .map(|cluster| cluster.net_r)
        .sum::<f64>();
    let largest_positive_net_r_cluster = positive_clusters.first().cloned();

    EventClusterAudit {
        cluster_window_ms: STRICT_STATIC_EVENT_CLUSTER_MS,
        linkage_rule: "same_direction_adjacent_signal_gap_lte_60m_single_linkage",
        raw_trade_count,
        long_trade_count,
        short_trade_count,
        total_clusters: clusters.len(),
        long_clusters,
        short_clusters,
        largest_trade_cluster_share_percent: largest_trade_cluster.as_ref().and_then(|cluster| {
            (raw_trade_count > 0).then_some(cluster.trades as f64 / raw_trade_count as f64 * 100.0)
        }),
        largest_trade_cluster_positive_net_r_share_percent: largest_trade_cluster
            .as_ref()
            .filter(|cluster| cluster.net_r > 0.0)
            .and_then(|cluster| positive_share_percent(cluster.net_r, positive_cluster_net_r)),
        largest_trade_cluster,
        largest_positive_net_r_cluster_share_percent: largest_positive_net_r_cluster
            .as_ref()
            .and_then(|cluster| positive_share_percent(cluster.net_r, positive_cluster_net_r)),
        largest_positive_net_r_cluster,
        clusters,
    }
}

fn blocked_signal_path_eq(left: &BlockedSignal, right: &BlockedSignal) -> bool {
    left.signal_time_ms == right.signal_time_ms
        && left.direction == right.direction
        && left.reason == right.reason
}

fn trade_path_eq(left: &Trade, right: &Trade) -> bool {
    left.direction == right.direction
        && left.families == right.families
        && left.exit_policy == right.exit_policy
        && left.signal_counter_trend_ema_age_bars_capped_600
            == right.signal_counter_trend_ema_age_bars_capped_600
        && same_optional_f64(
            left.counter_trend_structure_breakout_line,
            right.counter_trend_structure_breakout_line,
        )
        && left.counter_trend_structure_confirmed == right.counter_trend_structure_confirmed
        && left.counter_trend_two_r_trailing_activated
            == right.counter_trend_two_r_trailing_activated
        && left.signal_time_ms == right.signal_time_ms
        && left.entry_time_ms == right.entry_time_ms
        && left.exit_time_ms == right.exit_time_ms
        && same_f64(left.entry_price, right.entry_price)
        && same_f64(left.exit_price, right.exit_price)
        && same_f64(left.initial_stop, right.initial_stop)
        && left.exit_reason == right.exit_reason
        && same_f64(left.gross_pnl, right.gross_pnl)
        && same_f64(left.initial_risk, right.initial_risk)
        && same_optional_f64(left.volume_ratio, right.volume_ratio)
        && same_optional_f64(left.rsi, right.rsi)
}

fn same_f64(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits()
}

fn same_optional_f64(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => same_f64(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn serializable_profit_factor(gross_profit: f64, gross_loss: f64) -> SerializableProfitFactor {
    if gross_loss > 0.0 {
        SerializableProfitFactor {
            value: Some(gross_profit / gross_loss),
            is_infinite: false,
        }
    } else {
        SerializableProfitFactor {
            value: None,
            is_infinite: gross_profit > 0.0,
        }
    }
}

fn flatten_trades(reports: &[ReplayReport]) -> Vec<TradeRef<'_>> {
    reports
        .iter()
        .enumerate()
        .flat_map(|(report_index, report)| {
            report
                .trades
                .iter()
                .enumerate()
                .map(move |(trade_index, trade)| TradeRef {
                    report_index,
                    trade_index,
                    symbol: report.symbol.as_str(),
                    trade,
                })
        })
        .collect()
}

fn chronological_closed_equity_drawdown(trades: &[TradeRef<'_>]) -> f64 {
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for item in trades {
        equity += item.trade.net_pnl;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    max_drawdown
}

fn stable_trade_net_r_order(left: &TradeRef<'_>, right: &TradeRef<'_>) -> std::cmp::Ordering {
    right
        .trade
        .net_r
        .total_cmp(&left.trade.net_r)
        .then_with(|| left.symbol.cmp(right.symbol))
        .then_with(|| left.trade.signal_time_ms.cmp(&right.trade.signal_time_ms))
        .then_with(|| left.trade.entry_time_ms.cmp(&right.trade.entry_time_ms))
        .then_with(|| {
            direction_order(left.trade.direction).cmp(&direction_order(right.trade.direction))
        })
        .then_with(|| left.report_index.cmp(&right.report_index))
        .then_with(|| left.trade_index.cmp(&right.trade_index))
}

fn ranked_trade_contribution(item: TradeRef<'_>) -> RankedTradeContribution {
    RankedTradeContribution {
        symbol: item.symbol.to_owned(),
        direction: item.trade.direction,
        signal_time_ms: item.trade.signal_time_ms,
        entry_time_ms: item.trade.entry_time_ms,
        exit_time_ms: item.trade.exit_time_ms,
        net_r: item.trade.net_r,
        net_pnl: item.trade.net_pnl,
    }
}

fn removal_metrics<'a>(trades: impl IntoIterator<Item = &'a Trade>) -> RemovalMetricSnapshot {
    let mut remaining_trades = 0usize;
    let mut net_r = 0.0_f64;
    let mut net_pnl = 0.0_f64;
    let mut gross_profit = 0.0_f64;
    let mut gross_loss = 0.0_f64;
    for trade in trades {
        remaining_trades += 1;
        net_r += trade.net_r;
        net_pnl += trade.net_pnl;
        gross_profit += trade.net_pnl.max(0.0);
        gross_loss += (-trade.net_pnl).max(0.0);
    }
    RemovalMetricSnapshot {
        remaining_trades,
        net_r,
        net_pnl,
        gross_profit,
        gross_loss,
        profit_factor: serializable_profit_factor(gross_profit, gross_loss),
    }
}

fn positive_share_percent(value: f64, positive_total: f64) -> Option<f64> {
    (positive_total > 0.0).then_some(value / positive_total * 100.0)
}

fn append_direction_clusters(
    trades: &[TradeRef<'_>],
    direction: Direction,
    output: &mut Vec<EventClusterSummary>,
) {
    let mut selected = trades
        .iter()
        .copied()
        .filter(|item| item.trade.direction == direction)
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        left.trade
            .signal_time_ms
            .cmp(&right.trade.signal_time_ms)
            .then_with(|| left.symbol.cmp(right.symbol))
            .then_with(|| left.report_index.cmp(&right.report_index))
            .then_with(|| left.trade_index.cmp(&right.trade_index))
    });

    let mut current: Option<EventClusterSummary> = None;
    for item in selected {
        let starts_new_cluster = current.as_ref().is_some_and(|cluster| {
            item.trade.signal_time_ms - cluster.last_signal_time_ms > STRICT_STATIC_EVENT_CLUSTER_MS
        });
        if starts_new_cluster {
            output.push(current.take().expect("current cluster exists"));
        }
        let cluster = current.get_or_insert(EventClusterSummary {
            direction,
            first_signal_time_ms: item.trade.signal_time_ms,
            last_signal_time_ms: item.trade.signal_time_ms,
            trades: 0,
            net_r: 0.0,
            net_pnl: 0.0,
        });
        cluster.last_signal_time_ms = item.trade.signal_time_ms;
        cluster.trades += 1;
        cluster.net_r += item.trade.net_r;
        cluster.net_pnl += item.trade.net_pnl;
    }
    if let Some(cluster) = current {
        output.push(cluster);
    }
}

fn direction_order(direction: Direction) -> u8 {
    match direction {
        Direction::Long => 0,
        Direction::Short => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::{ExitPolicy, ExitReason, SignalFamily};

    fn trade(direction: Direction, signal_time_ms: i64, net_r: f64, net_pnl: f64) -> Trade {
        Trade {
            direction,
            families: vec![SignalFamily::EmaTrendLong],
            exit_policy: ExitPolicy::Fixed,
            signal_counter_trend_ema_age_bars_capped_600: None,
            counter_trend_structure_breakout_line: None,
            counter_trend_structure_confirmed: false,
            counter_trend_two_r_trailing_activated: false,
            range_partial_one_r_taken: false,
            range_two_r_trailing_activated: false,
            signal_time_ms,
            entry_time_ms: signal_time_ms + 900_000,
            exit_time_ms: signal_time_ms + 1_800_000,
            entry_price: 100.0,
            exit_price: 101.0,
            initial_stop: 99.0,
            exit_reason: ExitReason::TakeProfit,
            gross_pnl: 1.0,
            net_pnl,
            initial_risk: 1.0,
            net_r,
            anchor_upthrust_target_consumption_ratio: None,
            volume_ratio: Some(3.0),
            rsi: Some(55.0),
        }
    }

    fn report(symbol: &str, trades: Vec<Trade>) -> ReplayReport {
        let net_pnl = trades.iter().map(|trade| trade.net_pnl).sum::<f64>();
        let gross_profit = trades
            .iter()
            .map(|trade| trade.net_pnl.max(0.0))
            .sum::<f64>();
        let gross_loss = trades
            .iter()
            .map(|trade| (-trade.net_pnl).max(0.0))
            .sum::<f64>();
        ReplayReport {
            strategy_version: "fixture",
            pine_source_fnv1a32: "fixture",
            symbol: symbol.to_owned(),
            tick_size: 0.1,
            evaluation_start_ms: 0,
            evaluation_end_ms: 10_000_000,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            metrics: Metrics {
                trades: trades.len(),
                wins: trades.iter().filter(|trade| trade.net_pnl > 0.0).count(),
                losses: trades.iter().filter(|trade| trade.net_pnl < 0.0).count(),
                net_pnl,
                gross_profit,
                gross_loss,
                profit_factor: if gross_loss > 0.0 {
                    Some(gross_profit / gross_loss)
                } else if gross_profit > 0.0 {
                    Some(f64::INFINITY)
                } else {
                    None
                },
                win_rate_percent: 0.0,
                average_net_r: 0.0,
                closed_equity_max_drawdown: 0.0,
                max_drawdown: 0.0,
            },
            entry_candidates: Vec::new(),
            trades,
            blocked_signals: Vec::new(),
            open_position_at_end: false,
            pending_entry_at_end: false,
        }
    }

    #[test]
    fn cost_path_allows_only_net_outcome_and_metrics_to_change() {
        let zero = report("BTC-USDT-SWAP", vec![trade(Direction::Long, 0, 1.0, 1.0)]);
        let mut stress = zero.clone();
        stress.fee_bps_per_side = STRICT_STATIC_FEE_BPS_PER_SIDE;
        stress.slippage_bps_per_side = STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE;
        stress.trades[0].net_pnl = 0.8;
        stress.trades[0].net_r = 0.8;
        stress.metrics.net_pnl = 0.8;

        assert_cost_path_parity(&zero, &stress).expect("cost-only outcome drift is allowed");

        stress.trades[0].exit_time_ms += 1;
        assert!(assert_cost_path_parity(&zero, &stress).is_err());
    }

    #[test]
    fn concentration_handles_fewer_than_five_profitable_trades() {
        let reports = vec![report(
            "BTC-USDT-SWAP",
            vec![
                trade(Direction::Long, 0, 3.0, 3.0),
                trade(Direction::Long, 1, 2.0, 2.0),
                trade(Direction::Long, 2, 1.0, 1.0),
                trade(Direction::Long, 3, -1.0, -1.0),
            ],
        )];

        let audit = concentration_audit(&reports);

        assert_eq!(audit.top5_trades.len(), 3);
        assert_eq!(audit.after_removing_top5_trades.remaining_trades, 1);
        assert_eq!(audit.after_removing_top5_trades.net_r, -1.0);
    }

    #[test]
    fn concentration_uses_stable_symbol_tie_break_and_handles_zero_winners() {
        let tied = vec![
            report("B-USDT-SWAP", vec![trade(Direction::Long, 0, 1.0, 1.0)]),
            report("A-USDT-SWAP", vec![trade(Direction::Long, 0, 1.0, 1.0)]),
        ];
        let tied_audit = concentration_audit(&tied);
        assert_eq!(tied_audit.top5_trades[0].symbol, "A-USDT-SWAP");
        assert_eq!(
            tied_audit
                .top1_symbol
                .as_ref()
                .map(|item| item.symbol.as_str()),
            Some("A-USDT-SWAP")
        );

        let no_winners = vec![report(
            "C-USDT-SWAP",
            vec![trade(Direction::Short, 0, -1.0, -1.0)],
        )];
        let empty_audit = concentration_audit(&no_winners);
        assert!(empty_audit.top1_trade.is_none());
        assert!(empty_audit.top5_trades.is_empty());
        assert!(empty_audit
            .top5_trade_positive_net_r_share_percent
            .is_none());
        assert_eq!(empty_audit.after_removing_top5_trades.remaining_trades, 1);
    }

    #[test]
    fn event_clusters_keep_directions_separate_and_split_after_sixty_minutes() {
        let reports = vec![report(
            "BTC-USDT-SWAP",
            vec![
                trade(Direction::Long, 0, 1.0, 1.0),
                trade(Direction::Long, STRICT_STATIC_EVENT_CLUSTER_MS, 1.0, 1.0),
                trade(
                    Direction::Long,
                    2 * STRICT_STATIC_EVENT_CLUSTER_MS + 60_000,
                    1.0,
                    1.0,
                ),
                trade(Direction::Short, 0, 1.0, 1.0),
            ],
        )];

        let audit = event_cluster_audit(&reports);

        assert_eq!(audit.long_clusters, 2);
        assert_eq!(audit.short_clusters, 1);
        assert_eq!(audit.total_clusters, 3);
        assert_eq!(audit.largest_trade_cluster_share_percent, Some(50.0));
        assert_eq!(
            audit.largest_trade_cluster_positive_net_r_share_percent,
            Some(50.0)
        );
        assert_eq!(
            audit
                .largest_trade_cluster
                .as_ref()
                .map(|cluster| cluster.trades),
            Some(2)
        );
    }

    #[test]
    fn event_clusters_use_adjacent_single_linkage_for_zero_fifty_nine_one_eighteen() {
        let minute = 60_000;
        let reports = vec![report(
            "BTC-USDT-SWAP",
            vec![
                trade(Direction::Long, 0, 1.0, 1.0),
                trade(Direction::Long, 59 * minute, 1.0, 1.0),
                trade(Direction::Long, 118 * minute, 1.0, 1.0),
            ],
        )];

        let audit = event_cluster_audit(&reports);

        assert_eq!(audit.total_clusters, 1);
        assert_eq!(audit.clusters[0].trades, 3);
        assert_eq!(audit.clusters[0].last_signal_time_ms, 118 * minute);
    }

    #[test]
    fn blocked_signal_helper_excludes_warmup_and_after_end() {
        let mut value = report("BTC-USDT-SWAP", Vec::new());
        value.blocked_signals = vec![
            BlockedSignal {
                signal_time_ms: -1,
                direction: Some(Direction::Long),
                reason: "warmup".to_owned(),
            },
            BlockedSignal {
                signal_time_ms: 0,
                direction: Some(Direction::Long),
                reason: "start".to_owned(),
            },
            BlockedSignal {
                signal_time_ms: 60,
                direction: Some(Direction::Short),
                reason: "end".to_owned(),
            },
            BlockedSignal {
                signal_time_ms: 61,
                direction: Some(Direction::Short),
                reason: "after".to_owned(),
            },
        ];

        let filtered = blocked_signals_in_window(&value, 0, 60);
        assert_eq!(
            filtered
                .iter()
                .map(|signal| signal.signal_time_ms)
                .collect::<Vec<_>>(),
            vec![0, 60]
        );
        assert_eq!(blocked_signal_count_in_window(&value, 0, 60), 2);
    }

    #[test]
    fn infinite_profit_factor_serializes_without_non_finite_json_number() {
        let value = report("BTC-USDT-SWAP", vec![trade(Direction::Long, 0, 1.0, 1.0)]);
        let snapshot = metric_snapshot(&value.metrics);
        let symbol = symbol_snapshot(&value, 0, 10_000_000);
        let aggregate = aggregate_metric_snapshot(std::slice::from_ref(&value));
        let json = serde_json::to_string(&(snapshot.clone(), symbol, aggregate))
            .expect("safe report JSON");

        assert!(snapshot.profit_factor.is_infinite);
        assert_eq!(snapshot.profit_factor.value, None);
        assert!(!json.contains("Infinity"));
    }
}
