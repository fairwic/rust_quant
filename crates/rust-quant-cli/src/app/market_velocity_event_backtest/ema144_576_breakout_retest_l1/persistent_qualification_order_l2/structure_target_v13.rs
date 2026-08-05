//! V13 信号时结构目标授权与 L2 成本回放。

use super::*;
use crate::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::reexpansion_volume_rank_stable_panel_v12::{
    V12_CANDIDATE_KEY, V12_RULE_VERSION,
};
use crate::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::structure_target_v13::{
    V13_CANDIDATE_KEY, V13_RULE_VERSION,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// V13 L2 只替换每笔冻结目标，成交、止损、持仓和成本合同均沿用 V12。
pub const V13_L2_RULE_VERSION: &str =
    "l2_v12_touch_limit_sl04_latest_confirmed_fractal2_target_hold24h_cost8bps_v13";

const EXPECTED_V13_L1_SHA256: &str =
    "d0cac4f5650d0f9df579081c4db62cedc8d6194288ab21aab5fa5663d7c38ec0";
const EXPECTED_V12_L1_SHA256: &str =
    "201114b4ae1e519793f2988f000e00b5751c05fffc710ad0146e28340eb14dd1";
const EXPECTED_V13_CANDIDATE_LEDGER_SHA256: &str =
    "9f1395ea3d576d73a384863397fb69b9c6ea0c19cb51cf16b8a53ca0011e8b97";
const EXPECTED_V13_CANDIDATES: usize = 48_019;
const EXPECTED_V12_CANDIDATES: usize = 48_048;
const STRUCTURE_LOOKBACK_BARS: usize = 96;

#[derive(Debug, Deserialize)]
struct AuthorizationIdentity {
    /// V13 候选键。
    candidate_key: String,
    /// V13 L1 规则版本。
    rule_version: String,
    /// V13 继承的 V12 候选键。
    source_v12_candidate_key: String,
    /// V13 继承的 V12 L1 规则。
    source_v12_rule_version: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationStructureContract {
    /// 摆动中心左侧比较棒数。
    pivot_left_bars: usize,
    /// 摆动中心右侧确认棒数。
    pivot_right_bars: usize,
    /// 信号前已完成 K 的搜索窗口。
    lookback_completed_bars: usize,
    /// true 表示相等高低点不构成摆动。
    comparisons_are_strict: bool,
    /// L1 目标距离使用的初始止损比例。
    diagnostic_initial_stop_pct: f64,
    /// 必须明确不按目标 R 过滤。
    target_r_filter: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationSummary {
    /// V12 源候选数。
    source_candidate_count: usize,
    /// V13 有结构目标的候选数。
    candidate_count: usize,
    /// V13 全候选账本哈希。
    candidate_ledger_sha256: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationTargetAudit {
    /// true 表示用户样本在 V13 定义内。
    matched: bool,
}

#[derive(Debug, Deserialize)]
struct AuthorizationDecision {
    /// V13 L1 覆盖结论。
    status: String,
    /// 必须为 false，防止 outcome 混入授权。
    outcome_evaluation_performed: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct TargetAuthorization {
    /// OKX 永续合约。
    symbol: String,
    /// `long` 或 `short`。
    direction: String,
    /// 回踩信号 K 的 Unix 毫秒时间戳。
    signal_ts_ms: i64,
    /// 触碰前冻结的 EMA144。
    anchor_ema144: f64,
    /// 触碰前冻结的 ATR14。
    anchor_atr14: f64,
    /// V12 计划限价。
    touch_zone_boundary: f64,
    /// 摆动中心 K 的 Unix 毫秒时间戳。
    structure_pivot_ts_ms: i64,
    /// 结构目标首次确认可用的 Unix 毫秒时间戳。
    structure_confirmed_at_ms: i64,
    /// 摆动中心距信号 K 的 15m 棒数。
    structure_pivot_age_bars: usize,
    /// 信号时冻结的绝对结构目标价。
    structure_target_price: f64,
    /// 按计划限价和 4% 风险计算的诊断 R。
    structure_target_r_at_limit: f64,
}

#[derive(Debug, Deserialize)]
struct AuthorizationReport {
    /// V13 策略身份。
    identity: AuthorizationIdentity,
    /// V13 使用的 V12 L1 文件 SHA-256。
    source_v12_l1_report_sha256: String,
    /// 冻结行情指纹。
    dataset_fingerprint_sha256: String,
    /// Top60 实际返回成员数。
    returned_symbol_count: usize,
    /// 完成预热的本地成员数。
    eligible_symbol_count: usize,
    /// 结构目标冻结参数。
    structure_contract: AuthorizationStructureContract,
    /// 候选覆盖摘要。
    summary: AuthorizationSummary,
    /// 三张用户图覆盖。
    target_audits: Vec<AuthorizationTargetAudit>,
    /// V13 L1 门禁结论。
    decision: AuthorizationDecision,
    /// 全量信号时结构目标。
    candidates: Vec<TargetAuthorization>,
}

/// 校验 V6 入场源账本和 V13 无标签授权后，执行绝对结构目标成本回放。
pub async fn run_structure_target_v13_l2_replay(
    v6_l1_source: &Path,
    v13_l1_authorization: &Path,
    output: &Path,
) -> Result<EmaRetestL2Report> {
    let (mut l1, _) = load_and_validate_l1(v6_l1_source)?;
    let (targets, authorization_sha256) = load_v13_authorization(v13_l1_authorization)?;
    l1.candidates
        .retain(|candidate| targets.contains_key(&l1_candidate_id(candidate)));
    if l1.candidates.len() != EXPECTED_V13_CANDIDATES {
        bail!("V13 authorization did not map exactly onto V6 candidates");
    }
    let retained_ids = l1
        .candidates
        .iter()
        .map(l1_candidate_id)
        .collect::<BTreeSet<_>>();
    if retained_ids != targets.keys().cloned().collect::<BTreeSet<_>>() {
        bail!("V13 retained candidate identities differ from authorization ledger");
    }

    let config = config_from_env_and_args(frozen_l1_args()?)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 V13 L2 replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v13_l2_report(&data, l1, &targets, authorization_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("创建 EMA144/576 V13 L2 报告目录失败：{}", parent.display())
        })?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V13 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 完整文件 SHA、因果合同、账本哈希与 3/3 目标共同授权 outcome 回放。
fn load_v13_authorization(
    source: &Path,
) -> Result<(BTreeMap<String, TargetAuthorization>, String)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 EMA144/576 V13 L1 授权失败：{}", source.display()))?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != EXPECTED_V13_L1_SHA256 {
        bail!("V13 L1 authorization SHA mismatch");
    }
    let report: AuthorizationReport =
        serde_json::from_slice(&bytes).context("解析 EMA144/576 V13 L1 授权失败")?;
    if report.identity.candidate_key != V13_CANDIDATE_KEY
        || report.identity.rule_version != V13_RULE_VERSION
        || report.identity.source_v12_candidate_key != V12_CANDIDATE_KEY
        || report.identity.source_v12_rule_version != V12_RULE_VERSION
        || report.source_v12_l1_report_sha256 != EXPECTED_V12_L1_SHA256
        || report.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || report.returned_symbol_count != 60
        || report.eligible_symbol_count != 44
        || report.structure_contract.pivot_left_bars != 2
        || report.structure_contract.pivot_right_bars != 2
        || report.structure_contract.lookback_completed_bars != STRUCTURE_LOOKBACK_BARS
        || !report.structure_contract.comparisons_are_strict
        || !approx_equal(
            report.structure_contract.diagnostic_initial_stop_pct,
            STOP_LOSS_PCT,
        )
        || report.structure_contract.target_r_filter
            != "none; signal-time target R distribution is diagnostic only"
        || report.summary.source_candidate_count != EXPECTED_V12_CANDIDATES
        || report.summary.candidate_count != EXPECTED_V13_CANDIDATES
        || report.summary.candidate_ledger_sha256 != EXPECTED_V13_CANDIDATE_LEDGER_SHA256
        || report.target_audits.len() != 3
        || !report.target_audits.iter().all(|target| target.matched)
        || report.decision.status != "coverage_pass_ready_for_l2_prereg"
        || report.decision.outcome_evaluation_performed
        || report.candidates.len() != EXPECTED_V13_CANDIDATES
    {
        bail!("V13 L1 authorization identity or coverage gate mismatch");
    }

    let mut targets = BTreeMap::new();
    for target in report.candidates {
        validate_target_authorization(&target)?;
        let id = authorization_candidate_id(&target);
        if targets.insert(id, target).is_some() {
            bail!("V13 L1 authorization contains duplicate candidate identities");
        }
    }
    if targets.len() != EXPECTED_V13_CANDIDATES {
        bail!("V13 L1 authorization candidate count drifted");
    }
    Ok((targets, sha256))
}

/// 目标必须保持 L1 的信号前确认、盈利侧和无 R 阈值语义。
fn validate_target_authorization(target: &TargetAuthorization) -> Result<()> {
    let direction = parse_direction(&target.direction).map_err(anyhow::Error::msg)?;
    if target.structure_confirmed_at_ms > target.signal_ts_ms
        || target.structure_pivot_ts_ms >= target.signal_ts_ms
        || !(3..=STRUCTURE_LOOKBACK_BARS).contains(&target.structure_pivot_age_bars)
        || !target.touch_zone_boundary.is_finite()
        || target.touch_zone_boundary <= 0.0
        || !target.structure_target_price.is_finite()
        || target.structure_target_price <= 0.0
        || !target.structure_target_r_at_limit.is_finite()
        || target.structure_target_r_at_limit <= 0.0
    {
        bail!("V13 structure target causality or geometry invalid");
    }
    let distance = match direction {
        L2Direction::Long => target.structure_target_price - target.touch_zone_boundary,
        L2Direction::Short => target.touch_zone_boundary - target.structure_target_price,
    };
    let expected_r = distance / (target.touch_zone_boundary * STOP_LOSS_PCT);
    if distance <= 0.0 || !approx_equal(expected_r, target.structure_target_r_at_limit) {
        bail!("V13 structure target profitable-side or diagnostic R mismatch");
    }
    Ok(())
}

fn build_v13_l2_report(
    data: &BacktestDataSet,
    l1: L1InputReport,
    targets: &BTreeMap<String, TargetAuthorization>,
    source_l1_report_sha256: String,
) -> Result<EmaRetestL2Report> {
    let rebuilt_l1 = build_v6_l1_report(data)?;
    if rebuilt_l1.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || rebuilt_l1.summary.candidate_count != EXPECTED_L1_CANDIDATES
    {
        bail!("reloaded V6 L1 identity mismatch for V13");
    }
    let returned_symbol_count = l1.coverage.returned_symbol_count;
    let eligible_symbol_count = l1.coverage.eligible_symbol_count;
    let excluded_symbol_count = l1.coverage.excluded_symbols.len();
    let mut blockers = BTreeMap::new();
    let mut entries = Vec::with_capacity(l1.candidates.len());
    for candidate in l1.candidates {
        let id = l1_candidate_id(&candidate);
        let Some(target) = targets.get(&id) else {
            *blockers
                .entry("v13_structure_target_authorization_missing".to_owned())
                .or_default() += 1;
            continue;
        };
        match resolve_structure_entry(data, candidate, target) {
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
    let contract_identity_verified = trades.iter().all(|trade| {
        targets
            .get(&trade.candidate_id)
            .is_some_and(|target| structure_contract_is_consistent(trade, target))
    });
    let coverage = coverage(
        EXPECTED_V13_CANDIDATES,
        resolved_candidates,
        &trades,
        &completed,
        returned_symbol_count,
        eligible_symbol_count,
        excluded_symbol_count,
        blockers,
    );
    let decision = decide_l2(
        &coverage,
        &gross,
        &net,
        &net_by_direction,
        &concentration,
        contract_identity_verified,
        V13_CANDIDATE_KEY,
    );
    Ok(EmaRetestL2Report {
        schema_version: "market_momentum_15m_ema144_576_stable_panel_structure_target_l2_v13",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: EmaRetestL2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: V13_CANDIDATE_KEY,
            source_l1_rule_version: V13_RULE_VERSION,
            rule_version: V13_L2_RULE_VERSION,
            only_variable: "replace only the fixed 0.52R exit with each V13 signal-time confirmed absolute structure target; all V12 entries, stops, holding, cost, conflict, and universe contracts remain frozen",
            entry_policy: "resting limit at previous completed EMA144 plus/minus 0.30 ATR14; a gap through receives only the opening-price improvement",
            initial_stop_policy: "4 percent of actual entry price, mirrored by direction",
            target_policy: "absolute latest confirmed 2-left 2-right directional swing price frozen by V13 L1; no R floor, cap, protection, trailing, partial, or runner",
            intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
            symbol_position_policy: "one open trade per symbol; signals through the exit candle are ignored",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            outcome_evaluation_performed: true,
            runtime_boundary: "research-only V13 L2; not registered in paper, readonly shadow, live worker, compose, Pine, or production presets",
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

/// V12 因果限价照旧，目标价只从已校验的 V13 授权读取且不按实际成交重算。
fn resolve_structure_entry(
    data: &BacktestDataSet,
    candidate: L1InputCandidate,
    target: &TargetAuthorization,
) -> Result<EntryPlan, &'static str> {
    let direction = parse_direction(&candidate.direction)?;
    if candidate.symbol != target.symbol
        || candidate.direction != target.direction
        || candidate.signal_ts_ms != target.signal_ts_ms
        || !approx_equal(candidate.anchor_ema144, target.anchor_ema144)
        || !approx_equal(candidate.anchor_atr14, target.anchor_atr14)
        || !approx_equal(candidate.touch_zone_boundary, target.touch_zone_boundary)
    {
        return Err("v13_l1_entry_identity_mismatch");
    }
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
    let (stop_price, target_price) = structure_risk_prices(
        entry_price,
        candidate.touch_zone_boundary,
        target.structure_target_price,
        direction,
    )?;
    Ok(EntryPlan {
        candidate_id: l1_candidate_id(&candidate),
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

/// 跳空改善只能扩大到冻结结构价的距离，不能把绝对目标重新换算成固定 R。
fn structure_risk_prices(
    entry_price: f64,
    limit_price: f64,
    structure_target_price: f64,
    direction: L2Direction,
) -> Result<(f64, f64), &'static str> {
    if !entry_price.is_finite()
        || entry_price <= 0.0
        || !limit_price.is_finite()
        || limit_price <= 0.0
        || !structure_target_price.is_finite()
        || structure_target_price <= 0.0
    {
        return Err("entry_or_structure_target_price_invalid");
    }
    let stop_price = match direction {
        L2Direction::Long => entry_price * (1.0 - STOP_LOSS_PCT),
        L2Direction::Short => entry_price * (1.0 + STOP_LOSS_PCT),
    };
    let target_on_profitable_side = match direction {
        L2Direction::Long => {
            structure_target_price > limit_price && structure_target_price > entry_price
        }
        L2Direction::Short => {
            structure_target_price < limit_price && structure_target_price < entry_price
        }
    };
    if !stop_price.is_finite() || stop_price <= 0.0 || !target_on_profitable_side {
        return Err("structure_target_not_on_profitable_side");
    }
    Ok((stop_price, structure_target_price))
}

/// 逐笔核对绝对结构目标、风险、成本和事件身份均与 V13 授权一致。
fn structure_contract_is_consistent(
    trade: &EmaRetestL2TradeRecord,
    target: &TargetAuthorization,
) -> bool {
    let direction = match parse_direction(trade.direction) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let expected_limit = match direction {
        L2Direction::Long => trade.anchor_ema144 + 0.30 * trade.anchor_atr14,
        L2Direction::Short => trade.anchor_ema144 - 0.30 * trade.anchor_atr14,
    };
    let Ok((expected_stop, expected_target)) = structure_risk_prices(
        trade.entry_price,
        trade.limit_price,
        target.structure_target_price,
        direction,
    ) else {
        return false;
    };
    let risk = trade.entry_price * STOP_LOSS_PCT;
    let expected_gross = directional_r(trade.entry_price, trade.exit_price, risk, direction);
    let expected_cost = (trade.entry_price + trade.exit_price) * PER_SIDE_COST_RATE / risk;
    trade.candidate_id == authorization_candidate_id(target)
        && approx_equal(trade.limit_price, expected_limit)
        && approx_equal(trade.limit_price, target.touch_zone_boundary)
        && approx_equal(trade.initial_stop_price, expected_stop)
        && approx_equal(trade.target_price, expected_target)
        && approx_equal(trade.gross_r, expected_gross)
        && approx_equal(trade.cost_r, expected_cost)
        && approx_equal(trade.net_r, expected_gross - expected_cost)
        && (!trade.complete || trade.event_cluster_id.is_some())
        && !trade.exit_reason.is_empty()
}

fn l1_candidate_id(candidate: &L1InputCandidate) -> String {
    format!(
        "{}:{}:{}",
        candidate.symbol, candidate.signal_ts_ms, candidate.direction
    )
}

fn authorization_candidate_id(candidate: &TargetAuthorization) -> String {
    format!(
        "{}:{}:{}",
        candidate.symbol, candidate.signal_ts_ms, candidate.direction
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_improvement_keeps_absolute_long_structure_target() {
        let (stop, target) =
            structure_risk_prices(99.0, 100.0, 102.0, L2Direction::Long).expect("prices");
        assert!((stop - 95.04).abs() < 1e-12);
        assert_eq!(target, 102.0);
    }

    #[test]
    fn target_behind_planned_limit_is_rejected_for_both_directions() {
        assert!(structure_risk_prices(99.0, 100.0, 99.5, L2Direction::Long).is_err());
        assert!(structure_risk_prices(101.0, 100.0, 100.5, L2Direction::Short).is_err());
    }
}
