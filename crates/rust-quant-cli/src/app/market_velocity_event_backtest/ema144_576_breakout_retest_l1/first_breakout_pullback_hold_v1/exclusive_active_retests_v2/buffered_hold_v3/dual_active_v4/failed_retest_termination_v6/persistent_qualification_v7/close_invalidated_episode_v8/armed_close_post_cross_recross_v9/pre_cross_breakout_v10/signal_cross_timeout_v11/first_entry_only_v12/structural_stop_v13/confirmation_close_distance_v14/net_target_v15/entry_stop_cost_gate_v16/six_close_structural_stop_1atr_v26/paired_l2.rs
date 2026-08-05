//! 对 V25 0.30ATR 基线与 V26 1ATR 候选执行一次冻结的配对 L2 回放。

use super::*;
use serde::Serialize;

const EXPECTED_V26_L1_REPORT_SHA256: &str =
    "56e3eb7d2bd490a5bf60b6ee5af614514606cc2a690d266849185865390ac478";
const EXPECTED_V26_L1_PAYLOAD_SHA256: &str =
    "f303eb6ab0d0ed74918d1d23d0c3083ec1d0bab6d71d9affef708df558048e31";
const EXPECTED_V25_ELIGIBLE_SET_SHA256: &str =
    "7cfd7d464e98c3022cfc915127733a598b3eedb3fb804768fd9a6c77e9eb3c8d";
const EXPECTED_V26_ELIGIBLE_SET_SHA256: &str =
    "c8bdbaabd7c55ae3c9588c653429af19639417e738584a289a55f71fbb4c580c";
const EXPECTED_V25_ELIGIBLE: usize = 720;
const EXPECTED_V25_LONG: usize = 470;
const EXPECTED_V25_SHORT: usize = 250;
const EXPECTED_V26_ELIGIBLE: usize = 1_157;
const EXPECTED_V26_LONG: usize = 754;
const EXPECTED_V26_SHORT: usize = 403;
const BASELINE_STOP_BUFFER_ATR: f64 = 0.30;
const PER_SIDE_COST_RATE_V26: f64 = 0.0008;
const MAX_HOLDING_MS_V26: i64 = 24 * 60 * 60 * 1_000;
const ONE_HOUR_MS: i64 = 60 * 60 * 1_000;

/// V26 L2 的独立回放身份；它不覆盖旧版本风险合同。
pub const V26_L2_RULE_VERSION: &str =
    "l2_v26_paired_v25_structural030_vs_v26_structural100_net200_cost050_v1";

/// 一组冻结候选的完整文件和排序 ID 身份。
struct FrozenCandidateSet {
    report_sha256: String,
    candidate_ids: BTreeSet<String>,
}

/// 实际成交的风险宽度、成本与止损时点诊断。
#[derive(Debug, Clone, Serialize)]
pub struct V26TradeRiskDiagnostics {
    /// 应用持仓锁后进入回放的交易数。
    pub executed_trades: usize,
    /// 具有完整退出证据的交易数。
    pub completed_trades: usize,
    /// 止损或同 K 冲突时止损优先的交易数。
    pub stopped_trades: usize,
    /// 在入场 K 内触发止损的交易数。
    pub entry_bar_stops: usize,
    /// 入场 K 止损占全部止损百分比。
    pub entry_bar_stop_share_pct: f64,
    /// 入场后 60 分钟内触发止损的交易数。
    pub stops_within_60m: usize,
    /// 60 分钟内止损占全部止损百分比。
    pub stops_within_60m_share_pct: f64,
    /// 实际交易的初始风险除以信号 ATR14 分布。
    pub initial_risk_atr_distribution: V26Distribution,
    /// 实际交易的初始风险占入场价百分比分布。
    pub initial_risk_pct_distribution: V26Distribution,
    /// 假设初始止损成交时的双边成本 R 分布。
    pub initial_stop_cost_r_distribution: V26Distribution,
    /// 实际退出价结算的双边成本 R 分布。
    pub realized_cost_r_distribution: V26Distribution,
}

/// V26 相对 0.30ATR 基线的核心差值，正负方向均按候选减基线记录。
#[derive(Debug, Clone, Serialize)]
pub struct V26L2Delta {
    /// 实际完成交易数变化。
    pub completed_trades: i64,
    /// 成本后每笔期望变化。
    pub net_expectancy_r: f64,
    /// 成本后 Profit Factor 变化；任一侧不存在时为空。
    pub net_profit_factor: Option<f64>,
    /// 入场 K 止损占比变化；负值表示下降。
    pub entry_bar_stop_share_pct: f64,
    /// 60 分钟内止损占比变化；负值表示下降。
    pub stops_within_60m_share_pct: f64,
}

/// V26 配对回放的预注册联合门禁。
#[derive(Debug, Clone, Serialize)]
pub struct V26L2Decision {
    /// `L2_pass_L3_required` 或 `stop`。
    pub status: &'static str,
    /// 每项事前门禁结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 当前版本晋级或停止原因。
    pub reason: String,
}

/// V25 与 V26 的完整配对结果和风险诊断。
#[derive(Debug, Serialize)]
pub struct V26L2ComparisonReport {
    /// 配对报告 schema。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 冻结 V25 文件 SHA-256。
    pub source_v25_l1_report_sha256: String,
    /// 冻结 V26 文件 SHA-256。
    pub source_v26_l1_report_sha256: String,
    /// 重载行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// 两组候选 ID、方向数量与 V14 重建是否全部一致。
    pub source_candidate_sets_verified: bool,
    /// V25 六根信号在原 0.30ATR 风险下的基线回放。
    pub baseline_structural_stop_030atr: V10L2Report,
    /// V26 六根信号在 1ATR 风险下的候选回放。
    pub candidate_structural_stop_100atr: V10L2Report,
    /// 基线止损时点与风险分布。
    pub baseline_risk_diagnostics: V26TradeRiskDiagnostics,
    /// V26 止损时点与风险分布。
    pub candidate_risk_diagnostics: V26TradeRiskDiagnostics,
    /// 候选减基线的核心指标差值。
    pub delta: V26L2Delta,
    /// 预注册联合门禁结论。
    pub decision: V26L2Decision,
}

/// 校验两份冻结 L1 与 V14 重建账本后，执行唯一一次 0.30ATR/1ATR 配对回放。
pub async fn run_v26_l2(
    v25_source: &Path,
    v26_l1_source: &Path,
    v14_source: &Path,
    output: &Path,
) -> Result<V26L2ComparisonReport> {
    let frozen_v25 = load_v25_signals(v25_source)?;
    let baseline_ids = frozen_v25
        .source_v16_eligibility
        .iter()
        .filter_map(|(id, eligible)| eligible.then_some(id.clone()))
        .collect::<BTreeSet<_>>();
    if baseline_ids.len() != EXPECTED_V25_ELIGIBLE
        || candidate_set_sha256(&baseline_ids) != EXPECTED_V25_ELIGIBLE_SET_SHA256
    {
        bail!("V26 L2 baseline V25 candidate set mismatch");
    }
    let frozen_v26 = load_v26_candidate_set(v26_l1_source)?;
    if !frozen_v26
        .candidate_ids
        .is_subset(&frozen_v25.candidate_ids)
    {
        bail!("V26 L2 candidate set is not a subset of V25 signal-quality candidates");
    }

    let (v14_sha256, v14_l1) = super::super::load_v14_source(v14_source)?;
    if v14_sha256 != super::super::EXPECTED_V14_REPORT_SHA256 {
        bail!("V26 L2 source V14 report SHA mismatch");
    }
    let replay_input = super::super::super::super::load_verified_v14_replay_input(&v14_l1).await?;
    if replay_input.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("V26 L2 reloaded dataset fingerprint mismatch");
    }
    let rebuilt = replay_input
        .candidates
        .iter()
        .map(|candidate| (candidate_id(candidate), candidate.clone()))
        .collect::<HashMap<_, _>>();
    if rebuilt.len() != replay_input.candidates.len() {
        bail!("V26 L2 rebuilt V14 candidate IDs are not unique");
    }
    let baseline_candidates = select_candidates(&rebuilt, &baseline_ids)?;
    let candidate_candidates = select_candidates(&rebuilt, &frozen_v26.candidate_ids)?;
    validate_direction_counts(&baseline_candidates, EXPECTED_V25_LONG, EXPECTED_V25_SHORT)?;
    validate_direction_counts(&candidate_candidates, EXPECTED_V26_LONG, EXPECTED_V26_SHORT)?;

    let baseline = replay_verified_candidate_ledger(
        &replay_input.data,
        ReplaySource::new(
            "market_momentum_ema576_six_close_structural_stop_030atr_baseline_l2_v26",
            V10L2Identity {
                level: "L2_local_multi_symbol_paired_diagnostic",
                candidate_key: super::super::composite_acceptance_window_extreme_2_0atr_six_close_ema576_hold_relation_reset_v25::V25_CANDIDATE_KEY,
                source_l1_rule_version: super::super::composite_acceptance_window_extreme_2_0atr_six_close_ema576_hold_relation_reset_v25::V25_L1_RULE_VERSION,
                rule_version: V26_L2_RULE_VERSION,
                only_variable: "paired baseline: keep V25 six-close signals under signal EMA144 plus/minus 0.30 ATR14 while V26 changes only that buffer to 1.00 ATR14",
                entry_policy: "next contiguous 15m open after the completed signal; rejected or unresolved candidates do not consume the first-filled setup",
                initial_stop_policy: "baseline long signal EMA144 minus 0.30 ATR14 and short signal EMA144 plus 0.30 ATR14, frozen at entry",
                target_policy: "cost-adjusted target that settles to net 2.00R after 8bps per side",
                intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched",
                symbol_position_policy: "one open trade per symbol and first qualifying real fill per symbol x direction x setup",
                per_side_cost_rate: PER_SIDE_COST_RATE_V26,
                max_holding_ms: MAX_HOLDING_MS_V26,
                funding_modeled: false,
                outcome_evaluation_performed: true,
                runtime_boundary: "research-only V26 paired baseline; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
            },
            frozen_v25.report_sha256,
            replay_input.dataset_fingerprint_sha256.clone(),
            replay_input.returned_symbol_count,
            replay_input.eligible_symbol_count,
            replay_input.excluded_symbol_count,
            SetupEntryPolicy::FirstFilledPerSetup,
            InitialRiskPolicy::SignalEma144AtrBuffer(BASELINE_STOP_BUFFER_ATR),
            TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R_V26),
            EntryRiskGatePolicy::MaxStopCostR(MAX_STOP_COST_R_V26),
            baseline_candidates,
        ),
    );
    let candidate = replay_verified_candidate_ledger(
        &replay_input.data,
        ReplaySource::new(
            "market_momentum_ema576_six_close_structural_stop_100atr_l2_v26",
            V10L2Identity {
                level: "L2_local_multi_symbol_paired_diagnostic",
                candidate_key: V26_CANDIDATE_KEY,
                source_l1_rule_version: V26_L1_RULE_VERSION,
                rule_version: V26_L2_RULE_VERSION,
                only_variable: "relative to the paired V25 baseline, replace only the signal EMA144 plus/minus 0.30 ATR14 initial stop buffer with plus/minus 1.00 ATR14 and recompute the unchanged pre-fill cost gate",
                entry_policy: "next contiguous 15m open after the completed signal; rejected or unresolved candidates do not consume the first-filled setup",
                initial_stop_policy: "candidate long signal EMA144 minus 1.00 ATR14 and short signal EMA144 plus 1.00 ATR14, frozen at entry",
                target_policy: "cost-adjusted target that settles to net 2.00R after 8bps per side",
                intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched",
                symbol_position_policy: "one open trade per symbol and first qualifying real fill per symbol x direction x setup",
                per_side_cost_rate: PER_SIDE_COST_RATE_V26,
                max_holding_ms: MAX_HOLDING_MS_V26,
                funding_modeled: false,
                outcome_evaluation_performed: true,
                runtime_boundary: "research-only V26 paired candidate; no Pine, paper, readonly shadow, live worker, database write, scheduler, compose, or production registration",
            },
            frozen_v26.report_sha256.clone(),
            replay_input.dataset_fingerprint_sha256.clone(),
            replay_input.returned_symbol_count,
            replay_input.eligible_symbol_count,
            replay_input.excluded_symbol_count,
            SetupEntryPolicy::FirstFilledPerSetup,
            InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR_V26),
            TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R_V26),
            EntryRiskGatePolicy::MaxStopCostR(MAX_STOP_COST_R_V26),
            candidate_candidates,
        ),
    );
    let baseline_risk = risk_diagnostics(&baseline)?;
    let candidate_risk = risk_diagnostics(&candidate)?;
    let delta = build_delta(&baseline, &candidate, &baseline_risk, &candidate_risk);
    let decision = decide_v26_l2(&baseline, &candidate, &baseline_risk, &candidate_risk, true);
    let report = V26L2ComparisonReport {
        schema_version: "market_momentum_ema576_six_close_structural_stop_1atr_paired_l2_v26",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        source_v25_l1_report_sha256: EXPECTED_V25_REPORT_SHA256.to_owned(),
        source_v26_l1_report_sha256: frozen_v26.report_sha256,
        dataset_fingerprint_sha256: replay_input.dataset_fingerprint_sha256,
        source_candidate_sets_verified: true,
        baseline_structural_stop_030atr: baseline,
        candidate_structural_stop_100atr: candidate,
        baseline_risk_diagnostics: baseline_risk,
        candidate_risk_diagnostics: candidate_risk,
        delta,
        decision,
    };
    write_v26_l2_report(output, &report)?;
    Ok(report)
}

/// 读取 SHA 固定的 V26 L1，并提取唯一允许进入候选回放的 1,157 个 ID。
fn load_v26_candidate_set(source: &Path) -> Result<FrozenCandidateSet> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取冻结 V26 L1 失败：{}", source.display()))?;
    let report_sha256 = super::super::sha256_hex(&bytes);
    if report_sha256 != EXPECTED_V26_L1_REPORT_SHA256 {
        bail!("V26 L2 source V26 L1 report SHA mismatch");
    }
    let root: Value = serde_json::from_slice(&bytes).context("解析冻结 V26 L1 失败")?;
    if root.get("schema_version").and_then(Value::as_str)
        != Some("market_momentum_ema576_six_close_structural_stop_1atr_l1_v26")
        || root.get("source_v25_report_sha256").and_then(Value::as_str)
            != Some(EXPECTED_V25_REPORT_SHA256)
        || root.get("l1_payload_sha256").and_then(Value::as_str)
            != Some(EXPECTED_V26_L1_PAYLOAD_SHA256)
        || !root.get("l2").is_some_and(Value::is_null)
        || root
            .pointer("/l1/identity/candidate_key")
            .and_then(Value::as_str)
            != Some(V26_CANDIDATE_KEY)
        || root
            .pointer("/l1/identity/rule_version")
            .and_then(Value::as_str)
            != Some(V26_L1_RULE_VERSION)
        || root
            .pointer("/l1/source_v14_report_sha256")
            .and_then(Value::as_str)
            != Some(super::super::EXPECTED_V14_REPORT_SHA256)
        || root
            .pointer("/l1/dataset_fingerprint_sha256")
            .and_then(Value::as_str)
            != Some(EXPECTED_DATASET_FINGERPRINT_SHA256)
        || root.pointer("/l1/decision/status").and_then(Value::as_str)
            != Some("coverage_pass_ready_for_l2_prereg")
        || root
            .pointer("/l1/decision/outcome_evaluation_performed")
            .and_then(Value::as_bool)
            != Some(false)
    {
        bail!("V26 L2 source V26 L1 identity mismatch");
    }
    let rows = root
        .pointer("/l1/opportunities")
        .and_then(Value::as_array)
        .context("V26 L2 source opportunities missing")?;
    let mut candidate_ids = BTreeSet::new();
    let mut long = 0usize;
    let mut short = 0usize;
    for row in rows
        .iter()
        .filter(|row| row.get("eligible").and_then(Value::as_bool) == Some(true))
    {
        validate_source_row_has_no_outcome(row)?;
        let id = row
            .get("candidate_id")
            .and_then(Value::as_str)
            .context("V26 L2 candidate_id missing")?
            .to_owned();
        let symbol = row
            .get("symbol")
            .and_then(Value::as_str)
            .context("V26 L2 symbol missing")?;
        let direction = row
            .get("direction")
            .and_then(Value::as_str)
            .context("V26 L2 direction missing")?;
        let signal_ts_ms = row
            .get("signal_ts_ms")
            .and_then(Value::as_i64)
            .context("V26 L2 signal_ts_ms missing")?;
        if id != format!("{symbol}:{signal_ts_ms}:{direction}") || !candidate_ids.insert(id) {
            bail!("V26 L2 duplicate or inconsistent candidate identity");
        }
        match direction {
            "long" => long += 1,
            "short" => short += 1,
            _ => bail!("V26 L2 unsupported direction: {direction}"),
        }
    }
    if candidate_ids.len() != EXPECTED_V26_ELIGIBLE
        || long != EXPECTED_V26_LONG
        || short != EXPECTED_V26_SHORT
        || candidate_set_sha256(&candidate_ids) != EXPECTED_V26_ELIGIBLE_SET_SHA256
    {
        bail!("V26 L2 eligible candidate set mismatch");
    }
    Ok(FrozenCandidateSet {
        report_sha256,
        candidate_ids,
    })
}

/// 按冻结排序 ID 从 V14 重建账本中提取候选，缺失即停止而不补造。
fn select_candidates(
    rebuilt: &HashMap<String, V2Candidate>,
    ids: &BTreeSet<String>,
) -> Result<Vec<V2Candidate>> {
    ids.iter()
        .map(|id| {
            rebuilt
                .get(id)
                .cloned()
                .with_context(|| format!("V26 L2 rebuilt candidate missing: {id}"))
        })
        .collect()
}

/// 检查重建账本的多空数量与预注册集合一致。
fn validate_direction_counts(candidates: &[V2Candidate], long: usize, short: usize) -> Result<()> {
    let rebuilt_long = candidates
        .iter()
        .filter(|candidate| candidate.direction == "long")
        .count();
    let rebuilt_short = candidates
        .iter()
        .filter(|candidate| candidate.direction == "short")
        .count();
    if rebuilt_long != long
        || rebuilt_short != short
        || rebuilt_long + rebuilt_short != candidates.len()
    {
        bail!("V26 L2 rebuilt candidate direction counts mismatch");
    }
    Ok(())
}

/// 汇总真实成交的风险宽度和止损发生速度，直接检验“入场 K 止损过多”。
fn risk_diagnostics(report: &V10L2Report) -> Result<V26TradeRiskDiagnostics> {
    let stopped = report
        .trades
        .iter()
        .filter(|trade| matches!(trade.exit_reason, "stop_hit" | "both_hit_stop_first"))
        .collect::<Vec<_>>();
    let entry_bar_stops = stopped
        .iter()
        .filter(|trade| trade.exit_ts_ms == trade.entry_ts_ms)
        .count();
    let stops_within_60m = stopped
        .iter()
        .filter(|trade| trade.exit_ts_ms.saturating_sub(trade.entry_ts_ms) <= ONE_HOUR_MS)
        .count();
    let mut initial_risk_atr = Vec::with_capacity(report.trades.len());
    let mut initial_risk_pct = Vec::with_capacity(report.trades.len());
    let mut initial_stop_cost_r = Vec::with_capacity(report.trades.len());
    let mut realized_cost_r = Vec::with_capacity(report.trades.len());
    for trade in &report.trades {
        let risk = (trade.entry_price - trade.initial_stop_price).abs();
        if risk <= 0.0 || !risk.is_finite() || trade.signal_atr14 <= 0.0 || trade.entry_price <= 0.0
        {
            bail!("V26 L2 invalid executed-trade risk geometry");
        }
        initial_risk_atr.push(risk / trade.signal_atr14);
        initial_risk_pct.push(risk / trade.entry_price * 100.0);
        initial_stop_cost_r.push(
            stop_cost_r_for_prices(trade.entry_price, trade.initial_stop_price, risk)
                .map_err(anyhow::Error::msg)?,
        );
        realized_cost_r.push(trade.cost_r);
    }
    Ok(V26TradeRiskDiagnostics {
        executed_trades: report.trades.len(),
        completed_trades: report.coverage.completed_trades,
        stopped_trades: stopped.len(),
        entry_bar_stops,
        entry_bar_stop_share_pct: ratio_pct(entry_bar_stops, stopped.len()),
        stops_within_60m,
        stops_within_60m_share_pct: ratio_pct(stops_within_60m, stopped.len()),
        initial_risk_atr_distribution: distribution(initial_risk_atr)?,
        initial_risk_pct_distribution: distribution(initial_risk_pct)?,
        initial_stop_cost_r_distribution: distribution(initial_stop_cost_r)?,
        realized_cost_r_distribution: distribution(realized_cost_r)?,
    })
}

/// 计算候选减基线的绩效和快速止损差值。
fn build_delta(
    baseline: &V10L2Report,
    candidate: &V10L2Report,
    baseline_risk: &V26TradeRiskDiagnostics,
    candidate_risk: &V26TradeRiskDiagnostics,
) -> V26L2Delta {
    V26L2Delta {
        completed_trades: candidate.coverage.completed_trades as i64
            - baseline.coverage.completed_trades as i64,
        net_expectancy_r: candidate.net.expectancy_r - baseline.net.expectancy_r,
        net_profit_factor: candidate
            .net
            .profit_factor
            .zip(baseline.net.profit_factor)
            .map(|(candidate, baseline)| candidate - baseline),
        entry_bar_stop_share_pct: candidate_risk.entry_bar_stop_share_pct
            - baseline_risk.entry_bar_stop_share_pct,
        stops_within_60m_share_pct: candidate_risk.stops_within_60m_share_pct
            - baseline_risk.stops_within_60m_share_pct,
    }
}

/// 按 L2 清单联合判断边际、方向、快速止损、覆盖、集中度和合同身份。
fn decide_v26_l2(
    baseline: &V10L2Report,
    candidate: &V10L2Report,
    baseline_risk: &V26TradeRiskDiagnostics,
    candidate_risk: &V26TradeRiskDiagnostics,
    source_candidate_sets_verified: bool,
) -> V26L2Decision {
    let mut gates = BTreeMap::new();
    gates.insert(
        "candidate_cost_adjusted_ev_and_pf_positive",
        profitable(candidate.net.expectancy_r, candidate.net.profit_factor),
    );
    gates.insert(
        "candidate_both_directions_cost_adjusted_positive",
        ["long", "short"].iter().all(|direction| {
            candidate
                .net_by_direction
                .get(direction)
                .is_some_and(|performance| {
                    profitable(performance.expectancy_r, performance.profit_factor)
                })
        }),
    );
    gates.insert(
        "candidate_ev_and_pf_strictly_improve_baseline",
        candidate.net.expectancy_r > baseline.net.expectancy_r
            && candidate
                .net
                .profit_factor
                .zip(baseline.net.profit_factor)
                .is_some_and(|(candidate, baseline)| candidate > baseline),
    );
    gates.insert(
        "entry_bar_stop_share_drops_at_least_10pct_points",
        baseline_risk.entry_bar_stop_share_pct - candidate_risk.entry_bar_stop_share_pct >= 10.0,
    );
    gates.insert(
        "candidate_minimum_coverage",
        candidate.coverage.completed_trades >= 30
            && ["long", "short"].iter().all(|direction| {
                candidate
                    .coverage
                    .completed_by_direction
                    .get(direction)
                    .copied()
                    .unwrap_or_default()
                    >= 10
            })
            && candidate.coverage.completed_symbol_count >= 8
            && candidate.coverage.completed_month_count >= 6
            && candidate.coverage.completed_effective_market_events >= 15,
    );
    gates.insert(
        "candidate_concentration_limits",
        candidate.concentration.net_r_after_removing_top_two_trades > 0.0
            && candidate.concentration.net_r_after_removing_top_event > 0.0
            && candidate
                .concentration
                .max_symbol_positive_r_share_pct
                .is_some_and(|share| share <= 35.0)
            && candidate
                .concentration
                .max_event_positive_r_share_pct
                .is_some_and(|share| share <= 35.0),
    );
    gates.insert(
        "source_sets_and_replay_contracts_verified",
        source_candidate_sets_verified
            && baseline.source_candidate_ledger_verified
            && candidate.source_candidate_ledger_verified
            && baseline.contract_identity_verified
            && candidate.contract_identity_verified,
    );
    let passed = gates.values().all(|passed| *passed);
    V26L2Decision {
        status: if passed {
            "L2_pass_L3_required"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "1ATR 结构止损在冻结成本和退出合同下同时改善边际与入场 K 止损，并通过方向、覆盖、集中度和身份门禁；仍须 L3。".to_owned()
        } else {
            "至少一项预注册的成本后边际、双方向、基线改善、入场 K 止损、覆盖、集中度或身份门禁失败；V26 固定停止在 Research-only。".to_owned()
        },
    }
}

/// 同时要求成本后期望为正且 Profit Factor 大于 1。
fn profitable(expectancy_r: f64, profit_factor: Option<f64>) -> bool {
    expectancy_r > 0.0 && profit_factor.is_some_and(|profit_factor| profit_factor > 1.0)
}

/// 写入唯一 V26 配对 L2 报告，不修改冻结 L1 与任何运行态配置。
fn write_v26_l2_report(output: &Path, report: &V26L2ComparisonReport) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V26 L2 报告目录失败：{}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(report)?;
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V26 L2 报告失败：{}", output.display()))
}
