use super::ema_short_exit_counterfactual::ExitCounterfactualInput;
use super::research_runtime::CandidateLedger;
use anyhow::{bail, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    Candle, Direction, ExitPolicy, ExitReason, SignalFamily, Trade,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const ACTIVATION_R: f64 = 1.0;
const STRESS_COST_BPS_PER_SIDE: [f64; 4] = [0.0, 8.0, 10.0, 12.0];
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const FLOAT_TOLERANCE: f64 = 1e-8;
const STRICT_VISUAL_FAMILY: &str = "strict_visual_consolidation_break_long";

/// V3 在冻结 V6 交易清单上隔离验证“1R 后收盘跌破横盘上沿”的退出语义。
#[derive(Debug, Serialize)]
pub(crate) struct StrictVisualStructuralFailureReport {
    /// 独立 Research 身份，不能与 V6 入场版本或 V2 净保本版本混用。
    research_version: &'static str,
    /// 完成收盘激活、结构失效和下一棒开盘退出的因果定义。
    definition: &'static str,
    /// 提前退出不会释放容量或制造后续交易。
    implementation_mode: &'static str,
    /// 后续 K 线只参与退出，不反向修改信号与冻结风险。
    future_data_usage: &'static str,
    /// 严格横盘家族全部交易数，包含保持原样的非固定退出。
    valid_trade_records: usize,
    /// 零成本与成本路径的交易身份漂移数；验证失败会直接终止，因此正常为零。
    identity_changed_trades: usize,
    /// 可以应用 V3 的 Fixed 多单数。
    eligible_fixed_long_trades: usize,
    /// 已完成收盘达到 1R 的交易数。
    activated_on_completed_close: usize,
    /// 激活交易按 60 分钟链式规则聚类后的事件数。
    activated_effective_events_60m: usize,
    /// 激活后出现完成收盘跌破冻结上沿的交易数。
    range_upper_failure_closes: usize,
    /// 结构失效实际改变原退出时间或价格的交易数。
    range_upper_failure_next_open_exits: usize,
    /// 结构退出实际改变交易的 60 分钟事件数。
    changed_exit_effective_events_60m: usize,
    /// V3 未改变退出结果的交易数。
    unchanged_exits: usize,
    /// 零成本和多档成本压力下的同身份指标。
    costs: Vec<CostComparison>,
    /// 单币 8 bps 贡献，用于识别集中度。
    per_symbol_8bps: Vec<ContributionComparison>,
    /// 上海自然月 8 bps 贡献。
    per_shanghai_month_8bps: Vec<ContributionComparison>,
    /// 60 分钟事件级 8 bps 贡献。
    effective_events_60m_at_8bps: EventComparison,
    /// 预注册 L2 机械门禁；通过也不代表可以生产晋级。
    metric_gate: MetricGate,
    /// 每笔冻结身份、结构证据和反事实退出。
    records: Vec<StructuralFailureRecord>,
}

/// 固定成本压力下的 V6 与 V3 指标对照。
#[derive(Debug, Serialize)]
struct CostComparison {
    /// 单边手续费与滑点合计，单位 bps。
    cost_bps_per_side: f64,
    /// 原 V6 固定退出指标。
    baseline: RMetrics,
    /// 只改变结构失效退出后的指标。
    variant: RMetrics,
    /// V3 净 R 减去 V6 净 R。
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
    /// 毛盈利 R 除以毛亏损 R；无亏损时为 `None`。
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
    /// 当前分组完成收盘激活数。
    activations: usize,
    /// 当前分组结构失效数。
    failure_closes: usize,
    /// 原 V6 成本后净 R。
    baseline_net_r: f64,
    /// V3 成本后净 R。
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
    /// 至少有一笔完成收盘激活的事件数。
    activated_events: usize,
    /// 至少有一笔实际改变退出的事件数。
    changed_exit_events: usize,
    /// V3 净 R 高于 V6 的事件数。
    improved_events: usize,
    /// V3 净 R 低于 V6 的事件数。
    worsened_events: usize,
    /// V3 与 V6 近似相同的事件数。
    unchanged_events: usize,
    /// 单个事件包含的最大交易数。
    largest_event_trade_count: usize,
    /// 单个事件覆盖的最大币种数。
    largest_event_symbol_count: usize,
    /// 所有事件的 V6 成本后净 R。
    baseline_net_r: f64,
    /// 所有事件的 V3 成本后净 R。
    variant_net_r: f64,
    /// 事件级汇总的净 R 改善值。
    variant_minus_baseline_net_r: f64,
}

/// L2 门禁只判断是否值得进入新窗口验证，不产生运行态切换。
#[derive(Debug, Serialize)]
struct MetricGate {
    /// 8 bps 下 V3 净 R 是否高于 V6。
    net_r_improved_at_8bps: bool,
    /// 8 bps 下 V3 平均 R 是否高于 V6。
    average_r_improved_at_8bps: bool,
    /// 8 bps 下 V3 PF 是否高于 V6。
    profit_factor_improved_at_8bps: bool,
    /// 8 bps 下 V3 净 R 是否为正。
    variant_net_r_positive_at_8bps: bool,
    /// 8 bps 下 V3 PF 是否大于一。
    variant_profit_factor_above_one_at_8bps: bool,
    /// 实际改变退出是否达到预注册的 20 笔。
    changed_exits_at_least_20: bool,
    /// 改变退出的事件是否达到预注册的 15 个。
    changed_exit_events_at_least_15: bool,
    /// 正向改善是否至少分布在 3 笔交易。
    improved_trades_at_least_3: bool,
    /// 正向改善是否至少分布在 3 个币。
    improved_symbols_at_least_3: bool,
    /// 正向改善是否至少分布在 3 个上海月份。
    improved_months_at_least_3: bool,
    /// 正向改善是否至少分布在 3 个事件簇。
    improved_events_at_least_3: bool,
    /// `true` 表示基线与反事实只允许退出字段不同。
    trade_identity_preserved: bool,
    /// 全部 L2 预注册检查的合取结果。
    metric_gate_passed: bool,
}

/// V3 相对于冻结 V6 退出的逐笔证据。
#[derive(Debug, Serialize)]
struct StructuralFailureRecord {
    /// OKX 永续合约标识。
    symbol: String,
    /// 原始信号时间，Unix 毫秒。
    signal_time_ms: i64,
    /// 实际开仓时间，Unix 毫秒。
    entry_time_ms: i64,
    /// 原 V6 退出时间，Unix 毫秒。
    baseline_exit_time_ms: i64,
    /// V3 退出时间，Unix 毫秒。
    variant_exit_time_ms: i64,
    /// 原 V6 退出原因。
    baseline_exit_reason: ExitReason,
    /// V3 是否保持原样或在结构失效后的下一棒开盘退出。
    variant_exit_kind: StructuralExitKind,
    /// `true` 表示交易属于 Fixed 严格横盘突破多单。
    eligible: bool,
    /// 完成收盘达到 1R 的时间；`None` 表示原交易结束前没有激活。
    activation_time_ms: Option<i64>,
    /// 激活后首次完成收盘跌破冻结上沿的时间。
    failure_close_time_ms: Option<i64>,
    /// 失效棒完成收盘价；没有失效时为 `None`。
    failure_close_price: Option<f64>,
    /// 实际开仓价。
    entry_price: f64,
    /// 冻结初始止损价。
    initial_stop: f64,
    /// 开仓价到初始止损的固定价格风险。
    initial_risk: f64,
    /// 信号时冻结的横盘上沿；非 Fixed 交易也保留可审计值。
    frozen_range_upper: Option<f64>,
    /// 完成收盘必须达到的 1R 激活价。
    activation_price: Option<f64>,
    /// 原 V6 退出价。
    baseline_exit_price: f64,
    /// V3 下一棒真实开盘退出价，或保持原 V6 退出价。
    variant_exit_price: f64,
}

/// 结构信号只在下一棒开盘执行；不会虚构成交在上沿或失效收盘价。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum StructuralExitKind {
    BaselineUnchanged,
    RangeUpperFailureNextOpen,
}

/// 单笔模拟状态；激活和失效完成棒都只能影响更晚 K 线。
#[derive(Debug, Clone, Copy, PartialEq)]
struct SimulatedExit {
    /// 完成收盘达到 1R 的时间，Unix 毫秒。
    activation_time_ms: Option<i64>,
    /// 完成收盘跌破上沿的时间，Unix 毫秒。
    failure_close_time_ms: Option<i64>,
    /// 失效棒完成收盘价。
    failure_close_price: Option<f64>,
    /// 变体退出时间，Unix 毫秒。
    exit_time_ms: i64,
    /// 变体退出价。
    exit_price: f64,
    /// 变体退出类别。
    kind: StructuralExitKind,
}

/// 构建完成 1R 后按冻结横盘上沿收盘失效的 V3 隔离退出报告。
pub(crate) fn build_strict_visual_one_r_range_upper_failure_exit(
    inputs: &[ExitCounterfactualInput<'_>],
    candidate_ledger: &CandidateLedger,
) -> Result<StrictVisualStructuralFailureReport> {
    let frozen_uppers = frozen_range_uppers(candidate_ledger)?;
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
                zero_trade,
                frozen_uppers.get(&(input.symbol.to_owned(), zero_trade.signal_time_ms)),
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
    let eligible_fixed_long_trades = records.iter().filter(|record| record.eligible).count();
    let activated_on_completed_close = records
        .iter()
        .filter(|record| record.activation_time_ms.is_some())
        .count();
    let range_upper_failure_closes = records
        .iter()
        .filter(|record| record.failure_close_time_ms.is_some())
        .count();
    let range_upper_failure_next_open_exits = records
        .iter()
        .filter(|record| record.variant_exit_kind != StructuralExitKind::BaselineUnchanged)
        .count();
    Ok(StrictVisualStructuralFailureReport {
        research_version:
            "strict_visual_breakout_long_one_r_range_upper_close_failure_exit_15m_research_v3",
        definition: "strict_visual_consolidation_break_long Fixed trades; a completed close reaches entry + 1R, then a later completed close below the signal-time frozen range upper schedules a full exit at the next actual open",
        implementation_mode: "trade-isolated exit counterfactual over the frozen V6 trade list; earlier exits do not release capacity or manufacture later entries",
        future_data_usage: "post-entry completed closes only simulate the alternative exit; no future candle changes signal, entry, frozen range upper, initial risk, target, or another trade",
        valid_trade_records: records.len(),
        identity_changed_trades: 0,
        eligible_fixed_long_trades,
        activated_on_completed_close,
        activated_effective_events_60m: event_count_with(&records, |record| {
            record.activation_time_ms.is_some()
        }),
        range_upper_failure_closes,
        range_upper_failure_next_open_exits,
        changed_exit_effective_events_60m: effective_events_60m_at_8bps.changed_exit_events,
        unchanged_exits: records.len() - range_upper_failure_next_open_exits,
        costs,
        per_symbol_8bps,
        per_shanghai_month_8bps,
        effective_events_60m_at_8bps,
        metric_gate,
        records,
    })
}

/// 冻结上沿只能来自候选账本的信号时特征，不能从后续 K 线重新计算。
fn frozen_range_uppers(candidate_ledger: &CandidateLedger) -> Result<BTreeMap<(String, i64), f64>> {
    let mut uppers = BTreeMap::new();
    for candidate in &candidate_ledger.candidates {
        if !candidate.families.contains(&STRICT_VISUAL_FAMILY) {
            continue;
        }
        let Some(range_upper) = candidate.time_visible_features.breakout_line else {
            if candidate.frozen_risk.exit_policy == ExitPolicy::Fixed {
                bail!(
                    "{} 在信号 {} 缺少冻结横盘上沿",
                    candidate.symbol,
                    candidate.signal_time_ms
                );
            }
            continue;
        };
        if !range_upper.is_finite() || range_upper <= 0.0 {
            bail!(
                "{} 在信号 {} 的冻结横盘上沿无效",
                candidate.symbol,
                candidate.signal_time_ms
            );
        }
        let key = (candidate.symbol.clone(), candidate.signal_time_ms);
        if uppers.insert(key, range_upper).is_some() {
            bail!(
                "{} 在信号 {} 存在重复严格横盘候选",
                candidate.symbol,
                candidate.signal_time_ms
            );
        }
    }
    Ok(uppers)
}

/// 把冻结交易与信号时上沿合并成一条可独立审计的反事实记录。
fn build_record(
    symbol: &str,
    candles: &[Candle],
    trade: &Trade,
    frozen_range_upper: Option<&f64>,
) -> Result<StructuralFailureRecord> {
    let eligible = is_eligible(trade);
    let range_upper = frozen_range_upper.copied();
    if eligible && range_upper.is_none() {
        bail!(
            "{} 在信号 {} 找不到冻结横盘上沿",
            symbol,
            trade.signal_time_ms
        );
    }
    let simulated = if let Some(range_upper) = range_upper.filter(|_| eligible) {
        simulate_trade(trade, candles, range_upper)?
    } else {
        baseline_exit(trade)
    };
    Ok(StructuralFailureRecord {
        symbol: symbol.to_owned(),
        signal_time_ms: trade.signal_time_ms,
        entry_time_ms: trade.entry_time_ms,
        baseline_exit_time_ms: trade.exit_time_ms,
        variant_exit_time_ms: simulated.exit_time_ms,
        baseline_exit_reason: trade.exit_reason,
        variant_exit_kind: simulated.kind,
        eligible,
        activation_time_ms: simulated.activation_time_ms,
        failure_close_time_ms: simulated.failure_close_time_ms,
        failure_close_price: simulated.failure_close_price,
        entry_price: trade.entry_price,
        initial_stop: trade.initial_stop,
        initial_risk: trade.initial_risk,
        frozen_range_upper: range_upper,
        activation_price: eligible.then_some(trade.entry_price + ACTIVATION_R * trade.initial_risk),
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

/// 激活和结构失效都在完成收盘时冻结，任何退出只能发生在更晚 K 线。
fn simulate_trade(trade: &Trade, candles: &[Candle], range_upper: f64) -> Result<SimulatedExit> {
    let baseline = baseline_exit(trade);
    let entry_index = candles
        .binary_search_by_key(&trade.entry_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow::anyhow!("找不到严格横盘入场 K 线：{}", trade.entry_time_ms))?;
    let exit_index = candles
        .binary_search_by_key(&trade.exit_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow::anyhow!("找不到严格横盘退出 K 线：{}", trade.exit_time_ms))?;
    if exit_index < entry_index {
        bail!("严格横盘退出早于入场：{}", trade.signal_time_ms);
    }

    let activation_price = trade.entry_price + ACTIVATION_R * trade.initial_risk;
    let mut activation_time_ms = None;
    let mut failure_close = None::<(i64, f64)>;
    for candle in &candles[entry_index..=exit_index] {
        let is_baseline_exit_candle = candle.timestamp_ms == trade.exit_time_ms;

        // 失效只在上一棒完成后成立；下一棒开盘先于其盘中止损或目标路径。
        if failure_close.is_some_and(|(time, _)| candle.timestamp_ms > time) {
            if is_baseline_exit_candle && nearly_equal(trade.exit_price, candle.open) {
                return Ok(with_state(baseline, activation_time_ms, failure_close));
            }
            return Ok(SimulatedExit {
                activation_time_ms,
                failure_close_time_ms: failure_close.map(|value| value.0),
                failure_close_price: failure_close.map(|value| value.1),
                exit_time_ms: candle.timestamp_ms,
                exit_price: candle.open,
                kind: StructuralExitKind::RangeUpperFailureNextOpen,
            });
        }

        // 原止损或目标若在本棒盘中先成交，收盘后的结构判断已经失去持仓对象。
        if is_baseline_exit_candle {
            return Ok(with_state(baseline, activation_time_ms, failure_close));
        }

        let active_from_prior_close =
            activation_time_ms.is_some_and(|time| candle.timestamp_ms > time);
        if active_from_prior_close && candle.close < range_upper {
            failure_close = Some((candle.timestamp_ms, candle.close));
            continue;
        }
        if activation_time_ms.is_none() && candle.close >= activation_price {
            activation_time_ms = Some(candle.timestamp_ms);
        }
    }
    Ok(with_state(baseline, activation_time_ms, failure_close))
}

/// 未满足 V3 或未提前退出时完整复用原 V6 成交，不重新推导原 broker 路径。
fn baseline_exit(trade: &Trade) -> SimulatedExit {
    SimulatedExit {
        activation_time_ms: None,
        failure_close_time_ms: None,
        failure_close_price: None,
        exit_time_ms: trade.exit_time_ms,
        exit_price: trade.exit_price,
        kind: StructuralExitKind::BaselineUnchanged,
    }
}

/// 在保持原退出身份时附带已经发生的激活与失效证据。
fn with_state(
    mut exit: SimulatedExit,
    activation_time_ms: Option<i64>,
    failure_close: Option<(i64, f64)>,
) -> SimulatedExit {
    exit.activation_time_ms = activation_time_ms;
    exit.failure_close_time_ms = failure_close.map(|value| value.0);
    exit.failure_close_price = failure_close.map(|value| value.1);
    exit
}

/// 行情、报告成员和交易数量必须一致，否则逐笔退出对照无效。
fn validate_input_identity(input: &ExitCounterfactualInput<'_>) -> Result<()> {
    if input.symbol != input.zero_cost.symbol || input.symbol != input.cost_adjusted.symbol {
        bail!("严格横盘结构退出成员 identity 不一致：{}", input.symbol);
    }
    if input.zero_cost.trades.len() != input.cost_adjusted.trades.len() {
        bail!("{} 的零成本与成本后交易数量不一致", input.symbol);
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

/// 用成交价重建既有 8 bps 净 R，防止错误成本口径进入 V3 对照。
fn validate_cost_replay(zero: &Trade, cost: &Trade) -> Result<()> {
    let expected = net_r(zero.entry_price, zero.exit_price, zero.initial_risk, 8.0);
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

/// 在同一成本档位上生成 V6 与 V3 的成对指标。
fn cost_comparison(records: &[StructuralFailureRecord], cost_bps_per_side: f64) -> CostComparison {
    let baseline = r_metrics(records, cost_bps_per_side, false);
    let variant = r_metrics(records, cost_bps_per_side, true);
    CostComparison {
        cost_bps_per_side,
        variant_minus_baseline_net_r: variant.net_r - baseline.net_r,
        baseline,
        variant,
    }
}

/// 按实际退出时序统计固定 R 指标与闭仓权益回撤。
fn r_metrics(
    records: &[StructuralFailureRecord],
    cost_bps_per_side: f64,
    variant: bool,
) -> RMetrics {
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

/// 读取单笔原退出或结构退出在指定成本下的净 R。
fn record_net_r(record: &StructuralFailureRecord, variant: bool, cost_bps_per_side: f64) -> f64 {
    net_r(
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

/// 多单净 R 使用冻结初始风险，并对开仓和离场两侧分别计入成本。
fn net_r(entry_price: f64, exit_price: f64, initial_risk: f64, cost_bps_per_side: f64) -> f64 {
    if initial_risk <= 0.0 {
        return 0.0;
    }
    let gross = exit_price - entry_price;
    let costs = (entry_price + exit_price) * cost_bps_per_side / 10_000.0;
    (gross - costs) / initial_risk
}

/// 按币种或上海月份汇总改善，检查边际是否过度集中。
fn contribution_comparisons(
    records: &[StructuralFailureRecord],
    key: impl Fn(&StructuralFailureRecord) -> String,
) -> Vec<ContributionComparison> {
    let mut grouped = BTreeMap::<String, Vec<&StructuralFailureRecord>>::new();
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
                failure_closes: group
                    .iter()
                    .filter(|record| record.failure_close_time_ms.is_some())
                    .count(),
                baseline_net_r: baseline,
                variant_net_r: variant,
                variant_minus_baseline_net_r: variant - baseline,
            }
        })
        .collect()
}

/// 使用预注册的链式 60 分钟信号窗口构造有效市场事件。
fn event_groups(records: &[StructuralFailureRecord]) -> Vec<Vec<usize>> {
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
    events
}

/// 统计至少包含一笔指定结构状态的有效事件数量。
fn event_count_with(
    records: &[StructuralFailureRecord],
    predicate: impl Fn(&StructuralFailureRecord) -> bool,
) -> usize {
    event_groups(records)
        .iter()
        .filter(|event| event.iter().any(|&index| predicate(&records[index])))
        .count()
}

/// 在事件级汇总 V6/V3 净 R，避免把同步市场共振当成独立样本。
fn event_comparison(records: &[StructuralFailureRecord]) -> EventComparison {
    let events = event_groups(records);
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
        if event
            .iter()
            .any(|&index| records[index].variant_exit_kind != StructuralExitKind::BaselineUnchanged)
        {
            comparison.changed_exit_events += 1;
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

/// 严格执行预注册 L2 联合门禁，任一失败都禁止进入 L3。
fn metric_gate(
    records: &[StructuralFailureRecord],
    costs: &[CostComparison],
    per_symbol: &[ContributionComparison],
    per_month: &[ContributionComparison],
    events: &EventComparison,
) -> MetricGate {
    let cost_8 = costs
        .iter()
        .find(|cost| nearly_equal(cost.cost_bps_per_side, 8.0))
        .expect("8bps cost exists");
    let changed_exits = records
        .iter()
        .filter(|record| record.variant_exit_kind != StructuralExitKind::BaselineUnchanged)
        .count();
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
        changed_exits >= 20,
        events.changed_exit_events >= 15,
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
        changed_exits_at_least_20: checks[5],
        changed_exit_events_at_least_15: checks[6],
        improved_trades_at_least_3: checks[7],
        improved_symbols_at_least_3: checks[8],
        improved_months_at_least_3: checks[9],
        improved_events_at_least_3: checks[10],
        trade_identity_preserved: true,
        metric_gate_passed: checks.into_iter().all(|passed| passed),
    }
}

/// 比较含无穷 PF 的边界，避免没有亏损时被 `None` 误判为较差。
fn strictly_higher_pf(candidate: &RMetrics, baseline: &RMetrics) -> bool {
    if candidate.profit_factor_r_is_infinite {
        return !baseline.profit_factor_r_is_infinite;
    }
    match (candidate.profit_factor_r, baseline.profit_factor_r) {
        (Some(candidate), Some(baseline)) => candidate > baseline + FLOAT_TOLERANCE,
        _ => false,
    }
}

/// 用固定 UTC+8 偏移生成自然月标签，避免依赖运行机器时区。
fn shanghai_month(timestamp_ms: i64) -> String {
    let shifted = timestamp_ms + SHANGHAI_OFFSET_MS;
    let time = Utc
        .timestamp_millis_opt(shifted)
        .single()
        .expect("valid unix milliseconds");
    format!("{:04}-{:02}", time.year(), time.month())
}

/// 浮点身份校验使用相对容差，同时为接近零的价格保留绝对下限。
fn nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= FLOAT_TOLERANCE * left.abs().max(right.abs()).max(1.0)
}

#[cfg(test)]
#[path = "strict_visual_structural_failure_counterfactual_tests.rs"]
mod tests;
