//! V41 父区间内局部盘整的 outcome-blind L1 覆盖扫描。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, evaluate_strict_visual_dual_boundary_candidate_for_research,
    load_research_window_symbols_from_quant_core, strict_visual_body_wick_envelope,
    strict_visual_breakout_qualifies_for_range, Candle, Direction, ParityRuleVersion,
    ResearchWindowSymbolCandles, StrictVisualBodyWickEnvelope, StrictVisualBreakoutResearchVariant,
    StrictVisualBreakoutSignal, StrictVisualLongEntryEvent, StrictVisualPriorRangeClearanceState,
    StrictVisualRangeEvent, StrictVisualRangeEvidence, STRICT_VISUAL_CANDIDATE_LENGTHS,
    STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT, STRICT_VISUAL_RETAINED_ACCEPTANCE_WINDOW_BARS,
    STRICT_VISUAL_RETAINED_BREAKOUT_EXCESS_RATIO,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const HOUR_MS: i64 = 60 * 60 * 1_000;
const RETEST_BAND_PRICE_RATIO: f64 = 0.001;
const RETEST_BAND_ATR_RATIO: f64 = 0.25;
const MINIMUM_ACCEPTANCE_RETENTION_RATIO: f64 = 0.75;
const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_START_MIN_MS: i64 = 1_786_059_000_000;
const TARGET_START_MAX_MS: i64 = 1_786_062_600_000;
const TARGET_CONFIRM_MIN_MS: i64 = 1_786_068_000_000;
const TARGET_CONFIRM_MAX_MS: i64 = 1_786_091_400_000;

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

#[derive(Clone, Copy)]
struct ParentContext {
    range: StrictVisualRangeEvidence,
    visual: StrictVisualBodyWickEnvelope,
}

#[derive(Clone, Copy)]
struct WeakDeparture {
    range: StrictVisualRangeEvidence,
    departure_index: usize,
}

#[derive(Default)]
struct ChildTracker {
    active: Option<StrictVisualRangeEvidence>,
    pending_weak_departure: Option<WeakDeparture>,
    pending_acceptance: Option<StrictVisualBreakoutSignal>,
    last_closed_index: Option<usize>,
}

#[derive(Clone, Serialize)]
struct ChildRangeAudit {
    symbol: String,
    parent_start_time_ms: i64,
    parent_confirmation_time_ms: i64,
    parent_length_bars: usize,
    parent_core_upper: f64,
    parent_core_lower: f64,
    parent_visual_upper: f64,
    parent_visual_lower: f64,
    child_start_time_ms: i64,
    child_confirmation_time_ms: i64,
    child_length_bars: usize,
    child_upper: f64,
    child_lower: f64,
    starts_after_parent: bool,
    confirms_after_parent: bool,
    child_inside_parent_visual: bool,
    differs_from_parent_core_by_tick: bool,
}

#[derive(Serialize)]
struct ChildSourceAudit {
    symbol: String,
    direction: Direction,
    parent_start_time_ms: i64,
    child_start_time_ms: i64,
    child_confirmation_time_ms: i64,
    child_length_bars: usize,
    child_upper: f64,
    child_lower: f64,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    execution_boundary: f64,
    breakout_excess: f64,
    required_acceptance_close_25pct: f64,
    actual_retention_ratio: f64,
    source_atr: f64,
    parent_priority_preserved: bool,
    breakout_after_child_confirmation: bool,
}

#[derive(Default)]
struct Scan {
    parent_confirmed_sources: usize,
    child_ranges: Vec<ChildRangeAudit>,
    child_armed_sources: usize,
    child_confirmed_sources: Vec<ChildSourceAudit>,
    parent_priority_cancellations: usize,
    parent_priority_violations: usize,
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
    baseline_parent_confirmed_sources: usize,
    child_ranges_confirmed: usize,
    child_armed_sources: usize,
    child_confirmed_sources: usize,
    child_to_parent_source_ratio: f64,
    child_long_sources: usize,
    child_short_sources: usize,
    child_distinct_symbols: usize,
    child_direction_hour_clusters: usize,
    parent_priority_cancellations: usize,
    target_range_matched: bool,
    target_ranges: Vec<ChildRangeAudit>,
    containment_violations: usize,
    causal_confirmation_violations: usize,
    duplicate_parent_identity_violations: usize,
    breakout_causality_violations: usize,
    parent_priority_violations: usize,
    l1_gate_passed: bool,
    child_range_ledger: Vec<ChildRangeAudit>,
    child_source_ledger: Vec<ChildSourceAudit>,
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

    let mut scan = Scan::default();
    for symbol in &included {
        let symbol_scan = scan_symbol(
            symbol,
            args.evaluation_start_ms,
            args.evaluation_end_exclusive_ms,
        )?;
        scan.parent_confirmed_sources += symbol_scan.parent_confirmed_sources;
        scan.child_ranges.extend(symbol_scan.child_ranges);
        scan.child_armed_sources += symbol_scan.child_armed_sources;
        scan.child_confirmed_sources
            .extend(symbol_scan.child_confirmed_sources);
        scan.parent_priority_cancellations += symbol_scan.parent_priority_cancellations;
        scan.parent_priority_violations += symbol_scan.parent_priority_violations;
    }

    let child_source_count = scan.child_confirmed_sources.len();
    let child_to_parent_source_ratio = if scan.parent_confirmed_sources == 0 {
        0.0
    } else {
        child_source_count as f64 / scan.parent_confirmed_sources as f64
    };
    let child_long_sources = scan
        .child_confirmed_sources
        .iter()
        .filter(|source| source.direction == Direction::Long)
        .count();
    let child_short_sources = child_source_count - child_long_sources;
    let child_distinct_symbols = scan
        .child_confirmed_sources
        .iter()
        .map(|source| source.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let child_direction_hour_clusters = scan
        .child_confirmed_sources
        .iter()
        .map(|source| {
            (
                source.signal_time_ms.div_euclid(HOUR_MS),
                source.direction == Direction::Long,
            )
        })
        .collect::<BTreeSet<_>>()
        .len();
    let target_ranges = scan
        .child_ranges
        .iter()
        .filter(|range| {
            range.symbol == TARGET_SYMBOL
                && (TARGET_START_MIN_MS..=TARGET_START_MAX_MS).contains(&range.child_start_time_ms)
                && (TARGET_CONFIRM_MIN_MS..=TARGET_CONFIRM_MAX_MS)
                    .contains(&range.child_confirmation_time_ms)
        })
        .cloned()
        .collect::<Vec<_>>();
    let containment_violations = scan
        .child_ranges
        .iter()
        .filter(|range| !range.child_inside_parent_visual)
        .count();
    let causal_confirmation_violations = scan
        .child_ranges
        .iter()
        .filter(|range| !range.starts_after_parent || !range.confirms_after_parent)
        .count();
    let duplicate_parent_identity_violations = scan
        .child_ranges
        .iter()
        .filter(|range| !range.differs_from_parent_core_by_tick)
        .count();
    let breakout_causality_violations = scan
        .child_confirmed_sources
        .iter()
        .filter(|source| !source.breakout_after_child_confirmation)
        .count();
    let invariant_violations = containment_violations
        + causal_confirmation_violations
        + duplicate_parent_identity_violations
        + breakout_causality_violations
        + scan.parent_priority_violations;
    let target_range_matched = !target_ranges.is_empty();
    let l1_gate_passed = target_range_matched
        && child_source_count >= 10
        && child_long_sources > 0
        && child_short_sources > 0
        && child_distinct_symbols >= 3
        && child_direction_hour_clusters >= 3
        && (0.02..=0.50).contains(&child_to_parent_source_ratio)
        && invariant_violations == 0;
    let report = L1Report {
        schema_version: "strict_visual_breakout_v41_nested_child_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "signal_time_fields_only_no_trade_or_outcome_fields",
        baseline_variant: StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard.slug(),
        candidate_variant: "v41_nested_child_range",
        single_variable: "allow_one_valid_later_starting_child_range_inside_active_parent",
        dataset_fingerprint_sha256: fingerprint,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        baseline_parent_confirmed_sources: scan.parent_confirmed_sources,
        child_ranges_confirmed: scan.child_ranges.len(),
        child_armed_sources: scan.child_armed_sources,
        child_confirmed_sources: child_source_count,
        child_to_parent_source_ratio,
        child_long_sources,
        child_short_sources,
        child_distinct_symbols,
        child_direction_hour_clusters,
        parent_priority_cancellations: scan.parent_priority_cancellations,
        target_range_matched,
        target_ranges,
        containment_violations,
        causal_confirmation_violations,
        duplicate_parent_identity_violations,
        breakout_causality_violations,
        parent_priority_violations: scan.parent_priority_violations,
        l1_gate_passed,
        child_range_ledger: scan.child_ranges,
        child_source_ledger: scan.child_confirmed_sources,
    };
    write_json_atomically(&args.output, &report)
}

fn scan_symbol(
    symbol: &ResearchWindowSymbolCandles,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
) -> Result<Scan> {
    let variant = StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard;
    let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
    let mut parent_state = StrictVisualPriorRangeClearanceState::default();
    let mut parent = None;
    let mut child = ChildTracker::default();
    let mut scan = Scan::default();
    for (index, candle) in symbol.candles.iter().copied().enumerate() {
        if candle.timestamp_ms >= evaluation_end_exclusive_ms {
            break;
        }
        let point = &indicators.points[index];
        let parent_update = parent_state.update(
            &symbol.candles,
            index,
            symbol.tick_size,
            point.atr14,
            point.volume_event,
            point.filtered_volume_ratio,
            None,
            variant,
        );
        if candle.timestamp_ms >= evaluation_start_ms && parent_update.confirmed.is_some() {
            scan.parent_confirmed_sources += 1;
        }
        let parent_event = parent_update.event;
        let parent_took_priority = matches!(
            parent_event,
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
                | Some(StrictVisualLongEntryEvent::DirectSignal(_))
        );
        if parent_took_priority && child.has_pending_source() {
            scan.parent_priority_cancellations += 1;
        }
        let parent_structure_changed =
            apply_parent_event(&symbol.candles, parent_event, &mut parent, &mut child)?;
        if parent_structure_changed || parent_took_priority {
            continue;
        }
        let Some(parent_context) = parent else {
            child.reset();
            continue;
        };
        let child_update = child.update(
            &symbol.candles,
            index,
            symbol.tick_size,
            point.atr14,
            parent_context,
        );
        if candle.timestamp_ms < evaluation_start_ms {
            continue;
        }
        if let Some(range) = child_update.confirmed_range {
            scan.child_ranges
                .push(range_audit(symbol, parent_context, range, symbol.tick_size));
        }
        if child_update.armed_source.is_some() {
            scan.child_armed_sources += 1;
        }
        if let Some(source) = child_update.confirmed_source {
            scan.child_confirmed_sources
                .push(source_audit(symbol, parent_context, source));
        }
    }
    Ok(scan)
}

fn apply_parent_event(
    candles: &[Candle],
    event: Option<StrictVisualLongEntryEvent>,
    parent: &mut Option<ParentContext>,
    child: &mut ChildTracker,
) -> Result<bool> {
    let Some(event) = event else {
        return Ok(false);
    };
    match event {
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(range))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(range)) => {
            *parent = Some(parent_context(candles, range)?);
            child.reset();
            Ok(true)
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureReturned(_)) => {
            Ok(true)
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDeparturePending(_)) => {
            child.reset();
            Ok(true)
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureConsumed(_))
        | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::InternalImpulseReset(_))
        | StrictVisualLongEntryEvent::AcceptanceArmed(_)
        | StrictVisualLongEntryEvent::DirectSignal(_) => {
            *parent = None;
            child.reset();
            Ok(true)
        }
        StrictVisualLongEntryEvent::AcceptanceConfirmed(_)
        | StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(_)
        | StrictVisualLongEntryEvent::AcceptanceMarginRejected(_)
        | StrictVisualLongEntryEvent::ExternalStructureRejected(_)
        | StrictVisualLongEntryEvent::AcceptanceInvalidated(_)
        | StrictVisualLongEntryEvent::AcceptanceExpired(_) => Ok(true),
    }
}

fn parent_context(candles: &[Candle], range: StrictVisualRangeEvidence) -> Result<ParentContext> {
    let history = candles
        .get(range.start_index..=range.boundary_confirmation_index)
        .context("parent range history missing")?;
    let mut visual = strict_visual_body_wick_envelope(history)
        .context("parent visual envelope cannot be computed")?;
    visual.upper = visual.upper.max(range.upper);
    visual.lower = visual.lower.min(range.lower);
    Ok(ParentContext { range, visual })
}

#[derive(Default)]
struct ChildUpdate {
    confirmed_range: Option<StrictVisualRangeEvidence>,
    armed_source: Option<StrictVisualBreakoutSignal>,
    confirmed_source: Option<StrictVisualBreakoutSignal>,
}

impl ChildTracker {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn has_pending_source(&self) -> bool {
        self.pending_acceptance.is_some()
    }

    fn update(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        source_atr: Option<f64>,
        parent: ParentContext,
    ) -> ChildUpdate {
        let Some(candle) = candles
            .get(index)
            .copied()
            .filter(|candle| candle.is_valid())
        else {
            return ChildUpdate::default();
        };
        if let Some(source) = self.pending_acceptance.take() {
            return self.resolve_acceptance(candle, index, source);
        }
        if let Some(pending) = self.pending_weak_departure.take() {
            if index <= pending.departure_index {
                self.pending_weak_departure = Some(pending);
                return ChildUpdate::default();
            }
            if candle.close >= pending.range.lower && candle.close <= pending.range.upper {
                self.active = Some(pending.range);
            } else {
                self.active = None;
                self.last_closed_index = Some(pending.departure_index);
            }
            return ChildUpdate::default();
        }
        if let Some(active) = self.active {
            let direction = if candle.close > active.upper {
                Some(Direction::Long)
            } else if candle.close < active.lower {
                Some(Direction::Short)
            } else {
                None
            };
            if let Some(direction) = direction {
                if !strict_visual_breakout_qualifies_for_range(
                    candle,
                    direction,
                    active.upper - active.lower,
                    StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
                ) {
                    self.pending_weak_departure = Some(WeakDeparture {
                        range: active,
                        departure_index: index,
                    });
                    return ChildUpdate::default();
                }
                self.active = None;
                self.last_closed_index = Some(index);
                let Some(source_atr) = source_atr.filter(|value| *value > 0.0) else {
                    return ChildUpdate::default();
                };
                let source = build_source(candle, index, active, direction, source_atr, tick_size);
                self.pending_acceptance = Some(source);
                return ChildUpdate {
                    armed_source: Some(source),
                    ..ChildUpdate::default()
                };
            }
            let height = active.upper - active.lower;
            if height <= 0.0
                || (candle.close - candle.open).abs() / height
                    >= STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT
            {
                self.active = None;
                self.last_closed_index = Some(index);
                return ChildUpdate::default();
            }
            if let Some(candidate) =
                longest_nested_candidate(candles, index, tick_size, parent, self.last_closed_index)
            {
                let overlaps = candidate.upper >= active.lower && candidate.lower <= active.upper;
                if overlaps && candidate.start_index < active.start_index {
                    self.active = Some(StrictVisualRangeEvidence {
                        first_confirmation_index: active.first_confirmation_index,
                        ..candidate
                    });
                }
            }
            return ChildUpdate::default();
        }
        let Some(candidate) =
            longest_nested_candidate(candles, index, tick_size, parent, self.last_closed_index)
        else {
            return ChildUpdate::default();
        };
        self.active = Some(candidate);
        ChildUpdate {
            confirmed_range: Some(candidate),
            ..ChildUpdate::default()
        }
    }

    fn resolve_acceptance(
        &mut self,
        candle: Candle,
        index: usize,
        source: StrictVisualBreakoutSignal,
    ) -> ChildUpdate {
        let age = index.saturating_sub(source.breakout_index);
        let invalidated = match source.direction {
            Direction::Long => candle.close <= source.execution_boundary,
            Direction::Short => candle.close >= source.execution_boundary,
        };
        if invalidated {
            return ChildUpdate::default();
        }
        let touched = match source.direction {
            Direction::Long => candle.low <= source.execution_boundary + source.retest_band,
            Direction::Short => candle.high >= source.execution_boundary - source.retest_band,
        };
        let retained_25 = match source.direction {
            Direction::Long => candle.close >= source.required_acceptance_close,
            Direction::Short => candle.close <= source.required_acceptance_close,
        };
        if (1..=STRICT_VISUAL_RETAINED_ACCEPTANCE_WINDOW_BARS).contains(&age)
            && touched
            && retained_25
        {
            let retained =
                directional_distance(source.direction, source.execution_boundary, candle.close);
            if source.breakout_excess > 0.0
                && retained / source.breakout_excess + 1e-12 >= MINIMUM_ACCEPTANCE_RETENTION_RATIO
            {
                return ChildUpdate {
                    confirmed_source: Some(StrictVisualBreakoutSignal {
                        signal_index: index,
                        ..source
                    }),
                    ..ChildUpdate::default()
                };
            }
            return ChildUpdate::default();
        }
        if age >= STRICT_VISUAL_RETAINED_ACCEPTANCE_WINDOW_BARS {
            return ChildUpdate::default();
        }
        self.pending_acceptance = Some(source);
        ChildUpdate::default()
    }
}

fn longest_nested_candidate(
    candles: &[Candle],
    index: usize,
    tick_size: f64,
    parent: ParentContext,
    last_closed_index: Option<usize>,
) -> Option<StrictVisualRangeEvidence> {
    if index <= parent.range.first_confirmation_index {
        return None;
    }
    let mut selected = None;
    for length in STRICT_VISUAL_CANDIDATE_LENGTHS {
        if length > index + 1 {
            continue;
        }
        let Some(dual) = evaluate_strict_visual_dual_boundary_candidate_for_research(
            candles,
            index,
            length,
            tick_size,
            StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
        ) else {
            continue;
        };
        let candidate = dual.trade_core;
        let causal = candidate.start_index > parent.range.start_index
            && candidate.first_confirmation_index > parent.range.first_confirmation_index
            && last_closed_index.is_none_or(|closed| candidate.start_index > closed);
        let contained =
            candidate.upper <= parent.visual.upper && candidate.lower >= parent.visual.lower;
        let distinct = (candidate.upper - parent.range.upper).abs() >= tick_size
            || (candidate.lower - parent.range.lower).abs() >= tick_size;
        if causal && contained && distinct {
            selected = Some(candidate);
        }
    }
    selected
}

fn build_source(
    candle: Candle,
    index: usize,
    range: StrictVisualRangeEvidence,
    direction: Direction,
    source_atr: f64,
    tick_size: f64,
) -> StrictVisualBreakoutSignal {
    let boundary = match direction {
        Direction::Long => range.upper,
        Direction::Short => range.lower,
    };
    let breakout_excess = directional_distance(direction, boundary, candle.close);
    let retained_excess =
        tick_size.max(breakout_excess * STRICT_VISUAL_RETAINED_BREAKOUT_EXCESS_RATIO);
    let required_acceptance_close = match direction {
        Direction::Long => boundary + retained_excess,
        Direction::Short => boundary - retained_excess,
    };
    let height = range.upper - range.lower;
    StrictVisualBreakoutSignal {
        range,
        execution_boundary: boundary,
        direction,
        breakout_index: index,
        signal_index: index,
        breakout_open: candle.open,
        breakout_close: candle.close,
        breakout_candle_extreme_stop_price: Some(match direction {
            Direction::Long => round_down(candle.low - tick_size, tick_size),
            Direction::Short => round_up(candle.high + tick_size, tick_size),
        }),
        breakout_candle_extreme_stop_min_atr_multiple: Some(1.0),
        breakout_body_midpoint: (candle.open + candle.close) * 0.5,
        source_atr,
        source_volume_ratio: 0.0,
        source_take_profit_atr: 0.0,
        volume_gate_applied: false,
        short_range_one_r_target: false,
        retest_band: (boundary.abs() * RETEST_BAND_PRICE_RATIO)
            .max(source_atr * RETEST_BAND_ATR_RATIO),
        breakout_excess,
        required_acceptance_close,
        measured_move_target_price: Some(match direction {
            Direction::Long => range.upper + height,
            Direction::Short => range.lower - height,
        }),
        external_structure: None,
    }
}

fn directional_distance(direction: Direction, boundary: f64, price: f64) -> f64 {
    match direction {
        Direction::Long => price - boundary,
        Direction::Short => boundary - price,
    }
}

fn range_audit(
    symbol: &ResearchWindowSymbolCandles,
    parent: ParentContext,
    child: StrictVisualRangeEvidence,
    tick_size: f64,
) -> ChildRangeAudit {
    ChildRangeAudit {
        symbol: symbol.symbol.clone(),
        parent_start_time_ms: symbol.candles[parent.range.start_index].timestamp_ms,
        parent_confirmation_time_ms: symbol.candles[parent.range.first_confirmation_index]
            .timestamp_ms,
        parent_length_bars: parent.range.length_bars,
        parent_core_upper: parent.range.upper,
        parent_core_lower: parent.range.lower,
        parent_visual_upper: parent.visual.upper,
        parent_visual_lower: parent.visual.lower,
        child_start_time_ms: symbol.candles[child.start_index].timestamp_ms,
        child_confirmation_time_ms: symbol.candles[child.first_confirmation_index].timestamp_ms,
        child_length_bars: child.length_bars,
        child_upper: child.upper,
        child_lower: child.lower,
        starts_after_parent: child.start_index > parent.range.start_index,
        confirms_after_parent: child.first_confirmation_index
            > parent.range.first_confirmation_index,
        child_inside_parent_visual: child.upper <= parent.visual.upper
            && child.lower >= parent.visual.lower,
        differs_from_parent_core_by_tick: (child.upper - parent.range.upper).abs() >= tick_size
            || (child.lower - parent.range.lower).abs() >= tick_size,
    }
}

fn source_audit(
    symbol: &ResearchWindowSymbolCandles,
    parent: ParentContext,
    source: StrictVisualBreakoutSignal,
) -> ChildSourceAudit {
    let retained = directional_distance(
        source.direction,
        source.execution_boundary,
        symbol.candles[source.signal_index].close,
    );
    ChildSourceAudit {
        symbol: symbol.symbol.clone(),
        direction: source.direction,
        parent_start_time_ms: symbol.candles[parent.range.start_index].timestamp_ms,
        child_start_time_ms: symbol.candles[source.range.start_index].timestamp_ms,
        child_confirmation_time_ms: symbol.candles[source.range.first_confirmation_index]
            .timestamp_ms,
        child_length_bars: source.range.length_bars,
        child_upper: source.range.upper,
        child_lower: source.range.lower,
        breakout_time_ms: symbol.candles[source.breakout_index].timestamp_ms,
        signal_time_ms: symbol.candles[source.signal_index].timestamp_ms,
        execution_boundary: source.execution_boundary,
        breakout_excess: source.breakout_excess,
        required_acceptance_close_25pct: source.required_acceptance_close,
        actual_retention_ratio: retained / source.breakout_excess,
        source_atr: source.source_atr,
        parent_priority_preserved: true,
        breakout_after_child_confirmation: source.breakout_index
            > source.range.boundary_confirmation_index,
    }
}

fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
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
