//! V38 实体包络与扫影线容忍边界的 outcome-blind L1 覆盖审计。
//!
//! 只扫描信号时已经完成的固定横盘窗口，不构造订单或读取任何成交后结果。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    audit_strict_visual_body_wick_candidate_for_research,
    evaluate_strict_visual_body_wick_candidate_for_research,
    evaluate_strict_visual_percentile_candidate_for_research,
    load_research_window_symbols_from_quant_core, Candle, ResearchWindowSymbolCandles,
    StrictVisualBodyWickEnvelope, StrictVisualBreakoutResearchVariant,
    StrictVisualCandidateQualityAudit, StrictVisualRangeEvidence, STRICT_VISUAL_CANDIDATE_LENGTHS,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const MINIMUM_CANDIDATE_WINDOWS: usize = 30;
const MINIMUM_SYMBOLS: usize = 8;
const MINIMUM_EVENT_CLUSTERS: usize = 10;
const MINIMUM_EXPANDED_WINDOWS: usize = 3;
const MINIMUM_EXPANDED_SYMBOLS: usize = 3;
const MINIMUM_AFFECTED_RATIO: f64 = 0.10;
const MAXIMUM_AFFECTED_RATIO: f64 = 0.40;
const MAXIMUM_CHANGED_SAMPLES: usize = 200;

const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_END_TIME_MS: i64 = 1_785_148_200_000;
const TARGET_LENGTH_BARS: usize = 48;
const TARGET_BODY_0915_TIME_MS: i64 = 1_785_114_900_000;
const TARGET_BODY_1415_TIME_MS: i64 = 1_785_132_900_000;

struct Args {
    symbols: Vec<String>,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    output: PathBuf,
}

#[derive(Serialize)]
struct ExcludedSymbol {
    symbol: String,
    warmup_expected: usize,
    warmup_loaded: usize,
    evaluation_expected: usize,
    evaluation_loaded: usize,
    reason: &'static str,
}

#[derive(Default, Serialize)]
struct SymbolCounts {
    evaluated_windows: usize,
    baseline_valid_windows: usize,
    candidate_valid_windows: usize,
    shared_valid_windows: usize,
    baseline_only_windows: usize,
    candidate_only_windows: usize,
    changed_shared_boundaries: usize,
    body_expanded_windows: usize,
}

#[derive(Serialize)]
struct ChangedWindowSample {
    symbol: String,
    end_time_ms: i64,
    length_bars: usize,
    change_kind: &'static str,
    baseline_upper: Option<f64>,
    baseline_lower: Option<f64>,
    candidate_upper: Option<f64>,
    candidate_lower: Option<f64>,
    wick_upper: f64,
    wick_lower: f64,
    body_upper: f64,
    body_lower: f64,
    allowed_sweep_wicks_per_side: usize,
    upper_wicks_outside: usize,
    lower_wicks_outside: usize,
}

#[derive(Serialize)]
struct TargetBodyAudit {
    time_ms: i64,
    open: f64,
    close: f64,
    body_upper: f64,
    body_lower: f64,
    included: bool,
}

#[derive(Serialize)]
struct TargetAudit {
    symbol_present: bool,
    end_time_present: bool,
    window_start_time_ms: Option<i64>,
    candidate_valid: bool,
    baseline: Option<StrictVisualRangeEvidence>,
    candidate: Option<StrictVisualRangeEvidence>,
    envelope: Option<StrictVisualBodyWickEnvelope>,
    quality: Option<StrictVisualCandidateQualityAudit>,
    body_0915: Option<TargetBodyAudit>,
    body_1415: Option<TargetBodyAudit>,
    has_upper_sweep_outside: bool,
    overflow_violation: bool,
    passed: bool,
}

#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_boundary: &'static str,
    single_variable: &'static str,
    label_boundary: &'static str,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    evaluated_windows: usize,
    baseline_valid_windows: usize,
    candidate_valid_windows: usize,
    shared_valid_windows: usize,
    baseline_only_windows: usize,
    candidate_only_windows: usize,
    changed_shared_boundaries: usize,
    affected_valid_window_identities: usize,
    affected_ratio: f64,
    candidate_distinct_symbols: usize,
    candidate_hour_event_clusters: usize,
    body_expanded_windows: usize,
    body_expanded_distinct_symbols: usize,
    body_inclusion_violations: usize,
    sweep_allowance_violations: usize,
    target: TargetAudit,
    per_symbol: BTreeMap<String, SymbolCounts>,
    changed_window_samples: Vec<ChangedWindowSample>,
    l1_gate_passed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating V38 warmup start")?;
    let loaded = load_research_window_symbols_from_quant_core(
        &args.symbols,
        warmup_start_ms,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    )
    .await?;
    let dataset_fingerprint_sha256 = dataset_fingerprint(
        &loaded,
        warmup_start_ms,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    );
    let (included, excluded_symbols) = partition_complete_symbols(loaded);
    if included.is_empty() {
        bail!("no symbol has complete warmup and evaluation coverage");
    }

    let variant = StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard;
    let mut per_symbol = BTreeMap::new();
    let mut changed_window_samples = Vec::new();
    let mut candidate_symbols = BTreeSet::new();
    let mut candidate_clusters = BTreeSet::new();
    let mut expanded_symbols = BTreeSet::new();
    let mut body_inclusion_violations = 0;
    let mut sweep_allowance_violations = 0;

    for symbol in &included {
        let counts = per_symbol
            .entry(symbol.symbol.clone())
            .or_insert_with(SymbolCounts::default);
        for (index, candle) in symbol.candles.iter().enumerate() {
            if candle.timestamp_ms < args.evaluation_start_ms {
                continue;
            }
            if candle.timestamp_ms >= args.evaluation_end_exclusive_ms {
                break;
            }
            for length in STRICT_VISUAL_CANDIDATE_LENGTHS {
                if length > index + 1 {
                    continue;
                }
                counts.evaluated_windows += 1;
                let baseline = evaluate_strict_visual_percentile_candidate_for_research(
                    &symbol.candles,
                    index,
                    length,
                    symbol.tick_size,
                    variant,
                );
                let candidate = evaluate_strict_visual_body_wick_candidate_for_research(
                    &symbol.candles,
                    index,
                    length,
                    symbol.tick_size,
                    variant,
                );
                counts.baseline_valid_windows += usize::from(baseline.is_some());
                counts.candidate_valid_windows += usize::from(candidate.is_some());
                match (baseline, candidate) {
                    (Some(base), Some((next, envelope))) => {
                        counts.shared_valid_windows += 1;
                        candidate_symbols.insert(symbol.symbol.as_str());
                        candidate_clusters.insert(candle.timestamp_ms / EVENT_CLUSTER_MS);
                        let boundary_changed =
                            prices_differ(base.upper, next.upper, symbol.tick_size)
                                || prices_differ(base.lower, next.lower, symbol.tick_size);
                        counts.changed_shared_boundaries += usize::from(boundary_changed);
                        let body_expanded = envelope.upper > envelope.wick_upper
                            || envelope.lower < envelope.wick_lower;
                        if body_expanded {
                            counts.body_expanded_windows += 1;
                            expanded_symbols.insert(symbol.symbol.as_str());
                        }
                        audit_contract(
                            history(&symbol.candles, index, length),
                            envelope,
                            &mut body_inclusion_violations,
                            &mut sweep_allowance_violations,
                        );
                        if boundary_changed {
                            push_changed_sample(
                                &mut changed_window_samples,
                                symbol,
                                candle.timestamp_ms,
                                length,
                                "shared_boundary_changed",
                                Some(base),
                                Some(next),
                                envelope,
                            );
                        }
                    }
                    (Some(base), None) => {
                        counts.baseline_only_windows += 1;
                        if let Some(envelope) = envelope_for_window(&symbol.candles, index, length)
                        {
                            push_changed_sample(
                                &mut changed_window_samples,
                                symbol,
                                candle.timestamp_ms,
                                length,
                                "baseline_only",
                                Some(base),
                                None,
                                envelope,
                            );
                        }
                    }
                    (None, Some((next, envelope))) => {
                        counts.candidate_only_windows += 1;
                        counts.body_expanded_windows += usize::from(
                            envelope.upper > envelope.wick_upper
                                || envelope.lower < envelope.wick_lower,
                        );
                        candidate_symbols.insert(symbol.symbol.as_str());
                        candidate_clusters.insert(candle.timestamp_ms / EVENT_CLUSTER_MS);
                        if envelope.upper > envelope.wick_upper
                            || envelope.lower < envelope.wick_lower
                        {
                            expanded_symbols.insert(symbol.symbol.as_str());
                        }
                        audit_contract(
                            history(&symbol.candles, index, length),
                            envelope,
                            &mut body_inclusion_violations,
                            &mut sweep_allowance_violations,
                        );
                        push_changed_sample(
                            &mut changed_window_samples,
                            symbol,
                            candle.timestamp_ms,
                            length,
                            "candidate_only",
                            None,
                            Some(next),
                            envelope,
                        );
                    }
                    (None, None) => {}
                }
            }
        }
    }

    let totals = sum_counts(&per_symbol);
    let affected_valid_window_identities = totals.baseline_only_windows
        + totals.candidate_only_windows
        + totals.changed_shared_boundaries;
    let valid_union =
        totals.shared_valid_windows + totals.baseline_only_windows + totals.candidate_only_windows;
    let affected_ratio = ratio(affected_valid_window_identities, valid_union);
    let target = audit_target(&included, variant);
    let l1_gate_passed = target.passed
        && totals.candidate_valid_windows >= MINIMUM_CANDIDATE_WINDOWS
        && candidate_symbols.len() >= MINIMUM_SYMBOLS
        && candidate_clusters.len() >= MINIMUM_EVENT_CLUSTERS
        && totals.body_expanded_windows >= MINIMUM_EXPANDED_WINDOWS
        && expanded_symbols.len() >= MINIMUM_EXPANDED_SYMBOLS
        && body_inclusion_violations == 0
        && sweep_allowance_violations == 0
        && (MINIMUM_AFFECTED_RATIO..=MAXIMUM_AFFECTED_RATIO).contains(&affected_ratio);

    let report = L1Report {
        schema_version: "strict_visual_v38_body_wick_envelope_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "fixed_completed_candle_windows_only",
        baseline_variant: variant.slug(),
        candidate_boundary:
            "all_body_edges_plus_wick_endpoint_90pct_target_with_minimum_one_sweep_exception",
        single_variable: "fixed_window_boundary_estimator",
        label_boundary:
            "No order, entry, exit, stop, target, MFE, MAE, final R, PnL, or win/loss field was read or serialized.",
        dataset_fingerprint_sha256,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        evaluated_windows: totals.evaluated_windows,
        baseline_valid_windows: totals.baseline_valid_windows,
        candidate_valid_windows: totals.candidate_valid_windows,
        shared_valid_windows: totals.shared_valid_windows,
        baseline_only_windows: totals.baseline_only_windows,
        candidate_only_windows: totals.candidate_only_windows,
        changed_shared_boundaries: totals.changed_shared_boundaries,
        affected_valid_window_identities,
        affected_ratio,
        candidate_distinct_symbols: candidate_symbols.len(),
        candidate_hour_event_clusters: candidate_clusters.len(),
        body_expanded_windows: totals.body_expanded_windows,
        body_expanded_distinct_symbols: expanded_symbols.len(),
        body_inclusion_violations,
        sweep_allowance_violations,
        target,
        per_symbol,
        changed_window_samples,
        l1_gate_passed,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "body_wick_envelope_l1: included={} baseline={} candidate={} affected={:.2}% expanded={} target={} gate={} output={}",
        report.included_symbols,
        report.baseline_valid_windows,
        report.candidate_valid_windows,
        report.affected_ratio * 100.0,
        report.body_expanded_windows,
        report.target.passed,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("V38 body-wick envelope L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

fn history(candles: &[Candle], index: usize, length: usize) -> &[Candle] {
    &candles[index + 1 - length..=index]
}

fn envelope_for_window(
    candles: &[Candle],
    index: usize,
    length: usize,
) -> Option<StrictVisualBodyWickEnvelope> {
    rust_quant_cli::app::tradingview_velocity_parity::strict_visual_body_wick_envelope(history(
        candles, index, length,
    ))
}

fn audit_contract(
    history: &[Candle],
    envelope: StrictVisualBodyWickEnvelope,
    body_inclusion_violations: &mut usize,
    sweep_allowance_violations: &mut usize,
) {
    if history.iter().any(|candle| {
        candle.open.max(candle.close) > envelope.upper
            || candle.open.min(candle.close) < envelope.lower
    }) {
        *body_inclusion_violations += 1;
    }
    if envelope.upper_wicks_outside > envelope.allowed_sweep_wicks_per_side
        || envelope.lower_wicks_outside > envelope.allowed_sweep_wicks_per_side
    {
        *sweep_allowance_violations += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn push_changed_sample(
    samples: &mut Vec<ChangedWindowSample>,
    symbol: &ResearchWindowSymbolCandles,
    end_time_ms: i64,
    length_bars: usize,
    change_kind: &'static str,
    baseline: Option<StrictVisualRangeEvidence>,
    candidate: Option<StrictVisualRangeEvidence>,
    envelope: StrictVisualBodyWickEnvelope,
) {
    if samples.len() >= MAXIMUM_CHANGED_SAMPLES {
        return;
    }
    samples.push(ChangedWindowSample {
        symbol: symbol.symbol.clone(),
        end_time_ms,
        length_bars,
        change_kind,
        baseline_upper: baseline.map(|range| range.upper),
        baseline_lower: baseline.map(|range| range.lower),
        candidate_upper: candidate.map(|range| range.upper),
        candidate_lower: candidate.map(|range| range.lower),
        wick_upper: envelope.wick_upper,
        wick_lower: envelope.wick_lower,
        body_upper: envelope.body_upper,
        body_lower: envelope.body_lower,
        allowed_sweep_wicks_per_side: envelope.allowed_sweep_wicks_per_side,
        upper_wicks_outside: envelope.upper_wicks_outside,
        lower_wicks_outside: envelope.lower_wicks_outside,
    });
}

fn audit_target(
    symbols: &[ResearchWindowSymbolCandles],
    variant: StrictVisualBreakoutResearchVariant,
) -> TargetAudit {
    let Some(symbol) = symbols.iter().find(|symbol| symbol.symbol == TARGET_SYMBOL) else {
        return missing_target(false);
    };
    let Some(end_index) = symbol
        .candles
        .iter()
        .position(|candle| candle.timestamp_ms == TARGET_END_TIME_MS)
    else {
        return missing_target(true);
    };
    let baseline = evaluate_strict_visual_percentile_candidate_for_research(
        &symbol.candles,
        end_index,
        TARGET_LENGTH_BARS,
        symbol.tick_size,
        variant,
    );
    let candidate_pair = evaluate_strict_visual_body_wick_candidate_for_research(
        &symbol.candles,
        end_index,
        TARGET_LENGTH_BARS,
        symbol.tick_size,
        variant,
    );
    let candidate = candidate_pair.map(|pair| pair.0);
    let envelope = candidate_pair
        .map(|pair| pair.1)
        .or_else(|| envelope_for_window(&symbol.candles, end_index, TARGET_LENGTH_BARS));
    let quality = audit_strict_visual_body_wick_candidate_for_research(
        &symbol.candles,
        end_index,
        TARGET_LENGTH_BARS,
        symbol.tick_size,
        variant,
    )
    .map(|pair| pair.1);
    let body_0915 = target_body(symbol, TARGET_BODY_0915_TIME_MS, envelope);
    let body_1415 = target_body(symbol, TARGET_BODY_1415_TIME_MS, envelope);
    let has_upper_sweep_outside = envelope.is_some_and(|value| value.upper_wicks_outside > 0);
    let overflow_violation = envelope.is_none_or(|value| {
        value.upper_wicks_outside > value.allowed_sweep_wicks_per_side
            || value.lower_wicks_outside > value.allowed_sweep_wicks_per_side
    });
    let passed = candidate.is_some()
        && body_0915.as_ref().is_some_and(|body| body.included)
        && body_1415.as_ref().is_some_and(|body| body.included)
        && has_upper_sweep_outside
        && !overflow_violation;
    TargetAudit {
        symbol_present: true,
        end_time_present: true,
        window_start_time_ms: end_index
            .checked_add(1)
            .and_then(|end| end.checked_sub(TARGET_LENGTH_BARS))
            .map(|start| symbol.candles[start].timestamp_ms),
        candidate_valid: candidate.is_some(),
        baseline,
        candidate,
        envelope,
        quality,
        body_0915,
        body_1415,
        has_upper_sweep_outside,
        overflow_violation,
        passed,
    }
}

fn target_body(
    symbol: &ResearchWindowSymbolCandles,
    time_ms: i64,
    envelope: Option<StrictVisualBodyWickEnvelope>,
) -> Option<TargetBodyAudit> {
    let candle = symbol
        .candles
        .iter()
        .find(|candle| candle.timestamp_ms == time_ms)?;
    let body_upper = candle.open.max(candle.close);
    let body_lower = candle.open.min(candle.close);
    Some(TargetBodyAudit {
        time_ms,
        open: candle.open,
        close: candle.close,
        body_upper,
        body_lower,
        included: envelope
            .is_some_and(|bounds| body_upper <= bounds.upper && body_lower >= bounds.lower),
    })
}

fn missing_target(symbol_present: bool) -> TargetAudit {
    TargetAudit {
        symbol_present,
        end_time_present: false,
        window_start_time_ms: None,
        candidate_valid: false,
        baseline: None,
        candidate: None,
        envelope: None,
        quality: None,
        body_0915: None,
        body_1415: None,
        has_upper_sweep_outside: false,
        overflow_violation: true,
        passed: false,
    }
}

fn prices_differ(left: f64, right: f64, tick_size: f64) -> bool {
    (left - right).abs() >= tick_size * 0.5
}

fn sum_counts(per_symbol: &BTreeMap<String, SymbolCounts>) -> SymbolCounts {
    per_symbol
        .values()
        .fold(SymbolCounts::default(), |mut total, current| {
            total.evaluated_windows += current.evaluated_windows;
            total.baseline_valid_windows += current.baseline_valid_windows;
            total.candidate_valid_windows += current.candidate_valid_windows;
            total.shared_valid_windows += current.shared_valid_windows;
            total.baseline_only_windows += current.baseline_only_windows;
            total.candidate_only_windows += current.candidate_only_windows;
            total.changed_shared_boundaries += current.changed_shared_boundaries;
            total.body_expanded_windows += current.body_expanded_windows;
            total
        })
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn partition_complete_symbols(
    loaded: Vec<ResearchWindowSymbolCandles>,
) -> (Vec<ResearchWindowSymbolCandles>, Vec<ExcludedSymbol>) {
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for symbol in loaded {
        if symbol.warmup_coverage.is_complete && symbol.evaluation_coverage.is_complete {
            included.push(symbol);
        } else {
            excluded.push(ExcludedSymbol {
                symbol: symbol.symbol,
                warmup_expected: symbol.warmup_coverage.expected_candle_count,
                warmup_loaded: symbol.warmup_coverage.loaded_candle_count,
                evaluation_expected: symbol.evaluation_coverage.expected_candle_count,
                evaluation_loaded: symbol.evaluation_coverage.loaded_candle_count,
                reason: match (
                    symbol.warmup_coverage.is_complete,
                    symbol.evaluation_coverage.is_complete,
                ) {
                    (false, false) => "warmup_and_evaluation_incomplete",
                    (false, true) => "warmup_incomplete",
                    (true, false) => "evaluation_incomplete",
                    (true, true) => unreachable!("complete member is included"),
                },
            });
        }
    }
    (included, excluded)
}

fn dataset_fingerprint(
    symbols: &[ResearchWindowSymbolCandles],
    warmup_start_ms: i64,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(warmup_start_ms.to_le_bytes());
    hasher.update(evaluation_start_ms.to_le_bytes());
    hasher.update(evaluation_end_exclusive_ms.to_le_bytes());
    for symbol in symbols {
        hasher.update(symbol.symbol.as_bytes());
        hasher.update(symbol.tick_size.to_bits().to_le_bytes());
        for candle in &symbol.candles {
            hasher.update(candle.timestamp_ms.to_le_bytes());
            hasher.update(candle.open.to_bits().to_le_bytes());
            hasher.update(candle.high.to_bits().to_le_bytes());
            hasher.update(candle.low.to_bits().to_le_bytes());
            hasher.update(candle.close.to_bits().to_le_bytes());
            hasher.update(candle.volume.to_bits().to_le_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

fn parse_args(values: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut symbols = None;
    let mut evaluation_start_ms = None;
    let mut evaluation_end_exclusive_ms = None;
    let mut output = None;
    let mut values = values.into_iter();
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--symbols" => {
                symbols = Some(parse_symbols(&values.next().context("missing symbols")?)?)
            }
            "--evaluation-start-ms" => {
                evaluation_start_ms = Some(parse_i64(&values.next().context("missing start")?)?)
            }
            "--evaluation-end-exclusive-ms" => {
                evaluation_end_exclusive_ms =
                    Some(parse_i64(&values.next().context("missing end")?)?)
            }
            "--output" => output = Some(PathBuf::from(values.next().context("missing output")?)),
            other => bail!("unsupported argument: {other}"),
        }
    }
    Ok(Args {
        symbols: symbols.context("missing --symbols")?,
        evaluation_start_ms: evaluation_start_ms.context("missing --evaluation-start-ms")?,
        evaluation_end_exclusive_ms: evaluation_end_exclusive_ms
            .context("missing --evaluation-end-exclusive-ms")?,
        output: output.context("missing --output")?,
    })
}

fn parse_symbols(raw: &str) -> Result<Vec<String>> {
    let symbols = raw
        .split(',')
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>();
    if symbols.is_empty() || symbols.iter().collect::<BTreeSet<_>>().len() != symbols.len() {
        bail!("--symbols must be non-empty and unique");
    }
    Ok(symbols)
}

fn parse_i64(raw: &str) -> Result<i64> {
    raw.parse::<i64>()
        .context("timestamp must be Unix milliseconds")
}

fn write_json_atomically(path: &Path, report: &L1Report) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}
