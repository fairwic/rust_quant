use anyhow::{bail, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    Candle, Direction, ExitPolicy, ExitReason, ReplayReport, SignalFamily, Trade,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const STRUCTURE_LOOKBACK: usize = 20;
const FORWARD_HORIZONS: [usize; 5] = [1, 2, 4, 8, 16];
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;

/// 单币同源行情与零成本、成本后回放，供交易路径后验解剖使用。
pub(crate) struct AnatomyInput<'a> {
    pub(crate) symbol: &'a str,
    pub(crate) candles: &'a [Candle],
    pub(crate) zero_cost: &'a ReplayReport,
    pub(crate) cost_adjusted: &'a ReplayReport,
}

/// D0 EMA 空头实际成交样本的交易路径、市场状态与事件集中度报告。
#[derive(Debug, Serialize)]
pub(crate) struct EmaShortTradeAnatomyReport {
    definition: &'static str,
    future_data_usage: &'static str,
    sample_boundary: &'static str,
    valid_trade_records: usize,
    invalid_trade_records: usize,
    invalid_reasons: BTreeMap<String, usize>,
    overall: AnatomyCohort,
    target_2025_aug_sep: AnatomyCohort,
    outside_target_2025_aug_sep: AnatomyCohort,
    btc: AnatomyCohort,
    non_btc: AnatomyCohort,
    effective_events_60m: EventClusterSummary,
    target_2025_aug_sep_effective_events_60m: EventClusterSummary,
    outside_target_2025_aug_sep_effective_events_60m: EventClusterSummary,
    btc_effective_events_60m: EventClusterSummary,
    non_btc_effective_events_60m: EventClusterSummary,
    next_research_decision: NextResearchDecision,
    records: Vec<TradeAnatomyRecord>,
}

/// 一组交易的成本后表现、路径标签和关键连续变量分布。
#[derive(Debug, Default, Serialize)]
struct AnatomyCohort {
    trades: usize,
    wins: usize,
    losses: usize,
    nonpositive_trades: usize,
    net_r: f64,
    average_net_r: f64,
    profit_factor_r: Option<f64>,
    profit_factor_r_is_infinite: bool,
    forward_4_complete: usize,
    no_follow_through_4bar: usize,
    no_follow_through_4bar_rate_percent: f64,
    immediate_wrong_direction_4bar: usize,
    immediate_wrong_direction_4bar_rate_percent: f64,
    initial_stop_exits: usize,
    initial_stop_recovery_evaluable: usize,
    initial_stop_then_recovered_1r_within_16: usize,
    initial_stop_then_recovered_rate_percent: f64,
    nonpositive_with_path: usize,
    profit_giveback_after_1r: usize,
    profit_giveback_rate_of_nonpositive_percent: f64,
    healthy_capture_2r: usize,
    losing_reclaim_2bar_evaluable: usize,
    losing_reclaim_break_line_within_2: usize,
    losing_reclaim_2bar_rate_percent: f64,
    forward_4_mfe_r: DistributionSummary,
    forward_4_mae_r: DistributionSummary,
    forward_16_mfe_r: DistributionSummary,
    pre_exit_mfe_r: DistributionSummary,
    pre_exit_mae_r: DistributionSummary,
    short_efficiency_48: DistributionSummary,
    tr14_ratio_to_prior96_median: DistributionSummary,
}

/// 连续变量的稳健摘要；分位数采用最近秩，避免小样本插值制造精度。
#[derive(Debug, Default, Serialize)]
struct DistributionSummary {
    count: usize,
    mean: Option<f64>,
    p25: Option<f64>,
    median: Option<f64>,
    p75: Option<f64>,
}

/// 固定 N 根前向结果；这些字段只能作为结果标签，不能参与信号生成。
#[derive(Debug, Clone, Serialize)]
struct ForwardPath {
    bars: usize,
    mfe_r: f64,
    mae_r: f64,
    close_r: f64,
}

/// 单笔成交的可审计路径；同一记录同时保留零成本和成本后 R。
#[derive(Debug, Serialize)]
struct TradeAnatomyRecord {
    symbol: String,
    signal_time_ms: i64,
    entry_time_ms: i64,
    exit_time_ms: i64,
    exit_policy: ExitPolicy,
    exit_reason: ExitReason,
    zero_cost_net_r: f64,
    cost_adjusted_net_r: f64,
    break_line: f64,
    forward_paths: Vec<ForwardPath>,
    reclaim_break_line_within_1: Option<bool>,
    reclaim_break_line_within_2: Option<bool>,
    reclaim_break_line_within_4: Option<bool>,
    pre_exit_mfe_r: f64,
    pre_exit_mae_r: f64,
    initial_stop_recovered_1r_within_16: Option<bool>,
    short_efficiency_48: Option<f64>,
    tr14_ratio_to_prior96_median: Option<f64>,
    no_follow_through_4bar: bool,
    immediate_wrong_direction_4bar: bool,
    profit_giveback_after_1r: bool,
    healthy_capture_2r: bool,
    effective_event_id: usize,
}

/// 60 分钟链式事件聚类的组合集中度近似，不冒充正式相关系数模型。
#[derive(Debug, Default, Serialize)]
struct EventClusterSummary {
    raw_trades: usize,
    events: usize,
    single_symbol_events: usize,
    multi_symbol_events: usize,
    average_net_r_per_single_symbol_event: Option<f64>,
    average_net_r_per_multi_symbol_event: Option<f64>,
    multi_symbol_loss_share_percent: f64,
    largest_event_trade_count: usize,
    largest_event_symbol_count: usize,
    events_detail: Vec<EventCluster>,
}

/// 一个链式市场事件及其中实际执行的币种与成本后结果。
#[derive(Debug, Serialize)]
struct EventCluster {
    event_id: usize,
    start_signal_time_ms: i64,
    end_signal_time_ms: i64,
    trades: usize,
    symbols: usize,
    cost_adjusted_net_r: f64,
    negative_r_magnitude: f64,
}

/// 按预注册优先级选出的下一轮唯一研究方向及其判定证据。
#[derive(Debug, Serialize)]
struct NextResearchDecision {
    selected: &'static str,
    triggered_rule: u8,
    profit_giveback_rate_of_nonpositive_percent: f64,
    initial_stop_recovery_rate_percent: f64,
    losing_reclaim_2bar_rate_percent: f64,
    multi_symbol_loss_share_percent: f64,
    multi_symbol_event_average_net_r: Option<f64>,
    single_symbol_event_average_net_r: Option<f64>,
}

/// 构建研究报告；未来 K 线只生成 outcome label，不会反馈给 replay 入场路径。
pub(crate) fn build_ema_short_trade_anatomy(
    inputs: &[AnatomyInput<'_>],
) -> Result<EmaShortTradeAnatomyReport> {
    let mut records = Vec::new();
    let mut invalid_reasons = BTreeMap::new();

    for input in inputs {
        validate_input_identity(input)?;
        for (zero_trade, cost_trade) in input
            .zero_cost
            .trades
            .iter()
            .zip(&input.cost_adjusted.trades)
        {
            validate_trade_identity(input.symbol, zero_trade, cost_trade)?;
            if !zero_trade.families.contains(&SignalFamily::EmaTrendShort) {
                continue;
            }
            match build_trade_record(input, zero_trade, cost_trade) {
                Ok(record) => records.push(record),
                Err(reason) => *invalid_reasons.entry(reason).or_default() += 1,
            }
        }
    }

    let invalid_trade_records = invalid_reasons.values().sum();
    let effective_events_60m = assign_effective_events(&mut records);
    let all_indices = (0..records.len()).collect::<Vec<_>>();
    let target_indices = all_indices
        .iter()
        .copied()
        .filter(|&index| is_target_2025_aug_sep(records[index].signal_time_ms))
        .collect::<Vec<_>>();
    let outside_target_indices = all_indices
        .iter()
        .copied()
        .filter(|&index| !is_target_2025_aug_sep(records[index].signal_time_ms))
        .collect::<Vec<_>>();
    let btc_indices = all_indices
        .iter()
        .copied()
        .filter(|&index| is_btc_symbol(&records[index].symbol))
        .collect::<Vec<_>>();
    let non_btc_indices = all_indices
        .iter()
        .copied()
        .filter(|&index| !is_btc_symbol(&records[index].symbol))
        .collect::<Vec<_>>();

    let overall = anatomy_cohort(&records, &all_indices);
    let next_research_decision = select_next_research(&overall, &effective_events_60m);
    Ok(EmaShortTradeAnatomyReport {
        definition: "closed executed ema_trend_short trades from the active replay variant; D0 requires signal close below the frozen prior-20-low line",
        future_data_usage: "forward bars are ex-post outcome labels only and never participate in signal, entry, blocking, stop, or target decisions",
        sample_boundary: "executed trades only; blocked raw D0 candidates are outside this report",
        valid_trade_records: records.len(),
        invalid_trade_records,
        invalid_reasons,
        overall,
        target_2025_aug_sep: anatomy_cohort(&records, &target_indices),
        outside_target_2025_aug_sep: anatomy_cohort(&records, &outside_target_indices),
        btc: anatomy_cohort(&records, &btc_indices),
        non_btc: anatomy_cohort(&records, &non_btc_indices),
        target_2025_aug_sep_effective_events_60m: summarize_effective_events(
            &records,
            &target_indices,
        ),
        outside_target_2025_aug_sep_effective_events_60m: summarize_effective_events(
            &records,
            &outside_target_indices,
        ),
        btc_effective_events_60m: summarize_effective_events(&records, &btc_indices),
        non_btc_effective_events_60m: summarize_effective_events(&records, &non_btc_indices),
        effective_events_60m,
        next_research_decision,
        records,
    })
}

/// 验证行情和两种成本回放属于同一个稳定币种及相同交易序列。
fn validate_input_identity(input: &AnatomyInput<'_>) -> Result<()> {
    if input.symbol != input.zero_cost.symbol || input.symbol != input.cost_adjusted.symbol {
        bail!("交易解剖成员 identity 不一致：{}", input.symbol);
    }
    if input.zero_cost.trades.len() != input.cost_adjusted.trades.len() {
        bail!("{} 的零成本与成本后交易数量不一致", input.symbol);
    }
    Ok(())
}

/// 成本压力只能改变净收益，不能静默改变信号、成交或退出时点。
fn validate_trade_identity(symbol: &str, zero: &Trade, cost: &Trade) -> Result<()> {
    if zero.direction != cost.direction
        || zero.families != cost.families
        || zero.signal_time_ms != cost.signal_time_ms
        || zero.entry_time_ms != cost.entry_time_ms
        || zero.exit_time_ms != cost.exit_time_ms
        || zero.exit_reason != cost.exit_reason
    {
        bail!("{symbol} 的零成本与成本后交易路径发生漂移");
    }
    Ok(())
}

/// 将一笔成交映射回同源 K 线并计算预注册路径标签。
fn build_trade_record(
    input: &AnatomyInput<'_>,
    zero: &Trade,
    cost: &Trade,
) -> std::result::Result<TradeAnatomyRecord, String> {
    if zero.direction != Direction::Short {
        return Err("ema_trend_short 交易方向不是 short".to_owned());
    }
    if !zero.initial_risk.is_finite() || zero.initial_risk <= 0.0 {
        return Err("initial_risk 非正或非有限".to_owned());
    }
    let signal_index = candle_index(input.candles, zero.signal_time_ms)
        .ok_or_else(|| "找不到信号 K 线".to_owned())?;
    let entry_index = candle_index(input.candles, zero.entry_time_ms)
        .ok_or_else(|| "找不到入场 K 线".to_owned())?;
    let exit_index = candle_index(input.candles, zero.exit_time_ms)
        .ok_or_else(|| "找不到退出 K 线".to_owned())?;
    if entry_index <= signal_index || exit_index < entry_index {
        return Err("信号、入场、退出时间顺序非法".to_owned());
    }
    let start = signal_index
        .checked_sub(STRUCTURE_LOOKBACK)
        .ok_or_else(|| "信号前不足 20 根结构历史".to_owned())?;
    let break_line = input.candles[start..signal_index]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    if input.candles[signal_index].close >= break_line {
        return Err("信号收盘未严格跌破前 20 根低点".to_owned());
    }

    let forward_paths = FORWARD_HORIZONS
        .iter()
        .filter_map(|&bars| forward_path(input.candles, entry_index, bars, zero))
        .collect::<Vec<_>>();
    let path_4 = forward_paths.iter().find(|path| path.bars == 4);
    let (pre_exit_mfe_r, pre_exit_mae_r) =
        conservative_pre_exit_path(input.candles, entry_index, exit_index, zero);
    let no_follow_through_4bar = path_4.is_some_and(|path| path.mfe_r < 0.5);
    let immediate_wrong_direction_4bar =
        path_4.is_some_and(|path| path.mfe_r < 0.5 && path.mae_r >= 1.0);

    Ok(TradeAnatomyRecord {
        symbol: input.symbol.to_owned(),
        signal_time_ms: zero.signal_time_ms,
        entry_time_ms: zero.entry_time_ms,
        exit_time_ms: zero.exit_time_ms,
        exit_policy: zero.exit_policy,
        exit_reason: zero.exit_reason,
        zero_cost_net_r: zero.net_r,
        cost_adjusted_net_r: cost.net_r,
        break_line,
        reclaim_break_line_within_1: reclaimed_within(input.candles, entry_index, 1, break_line),
        reclaim_break_line_within_2: reclaimed_within(input.candles, entry_index, 2, break_line),
        reclaim_break_line_within_4: reclaimed_within(input.candles, entry_index, 4, break_line),
        pre_exit_mfe_r,
        pre_exit_mae_r,
        initial_stop_recovered_1r_within_16: initial_stop_recovery(
            input.candles,
            entry_index,
            exit_index,
            zero,
        ),
        short_efficiency_48: short_efficiency_48(input.candles, signal_index),
        tr14_ratio_to_prior96_median: tr14_ratio_to_prior96_median(input.candles, signal_index),
        no_follow_through_4bar,
        immediate_wrong_direction_4bar,
        profit_giveback_after_1r: pre_exit_mfe_r >= 1.0 && cost.net_r <= 0.0,
        healthy_capture_2r: pre_exit_mfe_r >= 2.0 && cost.net_r > 0.0,
        forward_paths,
        effective_event_id: 0,
    })
}

/// 使用二分定位严格匹配的已完成 K 线，拒绝邻近时间被误当成目标棒。
fn candle_index(candles: &[Candle], timestamp_ms: i64) -> Option<usize> {
    candles
        .binary_search_by_key(&timestamp_ms, |candle| candle.timestamp_ms)
        .ok()
}

/// 计算从实际入场开盘开始的固定 N 根前向 MFE、MAE 与收盘收益。
fn forward_path(
    candles: &[Candle],
    entry_index: usize,
    bars: usize,
    trade: &Trade,
) -> Option<ForwardPath> {
    let end_index = entry_index.checked_add(bars.checked_sub(1)?)?;
    let window = candles.get(entry_index..=end_index)?;
    let min_low = window
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let max_high = window
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    Some(ForwardPath {
        bars,
        mfe_r: ((trade.entry_price - min_low) / trade.initial_risk).max(0.0),
        mae_r: ((max_high - trade.entry_price) / trade.initial_risk).max(0.0),
        close_r: (trade.entry_price - window.last()?.close) / trade.initial_risk,
    })
}

/// 退出棒只使用实际退出价，防止未知棒内顺序把退出后的极值算成持仓可得路径。
fn conservative_pre_exit_path(
    candles: &[Candle],
    entry_index: usize,
    exit_index: usize,
    trade: &Trade,
) -> (f64, f64) {
    let mut min_price = trade.entry_price.min(trade.exit_price);
    let mut max_price = trade.entry_price.max(trade.exit_price);
    for candle in &candles[entry_index..exit_index] {
        min_price = min_price.min(candle.low);
        max_price = max_price.max(candle.high);
    }
    (
        ((trade.entry_price - min_price) / trade.initial_risk).max(0.0),
        ((max_price - trade.entry_price) / trade.initial_risk).max(0.0),
    )
}

/// 只有完整 N 根可见时才判断完成棒是否收回冻结破位线。
fn reclaimed_within(
    candles: &[Candle],
    entry_index: usize,
    bars: usize,
    break_line: f64,
) -> Option<bool> {
    let end_index = entry_index.checked_add(bars.checked_sub(1)?)?;
    Some(
        candles
            .get(entry_index..=end_index)?
            .iter()
            .any(|candle| candle.close >= break_line),
    )
}

/// 判断初始止损后、原 16 根观察窗结束前是否重新出现至少 1R 有利波动。
fn initial_stop_recovery(
    candles: &[Candle],
    entry_index: usize,
    exit_index: usize,
    trade: &Trade,
) -> Option<bool> {
    if trade.exit_reason != ExitReason::StopLoss {
        return None;
    }
    let horizon_end = entry_index.checked_add(15)?;
    candles.get(horizon_end)?;
    let recovery_start = exit_index.checked_add(1)?;
    if recovery_start > horizon_end {
        return Some(false);
    }
    let min_low = candles[recovery_start..=horizon_end]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    Some((trade.entry_price - min_low) / trade.initial_risk >= 1.0)
}

/// 计算信号时过去 48 根的做空方向效率；正值代表净下跌。
fn short_efficiency_48(candles: &[Candle], signal_index: usize) -> Option<f64> {
    let start = signal_index.checked_sub(48)?;
    let window = candles.get(start..=signal_index)?;
    let path = window
        .windows(2)
        .map(|pair| (pair[1].close - pair[0].close).abs())
        .sum::<f64>();
    (path > 0.0).then_some((window.first()?.close - window.last()?.close) / path)
}

/// 用当前 TR14 简单均值相对过去 96 个同口径观测中位数描述波动阶段。
fn tr14_ratio_to_prior96_median(candles: &[Candle], signal_index: usize) -> Option<f64> {
    let first_observation = signal_index.checked_sub(95)?;
    let mut observations = (first_observation..=signal_index)
        .map(|index| tr14_sma(candles, index))
        .collect::<Option<Vec<_>>>()?;
    let current = *observations.last()?;
    observations.sort_by(f64::total_cmp);
    let median = (observations[47] + observations[48]) / 2.0;
    (median > 0.0).then_some(current / median)
}

/// 计算以指定完成棒结束的 14 根 True Range 简单均值。
fn tr14_sma(candles: &[Candle], end_index: usize) -> Option<f64> {
    let start = end_index.checked_sub(13)?;
    let previous_start = start.checked_sub(1)?;
    let window = candles.get(previous_start..=end_index)?;
    let sum = window
        .windows(2)
        .map(|pair| {
            let previous_close = pair[0].close;
            let current = pair[1];
            (current.high - current.low)
                .max((current.high - previous_close).abs())
                .max((current.low - previous_close).abs())
        })
        .sum::<f64>();
    Some(sum / 14.0)
}

/// 按 60 分钟链式窗口归并交易，并把事件编号回写到逐笔审计记录。
fn assign_effective_events(records: &mut [TradeAnatomyRecord]) -> EventClusterSummary {
    let indices = (0..records.len()).collect::<Vec<_>>();
    let grouped = cluster_indices(records, &indices);
    for (event_id, cluster) in grouped.iter().enumerate() {
        for &index in cluster {
            records[index].effective_event_id = event_id + 1;
        }
    }
    summarize_event_groups(records, &grouped)
}

/// 为固定分组独立计算事件集中度，不改写全样本已经冻结的事件编号。
fn summarize_effective_events(
    records: &[TradeAnatomyRecord],
    indices: &[usize],
) -> EventClusterSummary {
    let grouped = cluster_indices(records, indices);
    summarize_event_groups(records, &grouped)
}

/// 对指定交易索引执行相邻不超过 60 分钟的链式归并。
fn cluster_indices(records: &[TradeAnatomyRecord], indices: &[usize]) -> Vec<Vec<usize>> {
    let mut order = indices.to_vec();
    order.sort_by(|&left, &right| {
        records[left]
            .signal_time_ms
            .cmp(&records[right].signal_time_ms)
            .then_with(|| records[left].symbol.cmp(&records[right].symbol))
    });
    let mut grouped = Vec::<Vec<usize>>::new();
    for index in order {
        let starts_new = grouped.last().is_none_or(|cluster| {
            let previous = *cluster.last().expect("non-empty event cluster");
            records[index].signal_time_ms - records[previous].signal_time_ms > EVENT_CLUSTER_MS
        });
        if starts_new {
            grouped.push(Vec::new());
        }
        grouped
            .last_mut()
            .expect("event cluster exists")
            .push(index);
    }
    grouped
}

/// 汇总已冻结的事件分组，保留事件级 R 而不是把每笔交易当独立样本。
fn summarize_event_groups(
    records: &[TradeAnatomyRecord],
    grouped: &[Vec<usize>],
) -> EventClusterSummary {
    let mut summary = EventClusterSummary {
        raw_trades: grouped.iter().map(Vec::len).sum(),
        events: grouped.len(),
        ..EventClusterSummary::default()
    };
    let mut single_event_r = Vec::new();
    let mut multi_event_r = Vec::new();
    let total_loss = grouped
        .iter()
        .flatten()
        .map(|&index| (-records[index].cost_adjusted_net_r).max(0.0))
        .sum::<f64>();
    let mut multi_loss = 0.0;

    for (event_id, indices) in grouped.iter().enumerate() {
        let symbol_count = indices
            .iter()
            .map(|&index| records[index].symbol.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        let net_r = indices
            .iter()
            .map(|&index| records[index].cost_adjusted_net_r)
            .sum::<f64>();
        let negative_r = indices
            .iter()
            .map(|&index| (-records[index].cost_adjusted_net_r).max(0.0))
            .sum::<f64>();
        let is_multi = symbol_count >= 2;
        if is_multi {
            summary.multi_symbol_events += 1;
            multi_event_r.push(net_r);
            multi_loss += negative_r;
        } else {
            summary.single_symbol_events += 1;
            single_event_r.push(net_r);
        }
        summary.largest_event_trade_count = summary.largest_event_trade_count.max(indices.len());
        summary.largest_event_symbol_count = summary.largest_event_symbol_count.max(symbol_count);
        summary.events_detail.push(EventCluster {
            event_id: event_id + 1,
            start_signal_time_ms: records[*indices.first().expect("non-empty event")]
                .signal_time_ms,
            end_signal_time_ms: records[*indices.last().expect("non-empty event")].signal_time_ms,
            trades: indices.len(),
            symbols: symbol_count,
            cost_adjusted_net_r: net_r,
            negative_r_magnitude: negative_r,
        });
    }
    summary.average_net_r_per_single_symbol_event = mean(&single_event_r);
    summary.average_net_r_per_multi_symbol_event = mean(&multi_event_r);
    summary.multi_symbol_loss_share_percent = percent(multi_loss, total_loss);
    summary
}

/// 按索引构建一个固定分组，所有比例都保留各自可评估分母。
fn anatomy_cohort(records: &[TradeAnatomyRecord], indices: &[usize]) -> AnatomyCohort {
    let selected = indices
        .iter()
        .map(|&index| &records[index])
        .collect::<Vec<_>>();
    let wins = selected
        .iter()
        .filter(|record| record.cost_adjusted_net_r > 0.0)
        .count();
    let losses = selected
        .iter()
        .filter(|record| record.cost_adjusted_net_r < 0.0)
        .count();
    let nonpositive = selected
        .iter()
        .filter(|record| record.cost_adjusted_net_r <= 0.0)
        .count();
    let net_r = selected
        .iter()
        .map(|record| record.cost_adjusted_net_r)
        .sum::<f64>();
    let gross_profit = selected
        .iter()
        .map(|record| record.cost_adjusted_net_r.max(0.0))
        .sum::<f64>();
    let gross_loss = selected
        .iter()
        .map(|record| (-record.cost_adjusted_net_r).max(0.0))
        .sum::<f64>();
    let forward_4 = selected
        .iter()
        .filter_map(|record| record.forward_paths.iter().find(|path| path.bars == 4))
        .collect::<Vec<_>>();
    let forward_16 = selected
        .iter()
        .filter_map(|record| record.forward_paths.iter().find(|path| path.bars == 16))
        .collect::<Vec<_>>();
    let initial_stops = selected
        .iter()
        .filter(|record| record.exit_reason == ExitReason::StopLoss)
        .collect::<Vec<_>>();
    let recovery = initial_stops
        .iter()
        .filter_map(|record| record.initial_stop_recovered_1r_within_16)
        .collect::<Vec<_>>();
    let losing_reclaims = selected
        .iter()
        .filter(|record| record.cost_adjusted_net_r < 0.0)
        .filter_map(|record| record.reclaim_break_line_within_2)
        .collect::<Vec<_>>();
    let givebacks = selected
        .iter()
        .filter(|record| record.profit_giveback_after_1r)
        .count();
    let no_follow = forward_4.iter().filter(|path| path.mfe_r < 0.5).count();
    let wrong_direction = forward_4
        .iter()
        .filter(|path| path.mfe_r < 0.5 && path.mae_r >= 1.0)
        .count();

    AnatomyCohort {
        trades: selected.len(),
        wins,
        losses,
        nonpositive_trades: nonpositive,
        net_r,
        average_net_r: mean(
            &selected
                .iter()
                .map(|record| record.cost_adjusted_net_r)
                .collect::<Vec<_>>(),
        )
        .unwrap_or(0.0),
        profit_factor_r: (gross_loss > 0.0).then_some(gross_profit / gross_loss),
        profit_factor_r_is_infinite: gross_loss == 0.0 && gross_profit > 0.0,
        forward_4_complete: forward_4.len(),
        no_follow_through_4bar: no_follow,
        no_follow_through_4bar_rate_percent: percent(no_follow as f64, forward_4.len() as f64),
        immediate_wrong_direction_4bar: wrong_direction,
        immediate_wrong_direction_4bar_rate_percent: percent(
            wrong_direction as f64,
            forward_4.len() as f64,
        ),
        initial_stop_exits: initial_stops.len(),
        initial_stop_recovery_evaluable: recovery.len(),
        initial_stop_then_recovered_1r_within_16: recovery
            .iter()
            .filter(|&&recovered| recovered)
            .count(),
        initial_stop_then_recovered_rate_percent: percent(
            recovery.iter().filter(|&&recovered| recovered).count() as f64,
            recovery.len() as f64,
        ),
        nonpositive_with_path: nonpositive,
        profit_giveback_after_1r: givebacks,
        profit_giveback_rate_of_nonpositive_percent: percent(givebacks as f64, nonpositive as f64),
        healthy_capture_2r: selected
            .iter()
            .filter(|record| record.healthy_capture_2r)
            .count(),
        losing_reclaim_2bar_evaluable: losing_reclaims.len(),
        losing_reclaim_break_line_within_2: losing_reclaims
            .iter()
            .filter(|&&reclaimed| reclaimed)
            .count(),
        losing_reclaim_2bar_rate_percent: percent(
            losing_reclaims
                .iter()
                .filter(|&&reclaimed| reclaimed)
                .count() as f64,
            losing_reclaims.len() as f64,
        ),
        forward_4_mfe_r: distribution(forward_4.iter().map(|path| path.mfe_r)),
        forward_4_mae_r: distribution(forward_4.iter().map(|path| path.mae_r)),
        forward_16_mfe_r: distribution(forward_16.iter().map(|path| path.mfe_r)),
        pre_exit_mfe_r: distribution(selected.iter().map(|record| record.pre_exit_mfe_r)),
        pre_exit_mae_r: distribution(selected.iter().map(|record| record.pre_exit_mae_r)),
        short_efficiency_48: distribution(
            selected
                .iter()
                .filter_map(|record| record.short_efficiency_48),
        ),
        tr14_ratio_to_prior96_median: distribution(
            selected
                .iter()
                .filter_map(|record| record.tr14_ratio_to_prior96_median),
        ),
    }
}

/// 依照预注册优先级选择下一轮唯一变量，避免从多个后验故事中挑最好看的一个。
fn select_next_research(
    overall: &AnatomyCohort,
    events: &EventClusterSummary,
) -> NextResearchDecision {
    let multi_worse = events
        .average_net_r_per_multi_symbol_event
        .zip(events.average_net_r_per_single_symbol_event)
        .is_some_and(|(multi, single)| multi < single);
    let (selected, triggered_rule) = if overall.profit_giveback_rate_of_nonpositive_percent >= 30.0
    {
        ("exit_protection_after_1r", 1)
    } else if overall.initial_stop_then_recovered_rate_percent >= 40.0 {
        ("entry_timing_or_initial_stop", 2)
    } else if overall.losing_reclaim_2bar_rate_percent >= 50.0 {
        ("break_acceptance_confirmation", 3)
    } else if events.multi_symbol_loss_share_percent >= 60.0 && multi_worse {
        ("effective_event_capacity_and_correlation_risk", 4)
    } else {
        ("signal_time_market_state_short_efficiency_48", 5)
    };
    NextResearchDecision {
        selected,
        triggered_rule,
        profit_giveback_rate_of_nonpositive_percent: overall
            .profit_giveback_rate_of_nonpositive_percent,
        initial_stop_recovery_rate_percent: overall.initial_stop_then_recovered_rate_percent,
        losing_reclaim_2bar_rate_percent: overall.losing_reclaim_2bar_rate_percent,
        multi_symbol_loss_share_percent: events.multi_symbol_loss_share_percent,
        multi_symbol_event_average_net_r: events.average_net_r_per_multi_symbol_event,
        single_symbol_event_average_net_r: events.average_net_r_per_single_symbol_event,
    }
}

/// 判断信号是否属于预注册的 2025 年 8～9 月目标窗口。
fn is_target_2025_aug_sep(timestamp_ms: i64) -> bool {
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .is_some_and(|time| time.year() == 2025 && matches!(time.month(), 8 | 9))
}

/// 兼容冻结 manifest 的 `BTC-USDT-SWAP` 及同前缀 BTC 标识。
fn is_btc_symbol(symbol: &str) -> bool {
    symbol == "BTC" || symbol.starts_with("BTC-") || symbol.starts_with("BTC/")
}

/// 构建有限值分布；空集合保留 `None`，不伪造零均值。
fn distribution(values: impl Iterator<Item = f64>) -> DistributionSummary {
    let mut values = values.filter(|value| value.is_finite()).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    DistributionSummary {
        count: values.len(),
        mean: mean(&values),
        p25: nearest_rank(&values, 0.25),
        median: nearest_rank(&values, 0.50),
        p75: nearest_rank(&values, 0.75),
    }
}

/// 最近秩分位数；单样本和小样本不会生成不存在的插值点。
fn nearest_rank(values: &[f64], quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let rank = ((values.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(values.len() - 1);
    Some(values[rank])
}

/// 返回有限集合均值，空集合保持缺失。
fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// 安全计算百分比；分母为零时返回零并由配套 count 字段解释。
fn percent(numerator: f64, denominator: f64) -> f64 {
    if denominator > 0.0 {
        numerator / denominator * 100.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建一根连续、价格可控的测试 K 线。
    fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: index as i64 * 15 * 60 * 1_000,
            open,
            high,
            low,
            close,
            volume: 10.0,
        }
    }

    /// 创建只包含本模块所需字段的 EMA 空头测试交易。
    fn short_trade(entry_index: usize, exit_index: usize, reason: ExitReason) -> Trade {
        Trade {
            direction: Direction::Short,
            families: vec![SignalFamily::EmaTrendShort],
            exit_policy:
                rust_quant_cli::app::tradingview_velocity_parity::ExitPolicy::ShortTrendExtension,
            signal_counter_trend_ema_age_bars_capped_600: None,
            counter_trend_structure_breakout_line: None,
            counter_trend_structure_confirmed: false,
            counter_trend_two_r_trailing_activated: false,
            range_partial_one_r_taken: false,
            range_two_r_trailing_activated: false,
            signal_time_ms: (entry_index - 1) as i64 * 15 * 60 * 1_000,
            entry_time_ms: entry_index as i64 * 15 * 60 * 1_000,
            exit_time_ms: exit_index as i64 * 15 * 60 * 1_000,
            entry_price: 100.0,
            exit_price: 101.0,
            initial_stop: 101.0,
            exit_reason: reason,
            gross_pnl: -1.0,
            net_pnl: -1.0,
            initial_risk: 1.0,
            net_r: -1.0,
            anchor_upthrust_target_consumption_ratio: None,
            volume_ratio: Some(3.0),
            rsi: Some(40.0),
        }
    }

    #[test]
    fn forward_path_uses_entry_bar_as_first_outcome_bar() {
        let candles = vec![
            candle(0, 100.0, 101.0, 99.0, 100.0),
            candle(1, 100.0, 100.5, 98.5, 99.0),
            candle(2, 99.0, 99.5, 97.5, 98.0),
        ];
        let trade = short_trade(1, 2, ExitReason::TakeProfit);

        let path = forward_path(&candles, 1, 2, &trade).expect("complete two-bar path");

        assert_eq!(path.mfe_r, 2.5);
        assert_eq!(path.mae_r, 0.5);
        assert_eq!(path.close_r, 2.0);
    }

    #[test]
    fn conservative_path_does_not_use_exit_bar_extremes() {
        let candles = vec![
            candle(0, 100.0, 100.0, 100.0, 100.0),
            candle(1, 100.0, 100.5, 99.0, 99.5),
            candle(2, 99.5, 105.0, 90.0, 101.0),
        ];
        let trade = short_trade(1, 2, ExitReason::StopLoss);

        let (mfe, mae) = conservative_pre_exit_path(&candles, 1, 2, &trade);

        assert_eq!(mfe, 1.0);
        assert_eq!(mae, 1.0);
    }

    #[test]
    fn stop_recovery_starts_after_exit_bar() {
        let mut candles = (0..20)
            .map(|index| candle(index, 100.0, 100.5, 99.5, 100.0))
            .collect::<Vec<_>>();
        candles[3] = candle(3, 101.0, 105.0, 95.0, 100.0);
        candles[4] = candle(4, 100.0, 100.0, 98.5, 99.0);
        let trade = short_trade(1, 3, ExitReason::StopLoss);

        assert_eq!(initial_stop_recovery(&candles, 1, 3, &trade), Some(true));
    }

    #[test]
    fn event_cluster_uses_chained_sixty_minute_windows() {
        let mut records = [0_i64, 45, 90]
            .into_iter()
            .map(|minute| TradeAnatomyRecord {
                symbol: format!("S{minute}"),
                signal_time_ms: minute * 60 * 1_000,
                entry_time_ms: 0,
                exit_time_ms: 0,
                exit_policy: ExitPolicy::Fixed,
                exit_reason: ExitReason::StopLoss,
                zero_cost_net_r: -1.0,
                cost_adjusted_net_r: -1.0,
                break_line: 99.0,
                forward_paths: Vec::new(),
                reclaim_break_line_within_1: None,
                reclaim_break_line_within_2: None,
                reclaim_break_line_within_4: None,
                pre_exit_mfe_r: 0.0,
                pre_exit_mae_r: 1.0,
                initial_stop_recovered_1r_within_16: None,
                short_efficiency_48: None,
                tr14_ratio_to_prior96_median: None,
                no_follow_through_4bar: false,
                immediate_wrong_direction_4bar: false,
                profit_giveback_after_1r: false,
                healthy_capture_2r: false,
                effective_event_id: 0,
            })
            .collect::<Vec<_>>();

        let summary = assign_effective_events(&mut records);

        assert_eq!(summary.events, 1);
        assert_eq!(summary.largest_event_trade_count, 3);
        assert!(records.iter().all(|record| record.effective_event_id == 1));
    }
}
