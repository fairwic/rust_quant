use anyhow::{bail, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    Candle, Direction, ExitPolicy, ExitReason, ReplayReport, SignalFamily, Trade,
};
use serde::Serialize;
use std::collections::BTreeSet;

const NET_BREAK_EVEN_COST_BPS_PER_SIDE: f64 = 8.0;
const STRESS_COST_BPS_PER_SIDE: [f64; 3] = [8.0, 10.0, 12.0];
const STRUCTURE_LOOKBACK: usize = 20;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const FLOAT_TOLERANCE: f64 = 1e-8;

/// 单币同源行情与两种成本回放，供冻结 D0 交易做隔离退出反事实。
pub(crate) struct ExitCounterfactualInput<'a> {
    pub(crate) symbol: &'a str,
    pub(crate) tick_size: f64,
    pub(crate) candles: &'a [Candle],
    pub(crate) zero_cost: &'a ReplayReport,
    pub(crate) cost_adjusted: &'a ReplayReport,
}

/// 冻结 D0 交易在一种净保本激活方式下的 Research-only 对照报告。
#[derive(Debug, Serialize)]
pub(crate) struct EmaShortCompletedCloseOneRNetBeReport {
    definition: &'static str,
    implementation_mode: &'static str,
    future_data_usage: &'static str,
    protected_cost_bps_per_side: f64,
    structure_lookback_bars: Option<usize>,
    valid_trade_records: usize,
    identity_changed_trades: usize,
    overall: CohortComparison,
    target_2025_aug_sep: CohortComparison,
    outside_target_2025_aug_sep: CohortComparison,
    btc: CohortComparison,
    non_btc: CohortComparison,
    per_symbol: Vec<SymbolComparison>,
    metric_gate: MetricGate,
    records: Vec<CounterfactualRecord>,
}

/// 一组固定交易身份在基线与唯一退出变量下的逐成本对照。
#[derive(Debug, Default, Serialize)]
struct CohortComparison {
    trades: usize,
    eligible_fixed_ema_short_trades: usize,
    one_r_confirmed_completed_close: usize,
    structure_break_confirmed: usize,
    failed_retest_confirmed: usize,
    activated_on_completed_close: usize,
    activated_effective_events_60m: usize,
    net_break_even_exits: usize,
    unchanged_exits: usize,
    original_winners_cut: usize,
    original_winner_profit_reduced_r_8bps: f64,
    original_losses_protected: usize,
    original_loss_reduction_r_8bps: f64,
    costs: Vec<CostComparison>,
    effective_events_60m_at_8bps: EventComparison,
}

/// 一个成本压力下的 R 口径表现；基线和变体共享完全相同的交易身份。
#[derive(Debug, Serialize)]
struct CostComparison {
    cost_bps_per_side: f64,
    baseline: RMetrics,
    variant: RMetrics,
    variant_minus_baseline_net_r: f64,
    baseline_loss_reduction_percent: Option<f64>,
}

/// 固定交易集合的 R 口径指标。
#[derive(Debug, Default, Clone, Serialize)]
struct RMetrics {
    trades: usize,
    wins: usize,
    losses: usize,
    flat: usize,
    win_rate_percent: f64,
    net_r: f64,
    average_net_r: f64,
    gross_profit_r: f64,
    gross_loss_r: f64,
    profit_factor_r: Option<f64>,
    profit_factor_r_is_infinite: bool,
    chronological_closed_equity_max_drawdown_r: f64,
}

/// 60 分钟链式事件聚类只用于识别市场共振集中度，不冒充独立相关性模型。
#[derive(Debug, Default, Serialize)]
struct EventComparison {
    raw_trades: usize,
    events: usize,
    single_symbol_events: usize,
    multi_symbol_events: usize,
    baseline_net_r: f64,
    variant_net_r: f64,
    variant_minus_baseline_net_r: f64,
    improved_events: usize,
    worsened_events: usize,
    largest_event_trade_count: usize,
    largest_event_symbol_count: usize,
}

/// 单币对照用于识别结果是否被少数交易品种主导。
#[derive(Debug, Serialize)]
struct SymbolComparison {
    symbol: String,
    comparison: CohortComparison,
}

/// 预注册指标闸门；逐笔时序测试仍由测试命令独立给出证据。
#[derive(Debug, Serialize)]
struct MetricGate {
    overall_net_r_improved_at_8bps: bool,
    overall_average_r_improved_at_8bps: bool,
    overall_profit_factor_improved_at_8bps: bool,
    target_loss_reduction_percent_at_8bps: Option<f64>,
    target_loss_reduction_at_least_25_percent: bool,
    target_profit_factor_not_lower_at_8bps: bool,
    outside_net_r_decline_percent_at_8bps: f64,
    outside_net_r_decline_within_10_percent: bool,
    variant_net_r_positive_at_10bps: bool,
    activation_sample_gate_required: bool,
    failed_retest_activations_at_least_30: bool,
    activated_effective_events_at_least_20: bool,
    trade_identity_preserved: bool,
    metric_gate_passed: bool,
}

/// 两个互斥 Research 退出变量共享同一逐笔模拟器，避免实现口径漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtectionMode {
    CompletedCloseOneR,
    StructureBreakFailedRetest,
}

impl ProtectionMode {
    fn definition(self) -> &'static str {
        match self {
            Self::CompletedCloseOneR => {
                "D0 ema_trend_short trades; only short Fixed exits activate an 8bps-per-side tick-rounded net break-even stop after a completed close reaches +1R"
            }
            Self::StructureBreakFailedRetest => {
                "D0 ema_trend_short trades; after a completed close reaches +1R, freeze the first later close below the prior 20-bar low and activate net break-even only after a still-later candle retests that line but closes below it"
            }
        }
    }

    fn structure_lookback(self) -> Option<usize> {
        (self == Self::StructureBreakFailedRetest).then_some(STRUCTURE_LOOKBACK)
    }
}

/// 反事实退出类别；未触发保护时完整保留原退出。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CounterfactualExitKind {
    BaselineUnchanged,
    NetBreakEvenStop,
    NetBreakEvenGapOpen,
}

/// 单笔同身份交易的基线与反事实退出证据。
#[derive(Debug, Serialize)]
struct CounterfactualRecord {
    symbol: String,
    signal_time_ms: i64,
    entry_time_ms: i64,
    baseline_exit_time_ms: i64,
    variant_exit_time_ms: i64,
    baseline_exit_reason: ExitReason,
    variant_exit_kind: CounterfactualExitKind,
    eligible: bool,
    one_r_confirmation_time_ms: Option<i64>,
    structure_break_time_ms: Option<i64>,
    failed_retest_time_ms: Option<i64>,
    structure_line: Option<f64>,
    activation_time_ms: Option<i64>,
    entry_price: f64,
    initial_stop: f64,
    initial_risk: f64,
    net_break_even_stop: Option<f64>,
    baseline_exit_price: f64,
    variant_exit_price: f64,
}

/// 逐笔模拟输出；保护在激活棒收盘后冻结，因此最早只检查下一根。
#[derive(Debug, Clone, Copy, PartialEq)]
struct SimulatedExit {
    one_r_confirmation_time_ms: Option<i64>,
    structure_break_time_ms: Option<i64>,
    failed_retest_time_ms: Option<i64>,
    structure_line: Option<f64>,
    activation_time_ms: Option<i64>,
    exit_time_ms: i64,
    exit_price: f64,
    kind: CounterfactualExitKind,
}

/// 激活前的因果状态；每一阶段只能由已完成 K 线单向推进。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct ProtectionState {
    one_r_confirmation_time_ms: Option<i64>,
    structure_break_time_ms: Option<i64>,
    failed_retest_time_ms: Option<i64>,
    structure_line: Option<f64>,
    activation_time_ms: Option<i64>,
}

/// 构建隔离退出反事实；原 broker 和后续信号完全不重跑，避免仓位提前释放造成样本漂移。
pub(crate) fn build_ema_short_completed_close_one_r_net_be(
    inputs: &[ExitCounterfactualInput<'_>],
) -> Result<EmaShortCompletedCloseOneRNetBeReport> {
    build_exit_counterfactual(inputs, ProtectionMode::CompletedCloseOneR)
}

/// 构建 1R 后新结构破位、失败回抽完成后才启用净保本的单变量报告。
pub(crate) fn build_ema_short_structure_break_failed_retest_net_be(
    inputs: &[ExitCounterfactualInput<'_>],
) -> Result<EmaShortCompletedCloseOneRNetBeReport> {
    build_exit_counterfactual(inputs, ProtectionMode::StructureBreakFailedRetest)
}

fn build_exit_counterfactual(
    inputs: &[ExitCounterfactualInput<'_>],
    mode: ProtectionMode,
) -> Result<EmaShortCompletedCloseOneRNetBeReport> {
    let mut records = Vec::new();
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
            validate_cost_replay(zero_trade, cost_trade)?;
            let simulated = simulate_trade(zero_trade, input.candles, input.tick_size, mode)?;
            records.push(CounterfactualRecord {
                symbol: input.symbol.to_owned(),
                signal_time_ms: zero_trade.signal_time_ms,
                entry_time_ms: zero_trade.entry_time_ms,
                baseline_exit_time_ms: zero_trade.exit_time_ms,
                variant_exit_time_ms: simulated.exit_time_ms,
                baseline_exit_reason: zero_trade.exit_reason,
                variant_exit_kind: simulated.kind,
                eligible: is_eligible(zero_trade),
                one_r_confirmation_time_ms: simulated.one_r_confirmation_time_ms,
                structure_break_time_ms: simulated.structure_break_time_ms,
                failed_retest_time_ms: simulated.failed_retest_time_ms,
                structure_line: simulated.structure_line,
                activation_time_ms: simulated.activation_time_ms,
                entry_price: zero_trade.entry_price,
                initial_stop: zero_trade.initial_stop,
                initial_risk: zero_trade.initial_risk,
                net_break_even_stop: is_eligible(zero_trade)
                    .then(|| short_net_break_even_price(zero_trade.entry_price, input.tick_size)),
                baseline_exit_price: zero_trade.exit_price,
                variant_exit_price: simulated.exit_price,
            });
        }
    }

    let all_indices = (0..records.len()).collect::<Vec<_>>();
    let target_indices = selected_indices(&records, |record| {
        is_target_2025_aug_sep(record.signal_time_ms)
    });
    let outside_indices = selected_indices(&records, |record| {
        !is_target_2025_aug_sep(record.signal_time_ms)
    });
    let btc_indices = selected_indices(&records, |record| is_btc_symbol(&record.symbol));
    let non_btc_indices = selected_indices(&records, |record| !is_btc_symbol(&record.symbol));
    let overall = cohort_comparison(&records, &all_indices);
    let target_2025_aug_sep = cohort_comparison(&records, &target_indices);
    let outside_target_2025_aug_sep = cohort_comparison(&records, &outside_indices);
    let btc = cohort_comparison(&records, &btc_indices);
    let non_btc = cohort_comparison(&records, &non_btc_indices);
    let metric_gate = metric_gate(
        &overall,
        &target_2025_aug_sep,
        &outside_target_2025_aug_sep,
        mode,
    );

    let symbols = records
        .iter()
        .map(|record| record.symbol.clone())
        .collect::<BTreeSet<_>>();
    let per_symbol = symbols
        .iter()
        .map(|symbol| {
            let indices = selected_indices(&records, |record| record.symbol == *symbol);
            SymbolComparison {
                symbol: symbol.clone(),
                comparison: cohort_comparison(&records, &indices),
            }
        })
        .collect();

    Ok(EmaShortCompletedCloseOneRNetBeReport {
        definition: mode.definition(),
        implementation_mode: "trade-isolated exit counterfactual over the frozen baseline trade list; later entries are deliberately not regenerated",
        future_data_usage: "candles after entry are used only to simulate the alternative exit; no future candle can alter signal, entry, eligibility, or another trade",
        protected_cost_bps_per_side: NET_BREAK_EVEN_COST_BPS_PER_SIDE,
        structure_lookback_bars: mode.structure_lookback(),
        valid_trade_records: records.len(),
        identity_changed_trades: 0,
        overall,
        target_2025_aug_sep,
        outside_target_2025_aug_sep,
        btc,
        non_btc,
        per_symbol,
        metric_gate,
        records,
    })
}

/// 行情、报告成员和交易数量必须一致，否则不能声称逐笔退出对照。
fn validate_input_identity(input: &ExitCounterfactualInput<'_>) -> Result<()> {
    if input.symbol != input.zero_cost.symbol || input.symbol != input.cost_adjusted.symbol {
        bail!("净保本反事实成员 identity 不一致：{}", input.symbol);
    }
    if input.zero_cost.trades.len() != input.cost_adjusted.trades.len() {
        bail!("{} 的零成本与成本后交易数量不一致", input.symbol);
    }
    if input.tick_size <= 0.0 || !input.tick_size.is_finite() {
        bail!("{} 的 tick size 无效", input.symbol);
    }
    Ok(())
}

/// 成本回放只能改变收益，不能改变任何信号或成交字段。
fn validate_trade_identity(symbol: &str, zero: &Trade, cost: &Trade) -> Result<()> {
    let same = zero.direction == cost.direction
        && zero.families == cost.families
        && zero.exit_policy == cost.exit_policy
        && zero.signal_time_ms == cost.signal_time_ms
        && zero.entry_time_ms == cost.entry_time_ms
        && zero.exit_time_ms == cost.exit_time_ms
        && nearly_equal(zero.entry_price, cost.entry_price)
        && nearly_equal(zero.exit_price, cost.exit_price)
        && nearly_equal(zero.initial_stop, cost.initial_stop)
        && zero.exit_reason == cost.exit_reason;
    if !same {
        bail!(
            "{} 在信号 {} 的零成本与成本后交易 identity 漂移",
            symbol,
            zero.signal_time_ms
        );
    }
    Ok(())
}

/// 用成交价重算冻结 8bps 成本，验证报告中的成本 R 没有隐藏数量或分批差异。
fn validate_cost_replay(zero: &Trade, cost: &Trade) -> Result<()> {
    let expected = net_r(
        zero.direction,
        zero.entry_price,
        zero.exit_price,
        zero.initial_risk,
        NET_BREAK_EVEN_COST_BPS_PER_SIDE,
    );
    if !nearly_equal(expected, cost.net_r) {
        bail!(
            "信号 {} 的 8bps 成本 R 无法从成交还原：{} != {}",
            zero.signal_time_ms,
            expected,
            cost.net_r
        );
    }
    Ok(())
}

/// 只有固定退出的 EMA 空头成交进入本变量，其余 D0 交易原样保留。
fn is_eligible(trade: &Trade) -> bool {
    trade.direction == Direction::Short
        && trade.exit_policy == ExitPolicy::Fixed
        && trade.families.contains(&SignalFamily::EmaTrendShort)
        && trade.initial_risk > 0.0
}

/// 沿原交易存活区间模拟唯一新保护；原退出作为不可晚于的终止事件。
fn simulate_trade(
    trade: &Trade,
    candles: &[Candle],
    tick_size: f64,
    mode: ProtectionMode,
) -> Result<SimulatedExit> {
    let baseline = baseline_exit(trade);
    if !is_eligible(trade) {
        return Ok(baseline);
    }
    let entry_index = candles
        .binary_search_by_key(&trade.entry_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow::anyhow!("找不到入场 K 线：{}", trade.entry_time_ms))?;
    let exit_index = candles
        .binary_search_by_key(&trade.exit_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow::anyhow!("找不到退出 K 线：{}", trade.exit_time_ms))?;
    if exit_index < entry_index {
        bail!("退出早于入场：{}", trade.signal_time_ms);
    }

    let activation_close = trade.entry_price - trade.initial_risk;
    let stop = short_net_break_even_price(trade.entry_price, tick_size);
    let mut state = ProtectionState::default();
    for index in entry_index..=exit_index {
        let candle = candles[index];
        let is_baseline_exit_candle = candle.timestamp_ms == trade.exit_time_ms;
        let active_from_prior_close = state
            .activation_time_ms
            .is_some_and(|activated| candle.timestamp_ms > activated);

        if active_from_prior_close {
            if is_baseline_exit_candle && trade.exit_reason == ExitReason::ReverseAtNextOpen {
                return Ok(with_state(baseline, state));
            }
            if candle.open >= stop {
                return Ok(with_state(
                    SimulatedExit {
                        exit_time_ms: candle.timestamp_ms,
                        exit_price: candle.open,
                        kind: CounterfactualExitKind::NetBreakEvenGapOpen,
                        ..baseline
                    },
                    state,
                ));
            }
            if is_baseline_exit_candle && nearly_equal(trade.exit_price, candle.open) {
                return Ok(with_state(baseline, state));
            }
            if let Some(kind) = first_exit_on_path(
                candle,
                stop,
                is_baseline_exit_candle.then_some(trade.exit_price),
            ) {
                return Ok(match kind {
                    CounterfactualExitKind::BaselineUnchanged => with_state(baseline, state),
                    CounterfactualExitKind::NetBreakEvenStop => with_state(
                        SimulatedExit {
                            exit_time_ms: candle.timestamp_ms,
                            exit_price: stop,
                            kind,
                            ..baseline
                        },
                        state,
                    ),
                    CounterfactualExitKind::NetBreakEvenGapOpen => {
                        unreachable!("盘中路径不会生成跳空退出")
                    }
                });
            }
        }

        if is_baseline_exit_candle {
            return Ok(with_state(baseline, state));
        }
        update_protection_state(mode, &mut state, candles, index, activation_close);
    }
    Ok(with_state(baseline, state))
}

/// 完成棒只能按 1R、首次新低破位、后续失败回抽的顺序推进，不能同棒补确认。
fn update_protection_state(
    mode: ProtectionMode,
    state: &mut ProtectionState,
    candles: &[Candle],
    index: usize,
    activation_close: f64,
) {
    let candle = candles[index];
    if state.one_r_confirmation_time_ms.is_none() {
        if candle.close <= activation_close {
            state.one_r_confirmation_time_ms = Some(candle.timestamp_ms);
            if mode == ProtectionMode::CompletedCloseOneR {
                state.activation_time_ms = Some(candle.timestamp_ms);
            }
        }
        return;
    }
    if mode == ProtectionMode::CompletedCloseOneR {
        return;
    }

    if state.structure_break_time_ms.is_none() {
        if candle.timestamp_ms
            <= state
                .one_r_confirmation_time_ms
                .expect("one R was confirmed")
        {
            return;
        }
        let Some(line) = prior_structure_low(candles, index) else {
            return;
        };
        if candle.close < line {
            state.structure_break_time_ms = Some(candle.timestamp_ms);
            state.structure_line = Some(line);
        }
        return;
    }

    if candle.timestamp_ms
        <= state
            .structure_break_time_ms
            .expect("structure break was confirmed")
    {
        return;
    }
    let line = state
        .structure_line
        .expect("structure break freezes a line");
    if candle.high >= line && candle.close < line {
        state.failed_retest_time_ms = Some(candle.timestamp_ms);
        state.activation_time_ms = Some(candle.timestamp_ms);
    }
}

/// 新结构只读取当前棒之前 20 根完整 K 线，并严格排除当前棒。
fn prior_structure_low(candles: &[Candle], index: usize) -> Option<f64> {
    let start = index.checked_sub(STRUCTURE_LOOKBACK)?;
    Some(
        candles[start..index]
            .iter()
            .map(|candle| candle.low)
            .fold(f64::INFINITY, f64::min),
    )
}

fn with_state(mut exit: SimulatedExit, state: ProtectionState) -> SimulatedExit {
    exit.one_r_confirmation_time_ms = state.one_r_confirmation_time_ms;
    exit.structure_break_time_ms = state.structure_break_time_ms;
    exit.failed_retest_time_ms = state.failed_retest_time_ms;
    exit.structure_line = state.structure_line;
    exit.activation_time_ms = state.activation_time_ms;
    exit
}

/// 在 TradingView 默认 OHLC 路径上比较净保本与原终止价的先后。
fn first_exit_on_path(
    candle: Candle,
    net_break_even_stop: f64,
    baseline_exit_level: Option<f64>,
) -> Option<CounterfactualExitKind> {
    for segment in broker_path(candle).windows(2) {
        let stop_hit = between(net_break_even_stop, segment[0], segment[1]);
        let baseline_hit =
            baseline_exit_level.is_some_and(|level| between(level, segment[0], segment[1]));
        match (stop_hit, baseline_hit) {
            (false, false) => {}
            (true, false) => return Some(CounterfactualExitKind::NetBreakEvenStop),
            (false, true) => return Some(CounterfactualExitKind::BaselineUnchanged),
            (true, true) => {
                let baseline = baseline_exit_level.expect("baseline_hit requires a level");
                let stop_first = if segment[1] >= segment[0] {
                    net_break_even_stop <= baseline
                } else {
                    net_break_even_stop >= baseline
                };
                return Some(if stop_first {
                    CounterfactualExitKind::NetBreakEvenStop
                } else {
                    CounterfactualExitKind::BaselineUnchanged
                });
            }
        }
    }
    None
}

/// 原交易是反事实的最晚终止点；未触发新保护时逐字段保持不变。
fn baseline_exit(trade: &Trade) -> SimulatedExit {
    SimulatedExit {
        one_r_confirmation_time_ms: None,
        structure_break_time_ms: None,
        failed_retest_time_ms: None,
        structure_line: None,
        activation_time_ms: None,
        exit_time_ms: trade.exit_time_ms,
        exit_price: trade.exit_price,
        kind: CounterfactualExitKind::BaselineUnchanged,
    }
}

/// 固定每边 8bps 并向下取 tick，确保做空保护不会高估净保本收益。
fn short_net_break_even_price(entry_price: f64, tick_size: f64) -> f64 {
    let cost_ratio = NET_BREAK_EVEN_COST_BPS_PER_SIDE / 10_000.0;
    round_down(
        entry_price * (1.0 - cost_ratio) / (1.0 + cost_ratio),
        tick_size,
    )
}

/// TradingView broker emulator 先走离开盘价更近的一侧。
fn broker_path(candle: Candle) -> [f64; 4] {
    if (candle.open - candle.high).abs() < (candle.open - candle.low).abs() {
        [candle.open, candle.high, candle.low, candle.close]
    } else {
        [candle.open, candle.low, candle.high, candle.close]
    }
}

fn between(level: f64, start: f64, end: f64) -> bool {
    if end >= start {
        level > start && level <= end
    } else {
        level < start && level >= end
    }
}

fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

/// 按筛选条件返回稳定的记录索引，避免复制整笔审计数据。
fn selected_indices(
    records: &[CounterfactualRecord],
    predicate: impl Fn(&CounterfactualRecord) -> bool,
) -> Vec<usize> {
    records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| predicate(record).then_some(index))
        .collect()
}

/// 汇总一组完全相同交易身份在三档成本下的基线与反事实结果。
fn cohort_comparison(records: &[CounterfactualRecord], indices: &[usize]) -> CohortComparison {
    let mut comparison = CohortComparison {
        trades: indices.len(),
        eligible_fixed_ema_short_trades: indices
            .iter()
            .filter(|&&index| records[index].eligible)
            .count(),
        one_r_confirmed_completed_close: indices
            .iter()
            .filter(|&&index| records[index].one_r_confirmation_time_ms.is_some())
            .count(),
        structure_break_confirmed: indices
            .iter()
            .filter(|&&index| records[index].structure_break_time_ms.is_some())
            .count(),
        failed_retest_confirmed: indices
            .iter()
            .filter(|&&index| records[index].failed_retest_time_ms.is_some())
            .count(),
        activated_on_completed_close: indices
            .iter()
            .filter(|&&index| records[index].activation_time_ms.is_some())
            .count(),
        activated_effective_events_60m: clustered_event_count(records, indices, |record| {
            record.activation_time_ms.is_some()
        }),
        net_break_even_exits: indices
            .iter()
            .filter(|&&index| {
                records[index].variant_exit_kind != CounterfactualExitKind::BaselineUnchanged
            })
            .count(),
        ..CohortComparison::default()
    };
    comparison.unchanged_exits = comparison.trades - comparison.net_break_even_exits;

    for &index in indices {
        let record = &records[index];
        let baseline = record_net_r(record, false, NET_BREAK_EVEN_COST_BPS_PER_SIDE);
        let variant = record_net_r(record, true, NET_BREAK_EVEN_COST_BPS_PER_SIDE);
        if baseline > 0.0 && variant + FLOAT_TOLERANCE < baseline {
            comparison.original_winners_cut += 1;
            comparison.original_winner_profit_reduced_r_8bps += baseline - variant;
        }
        if baseline < 0.0 && variant > baseline + FLOAT_TOLERANCE {
            comparison.original_losses_protected += 1;
            comparison.original_loss_reduction_r_8bps += variant - baseline;
        }
    }
    comparison.costs = STRESS_COST_BPS_PER_SIDE
        .iter()
        .map(|&cost| cost_comparison(records, indices, cost))
        .collect();
    comparison.effective_events_60m_at_8bps = event_comparison(records, indices);
    comparison
}

fn clustered_event_count(
    records: &[CounterfactualRecord],
    indices: &[usize],
    predicate: impl Fn(&CounterfactualRecord) -> bool,
) -> usize {
    let mut times = indices
        .iter()
        .filter_map(|&index| predicate(&records[index]).then_some(records[index].signal_time_ms))
        .collect::<Vec<_>>();
    times.sort_unstable();
    let mut events = 0;
    let mut prior = None;
    for timestamp_ms in times {
        if prior.is_none_or(|prior| timestamp_ms - prior > EVENT_CLUSTER_MS) {
            events += 1;
        }
        prior = Some(timestamp_ms);
    }
    events
}

/// 计算指定成本下的完整基线/变体指标与亏损收缩比例。
fn cost_comparison(
    records: &[CounterfactualRecord],
    indices: &[usize],
    cost_bps_per_side: f64,
) -> CostComparison {
    let baseline = r_metrics(records, indices, cost_bps_per_side, false);
    let variant = r_metrics(records, indices, cost_bps_per_side, true);
    let delta = variant.net_r - baseline.net_r;
    CostComparison {
        cost_bps_per_side,
        baseline_loss_reduction_percent: (baseline.net_r < 0.0)
            .then_some(delta / baseline.net_r.abs() * 100.0),
        baseline,
        variant,
        variant_minus_baseline_net_r: delta,
    }
}

/// 从逐笔退出重建闭仓 R 曲线；变体使用其真实提前退出时刻重新排序。
fn r_metrics(
    records: &[CounterfactualRecord],
    indices: &[usize],
    cost_bps_per_side: f64,
    variant: bool,
) -> RMetrics {
    let mut outcomes = indices
        .iter()
        .map(|&index| {
            let record = &records[index];
            (
                if variant {
                    record.variant_exit_time_ms
                } else {
                    record.baseline_exit_time_ms
                },
                record.symbol.as_str(),
                record.signal_time_ms,
                record_net_r(record, variant, cost_bps_per_side),
            )
        })
        .collect::<Vec<_>>();
    outcomes.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let wins = outcomes.iter().filter(|outcome| outcome.3 > 0.0).count();
    let losses = outcomes.iter().filter(|outcome| outcome.3 < 0.0).count();
    let gross_profit_r = outcomes.iter().map(|outcome| outcome.3.max(0.0)).sum();
    let gross_loss_r = outcomes.iter().map(|outcome| (-outcome.3).max(0.0)).sum();
    let net_r = outcomes.iter().map(|outcome| outcome.3).sum::<f64>();
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for outcome in &outcomes {
        equity += outcome.3;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    RMetrics {
        trades: outcomes.len(),
        wins,
        losses,
        flat: outcomes.len() - wins - losses,
        win_rate_percent: if outcomes.is_empty() {
            0.0
        } else {
            wins as f64 / outcomes.len() as f64 * 100.0
        },
        net_r,
        average_net_r: if outcomes.is_empty() {
            0.0
        } else {
            net_r / outcomes.len() as f64
        },
        gross_profit_r,
        gross_loss_r,
        profit_factor_r: (gross_loss_r > 0.0).then_some(gross_profit_r / gross_loss_r),
        profit_factor_r_is_infinite: gross_loss_r == 0.0 && gross_profit_r > 0.0,
        chronological_closed_equity_max_drawdown_r: max_drawdown,
    }
}

/// 同方向 60 分钟链式聚类，比较同一市场事件中退出保护的净影响。
fn event_comparison(records: &[CounterfactualRecord], indices: &[usize]) -> EventComparison {
    let mut ordered = indices.to_vec();
    ordered.sort_by_key(|&index| records[index].signal_time_ms);
    let mut events = Vec::<Vec<usize>>::new();
    for index in ordered {
        let starts_new = events.last().is_none_or(|event| {
            records[index].signal_time_ms
                - records[*event.last().expect("event is non-empty")].signal_time_ms
                > EVENT_CLUSTER_MS
        });
        if starts_new {
            events.push(vec![index]);
        } else {
            events.last_mut().expect("event exists").push(index);
        }
    }

    let mut comparison = EventComparison {
        raw_trades: indices.len(),
        events: events.len(),
        ..EventComparison::default()
    };
    for event in events {
        let symbols = event
            .iter()
            .map(|&index| records[index].symbol.as_str())
            .collect::<BTreeSet<_>>();
        if symbols.len() > 1 {
            comparison.multi_symbol_events += 1;
        } else {
            comparison.single_symbol_events += 1;
        }
        let baseline = event
            .iter()
            .map(|&index| record_net_r(&records[index], false, NET_BREAK_EVEN_COST_BPS_PER_SIDE))
            .sum::<f64>();
        let variant = event
            .iter()
            .map(|&index| record_net_r(&records[index], true, NET_BREAK_EVEN_COST_BPS_PER_SIDE))
            .sum::<f64>();
        comparison.baseline_net_r += baseline;
        comparison.variant_net_r += variant;
        if variant > baseline + FLOAT_TOLERANCE {
            comparison.improved_events += 1;
        } else if variant + FLOAT_TOLERANCE < baseline {
            comparison.worsened_events += 1;
        }
        comparison.largest_event_trade_count =
            comparison.largest_event_trade_count.max(event.len());
        comparison.largest_event_symbol_count =
            comparison.largest_event_symbol_count.max(symbols.len());
    }
    comparison.variant_minus_baseline_net_r = comparison.variant_net_r - comparison.baseline_net_r;
    comparison
}

/// 把成交价与单边成本转换成固定初始风险 R，不改写原始风险金额。
fn net_r(
    direction: Direction,
    entry_price: f64,
    exit_price: f64,
    initial_risk: f64,
    cost_bps_per_side: f64,
) -> f64 {
    if initial_risk <= 0.0 {
        return 0.0;
    }
    let gross = direction.gross_pnl(entry_price, exit_price);
    let costs = (entry_price + exit_price) * cost_bps_per_side / 10_000.0;
    (gross - costs) / initial_risk
}

fn record_net_r(record: &CounterfactualRecord, variant: bool, cost_bps_per_side: f64) -> f64 {
    net_r(
        Direction::Short,
        record.entry_price,
        if variant {
            record.variant_exit_price
        } else {
            record.baseline_exit_price
        },
        record.initial_risk,
        cost_bps_per_side,
    )
}

/// 按预注册五项数值门槛给出机械判断，不把通过写成生产晋级。
fn metric_gate(
    overall: &CohortComparison,
    target: &CohortComparison,
    outside: &CohortComparison,
    mode: ProtectionMode,
) -> MetricGate {
    let overall_8 = cost_at(overall, 8.0);
    let target_8 = cost_at(target, 8.0);
    let outside_8 = cost_at(outside, 8.0);
    let overall_10 = cost_at(overall, 10.0);
    let overall_pf_improved = pf_not_lower(
        overall_8.variant.profit_factor_r,
        overall_8.baseline.profit_factor_r,
    ) && overall_8.variant.profit_factor_r
        != overall_8.baseline.profit_factor_r;
    let target_pf_not_lower = pf_not_lower(
        target_8.variant.profit_factor_r,
        target_8.baseline.profit_factor_r,
    );
    let target_reduction = target_8.baseline_loss_reduction_percent;
    let outside_decline = if outside_8.variant.net_r >= outside_8.baseline.net_r {
        0.0
    } else if outside_8.baseline.net_r.abs() > FLOAT_TOLERANCE {
        (outside_8.baseline.net_r - outside_8.variant.net_r) / outside_8.baseline.net_r.abs()
            * 100.0
    } else {
        f64::INFINITY
    };
    let sample_gate_required = mode == ProtectionMode::StructureBreakFailedRetest;
    let activation_sample_passed = !sample_gate_required || overall.failed_retest_confirmed >= 30;
    let event_sample_passed = !sample_gate_required || overall.activated_effective_events_60m >= 20;
    let checks = [
        overall_8.variant.net_r > overall_8.baseline.net_r,
        overall_8.variant.average_net_r > overall_8.baseline.average_net_r,
        overall_pf_improved,
        target_reduction.is_some_and(|value| value >= 25.0),
        target_pf_not_lower,
        outside_decline <= 10.0,
        overall_10.variant.net_r > 0.0,
        activation_sample_passed,
        event_sample_passed,
    ];
    MetricGate {
        overall_net_r_improved_at_8bps: checks[0],
        overall_average_r_improved_at_8bps: checks[1],
        overall_profit_factor_improved_at_8bps: checks[2],
        target_loss_reduction_percent_at_8bps: target_reduction,
        target_loss_reduction_at_least_25_percent: checks[3],
        target_profit_factor_not_lower_at_8bps: checks[4],
        outside_net_r_decline_percent_at_8bps: outside_decline,
        outside_net_r_decline_within_10_percent: checks[5],
        variant_net_r_positive_at_10bps: checks[6],
        activation_sample_gate_required: sample_gate_required,
        failed_retest_activations_at_least_30: checks[7],
        activated_effective_events_at_least_20: checks[8],
        trade_identity_preserved: true,
        metric_gate_passed: checks.into_iter().all(|passed| passed),
    }
}

fn cost_at(comparison: &CohortComparison, bps: f64) -> &CostComparison {
    comparison
        .costs
        .iter()
        .find(|cost| nearly_equal(cost.cost_bps_per_side, bps))
        .expect("frozen stress cost exists")
}

fn pf_not_lower(candidate: Option<f64>, baseline: Option<f64>) -> bool {
    match (candidate, baseline) {
        (Some(candidate), Some(baseline)) => candidate + FLOAT_TOLERANCE >= baseline,
        (None, None) => true,
        (None, Some(_)) => true,
        (Some(_), None) => false,
    }
}

fn is_target_2025_aug_sep(timestamp_ms: i64) -> bool {
    let Some(timestamp) = Utc.timestamp_millis_opt(timestamp_ms).single() else {
        return false;
    };
    timestamp.year() == 2025 && matches!(timestamp.month(), 8 | 9)
}

fn is_btc_symbol(symbol: &str) -> bool {
    symbol.eq_ignore_ascii_case("BTC-USDT-SWAP")
}

fn nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= FLOAT_TOLERANCE.max(left.abs().max(right.abs()) * 1e-10)
}

#[cfg(test)]
#[path = "ema_short_exit_counterfactual_tests.rs"]
mod tests;
