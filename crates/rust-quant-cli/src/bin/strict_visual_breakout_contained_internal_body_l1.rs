//! V42 区间内大实体豁免的 outcome-blind L1 覆盖扫描。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core, Candle, Direction,
    ParityRuleVersion, ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant,
    StrictVisualBreakoutSignal, StrictVisualLongEntryEvent, StrictVisualLongEntryState,
    StrictVisualPriorRangeClearanceState, StrictVisualRangeEvent, StrictVisualRangeEvidence,
    STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_TIME_MS: i64 = 1_785_899_700_000;
const TARGET_RANGE_START_MS: i64 = 1_785_854_700_000;
const TARGET_BOUNDARY_CONFIRMATION_MS: i64 = 1_785_897_000_000;
const TARGET_RANGE_LENGTH: usize = 48;
const TARGET_UPPER: f64 = 64_459.0;
const TARGET_LOWER: f64 = 63_891.0;

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

#[derive(Clone, Serialize)]
struct RangeAudit {
    start_time_ms: i64,
    first_confirmation_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    length_bars: usize,
    upper: f64,
    lower: f64,
}

#[derive(Default, Serialize)]
struct TargetSnapshot {
    before: Option<RangeAudit>,
    after: Option<RangeAudit>,
}

#[derive(Clone, Serialize)]
struct ResetAudit {
    symbol: String,
    reset_time_ms: i64,
    range: RangeAudit,
    candle_open: f64,
    candle_high: f64,
    candle_low: f64,
    candle_close: f64,
    body_to_range_height: f64,
    fully_contained: bool,
}

#[derive(Clone, Serialize)]
struct SourceAudit {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    range_start_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    range_length_bars: usize,
}

struct VariantScan {
    resets: Vec<ResetAudit>,
    sources: Vec<SourceAudit>,
    target: TargetSnapshot,
}

#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_variant: &'static str,
    single_variable: &'static str,
    threshold: f64,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    target_symbol: &'static str,
    target_time_ms: i64,
    target_baseline_reset_present: bool,
    target_baseline_reset_fully_contained: bool,
    target_candidate_reset_absent: bool,
    target_candidate_range_continues: bool,
    baseline_target: TargetSnapshot,
    candidate_target: TargetSnapshot,
    baseline_resets: usize,
    candidate_resets: usize,
    exempted_contained_resets: usize,
    exempted_distinct_symbols: usize,
    retained_boundary_piercing_resets: usize,
    candidate_contained_reset_violations: usize,
    baseline_confirmed_sources: usize,
    candidate_confirmed_sources: usize,
    baseline_only_confirmed_identities: usize,
    candidate_only_confirmed_identities: usize,
    changed_confirmed_identities: usize,
    changed_identity_ratio_to_baseline: f64,
    l1_gate_passed: bool,
    exempted_reset_ledger: Vec<ResetAudit>,
    candidate_reset_ledger: Vec<ResetAudit>,
    baseline_source_ledger: Vec<SourceAudit>,
    candidate_source_ledger: Vec<SourceAudit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating V42 warmup start")?;
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

    let baseline_variant = StrictVisualBreakoutResearchVariant::V41NestedChildRange;
    let candidate_variant = StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption;
    let baseline = scan_variant(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        baseline_variant,
    );
    let candidate = scan_variant(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        candidate_variant,
    );

    let candidate_reset_identities = reset_identities(&candidate.resets);
    let exempted_reset_ledger = baseline
        .resets
        .iter()
        .filter(|reset| {
            reset.fully_contained && !candidate_reset_identities.contains(&reset_identity(reset))
        })
        .cloned()
        .collect::<Vec<_>>();
    let exempted_distinct_symbols = exempted_reset_ledger
        .iter()
        .map(|reset| reset.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let retained_boundary_piercing_resets = candidate
        .resets
        .iter()
        .filter(|reset| !reset.fully_contained)
        .count();
    let candidate_contained_reset_violations = candidate
        .resets
        .iter()
        .filter(|reset| reset.fully_contained)
        .count();

    let baseline_identities = source_identities(&baseline.sources);
    let candidate_identities = source_identities(&candidate.sources);
    let baseline_only = baseline_identities
        .difference(&candidate_identities)
        .count();
    let candidate_only = candidate_identities
        .difference(&baseline_identities)
        .count();
    let changed_confirmed_identities = baseline_only + candidate_only;
    let changed_identity_ratio_to_baseline = if baseline.sources.is_empty() {
        0.0
    } else {
        changed_confirmed_identities as f64 / baseline.sources.len() as f64
    };

    let target_baseline_reset = baseline.resets.iter().find(|reset| is_target_reset(reset));
    let target_candidate_reset_absent = candidate
        .resets
        .iter()
        .all(|reset| reset.reset_time_ms != TARGET_TIME_MS || !same_target_range(&reset.range));
    let target_candidate_range_continues = candidate
        .target
        .before
        .as_ref()
        .zip(candidate.target.after.as_ref())
        .is_some_and(|(before, after)| same_range_source(before, after));
    let target_baseline_reset_present = target_baseline_reset.is_some();
    let target_baseline_reset_fully_contained =
        target_baseline_reset.is_some_and(|reset| reset.fully_contained);
    let l1_gate_passed = target_baseline_reset_present
        && target_baseline_reset_fully_contained
        && target_candidate_reset_absent
        && target_candidate_range_continues
        && exempted_reset_ledger.len() >= 5
        && exempted_distinct_symbols >= 3
        && retained_boundary_piercing_resets >= 3
        && candidate_contained_reset_violations == 0
        && changed_confirmed_identities >= 3
        && changed_identity_ratio_to_baseline <= 0.50;

    let report = L1Report {
        schema_version: "strict_visual_v42_contained_internal_body_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "signal_time_fields_only_no_trade_or_outcome_fields",
        baseline_variant: baseline_variant.slug(),
        candidate_variant: candidate_variant.slug(),
        single_variable:
            "body_ge_0_50_range_height_resets_only_when_high_or_low_pierces_trade_core",
        threshold: STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT,
        dataset_fingerprint_sha256: fingerprint,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        target_symbol: TARGET_SYMBOL,
        target_time_ms: TARGET_TIME_MS,
        target_baseline_reset_present,
        target_baseline_reset_fully_contained,
        target_candidate_reset_absent,
        target_candidate_range_continues,
        baseline_target: baseline.target,
        candidate_target: candidate.target,
        baseline_resets: baseline.resets.len(),
        candidate_resets: candidate.resets.len(),
        exempted_contained_resets: exempted_reset_ledger.len(),
        exempted_distinct_symbols,
        retained_boundary_piercing_resets,
        candidate_contained_reset_violations,
        baseline_confirmed_sources: baseline.sources.len(),
        candidate_confirmed_sources: candidate.sources.len(),
        baseline_only_confirmed_identities: baseline_only,
        candidate_only_confirmed_identities: candidate_only,
        changed_confirmed_identities,
        changed_identity_ratio_to_baseline,
        l1_gate_passed,
        exempted_reset_ledger,
        candidate_reset_ledger: candidate.resets,
        baseline_source_ledger: baseline.sources,
        candidate_source_ledger: candidate.sources,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "contained_internal_body_l1: included={} resets={}->{} exempted={} symbols={} retained={} changed_sources={} ratio={:.4} target_reset={} target_contained={} target_continues={} violations={} gate={}",
        report.included_symbols,
        report.baseline_resets,
        report.candidate_resets,
        report.exempted_contained_resets,
        report.exempted_distinct_symbols,
        report.retained_boundary_piercing_resets,
        report.changed_confirmed_identities,
        report.changed_identity_ratio_to_baseline,
        report.target_baseline_reset_present,
        report.target_baseline_reset_fully_contained,
        report.target_candidate_range_continues,
        report.candidate_contained_reset_violations,
        report.l1_gate_passed,
    );
    if !report.l1_gate_passed {
        bail!("V42 L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

fn scan_variant(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> VariantScan {
    let mut resets = Vec::new();
    let mut sources = Vec::new();
    let mut target = TargetSnapshot::default();
    for symbol in symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut structure = StrictVisualLongEntryState::default();
        let mut trading = StrictVisualPriorRangeClearanceState::default();
        let mut active = None;
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let point = &indicators.points[index];
            if point.atr14.is_none_or(|atr| atr <= 0.0) {
                continue;
            }
            if symbol.symbol == TARGET_SYMBOL && candle.timestamp_ms == TARGET_TIME_MS {
                target.before = active.map(|range| range_audit(symbol, range));
            }
            let structure_event = structure.update(
                &symbol.candles,
                index,
                symbol.tick_size,
                point.atr14,
                point.volume_event,
                point.filtered_volume_ratio,
                None,
                variant,
            );
            if let Some(event) = structure_event {
                update_active_range(event, &mut active);
                if let StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::InternalImpulseReset(reset),
                ) = event
                {
                    if candle.timestamp_ms >= evaluation_start_ms {
                        resets.push(reset_audit(
                            symbol,
                            candle,
                            reset.range,
                            reset.body_to_range_height,
                        ));
                    }
                }
            }
            if symbol.symbol == TARGET_SYMBOL && candle.timestamp_ms == TARGET_TIME_MS {
                target.after = active.map(|range| range_audit(symbol, range));
            }

            let trade_update = trading.update(
                &symbol.candles,
                index,
                symbol.tick_size,
                point.atr14,
                point.volume_event,
                point.filtered_volume_ratio,
                None,
                variant,
            );
            if candle.timestamp_ms >= evaluation_start_ms {
                if let Some(signal) = trade_update.confirmed {
                    sources.push(source_audit(symbol, signal));
                }
            }
        }
    }
    VariantScan {
        resets,
        sources,
        target,
    }
}

fn update_active_range(
    event: StrictVisualLongEntryEvent,
    active: &mut Option<StrictVisualRangeEvidence>,
) {
    match event {
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(range))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(range)) => {
            *active = Some(range);
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDeparturePending(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureReturned(_)) => {}
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureConsumed(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::InternalImpulseReset(_))
        | StrictVisualLongEntryEvent::AcceptanceArmed(_)
        | StrictVisualLongEntryEvent::DirectSignal(_) => *active = None,
        StrictVisualLongEntryEvent::AcceptanceConfirmed(_)
        | StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(_)
        | StrictVisualLongEntryEvent::AcceptanceMarginRejected(_)
        | StrictVisualLongEntryEvent::ExternalStructureRejected(_)
        | StrictVisualLongEntryEvent::AcceptanceInvalidated(_)
        | StrictVisualLongEntryEvent::AcceptanceExpired(_) => {}
    }
}

fn reset_audit(
    symbol: &ResearchWindowSymbolCandles,
    candle: Candle,
    range: StrictVisualRangeEvidence,
    body_to_range_height: f64,
) -> ResetAudit {
    ResetAudit {
        symbol: symbol.symbol.clone(),
        reset_time_ms: candle.timestamp_ms,
        range: range_audit(symbol, range),
        candle_open: candle.open,
        candle_high: candle.high,
        candle_low: candle.low,
        candle_close: candle.close,
        body_to_range_height,
        fully_contained: candle.high <= range.upper && candle.low >= range.lower,
    }
}

fn range_audit(
    symbol: &ResearchWindowSymbolCandles,
    range: StrictVisualRangeEvidence,
) -> RangeAudit {
    RangeAudit {
        start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
    }
}

fn source_audit(
    symbol: &ResearchWindowSymbolCandles,
    signal: StrictVisualBreakoutSignal,
) -> SourceAudit {
    SourceAudit {
        symbol: symbol.symbol.clone(),
        direction: signal.direction,
        breakout_time_ms: symbol.candles[signal.breakout_index].timestamp_ms,
        signal_time_ms: symbol.candles[signal.signal_index].timestamp_ms,
        range_start_time_ms: symbol.candles[signal.range.start_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[signal.range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: signal.range.length_bars,
    }
}

type ResetIdentity = (String, i64, i64, i64, usize);

fn reset_identity(reset: &ResetAudit) -> ResetIdentity {
    (
        reset.symbol.clone(),
        reset.reset_time_ms,
        reset.range.start_time_ms,
        reset.range.boundary_confirmation_time_ms,
        reset.range.length_bars,
    )
}

fn reset_identities(resets: &[ResetAudit]) -> BTreeSet<ResetIdentity> {
    resets.iter().map(reset_identity).collect()
}

type SourceIdentity = (String, u8, i64, i64, i64, i64, usize);

fn source_identities(sources: &[SourceAudit]) -> BTreeSet<SourceIdentity> {
    sources
        .iter()
        .map(|source| {
            (
                source.symbol.clone(),
                match source.direction {
                    Direction::Long => 0,
                    Direction::Short => 1,
                },
                source.breakout_time_ms,
                source.signal_time_ms,
                source.range_start_time_ms,
                source.boundary_confirmation_time_ms,
                source.range_length_bars,
            )
        })
        .collect()
}

fn is_target_reset(reset: &ResetAudit) -> bool {
    reset.symbol == TARGET_SYMBOL
        && reset.reset_time_ms == TARGET_TIME_MS
        && same_target_range(&reset.range)
}

fn same_target_range(range: &RangeAudit) -> bool {
    range.start_time_ms == TARGET_RANGE_START_MS
        && range.boundary_confirmation_time_ms == TARGET_BOUNDARY_CONFIRMATION_MS
        && range.length_bars == TARGET_RANGE_LENGTH
        && range.upper.to_bits() == TARGET_UPPER.to_bits()
        && range.lower.to_bits() == TARGET_LOWER.to_bits()
}

fn same_range_source(before: &RangeAudit, after: &RangeAudit) -> bool {
    before.start_time_ms == after.start_time_ms
        && before.first_confirmation_time_ms == after.first_confirmation_time_ms
        && before.upper.to_bits() == after.upper.to_bits()
        && before.lower.to_bits() == after.lower.to_bits()
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
