//! V16 后续入场质量研究共用的无标签特征重建与冻结 L2 回放。

use super::*;
use crate::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::ema_close_series;
use crate::app::market_velocity_event_backtest::ComputedCandle;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

const EXPECTED_V16_REPORT_SHA256: &str =
    "0c199ad758becefab58a03da53837e85c022b6e4d0e510fa7b34c26332307cf2";
const EXPECTED_V16_CANDIDATES: usize = 15_024;
const EXPECTED_V16_ELIGIBLE: usize = 8_732;
const EMA576_PERIOD: usize = 576;
const RETEST_ZONE_ATR: f64 = 0.30;
const ACCEPTANCE_CLOSES: usize = 8;
const SIX_CLOSE_ACCEPTANCE_CLOSES: usize = 6;
const BREAKOUT_DISTANCE_ATR: f64 = 2.50;
const RELAXED_BREAKOUT_DISTANCE_ATR: f64 = 2.00;
const MIN_DIRECTION_RETAINED: usize = 100;
const MIN_SYMBOLS: usize = 8;
const MIN_MONTHS_BJT: usize = 6;
const MIN_EVENTS: usize = 100;
/// 单个研究批次唯一允许变化的信号时质量维度。
#[derive(Debug, Clone, Copy)]
pub(super) enum QualityRule {
    /// setup 到突破必须保持同一 EMA144/576 关系周期。
    QualificationCycleFresh,
    /// 突破确认收盘必须离 EMA576 至少 2.50 ATR14。
    BreakoutDistance2_5Atr,
    /// 第一根越线后必须先完成连续八收盘接受。
    BreakoutAcceptance8Bars,
    /// 用户明确指定：资格周期、2.50 ATR 与八收盘必须同时通过。
    CompositeCycleDistanceAcceptance,
    /// V21 仅把 V20 的突破距离放宽到 2.00 ATR。
    CompositeCycleDistance2_0Acceptance,
    /// V22 仅把 V21 的提前失效边界替换为 EMA576 盘中穿越。
    CompositeCycleDistance2_0AcceptanceEma576Hold,
    /// V23 要求资格时的 EMA144/576 关系一直保持到信号完成。
    CompositeCycleDistance2_0AcceptanceEma576HoldRelationUntilSignal,
    /// V24 把 2ATR 动量距离改为八根接受窗口内的最大顺势极值。
    CompositeCycleAcceptanceWindowExtreme2_0Ema576HoldRelationUntilSignal,
    /// V25 只把 V24 的接受与顺势极值窗口从八根缩短为六根。
    CompositeCycleAcceptanceWindowExtreme2_0SixCloseEma576HoldRelationUntilSignal,
}
/// 连续接受窗口完成前，用哪条结构边界识别过早回踩。
#[derive(Debug, Clone, Copy)]
enum AcceptanceBoundary {
    /// V19～V21 冻结的 EMA144±0.30ATR 回踩区。
    Ema144RetestZone,
    /// V22 指定的 EMA576 盘中严格穿越边界。
    Ema576IntrabarHold,
}
/// 一个独立 L1/L2 批次的冻结身份、目标样本与覆盖边界。
pub(super) struct QualitySpec {
    /// 独立候选键。
    pub candidate_key: &'static str,
    /// L1 精确规则版本。
    pub l1_rule_version: &'static str,
    /// L2 精确规则版本。
    pub l2_rule_version: &'static str,
    /// 合并机器报告 schema。
    pub machine_schema_version: &'static str,
    /// L1 报告 schema。
    pub l1_schema_version: &'static str,
    /// L2 报告 schema。
    pub l2_schema_version: &'static str,
    /// 相对 V16 的唯一变化。
    pub only_variable: &'static str,
    /// 失败机会是否消费 setup 或突破 episode。
    pub setup_consumption_policy: &'static str,
    /// L1 可读取的因果字段。
    pub causal_field_boundary: &'static str,
    /// L2 入场政策说明。
    pub entry_policy: &'static str,
    /// 当前批次唯一规则。
    pub rule: QualityRule,
    /// 事前预计最小影响比例。
    pub min_affected_ratio_pct: f64,
    /// 事前预计最大影响比例。
    pub max_affected_ratio_pct: f64,
    /// 必须被新门禁拒绝的用户目标样本。
    pub target_samples: &'static [TargetSample],
}

/// 从已冻结 L1 候选集合执行一次 L2 时必须绑定的文件、集合与规则身份。
pub(super) struct FrozenQualityL2Spec {
    /// L1 合并机器报告 schema。
    pub source_machine_schema_version: &'static str,
    /// L1 子报告 schema。
    pub source_l1_schema_version: &'static str,
    /// 冻结 L1 完整文件 SHA-256。
    pub expected_l1_report_sha256: &'static str,
    /// 冻结 L1 payload SHA-256。
    pub expected_l1_payload_sha256: &'static str,
    /// L1 候选版本。
    pub candidate_key: &'static str,
    /// L1 信号规则版本。
    pub source_l1_rule_version: &'static str,
    /// L2 回放规则版本。
    pub l2_rule_version: &'static str,
    /// L2 机器报告 schema。
    pub l2_schema_version: &'static str,
    /// L2 唯一允许读取 outcome 的假设。
    pub only_variable: &'static str,
    /// 冻结的下一根开盘与成交前门禁说明。
    pub entry_policy: &'static str,
    /// 预注册的 L1 合格候选总数。
    pub expected_candidate_count: usize,
    /// 预注册的多头候选数。
    pub expected_long_count: usize,
    /// 预注册的空头候选数。
    pub expected_short_count: usize,
    /// 排序候选 ID 逐行编码后的 SHA-256。
    pub expected_candidate_set_sha256: &'static str,
}

/// 冻结 L1 中一条合格机会用于和 V14 重建候选逐字段核对的身份。
#[derive(Debug)]
struct FrozenQualityCandidateIdentity {
    symbol: String,
    direction: String,
    setup_ts_ms: i64,
    breakout_ts_ms: i64,
    signal_ts_ms: i64,
}

/// 用户指定样本的稳定币种、方向与信号时间身份。
#[derive(Debug, Clone, Copy)]
pub(super) struct TargetSample {
    /// 报告中的简短样本名。
    pub name: &'static str,
    /// OKX 永续交易对。
    pub symbol: &'static str,
    /// long 或 short。
    pub direction: &'static str,
    /// 信号完成时间，Unix 毫秒。
    pub signal_ts_ms: i64,
}

/// 一个候选相对当前单变量的信号时可见证据。
#[derive(Debug, Clone, Serialize)]
pub struct QualityOpportunity {
    /// 稳定候选身份。
    pub candidate_id: String,
    /// OKX 永续交易对。
    pub symbol: String,
    /// long 或 short。
    pub direction: &'static str,
    /// 长期资格完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 两收盘突破确认时间，Unix 毫秒。
    pub breakout_ts_ms: i64,
    /// 回踩确认信号时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 回踩确认信号时间，北京时间。
    pub signal_time_bjt: String,
    /// 当前唯一质量指标名。
    pub metric_name: &'static str,
    /// 当前唯一质量指标值。
    pub metric_value: f64,
    /// 当前唯一质量指标的冻结阈值。
    pub threshold: f64,
    /// 首个使当前资格或突破失效的完成 K 时间。
    pub first_failure_ts_ms: Option<i64>,
    /// 首个失效时间，北京时间。
    pub first_failure_time_bjt: Option<String>,
    /// 当前规则完成确认的时间；未完成时为空。
    pub quality_confirmed_ts_ms: Option<i64>,
    /// 当前规则是否通过。
    pub quality_gate_passed: bool,
    /// 冻结 V16 成本与结构门禁是否允许成交。
    pub source_v16_eligible: bool,
    /// 同时通过 V16 与当前唯一门禁后是否可进入 L2。
    pub eligible: bool,
    /// 当前唯一门禁拒绝时的 blocker。
    pub blocked_reason: Option<&'static str>,
}

/// L1 候选覆盖、影响比例与分散性证据。
#[derive(Debug, Clone, Serialize)]
pub struct QualityCoverage {
    /// 冻结 V16 全部候选数。
    pub baseline_candidate_count: usize,
    /// 冻结 V16 成本合格机会数。
    pub baseline_v16_eligible_count: usize,
    /// 当前特征可完整重建的候选数。
    pub evaluated_candidate_count: usize,
    /// 当前门禁拒绝的 V16 合格机会数。
    pub rejected_v16_eligible_count: usize,
    /// 当前门禁保留的 V16 合格机会数。
    pub retained_v16_eligible_count: usize,
    /// 当前门禁影响 V16 合格机会的比例。
    pub affected_ratio_pct: f64,
    /// 当前门禁保留 V16 合格机会的比例。
    pub retained_ratio_pct: f64,
    /// 保留机会按方向数量。
    pub retained_by_direction: BTreeMap<String, usize>,
    /// 保留机会覆盖币种数。
    pub retained_symbol_count: usize,
    /// 保留机会覆盖北京时间月份数。
    pub retained_month_count_bjt: usize,
    /// 保留机会的一小时方向事件数。
    pub retained_effective_market_events: usize,
    /// 当前门禁 blocker 计数。
    pub blockers: BTreeMap<String, usize>,
}

/// 当前唯一指标在 V16 合格机会中的冻结分布。
#[derive(Debug, Clone, Serialize)]
pub struct QualityDistribution {
    /// 数值数量。
    pub count: usize,
    /// 最小值。
    pub min: f64,
    /// 10% 分位。
    pub p10: f64,
    /// 25% 分位。
    pub p25: f64,
    /// 中位数。
    pub median: f64,
    /// 75% 分位。
    pub p75: f64,
    /// 90% 分位。
    pub p90: f64,
    /// 最大值。
    pub max: f64,
}

/// 用户目标样本在当前单变量下的因果判定。
#[derive(Debug, Clone, Serialize)]
pub struct QualityTargetAudit {
    /// 简短样本名。
    pub sample: &'static str,
    /// 稳定候选身份。
    pub candidate_id: String,
    /// 当前指标值。
    pub metric_value: f64,
    /// 冻结阈值。
    pub threshold: f64,
    /// 当前门禁是否通过。
    pub quality_gate_passed: bool,
    /// 首个因果失效时间，北京时间。
    pub first_failure_time_bjt: Option<String>,
    /// true 表示目标样本按预注册被拒绝。
    pub passed: bool,
}

/// 当前单变量的无 outcome 身份。
#[derive(Debug, Clone, Serialize)]
pub struct QualityL1Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// 独立候选键。
    pub candidate_key: &'static str,
    /// L1 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V16 的唯一变量。
    pub only_variable: &'static str,
    /// setup 或突破 episode 消费政策。
    pub setup_consumption_policy: &'static str,
    /// L1 可读字段边界。
    pub causal_field_boundary: &'static str,
    /// L1 禁止读取的结果字段。
    pub label_boundary: &'static str,
    /// 运行隔离边界。
    pub runtime_boundary: &'static str,
}

/// 当前单变量的 L1 停止或晋级判定。
#[derive(Debug, Clone, Serialize)]
pub struct QualityL1Decision {
    /// coverage_pass_ready_for_l2_prereg 或 stop。
    pub status: &'static str,
    /// 全部预注册门禁。
    pub gates: BTreeMap<String, bool>,
    /// L1 必须为 false。
    pub outcome_evaluation_performed: bool,
    /// 中文停止或晋级原因。
    pub reason: String,
}

/// 一个批次完整的无 outcome L1 机器证据。
#[derive(Debug, Clone, Serialize)]
pub struct QualityL1Report {
    /// L1 schema。
    pub schema_version: &'static str,
    /// 生成时间，不参与身份哈希比较。
    pub generated_at_utc: String,
    /// 单变量因果身份。
    pub identity: QualityL1Identity,
    /// 冻结 V14 报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 冻结 V16 报告 SHA-256。
    pub source_v16_report_sha256: String,
    /// 重载行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// 候选覆盖。
    pub coverage: QualityCoverage,
    /// 当前唯一指标分布。
    pub metric_distribution: QualityDistribution,
    /// 用户目标样本审计。
    pub target_audits: Vec<QualityTargetAudit>,
    /// L1 判定。
    pub decision: QualityL1Decision,
    /// 全部候选的无 outcome 证据账本。
    pub opportunities: Vec<QualityOpportunity>,
}

/// 一个独立批次的 L1 与可选单次 L2 合并报告。
#[derive(Debug, Serialize)]
pub struct QualityMachineReport {
    /// 合并报告 schema。
    pub schema_version: &'static str,
    /// 生成时间。
    pub generated_at_utc: String,
    /// 冻结 V14 报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 冻结 V16 报告 SHA-256。
    pub source_v16_report_sha256: String,
    /// L1 负载 SHA-256。
    pub l1_payload_sha256: String,
    /// 无 outcome L1。
    pub l1: QualityL1Report,
    /// 仅当 L1 通过时存在的一次冻结 L2。
    pub l2: Option<V10L2Report>,
}

#[derive(Debug, Clone, Copy)]
struct FeatureEvidence {
    metric_name: &'static str,
    metric_value: f64,
    threshold: f64,
    first_failure_ts_ms: Option<i64>,
    quality_confirmed_ts_ms: Option<i64>,
    passed: bool,
    blocker: &'static str,
}

/// 校验冻结源、重建当前单变量账本，并只在 L1 通过时执行一次 L2。
pub(super) async fn run_quality_research(
    spec: QualitySpec,
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<QualityMachineReport> {
    run_quality_research_with_l2_policy(spec, v14_source, v16_source, output, true).await
}

/// 只生成无 outcome 的 L1 报告，即使覆盖通过也等待独立 L2 预注册。
pub(super) async fn run_quality_l1_only(
    spec: QualitySpec,
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<QualityMachineReport> {
    run_quality_research_with_l2_policy(spec, v14_source, v16_source, output, false).await
}

/// 精确载入预注册 L1 的合格集合，并在零候选漂移后执行一次冻结 L2。
pub(super) async fn run_frozen_quality_l2(
    spec: FrozenQualityL2Spec,
    l1_source: &Path,
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
) -> Result<V10L2Report> {
    // 完整文件 SHA 是 outcome 解封前的第一道门禁：即使 JSON 字段看似相同，任何人工重排、
    // 补写或删改候选也必须先停止，不能在看到结果后接受“等价”的新 L1 文件。
    let l1_bytes = std::fs::read(l1_source)
        .with_context(|| format!("读取冻结入场质量 L1 失败：{}", l1_source.display()))?;
    let l1_report_sha256 = super::sha256_hex(&l1_bytes);
    if l1_report_sha256 != spec.expected_l1_report_sha256 {
        bail!("frozen quality L1 report SHA mismatch");
    }
    let root: Value = serde_json::from_slice(&l1_bytes).context("解析冻结入场质量 L1 报告失败")?;
    validate_frozen_quality_l1(&root, &spec)?;
    let frozen_candidates = frozen_quality_candidates(&root, &spec)?;
    let candidate_set_sha256 = frozen_candidate_set_sha256(frozen_candidates.keys());
    if candidate_set_sha256 != spec.expected_candidate_set_sha256 {
        bail!("frozen quality candidate-set SHA mismatch");
    }

    // V24 L2 不重新运行 V24 门禁。它只用冻结 ID 授权 V14 原始候选，并再次要求该候选仍在
    // V16 成本合格集合内；这样可以把“候选选择”和“成交后结果”保持为两个不可反向影响的阶段。
    let (v14_sha256, v14_l1) = super::load_v14_source(v14_source)?;
    let (v16_sha256, v16_eligibility) = load_v16_eligibility(v16_source)?;
    if root.get("source_v14_report_sha256").and_then(Value::as_str) != Some(v14_sha256.as_str())
        || root.get("source_v16_report_sha256").and_then(Value::as_str) != Some(v16_sha256.as_str())
    {
        bail!("frozen quality source report identity mismatch");
    }

    let replay_input = super::load_verified_v14_replay_input(&v14_l1).await?;
    if replay_input.dataset_fingerprint_sha256
        != root
            .pointer("/l1/dataset_fingerprint_sha256")
            .and_then(Value::as_str)
            .context("frozen quality dataset fingerprint missing")?
    {
        bail!("frozen quality reloaded dataset fingerprint mismatch");
    }
    let mut rebuilt = HashMap::with_capacity(replay_input.candidates.len());
    for candidate in replay_input.candidates {
        let id = candidate_id(&candidate);
        if rebuilt.insert(id, candidate).is_some() {
            bail!("duplicate rebuilt quality candidate_id");
        }
    }
    let mut selected = Vec::with_capacity(frozen_candidates.len());
    for (id, frozen) in &frozen_candidates {
        let candidate = rebuilt
            .remove(id)
            .with_context(|| format!("frozen quality candidate missing after rebuild: {id}"))?;
        if candidate.symbol != frozen.symbol
            || candidate.direction != frozen.direction
            || candidate.setup_ts_ms != frozen.setup_ts_ms
            || candidate.breakout_ts_ms != frozen.breakout_ts_ms
            || candidate.signal_ts_ms != frozen.signal_ts_ms
        {
            bail!("frozen quality candidate identity drift: {id}");
        }
        if v16_eligibility.get(id) != Some(&true) {
            bail!("frozen quality candidate lost V16 eligibility: {id}");
        }
        selected.push(candidate);
    }
    if selected.len() != spec.expected_candidate_count {
        bail!("frozen quality selected candidate count mismatch");
    }

    // 只有文件、集合、逐字段候选和行情指纹全部一致后才允许读取 forward；以下风险与退出政策
    // 沿用 V16，确保本轮唯一新增信息只是这 715 个候选的成本后 outcome。
    let report = super::replay_verified_candidate_ledger(
        &replay_input.data,
        super::ReplaySource::new(
            spec.l2_schema_version,
            V10L2Identity {
                level: "L2_local_multi_symbol_diagnostic",
                candidate_key: spec.candidate_key,
                source_l1_rule_version: spec.source_l1_rule_version,
                rule_version: spec.l2_rule_version,
                only_variable: spec.only_variable,
                entry_policy: spec.entry_policy,
                initial_stop_policy: "unchanged V16 signal EMA144 plus/minus 0.30 ATR14 structural stop, frozen at entry and never loosened",
                target_policy: "unchanged V16 cost-adjusted target that settles to net 2.00R after 8bps per side",
                intrabar_conflict_policy: "unchanged V16 entry-candle inclusion and stop-first ordering when stop and target are both touched",
                symbol_position_policy: "unchanged one open trade per symbol and first qualifying real fill per symbol x direction x setup",
                per_side_cost_rate: PER_SIDE_COST_RATE,
                max_holding_ms: MAX_HOLDING_MS,
                funding_modeled: false,
                outcome_evaluation_performed: true,
                runtime_boundary: "research-only frozen V24 L2; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
            },
            l1_report_sha256,
            replay_input.dataset_fingerprint_sha256,
            replay_input.returned_symbol_count,
            replay_input.eligible_symbol_count,
            replay_input.excluded_symbol_count,
            SetupEntryPolicy::FirstFilledPerSetup,
            InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR),
            TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R),
            EntryRiskGatePolicy::MaxStopCostR(MAX_STOP_COST_R),
            selected,
        ),
    );
    write_frozen_l2_report(output, &report)?;
    Ok(report)
}

/// 校验 L1 文件身份、覆盖、无 outcome 边界和未执行 L2 状态。
fn validate_frozen_quality_l1(root: &Value, spec: &FrozenQualityL2Spec) -> Result<()> {
    if root.get("schema_version").and_then(Value::as_str)
        != Some(spec.source_machine_schema_version)
        || root.get("l1_payload_sha256").and_then(Value::as_str)
            != Some(spec.expected_l1_payload_sha256)
        || !root.get("l2").is_some_and(Value::is_null)
    {
        bail!("frozen quality L1 machine identity mismatch");
    }
    let l1 = root
        .get("l1")
        .context("frozen quality L1 payload missing")?;
    if l1.get("schema_version").and_then(Value::as_str) != Some(spec.source_l1_schema_version)
        || l1
            .pointer("/identity/candidate_key")
            .and_then(Value::as_str)
            != Some(spec.candidate_key)
        || l1.pointer("/identity/rule_version").and_then(Value::as_str)
            != Some(spec.source_l1_rule_version)
        || l1.pointer("/decision/status").and_then(Value::as_str)
            != Some("coverage_pass_ready_for_l2_prereg")
        || l1
            .pointer("/decision/outcome_evaluation_performed")
            .and_then(Value::as_bool)
            != Some(false)
        || l1
            .pointer("/coverage/retained_v16_eligible_count")
            .and_then(Value::as_u64)
            != Some(spec.expected_candidate_count as u64)
        || l1
            .pointer("/coverage/retained_by_direction/long")
            .and_then(Value::as_u64)
            != Some(spec.expected_long_count as u64)
        || l1
            .pointer("/coverage/retained_by_direction/short")
            .and_then(Value::as_u64)
            != Some(spec.expected_short_count as u64)
    {
        bail!("frozen quality L1 payload contract mismatch");
    }
    Ok(())
}

/// 提取且去重 L1 中同时通过 V16 与 V24 的 715 条稳定候选身份。
fn frozen_quality_candidates(
    root: &Value,
    spec: &FrozenQualityL2Spec,
) -> Result<BTreeMap<String, FrozenQualityCandidateIdentity>> {
    let rows = root
        .pointer("/l1/opportunities")
        .and_then(Value::as_array)
        .context("frozen quality opportunities missing")?;
    let mut selected = BTreeMap::new();
    let mut long_count = 0usize;
    let mut short_count = 0usize;
    for row in rows
        .iter()
        .filter(|row| row.get("eligible").and_then(Value::as_bool) == Some(true))
    {
        if row.get("source_v16_eligible").and_then(Value::as_bool) != Some(true)
            || row.get("quality_gate_passed").and_then(Value::as_bool) != Some(true)
        {
            bail!("frozen quality eligible row has inconsistent gates");
        }
        let candidate_id = row
            .get("candidate_id")
            .and_then(Value::as_str)
            .context("frozen quality candidate_id missing")?
            .to_owned();
        let symbol = row
            .get("symbol")
            .and_then(Value::as_str)
            .context("frozen quality symbol missing")?
            .to_owned();
        let direction = row
            .get("direction")
            .and_then(Value::as_str)
            .context("frozen quality direction missing")?
            .to_owned();
        let signal_ts_ms = frozen_i64(row, "signal_ts_ms")?;
        if candidate_id != format!("{symbol}:{signal_ts_ms}:{direction}") {
            bail!("frozen quality candidate_id fields mismatch");
        }
        match direction.as_str() {
            "long" => long_count += 1,
            "short" => short_count += 1,
            _ => bail!("unsupported frozen quality direction: {direction}"),
        }
        let identity = FrozenQualityCandidateIdentity {
            symbol,
            direction,
            setup_ts_ms: frozen_i64(row, "setup_ts_ms")?,
            breakout_ts_ms: frozen_i64(row, "breakout_ts_ms")?,
            signal_ts_ms,
        };
        if selected.insert(candidate_id, identity).is_some() {
            bail!("duplicate frozen quality candidate_id");
        }
    }
    if selected.len() != spec.expected_candidate_count
        || long_count != spec.expected_long_count
        || short_count != spec.expected_short_count
    {
        bail!("frozen quality candidate direction counts mismatch");
    }
    Ok(selected)
}

/// 读取候选身份中的毫秒时间，拒绝缺失或非整数值。
fn frozen_i64(row: &Value, field: &str) -> Result<i64> {
    row.get(field)
        .and_then(Value::as_i64)
        .with_context(|| format!("frozen quality {field} missing"))
}

/// 对 ASCII 排序候选 ID 逐行编码，生成与预注册一致的集合摘要。
fn frozen_candidate_set_sha256<'a>(ids: impl IntoIterator<Item = &'a String>) -> String {
    let mut bytes = Vec::new();
    for id in ids {
        bytes.extend_from_slice(id.as_bytes());
        bytes.push(b'\n');
    }
    super::sha256_hex(&bytes)
}

/// 只写独立 L2 成本回放，不回填或改写冻结 L1 文件。
fn write_frozen_l2_report(output: &Path, report: &V10L2Report) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建冻结 L2 报告目录失败：{}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(report)?;
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入冻结 L2 报告失败：{}", output.display()))
}

/// 按调用方冻结的最高研究等级执行共用质量扫描。
async fn run_quality_research_with_l2_policy(
    spec: QualitySpec,
    v14_source: &Path,
    v16_source: &Path,
    output: &Path,
    allow_l2: bool,
) -> Result<QualityMachineReport> {
    let (v14_sha256, v14_l1) = super::load_v14_source(v14_source)?;
    let (v16_sha256, v16_eligibility) = load_v16_eligibility(v16_source)?;
    let replay_input = super::load_verified_v14_replay_input(&v14_l1).await?;
    if replay_input.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("entry quality reloaded dataset fingerprint mismatch");
    }
    let ema576_by_symbol = build_ema576_cache(&replay_input.data);
    let mut opportunities = Vec::with_capacity(replay_input.candidates.len());
    let mut passing_candidate_ids = BTreeSet::new();
    for candidate in &replay_input.candidates {
        let candidate_id = candidate_id(candidate);
        let source_v16_eligible = *v16_eligibility
            .get(&candidate_id)
            .with_context(|| format!("V16 candidate missing: {candidate_id}"))?;
        let evidence =
            feature_evidence(&replay_input.data, &ema576_by_symbol, candidate, spec.rule)?;
        if evidence.passed {
            passing_candidate_ids.insert(candidate_id.clone());
        }
        opportunities.push(QualityOpportunity {
            candidate_id,
            symbol: candidate.symbol.clone(),
            direction: candidate.direction,
            setup_ts_ms: candidate.setup_ts_ms,
            breakout_ts_ms: candidate.breakout_ts_ms,
            signal_ts_ms: candidate.signal_ts_ms,
            signal_time_bjt: super::format_bjt(candidate.signal_ts_ms)?,
            metric_name: evidence.metric_name,
            metric_value: evidence.metric_value,
            threshold: evidence.threshold,
            first_failure_ts_ms: evidence.first_failure_ts_ms,
            first_failure_time_bjt: evidence
                .first_failure_ts_ms
                .map(super::format_bjt)
                .transpose()?,
            quality_confirmed_ts_ms: evidence.quality_confirmed_ts_ms,
            quality_gate_passed: evidence.passed,
            source_v16_eligible,
            eligible: source_v16_eligible && evidence.passed,
            blocked_reason: (!evidence.passed).then_some(evidence.blocker),
        });
    }
    if opportunities.len() != EXPECTED_V16_CANDIDATES
        || v16_eligibility.len() != EXPECTED_V16_CANDIDATES
    {
        bail!("entry quality baseline candidate count mismatch");
    }
    let l1 = build_l1_report(
        &spec,
        v14_sha256.clone(),
        v16_sha256.clone(),
        replay_input.dataset_fingerprint_sha256.clone(),
        opportunities,
    )?;
    let l1_payload_sha256 = super::sha256_hex(&serde_json::to_vec(&l1)?);
    let l2 = if allow_l2 && l1.decision.status == "coverage_pass_ready_for_l2_prereg" {
        let filtered = replay_input
            .candidates
            .iter()
            .filter(|candidate| passing_candidate_ids.contains(&candidate_id(candidate)))
            .cloned()
            .collect::<Vec<_>>();
        Some(super::replay_verified_candidate_ledger(
            &replay_input.data,
            super::ReplaySource::new(
                spec.l2_schema_version,
                V10L2Identity {
                    level: "L2_local_multi_symbol_diagnostic",
                    candidate_key: spec.candidate_key,
                    source_l1_rule_version: spec.l1_rule_version,
                    rule_version: spec.l2_rule_version,
                    only_variable: spec.only_variable,
                    entry_policy: spec.entry_policy,
                    initial_stop_policy: "unchanged V16 signal EMA144 plus/minus 0.30 ATR14 structural stop, frozen at entry and never loosened",
                    target_policy: "unchanged V16 cost-adjusted target that settles to net 2.00R after 8bps per side",
                    intrabar_conflict_policy: "unchanged V16 entry-candle inclusion and stop-first ordering when stop and target are both touched",
                    symbol_position_policy: "unchanged one open trade per symbol and first qualifying real fill per symbol x direction x setup",
                    per_side_cost_rate: PER_SIDE_COST_RATE,
                    max_holding_ms: MAX_HOLDING_MS,
                    funding_modeled: false,
                    outcome_evaluation_performed: true,
                    runtime_boundary: "research-only entry-quality L2; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
                },
                l1_payload_sha256.clone(),
                replay_input.dataset_fingerprint_sha256,
                replay_input.returned_symbol_count,
                replay_input.eligible_symbol_count,
                replay_input.excluded_symbol_count,
                SetupEntryPolicy::FirstFilledPerSetup,
                InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR),
                TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R),
                EntryRiskGatePolicy::MaxStopCostR(MAX_STOP_COST_R),
                filtered,
            ),
        ))
    } else {
        None
    };
    let report = QualityMachineReport {
        schema_version: spec.machine_schema_version,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        source_v14_report_sha256: v14_sha256,
        source_v16_report_sha256: v16_sha256,
        l1_payload_sha256,
        l1,
        l2,
    };
    write_report(output, &report)?;
    Ok(report)
}

/// 只解析冻结 V16 的 L1 机会，不读取其 L2 outcome。
fn load_v16_eligibility(source: &Path) -> Result<(String, HashMap<String, bool>)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取冻结 V16 报告失败：{}", source.display()))?;
    let sha256 = super::sha256_hex(&bytes);
    if sha256 != EXPECTED_V16_REPORT_SHA256 {
        bail!("entry quality source V16 report SHA mismatch");
    }
    let root: Value = serde_json::from_slice(&bytes).context("解析冻结 V16 报告失败")?;
    if root.get("schema_version").and_then(Value::as_str)
        != Some("market_momentum_ema576_entry_stop_cost_cap_050r_l1_l2_v16")
        || root.get("source_v14_report_sha256").and_then(Value::as_str)
            != Some(EXPECTED_V14_REPORT_SHA256)
    {
        bail!("entry quality source V16 identity mismatch");
    }
    let l1 = root
        .get("l1")
        .context("entry quality source V16 L1 missing")?;
    if l1
        .pointer("/identity/candidate_key")
        .and_then(Value::as_str)
        != Some(V16_CANDIDATE_KEY)
        || l1.pointer("/identity/rule_version").and_then(Value::as_str) != Some(V16_L1_RULE_VERSION)
        || l1.pointer("/decision/status").and_then(Value::as_str)
            != Some("coverage_pass_ready_for_l2_prereg")
        || l1
            .pointer("/coverage/baseline_candidate_count")
            .and_then(Value::as_u64)
            != Some(EXPECTED_V16_CANDIDATES as u64)
        || l1
            .pointer("/coverage/eligible_opportunity_count")
            .and_then(Value::as_u64)
            != Some(EXPECTED_V16_ELIGIBLE as u64)
    {
        bail!("entry quality source V16 L1 contract mismatch");
    }
    let rows = l1
        .get("opportunities")
        .and_then(Value::as_array)
        .context("entry quality source V16 opportunities missing")?;
    let mut eligibility = HashMap::with_capacity(rows.len());
    for row in rows {
        let candidate_id = row
            .get("candidate_id")
            .and_then(Value::as_str)
            .context("V16 opportunity candidate_id missing")?
            .to_owned();
        let eligible = row
            .get("eligible")
            .and_then(Value::as_bool)
            .context("V16 opportunity eligible missing")?;
        if eligibility.insert(candidate_id, eligible).is_some() {
            bail!("duplicate V16 opportunity candidate_id");
        }
    }
    Ok((sha256, eligibility))
}

/// 为每个币种一次性重建与冻结研究完全相同的 EMA576 序列。
fn build_ema576_cache(data: &BacktestDataSet) -> HashMap<String, Vec<Option<f64>>> {
    data.candles_15m_computed
        .iter()
        .map(|(symbol, candles)| (symbol.clone(), ema_close_series(candles, EMA576_PERIOD)))
        .collect()
}

/// 根据当前唯一规则提取候选在信号前已经可见的质量证据。
fn feature_evidence(
    data: &BacktestDataSet,
    ema576_by_symbol: &HashMap<String, Vec<Option<f64>>>,
    candidate: &V2Candidate,
    rule: QualityRule,
) -> Result<FeatureEvidence> {
    let candles = data
        .candles_15m_computed
        .get(&candidate.symbol)
        .with_context(|| format!("computed candles missing: {}", candidate.symbol))?;
    let ema576 = ema576_by_symbol
        .get(&candidate.symbol)
        .with_context(|| format!("EMA576 cache missing: {}", candidate.symbol))?;
    let setup_idx = candle_index(candles, candidate.setup_ts_ms)?;
    let breakout_idx = candle_index(candles, candidate.breakout_ts_ms)?;
    let signal_idx = candle_index(candles, candidate.signal_ts_ms)?;
    match rule {
        QualityRule::QualificationCycleFresh => qualification_cycle_evidence(
            candles,
            ema576,
            candidate.direction,
            setup_idx,
            breakout_idx,
        ),
        QualityRule::BreakoutDistance2_5Atr => breakout_distance_evidence(
            candles,
            ema576,
            candidate.direction,
            breakout_idx,
            BREAKOUT_DISTANCE_ATR,
            "breakout_confirmation_distance_below_2_5atr",
        ),
        QualityRule::BreakoutAcceptance8Bars => acceptance_evidence(
            candles,
            ema576,
            candidate.direction,
            breakout_idx,
            signal_idx,
            AcceptanceBoundary::Ema144RetestZone,
        ),
        QualityRule::CompositeCycleDistanceAcceptance => composite_evidence(
            candles,
            ema576,
            candidate.direction,
            setup_idx,
            breakout_idx,
            signal_idx,
            BREAKOUT_DISTANCE_ATR,
            AcceptanceBoundary::Ema144RetestZone,
        ),
        QualityRule::CompositeCycleDistance2_0Acceptance
        | QualityRule::CompositeCycleDistance2_0AcceptanceEma576Hold => {
            let boundary = match rule {
                QualityRule::CompositeCycleDistance2_0Acceptance => {
                    AcceptanceBoundary::Ema144RetestZone
                }
                _ => AcceptanceBoundary::Ema576IntrabarHold,
            };
            composite_evidence(
                candles,
                ema576,
                candidate.direction,
                setup_idx,
                breakout_idx,
                signal_idx,
                RELAXED_BREAKOUT_DISTANCE_ATR,
                boundary,
            )
        }
        QualityRule::CompositeCycleDistance2_0AcceptanceEma576HoldRelationUntilSignal => {
            composite_relation_intact_until_signal_evidence(
                candles,
                ema576,
                candidate.direction,
                setup_idx,
                breakout_idx,
                signal_idx,
            )
        }
        QualityRule::CompositeCycleAcceptanceWindowExtreme2_0Ema576HoldRelationUntilSignal => {
            composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
                candles,
                ema576,
                candidate.direction,
                setup_idx,
                breakout_idx,
                signal_idx,
            )
        }
        QualityRule::CompositeCycleAcceptanceWindowExtreme2_0SixCloseEma576HoldRelationUntilSignal => {
            composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
                candles,
                ema576,
                candidate.direction,
                setup_idx,
                breakout_idx,
                signal_idx,
            )
        }
    }
}
/// V24 只替换 2ATR 的取值位置，其余 V23 组合门禁保持不变。
fn composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    breakout_idx: usize,
    signal_idx: usize,
) -> Result<FeatureEvidence> {
    composite_acceptance_window_extreme_relation_intact_until_signal_evidence_with_closes(
        candles,
        ema576,
        direction,
        setup_idx,
        breakout_idx,
        signal_idx,
        ACCEPTANCE_CLOSES,
        "qualification_relation_or_acceptance_window_extreme_composite_gate_failed",
    )
}
/// V25 只缩短接受窗口；2ATR、盘中保持与关系周期均沿用 V24。
fn composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    breakout_idx: usize,
    signal_idx: usize,
) -> Result<FeatureEvidence> {
    composite_acceptance_window_extreme_relation_intact_until_signal_evidence_with_closes(
        candles,
        ema576,
        direction,
        setup_idx,
        breakout_idx,
        signal_idx,
        SIX_CLOSE_ACCEPTANCE_CLOSES,
        "qualification_relation_or_six_close_acceptance_window_extreme_composite_gate_failed",
    )
}
/// 按版本冻结的根数共同计算关系周期、窗口极值与 EMA576 接受证据。
fn composite_acceptance_window_extreme_relation_intact_until_signal_evidence_with_closes(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    breakout_idx: usize,
    signal_idx: usize,
    required_closes: usize,
    blocker: &'static str,
) -> Result<FeatureEvidence> {
    let cycle = qualification_cycle_evidence(candles, ema576, direction, setup_idx, signal_idx)?;
    let distance = acceptance_window_extreme_distance_evidence_with_closes(
        candles,
        ema576,
        direction,
        breakout_idx,
        signal_idx,
        RELAXED_BREAKOUT_DISTANCE_ATR,
        required_closes,
    )?;
    let acceptance = acceptance_evidence_with_closes(
        candles,
        ema576,
        direction,
        breakout_idx,
        signal_idx,
        AcceptanceBoundary::Ema576IntrabarHold,
        required_closes,
    )?;
    let components = [cycle, distance, acceptance];
    let passed = components.iter().all(|item| item.passed);
    let first_failure_ts_ms = components
        .iter()
        .filter_map(|item| item.first_failure_ts_ms)
        .min();
    let quality_confirmed_ts_ms = passed
        .then(|| {
            components
                .iter()
                .filter_map(|item| item.quality_confirmed_ts_ms)
                .max()
        })
        .flatten();
    // 报告保留窗口极值本身而非组合位图，确保 L1 分布只描述本轮唯一变化；
    // EMA 关系或版本冻结的连续收盘失败仍由 quality_gate_passed 与 first_failure 单独表达。
    Ok(FeatureEvidence {
        metric_name: "acceptance_window_max_directional_extreme_distance_atr",
        metric_value: distance.metric_value,
        threshold: distance.threshold,
        first_failure_ts_ms,
        quality_confirmed_ts_ms,
        passed,
        blocker,
    })
}
/// V23 在 V22 组合门禁之外，把原始均线关系的有效期延长到信号完成。
fn composite_relation_intact_until_signal_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    breakout_idx: usize,
    signal_idx: usize,
) -> Result<FeatureEvidence> {
    composite_evidence_with_relation_end(
        candles,
        ema576,
        direction,
        setup_idx,
        breakout_idx,
        signal_idx,
        signal_idx,
        RELAXED_BREAKOUT_DISTANCE_ATR,
        AcceptanceBoundary::Ema576IntrabarHold,
        "composite_quality_bitmask_cycle_until_signal1_distance2_acceptance4",
        "qualification_relation_cycle_broken_before_signal_or_composite_gate_failed",
    )
}
/// 合并三个已经冻结的因果门禁，只报告整体合同，不再声称单变量归因。
fn composite_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    breakout_idx: usize,
    signal_idx: usize,
    distance_threshold_atr: f64,
    acceptance_boundary: AcceptanceBoundary,
) -> Result<FeatureEvidence> {
    composite_evidence_with_relation_end(
        candles,
        ema576,
        direction,
        setup_idx,
        breakout_idx,
        signal_idx,
        breakout_idx,
        distance_threshold_atr,
        acceptance_boundary,
        "composite_quality_bitmask_cycle1_distance2_acceptance4",
        "composite_cycle_distance_acceptance_gate_failed",
    )
}
/// 在指定关系周期终点上合并资格、突破距离与八根接受门禁。
fn composite_evidence_with_relation_end(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    breakout_idx: usize,
    signal_idx: usize,
    relation_end_idx: usize,
    distance_threshold_atr: f64,
    acceptance_boundary: AcceptanceBoundary,
    metric_name: &'static str,
    blocker: &'static str,
) -> Result<FeatureEvidence> {
    let cycle =
        qualification_cycle_evidence(candles, ema576, direction, setup_idx, relation_end_idx)?;
    let distance = breakout_distance_evidence(
        candles,
        ema576,
        direction,
        breakout_idx,
        distance_threshold_atr,
        "composite_breakout_confirmation_distance_below_threshold",
    )?;
    let acceptance = acceptance_evidence(
        candles,
        ema576,
        direction,
        breakout_idx,
        signal_idx,
        acceptance_boundary,
    )?;
    let components = [cycle, distance, acceptance];
    // 位 0/1/2 分别记录资格周期、当前距离阈值与八收盘，保留组合筛选的逐项因果证据。
    let component_bitmask = u8::from(components[0].passed)
        | (u8::from(components[1].passed) << 1)
        | (u8::from(components[2].passed) << 2);
    let first_failure_ts_ms = components
        .iter()
        .filter_map(|item| item.first_failure_ts_ms)
        .min();
    let quality_confirmed_ts_ms = (component_bitmask == 0b111)
        .then(|| {
            components
                .iter()
                .filter_map(|item| item.quality_confirmed_ts_ms)
                .max()
        })
        .flatten();
    Ok(FeatureEvidence {
        metric_name,
        metric_value: component_bitmask as f64,
        threshold: 7.0,
        first_failure_ts_ms,
        quality_confirmed_ts_ms,
        passed: component_bitmask == 0b111,
        blocker,
    })
}
/// 旧资格只要离开一次原始 EMA 关系周期就立即失效。
fn qualification_cycle_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    setup_idx: usize,
    relation_end_idx: usize,
) -> Result<FeatureEvidence> {
    if setup_idx > relation_end_idx {
        bail!("qualification setup occurs after relation-cycle end");
    }
    let mut first_failure = None;
    for idx in setup_idx..=relation_end_idx {
        let ema144 = candles[idx]
            .ema144
            .filter(|value| value.is_finite() && *value > 0.0)
            .context("qualification EMA144 missing")?;
        let ema576 = ema576[idx]
            .filter(|value| value.is_finite() && *value > 0.0)
            .context("qualification EMA576 missing")?;
        if !qualification_relation_holds(direction, ema144, ema576)? {
            first_failure = Some(candles[idx].candle.ts);
            break;
        }
    }
    Ok(FeatureEvidence {
        metric_name: "qualification_cycle_intact",
        metric_value: if first_failure.is_none() { 1.0 } else { 0.0 },
        threshold: 1.0,
        first_failure_ts_ms: first_failure,
        quality_confirmed_ts_ms: first_failure
            .is_none()
            .then_some(relation_end_idx)
            .map(|idx| candles[idx].candle.ts),
        passed: first_failure.is_none(),
        blocker: "qualification_relation_cycle_broken_before_breakout",
    })
}
/// 使用突破确认收盘、同棒 EMA576 与 Wilder ATR14 计算方向性距离。
fn breakout_distance_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    breakout_idx: usize,
    threshold_atr: f64,
    blocker: &'static str,
) -> Result<FeatureEvidence> {
    let candle = &candles[breakout_idx];
    let slow = ema576[breakout_idx]
        .filter(|value| value.is_finite() && *value > 0.0)
        .context("breakout EMA576 missing")?;
    let atr = candle
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .context("breakout ATR14 missing")?;
    let distance = directional_distance(direction, candle.candle.close, slow)? / atr;
    Ok(FeatureEvidence {
        metric_name: "breakout_confirmation_close_distance_atr",
        metric_value: distance,
        threshold: threshold_atr,
        first_failure_ts_ms: (distance < threshold_atr).then_some(candle.candle.ts),
        quality_confirmed_ts_ms: (distance >= threshold_atr).then_some(candle.candle.ts),
        passed: distance >= threshold_atr,
        blocker,
    })
}
/// 在版本冻结的接受根数内计算顺势最大极值，禁止后续 K 线回填。
fn acceptance_window_extreme_distance_evidence_with_closes(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    breakout_idx: usize,
    signal_idx: usize,
    threshold_atr: f64,
    required_closes: usize,
) -> Result<FeatureEvidence> {
    if required_closes < 2 {
        bail!("acceptance closes must include both breakout confirmation closes");
    }
    let first_cross_idx = breakout_idx
        .checked_sub(1)
        .context("breakout first close missing")?;
    let acceptance_end_idx = first_cross_idx
        .checked_add(required_closes - 1)
        .context("acceptance end overflow")?;
    let last_visible_idx = signal_idx.saturating_sub(1);
    let visible_end_idx = acceptance_end_idx.min(last_visible_idx);
    let mut max_distance = f64::NEG_INFINITY;
    for idx in first_cross_idx..=visible_end_idx {
        let slow = ema576[idx]
            .filter(|value| value.is_finite() && *value > 0.0)
            .context("acceptance-window EMA576 missing")?;
        let atr = candles[idx]
            .atr14
            .filter(|value| value.is_finite() && *value > 0.0)
            .context("acceptance-window ATR14 missing")?;
        let directional_extreme = match direction {
            "long" => candles[idx].candle.high - slow,
            "short" => slow - candles[idx].candle.low,
            _ => bail!("unsupported candidate direction: {direction}"),
        };
        max_distance = max_distance.max(directional_extreme / atr);
    }
    if !max_distance.is_finite() {
        bail!("acceptance-window extreme distance is unavailable");
    }
    // 极值提前达标也必须等版本冻结的最后一根收盘；visible_end_idx 同步封顶，
    // 避免窗口结束后的走势回填旧突破。
    let window_complete = signal_idx > acceptance_end_idx;
    let passed = window_complete && max_distance >= threshold_atr;
    let decision_ts_ms = if window_complete {
        candles[acceptance_end_idx].candle.ts
    } else {
        candles[signal_idx].candle.ts
    };
    Ok(FeatureEvidence {
        metric_name: "acceptance_window_max_directional_extreme_distance_atr",
        metric_value: max_distance,
        threshold: threshold_atr,
        first_failure_ts_ms: (!passed).then_some(decision_ts_ms),
        quality_confirmed_ts_ms: passed.then_some(candles[acceptance_end_idx].candle.ts),
        passed,
        blocker: "acceptance_window_extreme_distance_below_2_0atr",
    })
}
/// 从第一根越线收盘开始累计八根，并按冻结边界拒绝过早回踩。
fn acceptance_evidence(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    breakout_idx: usize,
    signal_idx: usize,
    boundary: AcceptanceBoundary,
) -> Result<FeatureEvidence> {
    acceptance_evidence_with_closes(
        candles,
        ema576,
        direction,
        breakout_idx,
        signal_idx,
        boundary,
        ACCEPTANCE_CLOSES,
    )
}
/// 按版本冻结的根数累计同侧收盘，并在确认完成前执行盘中结构否决。
fn acceptance_evidence_with_closes(
    candles: &[ComputedCandle],
    ema576: &[Option<f64>],
    direction: &str,
    breakout_idx: usize,
    signal_idx: usize,
    boundary: AcceptanceBoundary,
    required_closes: usize,
) -> Result<FeatureEvidence> {
    if required_closes < 2 {
        bail!("acceptance closes must include both breakout confirmation closes");
    }
    let first_cross_idx = breakout_idx
        .checked_sub(1)
        .context("breakout first close missing")?;
    let acceptance_end_idx = first_cross_idx
        .checked_add(required_closes - 1)
        .context("acceptance end overflow")?;
    let last_visible_idx = signal_idx.saturating_sub(1);
    let mut consecutive = 0usize;
    let mut first_failure = None;
    for idx in first_cross_idx..=acceptance_end_idx.min(last_visible_idx) {
        let slow = ema576[idx]
            .filter(|value| value.is_finite() && *value > 0.0)
            .context("acceptance EMA576 missing")?;
        if !price_on_breakout_side(direction, candles[idx].candle.close, slow)? {
            first_failure = Some(candles[idx].candle.ts);
            break;
        }
        consecutive += 1;
    }
    let early_retest_end = acceptance_end_idx.min(signal_idx);
    if first_failure.is_none() && breakout_idx < early_retest_end {
        for idx in breakout_idx + 1..=early_retest_end {
            let breached = match boundary {
                AcceptanceBoundary::Ema144RetestZone => {
                    retest_zone_reached(candles, direction, idx)?
                }
                AcceptanceBoundary::Ema576IntrabarHold => {
                    let slow = ema576[idx]
                        .filter(|value| value.is_finite() && *value > 0.0)
                        .context("acceptance EMA576 missing")?;
                    match direction {
                        "long" => candles[idx].candle.low < slow,
                        "short" => candles[idx].candle.high > slow,
                        _ => bail!("unsupported candidate direction: {direction}"),
                    }
                }
            };
            if breached {
                first_failure = Some(candles[idx].candle.ts);
                break;
            }
        }
    }
    let passed = consecutive >= required_closes
        && signal_idx > acceptance_end_idx
        && first_failure.is_none();
    let blocker = match required_closes {
        ACCEPTANCE_CLOSES => "breakout_acceptance_below_8_completed_closes",
        SIX_CLOSE_ACCEPTANCE_CLOSES => "breakout_acceptance_below_6_completed_closes",
        _ => "breakout_acceptance_below_required_completed_closes",
    };
    Ok(FeatureEvidence {
        metric_name: "consecutive_breakout_side_closes_before_retest",
        metric_value: consecutive as f64,
        threshold: required_closes as f64,
        first_failure_ts_ms: first_failure
            .or_else(|| (!passed).then_some(candles[signal_idx].candle.ts)),
        quality_confirmed_ts_ms: passed.then_some(candles[acceptance_end_idx].candle.ts),
        passed,
        blocker,
    })
}
/// 检查一根完成 K 是否已经触及冻结的 EMA144±0.30ATR 回踩区。
fn retest_zone_reached(candles: &[ComputedCandle], direction: &str, idx: usize) -> Result<bool> {
    let candle = &candles[idx];
    let ema144 = candle
        .ema144
        .filter(|value| value.is_finite() && *value > 0.0)
        .context("acceptance EMA144 missing")?;
    let atr = candle
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .context("acceptance ATR14 missing")?;
    match direction {
        "long" => Ok(candle.candle.low <= ema144 + RETEST_ZONE_ATR * atr),
        "short" => Ok(candle.candle.high >= ema144 - RETEST_ZONE_ATR * atr),
        _ => bail!("unsupported candidate direction: {direction}"),
    }
}

/// 构造 L1 覆盖、目标样本审计与停止门禁。
fn build_l1_report(
    spec: &QualitySpec,
    v14_sha256: String,
    v16_sha256: String,
    dataset_fingerprint_sha256: String,
    opportunities: Vec<QualityOpportunity>,
) -> Result<QualityL1Report> {
    let baseline = opportunities
        .iter()
        .filter(|item| item.source_v16_eligible)
        .collect::<Vec<_>>();
    let retained = baseline
        .iter()
        .copied()
        .filter(|item| item.quality_gate_passed)
        .collect::<Vec<_>>();
    let rejected = baseline.len().saturating_sub(retained.len());
    let mut by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    let mut blockers = BTreeMap::new();
    for item in &retained {
        *by_direction.entry(item.direction.to_owned()).or_default() += 1;
        symbols.insert(item.symbol.clone());
        months.insert(super::month_bjt(item.signal_ts_ms)?);
    }
    for item in baseline.iter().filter(|item| !item.quality_gate_passed) {
        let blocker = item.blocked_reason.context("quality blocker missing")?;
        *blockers.entry(blocker.to_owned()).or_default() += 1;
    }
    let coverage = QualityCoverage {
        baseline_candidate_count: opportunities.len(),
        baseline_v16_eligible_count: baseline.len(),
        evaluated_candidate_count: opportunities.len(),
        rejected_v16_eligible_count: rejected,
        retained_v16_eligible_count: retained.len(),
        affected_ratio_pct: super::ratio_pct(rejected, baseline.len()),
        retained_ratio_pct: super::ratio_pct(retained.len(), baseline.len()),
        retained_by_direction: by_direction,
        retained_symbol_count: symbols.len(),
        retained_month_count_bjt: months.len(),
        retained_effective_market_events: effective_event_count(&retained),
        blockers,
    };
    let target_audits = spec
        .target_samples
        .iter()
        .map(|target| target_audit(&opportunities, *target))
        .collect::<Result<Vec<_>>>()?;
    let mut gates = BTreeMap::new();
    gates.insert(
        "baseline_v16_identity_matches".to_owned(),
        coverage.baseline_candidate_count == EXPECTED_V16_CANDIDATES
            && coverage.baseline_v16_eligible_count == EXPECTED_V16_ELIGIBLE,
    );
    gates.insert(
        "all_candidates_have_causal_metric".to_owned(),
        coverage.evaluated_candidate_count == coverage.baseline_candidate_count,
    );
    gates.insert(
        "affected_ratio_within_preregistered_range".to_owned(),
        coverage.affected_ratio_pct >= spec.min_affected_ratio_pct
            && coverage.affected_ratio_pct <= spec.max_affected_ratio_pct,
    );
    gates.insert(
        "both_directions_retain_at_least_100".to_owned(),
        ["long", "short"].iter().all(|direction| {
            coverage
                .retained_by_direction
                .get(*direction)
                .copied()
                .unwrap_or_default()
                >= MIN_DIRECTION_RETAINED
        }),
    );
    gates.insert(
        "cross_symbol_month_event_coverage_preserved".to_owned(),
        coverage.retained_symbol_count >= MIN_SYMBOLS
            && coverage.retained_month_count_bjt >= MIN_MONTHS_BJT
            && coverage.retained_effective_market_events >= MIN_EVENTS,
    );
    gates.insert(
        "target_samples_rejected".to_owned(),
        target_audits.iter().all(|audit| audit.passed),
    );
    gates.insert("forbidden_outcome_fields_absent".to_owned(), true);
    let passed = gates.values().all(|gate| *gate);
    Ok(QualityL1Report {
        schema_version: spec.l1_schema_version,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: QualityL1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: spec.candidate_key,
            rule_version: spec.l1_rule_version,
            only_variable: spec.only_variable,
            setup_consumption_policy: spec.setup_consumption_policy,
            causal_field_boundary: spec.causal_field_boundary,
            label_boundary: "no exit timestamp, exit price, exit reason, complete flag, MFE, MAE, gross R, cost R, net R, PnL, win, or loss is read",
            runtime_boundary: "research-only L1; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
        },
        source_v14_report_sha256: v14_sha256,
        source_v16_report_sha256: v16_sha256,
        dataset_fingerprint_sha256,
        coverage,
        metric_distribution: distribution(
            baseline.iter().map(|item| item.metric_value).collect(),
        )?,
        target_audits,
        decision: QualityL1Decision {
            status: if passed {
                "coverage_pass_ready_for_l2_prereg"
            } else {
                "stop"
            },
            gates,
            outcome_evaluation_performed: false,
            reason: if passed {
                "目标样本、预注册影响比例与跨方向/币种/月/事件覆盖全部通过；允许执行一次冻结 L2。".to_owned()
            } else {
                "至少一项无 outcome 的目标、影响比例或分散覆盖门禁失败；停止在 L1，不运行 L2。".to_owned()
            },
        },
        opportunities,
    })
}

/// 提取一个必须被当前门禁拒绝的用户样本。
fn target_audit(
    opportunities: &[QualityOpportunity],
    target: TargetSample,
) -> Result<QualityTargetAudit> {
    let item = opportunities
        .iter()
        .find(|item| {
            item.symbol == target.symbol
                && item.direction == target.direction
                && item.signal_ts_ms == target.signal_ts_ms
        })
        .with_context(|| format!("quality target missing: {}", target.name))?;
    Ok(QualityTargetAudit {
        sample: target.name,
        candidate_id: item.candidate_id.clone(),
        metric_value: item.metric_value,
        threshold: item.threshold,
        quality_gate_passed: item.quality_gate_passed,
        first_failure_time_bjt: item.first_failure_time_bjt.clone(),
        passed: !item.quality_gate_passed,
    })
}

/// 按方向与一小时连续触发窗口归并保留机会。
fn effective_event_count(opportunities: &[&QualityOpportunity]) -> usize {
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

/// 计算有限数值分布；L1 阈值在调用前已经冻结。
fn distribution(mut values: Vec<f64>) -> Result<QualityDistribution> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        bail!("quality metric distribution is empty or non-finite");
    }
    values.sort_by(f64::total_cmp);
    Ok(QualityDistribution {
        count: values.len(),
        min: values[0],
        p10: percentile(&values, 0.10),
        p25: percentile(&values, 0.25),
        median: percentile(&values, 0.50),
        p75: percentile(&values, 0.75),
        p90: percentile(&values, 0.90),
        max: *values.last().expect("quality values are non-empty"),
    })
}

/// 以最近秩位置返回冻结分位数，避免插值制造不存在的样本值。
fn percentile(values: &[f64], quantile: f64) -> f64 {
    let rank = (quantile * values.len() as f64).ceil().max(1.0) as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

/// 在排序完成 K 中定位精确时间，不允许近似或补偿信号。
fn candle_index(candles: &[ComputedCandle], ts_ms: i64) -> Result<usize> {
    candles
        .binary_search_by_key(&ts_ms, |candle| candle.candle.ts)
        .map_err(|_| anyhow::anyhow!("candle timestamp missing: {ts_ms}"))
}

/// 多头资格要求快线在慢线下，空头资格完全镜像。
fn qualification_relation_holds(direction: &str, ema144: f64, ema576: f64) -> Result<bool> {
    match direction {
        "long" => Ok(ema144 < ema576),
        "short" => Ok(ema144 > ema576),
        _ => bail!("unsupported candidate direction: {direction}"),
    }
}

/// 返回收盘相对 EMA576 的方向性正距离。
fn directional_distance(direction: &str, close: f64, ema576: f64) -> Result<f64> {
    match direction {
        "long" => Ok(close - ema576),
        "short" => Ok(ema576 - close),
        _ => bail!("unsupported candidate direction: {direction}"),
    }
}

/// 判断收盘是否仍停留在当前 EMA576 突破侧。
fn price_on_breakout_side(direction: &str, close: f64, ema576: f64) -> Result<bool> {
    Ok(directional_distance(direction, close, ema576)? > 0.0)
}

/// 构造与冻结 V16 一致的稳定候选 ID。
fn candidate_id(candidate: &V2Candidate) -> String {
    format!(
        "{}:{}:{}",
        candidate.symbol, candidate.signal_ts_ms, candidate.direction
    )
}

/// 序列化独立研究机器报告，不写数据库或运行态配置。
fn write_report(output: &Path, report: &QualityMachineReport) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建入场质量报告目录失败：{}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(report)?;
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入入场质量报告失败：{}", output.display()))
}

#[cfg(test)]
mod tests;
