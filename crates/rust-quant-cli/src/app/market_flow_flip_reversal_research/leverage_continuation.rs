use super::metrics::{load_metrics, ContinuationEvidence, MetricsStore};
use super::{
    atr_at, concentration_without_top_three, effective_event_count, load_symbol_candles, metrics,
    FlowFlipMetrics, FlowFlipResearchArgs, FlowFlipTrade, MetricsAudit, UniverseSchedule,
};
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const RETURN_6H_BARS: usize = 24;
const RETURN_24H_BARS: usize = 96;
const PRICE_COVERAGE_MIN_RATIO: f64 = 0.80;
const PRICE_CANDIDATES_PER_DECISION: usize = 3;
const STOP_ATR_MULTIPLIER: f64 = 2.0;
const MIN_RISK_PCT: f64 = 0.75;
const MAX_RISK_PCT: f64 = 5.0;
const TARGET_R: f64 = 2.0;
const MAX_HOLDING_BARS: usize = 96;
const MAX_CONCURRENT: usize = 4;
const MAX_SAME_DIRECTION: usize = 3;
const COST_RATE_PER_SIDE: f64 = 0.0008;
const ADVERSE_FUNDING_PER_8H: f64 = 0.0001;
const RULE_VERSION: &str = "price_impulse_6h_24h_oi4h_taker1h_alignment_8h_v1";
const DOWNSIDE_RULE_VERSION: &str = "negative_impulse_6h_24h_oi4h_taker1h_alignment_4h_v1";

/// 杠杆资金流延续研究的因果漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeverageContinuationStageCounts {
    pub decision_points: usize,
    pub price_coverage_blocked: usize,
    pub aligned_price_observations: usize,
    pub top_price_candidates: usize,
    pub metrics_pass: usize,
    pub selected: usize,
    pub risk_blocked: usize,
    pub capacity_blocked: usize,
    pub incomplete_outcomes: usize,
}

/// 冻结 V1 的数据审计、漏斗、交易级统计和稳定性结果。
#[derive(Debug, Clone, PartialEq)]
pub struct LeverageContinuationReport {
    pub rule_version: String,
    pub universe_version: String,
    pub symbols: usize,
    pub metrics_audit: MetricsAudit,
    pub stages: LeverageContinuationStageCounts,
    pub trades: Vec<FlowFlipTrade>,
    pub effective_events: usize,
    pub gross_zero_cost: FlowFlipMetrics,
    pub overall: FlowFlipMetrics,
    pub double_cost: FlowFlipMetrics,
    pub long: FlowFlipMetrics,
    pub short: FlowFlipMetrics,
    pub monthly: Vec<(i64, FlowFlipMetrics)>,
    pub positive_months: usize,
    pub top_three_positive_symbols: Vec<String>,
    pub net_r_without_top_three_symbols: f64,
    pub exit_reasons: BTreeMap<String, usize>,
    pub discovery_gate_passed: bool,
}

/// 信号时点冻结的价格冲量候选。
#[derive(Debug, Clone, PartialEq)]
struct PriceCandidate {
    symbol: String,
    decision_index: usize,
    decision_ts: i64,
    return_6h: f64,
    long: bool,
}

/// 候选结算结果明确区分风险阻塞和 outcome 缺口。
enum Settlement {
    Trade(FlowFlipTrade),
    RiskBlocked,
    Incomplete,
}

/// 保留已封存双向 V1，并为独立窗口暴露 short-only 下跌冲量规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImpulseRule {
    BothV1,
    DownsideV1,
}

impl ImpulseRule {
    /// 返回可审计的规则版本。
    fn version(self) -> &'static str {
        match self {
            Self::BothV1 => RULE_VERSION,
            Self::DownsideV1 => DOWNSIDE_RULE_VERSION,
        }
    }

    /// 返回冻结决策间隔；下跌专用版以 4h 频率独立验证。
    fn decision_interval_ms(self) -> i64 {
        match self {
            Self::BothV1 => MS_8H,
            Self::DownsideV1 => MS_8H / 2,
        }
    }

    /// 在数据覆盖完成后应用对应方向条件。
    fn accepts_returns(self, return_6h: f64, return_24h: f64) -> bool {
        match self {
            Self::BothV1 => return_6h.is_sign_positive() == return_24h.is_sign_positive(),
            Self::DownsideV1 => return_6h < 0.0 && return_24h < 0.0,
        }
    }
}

/// 运行冻结 V1：先形成价格候选日，再加载最小 Binance metrics 文件集并严格回放。
pub async fn run_leverage_continuation_research(
    args: &FlowFlipResearchArgs,
    database_url: &str,
) -> Result<LeverageContinuationReport> {
    run_research(args, database_url, ImpulseRule::BothV1).await
}

/// 运行独立窗口的 short-only 杠杆下跌冲量延续 V1。
pub async fn run_leverage_downside_continuation_research(
    args: &FlowFlipResearchArgs,
    database_url: &str,
) -> Result<LeverageContinuationReport> {
    run_research(args, database_url, ImpulseRule::DownsideV1).await
}

/// 复用原始数据与结算口径，不共享候选方向或决策频率。
async fn run_research(
    args: &FlowFlipResearchArgs,
    database_url: &str,
    rule: ImpulseRule,
) -> Result<LeverageContinuationReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode leverage-continuation universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule
        .windows
        .first()
        .context("missing first leverage-continuation window")?;
    let last = schedule
        .windows
        .last()
        .context("missing last leverage-continuation window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for leverage-continuation research")?;
    let mut candles_by_symbol = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        candles_by_symbol.insert(
            symbol.clone(),
            load_symbol_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(2 * DAY_MS),
                last.to_ms.saturating_add(DAY_MS),
            )
            .await?,
        );
    }
    let (candidates_by_time, candidate_times, mut stages) =
        build_price_candidates(&schedule, &candles_by_symbol, rule);
    let (metrics_store, metrics_audit) = load_metrics(args, &schedule, &candidate_times).await?;
    let mut trades = Vec::new();
    for candidates in candidates_by_time.into_values() {
        let Some((candidate, evidence)) =
            select_metrics_candidate(&candidates, &metrics_store, &mut stages.metrics_pass)
        else {
            continue;
        };
        stages.selected += 1;
        let candles = candles_by_symbol
            .get(&candidate.symbol)
            .with_context(|| format!("missing candles for {}", candidate.symbol))?;
        match settle_candidate(&candidate, candles, evidence) {
            Settlement::Trade(trade) => {
                if capacity_blocked(&trade, &trades) {
                    stages.capacity_blocked += 1;
                } else {
                    trades.push(trade);
                }
            }
            Settlement::RiskBlocked => stages.risk_blocked += 1,
            Settlement::Incomplete => stages.incomplete_outcomes += 1,
        }
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    Ok(build_report(
        &schedule,
        candles_by_symbol.len(),
        metrics_audit,
        stages,
        trades,
        rule,
    ))
}

/// 在规则指定的固定决策点按 6h/24h 冲量选出横截面前三名。
fn build_price_candidates(
    schedule: &UniverseSchedule,
    candles_by_symbol: &BTreeMap<String, Vec<CandleItem>>,
    rule: ImpulseRule,
) -> (
    BTreeMap<i64, Vec<PriceCandidate>>,
    BTreeMap<String, Vec<i64>>,
    LeverageContinuationStageCounts,
) {
    let mut grouped = BTreeMap::<i64, Vec<PriceCandidate>>::new();
    let mut coverage_by_time = BTreeMap::<i64, usize>::new();
    for (symbol, candles) in candles_by_symbol {
        for index in RETURN_24H_BARS..candles.len().saturating_sub(1) {
            let decision_ts = candles[index].ts.saturating_add(MS_15M);
            if decision_ts.rem_euclid(rule.decision_interval_ms()) != 0
                || !schedule
                    .window_at(decision_ts)
                    .is_some_and(|window| window.members.contains(symbol))
                || candles[index].ts - candles[index - RETURN_24H_BARS].ts
                    != RETURN_24H_BARS as i64 * MS_15M
                || candles[index].ts - candles[index - RETURN_6H_BARS].ts
                    != RETURN_6H_BARS as i64 * MS_15M
            {
                continue;
            }
            let return_6h = candles[index].c / candles[index - RETURN_6H_BARS].c - 1.0;
            let return_24h = candles[index].c / candles[index - RETURN_24H_BARS].c - 1.0;
            if !return_6h.is_finite()
                || !return_24h.is_finite()
                || return_6h == 0.0
                || return_24h == 0.0
            {
                continue;
            }
            *coverage_by_time.entry(decision_ts).or_default() += 1;
            if !rule.accepts_returns(return_6h, return_24h) {
                continue;
            }
            grouped
                .entry(decision_ts)
                .or_default()
                .push(PriceCandidate {
                    symbol: symbol.clone(),
                    decision_index: index,
                    decision_ts,
                    return_6h,
                    long: return_6h > 0.0,
                });
        }
    }
    let mut selected_by_time = BTreeMap::<i64, Vec<PriceCandidate>>::new();
    let mut candidate_times = BTreeMap::<String, Vec<i64>>::new();
    let mut stages = LeverageContinuationStageCounts::default();
    for (decision_ts, coverage) in coverage_by_time {
        let mut values = grouped.remove(&decision_ts).unwrap_or_default();
        stages.decision_points += 1;
        let Some(window) = schedule.window_at(decision_ts) else {
            continue;
        };
        let minimum = (window.members.len() as f64 * PRICE_COVERAGE_MIN_RATIO).ceil() as usize;
        if coverage < minimum {
            stages.price_coverage_blocked += 1;
            continue;
        }
        stages.aligned_price_observations += values.len();
        values.sort_by(|left, right| match rule {
            ImpulseRule::BothV1 => right
                .return_6h
                .abs()
                .total_cmp(&left.return_6h.abs())
                .then_with(|| left.symbol.cmp(&right.symbol)),
            ImpulseRule::DownsideV1 => left
                .return_6h
                .total_cmp(&right.return_6h)
                .then_with(|| left.symbol.cmp(&right.symbol)),
        });
        values.truncate(PRICE_CANDIDATES_PER_DECISION);
        stages.top_price_candidates += values.len();
        for candidate in &values {
            candidate_times
                .entry(candidate.symbol.clone())
                .or_default()
                .push(candidate.decision_ts);
        }
        selected_by_time.insert(decision_ts, values);
    }
    (selected_by_time, candidate_times, stages)
}

/// 按已冻结的价格顺序选择首个 OI 与主动流同时确认的候选。
fn select_metrics_candidate<'a>(
    candidates: &'a [PriceCandidate],
    metrics_store: &MetricsStore,
    metrics_pass: &mut usize,
) -> Option<(&'a PriceCandidate, ContinuationEvidence)> {
    let mut selected = None;
    for candidate in candidates {
        let Some(evidence) = metrics_store.continuation_evidence_at(
            &candidate.symbol,
            candidate.decision_ts,
            candidate.long,
        ) else {
            continue;
        };
        *metrics_pass += 1;
        selected.get_or_insert((candidate, evidence));
    }
    selected
}

/// 用 2ATR 初始风险、2R 目标和 24h 上限结算单个冻结候选。
fn settle_candidate(
    candidate: &PriceCandidate,
    candles: &[CandleItem],
    evidence: ContinuationEvidence,
) -> Settlement {
    let entry_index = candidate.decision_index + 1;
    if entry_index + MAX_HOLDING_BARS > candles.len() {
        return Settlement::Incomplete;
    }
    let Some(atr) = atr_at(candles, candidate.decision_index) else {
        return Settlement::RiskBlocked;
    };
    let entry = candles[entry_index].o;
    let risk = atr * STOP_ATR_MULTIPLIER;
    let risk_pct = risk / entry * 100.0;
    if !entry.is_finite()
        || !risk.is_finite()
        || risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return Settlement::RiskBlocked;
    }
    let stop = if candidate.long {
        entry - risk
    } else {
        entry + risk
    };
    let target = if candidate.long {
        entry + risk * TARGET_R
    } else {
        entry - risk * TARGET_R
    };
    let last_index = entry_index + MAX_HOLDING_BARS - 1;
    let mut exit_index = last_index;
    let mut exit = candles[last_index].c;
    let mut gross_r = directional_r(candidate.long, entry, exit, risk);
    let mut exit_reason = "max_holding_timeout";
    for (offset, candle) in candles[entry_index..=last_index].iter().enumerate() {
        let current = entry_index + offset;
        let stop_hit = if candidate.long {
            candle.l <= stop
        } else {
            candle.h >= stop
        };
        let target_hit = if candidate.long {
            candle.h >= target
        } else {
            candle.l <= target
        };
        if stop_hit {
            exit_index = current;
            exit = stop;
            gross_r = -1.0;
            exit_reason = "atr_stop";
            break;
        }
        if target_hit {
            exit_index = current;
            exit = target;
            gross_r = TARGET_R;
            exit_reason = "target_2r";
            break;
        }
    }
    let entry_ts = candles[entry_index].ts;
    let exit_ts = candles[exit_index].ts.saturating_add(MS_15M);
    let funding_intervals = (exit_ts.div_euclid(MS_8H) - entry_ts.div_euclid(MS_8H)).max(0);
    let execution_cost_r = (entry + exit) * COST_RATE_PER_SIDE / risk;
    let funding_cost_r = entry * ADVERSE_FUNDING_PER_8H * funding_intervals as f64 / risk;
    let cost_r = execution_cost_r + funding_cost_r;
    Settlement::Trade(FlowFlipTrade {
        symbol: candidate.symbol.clone(),
        direction: if candidate.long { "long" } else { "short" },
        setup_ts: candidate.decision_ts,
        decision_ts: candidate.decision_ts,
        entry_ts,
        exit_ts,
        oi_change_4h: Some(evidence.oi_change_4h),
        prior_taker_median: Some(evidence.taker_median_1h),
        current_taker_median: Some(evidence.taker_median_1h),
        top_account_ratio: evidence.top_account_ratio,
        top_position_ratio: evidence.top_position_ratio,
        entry,
        stop,
        target,
        gross_r,
        cost_r,
        net_r: gross_r - cost_r,
        exit_reason,
    })
}

/// 将多空价格路径统一转换成入场时固定风险的 R 值。
fn directional_r(long: bool, entry: f64, exit: f64, risk: f64) -> f64 {
    if long {
        (exit - entry) / risk
    } else {
        (entry - exit) / risk
    }
}

/// 按已接受交易的真实持仓区间执行同币、总容量和同方向限制。
fn capacity_blocked(candidate: &FlowFlipTrade, accepted: &[FlowFlipTrade]) -> bool {
    let active = accepted
        .iter()
        .filter(|trade| trade.exit_ts > candidate.entry_ts)
        .collect::<Vec<_>>();
    active.iter().any(|trade| trade.symbol == candidate.symbol)
        || active.len() >= MAX_CONCURRENT
        || active
            .iter()
            .filter(|trade| trade.direction == candidate.direction)
            .count()
            >= MAX_SAME_DIRECTION
}

/// 汇总冻结口径并立即打印一次性审计结果。
fn build_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    metrics_audit: MetricsAudit,
    stages: LeverageContinuationStageCounts,
    trades: Vec<FlowFlipTrade>,
    rule: ImpulseRule,
) -> LeverageContinuationReport {
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = trades
                .iter()
                .filter(|trade| trade.entry_ts >= window.from_ms && trade.entry_ts < window.to_ms)
                .cloned()
                .collect::<Vec<_>>();
            (window.from_ms, metrics(&values, 1.0))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, value)| value.net_sum_r > 0.0)
        .count();
    let long_trades = trades
        .iter()
        .filter(|trade| trade.direction == "long")
        .cloned()
        .collect::<Vec<_>>();
    let short_trades = trades
        .iter()
        .filter(|trade| trade.direction == "short")
        .cloned()
        .collect::<Vec<_>>();
    let gross_zero_cost = metrics(&trades, 0.0);
    let overall = metrics(&trades, 1.0);
    let double_cost = metrics(&trades, 2.0);
    let long = metrics(&long_trades, 1.0);
    let short = metrics(&short_trades, 1.0);
    let effective_events = effective_event_count(&trades);
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_owned())
            .or_default() += 1;
    }
    let minority = long.trades.min(short.trades);
    let dominant_share = long.trades.max(short.trades) as f64 / trades.len().max(1) as f64;
    let direction_gate_passed = match rule {
        ImpulseRule::BothV1 => dominant_share <= 0.80 || minority >= 60,
        ImpulseRule::DownsideV1 => true,
    };
    let discovery_gate_passed = gross_zero_cost
        .net_expectancy_r
        .is_some_and(|value| value > 0.0)
        && gross_zero_cost
            .profit_factor
            .is_some_and(|value| value > 1.0)
        && overall.net_expectancy_r.is_some_and(|value| value > 0.0)
        && overall.profit_factor.is_some_and(|value| value > 1.0)
        && trades.len() >= 300
        && effective_events >= 180
        && positive_months >= 8
        && net_r_without_top_three_symbols > 0.0
        && direction_gate_passed;
    let report = LeverageContinuationReport {
        rule_version: rule.version().to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        metrics_audit,
        stages,
        effective_events,
        gross_zero_cost,
        overall,
        double_cost,
        long,
        short,
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        discovery_gate_passed,
        trades,
    };
    print_report(&report);
    report
}

/// 以稳定文本字段输出漏斗、成本、方向、月份和集中度。
fn print_report(report: &LeverageContinuationReport) {
    println!(
        "leverage_continuation_research\trule={}\tuniverse={}\tsymbols={}\tmapped={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tmetric_rows={}\tdecision_points={}\tcoverage_blocked={}\taligned_price_observations={}\ttop_price_candidates={}\tmetrics_pass={}\tselected={}\trisk_blocked={}\tcapacity_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}\tdiscovery_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.metrics_audit.mapped_symbols,
        report.metrics_audit.mapping_blocked_symbols,
        report.metrics_audit.requested_files,
        report.metrics_audit.available_files,
        report.metrics_audit.missing_files,
        report.metrics_audit.invalid_files,
        report.metrics_audit.rows,
        report.stages.decision_points,
        report.stages.price_coverage_blocked,
        report.stages.aligned_price_observations,
        report.stages.top_price_candidates,
        report.stages.metrics_pass,
        report.stages.selected,
        report.stages.risk_blocked,
        report.stages.capacity_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
        report.discovery_gate_passed,
    );
    print_metrics("gross_zero_cost", &report.gross_zero_cost);
    print_metrics("overall", &report.overall);
    print_metrics("double_cost", &report.double_cost);
    print_metrics("long", &report.long);
    print_metrics("short", &report.short);
    for (month, values) in &report.monthly {
        print_metrics(&format!("month_{month}"), values);
    }
    println!(
        "leverage_continuation_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
        report.top_three_positive_symbols.join(","),
        report.net_r_without_top_three_symbols,
        report
            .exit_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(","),
    );
}

/// 打印单个统计窗口，空比率使用 `null` 而非伪造零值。
fn print_metrics(label: &str, values: &FlowFlipMetrics) {
    println!(
        "leverage_continuation_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
        label,
        values.trades,
        values.net_sum_r,
        optional(values.net_expectancy_r),
        optional(values.profit_factor),
        optional(values.win_rate_pct),
        optional(values.trade_sharpe),
        values.max_drawdown_r,
        optional(values.recovery_factor),
    );
}

/// 将可选浮点统一为稳定的文本输出。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::UniverseWindow;
    use super::*;
    use std::collections::BTreeSet;

    /// 构造连续 15m 上涨样本，末端可注入不同强度的冲量。
    fn candles(symbol_bias: f64) -> Vec<CandleItem> {
        (0..=RETURN_24H_BARS + 32)
            .map(|index| {
                let close = 100.0 + index as f64 * symbol_bias;
                CandleItem {
                    ts: index as i64 * MS_15M,
                    o: close - 0.1,
                    h: close + 0.2,
                    l: close - 0.2,
                    c: close,
                    v: 1.0,
                    confirm: 1,
                }
            })
            .collect()
    }

    #[test]
    fn price_candidates_are_ranked_deterministically_and_limited_to_three() {
        let symbols = ["A", "B", "C", "D", "E"]
            .into_iter()
            .map(|symbol| format!("{symbol}-USDT-SWAP"))
            .collect::<BTreeSet<_>>();
        let schedule = UniverseSchedule {
            version: "test".to_owned(),
            windows: vec![UniverseWindow {
                from_ms: 0,
                to_ms: 2 * DAY_MS,
                members: symbols.clone(),
            }],
        };
        let candles_by_symbol = symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| (symbol.clone(), candles((index + 1) as f64 * 0.01)))
            .collect::<BTreeMap<_, _>>();

        let (by_time, _, stages) =
            build_price_candidates(&schedule, &candles_by_symbol, ImpulseRule::BothV1);

        let candidates = by_time.values().next().unwrap();
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].symbol, "E-USDT-SWAP");
        assert_eq!(stages.price_coverage_blocked, 0);
    }

    #[test]
    fn directional_r_is_symmetric_for_long_and_short() {
        assert_eq!(directional_r(true, 100.0, 102.0, 1.0), 2.0);
        assert_eq!(directional_r(false, 100.0, 98.0, 1.0), 2.0);
    }

    /// 构造 24h 仍上涨但最后 6h 已回落的完整因子样本。
    fn turning_candles() -> Vec<CandleItem> {
        (0..=RETURN_24H_BARS + 32)
            .map(|index| {
                let close = if index <= 103 {
                    100.0 + index as f64 * 0.1
                } else {
                    110.3 - (index - 103) as f64 * 0.05
                };
                CandleItem {
                    ts: index as i64 * MS_15M,
                    o: close,
                    h: close + 0.2,
                    l: close - 0.2,
                    c: close,
                    v: 1.0,
                    confirm: 1,
                }
            })
            .collect()
    }

    #[test]
    fn coverage_counts_complete_factors_before_direction_alignment() {
        let symbols = ["A", "B", "C", "D", "E"]
            .into_iter()
            .map(|symbol| format!("{symbol}-USDT-SWAP"))
            .collect::<BTreeSet<_>>();
        let schedule = UniverseSchedule {
            version: "test".to_owned(),
            windows: vec![UniverseWindow {
                from_ms: 0,
                to_ms: 2 * DAY_MS,
                members: symbols.clone(),
            }],
        };
        let candles_by_symbol = symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| {
                let values = if index < 3 {
                    candles((index + 1) as f64 * 0.01)
                } else {
                    turning_candles()
                };
                (symbol.clone(), values)
            })
            .collect::<BTreeMap<_, _>>();

        let (by_time, _, stages) =
            build_price_candidates(&schedule, &candles_by_symbol, ImpulseRule::BothV1);

        assert_eq!(stages.price_coverage_blocked, 0);
        assert_eq!(stages.aligned_price_observations, 3);
        assert_eq!(by_time.values().next().unwrap().len(), 3);
    }

    #[test]
    fn downside_rule_keeps_only_negative_six_and_twenty_four_hour_impulses() {
        assert!(ImpulseRule::DownsideV1.accepts_returns(-0.02, -0.05));
        assert!(!ImpulseRule::DownsideV1.accepts_returns(0.02, 0.05));
        assert!(!ImpulseRule::DownsideV1.accepts_returns(-0.02, 0.05));
        assert_eq!(ImpulseRule::DownsideV1.decision_interval_ms(), MS_8H / 2);
    }
}
