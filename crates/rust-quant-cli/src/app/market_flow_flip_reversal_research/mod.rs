mod failed_rebound;
mod leverage_continuation;
mod metrics;
mod report;

pub use self::failed_rebound::{
    parse_failed_rebound_research_args, run_failed_rebound_research, FailedReboundResearchArgs,
};
pub use self::leverage_continuation::{
    run_leverage_continuation_research, run_leverage_downside_continuation_research,
    LeverageContinuationReport, LeverageContinuationStageCounts,
};
pub use self::metrics::MetricsAudit;

use self::metrics::{load_metrics, FlowEvidence, MetricsStore};
use self::report::print_report;
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{anyhow, bail, Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_30M: i64 = 30 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const HISTORY_BARS: usize = 96;
const LOW_MEMORY_BARS: usize = 8;
const BREAKOUT_BARS: usize = 4;
const ACCEPTANCE_WAIT_BARS: usize = 8;
const ATR_PERIOD: usize = 14;
const PRICE_BOTTOM_RATIO: f64 = 0.20;
const PRICE_COVERAGE_MIN_RATIO: f64 = 0.80;
const STOP_ATR_BUFFER: f64 = 0.25;
const MIN_RISK_PCT: f64 = 0.5;
const MAX_RISK_PCT: f64 = 3.0;
const TARGET_R: f64 = 3.0;
const MAX_HOLDING_BARS: usize = 48 * 4;
const COST_RATE_PER_SIDE: f64 = 0.0008;
const DEFAULT_BINANCE_REST_BASE: &str = "https://fapi.binance.com";
const DEFAULT_BINANCE_DATA_BASE: &str = "https://data.binance.vision";

/// 冻结 V2 只暴露数据位置和下载并发，不允许命令行修改策略阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowFlipResearchArgs {
    pub manifest: PathBuf,
    pub metrics_cache: PathBuf,
    pub download_concurrency: usize,
    pub binance_rest_base: String,
    pub binance_data_base: String,
}

/// V2 因果漏斗，区分价格候选、跨交易所证据、风险和 outcome 完整性。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlowFlipStageCounts {
    pub price_tail_pass: usize,
    pub recent_low_pass: usize,
    pub breakout_pass: usize,
    pub metrics_pass: usize,
    pub acceptance_pass: usize,
    pub acceptance_invalidated: usize,
    pub acceptance_expired: usize,
    pub failure_pass: usize,
    pub failure_expired: usize,
    pub risk_blocked: usize,
    pub incomplete_outcomes: usize,
}

/// 单笔交易保留跨交易所证据，便于审计 OKX 成交与 Binance 状态的边界。
#[derive(Debug, Clone, PartialEq)]
pub struct FlowFlipTrade {
    pub symbol: String,
    pub direction: &'static str,
    pub setup_ts: i64,
    pub decision_ts: i64,
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub oi_change_4h: Option<f64>,
    pub prior_taker_median: Option<f64>,
    pub current_taker_median: Option<f64>,
    pub top_account_ratio: Option<f64>,
    pub top_position_ratio: Option<f64>,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub gross_r: f64,
    pub cost_r: f64,
    pub net_r: f64,
    pub exit_reason: &'static str,
}

/// 交易级固定 R 指标；统一资金审计前不代表组合权益。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowFlipMetrics {
    pub trades: usize,
    pub net_sum_r: f64,
    pub net_expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_r: f64,
    pub recovery_factor: Option<f64>,
}

/// 冻结 V2 的覆盖、频率、稳定性、成本和集中度报告。
#[derive(Debug, Clone, PartialEq)]
pub struct FlowFlipResearchReport {
    pub rule_version: String,
    pub universe_version: String,
    pub symbols: usize,
    pub metrics_audit: MetricsAudit,
    pub price_coverage_blocked: usize,
    pub stages: FlowFlipStageCounts,
    pub trades: Vec<FlowFlipTrade>,
    pub effective_events: usize,
    pub gross_zero_cost: FlowFlipMetrics,
    pub overall: FlowFlipMetrics,
    pub discovery: FlowFlipMetrics,
    pub validation: FlowFlipMetrics,
    pub double_cost: FlowFlipMetrics,
    pub monthly: Vec<(i64, FlowFlipMetrics)>,
    pub positive_months: usize,
    pub top_three_positive_symbols: Vec<String>,
    pub net_r_without_top_three_symbols: f64,
    pub exit_reasons: BTreeMap<String, usize>,
}

/// 单个历史币池生效窗口及其可交易成员。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseWindow {
    pub from_ms: i64,
    pub to_ms: i64,
    pub members: BTreeSet<String>,
}

/// 按月份排列的历史币池日程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseSchedule {
    pub version: String,
    pub windows: Vec<UniverseWindow>,
}

/// 将一次候选结算区分为成交、风险阻塞或结果数据不完整。
enum Settlement {
    Trade(FlowFlipTrade, usize),
    RiskBlocked,
    Incomplete,
}

/// 选择冻结 V2 立即入场或 V3 首次回踩接受规则。
#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryRule {
    ImmediateV2,
    AcceptanceV3,
}

/// V3 回踩窗口内的首个确定性结果。
enum AcceptanceDecision {
    Accepted(usize),
    Invalidated(usize),
    Expired(usize),
    Incomplete,
}

/// 解析冻结 V2 参数；未知参数直接失败。
pub fn parse_flow_flip_research_args<I>(values: I) -> Result<FlowFlipResearchArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    let mut metrics_cache = None;
    let mut download_concurrency = 16usize;
    let mut binance_rest_base = DEFAULT_BINANCE_REST_BASE.to_owned();
    let mut binance_data_base = DEFAULT_BINANCE_DATA_BASE.to_owned();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut values)?)),
            "--metrics-cache" => metrics_cache = Some(PathBuf::from(value(&mut values)?)),
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?;
            }
            "--binance-rest-base" => {
                binance_rest_base = value(&mut values)?.trim_end_matches('/').to_owned()
            }
            "--binance-data-base" => {
                binance_data_base = value(&mut values)?.trim_end_matches('/').to_owned()
            }
            "--help" | "-h" => bail!(flow_flip_research_usage()),
            _ => bail!("unknown argument: {arg}\n{}", flow_flip_research_usage()),
        }
    }
    if !(1..=32).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 32");
    }
    Ok(FlowFlipResearchArgs {
        manifest: manifest.context("--manifest is required")?,
        metrics_cache: metrics_cache.context("--metrics-cache is required")?,
        download_concurrency,
        binance_rest_base,
        binance_data_base,
    })
}

/// 返回冻结 V2 的最小用法。
pub fn flow_flip_research_usage() -> &'static str {
    "Usage: market_flow_flip_reversal_research --manifest PATH --metrics-cache PATH [--download-concurrency 16]"
}

impl UniverseSchedule {
    /// 从历史 manifest 构造无重叠、按生效时间排序的研究窗口。
    fn from_manifest(manifest: HistoricalUniverseManifest) -> Result<Self> {
        if manifest.schema_version != 1
            || manifest.exchange != "okx"
            || manifest.market_type != "perpetual_swap"
            || manifest.quote_currency != "USDT"
            || manifest.timeframe != "15m"
            || !manifest
                .selection_rule
                .starts_with("current-live OKX USDT swaps only")
            || !manifest
                .source
                .classification_boundary
                .contains("instCategory=1")
        {
            bail!("flow-flip research requires current-live crypto-only OKX 15m manifest");
        }
        let mut windows = manifest
            .months
            .into_iter()
            .map(|month| UniverseWindow {
                from_ms: month.effective_from_ms,
                to_ms: month.effective_to_ms,
                members: month
                    .members
                    .into_iter()
                    .map(|member| member.symbol.to_ascii_uppercase())
                    .collect(),
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| window.from_ms);
        if windows.len() < 7
            || windows.iter().any(|window| {
                window.from_ms >= window.to_ms
                    || window.members.is_empty()
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("flow-flip research requires at least seven contiguous monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    /// 返回全部窗口成员的去重并集。
    pub(super) fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// 查找指定信号时点所属的币池窗口。
    fn window_at(&self, ts: i64) -> Option<&UniverseWindow> {
        self.windows
            .iter()
            .find(|window| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 先只用价格建立候选日，再加载校验后的 Binance metrics 并严格回放。
pub async fn run_flow_flip_research(
    args: &FlowFlipResearchArgs,
    database_url: &str,
) -> Result<FlowFlipResearchReport> {
    run_research(args, database_url, EntryRule::ImmediateV2).await
}

/// 运行冻结 V3 的“突破后首次回踩接受”研究，不修改 V2 或执行路径。
pub async fn run_flow_acceptance_research(
    args: &FlowFlipResearchArgs,
    database_url: &str,
) -> Result<FlowFlipResearchReport> {
    run_research(args, database_url, EntryRule::AcceptanceV3).await
}

/// 复用共同数据与统计口径运行指定的冻结入场规则。
async fn run_research(
    args: &FlowFlipResearchArgs,
    database_url: &str,
    entry_rule: EntryRule,
) -> Result<FlowFlipResearchReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode flow-flip universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule
        .windows
        .first()
        .context("missing first flow window")?;
    let last = schedule
        .windows
        .last()
        .context("missing last flow window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for flow-flip research")?;
    let mut candles_by_symbol = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        let outcome_padding = match entry_rule {
            EntryRule::ImmediateV2 => 2 * DAY_MS,
            EntryRule::AcceptanceV3 => 3 * DAY_MS,
        };
        candles_by_symbol.insert(
            symbol.clone(),
            load_symbol_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(32 * DAY_MS),
                last.to_ms.saturating_add(outcome_padding),
            )
            .await?,
        );
    }
    let (price_tail, price_coverage_blocked) =
        build_price_tail_states(&schedule, &candles_by_symbol);
    let (candidate_indices, candidate_times, mut stages) =
        build_price_candidates(&schedule, &candles_by_symbol, &price_tail);
    let (metrics_store, metrics_audit) = load_metrics(args, &schedule, &candidate_times).await?;
    let mut trades = Vec::new();
    for (symbol, indices) in candidate_indices {
        let candles = candles_by_symbol
            .get(&symbol)
            .with_context(|| format!("missing candles for {symbol}"))?;
        trades.extend(match entry_rule {
            EntryRule::ImmediateV2 => {
                scan_symbol(&symbol, candles, &indices, &metrics_store, &mut stages)
            }
            EntryRule::AcceptanceV3 => {
                scan_symbol_acceptance(&symbol, candles, &indices, &metrics_store, &mut stages)
            }
        });
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let split_index = match entry_rule {
        EntryRule::ImmediateV2 => 4,
        EntryRule::AcceptanceV3 => 6,
    };
    let split_ms = schedule
        .windows
        .get(split_index)
        .context("research window is shorter than the frozen discovery split")?
        .from_ms;
    let discovery = trades
        .iter()
        .filter(|trade| trade.entry_ts < split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let validation = trades
        .iter()
        .filter(|trade| trade.entry_ts >= split_ms)
        .cloned()
        .collect::<Vec<_>>();
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
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_owned())
            .or_default() += 1;
    }
    let report = FlowFlipResearchReport {
        rule_version: match entry_rule {
            EntryRule::ImmediateV2 => "oi4h_taker_flip_immediate_breakout_v2",
            EntryRule::AcceptanceV3 => "oi4h_taker_flip_first_retest_acceptance_v3",
        }
        .to_owned(),
        universe_version: schedule.version.clone(),
        symbols: candles_by_symbol.len(),
        metrics_audit,
        price_coverage_blocked,
        stages,
        effective_events: effective_event_count(&trades),
        gross_zero_cost: metrics(&trades, 0.0),
        overall: metrics(&trades, 1.0),
        discovery: metrics(&discovery, 1.0),
        validation: metrics(&validation, 1.0),
        double_cost: metrics(&trades, 2.0),
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        trades,
    };
    print_report(&report);
    Ok(report)
}

/// 在每个时点按 24h 跌幅横截面标记 bottom-20% 成员。
fn build_price_tail_states(
    schedule: &UniverseSchedule,
    candles_by_symbol: &BTreeMap<String, Vec<CandleItem>>,
) -> (BTreeMap<String, BTreeSet<i64>>, usize) {
    let mut grouped = BTreeMap::<i64, Vec<(String, f64)>>::new();
    for (symbol, candles) in candles_by_symbol {
        for index in HISTORY_BARS..candles.len() {
            if candles[index].ts - candles[index - HISTORY_BARS].ts != HISTORY_BARS as i64 * MS_15M
            {
                continue;
            }
            let decision_ts = candles[index].ts.saturating_add(MS_15M);
            if !schedule
                .window_at(decision_ts)
                .is_some_and(|window| window.members.contains(symbol))
            {
                continue;
            }
            let change = candles[index].c / candles[index - HISTORY_BARS].c - 1.0;
            if change.is_finite() {
                grouped
                    .entry(decision_ts)
                    .or_default()
                    .push((symbol.clone(), change));
            }
        }
    }
    let mut eligible = BTreeMap::<String, BTreeSet<i64>>::new();
    let mut blocked = 0usize;
    for (decision_ts, mut values) in grouped {
        let Some(window) = schedule.window_at(decision_ts) else {
            continue;
        };
        values.retain(|(symbol, _)| window.members.contains(symbol));
        let minimum = (window.members.len() as f64 * PRICE_COVERAGE_MIN_RATIO).ceil() as usize;
        if values.len() < minimum {
            blocked += 1;
            continue;
        }
        values.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let bottom = (values.len() as f64 * PRICE_BOTTOM_RATIO).ceil() as usize;
        for (rank, (symbol, change)) in values.into_iter().enumerate() {
            if change < 0.0 && rank < bottom {
                eligible.entry(symbol).or_default().insert(decision_ts);
            }
        }
    }
    (eligible, blocked)
}

/// 从价格尾部成员中筛出新低后向上突破的因果候选。
fn build_price_candidates(
    schedule: &UniverseSchedule,
    candles_by_symbol: &BTreeMap<String, Vec<CandleItem>>,
    price_tail: &BTreeMap<String, BTreeSet<i64>>,
) -> (
    BTreeMap<String, Vec<usize>>,
    BTreeMap<String, Vec<i64>>,
    FlowFlipStageCounts,
) {
    let mut indices = BTreeMap::<String, Vec<usize>>::new();
    let mut times = BTreeMap::<String, Vec<i64>>::new();
    let mut stages = FlowFlipStageCounts::default();
    for (symbol, candles) in candles_by_symbol {
        for index in HISTORY_BARS + LOW_MEMORY_BARS..candles.len().saturating_sub(1) {
            let decision_ts = candles[index].ts.saturating_add(MS_15M);
            if !schedule
                .window_at(decision_ts)
                .is_some_and(|window| window.members.contains(symbol))
                || !price_tail
                    .get(symbol)
                    .is_some_and(|points| points.contains(&decision_ts))
            {
                continue;
            }
            stages.price_tail_pass += 1;
            if !recent_new_low(candles, index) {
                continue;
            }
            stages.recent_low_pass += 1;
            if !bullish_breakout(candles, index) {
                continue;
            }
            stages.breakout_pass += 1;
            indices.entry(symbol.clone()).or_default().push(index);
            times.entry(symbol.clone()).or_default().push(decision_ts);
        }
    }
    (indices, times, stages)
}

/// 判断最近八根内是否出现相对前 96 根的新低。
fn recent_new_low(candles: &[CandleItem], index: usize) -> bool {
    (index + 1 - LOW_MEMORY_BARS..=index).any(|candidate| {
        candidate >= HISTORY_BARS
            && candles[candidate].l
                < candles[candidate - HISTORY_BARS..candidate]
                    .iter()
                    .map(|candle| candle.l)
                    .reduce(f64::min)
                    .unwrap_or(f64::NAN)
    })
}

/// 判断当前阳线收盘是否突破此前四根最高价。
fn bullish_breakout(candles: &[CandleItem], index: usize) -> bool {
    if index < BREAKOUT_BARS {
        return false;
    }
    let candle = &candles[index];
    let previous_high = candles[index - BREAKOUT_BARS..index]
        .iter()
        .map(|item| item.h)
        .reduce(f64::max)
        .unwrap_or(f64::NAN);
    candle.c > candle.o && candle.c > previous_high
}

/// 按 V2 立即入场规则回放单个币种并阻止持仓重叠。
fn scan_symbol(
    symbol: &str,
    candles: &[CandleItem],
    candidate_indices: &[usize],
    metrics_store: &MetricsStore,
    stages: &mut FlowFlipStageCounts,
) -> Vec<FlowFlipTrade> {
    let mut trades = Vec::new();
    let mut locked_until = None::<usize>;
    for index in candidate_indices.iter().copied() {
        if locked_until.is_some_and(|exit_index| index <= exit_index) {
            continue;
        }
        let decision_ts = candles[index].ts.saturating_add(MS_15M);
        let Some(evidence) = metrics_store.evidence_at(symbol, decision_ts) else {
            continue;
        };
        stages.metrics_pass += 1;
        match settle_trade(symbol, candles, index, evidence) {
            Settlement::Trade(trade, exit_index) => {
                trades.push(trade);
                locked_until = Some(exit_index);
            }
            Settlement::RiskBlocked => stages.risk_blocked += 1,
            Settlement::Incomplete => stages.incomplete_outcomes += 1,
        }
    }
    trades
}

/// 按 V3 首次回踩接受规则回放单个币种并阻止持仓重叠。
fn scan_symbol_acceptance(
    symbol: &str,
    candles: &[CandleItem],
    candidate_indices: &[usize],
    metrics_store: &MetricsStore,
    stages: &mut FlowFlipStageCounts,
) -> Vec<FlowFlipTrade> {
    let mut trades = Vec::new();
    let mut locked_until = None::<usize>;
    for setup_index in candidate_indices.iter().copied() {
        if locked_until.is_some_and(|resolved_index| setup_index <= resolved_index) {
            continue;
        }
        let decision_ts = candles[setup_index].ts.saturating_add(MS_15M);
        let Some(evidence) = metrics_store.evidence_at(symbol, decision_ts) else {
            continue;
        };
        stages.metrics_pass += 1;
        match acceptance_decision(candles, setup_index) {
            AcceptanceDecision::Accepted(acceptance_index) => {
                stages.acceptance_pass += 1;
                match settle_trade_at(symbol, candles, setup_index, acceptance_index + 1, evidence)
                {
                    Settlement::Trade(trade, exit_index) => {
                        trades.push(trade);
                        locked_until = Some(exit_index);
                    }
                    Settlement::RiskBlocked => {
                        stages.risk_blocked += 1;
                        locked_until = Some(acceptance_index);
                    }
                    Settlement::Incomplete => stages.incomplete_outcomes += 1,
                }
            }
            AcceptanceDecision::Invalidated(index) => {
                stages.acceptance_invalidated += 1;
                locked_until = Some(index);
            }
            AcceptanceDecision::Expired(index) => {
                stages.acceptance_expired += 1;
                locked_until = Some(index);
            }
            AcceptanceDecision::Incomplete => stages.incomplete_outcomes += 1,
        }
    }
    trades
}

/// 仅使用 setup 后八根已完成 K 线判定接受、失效或过期。
fn acceptance_decision(candles: &[CandleItem], setup_index: usize) -> AcceptanceDecision {
    let required_last = setup_index.saturating_add(ACCEPTANCE_WAIT_BARS);
    let Some(last_available) = candles.len().checked_sub(1) else {
        return AcceptanceDecision::Incomplete;
    };
    let last = required_last.min(last_available);
    let acceptance_level = candles[setup_index - BREAKOUT_BARS..setup_index]
        .iter()
        .map(|candle| candle.h)
        .reduce(f64::max)
        .unwrap_or(f64::NAN);
    let structure_low = candles[setup_index + 1 - LOW_MEMORY_BARS..=setup_index]
        .iter()
        .map(|candle| candle.l)
        .reduce(f64::min)
        .unwrap_or(f64::NAN);
    for index in setup_index + 1..=last {
        let candle = &candles[index];
        if candle.c <= structure_low {
            return AcceptanceDecision::Invalidated(index);
        }
        if candle.l <= acceptance_level && candle.c > acceptance_level && candle.c > candle.o {
            return AcceptanceDecision::Accepted(index);
        }
    }
    if required_last > last_available {
        AcceptanceDecision::Incomplete
    } else {
        AcceptanceDecision::Expired(last)
    }
}

/// 在 setup 下一根开盘按 V2 规则结算交易。
fn settle_trade(
    symbol: &str,
    candles: &[CandleItem],
    decision_index: usize,
    evidence: FlowEvidence,
) -> Settlement {
    settle_trade_at(
        symbol,
        candles,
        decision_index,
        decision_index + 1,
        evidence,
    )
}

/// 用冻结结构止损、3R 目标和 48h 上限结算指定入场位置。
fn settle_trade_at(
    symbol: &str,
    candles: &[CandleItem],
    decision_index: usize,
    entry_index: usize,
    evidence: FlowEvidence,
) -> Settlement {
    if entry_index + MAX_HOLDING_BARS > candles.len() {
        return Settlement::Incomplete;
    }
    let Some(atr) = atr_at(candles, decision_index) else {
        return Settlement::RiskBlocked;
    };
    let entry = candles[entry_index].o;
    let structure_low = candles[decision_index + 1 - LOW_MEMORY_BARS..=decision_index]
        .iter()
        .map(|candle| candle.l)
        .reduce(f64::min)
        .unwrap_or(f64::NAN);
    let stop = structure_low - atr * STOP_ATR_BUFFER;
    let risk = entry - stop;
    let risk_pct = risk / entry * 100.0;
    if !entry.is_finite()
        || !stop.is_finite()
        || risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return Settlement::RiskBlocked;
    }
    let target = entry + risk * TARGET_R;
    let last_index = entry_index + MAX_HOLDING_BARS - 1;
    let mut exit_index = last_index;
    let mut exit = candles[last_index].c;
    let mut gross_r = (exit - entry) / risk;
    let mut exit_reason = "max_holding_timeout";
    for (offset, candle) in candles[entry_index..=last_index].iter().enumerate() {
        let current = entry_index + offset;
        if candle.l <= stop {
            exit_index = current;
            exit = stop;
            gross_r = -1.0;
            exit_reason = "structure_stop";
            break;
        }
        if candle.h >= target {
            exit_index = current;
            exit = target;
            gross_r = TARGET_R;
            exit_reason = "target_3r";
            break;
        }
    }
    let cost_r = (entry + exit) * COST_RATE_PER_SIDE / risk;
    Settlement::Trade(
        FlowFlipTrade {
            symbol: symbol.to_owned(),
            direction: "long",
            setup_ts: candles[decision_index].ts.saturating_add(MS_15M),
            decision_ts: candles[decision_index].ts.saturating_add(MS_15M),
            entry_ts: candles[entry_index].ts,
            exit_ts: candles[exit_index].ts.saturating_add(MS_15M),
            oi_change_4h: Some(evidence.oi_change_4h),
            prior_taker_median: Some(evidence.prior_taker_median),
            current_taker_median: Some(evidence.current_taker_median),
            top_account_ratio: evidence.top_account_ratio,
            top_position_ratio: evidence.top_position_ratio,
            entry,
            stop,
            target,
            gross_r,
            cost_r,
            net_r: gross_r - cost_r,
            exit_reason,
        },
        exit_index,
    )
}

/// 从本地 quant_core 读取已确认且严格按时间排序的 15m K 线。
async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid manifest symbol {symbol}");
    }
    let table = format!("{}_candles_15m", symbol.to_ascii_lowercase());
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM \"{table}\" WHERE confirm = '1' AND ts >= $1 AND ts < $2 ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load flow-flip candles from {table}"))?
        .into_iter()
        .map(|row| {
            Ok(CandleItem {
                ts: row.get("ts"),
                o: parse_number(row.get::<String, _>("o"))?,
                h: parse_number(row.get::<String, _>("h"))?,
                l: parse_number(row.get::<String, _>("l"))?,
                c: parse_number(row.get::<String, _>("c"))?,
                v: parse_number(row.get::<String, _>("vol"))?,
                confirm: 1,
            })
        })
        .collect()
}

/// 计算截至指定位置的 14 根真实波幅均值。
fn atr_at(candles: &[CandleItem], index: usize) -> Option<f64> {
    if index + 1 < ATR_PERIOD {
        return None;
    }
    let start = index + 1 - ATR_PERIOD;
    let mut total = 0.0;
    for current in start..=index {
        let candle = &candles[current];
        let previous_close = current
            .checked_sub(1)
            .map(|previous| candles[previous].c)
            .unwrap_or(candle.c);
        total += (candle.h - candle.l)
            .max((candle.h - previous_close).abs())
            .max((candle.l - previous_close).abs());
    }
    let atr = total / ATR_PERIOD as f64;
    (atr.is_finite() && atr > 0.0).then_some(atr)
}

/// 按给定成本倍数汇总交易级 R 指标。
fn metrics(trades: &[FlowFlipTrade], cost_multiplier: f64) -> FlowFlipMetrics {
    if trades.is_empty() {
        return FlowFlipMetrics::default();
    }
    let values = trades
        .iter()
        .map(|trade| trade.gross_r - trade.cost_r * cost_multiplier)
        .collect::<Vec<_>>();
    let net_sum_r = values.iter().sum::<f64>();
    let profit = values.iter().filter(|value| **value > 0.0).sum::<f64>();
    let loss = values
        .iter()
        .filter(|value| **value < 0.0)
        .map(|value| value.abs())
        .sum::<f64>();
    let mean = net_sum_r / values.len() as f64;
    let variance = if values.len() > 1 {
        values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64
    } else {
        0.0
    };
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in &values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    FlowFlipMetrics {
        trades: values.len(),
        net_sum_r,
        net_expectancy_r: Some(mean),
        profit_factor: (loss > 0.0).then_some(profit / loss),
        win_rate_pct: Some(
            values.iter().filter(|value| **value > 0.0).count() as f64 / values.len() as f64
                * 100.0,
        ),
        trade_sharpe: (variance > 0.0)
            .then_some(mean / variance.sqrt() * (values.len() as f64).sqrt()),
        max_drawdown_r: max_drawdown,
        recovery_factor: (max_drawdown > 0.0).then_some(net_sum_r / max_drawdown),
    }
}

/// 把 30 分钟内的同时触发归并为同一有效市场事件。
fn effective_event_count(trades: &[FlowFlipTrade]) -> usize {
    let mut count = 0usize;
    let mut latest = None::<i64>;
    for trade in trades {
        if latest.is_none_or(|point| trade.entry_ts - point > MS_30M) {
            count += 1;
        }
        latest = Some(trade.entry_ts);
    }
    count
}

/// 计算移除净贡献最高三个盈利币种后的集中度结果。
fn concentration_without_top_three(trades: &[FlowFlipTrade]) -> (Vec<String>, f64) {
    let mut by_symbol = BTreeMap::<String, f64>::new();
    for trade in trades {
        *by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
    }
    let mut positive = by_symbol
        .into_iter()
        .filter(|(_, value)| *value > 0.0)
        .collect::<Vec<_>>();
    positive.sort_by(|left, right| right.1.total_cmp(&left.1));
    let top = positive
        .iter()
        .take(3)
        .map(|(symbol, _)| symbol.clone())
        .collect::<Vec<_>>();
    let removed = positive
        .iter()
        .take(3)
        .map(|(_, value)| *value)
        .sum::<f64>();
    (
        top,
        trades.iter().map(|trade| trade.net_r).sum::<f64>() - removed,
    )
}

/// 解析数据库数值并拒绝非有限值。
fn parse_number(value: String) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse candle number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite candle number {value}");
    }
    Ok(parsed)
}

/// 限制动态表名只能来自规范的 OKX USDT 永续标识。
fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
