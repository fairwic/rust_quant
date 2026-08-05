//! 来源信号极值收回确认的 L2 配对因果回放。
//!
//! 同一批 L1 确认 setup 分别按来源极值被动成交和确认后下一根开盘成交。两侧保持相同
//! ATR 风险公式、目标、成本、最长持仓和保守同棒顺序，以隔离入场政策的成本后差异。

use super::{build_l1_report, frozen_l1_args};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod report;
pub use report::{
    EntryExitLegRecord, SourceExtremeReclaimL2Concentration, SourceExtremeReclaimL2Decision,
    SourceExtremeReclaimL2EntrySummary, SourceExtremeReclaimL2Identity,
    SourceExtremeReclaimL2Performance, SourceExtremeReclaimL2Report,
    SourceExtremeReclaimL2TradeRecord,
};

/// L2 配对回放规则版本；不覆盖 L1 或任何运行态策略。
pub const SOURCE_EXTREME_RECLAIM_L2_RULE_VERSION: &str =
    "l2_source_extreme_reclaim_next_open_paired_v2_risk_v1";

const CANDIDATE_KEY: &str = "market_momentum_bollinger_wick_source_extreme_reclaim_15m_v1";
const L1_RULE_VERSION: &str = "l1_first_retest_close_back_through_source_extreme_next_open_v1";
const EXPECTED_L1_REPORT_SHA256: &str =
    "ab22aa5c485e660cb0dd32baf9d0327eb6927e8a2bb34e089972a0a949546925";
const EXPECTED_L1_CANDIDATE_LEDGER_SHA256: &str =
    "b343346886306d262d422fec9f0c2c5c12c229c5b8b1e0753624460eedcc77fa";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "0c3d1e6ce33187fbc0fd528486d837574fe176b73a748b1f44dedd3c14c328f5";
const EXPECTED_L1_CONFIRMED_SETUPS: usize = 143;
const INITIAL_STOP_ATR_MULTIPLIER: f64 = 1.5;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MS_15M: i64 = 15 * 60 * 1_000;
const MAX_HOLDING_MS: i64 = 48 * 60 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// L1 输入中需要校验的研究身份。
#[derive(Debug, Deserialize)]
struct L1InputIdentity {
    candidate_key: String,
    rule_version: String,
}

/// L1 输入中需要校验的行情身份。
#[derive(Debug, Deserialize)]
struct L1SourceEvidence {
    dataset_fingerprint_sha256: String,
}

/// L1 输入中需要校验的覆盖统计。
#[derive(Debug, Deserialize)]
struct L1InputSummary {
    base_touch_setups: usize,
    confirmed_setups: usize,
}

/// L1 输入必须已经通过且未读取结果标签。
#[derive(Debug, Deserialize)]
struct L1InputDecision {
    status: String,
    outcome_evaluation_performed: bool,
}

/// L2 只消费 L1 确认、入场和风险重建所需的因果字段。
#[derive(Debug, Deserialize)]
struct L1InputCandidate {
    symbol: String,
    setup_ts_ms: i64,
    direction: String,
    source_trigger: String,
    source_extreme_price: f64,
    filtered_volume_ratio: f64,
    first_retest_ts_ms: Option<i64>,
    confirmation_signal_ts_ms: Option<i64>,
    earliest_entry_ts_ms: Option<i64>,
    status: String,
}

/// 冻结 L1 机器报告的最小反序列化合同。
#[derive(Debug, Deserialize)]
struct L1InputReport {
    identity: L1InputIdentity,
    source_evidence: L1SourceEvidence,
    candidate_ledger_sha256: String,
    summary: L1InputSummary,
    decision: L1InputDecision,
    candidates: Vec<L1InputCandidate>,
}

#[derive(Debug, Clone, Copy)]
struct EntryPlan {
    entry_ts_ms: i64,
    entry_idx: usize,
    entry_price: f64,
    stop_price: f64,
    target_price: f64,
}

#[derive(Debug, Clone)]
struct PairedEntry {
    candidate_id: String,
    symbol: String,
    setup_ts_ms: i64,
    first_retest_ts_ms: i64,
    direction: MarketVelocityTradeDirection,
    source_trigger: String,
    source_extreme_price: f64,
    filtered_volume_ratio: f64,
    source_atr14: f64,
    target_atr_multiplier: f64,
    baseline: EntryPlan,
    variant: EntryPlan,
}

#[derive(Debug, Clone, Copy)]
struct ExitPath {
    complete: bool,
    exit_ts_ms: i64,
    exit_price: f64,
    exit_reason: &'static str,
}

/// 加载本地冻结行情，执行配对回放并写出 L2 机器报告。
pub async fn run_source_extreme_reclaim_l2(
    l1_source: &Path,
    output: &Path,
) -> Result<SourceExtremeReclaimL2Report> {
    let (l1_report, l1_report_sha256) = load_and_validate_l1(l1_source)?;
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for source-extreme reclaim L2")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_l2_report(&data, &config.args, l1_report, l1_report_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建来源极值 L2 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入来源极值 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 校验 L1 报告原始 SHA、策略身份、晋级状态和候选数量。
fn load_and_validate_l1(source: &Path) -> Result<(L1InputReport, String)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取来源极值 L1 报告失败：{}", source.display()))?;
    let report_sha256 = sha256_hex(&bytes);
    if report_sha256 != EXPECTED_L1_REPORT_SHA256 {
        bail!("L1 report SHA mismatch");
    }
    let report: L1InputReport = serde_json::from_slice(&bytes).context("解析 L1 报告失败")?;
    if report.identity.candidate_key != CANDIDATE_KEY
        || report.identity.rule_version != L1_RULE_VERSION
    {
        bail!("L1 strategy identity mismatch");
    }
    if report.candidate_ledger_sha256 != EXPECTED_L1_CANDIDATE_LEDGER_SHA256 {
        bail!("L1 candidate ledger SHA mismatch");
    }
    if report.source_evidence.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("L1 dataset fingerprint mismatch");
    }
    if report.decision.status != "coverage_pass_l2_ready"
        || report.decision.outcome_evaluation_performed
    {
        bail!("L1 is not eligible for L2 outcome replay");
    }
    let confirmed = report
        .candidates
        .iter()
        .filter(|candidate| candidate.status == "confirmed_close_back_through_source_extreme")
        .count();
    if report.summary.confirmed_setups != EXPECTED_L1_CONFIRMED_SETUPS
        || confirmed != EXPECTED_L1_CONFIRMED_SETUPS
    {
        bail!("L1 confirmed candidate count mismatch");
    }
    Ok((report, report_sha256))
}

/// 用相同数据重建基础 L1 身份，再执行 pair 解析、共同冲突和绩效统计。
fn build_l2_report(
    data: &BacktestDataSet,
    args: &MarketVelocityEventBacktestArgs,
    l1_report: L1InputReport,
    l1_report_sha256: String,
) -> Result<SourceExtremeReclaimL2Report> {
    let base_report = build_l1_report(data, args)?;
    if base_report.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("reloaded dataset fingerprint mismatch");
    }
    let mut blockers = BTreeMap::new();
    let confirmed = l1_report
        .candidates
        .into_iter()
        .filter(|candidate| candidate.status == "confirmed_close_back_through_source_extreme")
        .collect::<Vec<_>>();
    let mut paired_entries = Vec::with_capacity(confirmed.len());
    for candidate in confirmed {
        match resolve_paired_entry(data, candidate) {
            Ok(entry) => paired_entries.push(entry),
            Err(reason) => *blockers.entry(reason.to_string()).or_default() += 1,
        }
    }
    paired_entries.sort_by(|left, right| {
        (
            left.first_retest_ts_ms,
            left.symbol.as_str(),
            left.setup_ts_ms,
        )
            .cmp(&(
                right.first_retest_ts_ms,
                right.symbol.as_str(),
                right.setup_ts_ms,
            ))
    });
    let resolved_pairs = paired_entries.len();
    let mut trades = simulate_with_shared_conflicts(data, paired_entries, &mut blockers);
    trades.sort_by(|left, right| {
        (
            left.variant.entry_ts_ms,
            left.symbol.as_str(),
            left.setup_ts_ms,
        )
            .cmp(&(
                right.variant.entry_ts_ms,
                right.symbol.as_str(),
                right.setup_ts_ms,
            ))
    });
    let completed = trades
        .iter()
        .filter(|trade| trade.baseline.complete && trade.variant.complete)
        .collect::<Vec<_>>();
    let baseline = performance(completed.iter().map(|trade| trade.baseline.net_r));
    let variant = performance(completed.iter().map(|trade| trade.variant.net_r));
    let baseline_by_direction = performance_by_direction(&completed, false);
    let variant_by_direction = performance_by_direction(&completed, true);
    let concentration = concentration(&completed);
    let paired_contract_identity_verified = trades.iter().all(contract_is_consistent);
    let entry_summary = entry_summary(
        l1_report.summary.base_touch_setups,
        l1_report.summary.confirmed_setups,
        resolved_pairs,
        &trades,
        &completed,
        blockers,
    );
    let decision = decide_l2(
        &entry_summary,
        &baseline,
        &variant,
        &concentration,
        paired_contract_identity_verified,
    );

    Ok(SourceExtremeReclaimL2Report {
        schema_version: "momentum_bollinger_source_extreme_reclaim_l2_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: SourceExtremeReclaimL2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: CANDIDATE_KEY,
            rule_version: SOURCE_EXTREME_RECLAIM_L2_RULE_VERSION,
            only_variable: "replace the paired source-V2 passive source-extreme fill with the confirmed next-15m-open entry on the same L1-confirmed setups",
            baseline_entry_policy: "fill at frozen source setup extreme on the first-retest candle",
            variant_entry_policy: "after first-retest close confirmation, fill at the next existing 15m candle open",
            initial_stop_policy: "each side reanchors the unchanged V2 1.5 times source-setup ATR14 risk distance to its actual entry",
            target_policy: "each side reanchors the unchanged V2 2.7/3.6/4.5 source-setup ATR14 target distance to its actual entry",
            intrabar_conflict_policy: "stop first when stop and target are both touched in one candle",
            paired_position_conflict_policy: "both sides share one symbol lock ending at the later of the paired exits; a conflict drops both sides",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            outcome_evaluation_performed: true,
        },
        source_l1_report_sha256: l1_report_sha256,
        source_l1_candidate_ledger_sha256: l1_report.candidate_ledger_sha256,
        dataset_fingerprint_sha256: base_report.coverage.dataset_fingerprint_sha256,
        returned_symbol_count: base_report.coverage.returned_symbol_count,
        eligible_symbol_count: base_report.coverage.eligible_symbol_count,
        excluded_symbol_count: base_report.coverage.excluded_symbols.len(),
        entry_summary,
        baseline,
        variant,
        baseline_by_direction,
        variant_by_direction,
        concentration,
        paired_contract_identity_verified,
        decision,
        trades,
    })
}

/// 从 setup ATR、首次重测极值和下一根开盘构造一组因果入场计划。
fn resolve_paired_entry(
    data: &BacktestDataSet,
    candidate: L1InputCandidate,
) -> Result<PairedEntry, &'static str> {
    let candles = data
        .candles_15m_computed
        .get(&candidate.symbol)
        .ok_or("symbol_candles_missing")?;
    let setup_idx = candles
        .binary_search_by_key(&candidate.setup_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "setup_candle_missing")?;
    let setup = candles.get(setup_idx).ok_or("setup_candle_missing")?;
    let source_atr14 = setup
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("source_atr14_invalid")?;
    let first_retest_ts_ms = candidate
        .first_retest_ts_ms
        .ok_or("confirmed_candidate_missing_first_retest")?;
    if candidate.confirmation_signal_ts_ms != Some(first_retest_ts_ms) {
        return Err("confirmation_timestamp_mismatch");
    }
    let baseline_idx = candles
        .binary_search_by_key(&first_retest_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "first_retest_candle_missing")?;
    let baseline_candle = candles
        .get(baseline_idx)
        .ok_or("first_retest_candle_missing")?;
    let variant_ts_ms = candidate
        .earliest_entry_ts_ms
        .ok_or("confirmed_candidate_missing_next_open_timestamp")?;
    let variant_idx = candles
        .binary_search_by_key(&variant_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "next_open_candle_missing")?;
    if variant_idx != baseline_idx.saturating_add(1) {
        return Err("next_open_not_immediately_after_confirmation");
    }
    let variant_candle = candles.get(variant_idx).ok_or("next_open_candle_missing")?;
    let direction = parse_direction(&candidate.direction)?;
    if !directional_extreme_touched(baseline_candle, candidate.source_extreme_price, direction) {
        return Err("first_retest_does_not_touch_source_extreme");
    }
    let target_atr_multiplier = target_atr_multiplier(candidate.filtered_volume_ratio)
        .ok_or("source_target_tier_invalid")?;
    let baseline = entry_plan(
        first_retest_ts_ms,
        baseline_idx,
        candidate.source_extreme_price,
        source_atr14,
        target_atr_multiplier,
        direction,
    )?;
    let variant = entry_plan(
        variant_ts_ms,
        variant_idx,
        variant_candle.candle.open,
        source_atr14,
        target_atr_multiplier,
        direction,
    )?;
    Ok(PairedEntry {
        candidate_id: format!("{}:{}", candidate.symbol, candidate.setup_ts_ms),
        symbol: candidate.symbol,
        setup_ts_ms: candidate.setup_ts_ms,
        first_retest_ts_ms,
        direction,
        source_trigger: candidate.source_trigger,
        source_extreme_price: candidate.source_extreme_price,
        filtered_volume_ratio: candidate.filtered_volume_ratio,
        source_atr14,
        target_atr_multiplier,
        baseline,
        variant,
    })
}

/// 以实际入场价重锚相同 ATR 风险与目标距离。
fn entry_plan(
    entry_ts_ms: i64,
    entry_idx: usize,
    entry_price: f64,
    atr14: f64,
    target_atr: f64,
    direction: MarketVelocityTradeDirection,
) -> Result<EntryPlan, &'static str> {
    if !entry_price.is_finite() || entry_price <= 0.0 {
        return Err("entry_price_invalid");
    }
    let (stop_price, target_price) = match direction {
        MarketVelocityTradeDirection::Long => (
            entry_price - INITIAL_STOP_ATR_MULTIPLIER * atr14,
            entry_price + target_atr * atr14,
        ),
        MarketVelocityTradeDirection::Short => (
            entry_price + INITIAL_STOP_ATR_MULTIPLIER * atr14,
            entry_price - target_atr * atr14,
        ),
        MarketVelocityTradeDirection::Both => return Err("candidate_direction_invalid"),
    };
    if !stop_price.is_finite()
        || stop_price <= 0.0
        || !target_price.is_finite()
        || target_price <= 0.0
    {
        return Err("risk_or_target_price_invalid");
    }
    Ok(EntryPlan {
        entry_ts_ms,
        entry_idx,
        entry_price,
        stop_price,
        target_price,
    })
}

/// 按币种应用一个共同锁；任一侧仍持仓时，下一 pair 在两侧同时被拒绝。
fn simulate_with_shared_conflicts(
    data: &BacktestDataSet,
    entries: Vec<PairedEntry>,
    blockers: &mut BTreeMap<String, usize>,
) -> Vec<SourceExtremeReclaimL2TradeRecord> {
    let mut by_symbol: BTreeMap<String, Vec<PairedEntry>> = BTreeMap::new();
    for entry in entries {
        by_symbol
            .entry(entry.symbol.clone())
            .or_default()
            .push(entry);
    }
    let mut records = Vec::new();
    for (symbol, mut symbol_entries) in by_symbol {
        symbol_entries.sort_by_key(|entry| (entry.first_retest_ts_ms, entry.setup_ts_ms));
        let Some(candles) = data.candles_15m_computed.get(&symbol) else {
            *blockers
                .entry("symbol_candles_missing_during_replay".to_owned())
                .or_default() += symbol_entries.len();
            continue;
        };
        let mut locked_until = i64::MIN;
        for entry in symbol_entries {
            if entry.baseline.entry_ts_ms <= locked_until
                || entry.variant.entry_ts_ms <= locked_until
            {
                *blockers
                    .entry("pair_ignored_while_shared_symbol_lock_open".to_owned())
                    .or_default() += 1;
                continue;
            }
            let baseline_path = simulate_exit(candles, entry.baseline, entry.direction);
            let variant_path = simulate_exit(candles, entry.variant, entry.direction);
            let (Some(baseline_path), Some(variant_path)) = (baseline_path, variant_path) else {
                *blockers
                    .entry("entry_candle_missing_during_replay".to_owned())
                    .or_default() += 1;
                continue;
            };
            locked_until = baseline_path.exit_ts_ms.max(variant_path.exit_ts_ms);
            records.push(build_trade_record(&entry, baseline_path, variant_path));
        }
    }
    records
}

/// 从实际入场 K 开始执行止损优先、原目标和 48 小时超时。
fn simulate_exit(
    candles: &[ComputedCandle],
    plan: EntryPlan,
    direction: MarketVelocityTradeDirection,
) -> Option<ExitPath> {
    let horizon_end = plan.entry_ts_ms.saturating_add(MAX_HOLDING_MS);
    let mut last_seen = None;
    for candle in candles.get(plan.entry_idx..)? {
        if candle.candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let stop_hit = loss_level_touched(candle, plan.stop_price, direction);
        let target_hit = profit_level_touched(candle, plan.target_price, direction);
        if stop_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: plan.stop_price,
                exit_reason: if target_hit {
                    "both_hit_stop_first"
                } else {
                    "stop_hit"
                },
            });
        }
        if target_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: plan.target_price,
                exit_reason: "original_target_hit",
            });
        }
        if candle.candle.ts >= horizon_end {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: candle.candle.close,
                exit_reason: "max_holding_timeout",
            });
        }
    }
    let last = last_seen?;
    Some(ExitPath {
        complete: false,
        exit_ts_ms: last.candle.ts,
        exit_price: last.candle.close,
        exit_reason: "forward_data_incomplete",
    })
}

/// 将两侧路径折算到各自相同 ATR 风险分母并形成配对记录。
fn build_trade_record(
    entry: &PairedEntry,
    baseline_path: ExitPath,
    variant_path: ExitPath,
) -> SourceExtremeReclaimL2TradeRecord {
    let baseline = leg_record(entry.baseline, baseline_path, entry.direction);
    let variant = leg_record(entry.variant, variant_path, entry.direction);
    SourceExtremeReclaimL2TradeRecord {
        candidate_id: entry.candidate_id.clone(),
        symbol: entry.symbol.clone(),
        setup_ts_ms: entry.setup_ts_ms,
        first_retest_ts_ms: entry.first_retest_ts_ms,
        direction: direction_label(entry.direction),
        source_trigger: entry.source_trigger.clone(),
        source_extreme_price: entry.source_extreme_price,
        filtered_volume_ratio: entry.filtered_volume_ratio,
        source_atr14: entry.source_atr14,
        target_atr_multiplier: entry.target_atr_multiplier,
        delta_net_r: variant.net_r - baseline.net_r,
        baseline,
        variant,
    }
}

/// 构造一侧完整入场、退出、风险与成本后 R 证据。
fn leg_record(
    plan: EntryPlan,
    path: ExitPath,
    direction: MarketVelocityTradeDirection,
) -> EntryExitLegRecord {
    let risk = (plan.entry_price - plan.stop_price).abs();
    EntryExitLegRecord {
        entry_ts_ms: plan.entry_ts_ms,
        entry_price: plan.entry_price,
        initial_stop_price: plan.stop_price,
        initial_risk_price: risk,
        target_price: plan.target_price,
        complete: path.complete,
        exit_ts_ms: path.exit_ts_ms,
        exit_price: path.exit_price,
        exit_reason: path.exit_reason,
        net_r: net_r(plan.entry_price, path.exit_price, risk, direction),
    }
}

/// 汇总配对覆盖和共同冲突后的完整样本分散性。
fn entry_summary(
    base_touch_setups: usize,
    l1_confirmed_setups: usize,
    resolved_pairs: usize,
    trades: &[SourceExtremeReclaimL2TradeRecord],
    completed: &[&SourceExtremeReclaimL2TradeRecord],
    blockers: BTreeMap<String, usize>,
) -> SourceExtremeReclaimL2EntrySummary {
    let completed_long = completed
        .iter()
        .filter(|trade| trade.direction == "long")
        .count();
    let completed_short = completed
        .iter()
        .filter(|trade| trade.direction == "short")
        .count();
    let symbols = completed
        .iter()
        .map(|trade| trade.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let months = completed
        .iter()
        .filter_map(|trade| {
            Utc.timestamp_millis_opt(trade.variant.entry_ts_ms)
                .single()
                .map(|value| value.format("%Y-%m").to_string())
        })
        .collect::<BTreeSet<_>>()
        .len();
    SourceExtremeReclaimL2EntrySummary {
        base_touch_setups,
        l1_confirmed_setups,
        resolved_pairs,
        executed_pairs: trades.len(),
        completed_pairs: completed.len(),
        incomplete_pairs: trades.len().saturating_sub(completed.len()),
        completed_by_direction: BTreeMap::from([
            ("long", completed_long),
            ("short", completed_short),
        ]),
        completed_symbol_count: symbols,
        completed_month_count: months,
        completed_effective_market_events: effective_market_event_count(completed),
        blockers,
    }
}

/// 计算一组按候选入场时间排序的成本后交易级指标。
fn performance(values: impl Iterator<Item = f64>) -> SourceExtremeReclaimL2Performance {
    let values = values.collect::<Vec<_>>();
    let trades = values.len();
    let positive_net_r = values.iter().copied().filter(|value| *value > 0.0).sum();
    let negative_net_r_abs = -values
        .iter()
        .copied()
        .filter(|value| *value < 0.0)
        .sum::<f64>();
    let net_sum_r = values.iter().sum::<f64>();
    SourceExtremeReclaimL2Performance {
        trades,
        positive_net_r,
        negative_net_r_abs,
        net_sum_r,
        net_expectancy_r: if trades == 0 {
            0.0
        } else {
            net_sum_r / trades as f64
        },
        net_profit_factor: (negative_net_r_abs > 0.0)
            .then_some(positive_net_r / negative_net_r_abs),
        win_rate_pct: if trades == 0 {
            0.0
        } else {
            values.iter().filter(|value| **value > 0.0).count() as f64 / trades as f64 * 100.0
        },
        trade_sharpe: trade_sharpe(&values),
        max_drawdown_r: max_drawdown_r(&values),
    }
}

/// 分别报告多空指标，避免一个方向掩盖另一个方向的退化。
fn performance_by_direction(
    trades: &[&SourceExtremeReclaimL2TradeRecord],
    variant: bool,
) -> BTreeMap<&'static str, SourceExtremeReclaimL2Performance> {
    ["long", "short"]
        .into_iter()
        .map(|direction| {
            let metrics = performance(
                trades
                    .iter()
                    .filter(|trade| trade.direction == direction)
                    .map(|trade| {
                        if variant {
                            trade.variant.net_r
                        } else {
                            trade.baseline.net_r
                        }
                    }),
            );
            (direction, metrics)
        })
        .collect()
}

/// 汇总配对净增量的交易、币种与方向集中度。
fn concentration(
    trades: &[&SourceExtremeReclaimL2TradeRecord],
) -> SourceExtremeReclaimL2Concentration {
    let mut deltas = trades
        .iter()
        .map(|trade| trade.delta_net_r)
        .collect::<Vec<_>>();
    deltas.sort_by(|left, right| right.total_cmp(left));
    let total_delta_net_r = deltas.iter().sum::<f64>();
    let top_two = deltas.iter().take(2).sum::<f64>();
    let mut delta_net_r_by_symbol = BTreeMap::new();
    let mut delta_net_r_by_direction = BTreeMap::from([("long", 0.0), ("short", 0.0)]);
    let mut positive_by_symbol = BTreeMap::new();
    let mut total_positive_delta = 0.0;
    for trade in trades {
        *delta_net_r_by_symbol
            .entry(trade.symbol.clone())
            .or_default() += trade.delta_net_r;
        *delta_net_r_by_direction.entry(trade.direction).or_default() += trade.delta_net_r;
        if trade.delta_net_r > 0.0 {
            *positive_by_symbol.entry(trade.symbol.clone()).or_default() += trade.delta_net_r;
            total_positive_delta += trade.delta_net_r;
        }
    }
    let max_symbol_positive_delta_share_pct = (total_positive_delta > 0.0).then(|| {
        positive_by_symbol.values().copied().fold(0.0_f64, f64::max) / total_positive_delta * 100.0
    });
    SourceExtremeReclaimL2Concentration {
        total_delta_net_r,
        delta_net_r_after_removing_top_two_trades: total_delta_net_r - top_two,
        max_symbol_positive_delta_share_pct,
        delta_net_r_by_symbol,
        delta_net_r_by_direction,
    }
}

/// 应用预注册的覆盖、成本后边际、方向和集中度联合门禁。
fn decide_l2(
    summary: &SourceExtremeReclaimL2EntrySummary,
    baseline: &SourceExtremeReclaimL2Performance,
    variant: &SourceExtremeReclaimL2Performance,
    concentration: &SourceExtremeReclaimL2Concentration,
    paired_contract_identity_verified: bool,
) -> SourceExtremeReclaimL2Decision {
    let long_count = summary
        .completed_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .completed_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let long_delta = concentration
        .delta_net_r_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_delta = concentration
        .delta_net_r_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert("l1_identity_and_dataset_verified", true);
    gates.insert(
        "l1_confirmed_setups_equal_143",
        summary.l1_confirmed_setups == 143,
    );
    gates.insert("resolved_pairs_at_least_30", summary.resolved_pairs >= 30);
    gates.insert("completed_pairs_at_least_30", summary.completed_pairs >= 30);
    gates.insert(
        "completed_both_directions_at_least_5",
        long_count >= 5 && short_count >= 5,
    );
    gates.insert(
        "completed_symbols_at_least_8",
        summary.completed_symbol_count >= 8,
    );
    gates.insert(
        "completed_months_at_least_6",
        summary.completed_month_count >= 6,
    );
    gates.insert(
        "completed_effective_events_at_least_15",
        summary.completed_effective_market_events >= 15,
    );
    gates.insert(
        "variant_expectancy_positive_and_above_baseline",
        variant.net_expectancy_r > 0.0 && variant.net_expectancy_r > baseline.net_expectancy_r,
    );
    gates.insert(
        "variant_profit_factor_not_below_baseline",
        profit_factor_not_worse(baseline, variant),
    );
    gates.insert(
        "both_direction_delta_non_negative",
        long_delta >= 0.0 && short_delta >= 0.0,
    );
    gates.insert(
        "total_delta_net_r_positive",
        concentration.total_delta_net_r > 0.0,
    );
    gates.insert(
        "delta_positive_after_removing_top_two",
        concentration.delta_net_r_after_removing_top_two_trades > 0.0,
    );
    gates.insert(
        "max_symbol_positive_delta_share_at_most_35_pct",
        concentration
            .max_symbol_positive_delta_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert(
        "paired_contract_identity_verified",
        paired_contract_identity_verified,
    );
    let passed = gates.values().all(|value| *value);
    SourceExtremeReclaimL2Decision {
        status: if passed {
            "L2_pass_L3_required"
        } else {
            "stop"
        },
        reason: if passed {
            "来源极值收回后的下一根开盘入场在配对成本后诊断中产生分散正边际；仍需 L3 的 point-in-time、OOS、统一资金和压力验证。"
                .to_owned()
        } else {
            "至少一项预注册 L2 门禁失败；停止该入场版本，不得继续叠加中轨减仓、保本或对侧外轨/5R。"
                .to_owned()
        },
        gates,
    }
}

/// 检查两侧确实共享 setup、ATR 风险距离与冻结退出公式。
fn contract_is_consistent(trade: &SourceExtremeReclaimL2TradeRecord) -> bool {
    let expected_risk = trade.source_atr14 * INITIAL_STOP_ATR_MULTIPLIER;
    let expected_target_distance = trade.source_atr14 * trade.target_atr_multiplier;
    approx_equal(trade.baseline.initial_risk_price, expected_risk)
        && approx_equal(trade.variant.initial_risk_price, expected_risk)
        && approx_equal(
            (trade.baseline.target_price - trade.baseline.entry_price).abs(),
            expected_target_distance,
        )
        && approx_equal(
            (trade.variant.target_price - trade.variant.entry_price).abs(),
            expected_target_distance,
        )
        && approx_equal(trade.baseline.entry_price, trade.source_extreme_price)
        && trade.variant.entry_ts_ms == trade.first_retest_ts_ms.saturating_add(MS_15M)
        && trade.baseline.entry_ts_ms == trade.first_retest_ts_ms
        && trade.baseline.exit_reason != ""
        && trade.variant.exit_reason != ""
}

/// 按候选入场时间和方向归并一小时内的跨币共振事件。
fn effective_market_event_count(trades: &[&SourceExtremeReclaimL2TradeRecord]) -> usize {
    let mut ordered = trades.to_vec();
    ordered.sort_by_key(|trade| {
        (
            trade.variant.entry_ts_ms,
            trade.direction,
            trade.symbol.as_str(),
        )
    });
    let mut last_by_direction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut count = 0;
    for trade in ordered {
        let starts_new = last_by_direction
            .get(trade.direction)
            .is_none_or(|previous| trade.variant.entry_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(trade.direction, trade.variant.entry_ts_ms);
    }
    count
}

/// 来源 V2 量比分档到目标 ATR 倍数的冻结映射。
fn target_atr_multiplier(filtered_volume_ratio: f64) -> Option<f64> {
    if !filtered_volume_ratio.is_finite() || filtered_volume_ratio < 2.5 {
        return None;
    }
    if filtered_volume_ratio < 4.0 {
        Some(2.7)
    } else if filtered_volume_ratio < 6.0 {
        Some(3.6)
    } else {
        Some(4.5)
    }
}

/// 将冻结字符串方向转换为现有回放方向枚举。
fn parse_direction(value: &str) -> Result<MarketVelocityTradeDirection, &'static str> {
    match value {
        "long" => Ok(MarketVelocityTradeDirection::Long),
        "short" => Ok(MarketVelocityTradeDirection::Short),
        _ => Err("candidate_direction_invalid"),
    }
}

/// 将方向枚举写回稳定机器账本标签。
fn direction_label(direction: MarketVelocityTradeDirection) -> &'static str {
    match direction {
        MarketVelocityTradeDirection::Long => "long",
        MarketVelocityTradeDirection::Short => "short",
        MarketVelocityTradeDirection::Both => "both",
    }
}

/// 核对首次重测 K 是否实际触及冻结来源方向极值。
fn directional_extreme_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.low <= price,
        MarketVelocityTradeDirection::Short => candle.candle.high >= price,
        MarketVelocityTradeDirection::Both => false,
    }
}

/// 判断止损价在当前 K 内是否可达。
fn loss_level_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.low <= price,
        MarketVelocityTradeDirection::Short => candle.candle.high >= price,
        MarketVelocityTradeDirection::Both => true,
    }
}

/// 判断盈利目标价在当前 K 内是否可达。
fn profit_level_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.high >= price,
        MarketVelocityTradeDirection::Short => candle.candle.low <= price,
        MarketVelocityTradeDirection::Both => false,
    }
}

/// 按实际开平名义价格扣除双边成本并折算到入场初始风险 R。
fn net_r(
    entry_price: f64,
    exit_price: f64,
    risk: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    let gross = match direction {
        MarketVelocityTradeDirection::Long => (exit_price - entry_price) / risk,
        MarketVelocityTradeDirection::Short => (entry_price - exit_price) / risk,
        MarketVelocityTradeDirection::Both => 0.0,
    };
    gross - (entry_price + exit_price) * PER_SIDE_COST_RATE / risk
}

/// 计算逐笔 R 的交易级 Sharpe；不足两笔或零方差时为空。
fn trade_sharpe(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    (variance > 0.0).then_some(mean / variance.sqrt() * (values.len() as f64).sqrt())
}

/// 计算按候选入场顺序累计净 R 的最大回撤。
fn max_drawdown_r(values: &[f64]) -> f64 {
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    max_drawdown
}

/// 对有无负交易两种情况比较 Profit Factor，避免无穷值被误判。
fn profit_factor_not_worse(
    baseline: &SourceExtremeReclaimL2Performance,
    variant: &SourceExtremeReclaimL2Performance,
) -> bool {
    match (baseline.net_profit_factor, variant.net_profit_factor) {
        (_, None) if variant.negative_net_r_abs == 0.0 => true,
        (None, Some(_)) if baseline.negative_net_r_abs == 0.0 => false,
        (Some(base), Some(candidate)) => candidate >= base,
        _ => false,
    }
}

/// 浮点公式校验使用相对误差，避免不同价格量级下固定 epsilon 失真。
fn approx_equal(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 1e-10
}

/// 生成输入机器报告的 SHA-256 十六进制身份。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests;
