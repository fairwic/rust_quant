//! 来源极值严格收回确认后的原有效期重挂研究。
//!
//! L1 只判断确认后的限价能否在来源 setup 剩余的 12 根有效期内成交。只有全部无结果
//! 覆盖门禁通过，才在同一进程对成交 cohort 执行下一根开盘与来源极值重挂的 L2 配对回放。

use super::{build_l1_report, frozen_l1_args, EVALUATION_END_MS, EVALUATION_START_MS};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs,
};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod l2;
mod report;
pub use report::{
    RelimitCandidate, RelimitConcentration, RelimitCoverage, RelimitIdentity, RelimitL1Decision,
    RelimitL1Report, RelimitL1Summary, RelimitL2Decision, RelimitL2EntrySummary, RelimitL2Identity,
    RelimitL2Report, RelimitLegRecord, RelimitPerformance, RelimitResearchReport,
    RelimitSourceEvidence, RelimitTargetAudit, RelimitTradeRecord,
};

/// 新执行语义的独立候选键；不会覆盖上一版确认后下一根开盘策略。
pub const RELIMIT_CANDIDATE_KEY: &str =
    "market_momentum_bollinger_wick_confirmed_source_extreme_relimit_15m_v1";
/// L1 只评价确认后来源极值重挂在原 setup 有效期内的成交覆盖。
pub const RELIMIT_L1_RULE_VERSION: &str =
    "l1_confirmed_relimit_source_extreme_until_original_setup_expiry_v1";
/// L2 只比较同一成交 cohort 的下一根开盘与来源极值重挂。
pub const RELIMIT_L2_RULE_VERSION: &str = "l2_confirmed_source_extreme_relimit_paired_v2_risk_v1";

const SOURCE_SCHEMA_VERSION: &str = "momentum_bollinger_source_extreme_reclaim_l1_v1";
const SOURCE_CANDIDATE_KEY: &str = "market_momentum_bollinger_wick_source_extreme_reclaim_15m_v1";
const SOURCE_RULE_VERSION: &str = "l1_first_retest_close_back_through_source_extreme_next_open_v1";
const EXPECTED_SOURCE_REPORT_SHA256: &str =
    "ab22aa5c485e660cb0dd32baf9d0327eb6927e8a2bb34e089972a0a949546925";
const EXPECTED_SOURCE_CANDIDATE_LEDGER_SHA256: &str =
    "b343346886306d262d422fec9f0c2c5c12c229c5b8b1e0753624460eedcc77fa";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "0c3d1e6ce33187fbc0fd528486d837574fe176b73a748b1f44dedd3c14c328f5";
const EXPECTED_BASE_TOUCH_SETUPS: usize = 673;
const EXPECTED_CONFIRMED_SETUPS: usize = 143;
const ORIGINAL_SETUP_VALID_CANDLES: usize = 12;
const MS_15M: i64 = 15 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const TARGET_SAMPLES: [(&str, i64); 10] = [
    ("AGLD-USDT-SWAP", 1_783_444_500_000),
    ("YFI-USDT-SWAP", 1_783_334_700_000),
    ("ORDI-USDT-SWAP", 1_782_738_900_000),
    ("WIF-USDT-SWAP", 1_782_530_100_000),
    ("EIGEN-USDT-SWAP", 1_781_521_200_000),
    ("GPS-USDT-SWAP", 1_781_509_500_000),
    ("OP-USDT-SWAP", 1_781_332_200_000),
    ("CVX-USDT-SWAP", 1_781_216_100_000),
    ("MOVE-USDT-SWAP", 1_780_909_200_000),
    ("ACT-USDT-SWAP", 1_780_608_600_000),
];

#[derive(Debug, Deserialize)]
struct SourceIdentity {
    candidate_key: String,
    rule_version: String,
}

#[derive(Debug, Deserialize)]
struct SourceEvidence {
    dataset_fingerprint_sha256: String,
    candidate_schema_no_outcome_fields: bool,
}

#[derive(Debug, Deserialize)]
struct SourceSummary {
    base_touch_setups: usize,
    confirmed_setups: usize,
}

#[derive(Debug, Deserialize)]
struct SourceDecision {
    status: String,
    outcome_evaluation_performed: bool,
}

/// 冻结来源 L1 中重挂成交和 L2 风险重建所需的最小字段。
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
    confirmation_signal_ts_ms: Option<i64>,
    earliest_entry_ts_ms: Option<i64>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct SourceReport {
    schema_version: String,
    identity: SourceIdentity,
    source_evidence: SourceEvidence,
    candidate_ledger_sha256: String,
    summary: SourceSummary,
    decision: SourceDecision,
    candidates: Vec<SourceCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RelimitResolution {
    entry_idx: Option<usize>,
    replaced_by_ts_ms: Option<i64>,
    terminal_status: &'static str,
}

/// 校验冻结输入，加载同一行情，并生成本批唯一 L1/条件 L2 机器报告。
pub async fn run_confirmed_source_extreme_relimit(
    source: &Path,
    output: &Path,
) -> Result<RelimitResearchReport> {
    let (source_report, source_report_sha256) = load_and_validate_source(source)?;
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for confirmed source-extreme relimit research")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_research_report(&data, &config.args, source_report, source_report_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建重挂研究报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入重挂研究报告失败：{}", output.display()))?;
    Ok(report)
}

/// 重建基础行情身份，再按 L1 门禁决定是否允许读取成交后的 L2 路径。
fn build_research_report(
    data: &BacktestDataSet,
    args: &MarketVelocityEventBacktestArgs,
    source: SourceReport,
    source_report_sha256: String,
) -> Result<RelimitResearchReport> {
    let base_report = build_l1_report(data, args)?;
    if base_report.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("reloaded dataset fingerprint mismatch");
    }
    let l1 = build_relimit_l1(data, &source)?;
    let l2 = if l1.decision.status == "coverage_pass_l2_ready" {
        Some(l2::build_l2(data, &l1)?)
    } else {
        None
    };
    Ok(RelimitResearchReport {
        schema_version: "momentum_bollinger_confirmed_source_extreme_relimit_l1_l2_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: RelimitIdentity {
            level: "L1_no_outcome_fill_coverage_with_conditional_L2_local_diagnostic",
            candidate_key: RELIMIT_CANDIDATE_KEY,
            l1_rule_version: RELIMIT_L1_RULE_VERSION,
            l2_rule_version: RELIMIT_L2_RULE_VERSION,
            only_variable: "after strict source-extreme reclaim confirmation, replace next-15m-open entry with a source-extreme limit active only for the original setup's remaining 12-candle lifetime",
            activation_policy: "confirmation is visible at first-retest close; the relimit can fill only from the next completed 15m candle onward",
            expiry_policy: "the order expires after source setup offset 12 and confirmation never resets the original lifetime",
            replacement_policy: "within a candle, test the older relimit touch first; only an untouched order is replaced by a newer source setup at that candle close",
            l1_label_boundary: "L1 reads only source setup, confirmation, later high/low for fillability, original expiry, and replacement timestamps; it reads no post-entry stop, target, MFE, MAE, exit, PnL, R, win, or loss",
        },
        source_evidence: RelimitSourceEvidence {
            source_l1_report_sha256: source_report_sha256,
            source_l1_candidate_ledger_sha256: source.candidate_ledger_sha256,
            source_dataset_fingerprint_sha256: source
                .source_evidence
                .dataset_fingerprint_sha256,
            reloaded_dataset_fingerprint_sha256: base_report
                .coverage
                .dataset_fingerprint_sha256,
            source_candidate_schema_no_outcome_fields: true,
        },
        coverage: RelimitCoverage {
            returned_symbol_count: base_report.coverage.returned_symbol_count,
            eligible_symbol_count: base_report.coverage.eligible_symbol_count,
            excluded_symbol_count: base_report.coverage.excluded_symbols.len(),
            evaluation_start_ms: EVALUATION_START_MS,
            evaluation_end_ms: EVALUATION_END_MS,
            universe_limitation: "current-live Top60 with 44 locally complete members has survivorship bias and is not point-in-time or OOS evidence",
        },
        l1,
        l2,
    })
}

/// 校验来源文件字节身份、策略版本、无结果边界、候选数量与唯一键。
fn load_and_validate_source(source: &Path) -> Result<(SourceReport, String)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取来源极值确认 L1 失败：{}", source.display()))?;
    let report_sha256 = sha256_hex(&bytes);
    if report_sha256 != EXPECTED_SOURCE_REPORT_SHA256 {
        bail!("source L1 report SHA mismatch");
    }
    let raw: Value = serde_json::from_slice(&bytes).context("解析来源 L1 JSON 失败")?;
    validate_no_outcome_candidate_fields(&raw)?;
    let report: SourceReport = serde_json::from_value(raw).context("读取来源 L1 合同失败")?;
    if report.schema_version != SOURCE_SCHEMA_VERSION
        || report.identity.candidate_key != SOURCE_CANDIDATE_KEY
        || report.identity.rule_version != SOURCE_RULE_VERSION
    {
        bail!("source L1 strategy identity mismatch");
    }
    if report.candidate_ledger_sha256 != EXPECTED_SOURCE_CANDIDATE_LEDGER_SHA256 {
        bail!("source L1 candidate ledger SHA mismatch");
    }
    if report.source_evidence.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || !report.source_evidence.candidate_schema_no_outcome_fields
    {
        bail!("source L1 dataset or label boundary mismatch");
    }
    if report.decision.status != "coverage_pass_l2_ready"
        || report.decision.outcome_evaluation_performed
    {
        bail!("source L1 is not an eligible no-outcome ledger");
    }
    if report.summary.base_touch_setups != EXPECTED_BASE_TOUCH_SETUPS
        || report.candidates.len() != EXPECTED_BASE_TOUCH_SETUPS
        || report.summary.confirmed_setups != EXPECTED_CONFIRMED_SETUPS
    {
        bail!("source L1 candidate count mismatch");
    }
    let mut identities = BTreeSet::new();
    let mut confirmed = 0;
    for candidate in &report.candidates {
        validate_source_candidate(candidate)?;
        if !identities.insert((candidate.symbol.as_str(), candidate.setup_ts_ms)) {
            bail!("duplicate source candidate identity");
        }
        if candidate.status == "confirmed_close_back_through_source_extreme" {
            confirmed += 1;
        }
    }
    if confirmed != EXPECTED_CONFIRMED_SETUPS {
        bail!("source confirmed candidate count mismatch");
    }
    Ok((report, report_sha256))
}

/// 校验来源候选的方向、价格和确认字段必须成组出现。
fn validate_source_candidate(candidate: &SourceCandidate) -> Result<()> {
    if !matches!(candidate.direction.as_str(), "long" | "short") {
        bail!("invalid source direction for {}", candidate.symbol);
    }
    if !candidate.source_extreme_price.is_finite() || candidate.source_extreme_price <= 0.0 {
        bail!("invalid source extreme for {}", candidate.symbol);
    }
    let confirmed = candidate.status == "confirmed_close_back_through_source_extreme";
    for (name, present) in [
        ("first_retest_ts_ms", candidate.first_retest_ts_ms.is_some()),
        (
            "first_retest_offset_bars",
            candidate.first_retest_offset_bars.is_some(),
        ),
        (
            "confirmation_signal_ts_ms",
            candidate.confirmation_signal_ts_ms.is_some(),
        ),
        (
            "earliest_entry_ts_ms",
            candidate.earliest_entry_ts_ms.is_some(),
        ),
    ] {
        if confirmed != present {
            bail!(
                "inconsistent {name} for confirmed source {}",
                candidate.symbol
            );
        }
    }
    Ok(())
}

/// 为 143 个来源确认构建重挂成交或 blocker 的唯一无结果终态。
fn build_relimit_l1(data: &BacktestDataSet, source: &SourceReport) -> Result<RelimitL1Report> {
    let replacement_times = replacement_times_by_symbol(&source.candidates);
    let mut candidates = Vec::with_capacity(EXPECTED_CONFIRMED_SETUPS);
    for candidate in source
        .candidates
        .iter()
        .filter(|candidate| candidate.status == "confirmed_close_back_through_source_extreme")
    {
        let candles = data
            .candles_15m_computed
            .get(&candidate.symbol)
            .with_context(|| format!("missing candles for {}", candidate.symbol))?;
        let replacements = replacement_times
            .get(&candidate.symbol)
            .context("confirmed source missing replacement timeline")?;
        candidates.push(resolve_candidate(candles, candidate, replacements)?);
    }
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
    let summary = summarize_l1(&candidates, &target_sample_audit);
    let decision = decide_l1(&summary);
    let candidate_ledger_sha256 = sha256_hex(&serde_json::to_vec(&candidates)?);
    Ok(RelimitL1Report {
        candidate_ledger_sha256,
        summary,
        target_sample_audit,
        decision,
        candidates,
    })
}

/// 每个来源外轨 setup 都能在旧挂单未成交时替换同币种旧订单。
fn replacement_times_by_symbol(sources: &[SourceCandidate]) -> BTreeMap<String, BTreeSet<i64>> {
    let mut timelines = BTreeMap::new();
    for source in sources {
        timelines
            .entry(source.symbol.clone())
            .or_insert_with(BTreeSet::new)
            .insert(source.setup_ts_ms);
    }
    timelines
}

/// 校验确认时序后，在原 setup 剩余有效期内解析一条重挂订单。
fn resolve_candidate(
    candles: &[ComputedCandle],
    source: &SourceCandidate,
    replacement_times: &BTreeSet<i64>,
) -> Result<RelimitCandidate> {
    let setup_idx = candles
        .binary_search_by_key(&source.setup_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| anyhow::anyhow!("source setup candle missing for {}", source.symbol))?;
    let confirmation_ts_ms = source
        .confirmation_signal_ts_ms
        .context("confirmed source missing confirmation timestamp")?;
    let retest_idx = candles
        .binary_search_by_key(&confirmation_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| anyhow::anyhow!("source confirmation candle missing for {}", source.symbol))?;
    let retest_offset = source
        .first_retest_offset_bars
        .context("confirmed source missing retest offset")?;
    if source.first_retest_ts_ms != Some(confirmation_ts_ms)
        || retest_idx != setup_idx.saturating_add(retest_offset)
        || !(1..=ORIGINAL_SETUP_VALID_CANDLES).contains(&retest_offset)
    {
        bail!("source confirmation timing mismatch for {}", source.symbol);
    }
    let activation_ts_ms = confirmation_ts_ms
        .checked_add(MS_15M)
        .context("relimit activation timestamp overflow")?;
    if source.earliest_entry_ts_ms != Some(activation_ts_ms) {
        bail!("source next-open timestamp mismatch for {}", source.symbol);
    }
    let expiry_idx = setup_idx
        .checked_add(ORIGINAL_SETUP_VALID_CANDLES)
        .context("original expiry index overflow")?;
    let original_expiry_ts_ms = source
        .setup_ts_ms
        .checked_add(ORIGINAL_SETUP_VALID_CANDLES as i64 * MS_15M)
        .context("original expiry timestamp overflow")?;
    if candles
        .get(expiry_idx)
        .is_some_and(|candle| candle.candle.ts != original_expiry_ts_ms)
    {
        bail!("non-contiguous original expiry for {}", source.symbol);
    }
    let activation_idx = retest_idx
        .checked_add(1)
        .context("relimit activation index overflow")?;
    let resolution = resolve_relimit(
        candles,
        activation_idx,
        expiry_idx,
        source.setup_ts_ms,
        &source.direction,
        source.source_extreme_price,
        replacement_times,
    )?;
    let relimit_entry_ts_ms = resolution
        .entry_idx
        .and_then(|idx| candles.get(idx))
        .map(|candle| candle.candle.ts);
    let relimit_entry_offset_bars = resolution
        .entry_idx
        .and_then(|idx| idx.checked_sub(setup_idx));
    let wait_bars_after_confirmation = resolution
        .entry_idx
        .and_then(|idx| idx.checked_sub(retest_idx));
    Ok(RelimitCandidate {
        candidate_id: format!("{}:{}", source.symbol, source.setup_ts_ms),
        symbol: source.symbol.clone(),
        setup_ts_ms: source.setup_ts_ms,
        setup_month_utc: source.setup_month_utc.clone(),
        direction: source.direction.clone(),
        source_trigger: source.source_trigger.clone(),
        source_extreme_price: source.source_extreme_price,
        filtered_volume_ratio: source.filtered_volume_ratio,
        prior_96_net_move_pct: source.prior_96_net_move_pct,
        directional_wick_range_ratio: source.directional_wick_range_ratio,
        confirmation_signal_ts_ms: confirmation_ts_ms,
        first_retest_offset_bars: retest_offset,
        activation_ts_ms,
        original_expiry_ts_ms,
        relimit_entry_ts_ms,
        relimit_entry_offset_bars,
        wait_bars_after_confirmation,
        replaced_by_setup_ts_ms: resolution.replaced_by_ts_ms,
        terminal_status: resolution.terminal_status,
    })
}

/// 先检查本根是否触及旧限价，再允许本根收盘的新 setup 替换未成交订单。
fn resolve_relimit(
    candles: &[ComputedCandle],
    activation_idx: usize,
    expiry_idx: usize,
    source_setup_ts_ms: i64,
    direction: &str,
    source_extreme_price: f64,
    replacement_times: &BTreeSet<i64>,
) -> Result<RelimitResolution> {
    if activation_idx > expiry_idx {
        return Ok(RelimitResolution {
            entry_idx: None,
            replaced_by_ts_ms: None,
            terminal_status: "original_setup_ttl_exhausted_at_confirmation",
        });
    }
    let last_available_idx = candles.len().saturating_sub(1);
    let scan_end = expiry_idx.min(last_available_idx);
    if activation_idx >= candles.len() {
        return Ok(RelimitResolution {
            entry_idx: None,
            replaced_by_ts_ms: None,
            terminal_status: "forward_data_incomplete_before_activation",
        });
    }
    for idx in activation_idx..=scan_end {
        let candle = &candles[idx];
        if directional_extreme_touched(candle, source_extreme_price, direction)? {
            return Ok(RelimitResolution {
                entry_idx: Some(idx),
                replaced_by_ts_ms: None,
                terminal_status: "relimit_filled_at_source_extreme",
            });
        }
        if candle.candle.ts > source_setup_ts_ms && replacement_times.contains(&candle.candle.ts) {
            return Ok(RelimitResolution {
                entry_idx: None,
                replaced_by_ts_ms: Some(candle.candle.ts),
                terminal_status: "relimit_replaced_by_new_setup",
            });
        }
    }
    Ok(RelimitResolution {
        entry_idx: None,
        replaced_by_ts_ms: None,
        terminal_status: if expiry_idx > last_available_idx {
            "forward_data_incomplete_before_original_expiry"
        } else {
            "relimit_not_touched_before_original_setup_expiry"
        },
    })
}

/// 做空重挂用 high 触及来源 high，做多重挂严格镜像用 low 触及来源 low。
fn directional_extreme_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: &str,
) -> Result<bool> {
    match direction {
        "short" => Ok(candle.candle.high >= price),
        "long" => Ok(candle.candle.low <= price),
        other => bail!("invalid relimit direction: {other}"),
    }
}

/// 汇总成交覆盖、方向、月份、币种、事件和未成交终态，不读取入场后路径。
fn summarize_l1(
    candidates: &[RelimitCandidate],
    targets: &[RelimitTargetAudit],
) -> RelimitL1Summary {
    let filled = candidates
        .iter()
        .filter(|candidate| candidate.relimit_entry_ts_ms.is_some())
        .collect::<Vec<_>>();
    let mut filled_by_direction = BTreeMap::from([("long", 0), ("short", 0)]);
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    for candidate in &filled {
        if let Some(count) = filled_by_direction.get_mut(candidate.direction.as_str()) {
            *count += 1;
        }
        symbols.insert(candidate.symbol.as_str());
        if let Some(ts) = candidate.relimit_entry_ts_ms {
            if let Some(month) = Utc.timestamp_millis_opt(ts).single() {
                months.insert(month.format("%Y-%m").to_string());
            }
        }
    }
    let mut blockers = BTreeMap::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relimit_entry_ts_ms.is_none())
    {
        *blockers.entry(candidate.terminal_status).or_default() += 1;
    }
    RelimitL1Summary {
        source_base_touch_setups: EXPECTED_BASE_TOUCH_SETUPS,
        source_confirmed_setups: candidates.len(),
        terminal_setups: candidates.len(),
        relimit_filled_setups: filled.len(),
        fill_retention_pct: percentage(filled.len(), candidates.len()),
        filled_by_direction,
        filled_symbol_count: symbols.len(),
        filled_month_count: months.len(),
        filled_effective_market_events: effective_market_event_count(&filled),
        blockers,
        target_terminal_count: targets
            .iter()
            .filter(|target| target.source_found && target.terminal_resolved)
            .count(),
    }
}

/// 固定最近十笔仅核对来源身份与新成交政策终态，不以其胜负选择规则。
fn audit_target_samples(candidates: &[RelimitCandidate]) -> Vec<RelimitTargetAudit> {
    TARGET_SAMPLES
        .iter()
        .map(|(symbol, setup_ts_ms)| {
            let candidate = candidates.iter().find(|candidate| {
                candidate.symbol == *symbol && candidate.setup_ts_ms == *setup_ts_ms
            });
            RelimitTargetAudit {
                symbol,
                setup_ts_ms: *setup_ts_ms,
                source_found: candidate.is_some(),
                terminal_resolved: candidate.is_some(),
                terminal_status: candidate.map(|item| item.terminal_status),
                relimit_entry_ts_ms: candidate.and_then(|item| item.relimit_entry_ts_ms),
            }
        })
        .collect()
}

/// 应用预注册 L1 覆盖门禁；任何失败都会阻止同一进程构建 L2。
fn decide_l1(summary: &RelimitL1Summary) -> RelimitL1Decision {
    let long_count = summary
        .filled_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .filled_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert("source_identity_and_no_outcome_verified", true);
    gates.insert(
        "all_143_confirmed_setups_terminal",
        summary.source_confirmed_setups == EXPECTED_CONFIRMED_SETUPS
            && summary.terminal_setups == EXPECTED_CONFIRMED_SETUPS,
    );
    gates.insert(
        "relimit_filled_setups_at_least_30",
        summary.relimit_filled_setups >= 30,
    );
    gates.insert(
        "fill_retention_between_20_and_95_pct",
        (20.0..=95.0).contains(&summary.fill_retention_pct),
    );
    gates.insert(
        "both_directions_filled_at_least_5",
        long_count >= 5 && short_count >= 5,
    );
    gates.insert(
        "filled_symbols_at_least_8",
        summary.filled_symbol_count >= 8,
    );
    gates.insert("filled_months_at_least_6", summary.filled_month_count >= 6);
    gates.insert(
        "filled_effective_events_at_least_15",
        summary.filled_effective_market_events >= 15,
    );
    gates.insert(
        "all_10_fixed_targets_terminal",
        summary.target_terminal_count == TARGET_SAMPLES.len(),
    );
    gates.insert("outcome_evaluation_not_performed", true);
    let passed = gates.values().all(|value| *value);
    RelimitL1Decision {
        status: if passed {
            "coverage_pass_l2_ready"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "重挂来源极值在原 setup 剩余有效期内达到预注册成交、方向和分散性门槛；允许同进程进入一个主候选的 L2 成本后配对回放。"
                .to_owned()
        } else {
            "至少一项预注册 L1 成交覆盖门禁失败；L2 必须为空且本版本停止。".to_owned()
        },
        outcome_evaluation_performed: false,
    }
}

/// 按成交时间和方向将一小时内的跨币共振归并为一个有效事件。
fn effective_market_event_count(candidates: &[&RelimitCandidate]) -> usize {
    let mut ordered = candidates.to_vec();
    ordered.sort_by_key(|candidate| {
        (
            candidate.relimit_entry_ts_ms.unwrap_or(i64::MAX),
            candidate.direction.as_str(),
            candidate.symbol.as_str(),
        )
    });
    let mut last_by_direction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut count = 0;
    for candidate in ordered {
        let Some(ts) = candidate.relimit_entry_ts_ms else {
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

/// 拒绝来源候选中任何成交后价格路径或结果标签。
fn validate_no_outcome_candidate_fields(report: &Value) -> Result<()> {
    let candidates = report
        .get("candidates")
        .and_then(Value::as_array)
        .context("source L1 missing candidates")?;
    const FORBIDDEN: [&str; 18] = [
        "entry_price",
        "fill_price",
        "filled",
        "stop_price",
        "target_price",
        "mfe",
        "mae",
        "exit_ts_ms",
        "exit_price",
        "exit_reason",
        "pnl",
        "net_pnl",
        "r",
        "net_r",
        "outcome_r",
        "win",
        "loss",
        "profit_loss",
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

/// 百分比统一返回 0 到 100，空分母返回零。
fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

/// 生成报告和候选账本的稳定 SHA-256 十六进制身份。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests;
