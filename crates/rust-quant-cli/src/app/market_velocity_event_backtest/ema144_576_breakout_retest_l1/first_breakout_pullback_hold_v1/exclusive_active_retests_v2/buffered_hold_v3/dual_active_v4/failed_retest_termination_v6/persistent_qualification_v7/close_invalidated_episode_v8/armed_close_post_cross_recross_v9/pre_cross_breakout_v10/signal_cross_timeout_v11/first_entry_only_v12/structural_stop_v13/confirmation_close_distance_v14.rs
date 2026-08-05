//! V13 风险合同下，确认收盘距 EMA144 过远时拒绝入场的独立研究版本。
//!
//! L1 只筛选冻结 V11 候选的信号时字段；被距离过滤的候选不属于真实成交，
//! 因而不消费 V12/V13 的 setup 首笔成交资格。

pub mod net_target_v15;

use super::super::super::super::l2::{
    replay::{replay_verified_candidate_ledger, ReplaySource},
    EntryRiskGatePolicy, InitialRiskPolicy, SetupEntryPolicy, TargetRiskPolicy, V10L2Identity,
    V10L2Report,
};
use super::*;
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// V14 独立候选身份；距离过滤不得覆盖 V13 结构止损版本。
pub const V14_CANDIDATE_KEY: &str =
    "market_momentum_ema576_first_entry_ema144_structural_stop_close_distance_15m_v14";
/// V14 L1 规则身份；`cap100` 表示确认收盘绝对距离不超过 1.00 ATR14。
pub const V14_L1_RULE_VERSION: &str =
    "l1_v14_v13_abs_signal_close_to_ema144_atr_cap100_no_outcome_v1";
/// V14 L2 精确规则身份；只在 V13 上增加信号收盘 1.00 ATR 距离上限。
pub const V14_L2_RULE_VERSION: &str =
    "l2_v14_v13_abs_close_ema144_cap100_first_filled_structural030_r052_cost8bps_v1";

const EXPECTED_V11_L1_REPORT_SHA256: &str =
    "ebc2b886d1a64e6900e81900bc59a5b0a93c245437d8537642dbfe4f9b64e1e6";
const EXPECTED_V14_L1_REPORT_SHA256: &str =
    "23edf396fe1b82681789c9323d38a30b17e1defa03d9d8b4551ac2f059e7b475";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_SELECTED_CANDIDATES: usize = 15_024;
const PREREGISTERED_CAPS_ATR: [f64; 3] = [0.50, 0.75, 1.00];
const SELECTED_CAP_ATR: f64 = 1.00;
const MIN_AFFECTED_RATIO_PCT: f64 = 10.0;
const MAX_AFFECTED_RATIO_PCT: f64 = 60.0;
const MIN_KEPT_RATIO_PCT: f64 = 40.0;
const MIN_SYMBOLS: usize = 8;
const MIN_MONTHS: usize = 6;
const MIN_EVENTS: usize = 100;
const MIN_DIRECTION_CANDIDATES: usize = 10;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const LAYER_TARGET_SIGNAL_MS: i64 = 1_784_427_300_000;
const LAYER_LATER_SIGNAL_MS: i64 = 1_784_432_700_000;

const FORBIDDEN_OUTCOME_FIELDS: [&str; 14] = [
    "complete",
    "entry_price",
    "entry_ts_ms",
    "exit_price",
    "exit_reason",
    "exit_ts_ms",
    "gross_r",
    "net_r",
    "cost_r",
    "mfe",
    "mae",
    "pnl",
    "win",
    "loss",
];

/// V14 L1 的单变量、标签边界和运行隔离身份。
#[derive(Debug, Clone, Serialize)]
pub struct V14L1Identity {
    /// 当前研究等级；L1 禁止读取成交后结果。
    pub level: &'static str,
    /// 与 V13 并存的独立候选键。
    pub candidate_key: &'static str,
    /// 确认收盘距离上限的精确规则版本。
    pub rule_version: &'static str,
    /// 唯一允许变化的信号时过滤条件。
    pub only_variable: &'static str,
    /// 距离过滤失败与 setup 消费之间的生命周期合同。
    pub setup_consumption_policy: &'static str,
    /// L1 明确禁止读取的成交后标签边界。
    pub label_boundary: &'static str,
    /// Research-only 与运行态隔离边界。
    pub runtime_boundary: &'static str,
}

/// V14 L1 复用的冻结行情与成员身份。
#[derive(Debug, Clone, Serialize)]
pub struct V14CoverageIdentity {
    /// 冻结 15m K 线、成员和窗口的 SHA-256 指纹。
    pub dataset_fingerprint_sha256: String,
    /// 本地加载器返回的现时 Top60 成员数。
    pub returned_symbol_count: usize,
    /// 具备完整预热和评价窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 因缺 K 或预热不足而跳过的成员数。
    pub excluded_symbol_count: usize,
}

/// 冻结 V11 候选中确认收盘绝对距离的无 outcome 分布。
#[derive(Debug, Clone, Serialize)]
pub struct V14DistanceDistribution {
    /// 参与分布统计的候选数量。
    pub count: usize,
    /// 最小绝对距离，单位 ATR14。
    pub min_atr: f64,
    /// 25% 分位绝对距离，单位 ATR14。
    pub p25_atr: f64,
    /// 中位绝对距离，单位 ATR14。
    pub median_atr: f64,
    /// 75% 分位绝对距离，单位 ATR14。
    pub p75_atr: f64,
    /// 90% 分位绝对距离，单位 ATR14。
    pub p90_atr: f64,
    /// 95% 分位绝对距离，单位 ATR14。
    pub p95_atr: f64,
    /// 99% 分位绝对距离，单位 ATR14。
    pub p99_atr: f64,
    /// 最大绝对距离，单位 ATR14。
    pub max_atr: f64,
}

/// 一个预注册距离 cap 的无 outcome 覆盖与目标样本结果。
#[derive(Debug, Clone, Serialize)]
pub struct V14VariantSummary {
    /// 允许的最大确认收盘绝对距离，单位 ATR14。
    pub cap_atr: f64,
    /// 距离过滤前的冻结 V11 候选数。
    pub baseline_candidates: usize,
    /// 应用本 cap 后保留的候选数。
    pub kept_candidates: usize,
    /// 因距离超过本 cap 而删除的候选数。
    pub removed_candidates: usize,
    /// 删除候选占冻结基线的百分比。
    pub affected_ratio_pct: f64,
    /// 保留候选占冻结基线的百分比。
    pub kept_ratio_pct: f64,
    /// 保留候选的多空数量。
    pub by_direction: BTreeMap<String, usize>,
    /// 保留候选覆盖的 OKX 永续合约数。
    pub symbol_count: usize,
    /// 保留候选覆盖的 UTC 月份数。
    pub month_count: usize,
    /// 按方向与一小时连续触发链归并的事件数。
    pub effective_market_events: usize,
    /// true 表示用户指出的 LAYER 10:15 异常 K 被拒绝。
    pub layer_target_rejected: bool,
    /// true 表示同 setup 的 LAYER 11:45 较近候选仍保留。
    pub layer_later_candidate_kept: bool,
    /// true 表示本变体满足预注册覆盖与目标样本门禁。
    pub coverage_gate_passed: bool,
}

/// LAYER 两根候选的信号时距离与过滤状态审计。
#[derive(Debug, Clone, Serialize)]
pub struct V14LayerAudit {
    /// 用户指出的异常确认 K 时间，Unix 毫秒。
    pub target_signal_ts_ms: i64,
    /// 异常确认 K 的绝对收盘距离，单位 ATR14。
    pub target_abs_close_distance_atr: f64,
    /// true 表示主 cap 已拒绝异常确认 K。
    pub target_rejected: bool,
    /// 同 setup 后续较近候选时间，Unix 毫秒。
    pub later_signal_ts_ms: i64,
    /// 后续候选的绝对收盘距离，单位 ATR14。
    pub later_abs_close_distance_atr: f64,
    /// true 表示后续候选仍可尝试成为该 setup 的首笔真实成交。
    pub later_candidate_kept: bool,
}

/// V14 L1 是否允许进入一次冻结 L2 回放的无标签结论。
#[derive(Debug, Clone, Serialize)]
pub struct V14L1Decision {
    /// `coverage_pass_ready_for_l2_prereg` 或 `stop`。
    pub status: &'static str,
    /// 各项目标样本、覆盖、选择和标签门禁。
    pub gates: BTreeMap<String, bool>,
    /// true 表示 L1 没有读取成交、退出或收益字段。
    pub outcome_evaluation_performed: bool,
    /// 当前停止或升级依据。
    pub reason: String,
}

/// V14 L1 完整机器结果；候选只含信号完成时及此前可见字段。
#[derive(Debug, Clone, Serialize)]
pub struct V14L1Report {
    /// V14 L1 JSON 字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339；不参与行情身份。
    pub generated_at_utc: String,
    /// 单变量、标签与运行隔离身份。
    pub identity: V14L1Identity,
    /// 冻结 V11 L1 文件 SHA-256。
    pub source_v11_l1_report_sha256: String,
    /// 复用的行情与成员身份。
    pub coverage: V14CoverageIdentity,
    /// 全量冻结候选的信号时距离分布。
    pub distance_distribution: V14DistanceDistribution,
    /// 三个预注册 cap 的同账本无标签结果。
    pub variants: Vec<V14VariantSummary>,
    /// 按预注册“满足门禁后取最大 cap”选出的唯一 L2 cap。
    pub selected_cap_atr: f64,
    /// LAYER 异常与后续较近候选的因果审计。
    pub layer_audit: V14LayerAudit,
    /// 无标签覆盖结论。
    pub decision: V14L1Decision,
    /// 主 cap 保留的候选账本，不含成交后字段。
    pub candidates: Vec<Value>,
}

/// V14 单一机器产物，把无标签 L1 证据与唯一主候选 L2 回放放在同一文件。
#[derive(Debug, Clone, Serialize)]
pub struct V14MachineReport {
    /// V14 合并机器结果字段合同版本。
    pub schema_version: &'static str,
    /// 合并报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// 逐字段校验且 SHA 固定的 V14 L1 机器证据。
    pub l1: Value,
    /// 1.00 ATR 主候选在 V13 冻结成交与风险合同下的 L2 结果。
    pub l2: V10L2Report,
}

/// V14/V15 共用的已验证行情、成员身份和距离合格候选。
struct V14ReplayInput {
    data: BacktestDataSet,
    dataset_fingerprint_sha256: String,
    returned_symbol_count: usize,
    eligible_symbol_count: usize,
    excluded_symbol_count: usize,
    candidates: Vec<V2Candidate>,
}

/// 从冻结 V11 L1 文件生成 V14 距离分布、三变体覆盖和唯一主候选账本。
pub fn run_v14_l1_scan(v11_l1_source: &Path, output: &Path) -> Result<V14L1Report> {
    let bytes = std::fs::read(v11_l1_source)
        .with_context(|| format!("读取冻结 V11 L1 报告失败：{}", v11_l1_source.display()))?;
    let source_sha256 = sha256_hex(&bytes);
    if source_sha256 != EXPECTED_V11_L1_REPORT_SHA256 {
        bail!("V14 source V11 L1 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析冻结 V11 L1 报告失败")?;
    super::super::super::l2::validate_source_identity(&source)?;
    let source_candidates = source
        .pointer("/candidates")
        .and_then(Value::as_array)
        .context("V14 source V11 L1 candidates missing")?;
    validate_no_outcome_fields(source_candidates)?;

    let report = build_v14_l1_report(&source, source_candidates, source_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V14 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V14 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 校验冻结 V14 L1 后，重建同一行情与候选并执行唯一一次 V13 合同 L2 回放。
pub async fn run_v14_l2_replay(v14_l1_source: &Path, output: &Path) -> Result<V14MachineReport> {
    let bytes = std::fs::read(v14_l1_source)
        .with_context(|| format!("读取冻结 V14 L1 报告失败：{}", v14_l1_source.display()))?;
    let l1_sha256 = sha256_hex(&bytes);
    if l1_sha256 != EXPECTED_V14_L1_REPORT_SHA256 {
        bail!("V14 L1 report SHA mismatch");
    }
    let l1: Value = serde_json::from_slice(&bytes).context("解析冻结 V14 L1 报告失败")?;
    let replay_input = load_verified_v14_replay_input(&l1).await?;

    let l2 = replay_verified_candidate_ledger(
        &replay_input.data,
        ReplaySource::new(
            "market_momentum_ema576_confirmation_close_distance_l2_v14",
            V10L2Identity {
                level: "L2_local_multi_symbol_diagnostic",
                candidate_key: V14_CANDIDATE_KEY,
                source_l1_rule_version: V14_L1_RULE_VERSION,
                rule_version: V14_L2_RULE_VERSION,
                only_variable: "reject completed signal candles whose absolute close-to-EMA144 distance exceeds the preregistered 1.00 ATR14 cap",
                entry_policy: "distance-qualified signals retain V13 next-contiguous-15m-open execution; a rejected signal does not consume the setup, while the first later qualifying real fill does",
                initial_stop_policy: "unchanged V13 long signal EMA144 minus 0.30 ATR14 and short signal EMA144 plus 0.30 ATR14; entries already beyond the structural stop are blocked",
                target_policy: "unchanged fixed 0.52R from actual entry-to-structural-stop risk with no break-even, trailing, partial, runner, or reversal",
                intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
                symbol_position_policy: "unchanged one open trade per symbol and one real fill per symbol x direction x setup_ts",
                per_side_cost_rate: PER_SIDE_COST_RATE,
                max_holding_ms: MAX_HOLDING_MS,
                funding_modeled: false,
                outcome_evaluation_performed: true,
                runtime_boundary: "research-only V14 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
            },
            l1_sha256,
            replay_input.dataset_fingerprint_sha256,
            replay_input.returned_symbol_count,
            replay_input.eligible_symbol_count,
            replay_input.excluded_symbol_count,
            SetupEntryPolicy::FirstFilledPerSetup,
            InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR),
            TargetRiskPolicy::FixedGrossR,
            EntryRiskGatePolicy::AllowAnyPositiveRisk,
            replay_input.candidates,
        ),
    );
    let report = V14MachineReport {
        schema_version: "market_momentum_ema576_confirmation_close_distance_l1_l2_v14",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        l1,
        l2,
    };
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V14 机器报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V14 机器报告失败：{}", output.display()))?;
    Ok(report)
}

/// 重新加载冻结行情并逐字段核对 V14 距离账本，供退出政策版本安全复用。
async fn load_verified_v14_replay_input(l1: &Value) -> Result<V14ReplayInput> {
    validate_v14_l1_source(l1)?;
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for verified V14 candidate replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let rebuilt = build_v11_report(&data)?;
    if rebuilt.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || rebuilt.coverage.returned_symbol_count != 60
        || rebuilt.coverage.eligible_symbol_count != 44
        || rebuilt.coverage.excluded_symbols.len() != 16
    {
        bail!("V14 reloaded dataset or universe identity mismatch");
    }
    let dataset_fingerprint_sha256 = rebuilt.coverage.dataset_fingerprint_sha256;
    let returned_symbol_count = rebuilt.coverage.returned_symbol_count;
    let eligible_symbol_count = rebuilt.coverage.eligible_symbol_count;
    let excluded_symbol_count = rebuilt.coverage.excluded_symbols.len();
    let candidates = rebuilt
        .candidates
        .into_iter()
        .filter(|candidate| {
            candidate.close_to_ema144_directional_atr.is_finite()
                && candidate.close_to_ema144_directional_atr.abs() <= SELECTED_CAP_ATR
        })
        .collect::<Vec<_>>();
    if candidates.len() != EXPECTED_SELECTED_CANDIDATES
        || l1.pointer("/candidates") != Some(&serde_json::to_value(&candidates)?)
    {
        bail!("V14 reloaded filtered candidate ledger differs from frozen L1");
    }
    Ok(V14ReplayInput {
        data,
        dataset_fingerprint_sha256,
        returned_symbol_count,
        eligible_symbol_count,
        excluded_symbol_count,
        candidates,
    })
}

/// 只读取信号时字段，生成分布、变体、预注册选择与主候选账本。
fn build_v14_l1_report(
    source: &Value,
    source_candidates: &[Value],
    source_sha256: String,
) -> Result<V14L1Report> {
    let mut distances = source_candidates
        .iter()
        .map(candidate_abs_close_distance_atr)
        .collect::<Result<Vec<_>>>()?;
    distances.sort_by(f64::total_cmp);
    let distance_distribution = distance_distribution(&distances)?;
    let variants = PREREGISTERED_CAPS_ATR
        .iter()
        .map(|cap| summarize_variant(source_candidates, *cap))
        .collect::<Result<Vec<_>>>()?;

    // 阈值只能按预注册覆盖规则选择；即使机器文件未来带有 outcome，也不得进入此排序。
    let selected = variants
        .iter()
        .filter(|variant| variant.coverage_gate_passed)
        .max_by(|left, right| {
            left.cap_atr
                .partial_cmp(&right.cap_atr)
                .unwrap_or(Ordering::Equal)
        })
        .context("V14 no preregistered close-distance cap passed L1 coverage")?;
    let selected_cap_atr = selected.cap_atr;
    let selected_kept_candidates = selected.kept_candidates;
    let selected_coverage_gate_passed = selected.coverage_gate_passed;
    let selected_cap_matches_preregistered = approx_equal(selected_cap_atr, SELECTED_CAP_ATR);
    let candidates = filter_candidates(source_candidates, selected_cap_atr)?;
    let layer_audit = layer_audit(source_candidates, selected_cap_atr)?;
    let coverage = coverage_identity(source)?;

    let mut gates = BTreeMap::new();
    gates.insert(
        "all_candidates_have_finite_signal_distance".to_owned(),
        distances.len() == source_candidates.len(),
    );
    gates.insert("forbidden_outcome_fields_absent".to_owned(), true);
    gates.insert(
        "layer_target_rejected".to_owned(),
        layer_audit.target_rejected,
    );
    gates.insert(
        "selected_variant_coverage_passed".to_owned(),
        selected_coverage_gate_passed,
    );
    gates.insert(
        "selected_cap_matches_preregistered_rule".to_owned(),
        selected_cap_matches_preregistered,
    );
    gates.insert(
        "selected_candidate_count_matches".to_owned(),
        candidates.len() == selected_kept_candidates,
    );
    let passed = gates.values().all(|passed| *passed);

    Ok(V14L1Report {
        schema_version: "market_momentum_ema576_confirmation_close_distance_l1_v14",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V14L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V14_CANDIDATE_KEY,
            rule_version: V14_L1_RULE_VERSION,
            only_variable: "require the completed signal candle absolute close-to-EMA144 distance to stay within one preregistered ATR14 cap",
            setup_consumption_policy: "a distance-rejected signal is not a real fill and does not consume the setup; the first later qualifying real fill consumes it",
            label_boundary: "cap selection reads only signal-time close_to_ema144_directional_atr and coverage identity; no fill, exit, MFE, MAE, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V14 L1; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        source_v11_l1_report_sha256: source_sha256,
        coverage,
        distance_distribution,
        variants,
        selected_cap_atr,
        layer_audit,
        decision: V14L1Decision {
            status: if passed {
                "coverage_pass_ready_for_l2_prereg"
            } else {
                "stop"
            },
            gates,
            outcome_evaluation_performed: false,
            reason: if passed {
                "LAYER 异常确认 K 被拒绝，最大合格 cap 按预注册规则唯一选出且保持多币种、多月份、双方向和事件覆盖；允许进入一次冻结 L2。".to_owned()
            } else {
                "至少一个无 outcome 目标样本、覆盖、选择或标签边界门禁失败；停止在 L1。".to_owned()
            },
        },
        candidates,
    })
}

/// 校验 L1 身份、无 outcome 门禁、主 cap、候选数和目标样本均与预注册一致。
fn validate_v14_l1_source(source: &Value) -> Result<()> {
    if source_string(source, "/schema_version")?
        != "market_momentum_ema576_confirmation_close_distance_l1_v14"
        || source_string(source, "/identity/candidate_key")? != V14_CANDIDATE_KEY
        || source_string(source, "/identity/rule_version")? != V14_L1_RULE_VERSION
        || source_string(source, "/source_v11_l1_report_sha256")? != EXPECTED_V11_L1_REPORT_SHA256
        || source_string(source, "/coverage/dataset_fingerprint_sha256")?
            != EXPECTED_DATASET_FINGERPRINT_SHA256
    {
        bail!("V14 L1 strategy or dataset identity mismatch");
    }
    let selected_cap = source
        .pointer("/selected_cap_atr")
        .and_then(Value::as_f64)
        .context("V14 L1 selected cap missing")?;
    let candidates = source
        .pointer("/candidates")
        .and_then(Value::as_array)
        .context("V14 L1 candidates missing")?;
    validate_no_outcome_fields(candidates)?;
    if !approx_equal(selected_cap, SELECTED_CAP_ATR)
        || candidates.len() != EXPECTED_SELECTED_CANDIDATES
        || candidates
            .iter()
            .any(|candidate| candidate_abs_close_distance_atr(candidate).is_err())
        || candidates.iter().any(|candidate| {
            candidate_abs_close_distance_atr(candidate)
                .is_ok_and(|distance| distance > SELECTED_CAP_ATR)
        })
        || source_string(source, "/decision/status")? != "coverage_pass_ready_for_l2_prereg"
        || source
            .pointer("/decision/outcome_evaluation_performed")
            .and_then(Value::as_bool)
            != Some(false)
        || source
            .pointer("/layer_audit/target_rejected")
            .and_then(Value::as_bool)
            != Some(true)
        || source
            .pointer("/decision/gates")
            .and_then(Value::as_object)
            .is_none_or(|gates| gates.values().any(|passed| passed.as_bool() != Some(true)))
    {
        bail!("V14 L1 is not eligible for L2 replay");
    }
    Ok(())
}

/// 汇总一个 cap 的删除比例、方向、成员、月份、事件和 LAYER 状态。
fn summarize_variant(candidates: &[Value], cap_atr: f64) -> Result<V14VariantSummary> {
    let mut kept = Vec::new();
    for candidate in candidates {
        if candidate_abs_close_distance_atr(candidate)? <= cap_atr {
            kept.push(candidate);
        }
    }
    let baseline_candidates = candidates.len();
    let kept_candidates = kept.len();
    let removed_candidates = baseline_candidates.saturating_sub(kept_candidates);
    let affected_ratio_pct = ratio_pct(removed_candidates, baseline_candidates);
    let kept_ratio_pct = ratio_pct(kept_candidates, baseline_candidates);
    let mut by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    for candidate in &kept {
        let direction = candidate_string(candidate, "direction")?;
        *by_direction.entry(direction.to_owned()).or_default() += 1;
        symbols.insert(candidate_string(candidate, "symbol")?.to_owned());
        months.insert(candidate_string(candidate, "signal_month_utc")?.to_owned());
    }
    let layer_target_rejected = !contains_signal(&kept, LAYER_TARGET_SIGNAL_MS)?;
    let layer_later_candidate_kept = contains_signal(&kept, LAYER_LATER_SIGNAL_MS)?;
    let effective_market_events = effective_event_count(&kept)?;
    let direction_coverage = ["long", "short"].iter().all(|direction| {
        by_direction.get(*direction).copied().unwrap_or_default() >= MIN_DIRECTION_CANDIDATES
    });
    let coverage_gate_passed = layer_target_rejected
        && affected_ratio_pct >= MIN_AFFECTED_RATIO_PCT
        && affected_ratio_pct <= MAX_AFFECTED_RATIO_PCT
        && kept_ratio_pct >= MIN_KEPT_RATIO_PCT
        && symbols.len() >= MIN_SYMBOLS
        && months.len() >= MIN_MONTHS
        && effective_market_events >= MIN_EVENTS
        && direction_coverage;

    Ok(V14VariantSummary {
        cap_atr,
        baseline_candidates,
        kept_candidates,
        removed_candidates,
        affected_ratio_pct,
        kept_ratio_pct,
        by_direction,
        symbol_count: symbols.len(),
        month_count: months.len(),
        effective_market_events,
        layer_target_rejected,
        layer_later_candidate_kept,
        coverage_gate_passed,
    })
}

/// 对 source 候选应用主 cap，并保持原有确定性时间顺序。
fn filter_candidates(candidates: &[Value], cap_atr: f64) -> Result<Vec<Value>> {
    let mut kept = Vec::new();
    for candidate in candidates {
        if candidate_abs_close_distance_atr(candidate)? <= cap_atr {
            kept.push(candidate.clone());
        }
    }
    Ok(kept)
}

/// 提取 LAYER 异常信号和后续较近信号，不读取两者成交后的任何结果。
fn layer_audit(candidates: &[Value], cap_atr: f64) -> Result<V14LayerAudit> {
    let target = find_layer_signal(candidates, LAYER_TARGET_SIGNAL_MS)?;
    let later = find_layer_signal(candidates, LAYER_LATER_SIGNAL_MS)?;
    let target_distance = candidate_abs_close_distance_atr(target)?;
    let later_distance = candidate_abs_close_distance_atr(later)?;
    Ok(V14LayerAudit {
        target_signal_ts_ms: LAYER_TARGET_SIGNAL_MS,
        target_abs_close_distance_atr: target_distance,
        target_rejected: target_distance > cap_atr,
        later_signal_ts_ms: LAYER_LATER_SIGNAL_MS,
        later_abs_close_distance_atr: later_distance,
        later_candidate_kept: later_distance <= cap_atr,
    })
}

/// 读取源报告的行情、成员和排除数量，不复制与本轮无关的审计载荷。
fn coverage_identity(source: &Value) -> Result<V14CoverageIdentity> {
    Ok(V14CoverageIdentity {
        dataset_fingerprint_sha256: source_string(source, "/coverage/dataset_fingerprint_sha256")?
            .to_owned(),
        returned_symbol_count: source_usize(source, "/coverage/returned_symbol_count")?,
        eligible_symbol_count: source_usize(source, "/coverage/eligible_symbol_count")?,
        excluded_symbol_count: source
            .pointer("/coverage/excluded_symbols")
            .and_then(Value::as_array)
            .map(Vec::len)
            .context("V14 source excluded_symbols missing")?,
    })
}

/// 验证候选对象没有意外混入 L1 禁止读取的 outcome 字段。
fn validate_no_outcome_fields(candidates: &[Value]) -> Result<()> {
    for candidate in candidates {
        let object = candidate
            .as_object()
            .context("V14 source candidate is not an object")?;
        if let Some(field) = FORBIDDEN_OUTCOME_FIELDS
            .iter()
            .find(|field| object.contains_key(**field))
        {
            bail!("V14 L1 forbidden outcome field present: {field}");
        }
    }
    Ok(())
}

/// 计算候选确认收盘相对 EMA144 的绝对 ATR14 距离。
fn candidate_abs_close_distance_atr(candidate: &Value) -> Result<f64> {
    let distance = candidate
        .get("close_to_ema144_directional_atr")
        .and_then(Value::as_f64)
        .context("V14 candidate close distance missing")?
        .abs();
    if !distance.is_finite() {
        bail!("V14 candidate close distance is not finite");
    }
    Ok(distance)
}

/// 使用离散 nearest-rank 下标生成与预注册 jq 扫描一致的分位数。
fn distance_distribution(sorted: &[f64]) -> Result<V14DistanceDistribution> {
    let first = sorted.first().copied().context("V14 distance set empty")?;
    let last = sorted.last().copied().context("V14 distance set empty")?;
    Ok(V14DistanceDistribution {
        count: sorted.len(),
        min_atr: first,
        p25_atr: quantile(sorted, 0.25),
        median_atr: quantile(sorted, 0.50),
        p75_atr: quantile(sorted, 0.75),
        p90_atr: quantile(sorted, 0.90),
        p95_atr: quantile(sorted, 0.95),
        p99_atr: quantile(sorted, 0.99),
        max_atr: last,
    })
}

/// 返回 `floor((n-1)*p)` 对应的冻结样本值。
fn quantile(sorted: &[f64], probability: f64) -> f64 {
    let index = ((sorted.len().saturating_sub(1)) as f64 * probability).floor() as usize;
    sorted[index]
}

/// 按方向和与上一同方向信号相隔是否超过一小时归并事件链。
fn effective_event_count(candidates: &[&Value]) -> Result<usize> {
    let mut ordered = candidates
        .iter()
        .map(|candidate| {
            Ok((
                candidate_string(candidate, "direction")?.to_owned(),
                candidate_i64(candidate, "signal_ts_ms")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    ordered.sort_by(|left, right| left.cmp(right));
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for (direction, signal_ts_ms) in ordered {
        let starts_new = last_by_direction
            .get(&direction)
            .is_none_or(|previous| signal_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(direction, signal_ts_ms);
    }
    Ok(count)
}

/// 返回指定 LAYER 信号；币种与时间共同防止误命中其他候选。
fn find_layer_signal(candidates: &[Value], signal_ts_ms: i64) -> Result<&Value> {
    candidates
        .iter()
        .find(|candidate| {
            candidate.get("symbol").and_then(Value::as_str) == Some("LAYER-USDT-SWAP")
                && candidate.get("signal_ts_ms").and_then(Value::as_i64) == Some(signal_ts_ms)
        })
        .with_context(|| format!("V14 LAYER signal missing at {signal_ts_ms}"))
}

/// 判断过滤后候选引用是否包含指定 LAYER 信号时间。
fn contains_signal(candidates: &[&Value], signal_ts_ms: i64) -> Result<bool> {
    Ok(candidates.iter().any(|candidate| {
        candidate.get("symbol").and_then(Value::as_str) == Some("LAYER-USDT-SWAP")
            && candidate.get("signal_ts_ms").and_then(Value::as_i64) == Some(signal_ts_ms)
    }))
}

/// 从候选对象读取必需字符串字段。
fn candidate_string<'a>(candidate: &'a Value, field: &str) -> Result<&'a str> {
    candidate
        .get(field)
        .and_then(Value::as_str)
        .with_context(|| format!("V14 candidate string field missing: {field}"))
}

/// 从候选对象读取必需 Unix 毫秒时间字段。
fn candidate_i64(candidate: &Value, field: &str) -> Result<i64> {
    candidate
        .get(field)
        .and_then(Value::as_i64)
        .with_context(|| format!("V14 candidate i64 field missing: {field}"))
}

/// 从源报告 JSON Pointer 读取字符串身份字段。
fn source_string<'a>(source: &'a Value, pointer: &str) -> Result<&'a str> {
    source
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("V14 source string field missing: {pointer}"))
}

/// 从源报告 JSON Pointer 读取成员数量字段。
fn source_usize(source: &Value, pointer: &str) -> Result<usize> {
    let value = source
        .pointer(pointer)
        .and_then(Value::as_u64)
        .with_context(|| format!("V14 source unsigned field missing: {pointer}"))?;
    usize::try_from(value).with_context(|| format!("V14 source field exceeds usize: {pointer}"))
}

/// 把计数转为百分比；冻结源非空，零分母仅作防御性返回。
fn ratio_pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

/// 比较预注册浮点 cap，避免序列化往返造成无意义差异。
fn approx_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-12
}

/// 计算冻结源文件 SHA-256，防止 L1 静默切换候选账本。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 构造只含 V14 过滤所需信号时字段的候选。
    fn candidate(symbol: &str, signal_ts_ms: i64, distance: f64) -> Value {
        json!({
            "symbol": symbol,
            "direction": "long",
            "signal_ts_ms": signal_ts_ms,
            "signal_month_utc": "2026-07",
            "close_to_ema144_directional_atr": distance
        })
    }

    #[test]
    fn close_distance_filter_is_absolute_and_keeps_boundary() {
        let candidates = vec![
            candidate("A-USDT-SWAP", 1, -1.0),
            candidate("B-USDT-SWAP", 2, 1.0),
            candidate("C-USDT-SWAP", 3, -1.01),
            candidate("D-USDT-SWAP", 4, 1.01),
        ];
        let kept = filter_candidates(&candidates, 1.0).expect("filter candidates");
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0]["symbol"], "A-USDT-SWAP");
        assert_eq!(kept[1]["symbol"], "B-USDT-SWAP");
    }

    #[test]
    fn rejected_layer_signal_does_not_remove_later_candidate() {
        let candidates = vec![
            candidate("LAYER-USDT-SWAP", LAYER_TARGET_SIGNAL_MS, 1.4634),
            candidate("LAYER-USDT-SWAP", LAYER_LATER_SIGNAL_MS, 0.2982),
        ];
        let kept = filter_candidates(&candidates, 1.0).expect("filter LAYER candidates");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["signal_ts_ms"], LAYER_LATER_SIGNAL_MS);
    }

    #[test]
    fn l1_rejects_candidate_with_outcome_field() {
        let mut candidate = candidate("A-USDT-SWAP", 1, 0.2);
        candidate["net_r"] = json!(0.5);
        let error = validate_no_outcome_fields(&[candidate]).expect_err("outcome must fail");
        assert!(error.to_string().contains("net_r"));
    }

    #[test]
    fn event_count_splits_only_after_one_hour_per_direction() {
        let long_a = candidate("A-USDT-SWAP", 0, 0.2);
        let long_b = candidate("B-USDT-SWAP", EVENT_CLUSTER_WINDOW_MS, 0.2);
        let long_c = candidate("C-USDT-SWAP", EVENT_CLUSTER_WINDOW_MS * 2 + 1, 0.2);
        let candidates = vec![&long_a, &long_b, &long_c];
        assert_eq!(effective_event_count(&candidates).expect("event count"), 2);
    }
}
