use super::ema_short_exit_counterfactual::ExitCounterfactualInput;
use anyhow::{anyhow, bail, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    Candle, Direction, EntryIntent, ExitPolicy, ExitReason, SignalFamily, Trade,
};
use serde::Serialize;

#[path = "strict_visual_range_height_net_break_even/metrics.rs"]
mod metrics;
#[path = "strict_visual_range_height_net_break_even/two_close_failure.rs"]
mod two_close_failure;
use metrics::{
    contribution_comparisons, cost_comparisons, distribution_summary, event_comparison,
    metric_gate, net_r, shanghai_month, ContributionComparison, CostComparison,
    DistributionSummary, EventComparison, MetricGate,
};
pub(crate) use two_close_failure::{
    build_strict_visual_two_close_failure_l1, StrictVisualTwoCloseFailureL1Report,
};

const ACTIVATION_HEIGHT_MULTIPLE: f64 = 1.0;
const NET_BREAK_EVEN_COST_BPS_PER_SIDE: f64 = 8.0;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const FLOAT_TOLERANCE: f64 = 1e-8;

/// V8 纯严格视觉横盘交易按冻结区间高度激活净保本的隔离退出研究。
#[derive(Debug, Serialize)]
pub(crate) struct StrictVisualRangeHeightNetBreakEvenReport {
    /// 独立 Research 身份；不会覆盖冻结 V8 或注册到运行态。
    research_version: &'static str,
    /// 唯一变量与完成棒时序的可审计定义。
    definition: &'static str,
    /// 提前退出不释放容量，避免反事实补造后续交易。
    implementation_mode: &'static str,
    /// 入场后 K 线只参与退出模拟，不能反向改变交易身份。
    future_data_usage: &'static str,
    /// 激活距离相对冻结横盘高度的倍数。
    activation_height_multiple: f64,
    /// 净保本保护覆盖的单边手续费与滑点，单位 bps。
    protected_cost_bps_per_side: f64,
    /// V8 中所有包含严格视觉横盘家族的已闭仓交易。
    strict_visual_family_trades: usize,
    /// 真正应用唯一退出变量的纯家族 Fixed 交易。
    eligible_pure_fixed_trades: usize,
    /// 混合其他信号家族的交易保持 V8 原样，避免改变其他策略语义。
    excluded_mixed_family_trades: usize,
    /// 纯严格横盘但使用非 Fixed 退出的交易保持 V8 原样。
    excluded_non_fixed_pure_trades: usize,
    /// 缺失或非法冻结高度会使研究失败；正常结果必须为零。
    missing_or_invalid_range_height_trades: usize,
    /// 当前 V8 可用于经验回放的多单数量。
    empirical_long_trades: usize,
    /// 当前 V8 没有严格视觉横盘空单；镜像只由单元测试固定合同。
    empirical_short_trades: usize,
    /// 交易与信号身份发生漂移的数量；正常结果必须为零。
    identity_changed_trades: usize,
    /// 激活价不越过原冻结目标的交易数，属于不看结果的几何覆盖。
    activation_not_beyond_frozen_target: usize,
    /// 成本保本地板比 `entry ± 1H` 更远的交易数。
    activation_price_raised_to_cost_floor: usize,
    /// 原退出前由已完成 K 线激活保护的交易数。
    activated_on_completed_candle_extreme: usize,
    /// 激活交易按 60 分钟链式规则合并后的事件数。
    activated_effective_events_60m: usize,
    /// 新保护实际早于原退出成交的交易数。
    net_break_even_exits: usize,
    /// 因开盘越过保护价而按真实开盘成交的交易数。
    net_break_even_gap_open_exits: usize,
    /// 新保护未改变退出时间和价格的交易数。
    unchanged_exits: usize,
    /// 冻结横盘高度相对初始风险 R 的分布。
    range_height_in_initial_r: DistributionSummary,
    /// 完全相同交易身份在不同成本压力下的基线与变体指标。
    costs: Vec<CostComparison>,
    /// 8bps 单边成本下的币种贡献。
    per_symbol_8bps: Vec<ContributionComparison>,
    /// 8bps 单边成本下的上海自然月贡献。
    per_shanghai_month_8bps: Vec<ContributionComparison>,
    /// 8bps 单边成本下的 60 分钟市场事件贡献。
    effective_events_60m_at_8bps: EventComparison,
    /// 预注册 L2 闸门；通过也只代表值得进入 L3。
    metric_gate: MetricGate,
    /// 每笔交易的冻结高度、激活、保护与退出证据。
    records: Vec<CounterfactualRecord>,
}

/// 每笔交易只改变退出保护，保留原信号、成交、初始风险和目标。
#[derive(Debug, Serialize)]
struct CounterfactualRecord {
    symbol: String,
    direction: Direction,
    /// Unix 毫秒时间戳。
    signal_time_ms: i64,
    /// Unix 毫秒时间戳。
    entry_time_ms: i64,
    /// Unix 毫秒时间戳。
    baseline_exit_time_ms: i64,
    /// Unix 毫秒时间戳。
    variant_exit_time_ms: i64,
    baseline_exit_reason: ExitReason,
    variant_exit_kind: CounterfactualExitKind,
    /// 完成棒激活时间；`None` 表示原交易结束前没有激活。
    activation_time_ms: Option<i64>,
    entry_price: f64,
    initial_stop: f64,
    initial_risk: f64,
    frozen_target: f64,
    frozen_range_height: f64,
    range_height_in_initial_r: f64,
    activation_price: f64,
    net_break_even_stop: f64,
    activation_price_raised_to_cost_floor: bool,
    activation_not_beyond_frozen_target: bool,
    baseline_exit_price: f64,
    variant_exit_price: f64,
    baseline_net_r_8bps: f64,
    variant_net_r_8bps: f64,
}

/// 新保护未触发时保留原退出；跳空不会虚构成交在保护价。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CounterfactualExitKind {
    BaselineUnchanged,
    NetBreakEvenStop,
    NetBreakEvenGapOpen,
}

/// 单笔退出模拟状态；激活棒只冻结保护，不能改写自身价格路径。
#[derive(Debug, Clone, Copy, PartialEq)]
struct SimulatedExit {
    /// Unix 毫秒时间戳。
    activation_time_ms: Option<i64>,
    /// Unix 毫秒时间戳。
    exit_time_ms: i64,
    exit_price: f64,
    kind: CounterfactualExitKind,
}

/// 构建 V8 的 1.0H 净保本隔离退出报告，不重跑或制造后续入场。
pub(crate) fn build_strict_visual_range_height_net_break_even(
    inputs: &[ExitCounterfactualInput<'_>],
) -> Result<StrictVisualRangeHeightNetBreakEvenReport> {
    let mut records = Vec::new();
    let mut strict_visual_family_trades = 0;
    let mut excluded_mixed_family_trades = 0;
    let mut excluded_non_fixed_pure_trades = 0;

    for input in inputs {
        validate_input_identity(input)?;
        for (zero_trade, cost_trade) in input
            .zero_cost
            .trades
            .iter()
            .zip(&input.cost_adjusted.trades)
        {
            validate_trade_identity(input.symbol, zero_trade, cost_trade)?;
            if !zero_trade
                .families
                .contains(&SignalFamily::StrictVisualConsolidationBreakLong)
            {
                continue;
            }
            strict_visual_family_trades += 1;
            validate_cost_replay(zero_trade, cost_trade)?;

            if !is_pure_strict_visual(zero_trade) {
                excluded_mixed_family_trades += 1;
                continue;
            }
            if zero_trade.exit_policy != ExitPolicy::Fixed {
                excluded_non_fixed_pure_trades += 1;
                continue;
            }
            if zero_trade.direction != Direction::Long {
                bail!(
                    "{} @ {} 的严格视觉横盘家族方向不是多单",
                    input.symbol,
                    zero_trade.signal_time_ms
                );
            }

            let intent = matching_intent(input.zero_cost, zero_trade)?;
            validate_cost_intent(input.cost_adjusted, zero_trade, intent)?;
            let range_height = intent.strict_visual_range_height.ok_or_else(|| {
                anyhow!(
                    "{} @ {} 缺少信号时冻结的严格视觉横盘高度",
                    input.symbol,
                    zero_trade.signal_time_ms
                )
            })?;
            if !range_height.is_finite() || range_height <= 0.0 {
                bail!(
                    "{} @ {} 的冻结横盘高度无效：{}",
                    input.symbol,
                    zero_trade.signal_time_ms,
                    range_height
                );
            }
            let frozen_target =
                actual_target_from_intent(intent, zero_trade.entry_price, input.tick_size)?;
            records.push(build_record(
                input.symbol,
                input.candles,
                input.tick_size,
                zero_trade,
                range_height,
                frozen_target,
            )?);
        }
    }

    let costs = cost_comparisons(&records);
    let per_symbol_8bps = contribution_comparisons(&records, |record| record.symbol.clone());
    let per_shanghai_month_8bps =
        contribution_comparisons(&records, |record| shanghai_month(record.signal_time_ms));
    let effective_events_60m_at_8bps = event_comparison(&records);
    let metric_gate = metric_gate(
        &records,
        &costs,
        &per_symbol_8bps,
        &per_shanghai_month_8bps,
        &effective_events_60m_at_8bps,
    );
    let activated = records
        .iter()
        .filter(|record| record.activation_time_ms.is_some())
        .count();
    let changed = records
        .iter()
        .filter(|record| record.variant_exit_kind != CounterfactualExitKind::BaselineUnchanged)
        .count();
    let gap_exits = records
        .iter()
        .filter(|record| record.variant_exit_kind == CounterfactualExitKind::NetBreakEvenGapOpen)
        .count();

    Ok(StrictVisualRangeHeightNetBreakEvenReport {
        research_version: "strict_visual_breakout_range_height_1_0_net_be_15m_research_v4",
        definition: "pure strict_visual_consolidation_break_long Fixed trades; completed candle high reaches max(actual entry + 1.0 * frozen visual range height, tick-rounded 8bps-per-side net break-even), then the tighter stop starts on the next candle; short formula is mirrored and unit-tested",
        implementation_mode: "trade-isolated exit counterfactual over frozen V8 identities; earlier exits do not release capacity or manufacture later entries",
        future_data_usage: "range height is frozen on the signal intent; post-entry candles only simulate exit activation and fill, never signal, entry, eligibility, initial risk, target, or another trade",
        activation_height_multiple: ACTIVATION_HEIGHT_MULTIPLE,
        protected_cost_bps_per_side: NET_BREAK_EVEN_COST_BPS_PER_SIDE,
        strict_visual_family_trades,
        eligible_pure_fixed_trades: records.len(),
        excluded_mixed_family_trades,
        excluded_non_fixed_pure_trades,
        missing_or_invalid_range_height_trades: 0,
        empirical_long_trades: records
            .iter()
            .filter(|record| record.direction == Direction::Long)
            .count(),
        empirical_short_trades: records
            .iter()
            .filter(|record| record.direction == Direction::Short)
            .count(),
        identity_changed_trades: 0,
        activation_not_beyond_frozen_target: records
            .iter()
            .filter(|record| record.activation_not_beyond_frozen_target)
            .count(),
        activation_price_raised_to_cost_floor: records
            .iter()
            .filter(|record| record.activation_price_raised_to_cost_floor)
            .count(),
        activated_on_completed_candle_extreme: activated,
        activated_effective_events_60m: effective_events_60m_at_8bps.activated_events,
        net_break_even_exits: changed,
        net_break_even_gap_open_exits: gap_exits,
        unchanged_exits: records.len() - changed,
        range_height_in_initial_r: distribution_summary(
            records
                .iter()
                .map(|record| record.range_height_in_initial_r)
                .collect(),
        ),
        costs,
        per_symbol_8bps,
        per_shanghai_month_8bps,
        effective_events_60m_at_8bps,
        metric_gate,
        records,
    })
}

fn build_record(
    symbol: &str,
    candles: &[Candle],
    tick_size: f64,
    trade: &Trade,
    range_height: f64,
    frozen_target: f64,
) -> Result<CounterfactualRecord> {
    let net_break_even_stop = net_break_even_price(
        trade.direction,
        trade.entry_price,
        tick_size,
        NET_BREAK_EVEN_COST_BPS_PER_SIDE,
    );
    let raw_height_activation =
        raw_height_activation_price(trade.direction, trade.entry_price, range_height);
    let activation_price = activation_price(
        trade.direction,
        trade.entry_price,
        range_height,
        net_break_even_stop,
    );
    let simulated = simulate_trade(trade, candles, tick_size, range_height)?;
    let activation_price_raised_to_cost_floor = match trade.direction {
        Direction::Long => net_break_even_stop > raw_height_activation + FLOAT_TOLERANCE,
        Direction::Short => net_break_even_stop + FLOAT_TOLERANCE < raw_height_activation,
    };
    let activation_not_beyond_frozen_target = match trade.direction {
        Direction::Long => activation_price <= frozen_target + tick_size * 0.5,
        Direction::Short => activation_price + tick_size * 0.5 >= frozen_target,
    };

    Ok(CounterfactualRecord {
        symbol: symbol.to_owned(),
        direction: trade.direction,
        signal_time_ms: trade.signal_time_ms,
        entry_time_ms: trade.entry_time_ms,
        baseline_exit_time_ms: trade.exit_time_ms,
        variant_exit_time_ms: simulated.exit_time_ms,
        baseline_exit_reason: trade.exit_reason,
        variant_exit_kind: simulated.kind,
        activation_time_ms: simulated.activation_time_ms,
        entry_price: trade.entry_price,
        initial_stop: trade.initial_stop,
        initial_risk: trade.initial_risk,
        frozen_target,
        frozen_range_height: range_height,
        range_height_in_initial_r: range_height / trade.initial_risk,
        activation_price,
        net_break_even_stop,
        activation_price_raised_to_cost_floor,
        activation_not_beyond_frozen_target,
        baseline_exit_price: trade.exit_price,
        variant_exit_price: simulated.exit_price,
        baseline_net_r_8bps: net_r(
            trade.direction,
            trade.entry_price,
            trade.exit_price,
            trade.initial_risk,
            NET_BREAK_EVEN_COST_BPS_PER_SIDE,
        ),
        variant_net_r_8bps: net_r(
            trade.direction,
            trade.entry_price,
            simulated.exit_price,
            trade.initial_risk,
            NET_BREAK_EVEN_COST_BPS_PER_SIDE,
        ),
    })
}

fn is_pure_strict_visual(trade: &Trade) -> bool {
    trade.families.len() == 1
        && trade.families[0] == SignalFamily::StrictVisualConsolidationBreakLong
}

fn matching_intent<'a>(
    report: &'a rust_quant_cli::app::tradingview_velocity_parity::ReplayReport,
    trade: &Trade,
) -> Result<&'a EntryIntent> {
    let matches = report
        .entry_candidates
        .iter()
        .filter(|intent| {
            intent.signal_time_ms == trade.signal_time_ms && intent.direction == trade.direction
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!(
            "{} @ {} 匹配到 {} 个入场意图",
            report.symbol,
            trade.signal_time_ms,
            matches.len()
        );
    }
    Ok(matches[0])
}

fn validate_cost_intent(
    cost_report: &rust_quant_cli::app::tradingview_velocity_parity::ReplayReport,
    trade: &Trade,
    zero_intent: &EntryIntent,
) -> Result<()> {
    let cost_intent = matching_intent(cost_report, trade)?;
    if cost_intent.families != zero_intent.families
        || cost_intent.exit_policy != zero_intent.exit_policy
        || cost_intent.strict_visual_range_height != zero_intent.strict_visual_range_height
        || cost_intent.target_price != zero_intent.target_price
        || cost_intent.target_ticks != zero_intent.target_ticks
    {
        bail!(
            "{} @ {} 的零成本与成本后冻结意图漂移",
            cost_report.symbol,
            trade.signal_time_ms
        );
    }
    Ok(())
}

fn actual_target_from_intent(
    intent: &EntryIntent,
    entry_price: f64,
    tick_size: f64,
) -> Result<f64> {
    let target = intent.target_price.or_else(|| {
        intent.target_ticks.map(|ticks| match intent.direction {
            Direction::Long => entry_price + ticks as f64 * tick_size,
            Direction::Short => entry_price - ticks as f64 * tick_size,
        })
    });
    target
        .filter(|price| price.is_finite())
        .ok_or_else(|| anyhow!("信号 {} 缺少可还原的冻结目标", intent.signal_time_ms))
}

/// 沿原交易存活区间模拟动态保护；激活棒只更新状态，下一根才可成交。
fn simulate_trade(
    trade: &Trade,
    candles: &[Candle],
    tick_size: f64,
    range_height: f64,
) -> Result<SimulatedExit> {
    let baseline = baseline_exit(trade);
    let entry_index = candles
        .binary_search_by_key(&trade.entry_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow!("找不到严格横盘入场 K 线：{}", trade.entry_time_ms))?;
    let exit_index = candles
        .binary_search_by_key(&trade.exit_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow!("找不到严格横盘退出 K 线：{}", trade.exit_time_ms))?;
    if exit_index < entry_index {
        bail!("严格横盘退出早于入场：{}", trade.signal_time_ms);
    }

    let stop = net_break_even_price(
        trade.direction,
        trade.entry_price,
        tick_size,
        NET_BREAK_EVEN_COST_BPS_PER_SIDE,
    );
    let activation = activation_price(trade.direction, trade.entry_price, range_height, stop);
    let mut activation_time_ms = None;

    for candle in &candles[entry_index..=exit_index] {
        let is_baseline_exit_candle = candle.timestamp_ms == trade.exit_time_ms;
        let active_from_prior_close =
            activation_time_ms.is_some_and(|activated| candle.timestamp_ms > activated);

        if active_from_prior_close {
            // 反手在下一根开盘具有独立策略优先级，退出研究不能抢占该身份。
            if is_baseline_exit_candle && trade.exit_reason == ExitReason::ReverseAtNextOpen {
                return Ok(with_activation(baseline, activation_time_ms));
            }
            if stop_crossed_at_open(trade.direction, candle.open, stop) {
                return Ok(SimulatedExit {
                    activation_time_ms,
                    exit_time_ms: candle.timestamp_ms,
                    exit_price: candle.open,
                    kind: CounterfactualExitKind::NetBreakEvenGapOpen,
                });
            }
            if is_baseline_exit_candle && nearly_equal(trade.exit_price, candle.open) {
                return Ok(with_activation(baseline, activation_time_ms));
            }
            if let Some(kind) = first_exit_on_path(
                *candle,
                stop,
                is_baseline_exit_candle.then_some(trade.exit_price),
            ) {
                return Ok(match kind {
                    PathExit::Baseline => with_activation(baseline, activation_time_ms),
                    PathExit::Protection => SimulatedExit {
                        activation_time_ms,
                        exit_time_ms: candle.timestamp_ms,
                        exit_price: stop,
                        kind: CounterfactualExitKind::NetBreakEvenStop,
                    },
                });
            }
        }

        if is_baseline_exit_candle {
            return Ok(with_activation(baseline, activation_time_ms));
        }
        if activation_time_ms.is_none() && activation_reached(trade.direction, *candle, activation)
        {
            activation_time_ms = Some(candle.timestamp_ms);
        }
    }
    Ok(with_activation(baseline, activation_time_ms))
}

fn baseline_exit(trade: &Trade) -> SimulatedExit {
    SimulatedExit {
        activation_time_ms: None,
        exit_time_ms: trade.exit_time_ms,
        exit_price: trade.exit_price,
        kind: CounterfactualExitKind::BaselineUnchanged,
    }
}

fn with_activation(mut exit: SimulatedExit, activation_time_ms: Option<i64>) -> SimulatedExit {
    exit.activation_time_ms = activation_time_ms;
    exit
}

/// 成本后净保本按方向向安全侧取 tick，避免精度截断留下小亏。
fn net_break_even_price(
    direction: Direction,
    entry_price: f64,
    tick_size: f64,
    cost_bps_per_side: f64,
) -> f64 {
    let cost_ratio = cost_bps_per_side / 10_000.0;
    match direction {
        Direction::Long => round_up(
            entry_price * (1.0 + cost_ratio) / (1.0 - cost_ratio),
            tick_size,
        ),
        Direction::Short => round_down(
            entry_price * (1.0 - cost_ratio) / (1.0 + cost_ratio),
            tick_size,
        ),
    }
}

fn raw_height_activation_price(direction: Direction, entry_price: f64, height: f64) -> f64 {
    match direction {
        Direction::Long => entry_price + ACTIVATION_HEIGHT_MULTIPLE * height,
        Direction::Short => entry_price - ACTIVATION_HEIGHT_MULTIPLE * height,
    }
}

/// `1H` 若不足以覆盖成本则等待更远的净保本地板，不提前宣称已保本。
fn activation_price(
    direction: Direction,
    entry_price: f64,
    height: f64,
    net_break_even: f64,
) -> f64 {
    let raw = raw_height_activation_price(direction, entry_price, height);
    match direction {
        Direction::Long => raw.max(net_break_even),
        Direction::Short => raw.min(net_break_even),
    }
}

fn activation_reached(direction: Direction, candle: Candle, activation_price: f64) -> bool {
    match direction {
        Direction::Long => candle.high >= activation_price,
        Direction::Short => candle.low <= activation_price,
    }
}

fn stop_crossed_at_open(direction: Direction, open: f64, stop: f64) -> bool {
    match direction {
        Direction::Long => open <= stop,
        Direction::Short => open >= stop,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathExit {
    Baseline,
    Protection,
}

/// 按冻结的 TradingView OHLC 路径比较同一根内保护与原退出的先后。
fn first_exit_on_path(
    candle: Candle,
    protection_stop: f64,
    baseline_exit_level: Option<f64>,
) -> Option<PathExit> {
    for segment in broker_path(candle).windows(2) {
        let protection_hit = between(protection_stop, segment[0], segment[1]);
        let baseline_hit =
            baseline_exit_level.is_some_and(|level| between(level, segment[0], segment[1]));
        match (protection_hit, baseline_hit) {
            (false, false) => {}
            (true, false) => return Some(PathExit::Protection),
            (false, true) => return Some(PathExit::Baseline),
            (true, true) => {
                let baseline = baseline_exit_level.expect("baseline_hit requires a level");
                let protection_first = if segment[1] >= segment[0] {
                    protection_stop <= baseline
                } else {
                    protection_stop >= baseline
                };
                return Some(if protection_first {
                    PathExit::Protection
                } else {
                    PathExit::Baseline
                });
            }
        }
    }
    None
}

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

fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

/// 行情、零成本报告与成本报告必须是同一成员和同一交易序列。
fn validate_input_identity(input: &ExitCounterfactualInput<'_>) -> Result<()> {
    if input.symbol != input.zero_cost.symbol || input.symbol != input.cost_adjusted.symbol {
        bail!("严格横盘 1H 净保本成员 identity 不一致：{}", input.symbol);
    }
    if input.zero_cost.trades.len() != input.cost_adjusted.trades.len() {
        bail!("{} 的零成本与成本后交易数量不一致", input.symbol);
    }
    if input.zero_cost.entry_candidates.len() != input.cost_adjusted.entry_candidates.len() {
        bail!("{} 的零成本与成本后候选数量不一致", input.symbol);
    }
    if input.tick_size <= 0.0 || !input.tick_size.is_finite() {
        bail!("{} 的 tick size 无效", input.symbol);
    }
    Ok(())
}

/// 成本报告只能改变净收益，不能改变成交、风险或原退出身份。
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
        && nearly_equal(zero.initial_risk, cost.initial_risk)
        && zero.exit_reason == cost.exit_reason;
    if !same {
        bail!(
            "{} @ {} 的零成本与成本后交易 identity 漂移",
            symbol,
            zero.signal_time_ms
        );
    }
    Ok(())
}

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

fn nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= FLOAT_TOLERANCE.max(left.abs().max(right.abs()) * 1e-10)
}

#[cfg(test)]
#[path = "strict_visual_range_height_net_break_even_tests.rs"]
mod tests;
