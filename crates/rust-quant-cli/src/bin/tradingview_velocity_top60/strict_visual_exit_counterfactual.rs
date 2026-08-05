use super::ema_short_exit_counterfactual::ExitCounterfactualInput;
use anyhow::{bail, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    Candle, Direction, ExitPolicy, ExitReason, SignalFamily, Trade,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const NET_BREAK_EVEN_COST_BPS_PER_SIDE: f64 = 8.0;
const STRESS_COST_BPS_PER_SIDE: [f64; 4] = [0.0, 8.0, 10.0, 12.0];
const ACTIVATION_R_VARIANTS: [f64; 2] = [0.5, 1.0];
const COMPLETED_CLOSE_ACTIVATION_R: f64 = 1.0;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const FLOAT_TOLERANCE: f64 = 1e-8;

/// 严格视觉横盘突破多单在一个 Research 身份下的隔离净保本对照。
#[derive(Debug, Serialize)]
pub(crate) struct StrictVisualNetBreakEvenActivationReport {
    /// 独立 Research 身份，避免与 V6 入场版本混淆。
    research_version: &'static str,
    /// 仅改变退出保护阈值的因果定义。
    definition: &'static str,
    /// 说明提前退出不会释放容量或补造后续成交。
    implementation_mode: &'static str,
    /// 说明持仓后 K 线只参与退出，不反向改变信号或入场。
    future_data_usage: &'static str,
    /// 净保本价覆盖的单边成本，单位 bps。
    protected_cost_bps_per_side: f64,
    /// 逐笔身份校验后进入报告的严格横盘交易数。
    valid_trade_records: usize,
    /// 信号、入场、初始风险或原始目标发生漂移的交易数；必须为零。
    identity_changed_trades: usize,
    /// 当前 Research 身份预注册的激活变体。
    variants: Vec<ActivationVariantReport>,
}

/// 已完成 K 线用于确认保护激活的价格证据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivationEvidence {
    /// V1 使用完成棒最高价，允许上影触达阈值后激活。
    CompletedHigh,
    /// V2 要求完成棒收盘站上阈值，过滤仅盘中触达的弱接受。
    CompletedClose,
}

/// 一个完成棒激活阈值的全样本、集中度与逐笔审计结果。
#[derive(Debug, Serialize)]
struct ActivationVariantReport {
    /// 当前 Research 身份要求完成棒证据达到的初始风险倍数。
    activation_r: f64,
    /// 严格横盘家族全部交易数，包含保持原样的非固定退出。
    trades: usize,
    /// 可以应用本变量的 Fixed 多单交易数。
    eligible_fixed_long_trades: usize,
    /// 因成本净保本高于原始 R 阈值而抬高激活价的交易数。
    activation_price_raised_to_cost_floor: usize,
    /// V1 由完成 K 线最高价激活的交易数；V2 不输出该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    activated_on_completed_high: Option<usize>,
    /// V2 由完成 K 线收盘价激活的交易数；V1 不输出该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    activated_on_completed_close: Option<usize>,
    /// 激活交易按 60 分钟链式规则合并后的事件数。
    activated_effective_events_60m: usize,
    /// 实际由新保护提前退出的交易数。
    net_break_even_exits: usize,
    /// 未被新保护改变退出的交易数。
    unchanged_exits: usize,
    /// 各成本压力下完全相同交易身份的基线与变体指标。
    costs: Vec<CostComparison>,
    /// 单币贡献，用于识别改善是否集中。
    per_symbol_8bps: Vec<ContributionComparison>,
    /// 上海自然月贡献，用于识别单一月份依赖。
    per_shanghai_month_8bps: Vec<ContributionComparison>,
    /// 同方向 60 分钟市场事件贡献。
    effective_events_60m_at_8bps: EventComparison,
    /// 预注册 L2 数值门禁；通过也不等于可以生产晋级。
    metric_gate: MetricGate,
    /// 每笔交易的激活与退出证据，时间均为 Unix 毫秒。
    records: Vec<CounterfactualRecord>,
}

/// 固定成本压力下的基线与净保本变体 R 指标。
#[derive(Debug, Serialize)]
struct CostComparison {
    /// 单边手续费与滑点合计，单位 bps。
    cost_bps_per_side: f64,
    /// 原 V6 固定退出指标。
    baseline: RMetrics,
    /// 只改变净保本保护后的指标。
    variant: RMetrics,
    /// 变体净 R 减去基线净 R。
    variant_minus_baseline_net_r: f64,
}

/// 固定初始风险口径下的交易级表现。
#[derive(Debug, Default, Clone, Serialize)]
struct RMetrics {
    /// 完全相同的交易身份数量。
    trades: usize,
    /// 成本后 R 大于零的交易数。
    wins: usize,
    /// 成本后 R 小于零的交易数。
    losses: usize,
    /// 成本后 R 近似为零的交易数。
    flat: usize,
    /// 成本后胜率，单位百分比。
    win_rate_percent: f64,
    /// 全部交易成本后 R 之和。
    net_r: f64,
    /// 单笔成本后平均 R。
    average_net_r: f64,
    /// 正 R 之和。
    gross_profit_r: f64,
    /// 负 R 绝对值之和。
    gross_loss_r: f64,
    /// 毛盈利 R 除以毛亏损 R；无亏损时为 `None` 并由下一字段标记无穷。
    profit_factor_r: Option<f64>,
    /// `true` 表示有盈利且没有亏损，PF 应解释为无穷。
    profit_factor_r_is_infinite: bool,
    /// 按真实变体退出时间排序的闭仓 R 最大回撤。
    chronological_closed_equity_max_drawdown_r: f64,
}

/// 单币或单月的 8 bps 成本贡献。
#[derive(Debug, Serialize)]
struct ContributionComparison {
    /// 币种名或 `YYYY-MM` 上海月份。
    key: String,
    /// 当前分组交易数。
    trades: usize,
    /// 当前分组完成棒激活数。
    activations: usize,
    /// 原 V6 成本后净 R。
    baseline_net_r: f64,
    /// 净保本变体成本后净 R。
    variant_net_r: f64,
    /// 当前分组净 R 改善值。
    variant_minus_baseline_net_r: f64,
}

/// 60 分钟事件级退出贡献，避免把同一轮市场共振当作独立样本。
#[derive(Debug, Default, Serialize)]
struct EventComparison {
    /// 严格横盘原始交易数。
    raw_trades: usize,
    /// 链式聚类后的事件数。
    events: usize,
    /// 至少有一笔完成棒激活的事件数。
    activated_events: usize,
    /// 变体净 R 高于基线的事件数。
    improved_events: usize,
    /// 变体净 R 低于基线的事件数。
    worsened_events: usize,
    /// 变体与基线近似相同的事件数。
    unchanged_events: usize,
    /// 单个事件包含的最大交易数。
    largest_event_trade_count: usize,
    /// 单个事件覆盖的最大币种数。
    largest_event_symbol_count: usize,
    /// 所有事件的基线成本后净 R。
    baseline_net_r: f64,
    /// 所有事件的变体成本后净 R。
    variant_net_r: f64,
    /// 事件级汇总的净 R 改善值。
    variant_minus_baseline_net_r: f64,
}

/// L2 机械门禁只判断是否值得进入 L3，不产生任何运行态切换。
#[derive(Debug, Serialize)]
struct MetricGate {
    /// 8 bps 下变体净 R 是否高于基线。
    net_r_improved_at_8bps: bool,
    /// 8 bps 下变体平均 R 是否高于基线。
    average_r_improved_at_8bps: bool,
    /// 8 bps 下变体 PF 是否高于基线。
    profit_factor_improved_at_8bps: bool,
    /// 8 bps 下变体净 R 是否为正。
    variant_net_r_positive_at_8bps: bool,
    /// 8 bps 下变体 PF 是否大于一。
    variant_profit_factor_above_one_at_8bps: bool,
    /// 激活交易是否达到预注册的 30 笔。
    activated_trades_at_least_30: bool,
    /// 激活事件是否达到预注册的 20 个。
    activated_events_at_least_20: bool,
    /// 正向改善是否至少分布在 3 笔交易。
    improved_trades_at_least_3: bool,
    /// 正向改善是否至少分布在 3 个币。
    improved_symbols_at_least_3: bool,
    /// 正向改善是否至少分布在 3 个上海月份。
    improved_months_at_least_3: bool,
    /// 正向改善是否至少分布在 3 个事件簇。
    improved_events_at_least_3: bool,
    /// `true` 表示基线与反事实仅退出字段不同。
    trade_identity_preserved: bool,
    /// 全部 L2 预注册检查的合取结果。
    metric_gate_passed: bool,
}

/// 新保护相对于原冻结退出的逐笔证据。
#[derive(Debug, Serialize)]
struct CounterfactualRecord {
    /// OKX 永续合约标识。
    symbol: String,
    /// 原始信号时间，Unix 毫秒。
    signal_time_ms: i64,
    /// 实际开仓时间，Unix 毫秒。
    entry_time_ms: i64,
    /// 原 V6 退出时间，Unix 毫秒。
    baseline_exit_time_ms: i64,
    /// 净保本变体退出时间，Unix 毫秒。
    variant_exit_time_ms: i64,
    /// 原 V6 退出原因。
    baseline_exit_reason: ExitReason,
    /// 变体退出是否保持原样、触发净保本或发生跳空。
    variant_exit_kind: CounterfactualExitKind,
    /// `true` 表示交易属于 Fixed 严格横盘突破多单。
    eligible: bool,
    /// 完成棒激活时间；`None` 表示原交易结束前没有激活。
    activation_time_ms: Option<i64>,
    /// 实际开仓价。
    entry_price: f64,
    /// 冻结初始止损价。
    initial_stop: f64,
    /// 开仓价到初始止损的固定价格风险。
    initial_risk: f64,
    /// 完成棒指定价格证据必须达到的实际激活价。
    activation_price: Option<f64>,
    /// 覆盖双边 8 bps 并向上取 tick 的多单保护价。
    net_break_even_stop: Option<f64>,
    /// `true` 表示成本下限高于原始 R 激活价。
    activation_price_raised_to_cost_floor: bool,
    /// 原 V6 退出价。
    baseline_exit_price: f64,
    /// 净保本变体退出价。
    variant_exit_price: f64,
}

/// 新保护未触发时保留原退出；跳空不会虚构成交在保护价。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CounterfactualExitKind {
    BaselineUnchanged,
    NetBreakEvenStop,
    NetBreakEvenGapOpen,
}

/// 单笔模拟状态；保护只能在激活棒完成之后影响下一根。
#[derive(Debug, Clone, Copy, PartialEq)]
struct SimulatedExit {
    /// 完成棒激活时间，Unix 毫秒。
    activation_time_ms: Option<i64>,
    /// 变体退出时间，Unix 毫秒。
    exit_time_ms: i64,
    /// 变体退出价。
    exit_price: f64,
    /// 变体退出类别。
    kind: CounterfactualExitKind,
}

/// 构建两个预注册阈值的隔离退出报告；不会重跑或制造后续入场。
pub(crate) fn build_strict_visual_net_break_even_activation(
    inputs: &[ExitCounterfactualInput<'_>],
) -> Result<StrictVisualNetBreakEvenActivationReport> {
    let variants = ACTIVATION_R_VARIANTS
        .iter()
        .map(|&activation_r| build_variant(inputs, activation_r, ActivationEvidence::CompletedHigh))
        .collect::<Result<Vec<_>>>()?;
    let valid_trade_records = variants.first().map_or(0, |variant| variant.trades);
    Ok(StrictVisualNetBreakEvenActivationReport {
        research_version: "strict_visual_breakout_long_net_be_activation_15m_research_v1",
        definition: "strict_visual_consolidation_break_long Fixed trades; completed candle high reaches max(entry + activation_r * initial_risk, tick-rounded 8bps-per-side net break-even), then protection starts on the next candle",
        implementation_mode: "trade-isolated exit counterfactual over the frozen V6 trade list; earlier exits do not release capacity or manufacture later entries",
        future_data_usage: "post-entry candles only simulate the alternative exit; no future candle changes signal, entry, eligibility, initial risk, or another trade",
        protected_cost_bps_per_side: NET_BREAK_EVEN_COST_BPS_PER_SIDE,
        valid_trade_records,
        identity_changed_trades: 0,
        variants,
    })
}

/// 构建完成收盘确认 1R 后才激活的 V2 隔离退出报告。
pub(crate) fn build_strict_visual_completed_close_one_r_net_break_even(
    inputs: &[ExitCounterfactualInput<'_>],
) -> Result<StrictVisualNetBreakEvenActivationReport> {
    let variants = vec![build_variant(
        inputs,
        COMPLETED_CLOSE_ACTIVATION_R,
        ActivationEvidence::CompletedClose,
    )?];
    let valid_trade_records = variants.first().map_or(0, |variant| variant.trades);
    Ok(StrictVisualNetBreakEvenActivationReport {
        research_version: "strict_visual_breakout_long_completed_close_1r_net_be_15m_research_v2",
        definition: "strict_visual_consolidation_break_long Fixed trades; completed candle close reaches max(entry + 1R initial risk, tick-rounded 8bps-per-side net break-even), then protection starts on the next candle",
        implementation_mode: "trade-isolated exit counterfactual over the frozen V6 trade list; earlier exits do not release capacity or manufacture later entries",
        future_data_usage: "post-entry completed closes only simulate the alternative exit; no future candle changes signal, entry, eligibility, initial risk, or another trade",
        protected_cost_bps_per_side: NET_BREAK_EVEN_COST_BPS_PER_SIDE,
        valid_trade_records,
        identity_changed_trades: 0,
        variants,
    })
}

fn build_variant(
    inputs: &[ExitCounterfactualInput<'_>],
    activation_r: f64,
    activation_evidence: ActivationEvidence,
) -> Result<ActivationVariantReport> {
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
            if !zero_trade
                .families
                .contains(&SignalFamily::StrictVisualConsolidationBreakLong)
            {
                continue;
            }
            validate_cost_replay(zero_trade, cost_trade)?;
            records.push(build_record(
                input.symbol,
                input.candles,
                input.tick_size,
                zero_trade,
                activation_r,
                activation_evidence,
            )?);
        }
    }

    let costs = STRESS_COST_BPS_PER_SIDE
        .iter()
        .map(|&cost| cost_comparison(&records, cost))
        .collect::<Vec<_>>();
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
    let eligible = records.iter().filter(|record| record.eligible).count();
    let activated = records
        .iter()
        .filter(|record| record.activation_time_ms.is_some())
        .count();
    let exits = records
        .iter()
        .filter(|record| record.variant_exit_kind != CounterfactualExitKind::BaselineUnchanged)
        .count();
    Ok(ActivationVariantReport {
        activation_r,
        trades: records.len(),
        eligible_fixed_long_trades: eligible,
        activation_price_raised_to_cost_floor: records
            .iter()
            .filter(|record| record.activation_price_raised_to_cost_floor)
            .count(),
        activated_on_completed_high: (activation_evidence == ActivationEvidence::CompletedHigh)
            .then_some(activated),
        activated_on_completed_close: (activation_evidence == ActivationEvidence::CompletedClose)
            .then_some(activated),
        activated_effective_events_60m: effective_events_60m_at_8bps.activated_events,
        net_break_even_exits: exits,
        unchanged_exits: records.len() - exits,
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
    activation_r: f64,
    activation_evidence: ActivationEvidence,
) -> Result<CounterfactualRecord> {
    let eligible = is_eligible(trade);
    let net_break_even_stop =
        eligible.then(|| long_net_break_even_price(trade.entry_price, tick_size));
    let raw_activation_price = trade.entry_price + activation_r * trade.initial_risk;
    let activation_price = net_break_even_stop.map(|stop| raw_activation_price.max(stop));
    let simulated =
        simulate_trade_with_evidence(trade, candles, tick_size, activation_r, activation_evidence)?;
    Ok(CounterfactualRecord {
        symbol: symbol.to_owned(),
        signal_time_ms: trade.signal_time_ms,
        entry_time_ms: trade.entry_time_ms,
        baseline_exit_time_ms: trade.exit_time_ms,
        variant_exit_time_ms: simulated.exit_time_ms,
        baseline_exit_reason: trade.exit_reason,
        variant_exit_kind: simulated.kind,
        eligible,
        activation_time_ms: simulated.activation_time_ms,
        entry_price: trade.entry_price,
        initial_stop: trade.initial_stop,
        initial_risk: trade.initial_risk,
        activation_price,
        net_break_even_stop,
        activation_price_raised_to_cost_floor: net_break_even_stop
            .is_some_and(|stop| stop > raw_activation_price + FLOAT_TOLERANCE),
        baseline_exit_price: trade.exit_price,
        variant_exit_price: simulated.exit_price,
    })
}

/// 只有固定退出的严格横盘突破多单进入该变量，其余同家族交易保持原样。
fn is_eligible(trade: &Trade) -> bool {
    trade.direction == Direction::Long
        && trade.exit_policy == ExitPolicy::Fixed
        && trade
            .families
            .contains(&SignalFamily::StrictVisualConsolidationBreakLong)
        && trade.initial_risk > 0.0
}

/// 沿原交易存活区间模拟新保护；激活棒只更新状态，不允许改写自身路径。
fn simulate_trade(
    trade: &Trade,
    candles: &[Candle],
    tick_size: f64,
    activation_r: f64,
) -> Result<SimulatedExit> {
    simulate_trade_with_evidence(
        trade,
        candles,
        tick_size,
        activation_r,
        ActivationEvidence::CompletedHigh,
    )
}

/// 沿原交易存活区间读取指定完成棒证据；确认棒只更新状态，不能改写自身路径。
fn simulate_trade_with_evidence(
    trade: &Trade,
    candles: &[Candle],
    tick_size: f64,
    activation_r: f64,
    activation_evidence: ActivationEvidence,
) -> Result<SimulatedExit> {
    let baseline = baseline_exit(trade);
    if !is_eligible(trade) {
        return Ok(baseline);
    }
    let entry_index = candles
        .binary_search_by_key(&trade.entry_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow::anyhow!("找不到严格横盘入场 K 线：{}", trade.entry_time_ms))?;
    let exit_index = candles
        .binary_search_by_key(&trade.exit_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow::anyhow!("找不到严格横盘退出 K 线：{}", trade.exit_time_ms))?;
    if exit_index < entry_index {
        bail!("严格横盘退出早于入场：{}", trade.signal_time_ms);
    }

    let stop = long_net_break_even_price(trade.entry_price, tick_size);
    let activation_price = (trade.entry_price + activation_r * trade.initial_risk).max(stop);
    let mut activation_time_ms = None;
    for candle in &candles[entry_index..=exit_index] {
        let is_baseline_exit_candle = candle.timestamp_ms == trade.exit_time_ms;
        let active_from_prior_close =
            activation_time_ms.is_some_and(|activated| candle.timestamp_ms > activated);

        if active_from_prior_close {
            if is_baseline_exit_candle && trade.exit_reason == ExitReason::ReverseAtNextOpen {
                return Ok(with_activation(baseline, activation_time_ms));
            }
            if candle.open <= stop {
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
                    CounterfactualExitKind::BaselineUnchanged => {
                        with_activation(baseline, activation_time_ms)
                    }
                    CounterfactualExitKind::NetBreakEvenStop => SimulatedExit {
                        activation_time_ms,
                        exit_time_ms: candle.timestamp_ms,
                        exit_price: stop,
                        kind,
                    },
                    CounterfactualExitKind::NetBreakEvenGapOpen => {
                        unreachable!("盘中路径不会生成跳空退出")
                    }
                });
            }
        }

        if is_baseline_exit_candle {
            return Ok(with_activation(baseline, activation_time_ms));
        }
        let completed_evidence_price = match activation_evidence {
            ActivationEvidence::CompletedHigh => candle.high,
            ActivationEvidence::CompletedClose => candle.close,
        };
        if activation_time_ms.is_none() && completed_evidence_price >= activation_price {
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

/// 多单净保本必须向上取 tick，避免保护价因精度截断仍留下成本后小亏。
fn long_net_break_even_price(entry_price: f64, tick_size: f64) -> f64 {
    let cost_ratio = NET_BREAK_EVEN_COST_BPS_PER_SIDE / 10_000.0;
    round_up(
        entry_price * (1.0 + cost_ratio) / (1.0 - cost_ratio),
        tick_size,
    )
}

/// 按 TradingView 默认 OHLC 路径比较净保本与原固定退出的先后。
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

/// 行情、报告成员和交易数量必须一致，否则逐笔退出对照无效。
fn validate_input_identity(input: &ExitCounterfactualInput<'_>) -> Result<()> {
    if input.symbol != input.zero_cost.symbol || input.symbol != input.cost_adjusted.symbol {
        bail!("严格横盘净保本成员 identity 不一致：{}", input.symbol);
    }
    if input.zero_cost.trades.len() != input.cost_adjusted.trades.len() {
        bail!("{} 的零成本与成本后交易数量不一致", input.symbol);
    }
    if input.tick_size <= 0.0 || !input.tick_size.is_finite() {
        bail!("{} 的 tick size 无效", input.symbol);
    }
    Ok(())
}

/// 成本报告只能改变 R，不能改变信号、成交、风险或退出身份。
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

fn cost_comparison(records: &[CounterfactualRecord], cost_bps_per_side: f64) -> CostComparison {
    let baseline = r_metrics(records, cost_bps_per_side, false);
    let variant = r_metrics(records, cost_bps_per_side, true);
    CostComparison {
        cost_bps_per_side,
        variant_minus_baseline_net_r: variant.net_r - baseline.net_r,
        baseline,
        variant,
    }
}

fn r_metrics(records: &[CounterfactualRecord], cost_bps_per_side: f64, variant: bool) -> RMetrics {
    let mut outcomes = records
        .iter()
        .map(|record| {
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
    let wins = outcomes
        .iter()
        .filter(|outcome| outcome.3 > FLOAT_TOLERANCE)
        .count();
    let losses = outcomes
        .iter()
        .filter(|outcome| outcome.3 < -FLOAT_TOLERANCE)
        .count();
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

fn record_net_r(record: &CounterfactualRecord, variant: bool, cost_bps_per_side: f64) -> f64 {
    net_r(
        Direction::Long,
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

fn contribution_comparisons(
    records: &[CounterfactualRecord],
    key: impl Fn(&CounterfactualRecord) -> String,
) -> Vec<ContributionComparison> {
    let mut grouped = BTreeMap::<String, Vec<&CounterfactualRecord>>::new();
    for record in records {
        grouped.entry(key(record)).or_default().push(record);
    }
    grouped
        .into_iter()
        .map(|(key, group)| {
            let baseline = group
                .iter()
                .map(|record| record_net_r(record, false, 8.0))
                .sum::<f64>();
            let variant = group
                .iter()
                .map(|record| record_net_r(record, true, 8.0))
                .sum::<f64>();
            ContributionComparison {
                key,
                trades: group.len(),
                activations: group
                    .iter()
                    .filter(|record| record.activation_time_ms.is_some())
                    .count(),
                baseline_net_r: baseline,
                variant_net_r: variant,
                variant_minus_baseline_net_r: variant - baseline,
            }
        })
        .collect()
}

fn event_comparison(records: &[CounterfactualRecord]) -> EventComparison {
    let mut ordered = (0..records.len()).collect::<Vec<_>>();
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
        raw_trades: records.len(),
        events: events.len(),
        ..EventComparison::default()
    };
    for event in events {
        let symbols = event
            .iter()
            .map(|&index| records[index].symbol.as_str())
            .collect::<BTreeSet<_>>();
        let baseline = event
            .iter()
            .map(|&index| record_net_r(&records[index], false, 8.0))
            .sum::<f64>();
        let variant = event
            .iter()
            .map(|&index| record_net_r(&records[index], true, 8.0))
            .sum::<f64>();
        if event
            .iter()
            .any(|&index| records[index].activation_time_ms.is_some())
        {
            comparison.activated_events += 1;
        }
        if variant > baseline + FLOAT_TOLERANCE {
            comparison.improved_events += 1;
        } else if variant + FLOAT_TOLERANCE < baseline {
            comparison.worsened_events += 1;
        } else {
            comparison.unchanged_events += 1;
        }
        comparison.largest_event_trade_count =
            comparison.largest_event_trade_count.max(event.len());
        comparison.largest_event_symbol_count =
            comparison.largest_event_symbol_count.max(symbols.len());
        comparison.baseline_net_r += baseline;
        comparison.variant_net_r += variant;
    }
    comparison.variant_minus_baseline_net_r = comparison.variant_net_r - comparison.baseline_net_r;
    comparison
}

fn metric_gate(
    records: &[CounterfactualRecord],
    costs: &[CostComparison],
    per_symbol: &[ContributionComparison],
    per_month: &[ContributionComparison],
    events: &EventComparison,
) -> MetricGate {
    let cost_8 = costs
        .iter()
        .find(|cost| nearly_equal(cost.cost_bps_per_side, 8.0))
        .expect("8bps cost exists");
    let improved_trades = records
        .iter()
        .filter(|record| {
            record_net_r(record, true, 8.0) > record_net_r(record, false, 8.0) + FLOAT_TOLERANCE
        })
        .count();
    let improved_symbols = per_symbol
        .iter()
        .filter(|item| item.variant_minus_baseline_net_r > FLOAT_TOLERANCE)
        .count();
    let improved_months = per_month
        .iter()
        .filter(|item| item.variant_minus_baseline_net_r > FLOAT_TOLERANCE)
        .count();
    let activated = records
        .iter()
        .filter(|record| record.activation_time_ms.is_some())
        .count();
    let pf_improved = strictly_higher_pf(&cost_8.variant, &cost_8.baseline);
    let variant_pf_above_one = cost_8.variant.profit_factor_r_is_infinite
        || cost_8
            .variant
            .profit_factor_r
            .is_some_and(|value| value > 1.0);
    let checks = [
        cost_8.variant.net_r > cost_8.baseline.net_r + FLOAT_TOLERANCE,
        cost_8.variant.average_net_r > cost_8.baseline.average_net_r + FLOAT_TOLERANCE,
        pf_improved,
        cost_8.variant.net_r > 0.0,
        variant_pf_above_one,
        activated >= 30,
        events.activated_events >= 20,
        improved_trades >= 3,
        improved_symbols >= 3,
        improved_months >= 3,
        events.improved_events >= 3,
    ];
    MetricGate {
        net_r_improved_at_8bps: checks[0],
        average_r_improved_at_8bps: checks[1],
        profit_factor_improved_at_8bps: checks[2],
        variant_net_r_positive_at_8bps: checks[3],
        variant_profit_factor_above_one_at_8bps: checks[4],
        activated_trades_at_least_30: checks[5],
        activated_events_at_least_20: checks[6],
        improved_trades_at_least_3: checks[7],
        improved_symbols_at_least_3: checks[8],
        improved_months_at_least_3: checks[9],
        improved_events_at_least_3: checks[10],
        trade_identity_preserved: true,
        metric_gate_passed: checks.into_iter().all(|passed| passed),
    }
}

fn strictly_higher_pf(candidate: &RMetrics, baseline: &RMetrics) -> bool {
    if candidate.profit_factor_r_is_infinite {
        return !baseline.profit_factor_r_is_infinite;
    }
    match (candidate.profit_factor_r, baseline.profit_factor_r) {
        (Some(candidate), Some(baseline)) => candidate > baseline + FLOAT_TOLERANCE,
        (Some(_), None) => false,
        (None, Some(_)) => false,
        (None, None) => false,
    }
}

fn shanghai_month(timestamp_ms: i64) -> String {
    let Some(timestamp) = Utc
        .timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
    else {
        return "invalid".to_owned();
    };
    format!("{:04}-{:02}", timestamp.year(), timestamp.month())
}

fn nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= FLOAT_TOLERANCE.max(left.abs().max(right.abs()) * 1e-10)
}

#[cfg(test)]
#[path = "strict_visual_exit_counterfactual_tests.rs"]
mod tests;
