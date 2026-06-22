use super::{BacktestCandle, ConfirmedEvent, MarketVelocityEventBacktestArgs};
use rust_quant_domain::SignalDirection;
use rust_quant_strategies::framework::backtest::{
    run_indicator_strategy_backtest, BasicRiskStrategyConfig, IndicatorStrategyBacktest,
    SignalResult, TradeRecord,
};
use rust_quant_strategies::CandleItem;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

const INITIAL_FUND_PER_SYMBOL: f64 = 100.0;

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityReport {
    pub target_r: f64,
    pub initial_fund_per_symbol: f64,
    pub min_trades: usize,
    pub total_open_trades: usize,
    pub total_profit: f64,
    pub win_rate: Option<f64>,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_pct: f64,
    pub meets_min_trades: bool,
    pub symbols: Vec<FrameworkEquitySymbolReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquitySymbolReport {
    pub symbol: String,
    pub open_trades: usize,
    pub final_fund: f64,
    pub profit: f64,
    pub wins: usize,
    pub losses: usize,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquitySplitReport {
    pub label: &'static str,
    pub start_entry_ts: i64,
    pub end_entry_ts: i64,
    pub report: FrameworkEquityReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityTriggerReport {
    pub trigger: String,
    pub report: FrameworkEquityReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityFeatureReport {
    pub feature: &'static str,
    pub bucket: &'static str,
    pub report: FrameworkEquityReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquitySymbolWindowReport {
    pub split: FrameworkEquitySplitReport,
    pub top_symbols: Vec<FrameworkEquitySymbolReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityTradeReport {
    pub target_r: f64,
    pub symbol: String,
    pub event_id: i64,
    pub detected_at: String,
    pub entry_ts: i64,
    pub open_position_time: String,
    pub close_position_time: Option<String>,
    pub close_type: String,
    pub profit_loss: f64,
    pub outcome: &'static str,
    pub trigger: String,
    pub new_rank: i32,
    pub delta_rank: i32,
    pub price_change_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameworkEquityConcentrationReport {
    pub target_r: f64,
    pub min_trades: usize,
    pub removed_top_positive: usize,
    pub removed_symbols: Vec<String>,
    pub removed_profit: f64,
    pub removed_share_pct: Option<f64>,
    pub remaining_symbols: usize,
    pub remaining_open_trades: usize,
    pub remaining_total_profit: f64,
    pub remaining_win_rate: Option<f64>,
    pub remaining_max_drawdown_pct: f64,
    pub remaining_meets_min_trades: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct ReplayEntry {
    entry_price: f64,
    event_id: i64,
    trigger: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ReplayActivePosition {
    entry_price: f64,
    event_id: i64,
    trigger: String,
    profit_protected: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct ReplayOpenTrade {
    event_id: i64,
    trigger: String,
    open_position_time: String,
}

#[derive(Debug, Clone)]
struct MarketVelocityReplayStrategy {
    entries_by_ts: BTreeMap<i64, ReplayEntry>,
    stop_loss_pct: f64,
    target_r: f64,
    profit_protect_after_r: Option<f64>,
    profit_protect_stop_r: f64,
    active_position: Option<ReplayActivePosition>,
}

#[derive(Debug, Clone, PartialEq)]
struct ClosedTradeStats {
    wins: usize,
    losses: usize,
    returns: Vec<f64>,
    max_drawdown_pct: f64,
}

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
        let strategy = MarketVelocityReplayStrategy::new(
            confirmed
                .iter()
                .filter(|event| event.event.symbol == symbol)
                .cloned()
                .collect(),
            args.stop_loss_pct,
            target_r,
            args.profit_protect_after_r,
            args.profit_protect_stop_r,
        );
        let candle_items = candles.iter().map(to_candle_item).collect::<Vec<_>>();
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: args.stop_loss_pct,
            is_used_signal_k_line_stop_loss: Some(true),
            dynamic_max_loss: Some(false),
            ..BasicRiskStrategyConfig::default()
        };
        let result = run_indicator_strategy_backtest(&symbol, strategy, &candle_items, risk_config);
        let closed_stats = analyze_closed_trades(&result.trade_records);
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
        "new_rank",
        "1_10",
        |event| (1..=10).contains(&event.event.new_rank),
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "new_rank",
        "11_20",
        |event| (11..=20).contains(&event.event.new_rank),
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "new_rank",
        "21_30",
        |event| (21..=30).contains(&event.event.new_rank),
    );
    push_feature_report(
        &mut reports,
        confirmed,
        candles_15m,
        target_r,
        args,
        "new_rank",
        "31_plus",
        |event| event.event.new_rank >= 31,
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
        let strategy = MarketVelocityReplayStrategy::new(
            confirmed
                .iter()
                .filter(|event| event.event.symbol == symbol)
                .cloned()
                .collect(),
            args.stop_loss_pct,
            target_r,
            args.profit_protect_after_r,
            args.profit_protect_stop_r,
        );
        let candle_items = candles.iter().map(to_candle_item).collect::<Vec<_>>();
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
                open_position_time: open.open_position_time,
                close_position_time: record.close_position_time.clone(),
                close_type: record.close_type.clone(),
                profit_loss: record.profit_loss,
                outcome: trade_outcome_label(record.profit_loss),
                trigger: open.trigger,
                new_rank: event.event.new_rank,
                delta_rank: event.event.delta_rank,
                price_change_pct: event.event.price_change_pct,
            });
        }
    }

    reports.sort_by_key(|report| (report.entry_ts, report.event_id));
    reports
}

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

fn parse_replay_open_trade(record: &TradeRecord) -> Option<ReplayOpenTrade> {
    if record.option_type != "long" {
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
        open_position_time: record.open_position_time.clone(),
    })
}

fn trade_outcome_label(profit_loss: f64) -> &'static str {
    if profit_loss > 0.0 {
        "win"
    } else if profit_loss < 0.0 {
        "loss"
    } else {
        "flat"
    }
}

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

pub fn print_framework_equity_trade_reports(reports: &[FrameworkEquityTradeReport]) {
    for trade in reports {
        println!(
            "framework_equity_trade\ttarget={}R\tmode=symbol_isolated_100u\tsymbol={}\tevent_id={}\tdetected_at={}\tentry_ts={}\topen_time={}\tclose_time={}\tclose_type={}\tprofit_loss={:.8}\toutcome={}\ttrigger={}\tnew_rank={}\tdelta_rank={}\tprice_change_pct={}",
            trade.target_r,
            trade.symbol,
            trade.event_id,
            trade.detected_at,
            trade.entry_ts,
            trade.open_position_time,
            trade.close_position_time.as_deref().unwrap_or("NA"),
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
    fn new(
        events: Vec<ConfirmedEvent>,
        stop_loss_pct: f64,
        target_r: f64,
        profit_protect_after_r: Option<f64>,
        profit_protect_stop_r: f64,
    ) -> Self {
        let entries_by_ts = events
            .into_iter()
            .map(|event| {
                (
                    event.entry_ts,
                    ReplayEntry {
                        entry_price: event.entry_price,
                        event_id: event.event.id,
                        trigger: event.trigger,
                    },
                )
            })
            .collect();
        Self {
            entries_by_ts,
            stop_loss_pct,
            target_r,
            profit_protect_after_r,
            profit_protect_stop_r,
            active_position: None,
        }
    }

    fn build_entry_signal(&mut self, candle_ts: i64, entry: &ReplayEntry) -> SignalResult {
        self.active_position = Some(ReplayActivePosition {
            entry_price: entry.entry_price,
            event_id: entry.event_id,
            trigger: entry.trigger.clone(),
            profit_protected: false,
        });
        self.build_long_signal(
            candle_ts,
            entry.entry_price,
            entry.entry_price * (1.0 - self.stop_loss_pct),
            "MarketVelocityFixedRisk",
            entry.event_id,
            &entry.trigger,
            false,
        )
    }

    fn maybe_build_profit_protection_signal(
        &mut self,
        candle: &CandleItem,
    ) -> Option<SignalResult> {
        let after_r = self.profit_protect_after_r?;
        let active = self.active_position.as_mut()?;
        let target_price = active.entry_price * (1.0 + self.stop_loss_pct * self.target_r);
        let current_stop_price = active.entry_price
            * (1.0
                + self.stop_loss_pct
                    * if active.profit_protected {
                        self.profit_protect_stop_r
                    } else {
                        -1.0
                    });

        if candle.l <= current_stop_price || candle.h >= target_price {
            self.active_position = None;
            return None;
        }
        if active.profit_protected {
            return None;
        }

        let trigger_price = active.entry_price * (1.0 + self.stop_loss_pct * after_r);
        if candle.h < trigger_price {
            return None;
        }

        let entry_price = active.entry_price;
        let event_id = active.event_id;
        let trigger = active.trigger.clone();
        let protected_stop_price =
            entry_price * (1.0 + self.stop_loss_pct * self.profit_protect_stop_r);
        if candle.c <= protected_stop_price {
            return None;
        }
        active.profit_protected = true;
        Some(self.build_long_signal(
            candle.ts,
            entry_price,
            protected_stop_price,
            "MarketVelocityProfitProtect",
            event_id,
            &trigger,
            true,
        ))
    }

    fn build_long_signal(
        &self,
        candle_ts: i64,
        entry_price: f64,
        stop_loss_price: f64,
        stop_loss_source: &str,
        event_id: i64,
        trigger: &str,
        profit_protected: bool,
    ) -> SignalResult {
        SignalResult {
            should_buy: true,
            open_price: entry_price,
            signal_kline_stop_loss_price: Some(stop_loss_price),
            stop_loss_source: Some(stop_loss_source.to_string()),
            long_signal_take_profit_price: Some(
                entry_price * (1.0 + self.stop_loss_pct * self.target_r),
            ),
            ts: candle_ts,
            single_value: Some(
                json!({
                    "source": "market_velocity_framework_replay",
                    "rank_event_id": event_id,
                    "entry_trigger": trigger,
                    "target_r": self.target_r,
                    "stop_loss_pct": self.stop_loss_pct,
                    "profit_protected": profit_protected,
                })
                .to_string(),
            ),
            single_result: Some("market_velocity_framework_replay".to_string()),
            direction: SignalDirection::Long,
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

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _: &mut Self::IndicatorValues,
        _: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let Some(candle) = candles.last() else {
            return SignalResult::default();
        };
        if let Some(entry) = self.entries_by_ts.get(&candle.ts).cloned() {
            return self.build_entry_signal(candle.ts, &entry);
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

fn analyze_closed_trades(records: &[TradeRecord]) -> ClosedTradeStats {
    let mut wins = 0;
    let mut losses = 0;
    let mut equity = INITIAL_FUND_PER_SYMBOL;
    let mut peak = INITIAL_FUND_PER_SYMBOL;
    let mut max_drawdown_pct = 0.0;
    let mut returns = Vec::new();

    for record in records.iter().filter(|record| record.full_close) {
        if record.profit_loss > 0.0 {
            wins += 1;
        } else if record.profit_loss < 0.0 {
            losses += 1;
        }
        if equity > 0.0 {
            returns.push(record.profit_loss / equity);
        }
        equity += record.profit_loss;
        peak = peak.max(equity);
        if peak > 0.0 {
            let drawdown_pct = (peak - equity) / peak * 100.0;
            if drawdown_pct > max_drawdown_pct {
                max_drawdown_pct = drawdown_pct;
            }
        }
    }

    ClosedTradeStats {
        wins,
        losses,
        returns,
        max_drawdown_pct,
    }
}

fn trade_sharpe(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let stddev = sample_stddev(returns, mean);
    (stddev > 0.0).then_some(mean / stddev * (returns.len() as f64).sqrt())
}

fn sample_stddev(values: &[f64], mean: f64) -> f64 {
    let variance = values
        .iter()
        .map(|value| {
            let diff = value - mean;
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}

fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| {
            if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                format!("{value}")
            }
        })
        .unwrap_or_else(|| "NA".to_string())
}
