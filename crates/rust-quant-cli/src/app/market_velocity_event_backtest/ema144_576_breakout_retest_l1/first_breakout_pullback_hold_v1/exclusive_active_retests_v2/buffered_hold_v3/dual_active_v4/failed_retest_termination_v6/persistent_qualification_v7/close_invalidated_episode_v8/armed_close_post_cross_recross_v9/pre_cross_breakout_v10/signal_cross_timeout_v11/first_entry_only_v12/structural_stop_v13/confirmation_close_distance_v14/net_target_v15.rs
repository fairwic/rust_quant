//! V14 冻结形态与结构止损下，把毛 0.52R 目标替换为成本后净 2R 的独立研究版本。
//!
//! L1 只验证入场时已知价格能否形成合法目标，不读取退出、MFE、MAE、盈亏或后续 K 线；
//! L2 才从同一 V14 候选重新执行完整因果回放。

pub mod entry_stop_cost_gate_v16;

use super::super::super::super::super::l2::{
    replay::{replay_verified_candidate_ledger, ReplaySource},
    target_price_for_policy, EntryRiskGatePolicy, InitialRiskPolicy, L2Direction, SetupEntryPolicy,
    TargetRiskPolicy, V10L2Identity, V10L2Report,
};
use super::{
    load_verified_v14_replay_input, sha256_hex, validate_v14_l1_source, V14_L1_RULE_VERSION,
};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// V15 独立候选身份；退出目标变化不得覆盖 V14。
pub const V15_CANDIDATE_KEY: &str =
    "market_momentum_ema576_first_entry_ema144_structural_stop_close_distance_net_target_2r_15m_v15";
/// V15 L1 只检查成本反解目标的价格几何和样本覆盖。
pub const V15_L1_RULE_VERSION: &str =
    "l1_v15_v14_net_target200_after_cost8bps_geometry_no_outcome_v1";
/// V15 L2 仅替换目标政策，其余成交、止损、持仓与冲突政策保持 V14。
pub const V15_L2_RULE_VERSION: &str = "l2_v15_v14_structural030_net_target200_after_cost8bps_v1";

const EXPECTED_V14_REPORT_SHA256: &str =
    "099827aa7924f2e754fa5926e6b50299eaedfc03aa9c5ddc284ce2bf4fed51c1";
const EXPECTED_V14_L1_REPORT_SHA256: &str =
    "23edf396fe1b82681789c9323d38a30b17e1defa03d9d8b4551ac2f059e7b475";
const EXPECTED_BASELINE_PLANS: usize = 2_651;
const STRUCTURAL_STOP_BUFFER_ATR: f64 = 0.30;
const NET_TARGET_R: f64 = 2.00;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const NET_R_TOLERANCE: f64 = 1e-9;
const ONE_INCH_SIGNAL_TS_MS: i64 = 1_784_460_600_000;
const UMA_SIGNAL_TS_MS: i64 = 1_784_463_300_000;
const XRP_SIGNAL_TS_MS: i64 = 1_784_466_000_000;

/// V15 L1 单变量、因果字段边界和运行隔离身份。
#[derive(Debug, Clone, Serialize)]
pub struct V15L1Identity {
    /// 当前只处于无 outcome 快速研究。
    pub level: &'static str,
    /// 与 V14 并存的独立候选键。
    pub candidate_key: &'static str,
    /// 成本后净 2R 价格反解规则。
    pub rule_version: &'static str,
    /// 本轮唯一允许变化的政策。
    pub only_variable: &'static str,
    /// L1 实际读取的信号与入场时字段。
    pub causal_field_boundary: &'static str,
    /// L1 明确不读取的结果字段。
    pub label_boundary: &'static str,
    /// 与 Paper、Live 和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V15 L1 对冻结 V14 实际计划的覆盖统计。
#[derive(Debug, Clone, Serialize)]
pub struct V15L1Coverage {
    /// V14 冻结基线中的实际计划数。
    pub baseline_plan_count: usize,
    /// 能生成有限、正数且方向正确目标的计划数。
    pub valid_target_plan_count: usize,
    /// 目标价格或结构风险不合法的计划数。
    pub invalid_target_plan_count: usize,
    /// 数学上目标政策由毛 0.52R 改为净 2R 的计划数。
    pub policy_changed_plan_count: usize,
    /// 多空计划数量。
    pub by_direction: BTreeMap<String, usize>,
    /// 覆盖交易对数量。
    pub symbol_count: usize,
    /// 覆盖北京时间自然月数量。
    pub month_count_bjt: usize,
    /// 按方向和一小时连续触发链归并的事件数。
    pub effective_market_events: usize,
}

/// 一个冻结计划在净 2R 政策下的目标价格几何。
#[derive(Debug, Clone, PartialEq)]
struct TargetGeometry {
    candidate_id: String,
    symbol: String,
    direction: &'static str,
    setup_ts_ms: i64,
    signal_ts_ms: i64,
    entry_price: f64,
    signal_ema144: f64,
    signal_atr14: f64,
    initial_stop_price: f64,
    initial_risk: f64,
    target_price: f64,
    gross_target_r: f64,
    target_cost_r: f64,
    net_target_r: f64,
    target_distance_pct: f64,
    target_distance_atr: f64,
}

/// 一个数值维度的冻结离散分布。
#[derive(Debug, Clone, Serialize)]
pub struct V15Distribution {
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

/// 用户指定样本的入场时目标计算证据。
#[derive(Debug, Clone, Serialize)]
pub struct V15NamedSample {
    /// XRP、1INCH 或 UMA。
    pub sample: &'static str,
    /// 稳定候选身份。
    pub candidate_id: String,
    /// OKX USDT 永续交易对。
    pub symbol: String,
    /// 多空方向。
    pub direction: &'static str,
    /// setup 完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 信号完成时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 信号完成时间，北京时间。
    pub signal_time_bjt: String,
    /// 下一根连续 15m 开盘价。
    pub entry_price: f64,
    /// 信号时 EMA144。
    pub signal_ema144: f64,
    /// 信号时 ATR14。
    pub signal_atr14: f64,
    /// 入场时冻结的结构止损价。
    pub initial_stop_price: f64,
    /// 入场到结构止损的初始价格风险。
    pub initial_risk: f64,
    /// 成本后净 2R 对应目标价。
    pub target_price: f64,
    /// 目标价对应的成本前 R。
    pub gross_target_r: f64,
    /// 开平双边 8bps 折算的目标成本 R。
    pub target_cost_r: f64,
    /// 目标成交后的净 R。
    pub net_target_r: f64,
    /// 目标距入场百分比。
    pub target_distance_pct: f64,
    /// 目标距入场的 ATR14 倍数。
    pub target_distance_atr: f64,
}

/// V15 是否允许从目标几何检查进入一次冻结 L2。
#[derive(Debug, Clone, Serialize)]
pub struct V15L1Decision {
    /// coverage_pass_ready_for_l2_prereg 或 stop。
    pub status: &'static str,
    /// 每项预注册无标签门禁。
    pub gates: BTreeMap<&'static str, bool>,
    /// L1 必须固定为 false。
    pub outcome_evaluation_performed: bool,
    /// 停止或进入 L2 的原因。
    pub reason: String,
}

/// V15 L1 成本反解目标的完整无 outcome 机器结果。
#[derive(Debug, Clone, Serialize)]
pub struct V15L1Report {
    /// V15 L1 JSON 字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 单变量、因果与运行隔离身份。
    pub identity: V15L1Identity,
    /// 冻结 V14 合并机器报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 基线计划覆盖。
    pub coverage: V15L1Coverage,
    /// 成本前目标 R 分布。
    pub gross_target_r_distribution: V15Distribution,
    /// 目标距入场百分比分布。
    pub target_distance_pct_distribution: V15Distribution,
    /// 目标距入场 ATR14 分布。
    pub target_distance_atr_distribution: V15Distribution,
    /// 全部计划目标成交时净 R 的最大绝对误差。
    pub max_abs_net_target_error_r: f64,
    /// XRP、1INCH、UMA 的指定目标样本。
    pub named_samples: Vec<V15NamedSample>,
    /// 无标签覆盖结论。
    pub decision: V15L1Decision,
}

/// V15 单一机器产物，组合 L1 目标几何与唯一一次 L2 回放。
#[derive(Debug, Clone, Serialize)]
pub struct V15MachineReport {
    /// V15 合并机器结果字段合同版本。
    pub schema_version: &'static str,
    /// 合并报告生成时间，UTC。
    pub generated_at_utc: String,
    /// 冻结 V14 合并报告 SHA-256。
    pub source_v14_report_sha256: String,
    /// 入场时可见字段生成的净 2R 目标几何。
    pub l1: V15L1Report,
    /// 使用同一 V14 候选重新回放后的实际交易结果。
    pub l2: V10L2Report,
}

/// 校验冻结 V14 报告，执行 V15 L1 几何检查和唯一一次 L2 因果回放。
pub async fn run_v15_l1_l2_replay(v14_source: &Path, output: &Path) -> Result<V15MachineReport> {
    let bytes = std::fs::read(v14_source)
        .with_context(|| format!("读取冻结 V14 合并报告失败：{}", v14_source.display()))?;
    let source_sha256 = sha256_hex(&bytes);
    if source_sha256 != EXPECTED_V14_REPORT_SHA256 {
        bail!("V15 source V14 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析冻结 V14 合并报告失败")?;
    if source.pointer("/schema_version").and_then(Value::as_str)
        != Some("market_momentum_ema576_confirmation_close_distance_l1_l2_v14")
    {
        bail!("V15 source V14 schema mismatch");
    }
    let v14_l1 = source
        .pointer("/l1")
        .cloned()
        .context("V15 source V14 L1 missing")?;
    validate_v14_l1_source(&v14_l1)?;
    let l1 = build_v15_l1_report(&source, source_sha256.clone())?;
    if l1.decision.status != "coverage_pass_ready_for_l2_prereg" {
        bail!("V15 L1 target geometry gate failed");
    }

    let replay_input = load_verified_v14_replay_input(&v14_l1).await?;
    let l2 = replay_verified_candidate_ledger(
        &replay_input.data,
        ReplaySource::new(
            "market_momentum_ema576_net_target_2r_l2_v15",
            V10L2Identity {
                level: "L2_local_multi_symbol_diagnostic",
                candidate_key: V15_CANDIDATE_KEY,
                source_l1_rule_version: V14_L1_RULE_VERSION,
                rule_version: V15_L2_RULE_VERSION,
                only_variable: "replace V14 fixed gross 0.52R target with a price-inverted target that settles to net 2.00R after the unchanged 8bps per-side cost",
                entry_policy: "unchanged V14 next-contiguous-15m-open execution; distance rejection does not consume the setup and the first later qualifying real fill does",
                initial_stop_policy: "unchanged V14 long signal EMA144 minus 0.30 ATR14 and short signal EMA144 plus 0.30 ATR14; freeze at entry and never loosen",
                target_policy: "freeze a cost-adjusted target at entry so target execution settles to net 2.00R after 8bps per side; no break-even, trailing, partial, runner, or reversal",
                intrabar_conflict_policy: "unchanged entry candle inclusion and stop-first ordering when stop and target are both touched in one candle",
                symbol_position_policy: "unchanged one open trade per symbol and one real fill per symbol x direction x setup_ts",
                per_side_cost_rate: PER_SIDE_COST_RATE,
                max_holding_ms: MAX_HOLDING_MS,
                funding_modeled: false,
                outcome_evaluation_performed: true,
                runtime_boundary: "research-only V15 L2; not registered in paper, readonly shadow, live worker, compose, database, scheduler, or production presets",
            },
            EXPECTED_V14_L1_REPORT_SHA256.to_owned(),
            replay_input.dataset_fingerprint_sha256,
            replay_input.returned_symbol_count,
            replay_input.eligible_symbol_count,
            replay_input.excluded_symbol_count,
            SetupEntryPolicy::FirstFilledPerSetup,
            InitialRiskPolicy::SignalEma144AtrBuffer(STRUCTURAL_STOP_BUFFER_ATR),
            TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R),
            EntryRiskGatePolicy::AllowAnyPositiveRisk,
            replay_input.candidates,
        ),
    );
    let report = V15MachineReport {
        schema_version: "market_momentum_ema576_net_target_2r_l1_l2_v15",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        source_v14_report_sha256: source_sha256,
        l1,
        l2,
    };
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V15 机器报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V15 机器报告失败：{}", output.display()))?;
    Ok(report)
}

/// 只从 V14 基线计划提取入场时字段，汇总成本反解目标的覆盖和分布。
fn build_v15_l1_report(source: &Value, source_sha256: String) -> Result<V15L1Report> {
    let plans = source
        .pointer("/l2/trades")
        .and_then(Value::as_array)
        .context("V15 source V14 baseline plans missing")?;
    let mut geometry = Vec::with_capacity(plans.len());
    let mut invalid_target_plan_count = 0;
    for plan in plans {
        match target_geometry(plan) {
            Ok(item) => geometry.push(item),
            Err(_) => invalid_target_plan_count += 1,
        }
    }
    let mut by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    let mut gross_target_r = Vec::with_capacity(geometry.len());
    let mut target_distance_pct = Vec::with_capacity(geometry.len());
    let mut target_distance_atr = Vec::with_capacity(geometry.len());
    let mut max_abs_net_target_error_r = 0.0_f64;
    for item in &geometry {
        *by_direction.entry(item.direction.to_owned()).or_default() += 1;
        symbols.insert(item.symbol.clone());
        months.insert(month_bjt(item.signal_ts_ms)?);
        gross_target_r.push(item.gross_target_r);
        target_distance_pct.push(item.target_distance_pct);
        target_distance_atr.push(item.target_distance_atr);
        max_abs_net_target_error_r =
            max_abs_net_target_error_r.max((item.net_target_r - NET_TARGET_R).abs());
    }
    let named_samples = [
        ("1INCH", "1INCH-USDT-SWAP", ONE_INCH_SIGNAL_TS_MS),
        ("UMA", "UMA-USDT-SWAP", UMA_SIGNAL_TS_MS),
        ("XRP", "XRP-USDT-SWAP", XRP_SIGNAL_TS_MS),
    ]
    .into_iter()
    .map(|(sample, symbol, signal_ts_ms)| {
        let item = geometry
            .iter()
            .find(|item| item.symbol == symbol && item.signal_ts_ms == signal_ts_ms)
            .with_context(|| format!("V15 named sample missing: {sample}"))?;
        named_sample(sample, item)
    })
    .collect::<Result<Vec<_>>>()?;
    let effective_market_events = effective_event_count(&geometry);
    let coverage = V15L1Coverage {
        baseline_plan_count: plans.len(),
        valid_target_plan_count: geometry.len(),
        invalid_target_plan_count,
        policy_changed_plan_count: geometry.len(),
        by_direction,
        symbol_count: symbols.len(),
        month_count_bjt: months.len(),
        effective_market_events,
    };
    let mut gates = BTreeMap::new();
    gates.insert(
        "baseline_plan_count_matches",
        coverage.baseline_plan_count == EXPECTED_BASELINE_PLANS,
    );
    gates.insert(
        "all_targets_are_finite_positive_and_directional",
        coverage.valid_target_plan_count == coverage.baseline_plan_count
            && coverage.invalid_target_plan_count == 0,
    );
    gates.insert(
        "all_baseline_plans_use_changed_target_policy",
        coverage.policy_changed_plan_count == coverage.baseline_plan_count,
    );
    gates.insert(
        "both_directions_at_least_10",
        ["long", "short"].iter().all(|direction| {
            coverage
                .by_direction
                .get(*direction)
                .copied()
                .unwrap_or_default()
                >= 10
        }),
    );
    gates.insert("symbols_at_least_8", coverage.symbol_count >= 8);
    gates.insert("months_at_least_6", coverage.month_count_bjt >= 6);
    gates.insert("plans_at_least_30", coverage.baseline_plan_count >= 30);
    gates.insert(
        "named_samples_all_settle_to_net_2r",
        named_samples
            .iter()
            .all(|sample| (sample.net_target_r - NET_TARGET_R).abs() <= NET_R_TOLERANCE),
    );
    gates.insert(
        "max_net_target_error_within_tolerance",
        max_abs_net_target_error_r <= NET_R_TOLERANCE,
    );
    let passed = gates.values().all(|passed| *passed);

    Ok(V15L1Report {
        schema_version: "market_momentum_ema576_net_target_2r_l1_v15",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V15L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V15_CANDIDATE_KEY,
            rule_version: V15_L1_RULE_VERSION,
            only_variable: "replace the V14 fixed gross 0.52R target with a target that settles to net 2.00R after the frozen 8bps per-side cost",
            causal_field_boundary: "candidate id, symbol, direction, setup and signal time, entry price, signal EMA144, signal ATR14, and frozen initial stop only",
            label_boundary: "no complete flag, exit time, exit price, exit reason, gross R, cost R, net R, MFE, MAE, PnL, win, loss, or later candle is read",
            runtime_boundary: "research-only V15 L1; no Pine, paper, readonly shadow, live worker, database, scheduler, compose, or production registration",
        },
        source_v14_report_sha256: source_sha256,
        coverage,
        gross_target_r_distribution: distribution(&mut gross_target_r)?,
        target_distance_pct_distribution: distribution(&mut target_distance_pct)?,
        target_distance_atr_distribution: distribution(&mut target_distance_atr)?,
        max_abs_net_target_error_r,
        named_samples,
        decision: V15L1Decision {
            status: if passed {
                "coverage_pass_ready_for_l2_prereg"
            } else {
                "stop"
            },
            gates,
            outcome_evaluation_performed: false,
            reason: if passed {
                "全部冻结基线计划和三个指定样本都能仅用入场时字段生成合法目标，且按同一成本模型精确结算为净 2R；允许执行一次预注册 L2。".to_owned()
            } else {
                "至少一个价格合法性、覆盖、指定样本或净 R 精度门禁失败；停止在 L1。".to_owned()
            },
        },
    })
}

/// 从一个带 outcome 的 V14 交易对象中只提取预注册允许的入场时字段。
fn target_geometry(plan: &Value) -> Result<TargetGeometry> {
    let candidate_id = plan_string(plan, "candidate_id")?.to_owned();
    let symbol = plan_string(plan, "symbol")?.to_owned();
    let direction_label = plan_string(plan, "direction")?;
    let direction = match direction_label {
        "long" => L2Direction::Long,
        "short" => L2Direction::Short,
        _ => bail!("V15 plan direction invalid"),
    };
    let setup_ts_ms = plan_i64(plan, "setup_ts_ms")?;
    let signal_ts_ms = plan_i64(plan, "signal_ts_ms")?;
    let entry_price = plan_f64(plan, "entry_price")?;
    let signal_ema144 = plan_f64(plan, "signal_ema144")?;
    let signal_atr14 = plan_f64(plan, "signal_atr14")?;
    let initial_stop_price = plan_f64(plan, "initial_stop_price")?;
    if entry_price <= 0.0
        || signal_ema144 <= 0.0
        || signal_atr14 <= 0.0
        || initial_stop_price <= 0.0
    {
        bail!("V15 plan price or ATR is not positive");
    }
    let expected_stop = match direction {
        L2Direction::Long => signal_ema144 - STRUCTURAL_STOP_BUFFER_ATR * signal_atr14,
        L2Direction::Short => signal_ema144 + STRUCTURAL_STOP_BUFFER_ATR * signal_atr14,
    };
    if (expected_stop - initial_stop_price).abs() > 1e-10_f64.max(expected_stop.abs() * 1e-10) {
        bail!("V15 source structural stop contract mismatch");
    }
    let initial_risk = match direction {
        L2Direction::Long => entry_price - initial_stop_price,
        L2Direction::Short => initial_stop_price - entry_price,
    };
    if !initial_risk.is_finite() || initial_risk <= 0.0 {
        bail!("V15 plan initial risk invalid");
    }
    let target_price = target_price_for_policy(
        entry_price,
        initial_risk,
        direction,
        TargetRiskPolicy::NetAfterCostsR(NET_TARGET_R),
    )
    .map_err(|reason| anyhow::anyhow!(reason))?;
    let directional_move = match direction {
        L2Direction::Long => target_price - entry_price,
        L2Direction::Short => entry_price - target_price,
    };
    if directional_move <= 0.0 || !directional_move.is_finite() {
        bail!("V15 target is not beyond entry in trade direction");
    }
    let gross_target_r = directional_move / initial_risk;
    let target_cost_r = (entry_price + target_price) * PER_SIDE_COST_RATE / initial_risk;
    let net_target_r = gross_target_r - target_cost_r;
    Ok(TargetGeometry {
        candidate_id,
        symbol,
        direction: match direction {
            L2Direction::Long => "long",
            L2Direction::Short => "short",
        },
        setup_ts_ms,
        signal_ts_ms,
        entry_price,
        signal_ema144,
        signal_atr14,
        initial_stop_price,
        initial_risk,
        target_price,
        gross_target_r,
        target_cost_r,
        net_target_r,
        target_distance_pct: directional_move / entry_price * 100.0,
        target_distance_atr: directional_move / signal_atr14,
    })
}

/// 把内部目标几何投影为用户指定样本的可审计机器字段。
fn named_sample(sample: &'static str, item: &TargetGeometry) -> Result<V15NamedSample> {
    Ok(V15NamedSample {
        sample,
        candidate_id: item.candidate_id.clone(),
        symbol: item.symbol.clone(),
        direction: item.direction,
        setup_ts_ms: item.setup_ts_ms,
        signal_ts_ms: item.signal_ts_ms,
        signal_time_bjt: format_bjt(item.signal_ts_ms)?,
        entry_price: item.entry_price,
        signal_ema144: item.signal_ema144,
        signal_atr14: item.signal_atr14,
        initial_stop_price: item.initial_stop_price,
        initial_risk: item.initial_risk,
        target_price: item.target_price,
        gross_target_r: item.gross_target_r,
        target_cost_r: item.target_cost_r,
        net_target_r: item.net_target_r,
        target_distance_pct: item.target_distance_pct,
        target_distance_atr: item.target_distance_atr,
    })
}

/// 生成最小、P10、中位、P90 与最大值的冻结离散分布。
fn distribution(values: &mut [f64]) -> Result<V15Distribution> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        bail!("V15 distribution is empty or non-finite");
    }
    values.sort_by(f64::total_cmp);
    Ok(V15Distribution {
        count: values.len(),
        min: values[0],
        p10: quantile(values, 0.10),
        median: quantile(values, 0.50),
        p90: quantile(values, 0.90),
        max: values[values.len() - 1],
    })
}

/// 返回 floor((n-1)*p) 对应的冻结样本值。
fn quantile(sorted: &[f64], probability: f64) -> f64 {
    let index = ((sorted.len().saturating_sub(1)) as f64 * probability).floor() as usize;
    sorted[index]
}

/// 使用信号时间和方向重建一小时连续事件链，不读取既有退出结果或事件字段。
fn effective_event_count(plans: &[TargetGeometry]) -> usize {
    let mut ordered = plans
        .iter()
        .map(|plan| (plan.signal_ts_ms, plan.direction))
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
        .context("V15 signal timestamp invalid")?;
    Ok(format!("{:04}-{:02}", datetime.year(), datetime.month()))
}

/// 把 Unix 毫秒转换为秒精度北京时间。
fn format_bjt(ts_ms: i64) -> Result<String> {
    let datetime = chrono::FixedOffset::east_opt(8 * 60 * 60)
        .context("construct UTC+8 offset")?
        .timestamp_millis_opt(ts_ms)
        .single()
        .context("V15 signal timestamp invalid")?;
    Ok(datetime.format("%Y-%m-%d %H:%M:%S %:z").to_string())
}

/// 读取计划对象中的必需字符串字段。
fn plan_string<'a>(plan: &'a Value, field: &str) -> Result<&'a str> {
    plan.get(field)
        .and_then(Value::as_str)
        .with_context(|| format!("V15 plan string field missing: {field}"))
}

/// 读取计划对象中的必需时间字段。
fn plan_i64(plan: &Value, field: &str) -> Result<i64> {
    plan.get(field)
        .and_then(Value::as_i64)
        .with_context(|| format!("V15 plan i64 field missing: {field}"))
}

/// 读取计划对象中的必需有限浮点字段。
fn plan_f64(plan: &Value, field: &str) -> Result<f64> {
    let value = plan
        .get(field)
        .and_then(Value::as_f64)
        .with_context(|| format!("V15 plan f64 field missing: {field}"))?;
    if !value.is_finite() {
        bail!("V15 plan f64 field is not finite: {field}");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 构造只含 V15 因果字段、另附可任意污染 outcome 的计划。
    fn plan(direction: &str) -> Value {
        let (entry, ema144, stop) = if direction == "long" {
            (101.0, 100.0, 99.4)
        } else {
            (99.0, 100.0, 100.6)
        };
        json!({
            "candidate_id": format!("TEST:0:{direction}"),
            "symbol": "TEST-USDT-SWAP",
            "direction": direction,
            "setup_ts_ms": 0,
            "signal_ts_ms": 900_000,
            "entry_price": entry,
            "signal_ema144": ema144,
            "signal_atr14": 2.0,
            "initial_stop_price": stop,
            "complete": true,
            "exit_price": 999_999.0,
            "exit_reason": "poisoned",
            "gross_r": -999.0,
            "cost_r": 888.0,
            "net_r": -1_887.0
        })
    }

    #[test]
    fn target_geometry_settles_long_and_short_to_net_two_r() {
        for direction in ["long", "short"] {
            let geometry = target_geometry(&plan(direction)).expect("target geometry");
            assert!((geometry.net_target_r - NET_TARGET_R).abs() <= NET_R_TOLERANCE);
            assert!(geometry.target_distance_pct > 0.0);
            assert!(geometry.target_distance_atr > 0.0);
        }
    }

    #[test]
    fn target_geometry_is_insensitive_to_outcome_fields() {
        let mut source = plan("long");
        let before = target_geometry(&source).expect("before outcome mutation");
        source["complete"] = json!(false);
        source["exit_price"] = json!(0.000_001);
        source["exit_reason"] = json!("another_poison");
        source["gross_r"] = json!(123_456.0);
        source["cost_r"] = json!(-654_321.0);
        source["net_r"] = json!(777_777.0);
        let after = target_geometry(&source).expect("after outcome mutation");
        assert_eq!(before, after);
    }
}
