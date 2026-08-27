//! 对 V31 内部冲击棒实体占区间高度门禁执行 outcome-blind L1 审计。
//!
//! 本入口只推进当时已完成的 K 线与横盘状态，不构造订单、退出、MFE、MAE 或收益标签。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core,
    strict_visual_short_signal_gates_pass, Direction, ParityRuleVersion,
    ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal,
    StrictVisualLongEntryEvent, StrictVisualLongEntryState, StrictVisualRangeEvent,
    StrictVisualRangeEvidence, STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
/// 用户黄色框在状态机中完成确认的 2026-07-30 09:30 BJT，Unix 毫秒。
const TARGET_CONFIRMATION_TIME_MS: i64 = 1_785_375_000_000;
/// 目标 16 根横盘向左回画后的首根 K 线，2026-07-30 05:45 BJT。
const TARGET_RANGE_START_TIME_MS: i64 = 1_785_361_500_000;
const TARGET_RANGE_LENGTH_BARS: usize = 16;
const TARGET_RANGE_UPPER: f64 = 64_088.0;
const TARGET_RANGE_LOWER: f64 = 63_580.0;
const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_DIAGNOSTIC_RADIUS_MS: i64 = 6 * 60 * 60 * 1_000;

/// 显式 L1 数据身份；不开放冲击比例、成本或交易参数开关。
struct Args {
    /// 调用方冻结的 OKX USDT 永续成员列表。
    symbols: Vec<String>,
    /// 评价窗口首根 K 线开盘时间，Unix 毫秒。
    evaluation_start_ms: i64,
    /// 评价窗口不包含的结束时间，Unix 毫秒。
    evaluation_end_exclusive_ms: i64,
    /// 原子发布的 L1 JSON 路径。
    output: PathBuf,
}

/// 因预热或评价窗口不完整而被隔离的成员。
#[derive(Serialize)]
struct ExcludedSymbol {
    /// OKX USDT 永续交易对。
    symbol: String,
    /// 60 天预热理论 K 线数。
    warmup_expected: usize,
    /// 60 天预热实际确认 K 线数。
    warmup_loaded: usize,
    /// 评价窗口理论 K 线数。
    evaluation_expected: usize,
    /// 评价窗口实际确认 K 线数。
    evaluation_loaded: usize,
    /// 成员被排除的可审计原因。
    reason: &'static str,
}

/// 一次完成确认的突破来源，只保留信号时可见身份与冻结区间。
#[derive(Clone, Serialize)]
struct SourceAudit {
    /// OKX USDT 永续交易对。
    symbol: String,
    /// 突破方向。
    direction: Direction,
    /// 强突破棒开盘时间，Unix 毫秒。
    breakout_time_ms: i64,
    /// 首次合法接受棒开盘时间，Unix 毫秒。
    signal_time_ms: i64,
    /// 冻结横盘首根 K 线时间，Unix 毫秒。
    range_start_time_ms: i64,
    /// 当前边界确认时间，Unix 毫秒。
    boundary_confirmation_time_ms: i64,
    /// 冻结横盘离散长度。
    range_length_bars: usize,
    /// 冻结横盘上沿。
    upper: f64,
    /// 冻结横盘下沿。
    lower: f64,
}

/// V31 在活动横盘内部消费结构的一次当时可见记录。
#[derive(Clone, Serialize)]
struct ResetAudit {
    /// OKX USDT 永续交易对。
    symbol: String,
    /// 结构重置棒开盘时间，Unix 毫秒。
    reset_time_ms: i64,
    /// 被消费横盘首根 K 线时间，Unix 毫秒。
    range_start_time_ms: i64,
    /// 被消费横盘边界确认时间，Unix 毫秒。
    boundary_confirmation_time_ms: i64,
    /// 被消费横盘长度。
    range_length_bars: usize,
    /// 被消费横盘上沿。
    upper: f64,
    /// 被消费横盘下沿。
    lower: f64,
    /// 重置棒实体除以冻结横盘高度。
    body_to_range_height: f64,
}

/// 目标时点前后六小时内的横盘状态事件，用于解释视觉框的实际确认时点。
#[derive(Clone, Serialize)]
struct NearbyRangeEventAudit {
    /// 结构确认、升级、离区或内部重置事件。
    event_kind: &'static str,
    /// 事件完成棒开盘时间，Unix 毫秒。
    event_time_ms: i64,
    /// 事件引用横盘的首根 K 线时间，Unix 毫秒。
    range_start_time_ms: i64,
    /// 事件引用横盘的首次确认时间，Unix 毫秒。
    first_confirmation_time_ms: i64,
    /// 事件引用横盘的当前边界确认时间，Unix 毫秒。
    boundary_confirmation_time_ms: i64,
    /// 事件引用横盘长度。
    range_length_bars: usize,
    /// 冻结上沿。
    upper: f64,
    /// 冻结下沿。
    lower: f64,
    /// 截至该事件完成棒，窗口内最大实体除以冻结区间高度。
    maximum_body_to_range_height: f64,
    /// 最大实体所属 K 线开盘时间，Unix 毫秒。
    maximum_body_time_ms: i64,
}

/// 目标时点活动横盘及截至该时点最强内部实体的因果快照。
#[derive(Clone, Serialize)]
struct TargetRangeAudit {
    /// 横盘首根 K 线时间，Unix 毫秒。
    range_start_time_ms: i64,
    /// 横盘首次确认时间，Unix 毫秒。
    first_confirmation_time_ms: i64,
    /// 当前边界确认时间，Unix 毫秒。
    boundary_confirmation_time_ms: i64,
    /// 冻结横盘长度。
    range_length_bars: usize,
    /// 冻结上沿。
    upper: f64,
    /// 冻结下沿。
    lower: f64,
    /// 截至目标完成棒的最大实体 / 冻结区间高度。
    maximum_body_to_range_height: f64,
    /// 最大实体所属 K 线开盘时间，Unix 毫秒。
    maximum_body_time_ms: i64,
}

/// 同一变体在目标棒处理前后的活动横盘。
#[derive(Default, Serialize)]
struct TargetSnapshot {
    /// 目标完成棒进入状态机前仍有效的横盘；`None` 表示此前已消费。
    before: Option<TargetRangeAudit>,
    /// 目标完成棒处理后的横盘；`None` 表示该棒消费或没有形成结构。
    after: Option<TargetRangeAudit>,
}

/// 单个变体的无结果标签扫描结果。
struct VariantScan {
    sources: Vec<SourceAudit>,
    resets: Vec<ResetAudit>,
    nearby_events: Vec<NearbyRangeEventAudit>,
    target: TargetSnapshot,
}

/// L1 报告只验证目标、覆盖和身份变化，不序列化任何成交后字段。
#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_variant: &'static str,
    single_variable: &'static str,
    label_boundary: &'static str,
    threshold: f64,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    baseline_confirmed_sources: usize,
    candidate_confirmed_sources: usize,
    candidate_long_sources: usize,
    candidate_short_sources: usize,
    candidate_distinct_symbols: usize,
    candidate_direction_hour_event_clusters: usize,
    baseline_only_confirmed_identities: usize,
    candidate_only_confirmed_identities: usize,
    changed_confirmed_identities: usize,
    candidate_internal_impulse_resets: usize,
    candidate_reset_distinct_symbols: usize,
    target_symbol: &'static str,
    target_confirmation_time_ms: i64,
    target_range_start_time_ms: i64,
    target_range_length_bars: usize,
    target_range_upper: f64,
    target_range_lower: f64,
    target_baseline_present: bool,
    target_baseline_crosses_threshold: bool,
    target_same_range_absent_from_candidate: bool,
    target_candidate_rejected: bool,
    baseline_target: TargetSnapshot,
    candidate_target: TargetSnapshot,
    baseline_target_nearby_events: Vec<NearbyRangeEventAudit>,
    candidate_target_nearby_events: Vec<NearbyRangeEventAudit>,
    candidate_resets: Vec<ResetAudit>,
    baseline_sources: Vec<SourceAudit>,
    candidate_sources: Vec<SourceAudit>,
    l1_gate_passed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating V31 L1 warmup start")?;
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

    let baseline_variant = StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay;
    let candidate_variant = StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard;
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

    let baseline_identities = confirmed_identities(&baseline.sources);
    let candidate_identities = confirmed_identities(&candidate.sources);
    let baseline_only_confirmed_identities = baseline_identities
        .difference(&candidate_identities)
        .count();
    let candidate_only_confirmed_identities = candidate_identities
        .difference(&baseline_identities)
        .count();
    let changed_confirmed_identities =
        baseline_only_confirmed_identities + candidate_only_confirmed_identities;
    let candidate_long_sources = candidate
        .sources
        .iter()
        .filter(|source| source.direction == Direction::Long)
        .count();
    let candidate_short_sources = candidate.sources.len() - candidate_long_sources;
    let candidate_distinct_symbols = candidate
        .sources
        .iter()
        .map(|source| source.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let candidate_direction_hour_event_clusters = direction_hour_event_clusters(&candidate.sources);
    let candidate_reset_distinct_symbols = candidate
        .resets
        .iter()
        .map(|reset| reset.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    let baseline_target_event = baseline
        .nearby_events
        .iter()
        .find(|event| event.event_kind == "confirmed" && is_frozen_target_range(event));
    let target_baseline_present = baseline_target_event.is_some();
    let target_baseline_crosses_threshold = baseline_target_event.is_some_and(|event| {
        event.maximum_body_to_range_height >= STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT
    });
    let target_same_range_absent_from_candidate = candidate
        .nearby_events
        .iter()
        .all(|event| !is_frozen_target_range(event));
    let target_candidate_rejected = target_baseline_present
        && target_baseline_crosses_threshold
        && target_same_range_absent_from_candidate;
    let l1_gate_passed = target_baseline_present
        && target_baseline_crosses_threshold
        && target_same_range_absent_from_candidate
        && target_candidate_rejected
        && candidate.resets.len() >= 3
        && changed_confirmed_identities >= 3
        && candidate.sources.len() >= 30
        && candidate_long_sources >= 15
        && candidate_short_sources >= 15
        && candidate_distinct_symbols >= 8
        && candidate_direction_hour_event_clusters >= 10;

    let report = L1Report {
        schema_version: "strict_visual_internal_impulse_body_guard_causal_target_l1_v2",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "explicit_latest_confirmed_window",
        baseline_variant: baseline_variant.slug(),
        candidate_variant: candidate_variant.slug(),
        single_variable: "maximum_internal_candle_body_divided_by_frozen_range_height_below_0_50",
        label_boundary:
            "No order, entry, exit, MFE, MAE, final R, or win/loss field was read or serialized.",
        threshold: STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT,
        dataset_fingerprint_sha256,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        baseline_confirmed_sources: baseline.sources.len(),
        candidate_confirmed_sources: candidate.sources.len(),
        candidate_long_sources,
        candidate_short_sources,
        candidate_distinct_symbols,
        candidate_direction_hour_event_clusters,
        baseline_only_confirmed_identities,
        candidate_only_confirmed_identities,
        changed_confirmed_identities,
        candidate_internal_impulse_resets: candidate.resets.len(),
        candidate_reset_distinct_symbols,
        target_symbol: TARGET_SYMBOL,
        target_confirmation_time_ms: TARGET_CONFIRMATION_TIME_MS,
        target_range_start_time_ms: TARGET_RANGE_START_TIME_MS,
        target_range_length_bars: TARGET_RANGE_LENGTH_BARS,
        target_range_upper: TARGET_RANGE_UPPER,
        target_range_lower: TARGET_RANGE_LOWER,
        target_baseline_present,
        target_baseline_crosses_threshold,
        target_same_range_absent_from_candidate,
        target_candidate_rejected,
        baseline_target: baseline.target,
        candidate_target: candidate.target,
        baseline_target_nearby_events: baseline.nearby_events,
        candidate_target_nearby_events: candidate.nearby_events,
        candidate_resets: candidate.resets,
        baseline_sources: baseline.sources,
        candidate_sources: candidate.sources,
        l1_gate_passed,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "internal_impulse_l1: included={} baseline={} candidate={} long={} short={} resets={} changed={} target_present={} target_threshold={} target_absent={} target_rejected={} gate={} output={}",
        report.included_symbols,
        report.baseline_confirmed_sources,
        report.candidate_confirmed_sources,
        report.candidate_long_sources,
        report.candidate_short_sources,
        report.candidate_internal_impulse_resets,
        report.changed_confirmed_identities,
        report.target_baseline_present,
        report.target_baseline_crosses_threshold,
        report.target_same_range_absent_from_candidate,
        report.target_candidate_rejected,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("V31 L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

/// 逐根推进一个冻结变体，并只记录信号完成时可见字段。
fn scan_variant(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> VariantScan {
    let mut sources = Vec::new();
    let mut resets = Vec::new();
    let mut nearby_events = Vec::new();
    let mut target = TargetSnapshot::default();
    for symbol in symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualLongEntryState::default();
        let mut active_range: Option<StrictVisualRangeEvidence> = None;
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let point = &indicators.points[index];
            if point.atr14.is_none_or(|atr| atr <= 0.0) {
                continue;
            }
            if symbol.symbol == TARGET_SYMBOL && candle.timestamp_ms == TARGET_CONFIRMATION_TIME_MS
            {
                target.before = active_range
                    .filter(|range| range.start_index < index)
                    .map(|range| target_range_audit(symbol, index - 1, range));
            }
            let event = state.update(
                &symbol.candles,
                index,
                symbol.tick_size,
                point.atr14,
                point.volume_event,
                point.filtered_volume_ratio,
                None,
                variant,
            );
            match event {
                Some(StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(
                    range,
                ))) => {
                    record_nearby_event(&mut nearby_events, symbol, index, "confirmed", range);
                    active_range = Some(range);
                }
                Some(StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::ParentUpgraded(range),
                )) => {
                    record_nearby_event(
                        &mut nearby_events,
                        symbol,
                        index,
                        "parent_upgraded",
                        range,
                    );
                    active_range = Some(range);
                }
                Some(StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(
                    range,
                ))) => {
                    record_nearby_event(&mut nearby_events, symbol, index, "upper_break", range);
                    active_range = None;
                }
                Some(StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(
                    range,
                ))) => {
                    record_nearby_event(&mut nearby_events, symbol, index, "lower_break", range);
                    active_range = None;
                }
                Some(StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDeparturePending(pending),
                )) => record_nearby_event(
                    &mut nearby_events,
                    symbol,
                    index,
                    "weak_departure_pending",
                    pending.range,
                ),
                Some(StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDepartureReturned(returned),
                )) => record_nearby_event(
                    &mut nearby_events,
                    symbol,
                    index,
                    "weak_departure_returned",
                    returned.range,
                ),
                Some(StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDepartureConsumed(consumed),
                )) => {
                    record_nearby_event(
                        &mut nearby_events,
                        symbol,
                        index,
                        "weak_departure_consumed",
                        consumed.range,
                    );
                    active_range = None;
                }
                Some(StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::InternalImpulseReset(reset),
                )) => {
                    record_nearby_event(
                        &mut nearby_events,
                        symbol,
                        index,
                        "internal_impulse_reset",
                        reset.range,
                    );
                    active_range = None;
                    if candle.timestamp_ms >= evaluation_start_ms {
                        resets.push(ResetAudit {
                            symbol: symbol.symbol.clone(),
                            reset_time_ms: candle.timestamp_ms,
                            range_start_time_ms: symbol.candles[reset.range.start_index]
                                .timestamp_ms,
                            boundary_confirmation_time_ms: symbol.candles
                                [reset.range.boundary_confirmation_index]
                                .timestamp_ms,
                            range_length_bars: reset.range.length_bars,
                            upper: reset.range.upper,
                            lower: reset.range.lower,
                            body_to_range_height: reset.body_to_range_height,
                        });
                    }
                }
                Some(StrictVisualLongEntryEvent::AcceptanceArmed(signal)) => {
                    record_nearby_event(
                        &mut nearby_events,
                        symbol,
                        index,
                        "acceptance_armed",
                        signal.range,
                    );
                    active_range = None;
                }
                Some(StrictVisualLongEntryEvent::DirectSignal(signal)) => {
                    record_nearby_event(
                        &mut nearby_events,
                        symbol,
                        index,
                        "direct_signal",
                        signal.range,
                    );
                    active_range = None;
                }
                Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal))
                    if candle.timestamp_ms >= evaluation_start_ms
                        && (signal.direction != Direction::Short
                            || strict_visual_short_signal_gates_pass(
                                &signal,
                                indicators.points[signal.breakout_index].ema144,
                                variant,
                            )) =>
                {
                    sources.push(source_audit(symbol, signal));
                }
                _ => {}
            }
            if symbol.symbol == TARGET_SYMBOL && candle.timestamp_ms == TARGET_CONFIRMATION_TIME_MS
            {
                target.after = active_range.map(|range| target_range_audit(symbol, index, range));
            }
        }
    }
    VariantScan {
        sources,
        resets,
        nearby_events,
        target,
    }
}

/// 只保留目标币种附近的信号时事件，避免机器报告重复序列化全市场状态流。
fn record_nearby_event(
    events: &mut Vec<NearbyRangeEventAudit>,
    symbol: &ResearchWindowSymbolCandles,
    event_index: usize,
    event_kind: &'static str,
    range: StrictVisualRangeEvidence,
) {
    let event_time_ms = symbol.candles[event_index].timestamp_ms;
    if symbol.symbol != TARGET_SYMBOL
        || (event_time_ms - TARGET_CONFIRMATION_TIME_MS).abs() > TARGET_DIAGNOSTIC_RADIUS_MS
    {
        return;
    }
    let range_audit = target_range_audit(symbol, event_index, range);
    events.push(NearbyRangeEventAudit {
        event_kind,
        event_time_ms,
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
        maximum_body_to_range_height: range_audit.maximum_body_to_range_height,
        maximum_body_time_ms: range_audit.maximum_body_time_ms,
    });
}

/// 精确匹配预注册黄色框，不允许扫描后切换到附近其他横盘。
fn is_frozen_target_range(event: &NearbyRangeEventAudit) -> bool {
    event.event_time_ms == TARGET_CONFIRMATION_TIME_MS
        && event.range_start_time_ms == TARGET_RANGE_START_TIME_MS
        && event.first_confirmation_time_ms == TARGET_CONFIRMATION_TIME_MS
        && event.boundary_confirmation_time_ms == TARGET_CONFIRMATION_TIME_MS
        && event.range_length_bars == TARGET_RANGE_LENGTH_BARS
        && event.upper.to_bits() == TARGET_RANGE_UPPER.to_bits()
        && event.lower.to_bits() == TARGET_RANGE_LOWER.to_bits()
}

/// 将目标活动横盘及截至目标完成棒的最大内部实体冻结为可核图字段。
fn target_range_audit(
    symbol: &ResearchWindowSymbolCandles,
    target_index: usize,
    range: StrictVisualRangeEvidence,
) -> TargetRangeAudit {
    let height = range.upper - range.lower;
    let (maximum_body_index, maximum_body_to_range_height) = symbol.candles
        [range.start_index..=target_index]
        .iter()
        .enumerate()
        .map(|(offset, candle)| {
            (
                range.start_index + offset,
                (candle.close - candle.open).abs() / height,
            )
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .expect("active range always contains at least one completed candle");
    TargetRangeAudit {
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
        maximum_body_to_range_height,
        maximum_body_time_ms: symbol.candles[maximum_body_index].timestamp_ms,
    }
}

/// 将索引字段转换成跨变体稳定比较的确认来源身份。
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
        upper: signal.range.upper,
        lower: signal.range.lower,
    }
}

type ConfirmedIdentity = (String, u8, i64, i64, i64, i64, usize);

/// 确认身份排除价格结果字段，避免待验证门禁反过来定义“相同来源”。
fn confirmed_identities(sources: &[SourceAudit]) -> BTreeSet<ConfirmedIdentity> {
    sources
        .iter()
        .map(|source| {
            (
                source.symbol.clone(),
                direction_order(source.direction),
                source.breakout_time_ms,
                source.signal_time_ms,
                source.range_start_time_ms,
                source.boundary_confirmation_time_ms,
                source.range_length_bars,
            )
        })
        .collect()
}

/// 多空分离后按相邻不超过一小时的确认链归并事件簇。
fn direction_hour_event_clusters(sources: &[SourceAudit]) -> usize {
    let mut events = sources
        .iter()
        .map(|source| (direction_order(source.direction), source.signal_time_ms))
        .collect::<Vec<_>>();
    events.sort_unstable();
    let mut clusters = 0;
    let mut previous: Option<(u8, i64)> = None;
    for (direction, time) in events {
        if previous.is_none_or(|(last_direction, last_time)| {
            last_direction != direction || time - last_time > EVENT_CLUSTER_MS
        }) {
            clusters += 1;
        }
        previous = Some((direction, time));
    }
    clusters
}

const fn direction_order(direction: Direction) -> u8 {
    match direction {
        Direction::Long => 0,
        Direction::Short => 1,
    }
}

/// 按预热与评价窗口完整性隔离成员，禁止补值或使用部分窗口。
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

/// 数据指纹绑定显式成员、窗口与全部已确认 OHLCV。
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

/// 解析显式研究参数，拒绝运行后可调策略参数。
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

/// 验证显式成员非空且无重复，保持输入顺序作为数据身份的一部分。
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

/// 先写临时文件再原子发布，避免长扫描留下半份机器报告。
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
