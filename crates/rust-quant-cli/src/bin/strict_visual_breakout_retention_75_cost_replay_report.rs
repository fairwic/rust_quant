//! 汇总 V23 与首次确认保留率 75% 严格子集的冻结成本回放。
//!
//! 本工具只读取既有 JSON 结果并输出 L2 机器报告，不连接数据库、不生成信号、
//! 不搜索其他阈值，也不注册 Paper/Live 策略入口。

use anyhow::{bail, Context, Result};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};

#[path = "strict_visual_breakout_retention_75_cost_replay_report/report_evaluation.rs"]
mod report_evaluation;

const RETENTION_THRESHOLD: f64 = 0.75;
const RETENTION_EPSILON: f64 = 1e-12;
const VALIDATION_START_MS: i64 = 1_772_294_400_000;
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const REFERENCE_STARTING_EQUITY_R: f64 = 100.0;
const STRICT_LONG_FAMILY: &str = "strict_visual_consolidation_break_long";
const STRICT_SHORT_FAMILY: &str = "strict_visual_consolidation_break_short";

/// 最小命令行参数；路径全部显式传入，避免读取漂移的默认报告。
struct Args {
    baseline: PathBuf,
    candidate: PathBuf,
    quality_oracle: PathBuf,
    preregistration: PathBuf,
    output: PathBuf,
}

impl Args {
    /// 解析五个位置参数，拒绝任何未登记研究轴。
    fn parse() -> Result<Self> {
        let mut values = std::env::args().skip(1);
        let args = Self {
            baseline: values.next().context("缺少 V23 baseline JSON")?.into(),
            candidate: values
                .next()
                .context("缺少 retention>=0.75 candidate JSON")?
                .into(),
            quality_oracle: values
                .next()
                .context("缺少 V23 retention 质量账本 JSON")?
                .into(),
            preregistration: values.next().context("缺少本轮预注册文件")?.into(),
            output: values.next().context("缺少机器报告输出路径")?.into(),
        };
        if values.next().is_some() {
            bail!("只允许 baseline、candidate、quality oracle、preregistration、output 五个参数");
        }
        Ok(args)
    }
}

/// 四腿执行成本合同；浮点值来自回放报告，不在汇总层重算。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct ExecutionCostContract {
    entry_fee_bps: f64,
    entry_slippage_bps: f64,
    exit_fee_bps: f64,
    exit_slippage_bps: f64,
}

/// Top60 单成员身份；汇总只需要交易对名称。
#[derive(Debug, Deserialize)]
struct SymbolInput {
    symbol: String,
}

/// 候选账本外壳；blocked events 不参与实际成交指标。
#[derive(Debug, Deserialize)]
struct CandidateLedgerInput {
    candidates: Vec<ReplayCandidateInput>,
}

/// 信号时可见的严格横盘字段，用于最近亏损样本核图。
#[derive(Debug, Deserialize)]
struct VisibleFeaturesInput {
    signal_atr: Option<f64>,
    strict_visual_range_height: Option<f64>,
    strict_visual_range_length_bars: Option<usize>,
}

/// 冻结止损、目标和退出政策；这些值均在入场前确定。
#[derive(Debug, Deserialize)]
struct FrozenRiskInput {
    exit_policy: String,
    stop_price: Option<f64>,
    target_price: Option<f64>,
}

/// 候选的后验成交结果；只有 closed_trade 且存在成本后 R 才进入指标。
#[derive(Debug, Deserialize)]
struct OutcomeInput {
    status: String,
    entry_time_ms: Option<i64>,
    entry_price: Option<f64>,
    exit_time_ms: Option<i64>,
    exit_price: Option<f64>,
    exit_reason: Option<String>,
    actual_initial_stop: Option<f64>,
    actual_target_from_frozen_intent: Option<f64>,
    cost_adjusted_net_r: Option<f64>,
}

/// Top60 候选账本中的一条实际意图或成交。
#[derive(Debug, Deserialize)]
struct ReplayCandidateInput {
    symbol: String,
    direction: String,
    event_cluster_id: String,
    families: Vec<String>,
    signal_time_ms: i64,
    time_visible_features: VisibleFeaturesInput,
    frozen_risk: FrozenRiskInput,
    outcome: OutcomeInput,
}

/// 已发布家族指标，仅用于核对汇总层没有漏算长短交易。
#[derive(Debug, Deserialize)]
struct PublishedMetricInput {
    trades: usize,
    net_r: f64,
}

/// 单个家族的成本后指标。
#[derive(Debug, Deserialize)]
struct PublishedFamilyInput {
    cost_adjusted: PublishedMetricInput,
}

/// Top60 回放中本工具需要的最小字段集合。
#[derive(Debug, Deserialize)]
struct ReplayInput {
    strategy_version: String,
    strict_visual_breakout_research_variant: String,
    universe_version: String,
    universe_manifest_sha256: String,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    manifest_evaluation_end_ms: i64,
    evaluation_window_was_clipped: bool,
    expected_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<Value>,
    full_universe_complete: bool,
    execution_cost_contract: ExecutionCostContract,
    per_symbol: Vec<SymbolInput>,
    candidate_ledger: CandidateLedgerInput,
    strict_visual_breakout_family: PublishedFamilyInput,
    strict_visual_breakout_short_family: PublishedFamilyInput,
}

/// L1/L2 retention 账本的数据身份。
#[derive(Debug, Deserialize)]
struct QualityDatasetInput {
    universe_version: String,
    manifest_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    validation_start_ms: i64,
}

/// 原始 V23 首次合法确认的一条信号身份。
#[derive(Debug, Deserialize)]
struct QualityCandidateInput {
    symbol: String,
    direction: String,
    split: String,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    event_cluster: String,
    acceptance_retention_ratio: f64,
}

/// 先前 outcome-blind 账本；仅用来验证当前候选仍是原确认集合的严格子集。
#[derive(Debug, Deserialize)]
struct QualityOracleInput {
    dataset: QualityDatasetInput,
    candidate_ledger: Vec<QualityCandidateInput>,
}

/// 原始确认或实际意图的稳定身份；不包含任何结果字段。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SignalIdentity {
    symbol: String,
    direction: String,
    signal_time_ms: i64,
}

/// 一笔成本后严格横盘成交；只保留统计和人工核图所需字段。
#[derive(Debug, Clone)]
struct TradeObservation {
    ledger_index: usize,
    identity: SignalIdentity,
    event_cluster_id: String,
    entry_time_ms: i64,
    entry_price: f64,
    exit_time_ms: i64,
    exit_price: f64,
    exit_reason: String,
    frozen_stop: f64,
    frozen_target: f64,
    actual_initial_stop: Option<f64>,
    actual_target: Option<f64>,
    exit_policy: String,
    signal_atr: Option<f64>,
    range_height: Option<f64>,
    range_length_bars: Option<usize>,
    net_r: f64,
}

/// 成本后 R 指标；Profit Factor 使用正 R 总和除以负 R 绝对值。
#[derive(Debug, Clone, Serialize)]
struct MetricSummary {
    trades: usize,
    wins: usize,
    losses: usize,
    flats: usize,
    win_rate_percent: f64,
    net_r: f64,
    average_net_r: f64,
    gross_profit_r: f64,
    gross_loss_r: f64,
    profit_factor_r: Option<f64>,
    profit_factor_r_is_infinite: bool,
    chronological_closed_equity_max_drawdown_r: f64,
    reference_100r_max_drawdown_percent: f64,
    reference_100r_ending_equity_r: f64,
    reference_100r_minimum_equity_r: f64,
    reference_equity_breached: bool,
    distinct_symbols: usize,
    direction_hour_event_clusters: usize,
}

/// 总体、分窗和分方向指标；每项都复用同一成本后 R 定义。
#[derive(Debug, Serialize)]
struct MetricBreakdown {
    overall: MetricSummary,
    discovery: MetricSummary,
    validation: MetricSummary,
    long: MetricSummary,
    short: MetricSummary,
}

/// 单币、月份或事件簇的收益贡献。
#[derive(Debug, Clone, Serialize)]
struct Contribution {
    key: String,
    trades: usize,
    net_r: f64,
    average_net_r: f64,
}

/// 供结果报告直接引用的一笔成交摘要。
#[derive(Debug, Clone, Serialize)]
struct TradeSummary {
    symbol: String,
    direction: String,
    signal_time_bjt: String,
    entry_time_bjt: String,
    entry_price: f64,
    exit_time_bjt: String,
    exit_price: f64,
    exit_reason: String,
    frozen_stop: f64,
    frozen_target: f64,
    actual_initial_stop: Option<f64>,
    actual_target: Option<f64>,
    exit_policy: String,
    signal_atr: Option<f64>,
    range_height: Option<f64>,
    range_length_bars: Option<usize>,
    cost_adjusted_net_r: f64,
}

/// 头部成交、单币和同向小时事件簇的收益集中度。
#[derive(Debug, Serialize)]
struct ConcentrationAudit {
    top_five_winning_trades: Vec<TradeSummary>,
    top_five_winning_gross_profit_share_percent: f64,
    after_removing_top_five_winners: MetricSummary,
    symbol_contributions_descending: Vec<Contribution>,
    best_symbol_removed: Option<String>,
    after_removing_best_symbol: MetricSummary,
    month_contributions_chronological: Vec<Contribution>,
    event_cluster_count: usize,
    positive_event_cluster_r: f64,
    largest_positive_event_cluster: Option<Contribution>,
    largest_positive_event_cluster_share_percent: f64,
    top_positive_event_clusters: Vec<Contribution>,
}

/// 实际候选状态数量和统一风险暴露诊断。
#[derive(Debug, Serialize)]
struct ExecutionAudit {
    strict_candidate_intents: usize,
    closed_trades: usize,
    non_closed_status_counts: BTreeMap<String, usize>,
    max_concurrent_positions_same_timestamp_entries_first: usize,
    max_concurrent_initial_r: usize,
    published_family_trade_count: usize,
    published_family_net_r: f64,
    ledger_matches_published_family_metrics: bool,
}

/// 一个版本的完整 L2 成本诊断。
#[derive(Debug, Serialize)]
struct ReplayAnalysis {
    strategy_version: String,
    variant: String,
    execution: ExecutionAudit,
    metrics: MetricBreakdown,
    concentration: ConcentrationAudit,
    latest_ten_losses: Vec<TradeSummary>,
}

/// 原始信号身份和实际候选对 75% oracle 的覆盖核对。
#[derive(Debug, Serialize)]
struct RawSubsetAudit {
    eligible_symbols: usize,
    baseline_v23_raw_confirmations: usize,
    retention_75_raw_confirmations: usize,
    retention_75_share_percent: f64,
    retention_75_long_confirmations: usize,
    retention_75_short_confirmations: usize,
    retention_75_discovery_confirmations: usize,
    retention_75_validation_confirmations: usize,
    retention_75_symbols: usize,
    retention_75_direction_hour_event_clusters: usize,
    retention_75_minimum_observed: Option<f64>,
    raw_identity_duplicates: usize,
    candidate_actual_intents_absent_from_raw_retention_75_oracle: usize,
    candidate_actual_intents_not_present_in_baseline_actual_intents: usize,
    interpretation: &'static str,
}

/// 基线与候选的核心差值；正 EV/PF 差值只代表改善，不代表已盈利。
#[derive(Debug, Serialize)]
struct ReplayDelta {
    closed_trades: i64,
    net_r: f64,
    average_net_r: f64,
    profit_factor_r: Option<f64>,
    max_drawdown_r: f64,
}

/// 单条预注册停止/晋级门禁。
#[derive(Debug, Serialize)]
struct GateCheck {
    name: &'static str,
    passed: bool,
    observed: String,
    requirement: &'static str,
}

/// 最终机器产物；原始大账本保留在临时文件，不复制进第三份研究产物。
#[derive(Debug, Serialize)]
struct MachineReport {
    schema_version: &'static str,
    generated_at_ms: i64,
    level: &'static str,
    research_status: &'static str,
    promotion_status: &'static str,
    single_variable: &'static str,
    frozen_threshold: f64,
    forbidden_threshold_search: &'static str,
    preregistration_sha256: String,
    source_sha256: BTreeMap<&'static str, String>,
    dataset: DatasetAudit,
    execution_cost_contract: ExecutionCostContract,
    raw_subset_audit: RawSubsetAudit,
    baseline: ReplayAnalysis,
    candidate: ReplayAnalysis,
    candidate_minus_baseline: ReplayDelta,
    gate_checks: Vec<GateCheck>,
    verdict: &'static str,
    limitations: Vec<&'static str>,
}

/// 本轮实际数据覆盖和完整性证据。
#[derive(Debug, Serialize)]
struct DatasetAudit {
    universe_version: String,
    universe_manifest_sha256: String,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    manifest_evaluation_end_ms: i64,
    evaluation_window_was_clipped: bool,
    expected_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<Value>,
    full_universe_complete: bool,
}

/// 读取并反序列化 JSON，同时保留原始字节供 SHA-256 审计。
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<(T, Vec<u8>)> {
    let bytes = std::fs::read(path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let value =
        serde_json::from_slice(&bytes).with_context(|| format!("解析 {} 失败", path.display()))?;
    Ok((value, bytes))
}

/// 返回字节内容的十六进制 SHA-256。
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 判断候选是否属于本轮严格视觉横盘多空家族。
fn is_strict_visual(candidate: &ReplayCandidateInput) -> bool {
    candidate
        .families
        .iter()
        .any(|family| family == STRICT_LONG_FAMILY || family == STRICT_SHORT_FAMILY)
}

/// 从实际候选账本抽取成本后已平仓交易，并固定账本序号用于删除头部样本。
fn closed_trades(input: &ReplayInput) -> Vec<TradeObservation> {
    input
        .candidate_ledger
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| is_strict_visual(candidate))
        .filter_map(|(ledger_index, candidate)| {
            let outcome = &candidate.outcome;
            if outcome.status != "closed_trade" {
                return None;
            }
            Some(TradeObservation {
                ledger_index,
                identity: SignalIdentity {
                    symbol: candidate.symbol.clone(),
                    direction: candidate.direction.clone(),
                    signal_time_ms: candidate.signal_time_ms,
                },
                event_cluster_id: candidate.event_cluster_id.clone(),
                entry_time_ms: outcome.entry_time_ms?,
                entry_price: outcome.entry_price?,
                exit_time_ms: outcome.exit_time_ms?,
                exit_price: outcome.exit_price?,
                exit_reason: outcome.exit_reason.clone()?,
                frozen_stop: candidate.frozen_risk.stop_price?,
                frozen_target: candidate.frozen_risk.target_price?,
                actual_initial_stop: outcome.actual_initial_stop,
                actual_target: outcome.actual_target_from_frozen_intent,
                exit_policy: candidate.frozen_risk.exit_policy.clone(),
                signal_atr: candidate.time_visible_features.signal_atr,
                range_height: candidate.time_visible_features.strict_visual_range_height,
                range_length_bars: candidate
                    .time_visible_features
                    .strict_visual_range_length_bars,
                net_r: outcome.cost_adjusted_net_r?,
            })
        })
        .collect()
}

/// 按退出完成时间累加成本后 R；同一退出时点先聚合，避免任意排序改变回撤。
fn closed_equity_path(trades: &[TradeObservation]) -> (f64, f64, f64) {
    let mut by_exit = BTreeMap::<i64, f64>::new();
    for trade in trades {
        *by_exit.entry(trade.exit_time_ms).or_default() += trade.net_r;
    }
    let mut cumulative = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    let mut minimum_equity = REFERENCE_STARTING_EQUITY_R;
    for net_r in by_exit.values() {
        cumulative += net_r;
        peak = peak.max(cumulative);
        max_drawdown = max_drawdown.max(peak - cumulative);
        minimum_equity = minimum_equity.min(REFERENCE_STARTING_EQUITY_R + cumulative);
    }
    (
        max_drawdown,
        REFERENCE_STARTING_EQUITY_R + cumulative,
        minimum_equity,
    )
}

/// 汇总任意冻结交易子集的净 EV、PF、回撤和覆盖。
fn summarize(trades: &[TradeObservation]) -> MetricSummary {
    let wins = trades.iter().filter(|trade| trade.net_r > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.net_r < 0.0).count();
    let flats = trades.len().saturating_sub(wins + losses);
    let gross_profit_r: f64 = trades.iter().map(|trade| trade.net_r.max(0.0)).sum();
    let gross_loss_r: f64 = trades.iter().map(|trade| (-trade.net_r).max(0.0)).sum();
    let net_r: f64 = trades.iter().map(|trade| trade.net_r).sum();
    let (max_drawdown, ending_equity, minimum_equity) = closed_equity_path(trades);
    let symbols = trades
        .iter()
        .map(|trade| trade.identity.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let clusters = trades
        .iter()
        .map(|trade| trade.event_cluster_id.as_str())
        .collect::<BTreeSet<_>>();
    MetricSummary {
        trades: trades.len(),
        wins,
        losses,
        flats,
        win_rate_percent: ratio_percent(wins as f64, trades.len() as f64),
        net_r,
        average_net_r: if trades.is_empty() {
            0.0
        } else {
            net_r / trades.len() as f64
        },
        gross_profit_r,
        gross_loss_r,
        profit_factor_r: (gross_loss_r > 0.0).then_some(gross_profit_r / gross_loss_r),
        profit_factor_r_is_infinite: gross_loss_r == 0.0 && gross_profit_r > 0.0,
        chronological_closed_equity_max_drawdown_r: max_drawdown,
        reference_100r_max_drawdown_percent: max_drawdown,
        reference_100r_ending_equity_r: ending_equity,
        reference_100r_minimum_equity_r: minimum_equity,
        reference_equity_breached: minimum_equity <= 0.0,
        distinct_symbols: symbols.len(),
        direction_hour_event_clusters: clusters.len(),
    }
}

/// 计算安全百分比；空分母返回 0 而不是 NaN。
fn ratio_percent(numerator: f64, denominator: f64) -> f64 {
    if denominator > 0.0 {
        numerator / denominator * 100.0
    } else {
        0.0
    }
}

/// 把毫秒时间戳转换为上海时间，供最近亏损直接核图。
fn shanghai_time(timestamp_ms: i64) -> String {
    Utc.timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "invalid".to_owned())
}

/// 返回信号所在上海自然月。
fn shanghai_month(timestamp_ms: i64) -> String {
    Utc.timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
        .map(|time| time.format("%Y-%m").to_string())
        .unwrap_or_else(|| "invalid".to_owned())
}

/// 把内部成交转换为稳定、可序列化的人工核图摘要。
fn trade_summary(trade: &TradeObservation) -> TradeSummary {
    TradeSummary {
        symbol: trade.identity.symbol.clone(),
        direction: trade.identity.direction.clone(),
        signal_time_bjt: shanghai_time(trade.identity.signal_time_ms),
        entry_time_bjt: shanghai_time(trade.entry_time_ms),
        entry_price: trade.entry_price,
        exit_time_bjt: shanghai_time(trade.exit_time_ms),
        exit_price: trade.exit_price,
        exit_reason: trade.exit_reason.clone(),
        frozen_stop: trade.frozen_stop,
        frozen_target: trade.frozen_target,
        actual_initial_stop: trade.actual_initial_stop,
        actual_target: trade.actual_target,
        exit_policy: trade.exit_policy.clone(),
        signal_atr: trade.signal_atr,
        range_height: trade.range_height,
        range_length_bars: trade.range_length_bars,
        cost_adjusted_net_r: trade.net_r,
    }
}

/// 按指定键聚合成交贡献。
fn grouped_contributions(
    trades: &[TradeObservation],
    key: impl Fn(&TradeObservation) -> String,
) -> Vec<Contribution> {
    let mut grouped = BTreeMap::<String, (usize, f64)>::new();
    for trade in trades {
        let entry = grouped.entry(key(trade)).or_default();
        entry.0 += 1;
        entry.1 += trade.net_r;
    }
    grouped
        .into_iter()
        .map(|(key, (trades, net_r))| Contribution {
            key,
            trades,
            net_r,
            average_net_r: net_r / trades as f64,
        })
        .collect()
}

/// 构造收益集中度，不使用未来结果筛选入场，只在回放完成后做稳健性审计。
fn concentration_audit(trades: &[TradeObservation]) -> ConcentrationAudit {
    let mut winners = trades
        .iter()
        .filter(|trade| trade.net_r > 0.0)
        .cloned()
        .collect::<Vec<_>>();
    winners.sort_by(|left, right| {
        right
            .net_r
            .total_cmp(&left.net_r)
            .then_with(|| left.ledger_index.cmp(&right.ledger_index))
    });
    let top_five = winners.into_iter().take(5).collect::<Vec<_>>();
    let removed = top_five
        .iter()
        .map(|trade| trade.ledger_index)
        .collect::<BTreeSet<_>>();
    let without_top_five = trades
        .iter()
        .filter(|trade| !removed.contains(&trade.ledger_index))
        .cloned()
        .collect::<Vec<_>>();
    let gross_profit: f64 = trades.iter().map(|trade| trade.net_r.max(0.0)).sum();
    let top_five_profit: f64 = top_five.iter().map(|trade| trade.net_r).sum();

    let mut symbols = grouped_contributions(trades, |trade| trade.identity.symbol.clone());
    symbols.sort_by(|left, right| {
        right
            .net_r
            .total_cmp(&left.net_r)
            .then_with(|| left.key.cmp(&right.key))
    });
    let best_symbol = symbols.first().map(|symbol| symbol.key.clone());
    let without_best_symbol = trades
        .iter()
        .filter(|trade| best_symbol.as_ref() != Some(&trade.identity.symbol))
        .cloned()
        .collect::<Vec<_>>();

    let mut months = grouped_contributions(trades, |trade| {
        shanghai_month(trade.identity.signal_time_ms)
    });
    months.sort_by(|left, right| left.key.cmp(&right.key));

    let mut clusters = grouped_contributions(trades, |trade| trade.event_cluster_id.clone());
    clusters.sort_by(|left, right| {
        right
            .net_r
            .total_cmp(&left.net_r)
            .then_with(|| left.key.cmp(&right.key))
    });
    let positive_cluster_r: f64 = clusters
        .iter()
        .filter(|cluster| cluster.net_r > 0.0)
        .map(|cluster| cluster.net_r)
        .sum();
    let largest_positive = clusters.iter().find(|cluster| cluster.net_r > 0.0).cloned();
    let largest_share = largest_positive.as_ref().map_or(0.0, |cluster| {
        ratio_percent(cluster.net_r, positive_cluster_r)
    });

    ConcentrationAudit {
        top_five_winning_trades: top_five.iter().map(trade_summary).collect(),
        top_five_winning_gross_profit_share_percent: ratio_percent(top_five_profit, gross_profit),
        after_removing_top_five_winners: summarize(&without_top_five),
        symbol_contributions_descending: symbols,
        best_symbol_removed: best_symbol,
        after_removing_best_symbol: summarize(&without_best_symbol),
        month_contributions_chronological: months,
        event_cluster_count: clusters.len(),
        positive_event_cluster_r: positive_cluster_r,
        largest_positive_event_cluster: largest_positive,
        largest_positive_event_cluster_share_percent: largest_share,
        top_positive_event_clusters: clusters
            .into_iter()
            .filter(|cluster| cluster.net_r > 0.0)
            .take(10)
            .collect(),
    }
}

/// 保守统计同时持仓：同一时间戳的新入场先于退出计入峰值。
fn max_concurrency(trades: &[TradeObservation]) -> usize {
    let mut events = Vec::<(i64, i32)>::with_capacity(trades.len() * 2);
    for trade in trades {
        events.push((trade.entry_time_ms, 1));
        events.push((trade.exit_time_ms, -1));
    }
    events.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)));
    let mut current = 0_i32;
    let mut maximum = 0_i32;
    for (_, delta) in events {
        current += delta;
        maximum = maximum.max(current);
    }
    maximum.max(0) as usize
}

/// 读取冻结输入、完成纯汇总并写出唯一机器结果。
fn main() -> Result<()> {
    let args = Args::parse()?;
    let (baseline_input, baseline_bytes) = read_json::<ReplayInput>(&args.baseline)?;
    let (candidate_input, candidate_bytes) = read_json::<ReplayInput>(&args.candidate)?;
    let (quality_input, quality_bytes) = read_json::<QualityOracleInput>(&args.quality_oracle)?;
    let preregistration_bytes = std::fs::read(&args.preregistration)
        .with_context(|| format!("读取 {} 失败", args.preregistration.display()))?;
    let report = report_evaluation::build_report(
        &baseline_input,
        &candidate_input,
        &quality_input,
        sha256(&baseline_bytes),
        sha256(&candidate_bytes),
        sha256(&quality_bytes),
        sha256(&preregistration_bytes),
    )?;
    let file = File::create(&args.output)
        .with_context(|| format!("创建 {} 失败", args.output.display()))?;
    serde_json::to_writer_pretty(file, &report)?;
    println!("{}", args.output.display());
    Ok(())
}
