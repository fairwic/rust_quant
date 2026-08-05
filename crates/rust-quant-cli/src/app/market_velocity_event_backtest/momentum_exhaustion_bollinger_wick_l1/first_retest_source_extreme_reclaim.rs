//! 首次重测收回来源信号极值的 L1 无标签账本转换。
//!
//! 本模块只消费已经冻结的首次重测 L1 账本。它不会重新查询行情，也不会读取确认后开盘价、
//! 持仓路径或盈亏；唯一变化是把确认边界从动态布林外轨换成来源 setup 的方向极值。

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

/// 独立研究候选键；入场确认语义改变后不得覆盖前一版本。
pub const SOURCE_EXTREME_RECLAIM_CANDIDATE_KEY: &str =
    "market_momentum_bollinger_wick_source_extreme_reclaim_15m_v1";
/// 唯一规则版本：首次重测严格收回来源极值，下一根才允许激活。
pub const SOURCE_EXTREME_RECLAIM_RULE_VERSION: &str =
    "l1_first_retest_close_back_through_source_extreme_next_open_v1";

const SOURCE_REPORT_SCHEMA_VERSION: &str = "momentum_bollinger_first_retest_reentry_l1_v1";
const SOURCE_CANDIDATE_KEY: &str = "market_momentum_bollinger_wick_reentry_confirmation_15m_v1";
const SOURCE_RULE_VERSION: &str = "l1_first_retest_close_inside_bb20x2p5_next_open_v1";
const SOURCE_SETUP_CANDIDATE_KEY: &str = "market_momentum_bollinger_wick_reversion_15m_v1";
const SOURCE_SETUP_RULE_VERSION: &str = "l1_filtered_vol2p5_net96x8_wick60_bb20x2p5_outer_touch_v1";
const EXPECTED_SOURCE_REPORT_SHA256: &str =
    "d087732b342039bb5dde635da64ba8c02639ac741591c35ffbc3a35cdd5a5e51";
const EXPECTED_SOURCE_CANDIDATE_LEDGER_SHA256: &str =
    "5eea47aa2a945daffe3d450b568bd9e7a1ada497424cb91ec1e13e363a39ca6d";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "0c3d1e6ce33187fbc0fd528486d837574fe176b73a748b1f44dedd3c14c328f5";
const EXPECTED_BASE_TOUCH_SETUPS: usize = 673;
const EXPECTED_FIRST_RETEST_SETUPS: usize = 283;
const MS_15M: i64 = 15 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const TARGET_SAMPLES: [(&str, i64); 10] = [
    ("AGLD-USDT-SWAP", 1_783_444_500_000),
    ("YFI-USDT-SWAP", 1_783_328_400_000),
    ("NMR-USDT-SWAP", 1_782_962_100_000),
    ("ORDI-USDT-SWAP", 1_782_738_900_000),
    ("SATS-USDT-SWAP", 1_782_647_100_000),
    ("WIF-USDT-SWAP", 1_782_522_900_000),
    ("EIGEN-USDT-SWAP", 1_781_521_200_000),
    ("BABY-USDT-SWAP", 1_781_520_300_000),
    ("OP-USDT-SWAP", 1_781_332_200_000),
    ("CVX-USDT-SWAP", 1_781_216_100_000),
];

/// 来源机器报告中需要校验的策略身份。
#[derive(Debug, Deserialize)]
struct SourceIdentity {
    candidate_key: String,
    rule_version: String,
    source_candidate_key: String,
    source_rule_version: String,
}

/// 来源机器报告中需要保持一致的覆盖统计。
#[derive(Debug, Deserialize)]
struct SourceSummary {
    base_touch_setups: usize,
    first_retest_setups: usize,
}

/// 来源 L1 必须明确没有执行结果评估。
#[derive(Debug, Deserialize)]
struct SourceDecision {
    outcome_evaluation_performed: bool,
}

/// 来源首次重测候选；字段最晚在重测 K 收盘时可见。
#[derive(Debug, Clone, Deserialize)]
struct SourceCandidate {
    symbol: String,
    setup_ts_ms: i64,
    setup_month_utc: String,
    direction: String,
    source_trigger: String,
    source_extreme_price: f64,
    filtered_volume_ratio: f64,
    prior_96_net_move_pct: f64,
    directional_wick_range_ratio: f64,
    first_retest_ts_ms: Option<i64>,
    first_retest_offset_bars: Option<usize>,
    first_retest_close: Option<f64>,
    retest_bollinger_middle: Option<f64>,
    retest_bollinger_upper: Option<f64>,
    retest_bollinger_lower: Option<f64>,
    close_inside_directional_band: Option<bool>,
    close_beyond_source_extreme: Option<bool>,
    status: String,
}

/// 冻结来源报告；未声明的生成时间与展示字段不会参与新规则。
#[derive(Debug, Deserialize)]
struct SourceReport {
    schema_version: String,
    identity: SourceIdentity,
    source_candidate_ledger_sha256: String,
    coverage: Value,
    summary: SourceSummary,
    decision: SourceDecision,
    candidates: Vec<SourceCandidate>,
}

/// 新候选的研究身份和严格因果边界。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimIdentity {
    /// 当前阶段；L1 禁止使用结果标签。
    pub level: &'static str,
    /// 入场确认语义改变后的独立候选键。
    pub candidate_key: &'static str,
    /// 首次重测收回来源极值的规则版本。
    pub rule_version: &'static str,
    /// 直接输入的动态布林确认候选键。
    pub source_candidate_key: &'static str,
    /// 直接输入的动态布林确认规则版本。
    pub source_rule_version: &'static str,
    /// 本批只允许变化的一个判断边界。
    pub only_variable: &'static str,
    /// 做空与做多的严格镜像规则。
    pub close_confirmation_policy: &'static str,
    /// 首次重测失败后的终止规则。
    pub first_retest_policy: &'static str,
    /// 确认后最早允许激活的时间。
    pub activation_policy: &'static str,
    /// 明确禁止读取的后验字段。
    pub label_boundary: &'static str,
}

/// 输入账本的不可变身份，防止把其他窗口或策略结果混入比较。
#[derive(Debug, Clone, Serialize)]
pub struct SourceEvidence {
    /// 输入机器报告原始字节的 SHA-256。
    pub source_report_sha256: String,
    /// 输入报告记录的来源 setup 候选账本 SHA-256。
    pub source_candidate_ledger_sha256: String,
    /// 输入覆盖中的行情数据指纹。
    pub dataset_fingerprint_sha256: String,
    /// 输入候选字段已检查且没有后验结果标签。
    pub candidate_schema_no_outcome_fields: bool,
}

/// 单条来源 setup 在新确认边界下的无标签终态。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimCandidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 来源 setup K 开始时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 来源 setup 的 UTC 月份。
    pub setup_month_utc: String,
    /// `long` 或 `short`，完全继承来源方向。
    pub direction: String,
    /// 来源 V2 长影触发标签。
    pub source_trigger: String,
    /// 冻结确认边界；做空为 setup high，做多为 setup low。
    pub source_extreme_price: f64,
    /// 来源信号时过滤量比，仅供审计。
    pub filtered_volume_ratio: f64,
    /// 来源信号前 96 根有符号净移动百分比。
    pub prior_96_net_move_pct: f64,
    /// 来源方向影线占完整振幅比例。
    pub directional_wick_range_ratio: f64,
    /// 12 根内首次触及来源极值的 K 线时间。
    pub first_retest_ts_ms: Option<i64>,
    /// 首次重测距 setup 的 K 线根数。
    pub first_retest_offset_bars: Option<usize>,
    /// 首次重测完成后的收盘价。
    pub first_retest_close: Option<f64>,
    /// 首次重测 K 完成后的动态布林中轨，仅作对照。
    pub retest_bollinger_middle: Option<f64>,
    /// 首次重测 K 完成后的动态布林上轨，仅作对照。
    pub retest_bollinger_upper: Option<f64>,
    /// 首次重测 K 完成后的动态布林下轨，仅作对照。
    pub retest_bollinger_lower: Option<f64>,
    /// 来源动态布林规则是否确认，仅作分类对照。
    pub source_dynamic_band_confirmed: Option<bool>,
    /// 首次重测收盘是否严格回到来源方向极值以内。
    pub close_back_through_source_extreme: Option<bool>,
    /// 新确认信号时间；只有严格收回来源极值时存在。
    pub confirmation_signal_ts_ms: Option<i64>,
    /// 下一根 15m K 的开始时间；L1 不读取该 K 的价格。
    pub earliest_entry_ts_ms: Option<i64>,
    /// 新边界下的确认、拒绝或未重测终态。
    pub status: String,
}

/// 动态布林边界与冻结来源极值边界的无标签分类迁移。
#[derive(Debug, Clone, Default, Serialize)]
pub struct BoundaryComparison {
    /// 两种边界都确认的首次重测数。
    pub dynamic_confirmed_source_extreme_confirmed: usize,
    /// 动态布林确认但来源极值拒绝的首次重测数。
    pub dynamic_confirmed_source_extreme_rejected: usize,
    /// 动态布林拒绝但来源极值确认的首次重测数。
    pub dynamic_rejected_source_extreme_confirmed: usize,
    /// 两种边界都拒绝的首次重测数。
    pub dynamic_rejected_source_extreme_rejected: usize,
}

/// 固定十笔诊断样本在新定义下的命中结果。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeTargetAudit {
    /// 固定目标交易对。
    pub symbol: &'static str,
    /// 固定来源 setup 时间。
    pub setup_ts_ms: i64,
    /// 是否找到完全一致的来源候选。
    pub source_found: bool,
    /// 是否存在冻结账本中的首次重测。
    pub first_retest_ts_ms: Option<i64>,
    /// 首次重测收盘价。
    pub first_retest_close: Option<f64>,
    /// 来源方向极值。
    pub source_extreme_price: Option<f64>,
    /// 原动态布林边界下的终态。
    pub source_dynamic_band_status: Option<String>,
    /// 新来源极值边界下的终态。
    pub status: Option<String>,
    /// 是否因没有严格收回来源极值而拒绝。
    pub rejected_by_source_extreme: bool,
}

/// 新边界的覆盖、方向、分散性和固定目标统计。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimSummary {
    /// 来源外轨长影 setup 总数。
    pub base_touch_setups: usize,
    /// 12 根内发生首次重测的 setup 数。
    pub first_retest_setups: usize,
    /// 严格收回来源极值并确认的 setup 数。
    pub confirmed_setups: usize,
    /// 未严格收回来源极值并拒绝的 setup 数。
    pub rejected_source_extreme_setups: usize,
    /// 确认数占首次重测数的比例。
    pub confirmation_retention_pct_of_first_retests: f64,
    /// 拒绝数占首次重测数的比例。
    pub rejection_impact_pct_of_first_retests: f64,
    /// 确认信号的多空分布。
    pub confirmed_by_direction: BTreeMap<String, usize>,
    /// 拒绝信号的多空分布。
    pub rejected_by_direction: BTreeMap<String, usize>,
    /// 确认覆盖币种数。
    pub confirmed_symbol_count: usize,
    /// 确认覆盖 UTC 月份数。
    pub confirmed_month_count: usize,
    /// 按确认时间、方向和一小时窗口归并的有效事件数。
    pub confirmed_effective_market_events: usize,
    /// 未发生首次重测的来源终态计数。
    pub blockers: BTreeMap<String, usize>,
    /// 十个固定目标中被新边界拒绝的数量。
    pub target_rejected_count: usize,
}

/// 预注册 L1 门禁结论。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimDecision {
    /// `stop` 或 `coverage_pass_l2_ready`。
    pub status: &'static str,
    /// 每项冻结门槛的真假结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或允许进入 L2 的边界说明。
    pub reason: String,
    /// L1 必须始终为 `false`。
    pub outcome_evaluation_performed: bool,
}

/// 读取、校验、转换与序列化的耗时，单位毫秒。
#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceExtremePhaseTimingsMs {
    /// 读取并哈希冻结输入报告所用毫秒数。
    pub source_read_and_verify: u128,
    /// 转换候选并计算无标签覆盖所用毫秒数。
    pub ledger_transform_and_summary: u128,
}

/// 首次重测收回来源极值的完整 L1 机器报告。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimReport {
    /// 报告字段 schema 版本。
    pub schema_version: &'static str,
    /// 报告生成时间，不参与策略身份。
    pub generated_at_utc: String,
    /// 新候选与因果边界。
    pub identity: SourceExtremeReclaimIdentity,
    /// 冻结输入报告的校验证据。
    pub source_evidence: SourceEvidence,
    /// 原报告覆盖信息原样保留。
    pub coverage: Value,
    /// 阶段耗时。
    pub phase_timings_ms: SourceExtremePhaseTimingsMs,
    /// 新候选账本的稳定 SHA-256。
    pub candidate_ledger_sha256: String,
    /// 新边界覆盖汇总。
    pub summary: SourceExtremeReclaimSummary,
    /// 与动态布林边界的分类迁移。
    pub boundary_comparison: BoundaryComparison,
    /// 固定十笔诊断样本审计。
    pub target_sample_audit: Vec<SourceExtremeTargetAudit>,
    /// L1 停止或升级门禁。
    pub decision: SourceExtremeReclaimDecision,
    /// 全部 673 个来源 setup 的无标签终态。
    pub candidates: Vec<SourceExtremeReclaimCandidate>,
}

/// 读取冻结机器报告，校验身份后写出来源极值确认的 L1 账本。
pub fn run_first_retest_source_extreme_reclaim_l1(
    source: &Path,
    output: &Path,
) -> Result<SourceExtremeReclaimReport> {
    let read_started = Instant::now();
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取冻结首次重测 L1 报告失败：{}", source.display()))?;
    let source_report_sha256 = sha256_hex(&bytes);
    if source_report_sha256 != EXPECTED_SOURCE_REPORT_SHA256 {
        bail!(
            "source report SHA mismatch: expected {}, got {}",
            EXPECTED_SOURCE_REPORT_SHA256,
            source_report_sha256
        );
    }
    let raw: Value = serde_json::from_slice(&bytes).context("解析来源 L1 JSON 失败")?;
    validate_no_outcome_candidate_fields(&raw)?;
    let source_report: SourceReport =
        serde_json::from_value(raw).context("读取来源 L1 合同字段失败")?;
    let dataset_fingerprint_sha256 = validate_source_report(&source_report)?;
    let source_read_and_verify = read_started.elapsed().as_millis();

    let transform_started = Instant::now();
    let mut candidates = source_report
        .candidates
        .into_iter()
        .map(transform_candidate)
        .collect::<Result<Vec<_>>>()?;
    candidates.sort_by(|left, right| {
        (
            left.setup_ts_ms,
            left.direction.as_str(),
            left.symbol.as_str(),
        )
            .cmp(&(
                right.setup_ts_ms,
                right.direction.as_str(),
                right.symbol.as_str(),
            ))
    });
    let target_sample_audit = audit_target_samples(&candidates);
    let summary = summarize_candidates(&candidates, &target_sample_audit);
    let boundary_comparison = compare_boundaries(&candidates);
    let decision = decide_l1(&summary, &target_sample_audit);
    let candidate_ledger_sha256 = sha256_hex(&serde_json::to_vec(&candidates)?);
    let ledger_transform_and_summary = transform_started.elapsed().as_millis();

    let report = SourceExtremeReclaimReport {
        schema_version: "momentum_bollinger_source_extreme_reclaim_l1_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: SourceExtremeReclaimIdentity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: SOURCE_EXTREME_RECLAIM_CANDIDATE_KEY,
            rule_version: SOURCE_EXTREME_RECLAIM_RULE_VERSION,
            source_candidate_key: SOURCE_CANDIDATE_KEY,
            source_rule_version: SOURCE_RULE_VERSION,
            only_variable: "replace dynamic directional Bollinger-band close confirmation with strict close back through the frozen source setup extreme",
            close_confirmation_policy: "short confirms only when completed first-retest close<source setup high; long confirms only when close>source setup low; equality rejects",
            first_retest_policy: "reuse the frozen source ledger first retest within offsets 1..=12; a failed first retest is terminal and no later retest may confirm",
            activation_policy: "confirmation exists at first-retest close; earliest entry timestamp is the next 15m candle and L1 does not read its price",
            label_boundary: "reads the frozen source setup and completed first-retest fields only; no entry price, fill, stop/target path, MFE, MAE, exit, PnL, R, win, or loss fields",
        },
        source_evidence: SourceEvidence {
            source_report_sha256,
            source_candidate_ledger_sha256: source_report.source_candidate_ledger_sha256,
            dataset_fingerprint_sha256,
            candidate_schema_no_outcome_fields: true,
        },
        coverage: source_report.coverage,
        phase_timings_ms: SourceExtremePhaseTimingsMs {
            source_read_and_verify,
            ledger_transform_and_summary,
        },
        candidate_ledger_sha256,
        summary,
        boundary_comparison,
        target_sample_audit,
        decision,
        candidates,
    };

    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建来源极值 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入来源极值 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 校验策略、数据、候选数量和 L1 无结果边界，任何漂移都直接拒绝。
fn validate_source_report(report: &SourceReport) -> Result<String> {
    if report.schema_version != SOURCE_REPORT_SCHEMA_VERSION {
        bail!("unexpected source schema: {}", report.schema_version);
    }
    if report.identity.candidate_key != SOURCE_CANDIDATE_KEY
        || report.identity.rule_version != SOURCE_RULE_VERSION
        || report.identity.source_candidate_key != SOURCE_SETUP_CANDIDATE_KEY
        || report.identity.source_rule_version != SOURCE_SETUP_RULE_VERSION
    {
        bail!("source strategy identity mismatch");
    }
    if report.source_candidate_ledger_sha256 != EXPECTED_SOURCE_CANDIDATE_LEDGER_SHA256 {
        bail!("source candidate ledger SHA mismatch");
    }
    if report.decision.outcome_evaluation_performed {
        bail!("source report contains outcome evaluation");
    }
    if report.summary.base_touch_setups != EXPECTED_BASE_TOUCH_SETUPS
        || report.candidates.len() != EXPECTED_BASE_TOUCH_SETUPS
    {
        bail!("source base setup count mismatch");
    }
    if report.summary.first_retest_setups != EXPECTED_FIRST_RETEST_SETUPS {
        bail!("source first-retest count mismatch");
    }
    let candidate_first_retests = report
        .candidates
        .iter()
        .filter(|candidate| candidate.first_retest_ts_ms.is_some())
        .count();
    if candidate_first_retests != EXPECTED_FIRST_RETEST_SETUPS {
        bail!("source candidate first-retest count mismatch");
    }
    let dataset_fingerprint = report
        .coverage
        .get("dataset_fingerprint_sha256")
        .and_then(Value::as_str)
        .context("source coverage missing dataset fingerprint")?;
    if dataset_fingerprint != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("source dataset fingerprint mismatch");
    }
    Ok(dataset_fingerprint.to_owned())
}

/// 拒绝任何已经混入成交、路径或盈亏字段的候选账本。
fn validate_no_outcome_candidate_fields(report: &Value) -> Result<()> {
    let candidates = report
        .get("candidates")
        .and_then(Value::as_array)
        .context("source report missing candidates array")?;
    const FORBIDDEN: [&str; 14] = [
        "entry_price",
        "fill_price",
        "filled",
        "mfe",
        "mae",
        "exit_ts_ms",
        "exit_price",
        "exit_reason",
        "pnl",
        "net_pnl",
        "r",
        "outcome_r",
        "win",
        "loss",
    ];
    for (index, candidate) in candidates.iter().enumerate() {
        let object = candidate
            .as_object()
            .with_context(|| format!("source candidate {index} is not an object"))?;
        if let Some(field) = FORBIDDEN.iter().find(|field| object.contains_key(**field)) {
            bail!("source candidate {index} contains forbidden outcome field {field}");
        }
    }
    Ok(())
}

/// 将一条已完成首次重测或 blocker 候选映射到冻结来源极值边界。
fn transform_candidate(source: SourceCandidate) -> Result<SourceExtremeReclaimCandidate> {
    validate_direction_and_prices(&source)?;
    let source_dynamic_band_confirmed = source.close_inside_directional_band;
    let close_back_through_source_extreme = source
        .first_retest_close
        .map(|close| {
            strict_source_extreme_reclaim(&source.direction, close, source.source_extreme_price)
        })
        .transpose()?;
    let confirmation_signal_ts_ms =
        match (source.first_retest_ts_ms, close_back_through_source_extreme) {
            (Some(ts), Some(true)) => Some(ts),
            (Some(_), Some(false)) => None,
            (None, None) => None,
            _ => bail!("inconsistent first-retest fields for {}", source.symbol),
        };
    let earliest_entry_ts_ms = confirmation_signal_ts_ms
        .map(|ts| {
            ts.checked_add(MS_15M)
                .context("earliest entry timestamp overflow")
        })
        .transpose()?;
    let status = match close_back_through_source_extreme {
        Some(true) => "confirmed_close_back_through_source_extreme".to_owned(),
        Some(false) => "rejected_close_not_back_through_source_extreme".to_owned(),
        None => source.status.clone(),
    };

    Ok(SourceExtremeReclaimCandidate {
        symbol: source.symbol,
        setup_ts_ms: source.setup_ts_ms,
        setup_month_utc: source.setup_month_utc,
        direction: source.direction,
        source_trigger: source.source_trigger,
        source_extreme_price: source.source_extreme_price,
        filtered_volume_ratio: source.filtered_volume_ratio,
        prior_96_net_move_pct: source.prior_96_net_move_pct,
        directional_wick_range_ratio: source.directional_wick_range_ratio,
        first_retest_ts_ms: source.first_retest_ts_ms,
        first_retest_offset_bars: source.first_retest_offset_bars,
        first_retest_close: source.first_retest_close,
        retest_bollinger_middle: source.retest_bollinger_middle,
        retest_bollinger_upper: source.retest_bollinger_upper,
        retest_bollinger_lower: source.retest_bollinger_lower,
        source_dynamic_band_confirmed,
        close_back_through_source_extreme,
        confirmation_signal_ts_ms,
        earliest_entry_ts_ms,
        status,
    })
}

/// 校验镜像方向与首次重测字段成组出现，避免用缺省值生成确认。
fn validate_direction_and_prices(source: &SourceCandidate) -> Result<()> {
    if !matches!(source.direction.as_str(), "long" | "short") {
        bail!(
            "invalid direction {} for {}",
            source.direction,
            source.symbol
        );
    }
    if !source.source_extreme_price.is_finite() || source.source_extreme_price <= 0.0 {
        bail!("invalid source extreme for {}", source.symbol);
    }
    let has_retest = source.first_retest_ts_ms.is_some();
    for (name, present) in [
        (
            "first_retest_offset_bars",
            source.first_retest_offset_bars.is_some(),
        ),
        ("first_retest_close", source.first_retest_close.is_some()),
        (
            "retest_bollinger_middle",
            source.retest_bollinger_middle.is_some(),
        ),
        (
            "retest_bollinger_upper",
            source.retest_bollinger_upper.is_some(),
        ),
        (
            "retest_bollinger_lower",
            source.retest_bollinger_lower.is_some(),
        ),
        (
            "close_inside_directional_band",
            source.close_inside_directional_band.is_some(),
        ),
        (
            "close_beyond_source_extreme",
            source.close_beyond_source_extreme.is_some(),
        ),
    ] {
        if present != has_retest {
            bail!("inconsistent {name} for {}", source.symbol);
        }
    }
    if source
        .first_retest_close
        .is_some_and(|price| !price.is_finite() || price <= 0.0)
    {
        bail!("invalid first-retest close for {}", source.symbol);
    }
    Ok(())
}

/// 对做空和做多应用严格镜像的来源极值收回定义；等于边界不会确认。
fn strict_source_extreme_reclaim(direction: &str, close: f64, extreme: f64) -> Result<bool> {
    match direction {
        "short" => Ok(close < extreme),
        "long" => Ok(close > extreme),
        other => bail!("invalid direction: {other}"),
    }
}

/// 汇总无标签覆盖和分散性，不访问确认时点之后的任何字段。
fn summarize_candidates(
    candidates: &[SourceExtremeReclaimCandidate],
    targets: &[SourceExtremeTargetAudit],
) -> SourceExtremeReclaimSummary {
    let first_retest_setups = candidates
        .iter()
        .filter(|candidate| candidate.first_retest_ts_ms.is_some())
        .count();
    let confirmed = candidates
        .iter()
        .filter(|candidate| candidate.confirmation_signal_ts_ms.is_some())
        .collect::<Vec<_>>();
    let rejected = candidates
        .iter()
        .filter(|candidate| candidate.close_back_through_source_extreme == Some(false))
        .collect::<Vec<_>>();
    let mut confirmed_by_direction = BTreeMap::new();
    let mut rejected_by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    for candidate in &confirmed {
        *confirmed_by_direction
            .entry(candidate.direction.clone())
            .or_default() += 1;
        symbols.insert(candidate.symbol.as_str());
        if let Some(ts) = candidate.confirmation_signal_ts_ms {
            if let Some(month) = Utc.timestamp_millis_opt(ts).single() {
                months.insert(month.format("%Y-%m").to_string());
            }
        }
    }
    for candidate in &rejected {
        *rejected_by_direction
            .entry(candidate.direction.clone())
            .or_default() += 1;
    }
    let mut blockers = BTreeMap::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.first_retest_ts_ms.is_none())
    {
        *blockers.entry(candidate.status.clone()).or_default() += 1;
    }
    SourceExtremeReclaimSummary {
        base_touch_setups: candidates.len(),
        first_retest_setups,
        confirmed_setups: confirmed.len(),
        rejected_source_extreme_setups: rejected.len(),
        confirmation_retention_pct_of_first_retests: percentage(
            confirmed.len(),
            first_retest_setups,
        ),
        rejection_impact_pct_of_first_retests: percentage(rejected.len(), first_retest_setups),
        confirmed_by_direction,
        rejected_by_direction,
        confirmed_symbol_count: symbols.len(),
        confirmed_month_count: months.len(),
        confirmed_effective_market_events: effective_market_event_count(&confirmed),
        blockers,
        target_rejected_count: targets
            .iter()
            .filter(|target| target.rejected_by_source_extreme)
            .count(),
    }
}

/// 记录边界改变带来的四格分类，防止只报告被过滤的一侧。
fn compare_boundaries(candidates: &[SourceExtremeReclaimCandidate]) -> BoundaryComparison {
    let mut comparison = BoundaryComparison::default();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.first_retest_ts_ms.is_some())
    {
        match (
            candidate.source_dynamic_band_confirmed,
            candidate.close_back_through_source_extreme,
        ) {
            (Some(true), Some(true)) => comparison.dynamic_confirmed_source_extreme_confirmed += 1,
            (Some(true), Some(false)) => comparison.dynamic_confirmed_source_extreme_rejected += 1,
            (Some(false), Some(true)) => comparison.dynamic_rejected_source_extreme_confirmed += 1,
            (Some(false), Some(false)) => comparison.dynamic_rejected_source_extreme_rejected += 1,
            _ => unreachable!("validated retest candidates have both classifications"),
        }
    }
    comparison
}

/// 对固定十笔诊断 setup 核对定义，不读取其胜负或退出结果。
fn audit_target_samples(
    candidates: &[SourceExtremeReclaimCandidate],
) -> Vec<SourceExtremeTargetAudit> {
    TARGET_SAMPLES
        .iter()
        .map(|(symbol, setup_ts_ms)| {
            let candidate = candidates.iter().find(|candidate| {
                candidate.symbol == *symbol && candidate.setup_ts_ms == *setup_ts_ms
            });
            SourceExtremeTargetAudit {
                symbol,
                setup_ts_ms: *setup_ts_ms,
                source_found: candidate.is_some(),
                first_retest_ts_ms: candidate.and_then(|item| item.first_retest_ts_ms),
                first_retest_close: candidate.and_then(|item| item.first_retest_close),
                source_extreme_price: candidate.map(|item| item.source_extreme_price),
                source_dynamic_band_status: candidate.map(|item| {
                    if item.source_dynamic_band_confirmed == Some(true) {
                        "confirmed_close_inside_directional_band"
                    } else {
                        "rejected_close_outside_directional_band"
                    }
                    .to_owned()
                }),
                status: candidate.map(|item| item.status.clone()),
                rejected_by_source_extreme: candidate
                    .is_some_and(|item| item.close_back_through_source_extreme == Some(false)),
            }
        })
        .collect()
}

/// 应用预注册覆盖、方向、分散性和固定目标门禁。
fn decide_l1(
    summary: &SourceExtremeReclaimSummary,
    targets: &[SourceExtremeTargetAudit],
) -> SourceExtremeReclaimDecision {
    let confirmed_long = summary
        .confirmed_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let confirmed_short = summary
        .confirmed_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let rejected_long = summary
        .rejected_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let rejected_short = summary
        .rejected_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert("input_identity_verified", true);
    gates.insert(
        "first_retests_equal_frozen_283",
        summary.first_retest_setups == EXPECTED_FIRST_RETEST_SETUPS,
    );
    gates.insert(
        "confirmed_setups_at_least_30",
        summary.confirmed_setups >= 30,
    );
    gates.insert(
        "rejected_source_extreme_at_least_20",
        summary.rejected_source_extreme_setups >= 20,
    );
    gates.insert(
        "confirmation_retention_between_20_and_80_pct",
        (20.0..=80.0).contains(&summary.confirmation_retention_pct_of_first_retests),
    );
    gates.insert(
        "both_directions_confirmed_at_least_5",
        confirmed_long >= 5 && confirmed_short >= 5,
    );
    gates.insert(
        "both_directions_rejected_at_least_5",
        rejected_long >= 5 && rejected_short >= 5,
    );
    gates.insert(
        "confirmed_symbols_at_least_8",
        summary.confirmed_symbol_count >= 8,
    );
    gates.insert(
        "confirmed_months_at_least_6",
        summary.confirmed_month_count >= 6,
    );
    gates.insert(
        "confirmed_effective_events_at_least_15",
        summary.confirmed_effective_market_events >= 15,
    );
    gates.insert(
        "all_10_target_sources_found",
        targets.iter().all(|target| target.source_found),
    );
    gates.insert(
        "all_10_target_first_retests_resolved",
        targets
            .iter()
            .all(|target| target.first_retest_ts_ms.is_some()),
    );
    gates.insert(
        "target_rejected_at_least_5_of_10",
        summary.target_rejected_count >= 5,
    );
    gates.insert("outcome_evaluation_not_performed", true);
    let passed = gates.values().all(|value| *value);
    SourceExtremeReclaimDecision {
        status: if passed {
            "coverage_pass_l2_ready"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "来源极值收回定义达到预注册的无标签覆盖、方向、分散性与固定目标门槛；允许冻结独立 L2 回放。"
                .to_owned()
        } else {
            "至少一项预注册 L1 门禁失败；不得读取确认后的成交、退出或盈亏。".to_owned()
        },
        outcome_evaluation_performed: false,
    }
}

/// 按确认时间与方向把一小时内跨币共振归为一个市场事件。
fn effective_market_event_count(candidates: &[&SourceExtremeReclaimCandidate]) -> usize {
    let mut ordered = candidates.to_vec();
    ordered.sort_by_key(|candidate| {
        (
            candidate.confirmation_signal_ts_ms.unwrap_or(i64::MAX),
            candidate.direction.as_str(),
            candidate.symbol.as_str(),
        )
    });
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for candidate in ordered {
        let Some(ts) = candidate.confirmation_signal_ts_ms else {
            continue;
        };
        let starts_new = last_by_direction
            .get(candidate.direction.as_str())
            .is_none_or(|previous| ts - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(candidate.direction.as_str(), ts);
    }
    count
}

/// 生成原始字节或序列化候选的 SHA-256 十六进制字符串。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// 空分母按零处理，使覆盖门禁显式失败。
fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小来源候选，测试只改变方向、收盘和是否发生重测。
    fn source_candidate(direction: &str, close: Option<f64>) -> SourceCandidate {
        SourceCandidate {
            symbol: "TEST-USDT-SWAP".to_owned(),
            setup_ts_ms: 100 * MS_15M,
            setup_month_utc: "1970-02".to_owned(),
            direction: direction.to_owned(),
            source_trigger: "test_wick".to_owned(),
            source_extreme_price: 100.0,
            filtered_volume_ratio: 3.0,
            prior_96_net_move_pct: if direction == "short" { 10.0 } else { -10.0 },
            directional_wick_range_ratio: 0.7,
            first_retest_ts_ms: close.map(|_| 101 * MS_15M),
            first_retest_offset_bars: close.map(|_| 1),
            first_retest_close: close,
            retest_bollinger_middle: close.map(|_| 99.0),
            retest_bollinger_upper: close.map(|_| 102.0),
            retest_bollinger_lower: close.map(|_| 96.0),
            close_inside_directional_band: close.map(|_| true),
            close_beyond_source_extreme: close.map(|price| match direction {
                "short" => price > 100.0,
                "long" => price < 100.0,
                _ => false,
            }),
            status: if close.is_some() {
                "confirmed_close_inside_directional_band"
            } else {
                "first_retest_not_touched_within_12_candles"
            }
            .to_owned(),
        }
    }

    /// 做空首次重测只有严格收于来源高点下方才确认，并在下一根激活。
    #[test]
    fn short_requires_strict_close_below_source_high() {
        let candidate = transform_candidate(source_candidate("short", Some(99.0)))
            .expect("transform short reclaim");
        assert_eq!(candidate.close_back_through_source_extreme, Some(true));
        assert_eq!(candidate.confirmation_signal_ts_ms, Some(101 * MS_15M));
        assert_eq!(candidate.earliest_entry_ts_ms, Some(102 * MS_15M));
    }

    /// 多单镜像要求首次重测严格收于来源低点上方。
    #[test]
    fn long_requires_strict_close_above_source_low() {
        let candidate = transform_candidate(source_candidate("long", Some(101.0)))
            .expect("transform long reclaim");
        assert_eq!(candidate.close_back_through_source_extreme, Some(true));
        assert_eq!(
            candidate.status,
            "confirmed_close_back_through_source_extreme"
        );
    }

    /// 边界相等明确拒绝，不能沿用动态布林规则的包含等号语义。
    #[test]
    fn equality_rejects_for_both_directions() {
        for direction in ["short", "long"] {
            let candidate = transform_candidate(source_candidate(direction, Some(100.0)))
                .expect("transform equality");
            assert_eq!(candidate.close_back_through_source_extreme, Some(false));
            assert_eq!(candidate.confirmation_signal_ts_ms, None);
        }
    }

    /// 没有首次重测的 blocker 原样保留，不补造确认时间。
    #[test]
    fn untouched_blocker_is_preserved() {
        let candidate = transform_candidate(source_candidate("short", None))
            .expect("transform untouched blocker");
        assert_eq!(
            candidate.status,
            "first_retest_not_touched_within_12_candles"
        );
        assert_eq!(candidate.close_back_through_source_extreme, None);
        assert_eq!(candidate.earliest_entry_ts_ms, None);
    }

    /// 新候选 JSON 不得包含成交、退出或盈亏结果字段。
    #[test]
    fn output_candidate_schema_contains_no_outcome_fields() {
        let candidate = transform_candidate(source_candidate("short", Some(99.0)))
            .expect("transform schema candidate");
        let object = serde_json::to_value(candidate)
            .expect("serialize output candidate")
            .as_object()
            .expect("candidate object")
            .clone();
        for forbidden in [
            "entry_price",
            "fill_price",
            "mfe",
            "mae",
            "exit_ts_ms",
            "exit_price",
            "pnl",
            "r",
            "win",
            "loss",
        ] {
            assert!(!object.contains_key(forbidden), "unexpected {forbidden}");
        }
    }
}
