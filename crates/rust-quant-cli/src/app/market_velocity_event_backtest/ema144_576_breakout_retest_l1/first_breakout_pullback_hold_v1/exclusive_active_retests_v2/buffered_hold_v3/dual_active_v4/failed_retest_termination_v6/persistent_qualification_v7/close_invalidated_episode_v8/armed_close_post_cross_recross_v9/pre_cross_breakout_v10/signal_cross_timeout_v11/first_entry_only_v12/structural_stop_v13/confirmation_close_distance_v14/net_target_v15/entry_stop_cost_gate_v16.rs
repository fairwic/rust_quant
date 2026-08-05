//! V15 净 2R 退出合同下，按入场时结构止损成本占 R 过滤真实成交机会。
//!
//! L1 只读取冻结 V14 信号与下一根连续 15m 开盘；被门禁拒绝的机会不形成成交，
//! 也不消费同一 setup 的首笔真实成交资格。L1 通过后才执行一次冻结 L2。

pub mod breakout_acceptance_8bar_v19;
pub mod breakout_distance_2_5atr_v18;
mod breakout_entry_quality_common;
pub mod composite_acceptance_window_extreme_2_0atr_ema576_hold_relation_reset_v24;
pub mod composite_acceptance_window_extreme_2_0atr_six_close_ema576_hold_relation_reset_v25;
pub mod composite_breakout_quality_2_0atr_ema576_hold_relation_reset_v23;
pub mod composite_breakout_quality_2_0atr_ema576_hold_v22;
pub mod composite_breakout_quality_2_0atr_v21;
pub mod composite_breakout_quality_v20;
pub mod qualification_cycle_reset_v17;
pub mod six_close_structural_stop_1atr_v26;

use super::super::super::super::super::super::l2::{
    inspect_entry_risk,
    replay::{replay_verified_candidate_ledger, ReplaySource},
    stop_cost_r_for_prices, EntryRiskGatePolicy, InitialRiskPolicy, SetupEntryPolicy,
    TargetRiskPolicy, V10L2Identity, V10L2Report,
};
use super::super::{load_verified_v14_replay_input, sha256_hex, validate_v14_l1_source};
use super::{V15_CANDIDATE_KEY, V15_L1_RULE_VERSION, V15_L2_RULE_VERSION};
use crate::app::market_velocity_event_backtest::{
    ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::V2Candidate,
    BacktestDataSet,
};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// V16 独立候选身份；成交前成本门禁不得覆盖 V15。
pub const V16_CANDIDATE_KEY: &str =
    "market_momentum_ema576_first_entry_ema144_structural_stop_close_distance_net_target_2r_stop_cost_cap050r_15m_v16";
/// V16 L1 只使用入场时已知风险价格检查 0.50R 成本上限。
pub const V16_L1_RULE_VERSION: &str =
    "l1_v16_v15_stop_exit_roundtrip_cost_r_cap050_next_open_no_outcome_v1";
/// V16 L2 在 V15 上只增加成交前 0.50R 止损成本门禁。
pub const V16_L2_RULE_VERSION: &str =
    "l2_v16_v15_stop_cost_cap050_first_qualifying_fill_structural030_net200_v1";

const EXPECTED_V14_REPORT_SHA256: &str =
    "099827aa7924f2e754fa5926e6b50299eaedfc03aa9c5ddc284ce2bf4fed51c1";
const EXPECTED_V15_REPORT_SHA256: &str =
    "802527ed3c58627077337aca5a03a46cf57102a3f9d2b3a68eaa67dc3bed161c";
const EXPECTED_V14_L1_REPORT_SHA256: &str =
    "23edf396fe1b82681789c9323d38a30b17e1defa03d9d8b4551ac2f059e7b475";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_CANDIDATES: usize = 15_024;
const EXPECTED_VALID_OPPORTUNITIES: usize = 14_987;
const EXPECTED_STRUCTURAL_INVALIDS: usize = 37;
const STRUCTURAL_STOP_BUFFER_ATR: f64 = 0.30;
const NET_TARGET_R: f64 = 2.00;
const MAX_STOP_COST_R: f64 = 0.50;
const MIN_AFFECTED_RATIO_PCT: f64 = 10.0;
const MAX_AFFECTED_RATIO_PCT: f64 = 70.0;
const MIN_RETAINED_RATIO_PCT: f64 = 30.0;
const MIN_DIRECTION_OPPORTUNITIES: usize = 100;
const MIN_SYMBOLS: usize = 8;
const MIN_MONTHS_BJT: usize = 6;
const MIN_EVENTS: usize = 100;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const ONE_INCH_SIGNAL_TS_MS: i64 = 1_784_460_600_000;
const UMA_SIGNAL_TS_MS: i64 = 1_784_463_300_000;
const XRP_SIGNAL_TS_MS: i64 = 1_784_466_000_000;
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

/// V16 的单变量、因果字段和运行隔离身份。
#[derive(Debug, Clone, Serialize)]
pub struct V16L1Identity {
    /// 当前只处于无 outcome 快速研究。
    pub level: &'static str,
    /// 与 V15 并存的独立候选键。
    pub candidate_key: &'static str,
    /// 0.50R 成本门禁精确规则。
    pub rule_version: &'static str,
    /// 相对 V15 唯一允许变化的入场门禁。
    pub only_variable: &'static str,
    /// 被拒绝机会与首笔真实成交之间的生命周期合同。
    pub setup_consumption_policy: &'static str,
    /// L1 实际读取的生产时点字段。
    pub causal_field_boundary: &'static str,
    /// L1 明确禁止读取的结果字段。
    pub label_boundary: &'static str,
    /// 与所有运行态路径的隔离边界。
    pub runtime_boundary: &'static str,
}

/// 一个冻结候选在下一根连续开盘时的结构风险和成本门禁证据。
#[derive(Debug, Clone, Serialize)]
pub struct V16Opportunity {
    /// 稳定候选身份。
    pub candidate_id: String,
    /// OKX USDT 永续交易对。
    pub symbol: String,
    /// long 或 short。
    pub direction: &'static str,
    /// 长期资格完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 有效突破时间，Unix 毫秒。
    pub breakout_ts_ms: i64,
    /// 本次回踩重新武装时间，Unix 毫秒。
    pub rearmed_ts_ms: i64,
    /// 回踩确认收盘时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 回踩确认收盘时间，北京时间。
    pub signal_time_bjt: String,
    /// 交叉前或交叉后回踩分组。
    pub cross_phase: &'static str,
    /// 信号时 EMA144。
    pub signal_ema144: f64,
    /// 信号时 EMA576。
    pub signal_ema576: f64,
    /// 信号时 ATR14。
    pub signal_atr14: f64,
    /// 回踩极值到 EMA144 的方向归一化 ATR。
    pub retest_extreme_to_ema144_atr: f64,
    /// 确认收盘到 EMA144 的方向归一化 ATR。
    pub close_to_ema144_directional_atr: f64,
    /// 下一根连续 15m 开盘时间；无法解析时为空。
    pub entry_ts_ms: Option<i64>,
    /// 下一根连续 15m 开盘时间，北京时间。
    pub entry_time_bjt: Option<String>,
    /// 下一根连续 15m 开盘价。
    pub entry_price: Option<f64>,
    /// EMA144±0.30ATR14 冻结结构止损价。
    pub initial_stop_price: Option<f64>,
    /// 入场到止损的初始价格风险。
    pub initial_risk: Option<f64>,
    /// 初始价格风险占入场价百分比。
    pub initial_risk_pct: Option<f64>,
    /// V15 成本后净 2R 冻结目标价。
    pub target_price: Option<f64>,
    /// 假设结构止损成交时的开平双边成本 R。
    pub stop_cost_r: Option<f64>,
    /// true 表示该机会允许进入真实成交回放。
    pub eligible: bool,
    /// 结构无效或成本超过上限时的因果 blocker。
    pub blocked_reason: Option<&'static str>,
}

/// V16 L1 的合法机会、过滤比例和分散覆盖。
#[derive(Debug, Clone, Serialize)]
pub struct V16L1Coverage {
    /// 冻结 V14 候选总数。
    pub baseline_candidate_count: usize,
    /// 能在下一根连续开盘冻结合法结构风险的机会数。
    pub structurally_valid_opportunity_count: usize,
    /// 无法形成合法结构止损的候选数。
    pub structural_invalid_count: usize,
    /// 因止损成本超过 0.50R 被拒绝的机会数。
    pub rejected_by_cost_gate_count: usize,
    /// 允许进入真实成交回放的机会数。
    pub eligible_opportunity_count: usize,
    /// 成本门禁影响合法机会的百分比。
    pub affected_ratio_pct: f64,
    /// 成本门禁保留合法机会的百分比。
    pub retained_ratio_pct: f64,
    /// 合格机会按多空方向数量。
    pub eligible_by_direction: BTreeMap<String, usize>,
    /// 合格机会覆盖交易对数量。
    pub eligible_symbol_count: usize,
    /// 合格机会覆盖北京时间自然月数量。
    pub eligible_month_count_bjt: usize,
    /// 按方向与一小时连续触发链归并的合格事件数。
    pub eligible_effective_market_events: usize,
    /// 全部候选在成交前被拒绝的原因计数。
    pub blockers: BTreeMap<String, usize>,
}

/// 一个入场时数值维度的冻结离散分布。
#[derive(Debug, Clone, Serialize)]
pub struct V16Distribution {
    /// 样本数量。
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

/// 用户指定失败样本在成本门禁下的因果判定。
#[derive(Debug, Clone, Serialize)]
pub struct V16NamedSample {
    /// 1INCH、UMA 或 XRP。
    pub sample: &'static str,
    /// 稳定候选身份。
    pub candidate_id: String,
    /// OKX USDT 永续交易对。
    pub symbol: String,
    /// long 或 short。
    pub direction: &'static str,
    /// setup 完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 信号完成时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 信号完成时间，北京时间。
    pub signal_time_bjt: String,
    /// 下一根连续 15m 开盘时间，北京时间。
    pub entry_time_bjt: String,
    /// 下一根开盘价。
    pub entry_price: f64,
    /// 冻结结构止损价。
    pub initial_stop_price: f64,
    /// 初始价格风险。
    pub initial_risk: f64,
    /// 初始价格风险占入场价百分比。
    pub initial_risk_pct: f64,
    /// 假设止损成交时的双边成本 R。
    pub stop_cost_r: f64,
    /// false 表示目标样本已被成交前门禁拒绝。
    pub eligible: bool,
    /// 预期为 stop_cost_r_above_max。
    pub blocked_reason: &'static str,
}

/// V16 L1 是否允许执行一次冻结 L2 回放。
#[derive(Debug, Clone, Serialize)]
pub struct V16L1Decision {
    /// coverage_pass_ready_for_l2_prereg 或 stop。
    pub status: &'static str,
    /// 每项无 outcome 预注册门禁。
    pub gates: BTreeMap<&'static str, bool>,
    /// L1 必须固定为 false。
    pub outcome_evaluation_performed: bool,
    /// 当前停止或升级依据。
    pub reason: String,
}

/// V16 L1 完整机器证据；机会账本不包含任何成交后结果字段。
#[derive(Debug, Clone, Serialize)]
pub struct V16L1Report {
    /// V16 L1 JSON 字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 单变量、因果字段与运行隔离身份。
    pub identity: V16L1Identity,
    /// 冻结 V14 合并报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 冻结 V15 合并报告 SHA-256。
    pub source_v15_report_sha256: String,
    /// 合法机会、门禁影响与覆盖统计。
    pub coverage: V16L1Coverage,
    /// 全部合法机会的止损成本 R 分布。
    pub all_valid_stop_cost_r_distribution: V16Distribution,
    /// 合格机会的止损成本 R 分布。
    pub eligible_stop_cost_r_distribution: V16Distribution,
    /// 全部合法机会的初始风险百分比分布。
    pub all_valid_initial_risk_pct_distribution: V16Distribution,
    /// 合格机会的初始风险百分比分布。
    pub eligible_initial_risk_pct_distribution: V16Distribution,
    /// 1INCH、UMA、XRP 目标样本门禁证据。
    pub named_samples: Vec<V16NamedSample>,
    /// 无 outcome 覆盖结论。
    pub decision: V16L1Decision,
    /// 15,024 个候选的完整成交前因果账本。
    pub opportunities: Vec<V16Opportunity>,
}

/// V16 单一机器产物；L1 不通过时 L2 固定为空。
#[derive(Debug, Clone, Serialize)]
pub struct V16MachineReport {
    /// V16 合并机器结果字段合同版本。
    pub schema_version: &'static str,
    /// 合并报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 冻结 V14 合并报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 冻结 V15 合并报告 SHA-256。
    pub source_v15_report_sha256: String,
    /// 内嵌 L1 负载的 SHA-256，供 L2 回查精确门禁账本。
    pub l1_payload_sha256: String,
    /// 完整无 outcome L1 机器证据。
    pub l1: V16L1Report,
    /// 只有 L1 通过时才存在的唯一一次 L2 回放。
    pub l2: Option<V10L2Report>,
}

/// 校验冻结 V14/V15，执行 V16 L1，并仅在通过时运行一次 L2。
pub async fn run_v16_l1_l2_replay(
    v14_source: &Path,
    v15_source: &Path,
    output: &Path,
) -> Result<V16MachineReport> {
    let (v14_sha256, v14_l1) = load_v14_source(v14_source)?;
    let v15_sha256 = validate_v15_source(v15_source)?;
    let replay_input = load_verified_v14_replay_input(&v14_l1).await?;
    if replay_input.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("V16 reloaded dataset fingerprint mismatch");
    }
    let l1 = build_l1_report(
        &replay_input.data,
        &replay_input.candidates,
        v14_sha256.clone(),
        v15_sha256.clone(),
    )?;
    let l1_payload_sha256 = sha256_hex(&serde_json::to_vec(&l1)?);
    let l2 = if l1.decision.status == "coverage_pass_ready_for_l2_prereg" {
        Some(replay_verified_candidate_ledger(
            &replay_input.data,
            ReplaySource::new(
                "market_momentum_ema576_entry_stop_cost_cap_050r_l2_v16",
                V10L2Identity {
                    level: "L2_local_multi_symbol_diagnostic",
                    candidate_key: V16_CANDIDATE_KEY,
                    source_l1_rule_version: V16_L1_RULE_VERSION,
                    rule_version: V16_L2_RULE_VERSION,
                    only_variable: "allow a V15 opportunity to become a real fill only when its exact structural-stop roundtrip cost is at most 0.50R at the next contiguous 15m open",
                    entry_policy: "unchanged V15 next-contiguous-15m-open execution; stop-cost rejection occurs before fill and does not consume the setup, so the first later qualifying real fill remains eligible",
                    initial_stop_policy: "unchanged V15 long signal EMA144 minus 0.30 ATR14 and short signal EMA144 plus 0.30 ATR14; freeze at entry and never loosen",
                    target_policy: "unchanged V15 cost-adjusted target that settles to net 2.00R after 8bps per side; no break-even, trailing, partial, runner, or reversal",
                    intrabar_conflict_policy: "unchanged entry candle inclusion and stop-first ordering when stop and target are both touched in one candle",
                    symbol_position_policy: "unchanged one open trade per symbol and one qualifying real fill per symbol x direction x setup_ts",
                    per_side_cost_rate: PER_SIDE_COST_RATE,
                    max_holding_ms: MAX_HOLDING_MS,
                    funding_modeled: false,
                    outcome_evaluation_performed: true,
                    runtime_boundary: "research-only V16 L2; not registered in Pine, paper, readonly shadow, live worker, database, scheduler, compose, or production presets",
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
                replay_input.candidates,
            ),
        ))
    } else {
        None
    };
    let report = V16MachineReport {
        schema_version: "market_momentum_ema576_entry_stop_cost_cap_050r_l1_l2_v16",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        source_v14_report_sha256: v14_sha256,
        source_v15_report_sha256: v15_sha256,
        l1_payload_sha256,
        l1,
        l2,
    };
    write_report(output, &report)?;
    Ok(report)
}

/// 读取并验证冻结 V14 合并报告及其内嵌 L1 身份。
fn load_v14_source(source: &Path) -> Result<(String, Value)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取冻结 V14 合并报告失败：{}", source.display()))?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != EXPECTED_V14_REPORT_SHA256 {
        bail!("V16 source V14 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析冻结 V14 合并报告失败")?;
    if source.pointer("/schema_version").and_then(Value::as_str)
        != Some("market_momentum_ema576_confirmation_close_distance_l1_l2_v14")
    {
        bail!("V16 source V14 schema mismatch");
    }
    let l1 = source
        .pointer("/l1")
        .cloned()
        .context("V16 source V14 L1 missing")?;
    validate_v14_l1_source(&l1)?;
    Ok((sha256, l1))
}

/// 只校验 V15 文件身份和 L1 晋级状态，不读取或筛选其 L2 outcome。
fn validate_v15_source(source: &Path) -> Result<String> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取冻结 V15 合并报告失败：{}", source.display()))?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != EXPECTED_V15_REPORT_SHA256 {
        bail!("V16 source V15 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析冻结 V15 合并报告失败")?;
    if source.pointer("/schema_version").and_then(Value::as_str)
        != Some("market_momentum_ema576_net_target_2r_l1_l2_v15")
        || source
            .pointer("/source_v14_report_sha256")
            .and_then(Value::as_str)
            != Some(EXPECTED_V14_REPORT_SHA256)
        || source
            .pointer("/l1/identity/candidate_key")
            .and_then(Value::as_str)
            != Some(V15_CANDIDATE_KEY)
        || source
            .pointer("/l1/identity/rule_version")
            .and_then(Value::as_str)
            != Some(V15_L1_RULE_VERSION)
        || source
            .pointer("/l1/decision/status")
            .and_then(Value::as_str)
            != Some("coverage_pass_ready_for_l2_prereg")
        || source
            .pointer("/l2/identity/rule_version")
            .and_then(Value::as_str)
            != Some(V15_L2_RULE_VERSION)
        || source
            .pointer("/l2/source_l1_report_sha256")
            .and_then(Value::as_str)
            != Some(EXPECTED_V14_L1_REPORT_SHA256)
    {
        bail!("V16 source V15 strategy identity mismatch");
    }
    Ok(sha256)
}

/// 对全部 V14 候选生成下一根开盘的因果风险账本和无 outcome 覆盖结论。
fn build_l1_report(
    data: &BacktestDataSet,
    candidates: &[V2Candidate],
    v14_sha256: String,
    v15_sha256: String,
) -> Result<V16L1Report> {
    let opportunities = candidates
        .iter()
        .map(|candidate| opportunity(data, candidate))
        .collect::<Result<Vec<_>>>()?;
    validate_no_outcome_fields(&opportunities)?;

    let valid = opportunities
        .iter()
        .filter(|item| item.stop_cost_r.is_some())
        .collect::<Vec<_>>();
    let eligible = valid
        .iter()
        .copied()
        .filter(|item| item.eligible)
        .collect::<Vec<_>>();
    let rejected_by_cost_gate_count = valid.len().saturating_sub(eligible.len());
    let structural_invalid_count = opportunities.len().saturating_sub(valid.len());
    let mut eligible_by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    let mut blockers = BTreeMap::new();
    for item in &eligible {
        *eligible_by_direction
            .entry(item.direction.to_owned())
            .or_default() += 1;
        symbols.insert(item.symbol.clone());
        months.insert(month_bjt(item.signal_ts_ms)?);
    }
    for item in &opportunities {
        if let Some(reason) = item.blocked_reason {
            *blockers.entry(reason.to_owned()).or_default() += 1;
        }
    }
    let coverage = V16L1Coverage {
        baseline_candidate_count: opportunities.len(),
        structurally_valid_opportunity_count: valid.len(),
        structural_invalid_count,
        rejected_by_cost_gate_count,
        eligible_opportunity_count: eligible.len(),
        affected_ratio_pct: ratio_pct(rejected_by_cost_gate_count, valid.len()),
        retained_ratio_pct: ratio_pct(eligible.len(), valid.len()),
        eligible_by_direction,
        eligible_symbol_count: symbols.len(),
        eligible_month_count_bjt: months.len(),
        eligible_effective_market_events: effective_event_count(&eligible),
        blockers,
    };
    let named_samples = named_samples(&opportunities)?;
    let mut gates = BTreeMap::new();
    gates.insert(
        "baseline_candidate_count_matches",
        coverage.baseline_candidate_count == EXPECTED_CANDIDATES,
    );
    gates.insert(
        "structurally_valid_opportunity_count_matches",
        coverage.structurally_valid_opportunity_count == EXPECTED_VALID_OPPORTUNITIES,
    );
    gates.insert(
        "structural_invalid_count_matches",
        coverage.structural_invalid_count == EXPECTED_STRUCTURAL_INVALIDS
            && coverage.blockers.get("initial_stop_not_beyond_entry")
                == Some(&EXPECTED_STRUCTURAL_INVALIDS),
    );
    gates.insert(
        "affected_ratio_between_10_and_70_pct",
        coverage.affected_ratio_pct >= MIN_AFFECTED_RATIO_PCT
            && coverage.affected_ratio_pct <= MAX_AFFECTED_RATIO_PCT,
    );
    gates.insert(
        "retained_ratio_at_least_30_pct",
        coverage.retained_ratio_pct >= MIN_RETAINED_RATIO_PCT,
    );
    gates.insert(
        "both_directions_have_at_least_100_opportunities",
        ["long", "short"].iter().all(|direction| {
            coverage
                .eligible_by_direction
                .get(*direction)
                .copied()
                .unwrap_or_default()
                >= MIN_DIRECTION_OPPORTUNITIES
        }),
    );
    gates.insert(
        "cross_symbol_month_event_coverage_preserved",
        coverage.eligible_symbol_count >= MIN_SYMBOLS
            && coverage.eligible_month_count_bjt >= MIN_MONTHS_BJT
            && coverage.eligible_effective_market_events >= MIN_EVENTS,
    );
    gates.insert(
        "named_failed_samples_rejected_by_cost_gate",
        named_samples.iter().all(|sample| {
            !sample.eligible
                && sample.blocked_reason == "stop_cost_r_above_max"
                && sample.stop_cost_r > MAX_STOP_COST_R
        }),
    );
    gates.insert("forbidden_outcome_fields_absent", true);
    let passed = gates.values().all(|passed| *passed);
    Ok(V16L1Report {
        schema_version: "market_momentum_ema576_entry_stop_cost_cap_050r_l1_v16",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V16L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V16_CANDIDATE_KEY,
            rule_version: V16_L1_RULE_VERSION,
            only_variable: "after V15 entry, structural stop, and exact cost are known, allow a real fill only when stop-exit roundtrip cost is at most 0.50R",
            setup_consumption_policy: "a stop-cost-rejected opportunity is not a fill and does not consume symbol x direction x setup_ts; the first later qualifying real fill consumes it",
            causal_field_boundary: "frozen V14 signal fields plus only the next contiguous 15m open, V15 structural stop, initial risk, net-2R target geometry, and exact stop-exit cost R",
            label_boundary: "no later candle, complete flag, exit time, exit price, exit reason, gross R, realized cost R, net R, MFE, MAE, PnL, win, or loss is read",
            runtime_boundary: "research-only V16 L1; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
        },
        source_v14_report_sha256: v14_sha256,
        source_v15_report_sha256: v15_sha256,
        coverage,
        all_valid_stop_cost_r_distribution: distribution(
            valid
                .iter()
                .filter_map(|item| item.stop_cost_r)
                .collect(),
        )?,
        eligible_stop_cost_r_distribution: distribution(
            eligible
                .iter()
                .filter_map(|item| item.stop_cost_r)
                .collect(),
        )?,
        all_valid_initial_risk_pct_distribution: distribution(
            valid
                .iter()
                .filter_map(|item| item.initial_risk_pct)
                .collect(),
        )?,
        eligible_initial_risk_pct_distribution: distribution(
            eligible
                .iter()
                .filter_map(|item| item.initial_risk_pct)
                .collect(),
        )?,
        named_samples,
        decision: V16L1Decision {
            status: if passed {
                "coverage_pass_ready_for_l2_prereg"
            } else {
                "stop"
            },
            gates,
            outcome_evaluation_performed: false,
            reason: if passed {
                "0.50R 成本门禁拒绝三个指定失败样本，并保持预注册的双方向、多币种、多月份和事件覆盖；允许执行一次冻结 L2。".to_owned()
            } else {
                "至少一项无 outcome 的数量、目标样本或分散覆盖门禁失败；停止在 L1，不运行 L2。".to_owned()
            },
        },
        opportunities,
    })
}

/// 把一个冻结候选投影为结构无效、成本拒绝或合格机会，不读取其后价格路径。
fn opportunity(data: &BacktestDataSet, candidate: &V2Candidate) -> Result<V16Opportunity> {
    let mut result = V16Opportunity {
        candidate_id: format!(
            "{}:{}:{}",
            candidate.symbol, candidate.signal_ts_ms, candidate.direction
        ),
        symbol: candidate.symbol.clone(),
        direction: candidate.direction,
        setup_ts_ms: candidate.setup_ts_ms,
        breakout_ts_ms: candidate.breakout_ts_ms,
        rearmed_ts_ms: candidate.rearmed_ts_ms,
        signal_ts_ms: candidate.signal_ts_ms,
        signal_time_bjt: format_bjt(candidate.signal_ts_ms)?,
        cross_phase: candidate.cross_phase,
        signal_ema144: candidate.ema144,
        signal_ema576: candidate.ema576,
        signal_atr14: candidate.atr14,
        retest_extreme_to_ema144_atr: candidate.retest_extreme_to_ema144_atr,
        close_to_ema144_directional_atr: candidate.close_to_ema144_directional_atr,
        entry_ts_ms: None,
        entry_time_bjt: None,
        entry_price: None,
        initial_stop_price: None,
        initial_risk: None,
        initial_risk_pct: None,
        target_price: None,
        stop_cost_r: None,
        eligible: false,
        blocked_reason: None,
    };
    let plan = match inspect_entry_risk(
        data,
        candidate,
        InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR),
        TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R),
    ) {
        Ok(plan) => plan,
        Err(reason) => {
            result.blocked_reason = Some(reason);
            return Ok(result);
        }
    };
    result.entry_ts_ms = Some(plan.entry_ts_ms);
    result.entry_time_bjt = Some(format_bjt(plan.entry_ts_ms)?);
    result.entry_price = Some(plan.entry_price);
    result.initial_stop_price = Some(plan.stop_price);
    result.initial_risk = Some(plan.initial_risk);
    result.initial_risk_pct = Some(plan.initial_risk / plan.entry_price * 100.0);
    result.target_price = Some(plan.target_price);
    let stop_cost_r =
        match stop_cost_r_for_prices(plan.entry_price, plan.stop_price, plan.initial_risk) {
            Ok(stop_cost_r) => stop_cost_r,
            Err(reason) => {
                result.blocked_reason = Some(reason);
                return Ok(result);
            }
        };
    result.stop_cost_r = Some(stop_cost_r);
    result.eligible = stop_cost_r <= MAX_STOP_COST_R + 1e-12;
    if !result.eligible {
        result.blocked_reason = Some("stop_cost_r_above_max");
    }
    Ok(result)
}

/// 提取三个用户指定样本，并要求其结构风险字段均可审计。
fn named_samples(opportunities: &[V16Opportunity]) -> Result<Vec<V16NamedSample>> {
    [
        ("1INCH", "1INCH-USDT-SWAP", ONE_INCH_SIGNAL_TS_MS),
        ("UMA", "UMA-USDT-SWAP", UMA_SIGNAL_TS_MS),
        ("XRP", "XRP-USDT-SWAP", XRP_SIGNAL_TS_MS),
    ]
    .into_iter()
    .map(|(sample, symbol, signal_ts_ms)| {
        let item = opportunities
            .iter()
            .find(|item| item.symbol == symbol && item.signal_ts_ms == signal_ts_ms)
            .with_context(|| format!("V16 named sample missing: {sample}"))?;
        Ok(V16NamedSample {
            sample,
            candidate_id: item.candidate_id.clone(),
            symbol: item.symbol.clone(),
            direction: item.direction,
            setup_ts_ms: item.setup_ts_ms,
            signal_ts_ms: item.signal_ts_ms,
            signal_time_bjt: item.signal_time_bjt.clone(),
            entry_time_bjt: item
                .entry_time_bjt
                .clone()
                .with_context(|| format!("V16 named sample entry missing: {sample}"))?,
            entry_price: required_f64(item.entry_price, sample, "entry_price")?,
            initial_stop_price: required_f64(
                item.initial_stop_price,
                sample,
                "initial_stop_price",
            )?,
            initial_risk: required_f64(item.initial_risk, sample, "initial_risk")?,
            initial_risk_pct: required_f64(item.initial_risk_pct, sample, "initial_risk_pct")?,
            stop_cost_r: required_f64(item.stop_cost_r, sample, "stop_cost_r")?,
            eligible: item.eligible,
            blocked_reason: item
                .blocked_reason
                .with_context(|| format!("V16 named sample blocker missing: {sample}"))?,
        })
    })
    .collect()
}

/// 从指定样本的可选因果字段读取有限数值。
fn required_f64(value: Option<f64>, sample: &str, field: &str) -> Result<f64> {
    let value = value.with_context(|| format!("V16 named sample {field} missing: {sample}"))?;
    if !value.is_finite() {
        bail!("V16 named sample {field} non-finite: {sample}");
    }
    Ok(value)
}

/// 检查序列化后的 L1 机会对象没有意外混入成交后字段。
fn validate_no_outcome_fields(opportunities: &[V16Opportunity]) -> Result<()> {
    for opportunity in opportunities {
        let value = serde_json::to_value(opportunity)?;
        let object = value
            .as_object()
            .context("V16 opportunity is not an object")?;
        if let Some(field) = FORBIDDEN_OUTCOME_FIELDS
            .iter()
            .find(|field| object.contains_key(**field))
        {
            bail!("V16 L1 forbidden outcome field present: {field}");
        }
    }
    Ok(())
}

/// 生成最小、P10、中位、P90 与最大值的冻结离散分布。
fn distribution(mut values: Vec<f64>) -> Result<V16Distribution> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        bail!("V16 distribution is empty or non-finite");
    }
    values.sort_by(f64::total_cmp);
    Ok(V16Distribution {
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

/// 使用信号时间和方向重建一小时连续事件链，不读取任何成交后事件字段。
fn effective_event_count(opportunities: &[&V16Opportunity]) -> usize {
    let mut ordered = opportunities
        .iter()
        .map(|item| (item.signal_ts_ms, item.direction))
        .collect::<Vec<_>>();
    ordered.sort();
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for (signal_ts_ms, direction) in ordered {
        let starts_new = last_by_direction.get(direction).is_none_or(|previous| {
            signal_ts_ms.saturating_sub(*previous) > EVENT_CLUSTER_WINDOW_MS
        });
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(direction, signal_ts_ms);
    }
    count
}

/// 把 Unix 毫秒转换为北京时间月份。
fn month_bjt(ts_ms: i64) -> Result<String> {
    let datetime = chrono::FixedOffset::east_opt(8 * 60 * 60)
        .context("construct UTC+8 offset")?
        .timestamp_millis_opt(ts_ms)
        .single()
        .context("V16 signal timestamp invalid")?;
    Ok(format!("{:04}-{:02}", datetime.year(), datetime.month()))
}

/// 把 Unix 毫秒转换为秒精度北京时间。
fn format_bjt(ts_ms: i64) -> Result<String> {
    let datetime = chrono::FixedOffset::east_opt(8 * 60 * 60)
        .context("construct UTC+8 offset")?
        .timestamp_millis_opt(ts_ms)
        .single()
        .context("V16 timestamp invalid")?;
    Ok(datetime.format("%Y-%m-%d %H:%M:%S %:z").to_string())
}

/// 把计数转成百分比；冻结源非空，零分母仅作防御性返回。
fn ratio_pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

/// 序列化 V16 合并机器产物，不写数据库或修改任何运行态注册。
fn write_report(output: &Path, report: &V16MachineReport) -> Result<()> {
    let serialized = serde_json::to_string_pretty(report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V16 机器报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V16 机器报告失败：{}", output.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// V16 的下一根开盘机会对象必须允许因果入场字段，但禁止任何退出或收益标签。
    #[test]
    fn opportunity_schema_contains_no_outcome_fields() {
        let opportunity = V16Opportunity {
            candidate_id: "TEST:0:long".to_owned(),
            symbol: "TEST-USDT-SWAP".to_owned(),
            direction: "long",
            setup_ts_ms: 0,
            breakout_ts_ms: 900_000,
            rearmed_ts_ms: 1_800_000,
            signal_ts_ms: 2_700_000,
            signal_time_bjt: "1970-01-01 08:45:00 +08:00".to_owned(),
            cross_phase: "post_cross_retest",
            signal_ema144: 100.0,
            signal_ema576: 99.0,
            signal_atr14: 2.0,
            retest_extreme_to_ema144_atr: 0.1,
            close_to_ema144_directional_atr: 0.2,
            entry_ts_ms: Some(3_600_000),
            entry_time_bjt: Some("1970-01-01 09:00:00 +08:00".to_owned()),
            entry_price: Some(101.0),
            initial_stop_price: Some(99.4),
            initial_risk: Some(1.6),
            initial_risk_pct: Some(1.584_158_415_841_584_2),
            target_price: Some(104.36),
            stop_cost_r: Some(0.1002),
            eligible: true,
            blocked_reason: None,
        };

        validate_no_outcome_fields(&[opportunity]).expect("causal L1 schema");
    }

    /// 成本上限是包含边界的单一固定阈值。
    #[test]
    fn stop_cost_cap_is_inclusive() {
        assert!(0.50 <= MAX_STOP_COST_R + 1e-12);
        assert!(!(0.500_001 <= MAX_STOP_COST_R + 1e-12));
    }
}
