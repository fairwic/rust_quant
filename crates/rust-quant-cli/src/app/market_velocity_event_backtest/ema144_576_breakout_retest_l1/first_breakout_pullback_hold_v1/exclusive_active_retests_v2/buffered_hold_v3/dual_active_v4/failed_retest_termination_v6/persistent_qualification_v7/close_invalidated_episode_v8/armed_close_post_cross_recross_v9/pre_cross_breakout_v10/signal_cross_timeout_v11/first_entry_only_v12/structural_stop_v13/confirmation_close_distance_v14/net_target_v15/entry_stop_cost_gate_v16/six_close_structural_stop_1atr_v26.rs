//! 对 V25 六根信号候选只把 EMA144 结构止损缓冲改为 1ATR 的 V26 L1。

mod paired_l2;
pub use paired_l2::run_v26_l2;

use super::*;
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

/// V26 独立候选身份，风险语义变化不得覆盖 V25。
pub const V26_CANDIDATE_KEY: &str =
    "market_momentum_ema576_six_close_acceptance_window_extreme_200atr_structural_stop_100atr_cost_cap050r_15m_v26";
/// V26 L1 只读取信号、下一根开盘与 1ATR 风险几何。
pub const V26_L1_RULE_VERSION: &str =
    "l1_v26_v25_signal_quality_structural_stop_100atr_cost050_no_outcome_v1";

const EXPECTED_V25_REPORT_SHA256: &str =
    "92c63c78e1c016d7d671c608d384a764ae8443f726b9dcfbb482afa27cbffd6c";
const EXPECTED_V25_SCHEMA_VERSION: &str =
    "market_momentum_ema576_acceptance_window_extreme_2_0atr_six_close_l1_v25";
const EXPECTED_V25_CANDIDATE_KEY: &str =
    "market_momentum_ema576_relation_intact_until_signal_acceptance_window_extreme_200atr_acceptance6_ema576_intrabar_hold_structural_stop_cost_cap050r_15m_v25";
const EXPECTED_V25_RULE_VERSION: &str =
    "l1_v25_v24_acceptance_window_six_closes_directional_extreme_200atr_no_outcome_v1";
const EXPECTED_V25_QUALITY_CANDIDATES: usize = 1_207;
const EXPECTED_V25_QUALITY_LONG: usize = 780;
const EXPECTED_V25_QUALITY_SHORT: usize = 427;
const EXPECTED_V25_QUALITY_SET_SHA256: &str =
    "99b3fa269ca1c1bb9f1805c487c5f2e17926bc090b19ffe381f53d21bfd7132d";
const EXPECTED_V25_OLD_COST_ELIGIBLE: usize = 720;
const EXPECTED_V16_SOURCE_SHA256: &str =
    "0c199ad758becefab58a03da53837e85c022b6e4d0e510fa7b34c26332307cf2";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const STRUCTURAL_STOP_BUFFER_ATR_V26: f64 = 1.00;
const NET_TARGET_R_V26: f64 = 2.00;
const MAX_STOP_COST_R_V26: f64 = 0.50;
const MIN_ELIGIBLE_RATIO_PCT: f64 = 85.0;
const MIN_REOPENED_CANDIDATES: usize = 100;
const MIN_DIRECTION_RETAINED: usize = 100;
const MIN_SYMBOLS: usize = 8;
const MIN_MONTHS_BJT: usize = 6;
const MIN_EVENTS: usize = 100;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const FORBIDDEN_OUTCOME_FIELDS: [&str; 10] = [
    "complete",
    "exit_price",
    "exit_reason",
    "exit_ts_ms",
    "gross_r",
    "cost_r",
    "net_r",
    "mfe",
    "mae",
    "pnl",
];

/// V25 冻结信号集合及其旧 0.30ATR 成本门禁身份。
struct FrozenV25Signals {
    report_sha256: String,
    candidate_ids: BTreeSet<String>,
    source_v16_eligibility: HashMap<String, bool>,
}

/// V26 单变量、因果字段与运行隔离身份。
#[derive(Debug, Clone, Serialize)]
pub struct V26L1Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// 独立风险版本候选键。
    pub candidate_key: &'static str,
    /// 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V25 的唯一变化。
    pub only_variable: &'static str,
    /// L1 允许读取的字段边界。
    pub causal_field_boundary: &'static str,
    /// L1 禁止读取的成交后字段。
    pub label_boundary: &'static str,
    /// 与任何运行态路径的隔离边界。
    pub runtime_boundary: &'static str,
}

/// 一个 V25 信号在 EMA144±1ATR 下的成交前风险证据。
#[derive(Debug, Clone, Serialize)]
pub struct V26RiskOpportunity {
    /// `symbol:signal_ts_ms:direction` 稳定身份。
    pub candidate_id: String,
    /// OKX USDT 永续交易对。
    pub symbol: String,
    /// long 或 short。
    pub direction: &'static str,
    /// 长期资格完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// EMA576 两收盘突破确认时间，Unix 毫秒。
    pub breakout_ts_ms: i64,
    /// 回踩信号完成时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 回踩信号完成时间，北京时间。
    pub signal_time_bjt: String,
    /// 信号时 EMA144。
    pub signal_ema144: f64,
    /// 信号时 Wilder ATR14。
    pub signal_atr14: f64,
    /// 下一根连续 15m 开盘时间；结构无效时为空。
    pub entry_ts_ms: Option<i64>,
    /// 下一根连续 15m 开盘价；结构无效时为空。
    pub entry_price: Option<f64>,
    /// 信号 EMA144 外侧 1ATR 的冻结止损价。
    pub initial_stop_price: Option<f64>,
    /// 入场价到止损价的正向初始价格风险。
    pub initial_risk: Option<f64>,
    /// 初始价格风险除以信号 ATR14。
    pub initial_risk_atr: Option<f64>,
    /// 初始价格风险占入场价百分比。
    pub initial_risk_pct: Option<f64>,
    /// 按新初始风险反解的成本后净 2R 目标价。
    pub target_price: Option<f64>,
    /// 假设止损成交时，开平双边成本占新初始风险 R。
    pub stop_cost_r: Option<f64>,
    /// 旧 V16 0.30ATR 风险合同是否允许成交。
    pub source_v16_eligible: bool,
    /// true 表示新 1ATR 风险与 0.50R 成本门禁均通过。
    pub eligible: bool,
    /// 结构、连续开盘或成本门禁拒绝原因；合格时为空。
    pub blocked_reason: Option<&'static str>,
}

/// V26 成交前风险覆盖与旧门禁迁移统计。
#[derive(Debug, Clone, Serialize)]
pub struct V26L1Coverage {
    /// V25 信号质量通过的冻结候选数。
    pub source_quality_candidate_count: usize,
    /// 旧 0.30ATR 成本合同下合格的候选数。
    pub source_v16_eligible_count: usize,
    /// 新止损能形成正初始风险与合法目标的候选数。
    pub structurally_valid_count: usize,
    /// 新 1ATR 与 0.50R 成本门禁共同合格数。
    pub eligible_count: usize,
    /// 新合格候选占全部冻结信号百分比。
    pub eligible_ratio_pct: f64,
    /// 旧成本拒绝、但新 1ATR 风险重新合格的候选数。
    pub newly_eligible_count: usize,
    /// 旧合格、但在新风险合同下丢失的候选数；预期必须为零。
    pub lost_old_eligible_count: usize,
    /// 新合格候选按方向计数。
    pub eligible_by_direction: BTreeMap<String, usize>,
    /// 新合格候选覆盖交易对数量。
    pub eligible_symbol_count: usize,
    /// 新合格候选覆盖北京时间月份数量。
    pub eligible_month_count_bjt: usize,
    /// 新合格候选的一小时方向事件数。
    pub eligible_effective_market_events: usize,
    /// 结构或成本拒绝原因计数。
    pub blockers: BTreeMap<String, usize>,
}

/// V26 成交前有限数值的冻结离散分布。
#[derive(Debug, Clone, Serialize)]
pub struct V26Distribution {
    /// 数值数量。
    pub count: usize,
    /// 最小值。
    pub min: f64,
    /// 10% 分位。
    pub p10: f64,
    /// 中位数。
    pub median: f64,
    /// 90% 分位。
    pub p90: f64,
    /// 最大值。
    pub max: f64,
}

/// V26 L1 覆盖停止门禁。
#[derive(Debug, Clone, Serialize)]
pub struct V26L1Decision {
    /// `coverage_pass_ready_for_l2_prereg` 或 `stop`。
    pub status: &'static str,
    /// 每项预注册门禁结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// L1 固定为 false。
    pub outcome_evaluation_performed: bool,
    /// 当前等级结论。
    pub reason: String,
}

/// V26 完整无 outcome L1 证据。
#[derive(Debug, Clone, Serialize)]
pub struct V26L1Report {
    /// L1 schema 身份。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 单变量与运行隔离身份。
    pub identity: V26L1Identity,
    /// 冻结 V25 L1 文件 SHA-256。
    pub source_v25_report_sha256: String,
    /// 冻结 V14 合并报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 重载行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// 覆盖与旧门禁迁移统计。
    pub coverage: V26L1Coverage,
    /// 所有结构合法机会的初始风险 ATR 分布。
    pub all_valid_initial_risk_atr_distribution: V26Distribution,
    /// 新合格机会的初始风险百分比分布。
    pub eligible_initial_risk_pct_distribution: V26Distribution,
    /// 新合格机会的止损成本 R 分布。
    pub eligible_stop_cost_r_distribution: V26Distribution,
    /// L1 门禁结论。
    pub decision: V26L1Decision,
    /// 1,207 个冻结信号的完整成交前风险账本。
    pub opportunities: Vec<V26RiskOpportunity>,
}

/// V26 L1 单一机器产物；L2 在独立预注册前固定为空。
#[derive(Debug, Clone, Serialize)]
pub struct V26MachineReport {
    /// 合并机器报告 schema。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 冻结 V25 文件 SHA-256。
    pub source_v25_report_sha256: String,
    /// 内嵌 L1 负载 SHA-256。
    pub l1_payload_sha256: String,
    /// 完整无 outcome L1。
    pub l1: V26L1Report,
    /// 未经独立预注册不得出现 L2。
    pub l2: Option<Value>,
}

/// 校验 V25/V14 身份，重算 EMA144±1ATR 风险并只输出无 outcome L1。
pub async fn run_v26_l1(v25_source: &Path, v14_source: &Path, output: &Path) -> Result<()> {
    let frozen = load_v25_signals(v25_source)?;
    let (v14_sha256, v14_l1) = super::load_v14_source(v14_source)?;
    let replay_input = super::super::super::load_verified_v14_replay_input(&v14_l1).await?;
    if replay_input.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("V26 reloaded dataset fingerprint mismatch");
    }
    let candidates = replay_input
        .candidates
        .iter()
        .filter(|candidate| frozen.candidate_ids.contains(&candidate_id(candidate)))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.len() != EXPECTED_V25_QUALITY_CANDIDATES {
        bail!("V26 signal candidate join count mismatch");
    }
    let l1 = build_l1_report(
        &replay_input.data,
        &candidates,
        &frozen.source_v16_eligibility,
        frozen.report_sha256.clone(),
        v14_sha256,
        replay_input.dataset_fingerprint_sha256,
    )?;
    let l1_payload_sha256 = super::sha256_hex(&serde_json::to_vec(&l1)?);
    let report = V26MachineReport {
        schema_version: "market_momentum_ema576_six_close_structural_stop_1atr_l1_v26",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        source_v25_report_sha256: frozen.report_sha256,
        l1_payload_sha256,
        l1,
        l2: None,
    };
    write_report(output, &report)
}

/// 读取 SHA 固定的 V25 报告，并解锁全部信号质量通过身份而非旧成本合格子集。
fn load_v25_signals(source: &Path) -> Result<FrozenV25Signals> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取冻结 V25 L1 报告失败：{}", source.display()))?;
    let report_sha256 = super::sha256_hex(&bytes);
    if report_sha256 != EXPECTED_V25_REPORT_SHA256 {
        bail!("V26 source V25 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析冻结 V25 L1 报告失败")?;
    if source.pointer("/schema_version").and_then(Value::as_str)
        != Some(EXPECTED_V25_SCHEMA_VERSION)
        || source
            .pointer("/l1/identity/candidate_key")
            .and_then(Value::as_str)
            != Some(EXPECTED_V25_CANDIDATE_KEY)
        || source
            .pointer("/l1/identity/rule_version")
            .and_then(Value::as_str)
            != Some(EXPECTED_V25_RULE_VERSION)
        || source
            .pointer("/l1/source_v14_report_sha256")
            .and_then(Value::as_str)
            != Some(super::EXPECTED_V14_REPORT_SHA256)
        || source
            .pointer("/l1/source_v16_report_sha256")
            .and_then(Value::as_str)
            != Some(EXPECTED_V16_SOURCE_SHA256)
        || source
            .pointer("/l1/dataset_fingerprint_sha256")
            .and_then(Value::as_str)
            != Some(EXPECTED_DATASET_FINGERPRINT_SHA256)
        || source
            .pointer("/l1/decision/status")
            .and_then(Value::as_str)
            != Some("coverage_pass_ready_for_l2_prereg")
        || source
            .pointer("/l1/decision/outcome_evaluation_performed")
            .and_then(Value::as_bool)
            != Some(false)
        || !source.pointer("/l2").is_some_and(Value::is_null)
    {
        bail!("V26 source V25 identity mismatch");
    }
    let rows = source
        .pointer("/l1/opportunities")
        .and_then(Value::as_array)
        .context("V26 source V25 opportunities missing")?;
    let mut candidate_ids = BTreeSet::new();
    let mut source_v16_eligibility = HashMap::new();
    let mut by_direction = BTreeMap::new();
    for row in rows {
        validate_source_row_has_no_outcome(row)?;
        if row.get("quality_gate_passed").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let id = row
            .get("candidate_id")
            .and_then(Value::as_str)
            .context("V25 quality candidate_id missing")?
            .to_owned();
        let direction = row
            .get("direction")
            .and_then(Value::as_str)
            .context("V25 quality direction missing")?;
        let old_eligible = row
            .get("source_v16_eligible")
            .and_then(Value::as_bool)
            .context("V25 source_v16_eligible missing")?;
        if !candidate_ids.insert(id.clone())
            || source_v16_eligibility.insert(id, old_eligible).is_some()
        {
            bail!("duplicate V25 quality candidate identity");
        }
        *by_direction.entry(direction.to_owned()).or_default() += 1;
    }
    let old_eligible_count = source_v16_eligibility
        .values()
        .filter(|eligible| **eligible)
        .count();
    if candidate_ids.len() != EXPECTED_V25_QUALITY_CANDIDATES
        || by_direction.get("long") != Some(&EXPECTED_V25_QUALITY_LONG)
        || by_direction.get("short") != Some(&EXPECTED_V25_QUALITY_SHORT)
        || old_eligible_count != EXPECTED_V25_OLD_COST_ELIGIBLE
        || candidate_set_sha256(&candidate_ids) != EXPECTED_V25_QUALITY_SET_SHA256
    {
        bail!("V26 source V25 quality candidate set mismatch");
    }
    Ok(FrozenV25Signals {
        report_sha256,
        candidate_ids,
        source_v16_eligibility,
    })
}

/// 从 V25 质量候选生成 1ATR 成交前风险账本与覆盖门禁。
fn build_l1_report(
    data: &BacktestDataSet,
    candidates: &[V2Candidate],
    old_eligibility: &HashMap<String, bool>,
    v25_sha256: String,
    v14_sha256: String,
    dataset_fingerprint_sha256: String,
) -> Result<V26L1Report> {
    let opportunities = candidates
        .iter()
        .map(|candidate| {
            let id = candidate_id(candidate);
            let source_v16_eligible = old_eligibility
                .get(&id)
                .copied()
                .with_context(|| format!("V26 old eligibility missing: {id}"))?;
            risk_opportunity(data, candidate, source_v16_eligible)
        })
        .collect::<Result<Vec<_>>>()?;
    validate_no_outcome_fields(&opportunities)?;
    let valid = opportunities
        .iter()
        .filter(|item| item.stop_cost_r.is_some())
        .collect::<Vec<_>>();
    let eligible = opportunities
        .iter()
        .filter(|item| item.eligible)
        .collect::<Vec<_>>();
    let newly_eligible_count = eligible
        .iter()
        .filter(|item| !item.source_v16_eligible)
        .count();
    let lost_old_eligible_count = opportunities
        .iter()
        .filter(|item| item.source_v16_eligible && !item.eligible)
        .count();
    let mut eligible_by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    let mut blockers = BTreeMap::new();
    for item in &eligible {
        *eligible_by_direction
            .entry(item.direction.to_owned())
            .or_default() += 1;
        symbols.insert(item.symbol.clone());
        months.insert(super::month_bjt(item.signal_ts_ms)?);
    }
    for item in &opportunities {
        if let Some(reason) = item.blocked_reason {
            *blockers.entry(reason.to_owned()).or_default() += 1;
        }
    }
    let coverage = V26L1Coverage {
        source_quality_candidate_count: opportunities.len(),
        source_v16_eligible_count: opportunities
            .iter()
            .filter(|item| item.source_v16_eligible)
            .count(),
        structurally_valid_count: valid.len(),
        eligible_count: eligible.len(),
        eligible_ratio_pct: ratio_pct(eligible.len(), opportunities.len()),
        newly_eligible_count,
        lost_old_eligible_count,
        eligible_by_direction,
        eligible_symbol_count: symbols.len(),
        eligible_month_count_bjt: months.len(),
        eligible_effective_market_events: effective_event_count(&eligible),
        blockers,
    };
    let mut gates = BTreeMap::new();
    gates.insert(
        "source_signal_candidate_count_matches",
        coverage.source_quality_candidate_count == EXPECTED_V25_QUALITY_CANDIDATES,
    );
    gates.insert(
        "source_old_cost_eligible_count_matches",
        coverage.source_v16_eligible_count == EXPECTED_V25_OLD_COST_ELIGIBLE,
    );
    gates.insert(
        "eligible_ratio_at_least_85_pct",
        coverage.eligible_ratio_pct >= MIN_ELIGIBLE_RATIO_PCT,
    );
    gates.insert(
        "old_eligible_candidates_are_not_lost",
        coverage.lost_old_eligible_count == 0,
    );
    gates.insert(
        "at_least_100_old_cost_rejections_reopen",
        coverage.newly_eligible_count >= MIN_REOPENED_CANDIDATES,
    );
    gates.insert(
        "both_directions_retain_at_least_100",
        ["long", "short"].iter().all(|direction| {
            coverage
                .eligible_by_direction
                .get(*direction)
                .copied()
                .unwrap_or_default()
                >= MIN_DIRECTION_RETAINED
        }),
    );
    gates.insert(
        "cross_symbol_month_event_coverage_preserved",
        coverage.eligible_symbol_count >= MIN_SYMBOLS
            && coverage.eligible_month_count_bjt >= MIN_MONTHS_BJT
            && coverage.eligible_effective_market_events >= MIN_EVENTS,
    );
    gates.insert("forbidden_outcome_fields_absent", true);
    let passed = gates.values().all(|passed| *passed);
    Ok(V26L1Report {
        schema_version: "market_momentum_ema576_six_close_structural_stop_1atr_l1_v26",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V26L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V26_CANDIDATE_KEY,
            rule_version: V26_L1_RULE_VERSION,
            only_variable: "relative to V25, replace only the signal EMA144 plus/minus 0.30 ATR14 initial stop buffer with plus/minus 1.00 ATR14 and recompute the unchanged 0.50R pre-fill cost gate",
            causal_field_boundary: "the SHA-frozen 1,207 V25 quality candidates plus only their signal EMA144/ATR14, next contiguous 15m open, 1ATR stop, initial risk, net-2R target geometry, and stop-exit cost R",
            label_boundary: "no later candle, stop hit, target hit, complete flag, exit time, exit price, exit reason, gross R, realized cost R, net R, MFE, MAE, PnL, win, or loss is read",
            runtime_boundary: "research-only V26 L1; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
        },
        source_v25_report_sha256: v25_sha256,
        source_v14_report_sha256: v14_sha256,
        dataset_fingerprint_sha256,
        coverage,
        all_valid_initial_risk_atr_distribution: distribution(
            valid.iter().filter_map(|item| item.initial_risk_atr).collect(),
        )?,
        eligible_initial_risk_pct_distribution: distribution(
            eligible
                .iter()
                .filter_map(|item| item.initial_risk_pct)
                .collect(),
        )?,
        eligible_stop_cost_r_distribution: distribution(
            eligible
                .iter()
                .filter_map(|item| item.stop_cost_r)
                .collect(),
        )?,
        decision: V26L1Decision {
            status: if passed {
                "coverage_pass_ready_for_l2_prereg"
            } else {
                "stop"
            },
            gates,
            outcome_evaluation_performed: false,
            reason: if passed {
                "EMA144±1ATR 风险重算保持旧合格身份并通过预注册覆盖；允许冻结新候选后执行一次 L2。".to_owned()
            } else {
                "至少一项源身份、风险覆盖、旧候选单调性或分散门禁失败；停止在 L1，不读取 outcome。".to_owned()
            },
        },
        opportunities,
    })
}

/// 把一个 V25 信号投影为 1ATR 结构无效、成本拒绝或合格机会。
fn risk_opportunity(
    data: &BacktestDataSet,
    candidate: &V2Candidate,
    source_v16_eligible: bool,
) -> Result<V26RiskOpportunity> {
    let mut result = V26RiskOpportunity {
        candidate_id: candidate_id(candidate),
        symbol: candidate.symbol.clone(),
        direction: candidate.direction,
        setup_ts_ms: candidate.setup_ts_ms,
        breakout_ts_ms: candidate.breakout_ts_ms,
        signal_ts_ms: candidate.signal_ts_ms,
        signal_time_bjt: super::format_bjt(candidate.signal_ts_ms)?,
        signal_ema144: candidate.ema144,
        signal_atr14: candidate.atr14,
        entry_ts_ms: None,
        entry_price: None,
        initial_stop_price: None,
        initial_risk: None,
        initial_risk_atr: None,
        initial_risk_pct: None,
        target_price: None,
        stop_cost_r: None,
        source_v16_eligible,
        eligible: false,
        blocked_reason: None,
    };
    let plan = match inspect_entry_risk(
        data,
        candidate,
        InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR_V26),
        TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R_V26),
    ) {
        Ok(plan) => plan,
        Err(reason) => {
            result.blocked_reason = Some(reason);
            return Ok(result);
        }
    };
    result.entry_ts_ms = Some(plan.entry_ts_ms);
    result.entry_price = Some(plan.entry_price);
    result.initial_stop_price = Some(plan.stop_price);
    result.initial_risk = Some(plan.initial_risk);
    result.initial_risk_atr = Some(plan.initial_risk / candidate.atr14);
    result.initial_risk_pct = Some(plan.initial_risk / plan.entry_price * 100.0);
    result.target_price = Some(plan.target_price);
    let stop_cost_r =
        match stop_cost_r_for_prices(plan.entry_price, plan.stop_price, plan.initial_risk) {
            Ok(value) => value,
            Err(reason) => {
                result.blocked_reason = Some(reason);
                return Ok(result);
            }
        };
    result.stop_cost_r = Some(stop_cost_r);
    result.eligible = stop_cost_r <= MAX_STOP_COST_R_V26 + 1e-12;
    if !result.eligible {
        result.blocked_reason = Some("stop_cost_r_above_max");
    }
    Ok(result)
}

/// 生成与 V14/V25 一致的稳定候选身份。
fn candidate_id(candidate: &V2Candidate) -> String {
    format!(
        "{}:{}:{}",
        candidate.symbol, candidate.signal_ts_ms, candidate.direction
    )
}

/// 对排序身份逐行编码，冻结后续 L2 唯一允许消费的信号集合。
fn candidate_set_sha256(ids: &BTreeSet<String>) -> String {
    let mut encoded = String::new();
    for id in ids {
        encoded.push_str(id);
        encoded.push('\n');
    }
    super::sha256_hex(encoded.as_bytes())
}

/// 检查冻结 V25 行没有意外携带成交后字段。
fn validate_source_row_has_no_outcome(row: &Value) -> Result<()> {
    let object = row
        .as_object()
        .context("V25 opportunity is not an object")?;
    if let Some(field) = FORBIDDEN_OUTCOME_FIELDS
        .iter()
        .find(|field| object.contains_key(**field))
    {
        bail!("V25 forbidden outcome field present: {field}");
    }
    Ok(())
}

/// 检查 V26 序列化账本没有混入成交后字段。
fn validate_no_outcome_fields(opportunities: &[V26RiskOpportunity]) -> Result<()> {
    for opportunity in opportunities {
        let value = serde_json::to_value(opportunity)?;
        validate_source_row_has_no_outcome(&value)?;
    }
    Ok(())
}

/// 按方向与一小时连续触发窗口归并信号时市场事件。
fn effective_event_count(opportunities: &[&V26RiskOpportunity]) -> usize {
    let mut ordered = opportunities.to_vec();
    ordered.sort_by_key(|item| (item.signal_ts_ms, item.direction, item.symbol.as_str()));
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0usize;
    for item in ordered {
        let starts_new = last_by_direction
            .get(item.direction)
            .is_none_or(|previous| item.signal_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(item.direction, item.signal_ts_ms);
    }
    count
}

/// 生成最小、P10、中位、P90 与最大值的冻结离散分布。
fn distribution(mut values: Vec<f64>) -> Result<V26Distribution> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        bail!("V26 distribution is empty or non-finite");
    }
    values.sort_by(f64::total_cmp);
    Ok(V26Distribution {
        count: values.len(),
        min: values[0],
        p10: quantile(&values, 0.10),
        median: quantile(&values, 0.50),
        p90: quantile(&values, 0.90),
        max: values[values.len() - 1],
    })
}

/// 返回 floor((n-1)*p) 对应的冻结样本值。
fn quantile(sorted: &[f64], probability: f64) -> f64 {
    let index = ((sorted.len().saturating_sub(1)) as f64 * probability).floor() as usize;
    sorted[index]
}

/// 返回 part 占 total 的百分比；空分母固定为零。
fn ratio_pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}

/// 写入唯一 V26 L1 机器报告，不产生数据库副作用。
fn write_report(output: &Path, report: &V26MachineReport) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V26 报告目录失败：{}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(report)?;
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V26 L1 报告失败：{}", output.display()))
}
