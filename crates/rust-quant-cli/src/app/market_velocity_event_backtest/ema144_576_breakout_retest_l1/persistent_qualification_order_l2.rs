//! EMA144/576 永久历史资格回踩的 L2 多币种成本回放。
//!
//! V6 使用当前 15m 动量的 0.52R 目标；V9 只把目标改为 2.0R。两者共享相同的
//! 4% 止损、24 小时持仓与成交合同，且都不注册 Paper、ReadOnly、Live 或生产策略。

mod report;
mod stable_panel_v12;
mod structure_target_v13;
pub use report::*;
pub use stable_panel_v12::run_stable_panel_v12_l2_replay;
pub use structure_target_v13::run_structure_target_v13_l2_replay;

use super::frozen_l1_args;
use super::persistent_dynamic_retest_v2::{build_v6_l1_report, V6_CANDIDATE_KEY, V6_RULE_VERSION};
use super::target2r_v9::{V9_CANDIDATE_KEY, V9_L1_RULE_VERSION, V9_L2_RULE_VERSION};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// V6 L2 的冻结成交和风险版本；任何参数变化都必须创建新研究身份。
pub const L2_RULE_VERSION: &str = "l2_touch_limit_sl04_r052_hold24h_cost8bps_v1";

const EXPECTED_L1_REPORT_SHA256: &str =
    "a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_L1_CANDIDATES: usize = 54_837;
const STOP_LOSS_PCT: f64 = 0.04;
const V6_TARGET_R: f64 = 0.52;
const V9_TARGET_R: f64 = 2.0;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const EXPECTED_V9_L1_AUTHORIZATION_SHA256: &str =
    "31dbd99af9d9a0cc42b659eb99ef3038a22596dd15712adfa3671b57b2128769";

/// 同一回放器允许的冻结研究身份；目标倍数不能来自命令行。
#[derive(Debug, Clone, Copy)]
struct ReplayVariant {
    schema_version: &'static str,
    candidate_key: &'static str,
    source_l1_rule_version: &'static str,
    expected_l1_candidates: usize,
    rule_version: &'static str,
    only_variable: &'static str,
    target_policy: &'static str,
    target_r: f64,
    runtime_boundary: &'static str,
}

const V6_REPLAY: ReplayVariant = ReplayVariant {
    schema_version: "market_momentum_ema144_576_persistent_order_retest_l2_v1",
    candidate_key: V6_CANDIDATE_KEY,
    source_l1_rule_version: V6_RULE_VERSION,
    expected_l1_candidates: EXPECTED_L1_CANDIDATES,
    rule_version: L2_RULE_VERSION,
    only_variable: "add the frozen V6 EMA144/576 persistent-qualification retest entry to the unchanged 15m momentum risk and exit contract",
    target_policy: "fixed 0.52R from actual entry price with no protection, trailing, partial, or runner",
    target_r: V6_TARGET_R,
    runtime_boundary: "research-only V6 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
};

const V9_REPLAY: ReplayVariant = ReplayVariant {
    schema_version: "market_momentum_ema144_576_persistent_retest_target2r_l2_v9",
    candidate_key: V9_CANDIDATE_KEY,
    source_l1_rule_version: V6_RULE_VERSION,
    expected_l1_candidates: EXPECTED_L1_CANDIDATES,
    rule_version: V9_L2_RULE_VERSION,
    only_variable: "change only the fixed target from 0.52R to 2.0R while preserving the complete V6 entry, stop, holding, cost, conflict, and universe contracts",
    target_policy: "fixed 2.0R from actual entry price with no protection, trailing, partial, or runner",
    target_r: V9_TARGET_R,
    runtime_boundary: "research-only V9 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
};

#[derive(Debug, Deserialize)]
struct V9AuthorizationIdentity {
    candidate_key: String,
    rule_version: String,
    source_candidate_key: String,
    source_l1_rule_version: String,
}

#[derive(Debug, Deserialize)]
struct V9AuthorizationSummary {
    source_candidates: usize,
    valid_geometry_candidates: usize,
    invalid_geometry_candidates: usize,
}

#[derive(Debug, Deserialize)]
struct V9AuthorizationDecision {
    status: String,
    outcome_evaluation_performed: bool,
}

#[derive(Debug, Deserialize)]
struct V9AuthorizationReport {
    identity: V9AuthorizationIdentity,
    source_l1_report_sha256: String,
    dataset_fingerprint_sha256: String,
    summary: V9AuthorizationSummary,
    decision: V9AuthorizationDecision,
}

#[derive(Debug, Deserialize)]
struct L1InputIdentity {
    candidate_key: String,
    rule_version: String,
}

#[derive(Debug, Deserialize)]
struct L1InputCoverage {
    returned_symbol_count: usize,
    eligible_symbol_count: usize,
    excluded_symbols: Vec<serde_json::Value>,
    dataset_fingerprint_sha256: String,
}

#[derive(Debug, Deserialize)]
struct L1InputSummary {
    candidate_count: usize,
}

#[derive(Debug, Deserialize)]
struct L1InputDecision {
    status: String,
    outcome_evaluation_performed: bool,
}

/// L2 只读取成交所需的 L1 信号时字段；其余诊断字段由 serde 忽略。
#[derive(Debug, Clone, Deserialize)]
struct L1InputCandidate {
    symbol: String,
    direction: String,
    signal_ts_ms: i64,
    anchor_ema144: f64,
    anchor_atr14: f64,
    touch_zone_boundary: f64,
}

#[derive(Debug, Deserialize)]
struct L1InputReport {
    identity: L1InputIdentity,
    coverage: L1InputCoverage,
    summary: L1InputSummary,
    decision: L1InputDecision,
    candidates: Vec<L1InputCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum L2Direction {
    Long,
    Short,
}

impl L2Direction {
    fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }
}

#[derive(Debug, Clone)]
struct EntryPlan {
    candidate_id: String,
    symbol: String,
    direction: L2Direction,
    signal_ts_ms: i64,
    entry_idx: usize,
    anchor_ema144: f64,
    anchor_atr14: f64,
    limit_price: f64,
    entry_price: f64,
    stop_price: f64,
    target_price: f64,
}

#[derive(Debug, Clone, Copy)]
struct ExitPath {
    complete: bool,
    exit_ts_ms: i64,
    exit_price: f64,
    exit_reason: &'static str,
}

/// 校验冻结 L1，重载同一行情并生成原始 V6 L2 机器报告。
pub async fn run_l2_replay(l1_source: &Path, output: &Path) -> Result<EmaRetestL2Report> {
    run_replay(l1_source, output, V6_REPLAY).await
}

/// 同时校验 V6 入场账本与 V9 L1 授权，再执行唯一改为 2.0R 的成本回放。
pub async fn run_target2r_l2_replay(
    l1_source: &Path,
    v9_l1_authorization: &Path,
    output: &Path,
) -> Result<EmaRetestL2Report> {
    validate_v9_l1_authorization(v9_l1_authorization)?;
    run_replay(l1_source, output, V9_REPLAY).await
}

/// 冻结变体在进入数据库前已经确定，避免结果回放期间接受目标倍数调参。
async fn run_replay(
    l1_source: &Path,
    output: &Path,
    variant: ReplayVariant,
) -> Result<EmaRetestL2Report> {
    let (l1, source_l1_report_sha256) = load_and_validate_l1(l1_source)?;
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 L2 replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_l2_report(&data, l1, source_l1_report_sha256, variant)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 EMA144/576 L2 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// V9 必须消费精确的无标签 L1 几何授权，不能凭调用方口头指定 2R。
fn validate_v9_l1_authorization(source: &Path) -> Result<()> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 EMA144/576 V9 L1 授权失败：{}", source.display()))?;
    if sha256_hex(&bytes) != EXPECTED_V9_L1_AUTHORIZATION_SHA256 {
        bail!("V9 L1 authorization SHA mismatch");
    }
    let report: V9AuthorizationReport =
        serde_json::from_slice(&bytes).context("解析 EMA144/576 V9 L1 授权失败")?;
    if report.identity.candidate_key != V9_CANDIDATE_KEY
        || report.identity.rule_version != V9_L1_RULE_VERSION
        || report.identity.source_candidate_key != V6_CANDIDATE_KEY
        || report.identity.source_l1_rule_version != V6_RULE_VERSION
        || report.source_l1_report_sha256 != EXPECTED_L1_REPORT_SHA256
        || report.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || report.summary.source_candidates != EXPECTED_L1_CANDIDATES
        || report.summary.valid_geometry_candidates != EXPECTED_L1_CANDIDATES
        || report.summary.invalid_geometry_candidates != 0
        || report.decision.status != "coverage_pass_ready_for_l2_prereg"
        || report.decision.outcome_evaluation_performed
    {
        bail!("V9 L1 authorization identity or geometry gate mismatch");
    }
    Ok(())
}

/// 用完整文件 SHA 和 L1 研究身份阻止结果回放漂移到另一份候选账本。
fn load_and_validate_l1(source: &Path) -> Result<(L1InputReport, String)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 EMA144/576 V6 L1 报告失败：{}", source.display()))?;
    let report_sha256 = sha256_hex(&bytes);
    if report_sha256 != EXPECTED_L1_REPORT_SHA256 {
        bail!("V6 L1 report SHA mismatch");
    }
    let report: L1InputReport =
        serde_json::from_slice(&bytes).context("解析 EMA144/576 V6 L1 报告失败")?;
    if report.identity.candidate_key != V6_CANDIDATE_KEY
        || report.identity.rule_version != V6_RULE_VERSION
    {
        bail!("V6 L1 strategy identity mismatch");
    }
    if report.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("V6 L1 dataset fingerprint mismatch");
    }
    if report.summary.candidate_count != EXPECTED_L1_CANDIDATES
        || report.candidates.len() != EXPECTED_L1_CANDIDATES
    {
        bail!("V6 L1 candidate count mismatch");
    }
    if report.decision.status != "coverage_pass_ready_for_l2_prereg"
        || report.decision.outcome_evaluation_performed
    {
        bail!("V6 L1 is not eligible for outcome replay");
    }
    Ok((report, report_sha256))
}

/// 重建 L1 数据身份后，按冻结限价、同币种锁和退出合同执行成本诊断。
fn build_l2_report(
    data: &BacktestDataSet,
    l1: L1InputReport,
    source_l1_report_sha256: String,
    variant: ReplayVariant,
) -> Result<EmaRetestL2Report> {
    let rebuilt_l1 = build_v6_l1_report(data)?;
    if rebuilt_l1.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || rebuilt_l1.summary.candidate_count != EXPECTED_L1_CANDIDATES
    {
        bail!("reloaded V6 L1 identity mismatch");
    }
    let mut blockers = BTreeMap::new();
    let mut entries = Vec::with_capacity(l1.candidates.len());
    for candidate in l1.candidates {
        match resolve_entry(data, candidate, variant.target_r) {
            Ok(entry) => entries.push(entry),
            Err(reason) => *blockers.entry(reason.to_owned()).or_default() += 1,
        }
    }
    let resolved_candidates = entries.len();
    let mut trades = simulate_with_symbol_lock(data, entries, &mut blockers);
    trades.sort_by(|left, right| {
        (left.signal_ts_ms, left.symbol.as_str(), left.direction).cmp(&(
            right.signal_ts_ms,
            right.symbol.as_str(),
            right.direction,
        ))
    });
    assign_event_clusters(&mut trades);
    let completed = trades
        .iter()
        .filter(|trade| trade.complete)
        .collect::<Vec<_>>();
    let gross = performance(completed.iter().map(|trade| trade.gross_r));
    let net = performance(completed.iter().map(|trade| trade.net_r));
    let net_by_direction = performance_by_direction(&completed);
    let concentration = concentration(&completed);
    let contract_identity_verified = trades
        .iter()
        .all(|trade| contract_is_consistent(trade, variant.target_r));
    let coverage = coverage(
        variant.expected_l1_candidates,
        resolved_candidates,
        &trades,
        &completed,
        l1.coverage.returned_symbol_count,
        l1.coverage.eligible_symbol_count,
        l1.coverage.excluded_symbols.len(),
        blockers,
    );
    let decision = decide_l2(
        &coverage,
        &gross,
        &net,
        &net_by_direction,
        &concentration,
        contract_identity_verified,
        variant.candidate_key,
    );

    Ok(EmaRetestL2Report {
        schema_version: variant.schema_version,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: EmaRetestL2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: variant.candidate_key,
            source_l1_rule_version: variant.source_l1_rule_version,
            rule_version: variant.rule_version,
            only_variable: variant.only_variable,
            entry_policy: "resting limit at previous completed EMA144 plus/minus 0.30 ATR14; a gap through receives only the opening-price improvement",
            initial_stop_policy: "4 percent of actual entry price, mirrored by direction",
            target_policy: variant.target_policy,
            intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
            symbol_position_policy: "one open trade per symbol; signals through the exit candle are ignored",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            outcome_evaluation_performed: true,
            runtime_boundary: variant.runtime_boundary,
        },
        source_l1_report_sha256,
        dataset_fingerprint_sha256: rebuilt_l1.coverage.dataset_fingerprint_sha256,
        coverage,
        gross,
        net,
        net_by_direction,
        concentration,
        contract_identity_verified,
        decision,
        trades,
    })
}

/// 把 L1 的因果触碰证据解析成可在触碰 K 内成交的冻结限价计划。
fn resolve_entry(
    data: &BacktestDataSet,
    candidate: L1InputCandidate,
    target_r: f64,
) -> Result<EntryPlan, &'static str> {
    let direction = parse_direction(&candidate.direction)?;
    if !candidate.anchor_ema144.is_finite()
        || candidate.anchor_ema144 <= 0.0
        || !candidate.anchor_atr14.is_finite()
        || candidate.anchor_atr14 <= 0.0
        || !candidate.touch_zone_boundary.is_finite()
        || candidate.touch_zone_boundary <= 0.0
    {
        return Err("candidate_anchor_or_limit_invalid");
    }
    let expected_limit = match direction {
        L2Direction::Long => candidate.anchor_ema144 + 0.30 * candidate.anchor_atr14,
        L2Direction::Short => candidate.anchor_ema144 - 0.30 * candidate.anchor_atr14,
    };
    if !approx_equal(candidate.touch_zone_boundary, expected_limit) {
        return Err("candidate_limit_formula_mismatch");
    }
    let candles = data
        .candles_15m_computed
        .get(&candidate.symbol)
        .ok_or("symbol_candles_missing")?;
    let entry_idx = candles
        .binary_search_by_key(&candidate.signal_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "signal_candle_missing")?;
    let candle = candles.get(entry_idx).ok_or("signal_candle_missing")?;
    let limit_touched = match direction {
        L2Direction::Long => candle.candle.low <= candidate.touch_zone_boundary,
        L2Direction::Short => candle.candle.high >= candidate.touch_zone_boundary,
    };
    if !limit_touched {
        return Err("l1_limit_not_touched_in_reloaded_candle");
    }
    let entry_price =
        gap_aware_limit_fill(candle.candle.open, candidate.touch_zone_boundary, direction);
    let (stop_price, target_price) = risk_prices(entry_price, direction, target_r)?;
    Ok(EntryPlan {
        candidate_id: format!(
            "{}:{}:{}",
            candidate.symbol,
            candidate.signal_ts_ms,
            direction.label()
        ),
        symbol: candidate.symbol,
        direction,
        signal_ts_ms: candidate.signal_ts_ms,
        entry_idx,
        anchor_ema144: candidate.anchor_ema144,
        anchor_atr14: candidate.anchor_atr14,
        limit_price: candidate.touch_zone_boundary,
        entry_price,
        stop_price,
        target_price,
    })
}

/// 限价被跳空穿越时只接受开盘可获得的价格改善，不回看盘中最优价。
fn gap_aware_limit_fill(open: f64, limit: f64, direction: L2Direction) -> f64 {
    match direction {
        L2Direction::Long => open.min(limit),
        L2Direction::Short => open.max(limit),
    }
}

/// 按 4% 初始风险和冻结目标倍数生成保护价；目标倍数由研究身份确定。
fn risk_prices(
    entry_price: f64,
    direction: L2Direction,
    target_r: f64,
) -> Result<(f64, f64), &'static str> {
    if !entry_price.is_finite() || entry_price <= 0.0 || !target_r.is_finite() || target_r <= 0.0 {
        return Err("entry_price_invalid");
    }
    let (stop, target) = match direction {
        L2Direction::Long => (
            entry_price * (1.0 - STOP_LOSS_PCT),
            entry_price * (1.0 + STOP_LOSS_PCT * target_r),
        ),
        L2Direction::Short => (
            entry_price * (1.0 + STOP_LOSS_PCT),
            entry_price * (1.0 - STOP_LOSS_PCT * target_r),
        ),
    };
    if stop <= 0.0 || target <= 0.0 || !stop.is_finite() || !target.is_finite() {
        return Err("risk_or_target_price_invalid");
    }
    Ok((stop, target))
}

/// 逐币执行一个持仓锁，保持与现有忽略持仓期信号的运行语义一致。
fn simulate_with_symbol_lock(
    data: &BacktestDataSet,
    entries: Vec<EntryPlan>,
    blockers: &mut BTreeMap<String, usize>,
) -> Vec<EmaRetestL2TradeRecord> {
    let mut by_symbol: BTreeMap<String, Vec<EntryPlan>> = BTreeMap::new();
    for entry in entries {
        by_symbol
            .entry(entry.symbol.clone())
            .or_default()
            .push(entry);
    }
    let mut records = Vec::new();
    for (symbol, mut symbol_entries) in by_symbol {
        symbol_entries.sort_by_key(|entry| entry.signal_ts_ms);
        let Some(candles) = data.candles_15m_computed.get(&symbol) else {
            *blockers
                .entry("symbol_candles_missing_during_replay".to_owned())
                .or_default() += symbol_entries.len();
            continue;
        };
        let mut locked_until = i64::MIN;
        for entry in symbol_entries {
            if entry.signal_ts_ms <= locked_until {
                *blockers
                    .entry("signal_ignored_while_symbol_position_open".to_owned())
                    .or_default() += 1;
                continue;
            }
            let Some(path) = simulate_exit(candles, &entry) else {
                *blockers
                    .entry("entry_candle_missing_during_replay".to_owned())
                    .or_default() += 1;
                continue;
            };
            locked_until = path.exit_ts_ms;
            records.push(build_trade_record(entry, path));
        }
    }
    records
}

/// 从成交 K 开始执行固定保护，止损与目标同棒时采取保守止损优先。
fn simulate_exit(candles: &[ComputedCandle], entry: &EntryPlan) -> Option<ExitPath> {
    let horizon_end = entry.signal_ts_ms.saturating_add(MAX_HOLDING_MS);
    let mut last_seen = None;
    for candle in candles.get(entry.entry_idx..)? {
        if candle.candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let (stop_hit, target_hit) = exit_hits(
            candle.candle.high,
            candle.candle.low,
            entry.stop_price,
            entry.target_price,
            entry.direction,
        );
        if stop_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: entry.stop_price,
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
                exit_price: entry.target_price,
                exit_reason: "target_hit",
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

/// 将单根 OHLC 对固定保护价的触发拆出来，供状态机与回归测试共享。
fn exit_hits(high: f64, low: f64, stop: f64, target: f64, direction: L2Direction) -> (bool, bool) {
    match direction {
        L2Direction::Long => (low <= stop, high >= target),
        L2Direction::Short => (high >= stop, low <= target),
    }
}

/// 折算一笔交易的毛 R、压力成本 R 和净 R。
fn build_trade_record(entry: EntryPlan, path: ExitPath) -> EmaRetestL2TradeRecord {
    let risk = entry.entry_price * STOP_LOSS_PCT;
    let gross_r = directional_r(entry.entry_price, path.exit_price, risk, entry.direction);
    let cost_r = (entry.entry_price + path.exit_price) * PER_SIDE_COST_RATE / risk;
    EmaRetestL2TradeRecord {
        candidate_id: entry.candidate_id,
        symbol: entry.symbol,
        direction: entry.direction.label(),
        signal_ts_ms: entry.signal_ts_ms,
        anchor_ema144: entry.anchor_ema144,
        anchor_atr14: entry.anchor_atr14,
        limit_price: entry.limit_price,
        entry_price: entry.entry_price,
        initial_stop_price: entry.stop_price,
        target_price: entry.target_price,
        complete: path.complete,
        exit_ts_ms: path.exit_ts_ms,
        exit_price: path.exit_price,
        exit_reason: path.exit_reason,
        gross_r,
        cost_r,
        net_r: gross_r - cost_r,
        event_cluster_id: None,
    }
}

fn directional_r(entry: f64, exit: f64, risk: f64, direction: L2Direction) -> f64 {
    match direction {
        L2Direction::Long => (exit - entry) / risk,
        L2Direction::Short => (entry - exit) / risk,
    }
}

/// 按方向和连续一小时触发链给完整交易写入确定性的市场事件身份。
fn assign_event_clusters(trades: &mut [EmaRetestL2TradeRecord]) {
    let mut order = trades
        .iter()
        .enumerate()
        .filter(|(_, trade)| trade.complete)
        .map(|(idx, trade)| (idx, trade.signal_ts_ms, trade.direction))
        .collect::<Vec<_>>();
    order.sort_by_key(|(_, ts, direction)| (*ts, *direction));
    let mut chains: BTreeMap<&'static str, (i64, i64)> = BTreeMap::new();
    for (idx, ts, direction) in order {
        let chain = chains.entry(direction).or_insert((ts, ts));
        if ts.saturating_sub(chain.0) > EVENT_CLUSTER_WINDOW_MS {
            *chain = (ts, ts);
        } else {
            chain.0 = ts;
        }
        trades[idx].event_cluster_id = Some(format!("{direction}:{}", chain.1));
    }
}

/// 汇总完整交易覆盖，阻塞交易始终保留为可审计计数。
#[allow(clippy::too_many_arguments)]
fn coverage(
    l1_candidates: usize,
    resolved_candidates: usize,
    trades: &[EmaRetestL2TradeRecord],
    completed: &[&EmaRetestL2TradeRecord],
    returned_symbol_count: usize,
    eligible_symbol_count: usize,
    excluded_symbol_count: usize,
    blockers: BTreeMap<String, usize>,
) -> EmaRetestL2Coverage {
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
        .filter_map(|trade| utc_month(trade.signal_ts_ms))
        .collect::<BTreeSet<_>>()
        .len();
    let events = completed
        .iter()
        .filter_map(|trade| trade.event_cluster_id.as_deref())
        .collect::<BTreeSet<_>>()
        .len();
    EmaRetestL2Coverage {
        l1_candidates,
        resolved_candidates,
        executed_trades: trades.len(),
        completed_trades: completed.len(),
        incomplete_trades: trades.len().saturating_sub(completed.len()),
        completed_by_direction: BTreeMap::from([
            ("long", completed_long),
            ("short", completed_short),
        ]),
        completed_symbol_count: symbols,
        completed_month_count: months,
        completed_effective_market_events: events,
        completed_trades_per_month: if months == 0 {
            0.0
        } else {
            completed.len() as f64 / months as f64
        },
        returned_symbol_count,
        eligible_symbol_count,
        excluded_symbol_count,
        blockers,
    }
}

/// 计算按时间顺序排列的一组交易级 R 指标。
fn performance(values: impl Iterator<Item = f64>) -> EmaRetestL2Performance {
    let values = values.collect::<Vec<_>>();
    let trades = values.len();
    let positive_r = values.iter().copied().filter(|value| *value > 0.0).sum();
    let negative_r_abs = -values
        .iter()
        .copied()
        .filter(|value| *value < 0.0)
        .sum::<f64>();
    let sum_r = values.iter().sum::<f64>();
    EmaRetestL2Performance {
        trades,
        positive_r,
        negative_r_abs,
        sum_r,
        expectancy_r: if trades == 0 {
            0.0
        } else {
            sum_r / trades as f64
        },
        profit_factor: (negative_r_abs > 0.0).then_some(positive_r / negative_r_abs),
        win_rate_pct: if trades == 0 {
            0.0
        } else {
            values.iter().filter(|value| **value > 0.0).count() as f64 / trades as f64 * 100.0
        },
        trade_sharpe: trade_sharpe(&values),
        max_drawdown_r: max_drawdown_r(&values),
    }
}

fn performance_by_direction(
    trades: &[&EmaRetestL2TradeRecord],
) -> BTreeMap<&'static str, EmaRetestL2Performance> {
    ["long", "short"]
        .into_iter()
        .map(|direction| {
            (
                direction,
                performance(
                    trades
                        .iter()
                        .filter(|trade| trade.direction == direction)
                        .map(|trade| trade.net_r),
                ),
            )
        })
        .collect()
}

/// 计算头部交易、币种、月份和一小时市场事件的成本后贡献。
fn concentration(trades: &[&EmaRetestL2TradeRecord]) -> EmaRetestL2Concentration {
    let total_net_r = trades.iter().map(|trade| trade.net_r).sum::<f64>();
    let mut ordered = trades.iter().map(|trade| trade.net_r).collect::<Vec<_>>();
    ordered.sort_by(|left, right| right.total_cmp(left));
    let mut net_r_by_symbol = BTreeMap::new();
    let mut net_r_by_month = BTreeMap::new();
    let mut net_r_by_direction = BTreeMap::from([("long", 0.0), ("short", 0.0)]);
    let mut net_r_by_event = BTreeMap::new();
    let mut positive_by_symbol = BTreeMap::new();
    let mut positive_by_event = BTreeMap::new();
    let mut total_positive = 0.0;
    for trade in trades {
        *net_r_by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
        if let Some(month) = utc_month(trade.signal_ts_ms) {
            *net_r_by_month.entry(month).or_default() += trade.net_r;
        }
        *net_r_by_direction.entry(trade.direction).or_default() += trade.net_r;
        if let Some(event) = trade.event_cluster_id.as_ref() {
            *net_r_by_event.entry(event.clone()).or_default() += trade.net_r;
            if trade.net_r > 0.0 {
                *positive_by_event.entry(event.clone()).or_default() += trade.net_r;
            }
        }
        if trade.net_r > 0.0 {
            *positive_by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
            total_positive += trade.net_r;
        }
    }
    let top_event = net_r_by_event.values().copied().fold(0.0_f64, f64::max);
    EmaRetestL2Concentration {
        net_r_after_removing_top_two_trades: total_net_r - ordered.iter().take(2).sum::<f64>(),
        net_r_after_removing_top_event: total_net_r - top_event,
        max_symbol_positive_r_share_pct: max_positive_share(&positive_by_symbol, total_positive),
        max_event_positive_r_share_pct: max_positive_share(&positive_by_event, total_positive),
        net_r_by_symbol,
        net_r_by_month,
        net_r_by_direction,
    }
}

fn max_positive_share<K: Ord>(values: &BTreeMap<K, f64>, total: f64) -> Option<f64> {
    (total > 0.0).then(|| values.values().copied().fold(0.0_f64, f64::max) / total * 100.0)
}

/// 应用查看结果前冻结的覆盖、成本、方向和集中度联合门禁。
fn decide_l2(
    coverage: &EmaRetestL2Coverage,
    gross: &EmaRetestL2Performance,
    net: &EmaRetestL2Performance,
    net_by_direction: &BTreeMap<&'static str, EmaRetestL2Performance>,
    concentration: &EmaRetestL2Concentration,
    contract_identity_verified: bool,
    candidate_key: &str,
) -> EmaRetestL2Decision {
    let long = net_by_direction.get("long");
    let short = net_by_direction.get("short");
    let mut gates = BTreeMap::new();
    gates.insert("l1_identity_and_dataset_verified", true);
    gates.insert(
        "completed_trades_at_least_30",
        coverage.completed_trades >= 30,
    );
    gates.insert(
        "completed_both_directions_at_least_10",
        coverage
            .completed_by_direction
            .get("long")
            .copied()
            .unwrap_or_default()
            >= 10
            && coverage
                .completed_by_direction
                .get("short")
                .copied()
                .unwrap_or_default()
                >= 10,
    );
    gates.insert(
        "completed_symbols_at_least_8",
        coverage.completed_symbol_count >= 8,
    );
    gates.insert(
        "completed_months_at_least_6",
        coverage.completed_month_count >= 6,
    );
    gates.insert(
        "completed_effective_events_at_least_15",
        coverage.completed_effective_market_events >= 15,
    );
    gates.insert(
        "gross_expectancy_and_profit_factor_positive",
        profitable(gross),
    );
    gates.insert(
        "cost_adjusted_expectancy_and_profit_factor_positive",
        profitable(net),
    );
    gates.insert(
        "both_directions_cost_adjusted_positive",
        long.is_some_and(profitable) && short.is_some_and(profitable),
    );
    gates.insert(
        "net_positive_after_removing_top_two_trades",
        concentration.net_r_after_removing_top_two_trades > 0.0,
    );
    gates.insert(
        "net_positive_after_removing_top_event",
        concentration.net_r_after_removing_top_event > 0.0,
    );
    gates.insert(
        "max_symbol_positive_r_share_at_most_35_pct",
        concentration
            .max_symbol_positive_r_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert(
        "max_event_positive_r_share_at_most_35_pct",
        concentration
            .max_event_positive_r_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert("contract_identity_verified", contract_identity_verified);
    let passed = gates.values().all(|passed| *passed);
    EmaRetestL2Decision {
        status: if passed {
            "L2_pass_L3_required"
        } else {
            "stop"
        },
        reason: if passed {
            format!(
                "{candidate_key} 在冻结的 15m 动量风险、退出和压力成本下形成了分散正边际；仍须 L3 的 point-in-time 币池、OOS/walk-forward、统一资金与压力验证。"
            )
        } else {
            format!(
                "至少一项预注册 L2 门禁失败；{candidate_key} 停止在 Research-only，不得调参后直接接入 Paper、ReadOnly、Live 或生产。"
            )
        },
        gates,
    }
}

fn profitable(performance: &EmaRetestL2Performance) -> bool {
    performance.expectancy_r > 0.0
        && performance
            .profit_factor
            .is_some_and(|profit_factor| profit_factor > 1.0)
}

/// 逐笔验证限价、冻结目标倍数、成本和事件身份没有在回放中漂移。
fn contract_is_consistent(trade: &EmaRetestL2TradeRecord, target_r: f64) -> bool {
    let direction = match trade.direction {
        "long" => L2Direction::Long,
        "short" => L2Direction::Short,
        _ => return false,
    };
    let expected_limit = match direction {
        L2Direction::Long => trade.anchor_ema144 + 0.30 * trade.anchor_atr14,
        L2Direction::Short => trade.anchor_ema144 - 0.30 * trade.anchor_atr14,
    };
    let Ok((expected_stop, expected_target)) = risk_prices(trade.entry_price, direction, target_r)
    else {
        return false;
    };
    let risk = trade.entry_price * STOP_LOSS_PCT;
    let expected_gross = directional_r(trade.entry_price, trade.exit_price, risk, direction);
    let expected_cost = (trade.entry_price + trade.exit_price) * PER_SIDE_COST_RATE / risk;
    approx_equal(trade.limit_price, expected_limit)
        && approx_equal(trade.initial_stop_price, expected_stop)
        && approx_equal(trade.target_price, expected_target)
        && approx_equal(trade.gross_r, expected_gross)
        && approx_equal(trade.cost_r, expected_cost)
        && approx_equal(trade.net_r, expected_gross - expected_cost)
        && (!trade.complete || trade.event_cluster_id.is_some())
        && !trade.exit_reason.is_empty()
}

fn parse_direction(value: &str) -> Result<L2Direction, &'static str> {
    match value {
        "long" => Ok(L2Direction::Long),
        "short" => Ok(L2Direction::Short),
        _ => Err("candidate_direction_invalid"),
    }
}

fn utc_month(ts_ms: i64) -> Option<String> {
    Utc.timestamp_millis_opt(ts_ms)
        .single()
        .map(|value| value.format("%Y-%m").to_string())
}

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
    let standard_deviation = variance.sqrt();
    (standard_deviation > 0.0).then_some(mean / standard_deviation * (values.len() as f64).sqrt())
}

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

fn approx_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests;
