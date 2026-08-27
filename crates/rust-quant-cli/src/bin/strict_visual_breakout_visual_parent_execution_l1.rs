//! V40 只把冻结视觉父区间用于离区、突破和回踩的 outcome-blind L1 覆盖审计。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core,
    strict_visual_body_wick_envelope, Direction, ParityRuleVersion, ResearchWindowSymbolCandles,
    StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal, StrictVisualLongEntryEvent,
    StrictVisualPriorRangeClearanceState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const HOUR_MS: i64 = 60 * 60 * 1_000;

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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SourceKey {
    symbol: String,
    direction: u8,
    range_start_time_ms: i64,
    range_length_bars: usize,
    breakout_time_ms: i64,
    signal_time_ms: i64,
}

#[derive(Clone, Serialize)]
struct SourceAudit {
    symbol: String,
    direction: Direction,
    range_start_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    range_length_bars: usize,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    core_upper: f64,
    core_lower: f64,
    parent_upper: f64,
    parent_lower: f64,
    execution_boundary: f64,
    allowed_sweep_wicks_per_side: usize,
    upper_wicks_outside: usize,
    lower_wicks_outside: usize,
    parent_contains_core: bool,
    all_bodies_contained: bool,
    breakout_after_boundary_confirmation: bool,
    execution_matches_directional_parent: bool,
}

impl SourceAudit {
    fn key(&self) -> SourceKey {
        SourceKey {
            symbol: self.symbol.clone(),
            direction: u8::from(self.direction == Direction::Long),
            range_start_time_ms: self.range_start_time_ms,
            range_length_bars: self.range_length_bars,
            breakout_time_ms: self.breakout_time_ms,
            signal_time_ms: self.signal_time_ms,
        }
    }
}

#[derive(Serialize)]
struct SourceDifference {
    reason: &'static str,
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    baseline_execution_boundary: Option<f64>,
    candidate_execution_boundary: Option<f64>,
    frozen_parent_boundary: Option<f64>,
}

#[derive(Default)]
struct Scan {
    armed_sources: usize,
    confirmed: Vec<SourceAudit>,
}

#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_variant: &'static str,
    single_variable: &'static str,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    baseline_armed_sources: usize,
    baseline_confirmed_sources: usize,
    candidate_armed_sources: usize,
    candidate_confirmed_sources: usize,
    candidate_to_baseline_ratio: f64,
    candidate_long_sources: usize,
    candidate_short_sources: usize,
    candidate_distinct_symbols: usize,
    candidate_direction_hour_clusters: usize,
    baseline_sources_affected: usize,
    affected_distinct_symbols: usize,
    candidate_added_sources: usize,
    parent_core_containment_violations: usize,
    body_containment_violations: usize,
    sweep_allowance_violations: usize,
    causal_freeze_violations: usize,
    execution_boundary_mismatch_violations: usize,
    l1_gate_passed: bool,
    source_differences: Vec<SourceDifference>,
    candidate_source_ledger: Vec<SourceAudit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating warmup start")?;
    let loaded = load_research_window_symbols_from_quant_core(
        &args.symbols,
        warmup_start_ms,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    )
    .await?;
    let fingerprint = dataset_fingerprint(
        &loaded,
        warmup_start_ms,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    );
    let (included, excluded_symbols) = partition_complete_symbols(loaded);
    if included.is_empty() {
        bail!("no symbol has complete warmup and evaluation coverage");
    }

    let baseline = scan_universe(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
    )?;
    let candidate = scan_universe(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary,
    )?;
    let baseline_by_key = baseline
        .confirmed
        .iter()
        .map(|source| (source.key(), source))
        .collect::<BTreeMap<_, _>>();
    let candidate_by_key = candidate
        .confirmed
        .iter()
        .map(|source| (source.key(), source))
        .collect::<BTreeMap<_, _>>();
    let mut source_differences = Vec::new();
    let mut affected_symbols = BTreeSet::new();
    let mut baseline_sources_affected = 0;
    for (key, baseline_source) in &baseline_by_key {
        let candidate_source = candidate_by_key.get(key).copied();
        let boundary_changed = candidate_source.is_some_and(|source| {
            source.execution_boundary.to_bits() != baseline_source.execution_boundary.to_bits()
        });
        if candidate_source.is_none() || boundary_changed {
            baseline_sources_affected += 1;
            affected_symbols.insert(baseline_source.symbol.as_str());
            source_differences.push(SourceDifference {
                reason: if candidate_source.is_none() {
                    "baseline_source_removed_or_retimed"
                } else {
                    "same_source_parent_boundary_replaced_core"
                },
                symbol: baseline_source.symbol.clone(),
                direction: baseline_source.direction,
                breakout_time_ms: baseline_source.breakout_time_ms,
                signal_time_ms: baseline_source.signal_time_ms,
                baseline_execution_boundary: Some(baseline_source.execution_boundary),
                candidate_execution_boundary: candidate_source.map(|item| item.execution_boundary),
                frozen_parent_boundary: Some(match baseline_source.direction {
                    Direction::Long => baseline_source.parent_upper,
                    Direction::Short => baseline_source.parent_lower,
                }),
            });
        }
    }
    let mut candidate_added_sources = 0;
    for (key, source) in &candidate_by_key {
        if !baseline_by_key.contains_key(key) {
            candidate_added_sources += 1;
            source_differences.push(SourceDifference {
                reason: "candidate_source_added_or_retimed",
                symbol: source.symbol.clone(),
                direction: source.direction,
                breakout_time_ms: source.breakout_time_ms,
                signal_time_ms: source.signal_time_ms,
                baseline_execution_boundary: None,
                candidate_execution_boundary: Some(source.execution_boundary),
                frozen_parent_boundary: Some(match source.direction {
                    Direction::Long => source.parent_upper,
                    Direction::Short => source.parent_lower,
                }),
            });
        }
    }

    let baseline_count = baseline.confirmed.len();
    let candidate_count = candidate.confirmed.len();
    let ratio = if baseline_count == 0 {
        0.0
    } else {
        candidate_count as f64 / baseline_count as f64
    };
    let candidate_long_sources = candidate
        .confirmed
        .iter()
        .filter(|source| source.direction == Direction::Long)
        .count();
    let candidate_short_sources = candidate_count - candidate_long_sources;
    let candidate_symbols = candidate
        .confirmed
        .iter()
        .map(|source| source.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let candidate_clusters = candidate
        .confirmed
        .iter()
        .map(|source| {
            (
                source.signal_time_ms.div_euclid(HOUR_MS),
                source.direction == Direction::Long,
            )
        })
        .collect::<BTreeSet<_>>()
        .len();
    let parent_core_containment_violations = candidate
        .confirmed
        .iter()
        .filter(|source| !source.parent_contains_core)
        .count();
    let body_containment_violations = candidate
        .confirmed
        .iter()
        .filter(|source| !source.all_bodies_contained)
        .count();
    let sweep_allowance_violations = candidate
        .confirmed
        .iter()
        .filter(|source| {
            source.upper_wicks_outside > source.allowed_sweep_wicks_per_side
                || source.lower_wicks_outside > source.allowed_sweep_wicks_per_side
        })
        .count();
    let causal_freeze_violations = candidate
        .confirmed
        .iter()
        .filter(|source| !source.breakout_after_boundary_confirmation)
        .count();
    let execution_boundary_mismatch_violations = candidate
        .confirmed
        .iter()
        .filter(|source| !source.execution_matches_directional_parent)
        .count();
    let invariant_violations = parent_core_containment_violations
        + body_containment_violations
        + sweep_allowance_violations
        + causal_freeze_violations
        + execution_boundary_mismatch_violations;
    let l1_gate_passed = baseline_count > 0
        && (0.10..=1.0).contains(&ratio)
        && candidate_long_sources > 0
        && candidate_short_sources > 0
        && candidate_symbols >= 8
        && candidate_clusters >= 10
        && baseline_sources_affected >= 3
        && invariant_violations == 0;
    let report = L1Report {
        schema_version: "strict_visual_breakout_v40_visual_parent_execution_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "signal_time_fields_only_no_trade_or_outcome_fields",
        baseline_variant: StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard.slug(),
        candidate_variant: StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary
            .slug(),
        single_variable:
            "frozen_visual_parent_replaces_core_only_for_departure_breakout_and_retest",
        dataset_fingerprint_sha256: fingerprint,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        baseline_armed_sources: baseline.armed_sources,
        baseline_confirmed_sources: baseline_count,
        candidate_armed_sources: candidate.armed_sources,
        candidate_confirmed_sources: candidate_count,
        candidate_to_baseline_ratio: ratio,
        candidate_long_sources,
        candidate_short_sources,
        candidate_distinct_symbols: candidate_symbols,
        candidate_direction_hour_clusters: candidate_clusters,
        baseline_sources_affected,
        affected_distinct_symbols: affected_symbols.len(),
        candidate_added_sources,
        parent_core_containment_violations,
        body_containment_violations,
        sweep_allowance_violations,
        causal_freeze_violations,
        execution_boundary_mismatch_violations,
        l1_gate_passed,
        source_differences,
        candidate_source_ledger: candidate.confirmed,
    };
    write_json_atomically(&args.output, &report)
}

fn scan_universe(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Result<Scan> {
    let mut aggregate = Scan::default();
    for symbol in symbols {
        let scan = scan_symbol(
            symbol,
            evaluation_start_ms,
            evaluation_end_exclusive_ms,
            variant,
        )?;
        aggregate.armed_sources += scan.armed_sources;
        aggregate.confirmed.extend(scan.confirmed);
    }
    Ok(aggregate)
}

fn scan_symbol(
    symbol: &ResearchWindowSymbolCandles,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Result<Scan> {
    let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
    let mut state = StrictVisualPriorRangeClearanceState::default();
    let mut scan = Scan::default();
    for (index, candle) in symbol.candles.iter().copied().enumerate() {
        if candle.timestamp_ms >= evaluation_end_exclusive_ms {
            break;
        }
        let point = &indicators.points[index];
        let update = state.update(
            &symbol.candles,
            index,
            symbol.tick_size,
            point.atr14,
            point.volume_event,
            point.filtered_volume_ratio,
            None,
            variant,
        );
        if candle.timestamp_ms < evaluation_start_ms {
            continue;
        }
        if matches!(
            update.event,
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
                | Some(StrictVisualLongEntryEvent::DirectSignal(_))
        ) {
            scan.armed_sources += 1;
        }
        if let Some(source) = update.confirmed {
            scan.confirmed.push(source_audit(symbol, source)?);
        }
    }
    Ok(scan)
}

fn source_audit(
    symbol: &ResearchWindowSymbolCandles,
    source: StrictVisualBreakoutSignal,
) -> Result<SourceAudit> {
    let history = symbol
        .candles
        .get(source.range.start_index..=source.range.boundary_confirmation_index)
        .context("source range history missing")?;
    let mut parent = strict_visual_body_wick_envelope(history)
        .context("source visual parent cannot be computed")?;
    parent.upper = parent.upper.max(source.range.upper);
    parent.lower = parent.lower.min(source.range.lower);
    parent.upper_wicks_outside = history
        .iter()
        .filter(|candle| candle.high > parent.upper)
        .count();
    parent.lower_wicks_outside = history
        .iter()
        .filter(|candle| candle.low < parent.lower)
        .count();
    let all_bodies_contained = history.iter().all(|candle| {
        candle.open.max(candle.close) <= parent.upper
            && candle.open.min(candle.close) >= parent.lower
    });
    let expected_execution = match source.direction {
        Direction::Long => parent.upper,
        Direction::Short => parent.lower,
    };
    Ok(SourceAudit {
        symbol: symbol.symbol.clone(),
        direction: source.direction,
        range_start_time_ms: symbol.candles[source.range.start_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[source.range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: source.range.length_bars,
        breakout_time_ms: symbol.candles[source.breakout_index].timestamp_ms,
        signal_time_ms: symbol.candles[source.signal_index].timestamp_ms,
        core_upper: source.range.upper,
        core_lower: source.range.lower,
        parent_upper: parent.upper,
        parent_lower: parent.lower,
        execution_boundary: source.execution_boundary,
        allowed_sweep_wicks_per_side: parent.allowed_sweep_wicks_per_side,
        upper_wicks_outside: parent.upper_wicks_outside,
        lower_wicks_outside: parent.lower_wicks_outside,
        parent_contains_core: parent.upper >= source.range.upper
            && parent.lower <= source.range.lower,
        all_bodies_contained,
        breakout_after_boundary_confirmation: source.breakout_index
            > source.range.boundary_confirmation_index,
        execution_matches_directional_parent: source.execution_boundary.to_bits()
            == expected_execution.to_bits(),
    })
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
                    (true, true) => unreachable!(),
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
