use super::equity_stats::{analyze_profit_losses, format_optional_f64, trade_sharpe};
use super::{
    runner_exit_for_target, select_stop_loss_for_confirmed_signal, trade_direction_for_event,
    BacktestCandle, ConfirmedEvent, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
    RunnerExit,
};
use anyhow::Result;
use chrono::{FixedOffset, TimeZone, Utc};
use rust_quant_domain::entities::BacktestDetail;
use rust_quant_domain::SignalDirection;
use rust_quant_strategies::framework::backtest::{
    run_indicator_strategy_backtest, BasicRiskStrategyConfig, IndicatorStrategyBacktest,
    SignalResult, TradeRecord,
};
use rust_quant_strategies::CandleItem;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
const INITIAL_FUND_PER_SYMBOL: f64 = 100.0;
const FRAMEWORK_SIGNAL_WARMUP_CANDLES: usize = 500;
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityReport {
    /// targetr，用于展示或持久化查询结果。
    pub target_r: f64,
    /// initial资金per交易对，用于展示或持久化查询结果。
    pub initial_fund_per_symbol: f64,
    /// 最小trades，用于控制策略触发门槛。
    pub min_trades: usize,
    /// total开盘trades，用于展示或持久化查询结果。
    pub total_open_trades: usize,
    /// total收益，用于展示或持久化查询结果。
    pub total_profit: f64,
    /// 胜率；为空时使用默认值或表示不限制。
    pub win_rate: Option<f64>,
    /// 交易级 Sharpe；为空时表示样本不足。
    pub trade_sharpe: Option<f64>,
    /// 最大回撤百分比。
    pub max_drawdown_pct: f64,
    /// meets最小trades，用于展示或持久化查询结果。
    pub meets_min_trades: bool,
    /// 列表数据。
    pub symbols: Vec<FrameworkEquitySymbolReport>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquitySymbolReport {
    /// 交易对或资产符号。
    pub symbol: String,
    /// 回测过程中仍未平仓的交易记录。
    pub open_trades: usize,
    /// 金额数值。
    pub final_fund: f64,
    /// 收益。
    pub profit: f64,
    /// wins，用于展示或持久化查询结果。
    pub wins: usize,
    /// losses，用于展示或持久化查询结果。
    pub losses: usize,
    /// 交易级 Sharpe；为空时表示样本不足。
    pub trade_sharpe: Option<f64>,
    /// 最大回撤百分比。
    pub max_drawdown_pct: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquitySplitReport {
    /// label，用于展示或持久化查询结果。
    pub label: &'static str,
    /// 开始时间。
    pub start_entry_ts: i64,
    /// 结束时间。
    pub end_entry_ts: i64,
    /// 报告。
    pub report: FrameworkEquityReport,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityTriggerReport {
    /// trigger，用于展示或持久化查询结果。
    pub trigger: String,
    /// 报告。
    pub report: FrameworkEquityReport,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityFeatureReport {
    /// feature，用于展示或持久化查询结果。
    pub feature: &'static str,
    /// bucket，用于展示或持久化查询结果。
    pub bucket: &'static str,
    /// 报告。
    pub report: FrameworkEquityReport,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquitySymbolWindowReport {
    /// split，用于展示或持久化查询结果。
    pub split: FrameworkEquitySplitReport,
    /// 列表数据。
    pub top_symbols: Vec<FrameworkEquitySymbolReport>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityTradeReport {
    /// targetr，用于展示或持久化查询结果。
    pub target_r: f64,
    /// 交易对或资产符号。
    pub symbol: String,
    /// event ID。
    pub event_id: i64,
    /// 时间字段。
    pub detected_at: String,
    /// 时间戳。
    pub entry_ts: i64,
    /// 开仓时间。
    pub signal_open_position_time: String,
    /// 开仓时间。
    pub open_position_time: String,
    /// 平仓时间。
    pub close_position_time: Option<String>,
    /// 价格数值。
    pub open_price: f64,
    /// 离场价格。
    pub close_price: Option<f64>,
    /// 类型标识。
    pub close_type: String,
    /// 状态值。
    pub signal_status: i32,
    /// 收益亏损，用于展示或持久化查询结果。
    pub profit_loss: f64,
    /// 数量。
    pub quantity: f64,
    /// outcome，用于展示或持久化查询结果。
    pub outcome: &'static str,
    /// trigger，用于展示或持久化查询结果。
    pub trigger: String,
    /// new排名，用于展示或持久化查询结果。
    pub new_rank: i32,
    /// delta排名，用于展示或持久化查询结果。
    pub delta_rank: i32,
    /// 价格涨跌幅百分比。
    pub price_change_pct: f64,
    /// 列表数据。
    pub close_legs: Vec<FrameworkEquityCloseLegReport>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityCloseLegReport {
    /// 时间戳。
    pub close_ts: i64,
    /// 平仓时间。
    pub close_position_time: String,
    /// 离场价格。
    pub close_price: f64,
    /// 类型标识。
    pub close_type: String,
    /// 收益亏损，用于展示或持久化查询结果。
    pub profit_loss: f64,
    /// 数量。
    pub quantity: f64,
    /// full收盘，用于展示或持久化查询结果。
    pub full_close: bool,
    /// 原因说明。
    pub exit_reason: String,
    /// 结果r，用于展示或持久化查询结果。
    pub result_r: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityConcentrationReport {
    /// targetr，用于展示或持久化查询结果。
    pub target_r: f64,
    /// 最小trades，用于控制策略触发门槛。
    pub min_trades: usize,
    /// removedtoppositive，用于展示或持久化查询结果。
    pub removed_top_positive: usize,
    /// 列表数据。
    pub removed_symbols: Vec<String>,
    /// removed收益，用于展示或持久化查询结果。
    pub removed_profit: f64,
    /// 被移除样本占比。
    pub removed_share_pct: Option<f64>,
    /// remainingsymbols，用于展示或持久化查询结果。
    pub remaining_symbols: usize,
    /// remaining开盘trades，用于展示或持久化查询结果。
    pub remaining_open_trades: usize,
    /// remainingtotal收益，用于展示或持久化查询结果。
    pub remaining_total_profit: f64,
    /// remainingwin 费率；为空时使用默认值或表示不限制。
    pub remaining_win_rate: Option<f64>,
    /// 剩余样本最大回撤百分比。
    pub remaining_max_drawdown_pct: f64,
    /// remainingmeets最小trades，用于展示或持久化查询结果。
    pub remaining_meets_min_trades: bool,
}
#[derive(Debug, Clone, PartialEq)]
struct ReplayEntry {
    /// 入场价格。
    entry_price: f64,
    /// event ID。
    event_id: i64,
    /// trigger，用于行情、K 线或市场扫描。
    trigger: String,
    /// direction，用于行情、K 线或市场扫描。
    direction: MarketVelocityTradeDirection,
    /// 止损百分比。
    stop_loss_pct: f64,
    /// 止损价格。
    stop_loss_price: f64,
    /// 止损来源。
    stop_loss_source: String,
}
#[derive(Debug, Clone, PartialEq)]
struct ReplayActivePosition {
    /// 入场价格。
    entry_price: f64,
    /// event ID。
    event_id: i64,
    /// trigger，用于记录交易或执行状态。
    trigger: String,
    /// direction，用于记录交易或执行状态。
    direction: MarketVelocityTradeDirection,
    /// 止损百分比。
    stop_loss_pct: f64,
    /// 止损价格。
    stop_loss_price: f64,
    /// 止损来源。
    stop_loss_source: String,
    /// 收益protected，用于记录交易或执行状态。
    profit_protected: bool,
    /// observedK 线，用于记录交易或执行状态。
    observed_candles: usize,
}
#[derive(Debug, Clone, PartialEq)]
struct ReplayOpenTrade {
    /// event ID。
    event_id: i64,
    /// trigger，用于记录交易或执行状态。
    trigger: String,
    /// 开仓时间。
    open_position_time: String,
    /// 价格数值。
    open_price: f64,
    /// 数量。
    quantity: f64,
    /// 状态值。
    signal_status: i32,
}
#[derive(Debug, Clone)]
struct MarketVelocityReplayStrategy {
    /// 时间戳。
    entries_by_ts: BTreeMap<i64, ReplayEntry>,
    /// targetr，用于行情、K 线或市场扫描。
    target_r: f64,
    /// 达到指定 R 倍数后启用利润保护；为空时不启用。
    profit_protect_after_r: Option<f64>,
    /// 收益protect止损r，用于行情、K 线或市场扫描。
    profit_protect_stop_r: f64,
    /// 无盈利时提前退出所需 K 线数量；为空时不启用。
    early_exit_no_profit_candles: Option<usize>,
    ignore_entry_signal_updates_while_open: bool,
    /// 活动仓位；为空时表示没有活动仓位。
    active_position: Option<ReplayActivePosition>,
}
#[derive(Debug, Clone, PartialEq)]
struct RunnerReplayTrade {
    /// event ID。
    event_id: i64,
    /// 开仓时间。
    open_position_time: String,
    /// 平仓时间。
    close_position_time: String,
    /// 价格数值。
    open_price: f64,
    /// 离场价格。
    close_price: f64,
    /// 数量。
    quantity: f64,
    /// 收益亏损，用于记录交易或执行状态。
    profit_loss: f64,
    /// 类型标识。
    close_type: String,
    /// 结果r，用于记录交易或执行状态。
    result_r: f64,
    /// 列表数据。
    close_legs: Vec<FrameworkEquityCloseLegReport>,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
pub fn build_framework_equity_report(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> FrameworkEquityReport {
    let mut symbols = confirmed
        .iter()
        .map(|event| event.event.symbol.clone())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    let mut symbol_reports = Vec::new();
    let mut all_returns = Vec::new();
    for symbol in symbols {
        let Some(candles) = candles_15m
            .get(&symbol)
            .filter(|candles| !candles.is_empty())
        else {
            continue;
        };
        if let Some(runner) = runner_exit_for_target(args, target_r) {
            let trades = build_runner_replay_trades(
                confirmed
                    .iter()
                    .filter(|event| event.event.symbol == symbol)
                    .cloned()
                    .collect(),
                candles,
                target_r,
                args,
                runner,
            );
            let closed_stats = analyze_profit_losses(
                trades.iter().map(|trade| trade.profit_loss),
                INITIAL_FUND_PER_SYMBOL,
            );
            let profit = trades.iter().map(|trade| trade.profit_loss).sum::<f64>();
            all_returns.extend(closed_stats.returns.iter().copied());
            symbol_reports.push(FrameworkEquitySymbolReport {
                symbol,
                open_trades: trades.len(),
                final_fund: INITIAL_FUND_PER_SYMBOL + profit,
                profit,
                wins: closed_stats.wins,
                losses: closed_stats.losses,
                trade_sharpe: trade_sharpe(&closed_stats.returns),
                max_drawdown_pct: closed_stats.max_drawdown_pct,
            });
            continue;
        }
        let strategy = MarketVelocityReplayStrategy::new(
            confirmed
                .iter()
                .filter(|event| event.event.symbol == symbol)
                .cloned()
                .collect(),
            args,
            target_r,
            args.profit_protect_after_r,
            args.profit_protect_stop_r,
            args.early_exit_no_profit_candles,
            args.ignore_entry_signal_updates_while_open,
        );
        let candle_items = framework_replay_candle_items(candles);
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: args.stop_loss_pct,
            is_used_signal_k_line_stop_loss: Some(true),
            dynamic_max_loss: Some(false),
            ..BasicRiskStrategyConfig::default()
        };
        let result = run_indicator_strategy_backtest(&symbol, strategy, &candle_items, risk_config);
        let closed_stats = analyze_profit_losses(
            result
                .trade_records
                .iter()
                .filter(|record| record.full_close)
                .map(|record| record.profit_loss),
            INITIAL_FUND_PER_SYMBOL,
        );
        let profit = result.funds - INITIAL_FUND_PER_SYMBOL;
        all_returns.extend(closed_stats.returns.iter().copied());
        symbol_reports.push(FrameworkEquitySymbolReport {
            symbol,
            open_trades: result.open_trades,
            final_fund: result.funds,
            profit,
            wins: closed_stats.wins,
            losses: closed_stats.losses,
            trade_sharpe: trade_sharpe(&closed_stats.returns),
            max_drawdown_pct: closed_stats.max_drawdown_pct,
        });
    }
    let total_open_trades = symbol_reports.iter().map(|item| item.open_trades).sum();
    let wins = symbol_reports.iter().map(|item| item.wins).sum::<usize>();
    let losses = symbol_reports.iter().map(|item| item.losses).sum::<usize>();
    let total_profit = symbol_reports.iter().map(|item| item.profit).sum();
    let max_drawdown_pct = symbol_reports
        .iter()
        .map(|item| item.max_drawdown_pct)
        .fold(0.0, f64::max);
    let resolved = wins + losses;
    let win_rate = (resolved > 0).then_some(wins as f64 / resolved as f64 * 100.0);
    FrameworkEquityReport {
        target_r,
        initial_fund_per_symbol: INITIAL_FUND_PER_SYMBOL,
        min_trades: args.min_trades,
        total_open_trades,
        total_profit,
        win_rate,
        trade_sharpe: trade_sharpe(&all_returns),
        max_drawdown_pct,
        meets_min_trades: total_open_trades >= args.min_trades,
        symbols: symbol_reports,
    }
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_split_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<FrameworkEquitySplitReport> {
    if confirmed.len() < 2 {
        return Vec::new();
    }
    let mut ordered = confirmed.to_vec();
    ordered.sort_by_key(|event| (event.entry_ts, event.event.id));
    let midpoint = ordered.len() / 2;
    [
        ("early", &ordered[..midpoint]),
        ("late", &ordered[midpoint..]),
    ]
    .into_iter()
    .filter_map(|(label, events)| {
        let start_entry_ts = events.first()?.entry_ts;
        let end_entry_ts = events.last()?.entry_ts;
        Some(FrameworkEquitySplitReport {
            label,
            start_entry_ts,
            end_entry_ts,
            report: build_framework_equity_report(events, candles_15m, target_r, args),
        })
    })
    .collect()
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_quartile_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<FrameworkEquitySplitReport> {
    if confirmed.len() < 4 {
        return Vec::new();
    }
    let mut ordered = confirmed.to_vec();
    ordered.sort_by_key(|event| (event.entry_ts, event.event.id));
    ["q1", "q2", "q3", "q4"]
        .into_iter()
        .enumerate()
        .filter_map(|(index, label)| {
            let start = ordered.len() * index / 4;
            let end = ordered.len() * (index + 1) / 4;
            let events = &ordered[start..end];
            let start_entry_ts = events.first()?.entry_ts;
            let end_entry_ts = events.last()?.entry_ts;
            Some(FrameworkEquitySplitReport {
                label,
                start_entry_ts,
                end_entry_ts,
                report: build_framework_equity_report(events, candles_15m, target_r, args),
            })
        })
        .collect()
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_trigger_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<FrameworkEquityTriggerReport> {
    let mut events_by_trigger = BTreeMap::<String, Vec<ConfirmedEvent>>::new();
    for event in confirmed {
        events_by_trigger
            .entry(event.trigger.clone())
            .or_default()
            .push(event.clone());
    }
    events_by_trigger
        .into_iter()
        .map(|(trigger, events)| FrameworkEquityTriggerReport {
            trigger,
            report: build_framework_equity_report(&events, candles_15m, target_r, args),
        })
        .collect()
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_feature_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<FrameworkEquityFeatureReport> {
    let mut reports = Vec::new();
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "delta_rank",
        "lt12",
        |event| event.event.delta_rank < 12,
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "delta_rank",
        "12_24",
        |event| (12..=24).contains(&event.event.delta_rank),
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "delta_rank",
        "25_48",
        |event| (25..=48).contains(&event.event.delta_rank),
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "delta_rank",
        "49_plus",
        |event| event.event.delta_rank >= 49,
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "price_change_pct",
        "lt5",
        |event| event.event.price_change_pct < 5.0,
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "price_change_pct",
        "5_10",
        |event| event.event.price_change_pct >= 5.0 && event.event.price_change_pct < 10.0,
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "price_change_pct",
        "10_20",
        |event| event.event.price_change_pct >= 10.0 && event.event.price_change_pct < 20.0,
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "price_change_pct",
        "20_plus",
        |event| event.event.price_change_pct >= 20.0,
    );
    reports
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_symbol_window_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
    sample_limit: usize,
) -> Vec<FrameworkEquitySymbolWindowReport> {
    build_framework_equity_quartile_reports(confirmed, candles_15m, target_r, args)
        .into_iter()
        .map(|split| {
            let mut top_symbols = split.report.symbols.clone();
            top_symbols.sort_by(|left, right| {
                right
                    .profit
                    .partial_cmp(&left.profit)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.symbol.cmp(&right.symbol))
            });
            top_symbols.truncate(sample_limit);
            FrameworkEquitySymbolWindowReport { split, top_symbols }
        })
        .collect()
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_trade_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<FrameworkEquityTradeReport> {
    let mut symbols = confirmed
        .iter()
        .map(|event| event.event.symbol.clone())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    let confirmed_by_event_id = confirmed
        .iter()
        .map(|event| (event.event.id, event))
        .collect::<HashMap<_, _>>();
    let mut reports = Vec::new();
    for symbol in symbols {
        let Some(candles) = candles_15m
            .get(&symbol)
            .filter(|candles| !candles.is_empty())
        else {
            continue;
        };
        if let Some(runner) = runner_exit_for_target(args, target_r) {
            let runner_trades = build_runner_replay_trades(
                confirmed
                    .iter()
                    .filter(|event| event.event.symbol == symbol)
                    .cloned()
                    .collect(),
                candles,
                target_r,
                args,
                runner,
            );
            reports.extend(runner_trades.into_iter().filter_map(|trade| {
                let event = confirmed_by_event_id.get(&trade.event_id)?;
                Some(FrameworkEquityTradeReport {
                    target_r,
                    symbol: symbol.clone(),
                    event_id: trade.event_id,
                    detected_at: event.event.detected_at.clone(),
                    entry_ts: event.entry_ts,
                    signal_open_position_time: timestamp_ms_to_shanghai_datetime(event.event.ts),
                    open_position_time: shanghai_offset_label(trade.open_position_time),
                    close_position_time: Some(shanghai_offset_label(trade.close_position_time)),
                    open_price: trade.open_price,
                    close_price: Some(trade.close_price),
                    close_type: trade.close_type,
                    signal_status: 0,
                    profit_loss: trade.profit_loss,
                    quantity: trade.quantity,
                    outcome: trade_outcome_label(trade.profit_loss),
                    trigger: event.trigger.clone(),
                    new_rank: event.event.new_rank,
                    delta_rank: event.event.delta_rank,
                    price_change_pct: event.event.price_change_pct,
                    close_legs: trade.close_legs,
                })
            }));
            continue;
        }
        let strategy = MarketVelocityReplayStrategy::new(
            confirmed
                .iter()
                .filter(|event| event.event.symbol == symbol)
                .cloned()
                .collect(),
            args,
            target_r,
            args.profit_protect_after_r,
            args.profit_protect_stop_r,
            args.early_exit_no_profit_candles,
            args.ignore_entry_signal_updates_while_open,
        );
        let candle_items = framework_replay_candle_items(candles);
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: args.stop_loss_pct,
            is_used_signal_k_line_stop_loss: Some(true),
            dynamic_max_loss: Some(false),
            ..BasicRiskStrategyConfig::default()
        };
        let result = run_indicator_strategy_backtest(&symbol, strategy, &candle_items, risk_config);
        let mut open_trade = None;
        for record in &result.trade_records {
            if !record.full_close {
                open_trade = parse_replay_open_trade(record);
                continue;
            }
            let Some(open) = open_trade.take() else {
                continue;
            };
            let Some(event) = confirmed_by_event_id.get(&open.event_id) else {
                continue;
            };
            reports.push(FrameworkEquityTradeReport {
                target_r,
                symbol: symbol.clone(),
                event_id: open.event_id,
                detected_at: event.event.detected_at.clone(),
                entry_ts: event.entry_ts,
                signal_open_position_time: timestamp_ms_to_shanghai_datetime(event.event.ts),
                open_position_time: shanghai_offset_label(open.open_position_time),
                close_position_time: record
                    .close_position_time
                    .clone()
                    .map(shanghai_offset_label),
                open_price: open.open_price,
                close_price: record.close_price,
                close_type: framework_close_type(record),
                signal_status: record.signal_status,
                profit_loss: record.profit_loss,
                quantity: if record.quantity > 0.0 {
                    record.quantity
                } else {
                    open.quantity
                },
                outcome: trade_outcome_label(record.profit_loss),
                trigger: open.trigger,
                new_rank: event.event.new_rank,
                delta_rank: event.event.delta_rank,
                price_change_pct: event.event.price_change_pct,
                close_legs: Vec::new(),
            });
        }
    }
    reports.sort_by_key(|report| (report.entry_ts, report.event_id));
    reports
}
/// 提供框架平仓type的集中实现，避免回测策略调用方重复处理相同细节。
fn framework_close_type(record: &TradeRecord) -> String {
    if !record.close_type.starts_with("反向信号平仓") {
        return record.close_type.clone();
    }
    let Some(value) = record
        .signal_value
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
    else {
        return record.close_type.clone();
    };
    value
        .get("exit_reason")
        .and_then(Value::as_str)
        .unwrap_or(&record.close_type)
        .to_string()
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_runner_replay_trades(
    events: Vec<ConfirmedEvent>,
    candles: &[BacktestCandle],
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
    runner: RunnerExit,
) -> Vec<RunnerReplayTrade> {
    let mut entries_by_ts = events
        .into_iter()
        .map(|event| (event.entry_ts, event))
        .collect::<BTreeMap<_, _>>();
    if entries_by_ts.is_empty() || candles.is_empty() {
        return Vec::new();
    }
    let mut trades = Vec::new();
    let mut locked_until = i64::MIN;
    for (entry_ts, event) in std::mem::take(&mut entries_by_ts) {
        if entry_ts <= locked_until {
            continue;
        }
        let Some(entry_idx) = candles.iter().position(|candle| candle.ts == entry_ts) else {
            continue;
        };
        let selected_stop_loss = select_stop_loss_for_confirmed_signal(&event, args);
        let Some(trade) = simulate_framework_runner_trade(
            candles,
            entry_idx,
            &event,
            target_r,
            selected_stop_loss.stop_loss_pct,
            runner,
        ) else {
            continue;
        };
        locked_until = trade_exit_ts(&trade);
        trades.push(trade);
    }
    trades
}
/// 执行模拟框架Runner交易步骤，串起回测策略需要的状态推进和错误处理。
fn simulate_framework_runner_trade(
    candles: &[BacktestCandle],
    entry_idx: usize,
    event: &ConfirmedEvent,
    target_r: f64,
    stop_loss_pct: f64,
    runner: RunnerExit,
) -> Option<RunnerReplayTrade> {
    let entry_price = event.entry_price;
    let direction = trade_direction_for_event(&event.event);
    let quantity = INITIAL_FUND_PER_SYMBOL / entry_price;
    let stop_price = stop_price_for(entry_price, stop_loss_pct, direction);
    let target_price = target_price_for(entry_price, stop_loss_pct, target_r, direction);
    let runner_target_price =
        target_price_for(entry_price, stop_loss_pct, runner.target_r, direction);
    let runner_stop_price = target_price_for(entry_price, stop_loss_pct, runner.stop_r, direction);
    let base_fraction = 1.0 - runner.fraction;
    let base_quantity = quantity * base_fraction;
    let runner_quantity = quantity * runner.fraction;
    let open_position_time = timestamp_ms_to_shanghai_datetime(event.entry_ts);
    let mut base_leg: Option<FrameworkEquityCloseLegReport> = None;
    let mut last_seen: Option<&BacktestCandle> = None;
    for candle in candles.iter().skip(entry_idx) {
        last_seen = Some(candle);
        if base_leg.is_none() {
            let hit_stop = hit_stop(candle.low, candle.high, stop_price, direction);
            let hit_target = hit_target(candle.low, candle.high, target_price, direction);
            if hit_stop && hit_target {
                return Some(build_single_leg_runner_trade(
                    event,
                    direction,
                    &open_position_time,
                    entry_price,
                    quantity,
                    stop_price,
                    candle.ts,
                    "both_hit_stop_first",
                    -1.0,
                ));
            }
            if hit_stop {
                return Some(build_single_leg_runner_trade(
                    event,
                    direction,
                    &open_position_time,
                    entry_price,
                    quantity,
                    stop_price,
                    candle.ts,
                    "stop_hit",
                    -1.0,
                ));
            }
            if hit_target {
                base_leg = Some(build_close_leg(
                    direction,
                    event.entry_price,
                    base_quantity,
                    target_price,
                    candle.ts,
                    "runner_base_target_hit",
                    target_r,
                    false,
                ));
            }
            continue;
        }
        let hit_stop = hit_stop(candle.low, candle.high, runner_stop_price, direction);
        let hit_target = hit_target(candle.low, candle.high, runner_target_price, direction);
        let (close_price, exit_reason, result_r) = if hit_stop && hit_target {
            (runner_stop_price, "runner_stop_first", runner.stop_r)
        } else if hit_target {
            (runner_target_price, "runner_target_hit", runner.target_r)
        } else if hit_stop {
            (runner_stop_price, "runner_stop_hit", runner.stop_r)
        } else {
            continue;
        };
        let runner_leg = build_close_leg(
            direction,
            event.entry_price,
            runner_quantity,
            close_price,
            candle.ts,
            exit_reason,
            result_r,
            true,
        );
        return Some(build_multi_leg_runner_trade(
            event,
            open_position_time,
            entry_price,
            quantity,
            vec![base_leg.expect("base leg exists"), runner_leg],
        ));
    }
    let last_seen = last_seen?;
    if let Some(base_leg) = base_leg {
        let close_r = r_for_price(entry_price, stop_loss_pct, last_seen.close, direction);
        let runner_leg = build_close_leg(
            direction,
            entry_price,
            runner_quantity,
            last_seen.close,
            last_seen.ts,
            "runner_forward_data_incomplete",
            close_r,
            true,
        );
        return Some(build_multi_leg_runner_trade(
            event,
            open_position_time,
            entry_price,
            quantity,
            vec![base_leg, runner_leg],
        ));
    }
    let close_r = r_for_price(entry_price, stop_loss_pct, last_seen.close, direction);
    Some(build_single_leg_runner_trade(
        event,
        direction,
        &open_position_time,
        entry_price,
        quantity,
        last_seen.close,
        last_seen.ts,
        "forward_data_incomplete",
        close_r,
    ))
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_single_leg_runner_trade(
    event: &ConfirmedEvent,
    direction: MarketVelocityTradeDirection,
    open_position_time: &str,
    entry_price: f64,
    quantity: f64,
    close_price: f64,
    close_ts: i64,
    exit_reason: &str,
    result_r: f64,
) -> RunnerReplayTrade {
    build_multi_leg_runner_trade(
        event,
        open_position_time.to_string(),
        entry_price,
        quantity,
        vec![build_close_leg(
            direction,
            entry_price,
            quantity,
            close_price,
            close_ts,
            exit_reason,
            result_r,
            true,
        )],
    )
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_multi_leg_runner_trade(
    event: &ConfirmedEvent,
    open_position_time: String,
    entry_price: f64,
    quantity: f64,
    close_legs: Vec<FrameworkEquityCloseLegReport>,
) -> RunnerReplayTrade {
    let close_leg = close_legs
        .last()
        .expect("runner replay trade should have at least one close leg");
    let profit_loss = close_legs.iter().map(|leg| leg.profit_loss).sum::<f64>();
    let close_type = close_legs
        .iter()
        .map(|leg| leg.close_type.clone())
        .collect::<Vec<_>>()
        .join("+");
    RunnerReplayTrade {
        event_id: event.event.id,
        open_position_time,
        close_position_time: close_leg.close_position_time.clone(),
        open_price: entry_price,
        close_price: close_leg.close_price,
        quantity,
        profit_loss,
        close_type,
        result_r: close_legs
            .iter()
            .map(|leg| leg.result_r * leg.quantity / quantity)
            .sum(),
        close_legs,
    }
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_close_leg(
    direction: MarketVelocityTradeDirection,
    entry_price: f64,
    quantity: f64,
    close_price: f64,
    close_ts: i64,
    exit_reason: &str,
    result_r: f64,
    full_close: bool,
) -> FrameworkEquityCloseLegReport {
    let raw_profit = match direction {
        MarketVelocityTradeDirection::Long => quantity * (close_price - entry_price),
        MarketVelocityTradeDirection::Short => quantity * (entry_price - close_price),
        MarketVelocityTradeDirection::Both => 0.0,
    };
    let fee = if raw_profit != 0.0 {
        quantity * entry_price * 0.0007
    } else {
        0.0
    };
    FrameworkEquityCloseLegReport {
        close_ts,
        close_position_time: timestamp_ms_to_shanghai_datetime(close_ts),
        close_price,
        close_type: exit_reason.to_string(),
        profit_loss: raw_profit - fee,
        quantity,
        full_close,
        exit_reason: exit_reason.to_string(),
        result_r,
    }
}
/// 提供交易离场ts的集中实现，避免回测策略调用方重复处理相同细节。
fn trade_exit_ts(trade: &RunnerReplayTrade) -> i64 {
    trade
        .close_legs
        .last()
        .map(|leg| leg.close_ts)
        .unwrap_or(i64::MAX)
}
/// 停止 回测与策略研究 后台流程，确保退出时不留下未释放状态。
fn stop_price_for(
    entry_price: f64,
    stop_loss_pct: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => entry_price * (1.0 - stop_loss_pct),
        MarketVelocityTradeDirection::Short => entry_price * (1.0 + stop_loss_pct),
        MarketVelocityTradeDirection::Both => entry_price,
    }
}
/// 提供目标价格for的集中实现，避免回测策略调用方重复处理相同细节。
fn target_price_for(
    entry_price: f64,
    stop_loss_pct: f64,
    target_r: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => entry_price * (1.0 + stop_loss_pct * target_r),
        MarketVelocityTradeDirection::Short => entry_price * (1.0 - stop_loss_pct * target_r),
        MarketVelocityTradeDirection::Both => entry_price,
    }
}
/// 提供hit止损的集中实现，避免回测策略调用方重复处理相同细节。
fn hit_stop(
    candle_low: f64,
    candle_high: f64,
    stop_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle_low <= stop_price,
        MarketVelocityTradeDirection::Short => candle_high >= stop_price,
        MarketVelocityTradeDirection::Both => false,
    }
}
/// 提供hit目标的集中实现，避免回测策略调用方重复处理相同细节。
fn hit_target(
    candle_low: f64,
    candle_high: f64,
    target_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle_high >= target_price,
        MarketVelocityTradeDirection::Short => candle_low <= target_price,
        MarketVelocityTradeDirection::Both => false,
    }
}
/// 停止 回测与策略研究 后台流程，确保退出时不留下未释放状态。
fn stop_already_crossed(
    close_price: f64,
    stop_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => close_price <= stop_price,
        MarketVelocityTradeDirection::Short => close_price >= stop_price,
        MarketVelocityTradeDirection::Both => true,
    }
}
/// 提供no盈利平仓的集中实现，避免回测策略调用方重复处理相同细节。
fn no_profit_close(
    close_price: f64,
    entry_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => close_price <= entry_price,
        MarketVelocityTradeDirection::Short => close_price >= entry_price,
        MarketVelocityTradeDirection::Both => false,
    }
}
/// 提供rfor价格的集中实现，避免回测策略调用方重复处理相同细节。
fn r_for_price(
    entry_price: f64,
    stop_loss_pct: f64,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => (price - entry_price) / (entry_price * stop_loss_pct),
        MarketVelocityTradeDirection::Short => {
            (entry_price - price) / (entry_price * stop_loss_pct)
        }
        MarketVelocityTradeDirection::Both => 0.0,
    }
}
/// 把数据加入 回测与策略研究 聚合结果，保持集合构造逻辑集中。
fn push_feature_report<F>(
    reports: &mut Vec<FrameworkEquityFeatureReport>,
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
    feature: &'static str,
    bucket: &'static str,
    include: F,
) where
    F: Fn(&ConfirmedEvent) -> bool,
{
    let events = confirmed
        .iter()
        .filter(|event| include(event))
        .cloned()
        .collect::<Vec<_>>();
    if !events.is_empty() {
        reports.push(FrameworkEquityFeatureReport {
            feature,
            bucket,
            report: build_framework_equity_report(&events, candles_15m, target_r, args),
        });
    }
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_replay_open_trade(record: &TradeRecord) -> Option<ReplayOpenTrade> {
    if record.option_type != "long" && record.option_type != "short" {
        return None;
    }
    let payload = record.signal_value.as_deref()?;
    let value = serde_json::from_str::<Value>(payload).ok()?;
    let event_id = value.get("rank_event_id")?.as_i64()?;
    let trigger = value
        .get("entry_trigger")
        .and_then(Value::as_str)
        .unwrap_or("NA")
        .to_string();
    Some(ReplayOpenTrade {
        event_id,
        trigger,
        open_position_time: shanghai_offset_label(record.open_position_time.clone()),
        open_price: record.open_price,
        quantity: record.quantity,
        signal_status: record.signal_status,
    })
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_backtest_details(
    trade: &FrameworkEquityTradeReport,
    back_test_id: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<BacktestDetail>> {
    let close_position_time = trade
        .close_position_time
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("market velocity trade missing close_position_time"))?;
    let signal_value = market_velocity_detail_signal_value(trade, args).to_string();
    let signal_result = "market_velocity_framework_replay".to_string();
    let strategy_type = market_velocity_strategy_type(args).to_string();
    let open_option_type = if trade.price_change_pct < 0.0 {
        "short"
    } else {
        "long"
    };
    let open_price = trade.open_price.to_string();
    let quantity = trade.quantity.to_string();
    let (win_nums, loss_nums) = match trade.outcome {
        "win" => (1, 0),
        "loss" => (0, 1),
        _ => (0, 0),
    };
    let mut details = vec![BacktestDetail::new(
        back_test_id,
        open_option_type.to_string(),
        strategy_type.clone(),
        trade.symbol.clone(),
        "15m".to_string(),
        trade.open_position_time.clone(),
        Some(trade.signal_open_position_time.clone()),
        trade.signal_status,
        trade.open_position_time.clone(),
        open_price.clone(),
        None,
        "0".to_string(),
        quantity.clone(),
        "false".to_string(),
        String::new(),
        0,
        0,
        signal_value.clone(),
        signal_result.clone(),
        None,
        None,
    )];
    if trade.close_legs.is_empty() {
        details.push(BacktestDetail::new(
            back_test_id,
            "close".to_string(),
            strategy_type,
            trade.symbol.clone(),
            "15m".to_string(),
            trade.open_position_time.clone(),
            Some(trade.signal_open_position_time.clone()),
            trade.signal_status,
            close_position_time,
            open_price,
            trade.close_price.map(|value| value.to_string()),
            trade.profit_loss.to_string(),
            quantity,
            "true".to_string(),
            trade.close_type.clone(),
            win_nums,
            loss_nums,
            signal_value,
            signal_result,
            None,
            None,
        ));
        return Ok(details);
    }
    for leg in &trade.close_legs {
        let leg_signal_value =
            market_velocity_detail_signal_value_for_leg(trade, args, leg).to_string();
        let (leg_win_nums, leg_loss_nums) = if leg.full_close {
            (win_nums, loss_nums)
        } else {
            (0, 0)
        };
        details.push(BacktestDetail::new(
            back_test_id,
            "close".to_string(),
            strategy_type.clone(),
            trade.symbol.clone(),
            "15m".to_string(),
            trade.open_position_time.clone(),
            Some(trade.signal_open_position_time.clone()),
            trade.signal_status,
            leg.close_position_time.clone(),
            open_price.clone(),
            Some(leg.close_price.to_string()),
            leg.profit_loss.to_string(),
            leg.quantity.to_string(),
            leg.full_close.to_string(),
            leg.close_type.clone(),
            leg_win_nums,
            leg_loss_nums,
            leg_signal_value,
            signal_result.clone(),
            None,
            None,
        ));
    }
    Ok(details)
}
/// 提供市场动量策略type的集中实现，避免回测策略调用方重复处理相同细节。
pub fn market_velocity_strategy_type(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    match args.event_source {
        super::MarketVelocityEventSource::Episodes => "market_velocity_episode",
        super::MarketVelocityEventSource::RawEvents => "market_velocity_raw_events",
        super::MarketVelocityEventSource::RawState => "market_velocity_raw_state",
    }
}
/// 提供市场动量detail信号值的集中实现，避免回测策略调用方重复处理相同细节。
fn market_velocity_detail_signal_value(
    trade: &FrameworkEquityTradeReport,
    args: &MarketVelocityEventBacktestArgs,
) -> Value {
    json!({
        "source": "market_velocity_framework_replay",
        "rank_event_id": trade.event_id,
        "detected_at": &trade.detected_at,
        "entry_ts": trade.entry_ts,
        "entry_trigger": &trade.trigger,
        "trade_direction": if trade.price_change_pct < 0.0 { "short" } else { "long" },
        "new_rank": trade.new_rank,
        "delta_rank": trade.delta_rank,
        "price_change_pct": trade.price_change_pct,
        "target_r": trade.target_r,
        "stop_loss_pct": args.stop_loss_pct,
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "event_source": match args.event_source {
            super::MarketVelocityEventSource::Episodes => "episodes",
            super::MarketVelocityEventSource::RawEvents => "raw_events",
            super::MarketVelocityEventSource::RawState => "raw_state",
        },
    })
}
/// 提供市场动量detail信号值forleg的集中实现，避免回测策略调用方重复处理相同细节。
fn market_velocity_detail_signal_value_for_leg(
    trade: &FrameworkEquityTradeReport,
    args: &MarketVelocityEventBacktestArgs,
    leg: &FrameworkEquityCloseLegReport,
) -> Value {
    let mut value = market_velocity_detail_signal_value(trade, args);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "exit_reason".to_string(),
            Value::String(leg.exit_reason.clone()),
        );
        object.insert(
            "runner_target_r".to_string(),
            args.runner_target_r.map_or(Value::Null, Value::from),
        );
        object.insert(
            "runner_fraction".to_string(),
            Value::from(args.runner_fraction),
        );
        object.insert("runner_stop_r".to_string(), Value::from(args.runner_stop_r));
        object.insert("leg_result_r".to_string(), Value::from(leg.result_r));
        object.insert("leg_full_close".to_string(), Value::from(leg.full_close));
    }
    value
}
fn timestamp_ms_to_shanghai_datetime(timestamp_ms: i64) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid shanghai fixed offset");
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .unwrap_or_else(|| {
            Utc.timestamp_millis_opt(0)
                .single()
                .expect("unix epoch timestamp should be valid")
        })
        .with_timezone(&offset)
        .format("%Y-%m-%d %H:%M:%S%:z")
        .to_string()
}
fn shanghai_offset_label(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.ends_with("+08:00") || trimmed.ends_with('Z') {
        return value;
    }
    format!("{trimmed}+08:00")
}
/// 提供交易结果标签的集中实现，避免回测策略调用方重复处理相同细节。
fn trade_outcome_label(profit_loss: f64) -> &'static str {
    if profit_loss > 0.0 {
        "win"
    } else if profit_loss < 0.0 {
        "loss"
    } else {
        "flat"
    }
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_framework_equity_concentration_reports(
    report: &FrameworkEquityReport,
) -> Vec<FrameworkEquityConcentrationReport> {
    let mut positive_symbols = report
        .symbols
        .iter()
        .filter(|symbol| symbol.profit > 0.0)
        .collect::<Vec<_>>();
    positive_symbols.sort_by(|left, right| {
        right
            .profit
            .partial_cmp(&left.profit)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    [1, 3, 5]
        .into_iter()
        .filter(|count| positive_symbols.len() >= *count)
        .map(|count| {
            let removed = &positive_symbols[..count];
            let removed_symbols = removed
                .iter()
                .map(|symbol| symbol.symbol.clone())
                .collect::<Vec<_>>();
            let removed_profit = removed.iter().map(|symbol| symbol.profit).sum::<f64>();
            let remaining = report
                .symbols
                .iter()
                .filter(|symbol| {
                    !removed_symbols
                        .iter()
                        .any(|removed| removed == &symbol.symbol)
                })
                .collect::<Vec<_>>();
            let remaining_open_trades = remaining.iter().map(|symbol| symbol.open_trades).sum();
            let remaining_wins = remaining.iter().map(|symbol| symbol.wins).sum::<usize>();
            let remaining_losses = remaining.iter().map(|symbol| symbol.losses).sum::<usize>();
            let remaining_resolved = remaining_wins + remaining_losses;
            let remaining_win_rate = (remaining_resolved > 0)
                .then_some(remaining_wins as f64 / remaining_resolved as f64 * 100.0);
            let remaining_max_drawdown_pct = remaining
                .iter()
                .map(|symbol| symbol.max_drawdown_pct)
                .fold(0.0, f64::max);
            FrameworkEquityConcentrationReport {
                target_r: report.target_r,
                min_trades: report.min_trades,
                removed_top_positive: count,
                removed_symbols,
                removed_profit,
                removed_share_pct: (report.total_profit > 0.0)
                    .then_some(removed_profit / report.total_profit * 100.0),
                remaining_symbols: remaining.len(),
                remaining_open_trades,
                remaining_total_profit: report.total_profit - removed_profit,
                remaining_win_rate,
                remaining_max_drawdown_pct,
                remaining_meets_min_trades: remaining_open_trades >= report.min_trades,
            }
        })
        .collect()
}
/// 执行输出框架equity报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_report(report: &FrameworkEquityReport, sample_limit: usize) {
    println!(
        "framework_equity_result\ttarget={}R\tmode=symbol_isolated_100u\tmin_trades={}\tmeets_min_trades={}\tsymbols={}\ttrades={}\twin_rate={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}\ttotal_profit={:.8}",
        report.target_r,
        report.min_trades,
        report.meets_min_trades,
        report.symbols.len(),
        report.total_open_trades,
        format_optional_f64(report.win_rate),
        format_optional_f64(report.trade_sharpe),
        report.max_drawdown_pct,
        report.total_profit,
    );
    for symbol in report.symbols.iter().take(sample_limit) {
        println!(
            "framework_equity_symbol\ttarget={}R\t{}\ttrades={}\tprofit={:.8}\tfinal_fund={:.8}\twins={}\tlosses={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}",
            report.target_r,
            symbol.symbol,
            symbol.open_trades,
            symbol.profit,
            symbol.final_fund,
            symbol.wins,
            symbol.losses,
            format_optional_f64(symbol.trade_sharpe),
            symbol.max_drawdown_pct,
        );
    }
}
/// 执行输出框架equity分组报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_split_reports(reports: &[FrameworkEquitySplitReport]) {
    for split in reports {
        let report = &split.report;
        println!(
            "framework_equity_split_result\ttarget={}R\tmode=symbol_isolated_100u\tsplit={}\tstart_entry_ts={}\tend_entry_ts={}\tmin_trades={}\tmeets_min_trades={}\tsymbols={}\ttrades={}\twin_rate={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}\ttotal_profit={:.8}",
            report.target_r,
            split.label,
            split.start_entry_ts,
            split.end_entry_ts,
            report.min_trades,
            report.meets_min_trades,
            report.symbols.len(),
            report.total_open_trades,
            format_optional_f64(report.win_rate),
            format_optional_f64(report.trade_sharpe),
            report.max_drawdown_pct,
            report.total_profit,
        );
    }
}
/// 执行输出框架equity四分位报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_quartile_reports(reports: &[FrameworkEquitySplitReport]) {
    for split in reports {
        let report = &split.report;
        println!(
            "framework_equity_quartile_result\ttarget={}R\tmode=symbol_isolated_100u\tquartile={}\tstart_entry_ts={}\tend_entry_ts={}\tmin_trades={}\tmeets_min_trades={}\tsymbols={}\ttrades={}\twin_rate={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}\ttotal_profit={:.8}",
            report.target_r,
            split.label,
            split.start_entry_ts,
            split.end_entry_ts,
            report.min_trades,
            report.meets_min_trades,
            report.symbols.len(),
            report.total_open_trades,
            format_optional_f64(report.win_rate),
            format_optional_f64(report.trade_sharpe),
            report.max_drawdown_pct,
            report.total_profit,
        );
    }
}
/// 执行输出框架equity触发报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_trigger_reports(reports: &[FrameworkEquityTriggerReport]) {
    for trigger in reports {
        let report = &trigger.report;
        println!(
            "framework_equity_trigger_result\ttarget={}R\tmode=symbol_isolated_100u\ttrigger={}\tmin_trades={}\tmeets_min_trades={}\tsymbols={}\ttrades={}\twin_rate={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}\ttotal_profit={:.8}",
            report.target_r,
            trigger.trigger,
            report.min_trades,
            report.meets_min_trades,
            report.symbols.len(),
            report.total_open_trades,
            format_optional_f64(report.win_rate),
            format_optional_f64(report.trade_sharpe),
            report.max_drawdown_pct,
            report.total_profit,
        );
    }
}
/// 执行输出框架equity集中度报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_concentration_reports(
    reports: &[FrameworkEquityConcentrationReport],
) {
    for report in reports {
        println!(
            "framework_equity_concentration_result\ttarget={}R\tmode=symbol_isolated_100u\tremoved_top_positive={}\tremoved_symbols={}\tremoved_profit={:.8}\tremoved_share_pct={}\tremaining_symbols={}\tremaining_trades={}\tremaining_win_rate={}\tremaining_max_drawdown_pct={:.8}\tremaining_total_profit={:.8}\tremaining_meets_min_trades={}",
            report.target_r,
            report.removed_top_positive,
            report.removed_symbols.join(","),
            report.removed_profit,
            format_optional_f64(report.removed_share_pct),
            report.remaining_symbols,
            report.remaining_open_trades,
            format_optional_f64(report.remaining_win_rate),
            report.remaining_max_drawdown_pct,
            report.remaining_total_profit,
            report.remaining_meets_min_trades,
        );
    }
}
/// 执行输出框架equity特征报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_feature_reports(reports: &[FrameworkEquityFeatureReport]) {
    for feature in reports {
        let report = &feature.report;
        println!(
            "framework_equity_feature_result\ttarget={}R\tmode=symbol_isolated_100u\tfeature={}\tbucket={}\tmin_trades={}\tmeets_min_trades={}\tsymbols={}\ttrades={}\twin_rate={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}\ttotal_profit={:.8}",
            report.target_r,
            feature.feature,
            feature.bucket,
            report.min_trades,
            report.meets_min_trades,
            report.symbols.len(),
            report.total_open_trades,
            format_optional_f64(report.win_rate),
            format_optional_f64(report.trade_sharpe),
            report.max_drawdown_pct,
            report.total_profit,
        );
    }
}
/// 执行输出框架equity交易对窗口报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_symbol_window_reports(reports: &[FrameworkEquitySymbolWindowReport]) {
    for window in reports {
        let report = &window.split.report;
        println!(
            "framework_equity_symbol_window_result\ttarget={}R\tmode=symbol_isolated_100u\twindow={}\tstart_entry_ts={}\tend_entry_ts={}\tmin_trades={}\tmeets_min_trades={}\tsymbols={}\ttrades={}\twin_rate={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}\ttotal_profit={:.8}",
            report.target_r,
            window.split.label,
            window.split.start_entry_ts,
            window.split.end_entry_ts,
            report.min_trades,
            report.meets_min_trades,
            report.symbols.len(),
            report.total_open_trades,
            format_optional_f64(report.win_rate),
            format_optional_f64(report.trade_sharpe),
            report.max_drawdown_pct,
            report.total_profit,
        );
        for (index, symbol) in window.top_symbols.iter().enumerate() {
            println!(
                "framework_equity_symbol_window_symbol\ttarget={}R\twindow={}\trank={}\tsymbol={}\ttrades={}\tprofit={:.8}\tfinal_fund={:.8}\twins={}\tlosses={}\ttrade_sharpe={}\tmax_drawdown_pct={:.8}",
                report.target_r,
                window.split.label,
                index + 1,
                symbol.symbol,
                symbol.open_trades,
                symbol.profit,
                symbol.final_fund,
                symbol.wins,
                symbol.losses,
                format_optional_f64(symbol.trade_sharpe),
                symbol.max_drawdown_pct,
            );
        }
    }
}
/// 执行输出框架equity交易报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_trade_reports(reports: &[FrameworkEquityTradeReport]) {
    for trade in reports {
        println!(
            "framework_equity_trade\ttarget={}R\tmode=symbol_isolated_100u\tsymbol={}\tevent_id={}\tdetected_at={}\tentry_ts={}\topen_time={}\tclose_time={}\topen_price={}\tclose_price={}\tquantity={}\tclose_type={}\tprofit_loss={:.8}\toutcome={}\ttrigger={}\tnew_rank={}\tdelta_rank={}\tprice_change_pct={}",
            trade.target_r,
            trade.symbol,
            trade.event_id,
            trade.detected_at,
            trade.entry_ts,
            trade.open_position_time,
            trade.close_position_time.as_deref().unwrap_or("NA"),
            trade.open_price,
            trade
                .close_price
                .map(|value| value.to_string())
                .unwrap_or_else(|| "NA".to_string()),
            trade.quantity,
            trade.close_type,
            trade.profit_loss,
            trade.outcome,
            trade.trigger,
            trade.new_rank,
            trade.delta_rank,
            trade.price_change_pct,
        );
    }
}
/// 执行输出框架equity报告步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_framework_equity_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) {
    for target_r in &args.target_rs {
        let report = build_framework_equity_report(confirmed, candles_15m, *target_r, args);
        if args.equity_report {
            print_framework_equity_report(&report, args.sample_limit);
        }
        if args.equity_split_report {
            let split_reports =
                build_framework_equity_split_reports(confirmed, candles_15m, *target_r, args);
            print_framework_equity_split_reports(&split_reports);
        }
        if args.equity_quartile_report {
            let quartile_reports =
                build_framework_equity_quartile_reports(confirmed, candles_15m, *target_r, args);
            print_framework_equity_quartile_reports(&quartile_reports);
        }
        if args.equity_trigger_report {
            let trigger_reports =
                build_framework_equity_trigger_reports(confirmed, candles_15m, *target_r, args);
            print_framework_equity_trigger_reports(&trigger_reports);
        }
        if args.equity_concentration_report {
            let concentration_reports = build_framework_equity_concentration_reports(&report);
            print_framework_equity_concentration_reports(&concentration_reports);
        }
        if args.equity_feature_report {
            let feature_reports =
                build_framework_equity_feature_reports(confirmed, candles_15m, *target_r, args);
            print_framework_equity_feature_reports(&feature_reports);
        }
        if args.equity_symbol_window_report {
            let window_reports = build_framework_equity_symbol_window_reports(
                confirmed,
                candles_15m,
                *target_r,
                args,
                args.sample_limit,
            );
            print_framework_equity_symbol_window_reports(&window_reports);
        }
        if args.equity_trade_report {
            let trade_reports =
                build_framework_equity_trade_reports(confirmed, candles_15m, *target_r, args);
            print_framework_equity_trade_reports(&trade_reports);
        }
    }
}
impl MarketVelocityReplayStrategy {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    fn new(
        events: Vec<ConfirmedEvent>,
        args: &MarketVelocityEventBacktestArgs,
        target_r: f64,
        profit_protect_after_r: Option<f64>,
        profit_protect_stop_r: f64,
        early_exit_no_profit_candles: Option<usize>,
        ignore_entry_signal_updates_while_open: bool,
    ) -> Self {
        let entries_by_ts = events
            .into_iter()
            .map(|event| {
                let selected_stop_loss = select_stop_loss_for_confirmed_signal(&event, args);
                (
                    event.entry_ts,
                    ReplayEntry {
                        entry_price: event.entry_price,
                        event_id: event.event.id,
                        trigger: event.trigger,
                        direction: trade_direction_for_event(&event.event),
                        stop_loss_pct: selected_stop_loss.stop_loss_pct,
                        stop_loss_price: selected_stop_loss.price,
                        stop_loss_source: selected_stop_loss.source,
                    },
                )
            })
            .collect();
        Self {
            entries_by_ts,
            target_r,
            profit_protect_after_r,
            profit_protect_stop_r,
            early_exit_no_profit_candles,
            ignore_entry_signal_updates_while_open,
            active_position: None,
        }
    }
    /// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
    fn build_entry_signal(&mut self, candle_ts: i64, entry: &ReplayEntry) -> SignalResult {
        self.active_position = Some(ReplayActivePosition {
            entry_price: entry.entry_price,
            event_id: entry.event_id,
            trigger: entry.trigger.clone(),
            direction: entry.direction,
            stop_loss_pct: entry.stop_loss_pct,
            stop_loss_price: entry.stop_loss_price,
            stop_loss_source: entry.stop_loss_source.clone(),
            profit_protected: false,
            observed_candles: 0,
        });
        self.build_entry_direction_signal(
            candle_ts,
            entry.entry_price,
            entry.stop_loss_price,
            entry.stop_loss_pct,
            &entry.stop_loss_source,
            entry.event_id,
            &entry.trigger,
            entry.direction,
            false,
        )
    }
    /// 判断按条件build盈利保护信号，给回测策略流程提供布尔结果。
    fn maybe_build_profit_protection_signal(
        &mut self,
        candle: &CandleItem,
    ) -> Option<SignalResult> {
        let after_r = self.profit_protect_after_r?;
        let active = self.active_position.as_mut()?;
        let target_price = target_price_for(
            active.entry_price,
            active.stop_loss_pct,
            self.target_r,
            active.direction,
        );
        let current_stop_price = if active.profit_protected {
            target_price_for(
                active.entry_price,
                active.stop_loss_pct,
                self.profit_protect_stop_r,
                active.direction,
            )
        } else {
            active.stop_loss_price
        };
        if hit_stop(candle.l, candle.h, current_stop_price, active.direction)
            || hit_target(candle.l, candle.h, target_price, active.direction)
        {
            self.active_position = None;
            return None;
        }
        if active.profit_protected {
            return None;
        }
        let trigger_price = target_price_for(
            active.entry_price,
            active.stop_loss_pct,
            after_r,
            active.direction,
        );
        if !hit_target(candle.l, candle.h, trigger_price, active.direction) {
            return None;
        }
        let entry_price = active.entry_price;
        let event_id = active.event_id;
        let trigger = active.trigger.clone();
        let direction = active.direction;
        let stop_loss_pct = active.stop_loss_pct;
        let protected_stop_price = target_price_for(
            entry_price,
            stop_loss_pct,
            self.profit_protect_stop_r,
            direction,
        );
        if stop_already_crossed(candle.c, protected_stop_price, direction) {
            return None;
        }
        active.profit_protected = true;
        Some(self.build_entry_direction_signal(
            candle.ts,
            entry_price,
            protected_stop_price,
            stop_loss_pct,
            "MarketVelocityProfitProtect",
            event_id,
            &trigger,
            direction,
            true,
        ))
    }
    /// 判断按条件buildearly离场信号，给回测策略流程提供布尔结果。
    fn maybe_build_early_exit_signal(&mut self, candle: &CandleItem) -> Option<SignalResult> {
        let no_profit_candles = self.early_exit_no_profit_candles?;
        let active = self.active_position.as_mut()?;
        active.observed_candles += 1;
        if active.observed_candles < no_profit_candles
            || !no_profit_close(candle.c, active.entry_price, active.direction)
        {
            return None;
        }
        let event_id = active.event_id;
        let trigger = active.trigger.clone();
        let direction = active.direction;
        self.active_position = None;
        Some(SignalResult {
            should_buy: direction == MarketVelocityTradeDirection::Short,
            should_sell: direction == MarketVelocityTradeDirection::Long,
            open_price: candle.c,
            ts: candle.ts,
            single_value: Some(
                json!({
                    "source": "market_velocity_framework_replay",
                    "rank_event_id": event_id,
                    "entry_trigger": trigger,
                    "exit_reason": "early_exit_no_profit",
                    "no_profit_candles": no_profit_candles,
                })
                .to_string(),
            ),
            single_result: Some("market_velocity_framework_replay".to_string()),
            filter_reasons: vec![match direction {
                MarketVelocityTradeDirection::Long => "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT",
                MarketVelocityTradeDirection::Short => "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG",
                MarketVelocityTradeDirection::Both => "MARKET_VELOCITY_EARLY_EXIT",
            }
            .to_string()],
            direction: match direction {
                MarketVelocityTradeDirection::Long => SignalDirection::Short,
                MarketVelocityTradeDirection::Short => SignalDirection::Long,
                MarketVelocityTradeDirection::Both => SignalDirection::None,
            },
            ..SignalResult::default()
        })
    }
    fn clear_active_position_if_exit_hit(&mut self, candle: &CandleItem) {
        let should_clear = self.active_position.as_ref().is_some_and(|active| {
            let target_price = target_price_for(
                active.entry_price,
                active.stop_loss_pct,
                self.target_r,
                active.direction,
            );
            let stop_price = if active.profit_protected {
                target_price_for(
                    active.entry_price,
                    active.stop_loss_pct,
                    self.profit_protect_stop_r,
                    active.direction,
                )
            } else {
                active.stop_loss_price
            };
            hit_stop(candle.l, candle.h, stop_price, active.direction)
                || hit_target(candle.l, candle.h, target_price, active.direction)
        });
        if should_clear {
            self.active_position = None;
        }
    }
    fn should_ignore_entry_update(&self, entry: &ReplayEntry) -> bool {
        self.ignore_entry_signal_updates_while_open
            && self
                .active_position
                .as_ref()
                .is_some_and(|active| active.direction == entry.direction)
    }
    /// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
    fn build_entry_direction_signal(
        &self,
        candle_ts: i64,
        entry_price: f64,
        stop_loss_price: f64,
        stop_loss_pct: f64,
        stop_loss_source: &str,
        event_id: i64,
        trigger: &str,
        direction: MarketVelocityTradeDirection,
        profit_protected: bool,
    ) -> SignalResult {
        SignalResult {
            should_buy: direction == MarketVelocityTradeDirection::Long,
            should_sell: direction == MarketVelocityTradeDirection::Short,
            open_price: entry_price,
            signal_kline_stop_loss_price: Some(stop_loss_price),
            stop_loss_source: Some(stop_loss_source.to_string()),
            long_signal_take_profit_price: (direction == MarketVelocityTradeDirection::Long)
                .then_some(target_price_for(
                    entry_price,
                    stop_loss_pct,
                    self.target_r,
                    direction,
                )),
            short_signal_take_profit_price: (direction == MarketVelocityTradeDirection::Short)
                .then_some(target_price_for(
                    entry_price,
                    stop_loss_pct,
                    self.target_r,
                    direction,
                )),
            ts: candle_ts,
            single_value: Some(
                json!({
                    "source": "market_velocity_framework_replay",
                    "rank_event_id": event_id,
                    "entry_trigger": trigger,
                    "trade_direction": direction.label(),
                    "target_r": self.target_r,
                    "stop_loss_pct": stop_loss_pct,
                    "profit_protected": profit_protected,
                })
                .to_string(),
            ),
            single_result: Some("market_velocity_framework_replay".to_string()),
            direction: match direction {
                MarketVelocityTradeDirection::Long => SignalDirection::Long,
                MarketVelocityTradeDirection::Short => SignalDirection::Short,
                MarketVelocityTradeDirection::Both => SignalDirection::None,
            },
            ..SignalResult::default()
        }
    }
}
impl IndicatorStrategyBacktest for MarketVelocityReplayStrategy {
    type IndicatorCombine = ();
    type IndicatorValues = ();
    fn min_data_length(&self) -> usize {
        1
    }
    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}
    fn build_indicator_values(
        _: &mut Self::IndicatorCombine,
        _: &CandleItem,
    ) -> Self::IndicatorValues {
    }
    /// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _: &mut Self::IndicatorValues,
        _: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let Some(candle) = candles.last() else {
            return SignalResult::default();
        };
        if self.ignore_entry_signal_updates_while_open {
            self.clear_active_position_if_exit_hit(candle);
        }
        if let Some(entry) = self.entries_by_ts.get(&candle.ts).cloned() {
            if !self.should_ignore_entry_update(&entry) {
                return self.build_entry_signal(candle.ts, &entry);
            }
        }
        if let Some(signal) = self.maybe_build_early_exit_signal(candle) {
            return signal;
        }
        if let Some(signal) = self.maybe_build_profit_protection_signal(candle) {
            return signal;
        }
        SignalResult {
            ts: candle.ts,
            open_price: candle.c,
            ..SignalResult::default()
        }
    }
}
/// 将内部模型转换为输出结构，避免 回测与策略研究 的内部字段直接外泄。
fn to_candle_item(candle: &BacktestCandle) -> CandleItem {
    CandleItem {
        o: candle.open,
        h: candle.high,
        l: candle.low,
        c: candle.close,
        v: candle.volume,
        ts: candle.ts,
        confirm: 1,
    }
}
fn framework_replay_candle_items(candles: &[BacktestCandle]) -> Vec<CandleItem> {
    let Some(first) = candles.first() else {
        return Vec::new();
    };
    let interval_ms = candles
        .get(1)
        .map(|second| second.ts - first.ts)
        .filter(|interval| *interval > 0)
        .unwrap_or(super::MS_15M);
    let mut items = Vec::with_capacity(candles.len() + FRAMEWORK_SIGNAL_WARMUP_CANDLES);
    for offset in (1..=FRAMEWORK_SIGNAL_WARMUP_CANDLES).rev() {
        let ts = first
            .ts
            .saturating_sub(interval_ms.saturating_mul(offset as i64));
        items.push(CandleItem {
            o: first.open,
            h: first.open,
            l: first.open,
            c: first.open,
            v: 0.0,
            ts,
            confirm: 1,
        });
    }
    items.extend(candles.iter().map(to_candle_item));
    items
}
