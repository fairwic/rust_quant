use anyhow::{bail, Context, Result};
use chrono::{Datelike, FixedOffset, TimeZone};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_frozen_top60_from_quant_core,
    strict_visual_breakout_body_strength_for_variant, Candle, Direction, FrozenSymbolCandles,
    ParityRuleVersion, StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal,
    StrictVisualDepartureSide, StrictVisualLongEntryEvent, StrictVisualLongEntryState,
    StrictVisualRangeEvent, FROZEN_UNIVERSE_MANIFEST_SHA256, FROZEN_UNIVERSE_VERSION,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const MIN_CANDIDATES: usize = 30;
const MIN_DIRECTION_CANDIDATES: usize = 15;
const MIN_SYMBOLS: usize = 8;
const MIN_MONTHS: usize = 3;
const MIN_EVENT_CLUSTERS: usize = 10;
const DEFAULT_VARIANT: StrictVisualBreakoutResearchVariant =
    StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout;

/// L1 只读扫描参数；部分成员模式必须显式开启，不能冒充完整 Top60。
#[derive(Debug)]
struct Args {
    output: Option<PathBuf>,
    allow_partial_diagnostic: bool,
    variant: StrictVisualBreakoutResearchVariant,
}

/// 冻结成员因缺 K 线或预热不足而未进入本轮覆盖审计。
#[derive(Debug, Serialize)]
struct ExcludedSymbol {
    symbol: String,
    evaluation_loaded: usize,
    evaluation_expected: usize,
    reason: &'static str,
}

/// 一个方向的来源与确认生命周期计数。
#[derive(Debug, Default, Serialize)]
struct DirectionCounts {
    armed_sources: usize,
    confirmed_candidates: usize,
    invalidated_sources: usize,
    expired_sources: usize,
    unarmed_boundary_breaks: usize,
}

/// 弱离区一棒观察期的因果生命周期计数。
#[derive(Debug, Default, Serialize)]
struct WeakDepartureCounts {
    pending: usize,
    returned: usize,
    consumed: usize,
    unresolved: usize,
    maximum_resolution_age_bars: usize,
}

/// 信号完成时已经冻结的全部候选字段；严禁加入 MFE、MAE、退出或盈亏标签。
#[derive(Debug, Serialize)]
struct L1Candidate {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    acceptance_age_bars: usize,
    shanghai_month: String,
    event_cluster_id: String,
    range_start_time_ms: i64,
    first_confirmation_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    range_length_bars: usize,
    upper: f64,
    lower: f64,
    range_height: f64,
    breakout_open: f64,
    breakout_high: f64,
    breakout_low: f64,
    breakout_close: f64,
    breakout_body_ratio: f64,
    breakout_directional_move_ratio: f64,
    breakout_excess: f64,
    breakout_candle_extreme_stop_price: Option<f64>,
    breakout_candle_extreme_stop_min_atr_multiple: Option<f64>,
    required_acceptance_close: f64,
    confirmation_open: f64,
    confirmation_high: f64,
    confirmation_low: f64,
    confirmation_close: f64,
    retained_excess: f64,
    retained_excess_ratio: f64,
    source_atr: f64,
    confirmation_atr: f64,
    source_volume_ratio_diagnostic: Option<f64>,
    measured_move_target_price: f64,
    containment_ratio: f64,
    direction_efficiency: f64,
    edge_transition_count: usize,
}

/// L1 结果只回答覆盖与因果完整性，不读取任何成交后标签。
#[derive(Debug, Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    strategy_version: &'static str,
    baseline_strategy_version: &'static str,
    universe_version: &'static str,
    universe_manifest_sha256: &'static str,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    manifest_evaluation_end_ms: i64,
    expected_symbols: usize,
    included_symbols: usize,
    full_universe_complete: bool,
    excluded_symbols: Vec<ExcludedSymbol>,
    confirmed_ranges: usize,
    parent_upgrades: usize,
    weak_upper_departures: WeakDepartureCounts,
    weak_lower_departures: WeakDepartureCounts,
    long: DirectionCounts,
    short: DirectionCounts,
    acceptance_unresolved: usize,
    unexpected_legacy_rejections: usize,
    invalid_breakout_candle_extreme_stops: usize,
    invalid_breakout_candle_extreme_stop_min_atr_contracts: usize,
    qualified_candidates: usize,
    covered_symbols: usize,
    covered_shanghai_months: usize,
    direction_time_event_clusters_60m: usize,
    l1_gate_passed: bool,
    l1_gate: &'static str,
    label_boundary: &'static str,
    candidates: Vec<L1Candidate>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let variant = args.variant;
    let mut dataset = load_frozen_top60_from_quant_core().await?;
    if dataset.universe_version != FROZEN_UNIVERSE_VERSION
        || dataset.manifest_sha256 != FROZEN_UNIVERSE_MANIFEST_SHA256
    {
        bail!("冻结币池 identity 与编译时常量不一致");
    }
    let dataset_fingerprint_sha256 = dataset_fingerprint(&dataset.symbols);
    let evaluation_start_ms = dataset.window_start_ms;
    let manifest_evaluation_end_ms = dataset
        .window_end_ms
        .checked_sub(1)
        .context("冻结评价上界下溢")?;
    let evaluation_end_ms = if args.allow_partial_diagnostic {
        modal_snapshot_end(&dataset.symbols, evaluation_start_ms)
            .context("冻结 Top60 没有正式窗口 K 线")?
    } else {
        manifest_evaluation_end_ms
    };
    let expected_symbols = dataset.coverage.expected_symbol_count;
    let mut eligible_symbols = Vec::new();
    let mut excluded_symbols = Vec::new();
    for symbol in std::mem::take(&mut dataset.symbols) {
        let coverage =
            replay_window_coverage(&symbol.candles, evaluation_start_ms, evaluation_end_ms)?;
        if coverage.is_complete && symbol.warmup_is_complete {
            eligible_symbols.push(symbol);
        } else {
            excluded_symbols.push(ExcludedSymbol {
                symbol: symbol.symbol,
                evaluation_loaded: coverage.loaded,
                evaluation_expected: coverage.expected,
                reason: match (coverage.is_complete, symbol.warmup_is_complete) {
                    (false, false) => "评价窗口与 60 天预热均不完整",
                    (false, true) => "评价窗口不完整",
                    (true, false) => "60 天指标预热不完整",
                    (true, true) => unreachable!("完整成员不会被排除"),
                },
            });
        }
    }
    let full_universe_complete = eligible_symbols.len() == expected_symbols
        && evaluation_end_ms == manifest_evaluation_end_ms;
    if !full_universe_complete && !args.allow_partial_diagnostic {
        bail!(
            "严格 L1 需要完整 Top60；本地只有 {}/{} 个完整成员",
            eligible_symbols.len(),
            expected_symbols
        );
    }
    if eligible_symbols.is_empty() {
        bail!("没有同时具备完整评价窗口和 60 天预热的冻结成员");
    }

    let mut confirmed_ranges = 0;
    let mut parent_upgrades = 0;
    let mut weak_upper = WeakDepartureCounts::default();
    let mut weak_lower = WeakDepartureCounts::default();
    let mut long = DirectionCounts::default();
    let mut short = DirectionCounts::default();
    let mut unexpected_legacy_rejections = 0;
    let mut candidates = Vec::new();
    for symbol in &eligible_symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualLongEntryState::default();
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms > evaluation_end_ms {
                break;
            }
            let point = &indicators.points[index];
            let Some(event) = state.update(
                &symbol.candles,
                index,
                symbol.tick_size,
                point.atr14,
                point.volume_event,
                point.filtered_volume_ratio,
                None,
                variant,
            ) else {
                continue;
            };
            if candle.timestamp_ms < evaluation_start_ms {
                continue;
            }
            match event {
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(_)) => {
                    confirmed_ranges += 1;
                }
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(_)) => {
                    parent_upgrades += 1;
                }
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(_)) => {
                    long.unarmed_boundary_breaks += 1;
                }
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(_)) => {
                    short.unarmed_boundary_breaks += 1;
                }
                StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDeparturePending(evidence),
                ) => {
                    direction_weak_counts_mut(&mut weak_upper, &mut weak_lower, evidence.side)
                        .pending += 1
                }
                StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDepartureReturned(evidence),
                ) => record_weak_resolution(
                    &mut weak_upper,
                    &mut weak_lower,
                    evidence.side,
                    evidence.departure_index,
                    evidence.confirmation_index,
                    true,
                ),
                StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDepartureConsumed(evidence),
                ) => record_weak_resolution(
                    &mut weak_upper,
                    &mut weak_lower,
                    evidence.side,
                    evidence.departure_index,
                    evidence.confirmation_index,
                    false,
                ),
                StrictVisualLongEntryEvent::AcceptanceArmed(signal) => {
                    direction_counts_mut(&mut long, &mut short, signal.direction).armed_sources +=
                        1;
                }
                StrictVisualLongEntryEvent::AcceptanceConfirmed(signal) => {
                    direction_counts_mut(&mut long, &mut short, signal.direction)
                        .confirmed_candidates += 1;
                    candidates.push(candidate_from_signal(
                        symbol,
                        signal,
                        indicators.points[signal.breakout_index].filtered_volume_ratio,
                        indicators.points[signal.signal_index].atr14,
                        variant,
                    )?);
                }
                StrictVisualLongEntryEvent::AcceptanceInvalidated(signal) => {
                    direction_counts_mut(&mut long, &mut short, signal.direction)
                        .invalidated_sources += 1;
                }
                StrictVisualLongEntryEvent::AcceptanceExpired(signal) => {
                    direction_counts_mut(&mut long, &mut short, signal.direction)
                        .expired_sources += 1;
                }
                StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(_)
                | StrictVisualLongEntryEvent::AcceptanceMarginRejected(_)
                | StrictVisualLongEntryEvent::ExternalStructureRejected(_) => {
                    unexpected_legacy_rejections += 1;
                }
                StrictVisualLongEntryEvent::DirectSignal(_) => unexpected_legacy_rejections += 1,
            }
        }
    }

    weak_upper.unresolved = weak_upper
        .pending
        .saturating_sub(weak_upper.returned + weak_upper.consumed);
    weak_lower.unresolved = weak_lower
        .pending
        .saturating_sub(weak_lower.returned + weak_lower.consumed);
    let acceptance_unresolved = long.armed_sources.saturating_sub(
        long.confirmed_candidates + long.invalidated_sources + long.expired_sources,
    ) + short.armed_sources.saturating_sub(
        short.confirmed_candidates + short.invalidated_sources + short.expired_sources,
    );
    assign_event_clusters(&mut candidates);
    let qualified_candidates = candidates.len();
    let covered_symbols = candidates
        .iter()
        .map(|candidate| candidate.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let covered_shanghai_months = candidates
        .iter()
        .map(|candidate| candidate.shanghai_month.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let direction_time_event_clusters_60m = candidates
        .iter()
        .map(|candidate| candidate.event_cluster_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let invalid_breakout_candle_extreme_stops = candidates
        .iter()
        .filter(|candidate| {
            variant.uses_breakout_candle_extreme_stop()
                && !candidate
                    .breakout_candle_extreme_stop_price
                    .is_some_and(|stop| match candidate.direction {
                        Direction::Long => stop < candidate.breakout_low,
                        Direction::Short => stop > candidate.breakout_high,
                    })
        })
        .count();
    let expected_minimum_stop_atr =
        (variant == StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr).then_some(1.0);
    let invalid_breakout_candle_extreme_stop_min_atr_contracts = candidates
        .iter()
        .filter(|candidate| {
            candidate.breakout_candle_extreme_stop_min_atr_multiple != expected_minimum_stop_atr
                || candidate.confirmation_atr <= 0.0
                || !candidate.confirmation_atr.is_finite()
        })
        .count();
    let l1_gate_passed = qualified_candidates >= MIN_CANDIDATES
        && long.confirmed_candidates >= MIN_DIRECTION_CANDIDATES
        && short.confirmed_candidates >= MIN_DIRECTION_CANDIDATES
        && covered_symbols >= MIN_SYMBOLS
        && covered_shanghai_months >= MIN_MONTHS
        && direction_time_event_clusters_60m >= MIN_EVENT_CLUSTERS
        && weak_upper.unresolved + weak_lower.unresolved == 0
        && weak_upper.maximum_resolution_age_bars <= 1
        && weak_lower.maximum_resolution_age_bars <= 1
        && unexpected_legacy_rejections == 0
        && invalid_breakout_candle_extreme_stops == 0
        && invalid_breakout_candle_extreme_stop_min_atr_contracts == 0;
    let report = L1Report {
        schema_version: "strict_visual_symmetric_retained_breakout_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: if full_universe_complete {
            "frozen_top60_complete"
        } else {
            "partial_data_diagnostic"
        },
        strategy_version: variant.strategy_version(ParityRuleVersion::CandidateV20),
        baseline_strategy_version: ParityRuleVersion::CandidateV20.strategy_version(),
        universe_version: FROZEN_UNIVERSE_VERSION,
        universe_manifest_sha256: FROZEN_UNIVERSE_MANIFEST_SHA256,
        dataset_fingerprint_sha256,
        evaluation_start_ms,
        evaluation_end_ms,
        manifest_evaluation_end_ms,
        expected_symbols,
        included_symbols: eligible_symbols.len(),
        full_universe_complete,
        excluded_symbols,
        confirmed_ranges,
        parent_upgrades,
        weak_upper_departures: weak_upper,
        weak_lower_departures: weak_lower,
        long,
        short,
        acceptance_unresolved,
        unexpected_legacy_rejections,
        invalid_breakout_candle_extreme_stops,
        invalid_breakout_candle_extreme_stop_min_atr_contracts,
        qualified_candidates,
        covered_symbols,
        covered_shanghai_months,
        direction_time_event_clusters_60m,
        l1_gate_passed,
        l1_gate: "candidates>=30; long>=15; short>=15; symbols>=8; months>=3; direction+60m clusters>=10; weak lifecycle causal; no legacy rejection path; structural stop outside breakout extreme; V12 declares exactly 1.0 ATR minimum risk",
        label_boundary: "No MFE, MAE, exit time, final R, win/loss, or post-signal field was read or serialized.",
        candidates,
    };
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(path) = args.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serialized)?;
    } else {
        println!("{serialized}");
    }
    if !l1_gate_passed {
        bail!("L1 覆盖门禁未通过；按预注册停止，不得进入 L2");
    }
    Ok(())
}

fn candidate_from_signal(
    symbol: &FrozenSymbolCandles,
    signal: StrictVisualBreakoutSignal,
    source_volume_ratio_diagnostic: Option<f64>,
    confirmation_atr: Option<f64>,
    variant: StrictVisualBreakoutResearchVariant,
) -> Result<L1Candidate> {
    let breakout = symbol.candles[signal.breakout_index];
    let confirmation = symbol.candles[signal.signal_index];
    let range = signal.range;
    let strength =
        strict_visual_breakout_body_strength_for_variant(breakout, signal.direction, variant);
    let boundary = match signal.direction {
        Direction::Long => range.upper,
        Direction::Short => range.lower,
    };
    let retained_excess = match signal.direction {
        Direction::Long => confirmation.close - boundary,
        Direction::Short => boundary - confirmation.close,
    };
    let shanghai = FixedOffset::east_opt(8 * 60 * 60).context("上海时区偏移无效")?;
    let timestamp = shanghai
        .timestamp_millis_opt(confirmation.timestamp_ms)
        .single()
        .context("信号时间戳超出 chrono 范围")?;
    Ok(L1Candidate {
        symbol: symbol.symbol.clone(),
        direction: signal.direction,
        breakout_time_ms: breakout.timestamp_ms,
        signal_time_ms: confirmation.timestamp_ms,
        acceptance_age_bars: signal.signal_index - signal.breakout_index,
        shanghai_month: format!("{:04}-{:02}", timestamp.year(), timestamp.month()),
        event_cluster_id: String::new(),
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
        range_height: range.upper - range.lower,
        breakout_open: breakout.open,
        breakout_high: breakout.high,
        breakout_low: breakout.low,
        breakout_close: breakout.close,
        breakout_body_ratio: strength.body_ratio,
        breakout_directional_move_ratio: strength.directional_move_ratio,
        breakout_excess: signal.breakout_excess,
        breakout_candle_extreme_stop_price: signal.breakout_candle_extreme_stop_price,
        breakout_candle_extreme_stop_min_atr_multiple: signal
            .breakout_candle_extreme_stop_min_atr_multiple,
        required_acceptance_close: signal.required_acceptance_close,
        confirmation_open: confirmation.open,
        confirmation_high: confirmation.high,
        confirmation_low: confirmation.low,
        confirmation_close: confirmation.close,
        retained_excess,
        retained_excess_ratio: retained_excess / signal.breakout_excess,
        source_atr: signal.source_atr,
        confirmation_atr: confirmation_atr.context("严格视觉确认信号缺少 ATR14")?,
        source_volume_ratio_diagnostic,
        measured_move_target_price: signal
            .measured_move_target_price
            .context("双向合同缺少冻结量度目标")?,
        containment_ratio: range.containment_ratio,
        direction_efficiency: range.direction_efficiency,
        edge_transition_count: range.edge_transition_count,
    })
}

fn direction_counts_mut<'a>(
    long: &'a mut DirectionCounts,
    short: &'a mut DirectionCounts,
    direction: Direction,
) -> &'a mut DirectionCounts {
    match direction {
        Direction::Long => long,
        Direction::Short => short,
    }
}

fn direction_weak_counts_mut<'a>(
    upper: &'a mut WeakDepartureCounts,
    lower: &'a mut WeakDepartureCounts,
    side: StrictVisualDepartureSide,
) -> &'a mut WeakDepartureCounts {
    match side {
        StrictVisualDepartureSide::Upper => upper,
        StrictVisualDepartureSide::Lower => lower,
    }
}

fn record_weak_resolution(
    upper: &mut WeakDepartureCounts,
    lower: &mut WeakDepartureCounts,
    side: StrictVisualDepartureSide,
    departure_index: usize,
    confirmation_index: Option<usize>,
    returned: bool,
) {
    let counts = direction_weak_counts_mut(upper, lower, side);
    if returned {
        counts.returned += 1;
    } else {
        counts.consumed += 1;
    }
    let age = confirmation_index
        .unwrap_or(departure_index)
        .saturating_sub(departure_index);
    counts.maximum_resolution_age_bars = counts.maximum_resolution_age_bars.max(age);
}

/// 多空分别按相邻一小时串联，避免一次市场脉冲被误当成独立样本。
fn assign_event_clusters(candidates: &mut [L1Candidate]) {
    candidates.sort_by(|left, right| {
        left.signal_time_ms
            .cmp(&right.signal_time_ms)
            .then_with(|| direction_slug(left.direction).cmp(direction_slug(right.direction)))
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let mut last_long = None;
    let mut last_short = None;
    let mut long_start = 0;
    let mut short_start = 0;
    for candidate in candidates {
        let (last, cluster_start) = match candidate.direction {
            Direction::Long => (&mut last_long, &mut long_start),
            Direction::Short => (&mut last_short, &mut short_start),
        };
        if last.is_none_or(|time| candidate.signal_time_ms - time > EVENT_CLUSTER_MS) {
            *cluster_start = candidate.signal_time_ms;
        }
        candidate.event_cluster_id =
            format!("{}-{}", direction_slug(candidate.direction), *cluster_start);
        *last = Some(candidate.signal_time_ms);
    }
}

const fn direction_slug(direction: Direction) -> &'static str {
    match direction {
        Direction::Long => "long",
        Direction::Short => "short",
    }
}

#[derive(Debug)]
struct ReplayWindowCoverage {
    expected: usize,
    loaded: usize,
    is_complete: bool,
}

fn modal_snapshot_end(symbols: &[FrozenSymbolCandles], evaluation_start_ms: i64) -> Option<i64> {
    let mut counts = BTreeMap::<i64, usize>::new();
    for timestamp_ms in symbols.iter().filter_map(|symbol| {
        symbol
            .candles
            .iter()
            .rev()
            .find(|candle| candle.timestamp_ms >= evaluation_start_ms)
            .map(|candle| candle.timestamp_ms)
    }) {
        *counts.entry(timestamp_ms).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(timestamp_ms, _)| timestamp_ms)
}

fn replay_window_coverage(
    candles: &[Candle],
    start_ms: i64,
    end_ms: i64,
) -> Result<ReplayWindowCoverage> {
    if end_ms < start_ms
        || start_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
        || end_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
    {
        bail!("L1 评价窗口没有对齐 15 分钟");
    }
    let expected =
        usize::try_from((end_ms - start_ms) / CANDLE_INTERVAL_MS + 1).context("L1 根数溢出")?;
    let selected = candles
        .iter()
        .filter(|candle| (start_ms..=end_ms).contains(&candle.timestamp_ms))
        .collect::<Vec<_>>();
    let loaded = selected.len();
    let is_complete = loaded == expected
        && selected
            .first()
            .is_some_and(|candle| candle.timestamp_ms == start_ms)
        && selected
            .last()
            .is_some_and(|candle| candle.timestamp_ms == end_ms)
        && selected
            .windows(2)
            .all(|pair| pair[1].timestamp_ms - pair[0].timestamp_ms == CANDLE_INTERVAL_MS);
    Ok(ReplayWindowCoverage {
        expected,
        loaded,
        is_complete,
    })
}

fn dataset_fingerprint(symbols: &[FrozenSymbolCandles]) -> String {
    let mut hasher = Sha256::new();
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

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut output = None;
    let mut allow_partial_diagnostic = false;
    let mut variant = DEFAULT_VARIANT;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = Some(PathBuf::from(args.next().context("--output 缺少路径")?)),
            "--allow-partial-diagnostic" => allow_partial_diagnostic = true,
            "--variant" => {
                variant = match args.next().context("--variant 缺少版本")?.as_str() {
                    "v10" | "v10-symmetric-retained-breakout" => {
                        StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout
                    }
                    "v11" | "v11-breakout-candle-extreme-stop" => {
                        StrictVisualBreakoutResearchVariant::V11BreakoutCandleExtremeStop
                    }
                    "v12" | "v12-breakout-candle-extreme-stop-min-1atr" => {
                        StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr
                    }
                    other => bail!("unsupported strict visual L1 variant: {other}"),
                };
            }
            "--help" | "-h" => {
                println!("Usage: tradingview_strict_visual_symmetric_breakout_l1 [--variant v10|v11|v12] [--output PATH] [--allow-partial-diagnostic]");
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(Args {
        output,
        allow_partial_diagnostic,
        variant,
    })
}
