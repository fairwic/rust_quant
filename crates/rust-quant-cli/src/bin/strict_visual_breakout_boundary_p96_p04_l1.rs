//! 对严格视觉横盘 P90/P10 与 P96/P04 边界执行 outcome-blind L1 身份审计。
//!
//! 本入口只读取横盘与信号形成时已经完成的 K 线，不构造订单、退出、MFE、MAE 或收益标签。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core,
    strict_visual_short_signal_gates_pass, Direction, ParityRuleVersion,
    ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal,
    StrictVisualLongEntryEvent, StrictVisualLongEntryState, StrictVisualRangeEvent,
    StrictVisualRangeEvidence,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;

/// 显式 L1 数据身份；不开放百分位、成本或交易参数开关。
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

/// 同一完成时点公开的横盘边界快照，只包含当时可见字段。
#[derive(Clone, Serialize)]
struct RangeAudit {
    symbol: String,
    event_kind: &'static str,
    event_time_ms: i64,
    range_start_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    length_bars: usize,
    upper: f64,
    lower: f64,
}

/// 一次完成确认的突破来源，不包含成交后字段。
#[derive(Clone, Serialize)]
struct SourceAudit {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    range_start_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    range_length_bars: usize,
    upper: f64,
    lower: f64,
}

/// 单个变体在同一数据身份上的无结果标签扫描结果。
struct VariantScan {
    ranges: Vec<RangeAudit>,
    sources: Vec<SourceAudit>,
}

/// 一个基线与候选都公开过的相同横盘窗口边界对照。
#[derive(Serialize)]
struct SharedRangeAudit {
    symbol: String,
    event_kind: &'static str,
    event_time_ms: i64,
    range_start_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    length_bars: usize,
    baseline_upper: f64,
    candidate_upper: f64,
    baseline_lower: f64,
    candidate_lower: f64,
    monotonic: bool,
}

/// L1 报告只验证边界性质、来源身份变化与覆盖，不读取任何结果标签。
#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_variant: &'static str,
    single_variable: &'static str,
    label_boundary: &'static str,
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
    shared_range_windows: usize,
    monotonic_range_violations: usize,
    baseline_only_confirmed_identities: usize,
    candidate_only_confirmed_identities: usize,
    changed_confirmed_identities: usize,
    l1_gate_passed: bool,
    shared_ranges: Vec<SharedRangeAudit>,
    baseline_sources: Vec<SourceAudit>,
    candidate_sources: Vec<SourceAudit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating P96/P04 L1 warmup start")?;
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
    let candidate_variant = StrictVisualBreakoutResearchVariant::V29BoundaryP96P04;
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

    let shared_ranges = compare_shared_ranges(&baseline.ranges, &candidate.ranges);
    let monotonic_range_violations = shared_ranges
        .iter()
        .filter(|audit| !audit.monotonic)
        .count();
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
    let l1_gate_passed = candidate.sources.len() >= 30
        && candidate_long_sources >= 15
        && candidate_short_sources >= 15
        && candidate_distinct_symbols >= 8
        && candidate_direction_hour_event_clusters >= 10
        && !shared_ranges.is_empty()
        && monotonic_range_violations == 0
        && changed_confirmed_identities >= 3;

    let report = L1Report {
        schema_version: "strict_visual_boundary_p96_p04_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "explicit_latest_confirmed_window",
        baseline_variant: baseline_variant.slug(),
        candidate_variant: candidate_variant.slug(),
        single_variable: "range_high_percentile_90_to_96_and_low_percentile_10_to_4",
        label_boundary:
            "No order, entry, exit, MFE, MAE, final R, or win/loss field was read or serialized.",
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
        shared_range_windows: shared_ranges.len(),
        monotonic_range_violations,
        baseline_only_confirmed_identities,
        candidate_only_confirmed_identities,
        changed_confirmed_identities,
        l1_gate_passed,
        shared_ranges,
        baseline_sources: baseline.sources,
        candidate_sources: candidate.sources,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "boundary_p96_p04_l1: included={} baseline={} candidate={} long={} short={} shared_ranges={} violations={} changed={} gate={} output={}",
        report.included_symbols,
        report.baseline_confirmed_sources,
        report.candidate_confirmed_sources,
        report.candidate_long_sources,
        report.candidate_short_sources,
        report.shared_range_windows,
        report.monotonic_range_violations,
        report.changed_confirmed_identities,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("V29 L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

/// 在一个冻结变体上逐根推进，并只保留当时可见的横盘与确认来源。
fn scan_variant(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> VariantScan {
    let mut ranges = Vec::new();
    let mut sources = Vec::new();
    for symbol in symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualLongEntryState::default();
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let point = &indicators.points[index];
            if point.atr14.is_none_or(|atr| atr <= 0.0) {
                continue;
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
            if candle.timestamp_ms < evaluation_start_ms {
                continue;
            }
            match event {
                Some(StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(
                    range,
                ))) => ranges.push(range_audit(symbol, "confirmed", index, range)),
                Some(StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::ParentUpgraded(range),
                )) => ranges.push(range_audit(symbol, "parent_upgraded", index, range)),
                Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal))
                    if signal.direction != Direction::Short
                        || strict_visual_short_signal_gates_pass(
                            &signal,
                            indicators.points[signal.breakout_index].ema144,
                            variant,
                        ) =>
                {
                    sources.push(source_audit(symbol, signal));
                }
                _ => {}
            }
        }
    }
    VariantScan { ranges, sources }
}

/// 把索引字段转换成可跨两次扫描稳定比较的时间身份。
fn range_audit(
    symbol: &ResearchWindowSymbolCandles,
    event_kind: &'static str,
    event_index: usize,
    range: StrictVisualRangeEvidence,
) -> RangeAudit {
    RangeAudit {
        symbol: symbol.symbol.clone(),
        event_kind,
        event_time_ms: symbol.candles[event_index].timestamp_ms,
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
    }
}

/// 冻结一次确认来源的结构身份，价格边界只用于审计而不参与结果选择。
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

type RangeIdentity = (String, &'static str, i64, i64, i64, usize);

/// 对同一原始窗口验证 P96 上沿不低于 P90、P04 下沿不高于 P10。
fn compare_shared_ranges(
    baseline: &[RangeAudit],
    candidate: &[RangeAudit],
) -> Vec<SharedRangeAudit> {
    let baseline_by_identity = baseline
        .iter()
        .map(|range| (range_identity(range), range))
        .collect::<BTreeMap<_, _>>();
    candidate
        .iter()
        .filter_map(|candidate_range| {
            let baseline_range = baseline_by_identity.get(&range_identity(candidate_range))?;
            Some(SharedRangeAudit {
                symbol: candidate_range.symbol.clone(),
                event_kind: candidate_range.event_kind,
                event_time_ms: candidate_range.event_time_ms,
                range_start_time_ms: candidate_range.range_start_time_ms,
                boundary_confirmation_time_ms: candidate_range.boundary_confirmation_time_ms,
                length_bars: candidate_range.length_bars,
                baseline_upper: baseline_range.upper,
                candidate_upper: candidate_range.upper,
                baseline_lower: baseline_range.lower,
                candidate_lower: candidate_range.lower,
                monotonic: candidate_range.upper >= baseline_range.upper
                    && candidate_range.lower <= baseline_range.lower,
            })
        })
        .collect()
}

/// 横盘事件身份排除边界值，避免用待验证价格反过来定义“相同窗口”。
fn range_identity(range: &RangeAudit) -> RangeIdentity {
    (
        range.symbol.clone(),
        range.event_kind,
        range.event_time_ms,
        range.range_start_time_ms,
        range.boundary_confirmation_time_ms,
        range.length_bars,
    )
}

type ConfirmedIdentity = (String, u8, i64, i64, i64, i64, usize);

/// 确认来源身份不含边界价格，边界改变导致的来源增删才会进入对称差。
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

/// 多空分离后按相邻不超过一小时的确认链归并市场事件。
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

/// 为不能实现全序的方向枚举提供固定审计顺序。
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

/// 验证显式成员非空且无重复，保持输入顺序作为身份的一部分。
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

/// 解析 Unix 毫秒参数。
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
