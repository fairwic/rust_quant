//! 对 V31 弱离区执行顺向影线抵扣实体门槛的 outcome-blind L1 扫描。
//!
//! 本入口只读取信号时已完成的 OHLC 与横盘状态，不构造订单、退出或收益标签。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core, Candle, Direction,
    ParityRuleVersion, ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant,
    StrictVisualBreakoutSignal, StrictVisualDepartureSide, StrictVisualLongEntryEvent,
    StrictVisualLongEntryState, StrictVisualRangeEvent, StrictVisualRangeEvidence,
    StrictVisualWeakDepartureEvidence,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const BASE_BODY_RATIO: f64 = 0.50;
const MINIMUM_BODY_RATIO: f64 = 0.40;
const MINIMUM_DIRECTIONAL_MOVE_RATIO: f64 = 0.0015;
const WICK_CREDIT_MULTIPLIER: f64 = 0.20;
const MAXIMUM_WICK_CREDIT: f64 = 0.10;
const MINIMUM_RESCUED_RATIO: f64 = 0.02;
const MAXIMUM_RESCUED_RATIO: f64 = 0.30;
const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
/// 北京时间 2026-08-03 21:30 的 15 分钟完成棒开盘时间。
const TARGET_TIME_MS: i64 = 1_785_763_800_000;
const TARGET_DIAGNOSTIC_RADIUS_MS: i64 = 6 * 60 * 60 * 1_000;

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

/// 单根弱离区在当时可见 OHLC 上的实体、影线和动态门槛证据。
#[derive(Clone, Copy, Serialize)]
struct BodyCreditEvidence {
    body_ratio: f64,
    directional_move_ratio: f64,
    supportive_wick_ratio: f64,
    opposing_wick_ratio: f64,
    net_supportive_wick_ratio: f64,
    wick_credit: f64,
    required_body_ratio: f64,
    baseline_qualifies: bool,
    candidate_qualifies: bool,
}

#[derive(Clone, Serialize)]
struct WeakDepartureAudit {
    symbol: String,
    departure_time_ms: i64,
    side: StrictVisualDepartureSide,
    direction: Direction,
    range_start_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    range_length_bars: usize,
    upper: f64,
    lower: f64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    evidence: BodyCreditEvidence,
}

#[derive(Clone, Serialize)]
struct RangeStateAudit {
    range_start_time_ms: i64,
    first_confirmation_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    range_length_bars: usize,
    upper: f64,
    lower: f64,
}

#[derive(Clone, Serialize)]
struct PendingWeakStateAudit {
    departure_time_ms: i64,
    side: StrictVisualDepartureSide,
    range: RangeStateAudit,
}

#[derive(Clone, Serialize)]
struct PendingAcceptanceStateAudit {
    direction: Direction,
    breakout_time_ms: i64,
    range: RangeStateAudit,
}

#[derive(Clone, Default, Serialize)]
struct TargetStateAudit {
    active_range: Option<RangeStateAudit>,
    pending_weak_departure: Option<PendingWeakStateAudit>,
    pending_acceptance: Option<PendingAcceptanceStateAudit>,
}

#[derive(Clone, Serialize)]
struct NearbyLifecycleEventAudit {
    event_kind: &'static str,
    event_time_ms: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    range: RangeStateAudit,
    direction: Option<Direction>,
    state_after: TargetStateAudit,
}

#[derive(Default)]
struct ScanResult {
    weak_departures: Vec<WeakDepartureAudit>,
    target_state_before: Option<TargetStateAudit>,
    target_state_after: Option<TargetStateAudit>,
    target_nearby_events: Vec<NearbyLifecycleEventAudit>,
}

#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_variant: &'static str,
    single_variable: &'static str,
    label_boundary: &'static str,
    base_body_ratio: f64,
    minimum_body_ratio: f64,
    minimum_directional_move_ratio: f64,
    wick_credit_multiplier: f64,
    maximum_wick_credit: f64,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    baseline_weak_departures: usize,
    candidate_rescued_departures: usize,
    candidate_rescued_ratio: f64,
    candidate_long_rescues: usize,
    candidate_short_rescues: usize,
    candidate_distinct_symbols: usize,
    target_symbol: &'static str,
    target_time_ms: i64,
    target_baseline_weak_departure_found: bool,
    target_candidate_rescued: bool,
    target: Option<WeakDepartureAudit>,
    target_state_before: Option<TargetStateAudit>,
    target_state_after: Option<TargetStateAudit>,
    target_nearby_events: Vec<NearbyLifecycleEventAudit>,
    rescued_departures: Vec<WeakDepartureAudit>,
    weak_departures: Vec<WeakDepartureAudit>,
    l1_gate_passed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating supportive-wick L1 warmup start")?;
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

    let scan = scan_weak_departures(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    );
    let ScanResult {
        weak_departures,
        target_state_before,
        target_state_after,
        target_nearby_events,
    } = scan;
    let rescued_departures = weak_departures
        .iter()
        .filter(|departure| departure.evidence.candidate_qualifies)
        .cloned()
        .collect::<Vec<_>>();
    let candidate_rescued_ratio = if weak_departures.is_empty() {
        0.0
    } else {
        rescued_departures.len() as f64 / weak_departures.len() as f64
    };
    let candidate_long_rescues = rescued_departures
        .iter()
        .filter(|departure| departure.direction == Direction::Long)
        .count();
    let candidate_short_rescues = rescued_departures.len() - candidate_long_rescues;
    let candidate_distinct_symbols = rescued_departures
        .iter()
        .map(|departure| departure.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let target = weak_departures
        .iter()
        .find(|departure| {
            departure.symbol == TARGET_SYMBOL && departure.departure_time_ms == TARGET_TIME_MS
        })
        .cloned();
    let target_baseline_weak_departure_found = target.is_some();
    let target_candidate_rescued = target
        .as_ref()
        .is_some_and(|departure| departure.evidence.candidate_qualifies);
    let l1_gate_passed = target_baseline_weak_departure_found
        && target_candidate_rescued
        && rescued_departures.len() >= 3
        && candidate_distinct_symbols >= 3
        && candidate_long_rescues > 0
        && candidate_short_rescues > 0
        && (MINIMUM_RESCUED_RATIO..=MAXIMUM_RESCUED_RATIO).contains(&candidate_rescued_ratio);

    let report = L1Report {
        schema_version: "strict_visual_supportive_wick_body_credit_l1_v2",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "explicit_latest_confirmed_window",
        baseline_variant: StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard.slug(),
        candidate_variant: "v37_supportive_wick_body_credit_research",
        single_variable:
            "supportive_minus_opposing_wick_reduces_body_threshold_from_0_50_to_floor_0_40",
        label_boundary:
            "No order, entry, exit, MFE, MAE, final R, or win/loss field was read or serialized.",
        base_body_ratio: BASE_BODY_RATIO,
        minimum_body_ratio: MINIMUM_BODY_RATIO,
        minimum_directional_move_ratio: MINIMUM_DIRECTIONAL_MOVE_RATIO,
        wick_credit_multiplier: WICK_CREDIT_MULTIPLIER,
        maximum_wick_credit: MAXIMUM_WICK_CREDIT,
        dataset_fingerprint_sha256,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        baseline_weak_departures: weak_departures.len(),
        candidate_rescued_departures: rescued_departures.len(),
        candidate_rescued_ratio,
        candidate_long_rescues,
        candidate_short_rescues,
        candidate_distinct_symbols,
        target_symbol: TARGET_SYMBOL,
        target_time_ms: TARGET_TIME_MS,
        target_baseline_weak_departure_found,
        target_candidate_rescued,
        target,
        target_state_before,
        target_state_after,
        target_nearby_events,
        rescued_departures,
        weak_departures,
        l1_gate_passed,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "supportive_wick_l1: included={} weak={} rescued={} ratio={:.4} long={} short={} symbols={} target_found={} target_rescued={} gate={} output={}",
        report.included_symbols,
        report.baseline_weak_departures,
        report.candidate_rescued_departures,
        report.candidate_rescued_ratio,
        report.candidate_long_rescues,
        report.candidate_short_rescues,
        report.candidate_distinct_symbols,
        report.target_baseline_weak_departure_found,
        report.target_candidate_rescued,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("V37 supportive-wick L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

fn scan_weak_departures(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
) -> ScanResult {
    let mut result = ScanResult::default();
    for symbol in symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualLongEntryState::default();
        let mut active_range = None;
        let mut pending_weak_departure = None;
        let mut pending_acceptance = None;
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let point = &indicators.points[index];
            if point.atr14.is_none_or(|atr| atr <= 0.0) {
                continue;
            }
            if symbol.symbol == TARGET_SYMBOL && candle.timestamp_ms == TARGET_TIME_MS {
                result.target_state_before = Some(target_state_audit(
                    symbol,
                    active_range,
                    pending_weak_departure,
                    pending_acceptance,
                ));
            }
            let event = state.update(
                &symbol.candles,
                index,
                symbol.tick_size,
                point.atr14,
                point.volume_event,
                point.filtered_volume_ratio,
                None,
                StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
            );
            if let Some(event) = event {
                match event {
                    StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(range))
                    | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(
                        range,
                    )) => active_range = Some(range),
                    StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(_))
                    | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(_))
                    | StrictVisualLongEntryEvent::Range(
                        StrictVisualRangeEvent::InternalImpulseReset(_),
                    ) => active_range = None,
                    StrictVisualLongEntryEvent::Range(
                        StrictVisualRangeEvent::WeakDeparturePending(pending),
                    ) => {
                        pending_weak_departure = Some(pending);
                        if candle.timestamp_ms >= evaluation_start_ms {
                            if let Some(departure) = weak_departure_audit(symbol, candle, pending) {
                                result.weak_departures.push(departure);
                            }
                        }
                    }
                    StrictVisualLongEntryEvent::Range(
                        StrictVisualRangeEvent::WeakDepartureReturned(returned),
                    ) => {
                        pending_weak_departure = None;
                        active_range = Some(returned.range);
                    }
                    StrictVisualLongEntryEvent::Range(
                        StrictVisualRangeEvent::WeakDepartureConsumed(_),
                    ) => {
                        pending_weak_departure = None;
                        active_range = None;
                    }
                    StrictVisualLongEntryEvent::AcceptanceArmed(signal) => {
                        active_range = None;
                        pending_acceptance = Some(signal);
                    }
                    StrictVisualLongEntryEvent::DirectSignal(_) => active_range = None,
                    StrictVisualLongEntryEvent::AcceptanceConfirmed(_)
                    | StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(_)
                    | StrictVisualLongEntryEvent::AcceptanceMarginRejected(_)
                    | StrictVisualLongEntryEvent::ExternalStructureRejected(_)
                    | StrictVisualLongEntryEvent::AcceptanceInvalidated(_)
                    | StrictVisualLongEntryEvent::AcceptanceExpired(_) => {
                        pending_acceptance = None;
                    }
                }
                if symbol.symbol == TARGET_SYMBOL
                    && (candle.timestamp_ms - TARGET_TIME_MS).abs() <= TARGET_DIAGNOSTIC_RADIUS_MS
                {
                    let (event_kind, range, direction) = lifecycle_event_identity(event);
                    result.target_nearby_events.push(NearbyLifecycleEventAudit {
                        event_kind,
                        event_time_ms: candle.timestamp_ms,
                        open: candle.open,
                        high: candle.high,
                        low: candle.low,
                        close: candle.close,
                        range: range_state_audit(symbol, range),
                        direction,
                        state_after: target_state_audit(
                            symbol,
                            active_range,
                            pending_weak_departure,
                            pending_acceptance,
                        ),
                    });
                }
            }
            if symbol.symbol == TARGET_SYMBOL && candle.timestamp_ms == TARGET_TIME_MS {
                result.target_state_after = Some(target_state_audit(
                    symbol,
                    active_range,
                    pending_weak_departure,
                    pending_acceptance,
                ));
            }
        }
    }
    result
}

fn weak_departure_audit(
    symbol: &ResearchWindowSymbolCandles,
    candle: Candle,
    pending: StrictVisualWeakDepartureEvidence,
) -> Option<WeakDepartureAudit> {
    let direction = match pending.side {
        StrictVisualDepartureSide::Upper => Direction::Long,
        StrictVisualDepartureSide::Lower => Direction::Short,
    };
    let evidence = supportive_wick_body_credit(candle, direction)?;
    Some(WeakDepartureAudit {
        symbol: symbol.symbol.clone(),
        departure_time_ms: candle.timestamp_ms,
        side: pending.side,
        direction,
        range_start_time_ms: symbol.candles[pending.range.start_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[pending.range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: pending.range.length_bars,
        upper: pending.range.upper,
        lower: pending.range.lower,
        open: candle.open,
        high: candle.high,
        low: candle.low,
        close: candle.close,
        evidence,
    })
}

fn target_state_audit(
    symbol: &ResearchWindowSymbolCandles,
    active_range: Option<StrictVisualRangeEvidence>,
    pending_weak_departure: Option<StrictVisualWeakDepartureEvidence>,
    pending_acceptance: Option<StrictVisualBreakoutSignal>,
) -> TargetStateAudit {
    TargetStateAudit {
        active_range: active_range.map(|range| range_state_audit(symbol, range)),
        pending_weak_departure: pending_weak_departure.map(|pending| PendingWeakStateAudit {
            departure_time_ms: symbol.candles[pending.departure_index].timestamp_ms,
            side: pending.side,
            range: range_state_audit(symbol, pending.range),
        }),
        pending_acceptance: pending_acceptance.map(|signal| PendingAcceptanceStateAudit {
            direction: signal.direction,
            breakout_time_ms: symbol.candles[signal.breakout_index].timestamp_ms,
            range: range_state_audit(symbol, signal.range),
        }),
    }
}

fn range_state_audit(
    symbol: &ResearchWindowSymbolCandles,
    range: StrictVisualRangeEvidence,
) -> RangeStateAudit {
    RangeStateAudit {
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
    }
}

fn lifecycle_event_identity(
    event: StrictVisualLongEntryEvent,
) -> (&'static str, StrictVisualRangeEvidence, Option<Direction>) {
    match event {
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(range)) => {
            ("range_confirmed", range, None)
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(range)) => {
            ("parent_upgraded", range, None)
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(range)) => {
            ("upper_break", range, Some(Direction::Long))
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(range)) => {
            ("lower_break", range, Some(Direction::Short))
        }
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDeparturePending(
            pending,
        )) => (
            "weak_departure_pending",
            pending.range,
            Some(match pending.side {
                StrictVisualDepartureSide::Upper => Direction::Long,
                StrictVisualDepartureSide::Lower => Direction::Short,
            }),
        ),
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureReturned(
            returned,
        )) => ("weak_departure_returned", returned.range, None),
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureConsumed(
            consumed,
        )) => ("weak_departure_consumed", consumed.range, None),
        StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::InternalImpulseReset(reset)) => {
            ("internal_impulse_reset", reset.range, None)
        }
        StrictVisualLongEntryEvent::DirectSignal(signal) => {
            ("direct_signal", signal.range, Some(signal.direction))
        }
        StrictVisualLongEntryEvent::AcceptanceArmed(signal) => {
            ("acceptance_armed", signal.range, Some(signal.direction))
        }
        StrictVisualLongEntryEvent::AcceptanceConfirmed(signal) => {
            ("acceptance_confirmed", signal.range, Some(signal.direction))
        }
        StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(signal) => (
            "acceptance_body_midpoint_rejected",
            signal.range,
            Some(signal.direction),
        ),
        StrictVisualLongEntryEvent::AcceptanceMarginRejected(signal) => (
            "acceptance_margin_rejected",
            signal.range,
            Some(signal.direction),
        ),
        StrictVisualLongEntryEvent::ExternalStructureRejected(signal) => (
            "external_structure_rejected",
            signal.range,
            Some(signal.direction),
        ),
        StrictVisualLongEntryEvent::AcceptanceInvalidated(signal) => (
            "acceptance_invalidated",
            signal.range,
            Some(signal.direction),
        ),
        StrictVisualLongEntryEvent::AcceptanceExpired(signal) => {
            ("acceptance_expired", signal.range, Some(signal.direction))
        }
    }
}

fn supportive_wick_body_credit(candle: Candle, direction: Direction) -> Option<BodyCreditEvidence> {
    let range = candle.range();
    if !candle.is_valid() || range <= 0.0 || candle.open <= 0.0 {
        return None;
    }
    let body_top = candle.open.max(candle.close);
    let body_bottom = candle.open.min(candle.close);
    let upper_wick_ratio = (candle.high - body_top) / range;
    let lower_wick_ratio = (body_bottom - candle.low) / range;
    let (supportive_wick_ratio, opposing_wick_ratio, directional_move_ratio) = match direction {
        Direction::Long => (
            lower_wick_ratio,
            upper_wick_ratio,
            (candle.close - candle.open) / candle.open,
        ),
        Direction::Short => (
            upper_wick_ratio,
            lower_wick_ratio,
            (candle.open - candle.close) / candle.open,
        ),
    };
    let body_ratio = candle.body() / range;
    let net_supportive_wick_ratio = (supportive_wick_ratio - opposing_wick_ratio).max(0.0);
    let wick_credit = (net_supportive_wick_ratio * WICK_CREDIT_MULTIPLIER).min(MAXIMUM_WICK_CREDIT);
    let required_body_ratio = (BASE_BODY_RATIO - wick_credit).max(MINIMUM_BODY_RATIO);
    let baseline_qualifies =
        body_ratio >= BASE_BODY_RATIO && directional_move_ratio >= MINIMUM_DIRECTIONAL_MOVE_RATIO;
    let candidate_qualifies = body_ratio >= required_body_ratio
        && directional_move_ratio >= MINIMUM_DIRECTIONAL_MOVE_RATIO;
    Some(BodyCreditEvidence {
        body_ratio,
        directional_move_ratio,
        supportive_wick_ratio,
        opposing_wick_ratio,
        net_supportive_wick_ratio,
        wick_credit,
        required_body_ratio,
        baseline_qualifies,
        candidate_qualifies,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_btc_lower_wick_reduces_long_body_threshold_without_reaching_fifty_percent() {
        let candle = Candle {
            timestamp_ms: TARGET_TIME_MS,
            open: 62_737.3,
            high: 62_915.9,
            low: 62_533.3,
            close: 62_901.5,
            volume: 4_010.0,
        };
        let evidence = supportive_wick_body_credit(candle, Direction::Long).unwrap();
        assert!(evidence.body_ratio >= 0.40 && evidence.body_ratio < 0.50);
        assert!(evidence.supportive_wick_ratio > evidence.opposing_wick_ratio);
        assert!(!evidence.baseline_qualifies);
        assert!(evidence.candidate_qualifies);
    }

    #[test]
    fn upper_wick_credit_is_the_exact_short_mirror() {
        let candle = Candle {
            timestamp_ms: 0,
            open: 100.0,
            high: 100.54,
            low: 99.54,
            close: 99.58,
            volume: 1.0,
        };
        let evidence = supportive_wick_body_credit(candle, Direction::Short).unwrap();
        assert!((evidence.body_ratio - 0.42).abs() < 1e-12);
        assert!(!evidence.baseline_qualifies);
        assert!(evidence.candidate_qualifies);
    }

    #[test]
    fn opposing_wick_does_not_discount_a_forty_percent_body() {
        let candle = Candle {
            timestamp_ms: 0,
            open: 100.0,
            high: 100.97,
            low: 99.97,
            close: 100.42,
            volume: 1.0,
        };
        let evidence = supportive_wick_body_credit(candle, Direction::Long).unwrap();
        assert_eq!(evidence.wick_credit, 0.0);
        assert_eq!(evidence.required_body_ratio, 0.50);
        assert!(!evidence.candidate_qualifies);
    }
}
