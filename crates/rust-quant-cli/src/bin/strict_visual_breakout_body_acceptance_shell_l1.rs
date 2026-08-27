//! 对 V31 实体重复接受视觉外壳执行 outcome-blind L1 审计。
//!
//! 本入口不改变 V31 的横盘生命周期、交易边界或信号，只读取完成 K 线生成绘图候选账本。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core, Candle, ParityRuleVersion,
    ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant,
    StrictVisualConsolidationState, StrictVisualRangeEvent, StrictVisualRangeEvidence,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const MINIMUM_TOUCH_GAP_BARS: usize = 2;
const CORE_HEIGHT_ACCEPTANCE_RATIO: f64 = 0.10;
const CONFIRMATION_ATR_ACCEPTANCE_RATIO: f64 = 0.25;
const MINIMUM_COMPLETE_SHELL_LIFECYCLES: usize = 30;
const MINIMUM_COMPLETE_SHELL_SYMBOLS: usize = 8;
const MINIMUM_COMPLETE_SHELL_EVENT_CLUSTERS: usize = 10;
const MINIMUM_EXPANDED_LIFECYCLES: usize = 3;
const MINIMUM_EXPANDED_SYMBOLS: usize = 3;
const MINIMUM_COMPLETE_SHELL_RATIO: f64 = 0.10;
const MAXIMUM_COMPLETE_SHELL_RATIO: f64 = 0.90;

const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_WINDOW_START_MS: i64 = 1_785_103_200_000;
const TARGET_WINDOW_END_MS: i64 = 1_785_156_300_000;
const TARGET_MANUAL_UPPER: f64 = 65_570.0;
const TARGET_MANUAL_LOWER: f64 = 64_970.0;
const TARGET_DRAWING_TOLERANCE: f64 = 50.0;
const MINIMUM_TARGET_OVERLAP_BARS: usize = 8;

/// 显式 L1 数据身份；不开放外壳阈值或交易参数开关。
struct Args {
    symbols: Vec<String>,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    output: PathBuf,
}

/// 因预热或评价窗口不完整而被隔离的成员。
#[derive(Serialize)]
struct ExcludedSymbol {
    symbol: String,
    warmup_expected: usize,
    warmup_loaded: usize,
    evaluation_expected: usize,
    evaluation_loaded: usize,
    reason: &'static str,
}

/// 一个活动横盘在某个完成时点可见的视觉外壳。
#[derive(Clone, Serialize)]
struct BodyAcceptanceShellSnapshot {
    snapshot_time_ms: i64,
    body_sample_count: usize,
    confirmation_atr14: f64,
    acceptance_band: f64,
    core_upper: f64,
    core_lower: f64,
    core_height: f64,
    accepted_body_upper: Option<f64>,
    accepted_body_lower: Option<f64>,
    upper_touch_groups: usize,
    lower_touch_groups: usize,
    visual_upper: f64,
    visual_lower: f64,
    upper_expansion_core_heights: f64,
    lower_expansion_core_heights: f64,
    raw_high: f64,
    raw_low: f64,
    raw_high_exceeds_visual_upper: bool,
    raw_low_exceeds_visual_lower: bool,
    shell_complete: bool,
}

/// 一个 V31 已确认横盘从确认到结束前最后一根区间内完成棒的只读账本。
#[derive(Clone, Serialize)]
struct VisualShellLifecycleAudit {
    symbol: String,
    range_start_time_ms: i64,
    first_confirmation_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    last_inside_time_ms: i64,
    end_reason: &'static str,
    range_length_bars: usize,
    snapshot: BodyAcceptanceShellSnapshot,
}

/// 当前仍活动的 V31 生命周期；父区间升级继续复用同一首次确认身份。
struct ActiveLifecycle {
    range: StrictVisualRangeEvidence,
    latest_inside_snapshot: Option<BodyAcceptanceShellSnapshot>,
}

/// 人工黄色框与确定性最大重叠 V31 生命周期的比较。
#[derive(Serialize)]
struct TargetAudit {
    selected: Option<VisualShellLifecycleAudit>,
    overlap_bars: usize,
    upper_within_drawing_tolerance: bool,
    lower_within_drawing_tolerance: bool,
    upper_closer_than_core: bool,
    lower_closer_than_core: bool,
    isolated_upper_wick_excluded: bool,
    isolated_lower_wick_excluded: bool,
    passed: bool,
}

/// L1 只验证目标与无标签覆盖，不序列化任何交易结果字段。
#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_overlay: &'static str,
    single_variable: &'static str,
    label_boundary: &'static str,
    core_height_acceptance_ratio: f64,
    confirmation_atr_acceptance_ratio: f64,
    minimum_touch_gap_bars: usize,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    confirmed_lifecycles: usize,
    complete_shell_lifecycles: usize,
    complete_shell_ratio: f64,
    complete_shell_distinct_symbols: usize,
    complete_shell_hour_event_clusters: usize,
    expanded_lifecycles: usize,
    expanded_distinct_symbols: usize,
    target_symbol: &'static str,
    target_window_start_ms: i64,
    target_window_end_ms: i64,
    target_manual_upper: f64,
    target_manual_lower: f64,
    target_drawing_tolerance: f64,
    target: TargetAudit,
    lifecycles: Vec<VisualShellLifecycleAudit>,
    l1_gate_passed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating body-acceptance shell warmup start")?;
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
    let mut lifecycles = scan_lifecycles(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        variant,
    );
    lifecycles.sort_by(|left, right| {
        left.first_confirmation_time_ms
            .cmp(&right.first_confirmation_time_ms)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });

    let complete = lifecycles
        .iter()
        .filter(|lifecycle| lifecycle.snapshot.shell_complete)
        .collect::<Vec<_>>();
    let expanded = complete
        .iter()
        .copied()
        .filter(|lifecycle| shell_expands_core(lifecycle))
        .collect::<Vec<_>>();
    let complete_shell_ratio = if lifecycles.is_empty() {
        0.0
    } else {
        complete.len() as f64 / lifecycles.len() as f64
    };
    let complete_shell_distinct_symbols = complete
        .iter()
        .map(|lifecycle| lifecycle.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let complete_shell_hour_event_clusters = complete
        .iter()
        .map(|lifecycle| lifecycle.first_confirmation_time_ms / EVENT_CLUSTER_MS)
        .collect::<BTreeSet<_>>()
        .len();
    let expanded_distinct_symbols = expanded
        .iter()
        .map(|lifecycle| lifecycle.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let target = audit_target(&lifecycles);
    let l1_gate_passed = target.passed
        && complete.len() >= MINIMUM_COMPLETE_SHELL_LIFECYCLES
        && complete_shell_distinct_symbols >= MINIMUM_COMPLETE_SHELL_SYMBOLS
        && complete_shell_hour_event_clusters >= MINIMUM_COMPLETE_SHELL_EVENT_CLUSTERS
        && expanded.len() >= MINIMUM_EXPANDED_LIFECYCLES
        && expanded_distinct_symbols >= MINIMUM_EXPANDED_SYMBOLS
        && (MINIMUM_COMPLETE_SHELL_RATIO..=MAXIMUM_COMPLETE_SHELL_RATIO)
            .contains(&complete_shell_ratio);

    let report = L1Report {
        schema_version: "strict_visual_v31_body_acceptance_visual_shell_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "completed_candles_and_active_v31_ranges_only",
        baseline_variant: variant.slug(),
        candidate_overlay: "body_acceptance_visual_shell_v1",
        single_variable:
            "repeat_accepted_candle_body_edges_expand_visual_bounds_without_changing_trade_core",
        label_boundary:
            "No order, entry, exit, MFE, MAE, final R, PnL, or win/loss field was read or serialized.",
        core_height_acceptance_ratio: CORE_HEIGHT_ACCEPTANCE_RATIO,
        confirmation_atr_acceptance_ratio: CONFIRMATION_ATR_ACCEPTANCE_RATIO,
        minimum_touch_gap_bars: MINIMUM_TOUCH_GAP_BARS,
        dataset_fingerprint_sha256,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        confirmed_lifecycles: lifecycles.len(),
        complete_shell_lifecycles: complete.len(),
        complete_shell_ratio,
        complete_shell_distinct_symbols,
        complete_shell_hour_event_clusters,
        expanded_lifecycles: expanded.len(),
        expanded_distinct_symbols,
        target_symbol: TARGET_SYMBOL,
        target_window_start_ms: TARGET_WINDOW_START_MS,
        target_window_end_ms: TARGET_WINDOW_END_MS,
        target_manual_upper: TARGET_MANUAL_UPPER,
        target_manual_lower: TARGET_MANUAL_LOWER,
        target_drawing_tolerance: TARGET_DRAWING_TOLERANCE,
        target,
        lifecycles,
        l1_gate_passed,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "body_acceptance_shell_l1: included={} ranges={} complete={} ({:.1}%) expanded={} symbols={} target={} gate={} output={}",
        report.included_symbols,
        report.confirmed_lifecycles,
        report.complete_shell_lifecycles,
        report.complete_shell_ratio * 100.0,
        report.expanded_lifecycles,
        report.expanded_distinct_symbols,
        report.target.passed,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("body-acceptance visual shell L1 gate failed; stop before Pine or outcome replay");
    }
    Ok(())
}

/// 逐根推进 V31，只记录首次确认发生在评价窗口内的完整横盘生命周期。
fn scan_lifecycles(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Vec<VisualShellLifecycleAudit> {
    let mut audits = Vec::new();
    for symbol in symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualConsolidationState::default();
        let mut active: Option<ActiveLifecycle> = None;
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let event = state.update_for_variant(&symbol.candles, index, symbol.tick_size, variant);
            match event {
                Some(StrictVisualRangeEvent::Confirmed(range)) => {
                    active = Some(ActiveLifecycle {
                        range,
                        latest_inside_snapshot: shell_snapshot(
                            symbol,
                            index,
                            range,
                            indicators.points[range.first_confirmation_index].atr14,
                        ),
                    });
                }
                Some(StrictVisualRangeEvent::ParentUpgraded(range)) => {
                    if let Some(lifecycle) = active.as_mut() {
                        lifecycle.range = range;
                        lifecycle.latest_inside_snapshot = shell_snapshot(
                            symbol,
                            index,
                            range,
                            indicators.points[range.first_confirmation_index].atr14,
                        );
                    }
                }
                Some(StrictVisualRangeEvent::WeakDepartureReturned(returned)) => {
                    if let Some(lifecycle) = active.as_mut() {
                        lifecycle.range = returned.range;
                        lifecycle.latest_inside_snapshot = shell_snapshot(
                            symbol,
                            index,
                            returned.range,
                            indicators.points[returned.range.first_confirmation_index].atr14,
                        );
                    }
                }
                Some(StrictVisualRangeEvent::WeakDeparturePending(_)) => {}
                Some(StrictVisualRangeEvent::UpperBreak(_)) => finalize_lifecycle(
                    &mut audits,
                    symbol,
                    active.take(),
                    evaluation_start_ms,
                    "upper_break",
                ),
                Some(StrictVisualRangeEvent::LowerBreak(_)) => finalize_lifecycle(
                    &mut audits,
                    symbol,
                    active.take(),
                    evaluation_start_ms,
                    "lower_break",
                ),
                Some(StrictVisualRangeEvent::WeakDepartureConsumed(_)) => finalize_lifecycle(
                    &mut audits,
                    symbol,
                    active.take(),
                    evaluation_start_ms,
                    "weak_departure_consumed",
                ),
                Some(StrictVisualRangeEvent::InternalImpulseReset(_)) => finalize_lifecycle(
                    &mut audits,
                    symbol,
                    active.take(),
                    evaluation_start_ms,
                    "internal_impulse_reset",
                ),
                None => {
                    if let Some(lifecycle) = active.as_mut() {
                        if candle.close >= lifecycle.range.lower
                            && candle.close <= lifecycle.range.upper
                        {
                            lifecycle.latest_inside_snapshot = shell_snapshot(
                                symbol,
                                index,
                                lifecycle.range,
                                indicators.points[lifecycle.range.first_confirmation_index].atr14,
                            );
                        }
                    }
                }
            }
        }
        finalize_lifecycle(
            &mut audits,
            symbol,
            active,
            evaluation_start_ms,
            "evaluation_window_end",
        );
    }
    audits
}

fn finalize_lifecycle(
    audits: &mut Vec<VisualShellLifecycleAudit>,
    symbol: &ResearchWindowSymbolCandles,
    active: Option<ActiveLifecycle>,
    evaluation_start_ms: i64,
    end_reason: &'static str,
) {
    let Some(active) = active else {
        return;
    };
    let confirmation_time = symbol.candles[active.range.first_confirmation_index].timestamp_ms;
    if confirmation_time < evaluation_start_ms {
        return;
    }
    let Some(snapshot) = active.latest_inside_snapshot else {
        return;
    };
    audits.push(VisualShellLifecycleAudit {
        symbol: symbol.symbol.clone(),
        range_start_time_ms: symbol.candles[active.range.start_index].timestamp_ms,
        first_confirmation_time_ms: confirmation_time,
        boundary_confirmation_time_ms: symbol.candles[active.range.boundary_confirmation_index]
            .timestamp_ms,
        last_inside_time_ms: snapshot.snapshot_time_ms,
        end_reason,
        range_length_bars: active.range.length_bars,
        snapshot,
    });
}

fn shell_snapshot(
    symbol: &ResearchWindowSymbolCandles,
    snapshot_index: usize,
    range: StrictVisualRangeEvidence,
    confirmation_atr14: Option<f64>,
) -> Option<BodyAcceptanceShellSnapshot> {
    let confirmation_atr14 = confirmation_atr14.filter(|atr| *atr > 0.0)?;
    body_acceptance_shell_snapshot(
        &symbol.candles,
        range,
        snapshot_index,
        confirmation_atr14,
        symbol.tick_size,
    )
}

/// 只用横盘开始到当前完成棒的实体边缘寻找重复接受价位。
fn body_acceptance_shell_snapshot(
    candles: &[Candle],
    range: StrictVisualRangeEvidence,
    snapshot_index: usize,
    confirmation_atr14: f64,
    tick_size: f64,
) -> Option<BodyAcceptanceShellSnapshot> {
    let history = candles.get(range.start_index..=snapshot_index)?;
    let core_height = range.upper - range.lower;
    if core_height <= 0.0 || confirmation_atr14 <= 0.0 || tick_size <= 0.0 {
        return None;
    }
    let acceptance_band = (core_height * CORE_HEIGHT_ACCEPTANCE_RATIO)
        .max(confirmation_atr14 * CONFIRMATION_ATR_ACCEPTANCE_RATIO)
        .max(tick_size * 2.0);
    let body_highs = history
        .iter()
        .map(|candle| candle.open.max(candle.close))
        .collect::<Vec<_>>();
    let body_lows = history
        .iter()
        .map(|candle| candle.open.min(candle.close))
        .collect::<Vec<_>>();
    let accepted_upper = repeated_accepted_edge(&body_highs, acceptance_band, true);
    let accepted_lower = repeated_accepted_edge(&body_lows, acceptance_band, false);
    let visual_upper = accepted_upper
        .map(|(price, _)| range.upper.max(price))
        .unwrap_or(range.upper);
    let visual_lower = accepted_lower
        .map(|(price, _)| range.lower.min(price))
        .unwrap_or(range.lower);
    let raw_high = history
        .iter()
        .map(|candle| candle.high)
        .fold(f64::MIN, f64::max);
    let raw_low = history
        .iter()
        .map(|candle| candle.low)
        .fold(f64::MAX, f64::min);
    Some(BodyAcceptanceShellSnapshot {
        snapshot_time_ms: candles[snapshot_index].timestamp_ms,
        body_sample_count: history.len(),
        confirmation_atr14,
        acceptance_band,
        core_upper: range.upper,
        core_lower: range.lower,
        core_height,
        accepted_body_upper: accepted_upper.map(|(price, _)| price),
        accepted_body_lower: accepted_lower.map(|(price, _)| price),
        upper_touch_groups: accepted_upper.map(|(_, groups)| groups).unwrap_or(0),
        lower_touch_groups: accepted_lower.map(|(_, groups)| groups).unwrap_or(0),
        visual_upper,
        visual_lower,
        upper_expansion_core_heights: (visual_upper - range.upper) / core_height,
        lower_expansion_core_heights: (range.lower - visual_lower) / core_height,
        raw_high,
        raw_low,
        raw_high_exceeds_visual_upper: raw_high > visual_upper + tick_size,
        raw_low_exceeds_visual_lower: raw_low < visual_lower - tick_size,
        shell_complete: accepted_upper.is_some() && accepted_lower.is_some(),
    })
}

/// 极端候选优先；只有两个非连续实体触碰组都位于同一接受带时才返回。
fn repeated_accepted_edge(
    body_edges: &[f64],
    acceptance_band: f64,
    upper_side: bool,
) -> Option<(f64, usize)> {
    let mut candidates = body_edges.to_vec();
    if upper_side {
        candidates.sort_by(|left, right| right.total_cmp(left));
    } else {
        candidates.sort_by(f64::total_cmp);
    }
    candidates.dedup_by(|left, right| left.to_bits() == right.to_bits());
    candidates.into_iter().find_map(|candidate| {
        let groups = touch_groups(body_edges, candidate, acceptance_band);
        (groups >= 2).then_some((candidate, groups))
    })
}

fn touch_groups(body_edges: &[f64], candidate: f64, acceptance_band: f64) -> usize {
    let mut groups = 0;
    let mut last_touch = None;
    for (index, edge) in body_edges.iter().copied().enumerate() {
        if (edge - candidate).abs() <= acceptance_band {
            if last_touch.is_none_or(|last| index - last >= MINIMUM_TOUCH_GAP_BARS) {
                groups += 1;
            }
            last_touch = Some(index);
        }
    }
    groups
}

fn shell_expands_core(lifecycle: &VisualShellLifecycleAudit) -> bool {
    lifecycle.snapshot.upper_expansion_core_heights > f64::EPSILON
        || lifecycle.snapshot.lower_expansion_core_heights > f64::EPSILON
}

/// 目标选择只按预注册时间重叠，不读取视觉距离来挑选更好看的横盘。
fn audit_target(lifecycles: &[VisualShellLifecycleAudit]) -> TargetAudit {
    let selected = lifecycles
        .iter()
        .filter(|lifecycle| lifecycle.symbol == TARGET_SYMBOL)
        .map(|lifecycle| (target_overlap_bars(lifecycle), lifecycle))
        .filter(|(overlap, _)| *overlap > 0)
        .max_by(|left, right| {
            left.0.cmp(&right.0).then_with(|| {
                left.1
                    .first_confirmation_time_ms
                    .cmp(&right.1.first_confirmation_time_ms)
                    .reverse()
            })
        });
    let Some((overlap_bars, lifecycle)) = selected else {
        return TargetAudit {
            selected: None,
            overlap_bars: 0,
            upper_within_drawing_tolerance: false,
            lower_within_drawing_tolerance: false,
            upper_closer_than_core: false,
            lower_closer_than_core: false,
            isolated_upper_wick_excluded: false,
            isolated_lower_wick_excluded: false,
            passed: false,
        };
    };
    let snapshot = &lifecycle.snapshot;
    let upper_within_drawing_tolerance =
        (snapshot.visual_upper - TARGET_MANUAL_UPPER).abs() <= TARGET_DRAWING_TOLERANCE;
    let lower_within_drawing_tolerance =
        (snapshot.visual_lower - TARGET_MANUAL_LOWER).abs() <= TARGET_DRAWING_TOLERANCE;
    let upper_closer_than_core = (snapshot.visual_upper - TARGET_MANUAL_UPPER).abs()
        < (snapshot.core_upper - TARGET_MANUAL_UPPER).abs();
    let lower_closer_than_core = (snapshot.visual_lower - TARGET_MANUAL_LOWER).abs()
        < (snapshot.core_lower - TARGET_MANUAL_LOWER).abs();
    let isolated_upper_wick_excluded = snapshot.raw_high_exceeds_visual_upper;
    let isolated_lower_wick_excluded = snapshot.raw_low_exceeds_visual_lower;
    let passed = overlap_bars >= MINIMUM_TARGET_OVERLAP_BARS
        && snapshot.shell_complete
        && upper_within_drawing_tolerance
        && lower_within_drawing_tolerance
        && upper_closer_than_core
        && lower_closer_than_core
        && isolated_upper_wick_excluded
        && isolated_lower_wick_excluded;
    TargetAudit {
        selected: Some(lifecycle.clone()),
        overlap_bars,
        upper_within_drawing_tolerance,
        lower_within_drawing_tolerance,
        upper_closer_than_core,
        lower_closer_than_core,
        isolated_upper_wick_excluded,
        isolated_lower_wick_excluded,
        passed,
    }
}

fn target_overlap_bars(lifecycle: &VisualShellLifecycleAudit) -> usize {
    let start = lifecycle.range_start_time_ms.max(TARGET_WINDOW_START_MS);
    let end = lifecycle.last_inside_time_ms.min(TARGET_WINDOW_END_MS);
    if end < start {
        0
    } else {
        ((end - start) / CANDLE_INTERVAL_MS + 1) as usize
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

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
            open,
            high,
            low,
            close,
            volume: 100.0,
        }
    }

    fn core_range() -> StrictVisualRangeEvidence {
        StrictVisualRangeEvidence {
            start_index: 0,
            first_confirmation_index: 7,
            boundary_confirmation_index: 7,
            length_bars: 8,
            upper: 102.0,
            lower: 100.0,
            containment_ratio: 1.0,
            direction_efficiency: 0.1,
            edge_transition_count: 3,
            upper_touch_groups: 2,
            lower_touch_groups: 2,
            upper_drift_ratio: 0.1,
            lower_drift_ratio: 0.1,
        }
    }

    #[test]
    fn repeated_body_edges_expand_the_shell_but_isolated_wicks_do_not() {
        let candles = vec![
            candle(0, 100.4, 102.1, 100.2, 101.8),
            candle(1, 101.8, 102.5, 101.7, 102.4),
            candle(2, 99.6, 104.0, 99.4, 101.7),
            candle(3, 101.6, 101.9, 99.5, 99.65),
            candle(4, 101.9, 102.45, 101.8, 102.35),
            candle(5, 99.7, 101.8, 99.3, 101.6),
            candle(6, 101.5, 101.9, 99.6, 99.6),
            candle(7, 99.65, 101.8, 99.2, 101.7),
        ];
        let snapshot = body_acceptance_shell_snapshot(&candles, core_range(), 7, 0.5, 0.01)
            .expect("valid shell");

        assert_eq!(snapshot.visual_upper, 102.4);
        assert_eq!(snapshot.visual_lower, 99.6);
        assert!(snapshot.raw_high_exceeds_visual_upper);
        assert!(snapshot.raw_low_exceeds_visual_lower);
        assert!(snapshot.shell_complete);
    }

    #[test]
    fn one_isolated_body_spike_cannot_expand_the_upper_shell() {
        let candles = vec![
            candle(0, 100.5, 101.9, 100.2, 101.8),
            candle(1, 101.7, 103.1, 101.6, 103.0),
            candle(2, 101.8, 101.9, 100.1, 100.3),
            candle(3, 100.4, 101.8, 100.2, 101.7),
            candle(4, 101.6, 101.9, 100.1, 100.4),
            candle(5, 100.5, 101.8, 100.2, 101.7),
            candle(6, 101.6, 101.9, 100.1, 100.4),
            candle(7, 100.5, 101.8, 100.2, 101.7),
        ];
        let snapshot = body_acceptance_shell_snapshot(&candles, core_range(), 7, 0.5, 0.01)
            .expect("valid shell");

        assert_eq!(snapshot.visual_upper, 102.0);
        assert!(snapshot.raw_high_exceeds_visual_upper);
    }
}
