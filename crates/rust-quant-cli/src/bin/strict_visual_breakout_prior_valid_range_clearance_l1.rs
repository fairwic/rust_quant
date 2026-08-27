//! 对 V30 前方旧有效横盘净空执行 outcome-blind L1 覆盖审计。
//!
//! 本入口只读取突破与确认时点已经完成的横盘证据，不构造订单、退出、MFE、MAE 或收益标签。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core,
    strict_visual_short_signal_gates_pass, Direction, ParityRuleVersion,
    ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal,
    StrictVisualPriorRangeClearanceState, StrictVisualPriorRangeObstruction,
    STRICT_VISUAL_PRIOR_RANGE_LOOKBACK_BARS,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const TARGET_FIRST_SIGNAL_MS: i64 = 1_784_678_400_000;
const TARGET_SECOND_SIGNAL_MS: i64 = 1_784_840_400_000;
const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";

/// 显式 L1 数据身份，不接受 lookback 或净空阈值参数。
struct Args {
    symbols: Vec<String>,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    output: PathBuf,
}

/// 因完整性门禁未进入扫描的显式成员。
#[derive(Serialize)]
struct ExcludedSymbol {
    symbol: String,
    warmup_expected: usize,
    warmup_loaded: usize,
    evaluation_expected: usize,
    evaluation_loaded: usize,
    reason: &'static str,
}

/// 一个通过全部当前版本门禁的确认来源，只含信号时可见字段。
#[derive(Clone, Serialize)]
struct SourceAudit {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    range_start_time_ms: i64,
    range_length_bars: usize,
    upper: f64,
    lower: f64,
}

/// 一个因 V30 前方净空不足被拒绝的确认来源及其冻结旧横盘证据。
#[derive(Clone, Serialize)]
struct RejectionAudit {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    current_range_start_time_ms: i64,
    current_range_length_bars: usize,
    current_upper: f64,
    current_lower: f64,
    prior_range_start_time_ms: i64,
    prior_range_departure_time_ms: i64,
    prior_range_length_bars: usize,
    prior_upper: f64,
    prior_lower: f64,
    clearance: f64,
    current_range_height: f64,
    clearance_in_range_heights: f64,
}

/// 一个变体的确认来源与 V30 拒绝账本。
struct VariantScan {
    sources: Vec<SourceAudit>,
    rejections: Vec<RejectionAudit>,
}

/// L1 报告不包含任何成交后字段。
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
    lookback_bars: usize,
    clearance_limit_range_heights: f64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    baseline_confirmed_sources: usize,
    candidate_confirmed_sources: usize,
    rejected_sources: usize,
    rejected_source_percent: f64,
    candidate_long_sources: usize,
    candidate_short_sources: usize,
    candidate_distinct_symbols: usize,
    candidate_direction_hour_event_clusters: usize,
    first_target_found_and_rejected: bool,
    second_target_found_and_rejected: bool,
    l1_gate_passed: bool,
    rejections: Vec<RejectionAudit>,
    candidate_sources: Vec<SourceAudit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating prior range L1 warmup start")?;
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
    let candidate_variant = StrictVisualBreakoutResearchVariant::V30PriorValidRangeClearance;
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
    if !baseline.rejections.is_empty() {
        bail!("baseline unexpectedly produced prior-range rejections");
    }

    let rejected_sources = candidate.rejections.len();
    let baseline_confirmed_sources = baseline.sources.len();
    let rejected_source_percent = if baseline_confirmed_sources == 0 {
        0.0
    } else {
        rejected_sources as f64 / baseline_confirmed_sources as f64 * 100.0
    };
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
    let first_target_found_and_rejected =
        target_rejected(&candidate.rejections, TARGET_FIRST_SIGNAL_MS);
    let second_target_found_and_rejected =
        target_rejected(&candidate.rejections, TARGET_SECOND_SIGNAL_MS);
    let l1_gate_passed = first_target_found_and_rejected
        && second_target_found_and_rejected
        && rejected_sources >= 3
        && rejected_source_percent <= 60.0
        && candidate.sources.len() >= 30
        && candidate_long_sources >= 15
        && candidate_short_sources >= 15
        && candidate_distinct_symbols >= 8
        && candidate_direction_hour_event_clusters >= 10
        && baseline_confirmed_sources == candidate.sources.len() + rejected_sources;

    let report = L1Report {
        schema_version: "strict_visual_prior_valid_range_clearance_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "explicit_latest_confirmed_window",
        baseline_variant: baseline_variant.slug(),
        candidate_variant: candidate_variant.slug(),
        single_variable:
            "reject_unresolved_prior_valid_range_within_one_current_range_height_over_96_bars",
        label_boundary:
            "No order, entry, exit, MFE, MAE, final R, or win/loss field was read or serialized.",
        dataset_fingerprint_sha256,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        lookback_bars: STRICT_VISUAL_PRIOR_RANGE_LOOKBACK_BARS,
        clearance_limit_range_heights: 1.0,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        baseline_confirmed_sources,
        candidate_confirmed_sources: candidate.sources.len(),
        rejected_sources,
        rejected_source_percent,
        candidate_long_sources,
        candidate_short_sources,
        candidate_distinct_symbols,
        candidate_direction_hour_event_clusters,
        first_target_found_and_rejected,
        second_target_found_and_rejected,
        l1_gate_passed,
        rejections: candidate.rejections,
        candidate_sources: candidate.sources,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "prior_valid_range_l1: included={} baseline={} candidate={} rejected={} rejected_pct={:.2} targets={}/{} long={} short={} clusters={} gate={} output={}",
        report.included_symbols,
        report.baseline_confirmed_sources,
        report.candidate_confirmed_sources,
        report.rejected_sources,
        report.rejected_source_percent,
        report.first_target_found_and_rejected,
        report.second_target_found_and_rejected,
        report.candidate_long_sources,
        report.candidate_short_sources,
        report.candidate_direction_hour_event_clusters,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("V30 L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

/// 在一个冻结变体上推进全部完成棒，V30 阻挡证据在突破棒时冻结。
fn scan_variant(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> VariantScan {
    let mut sources = Vec::new();
    let mut rejections = Vec::new();
    for symbol in symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualPriorRangeClearanceState::default();
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let point = &indicators.points[index];
            if point.atr14.is_none_or(|atr| atr <= 0.0) {
                continue;
            }
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
            if let Some(signal) = update.confirmed.filter(|signal| {
                signal.direction != Direction::Short
                    || strict_visual_short_signal_gates_pass(
                        signal,
                        indicators.points[signal.breakout_index].ema144,
                        variant,
                    )
            }) {
                sources.push(source_audit(symbol, signal));
            }
            if let Some((signal, obstruction)) = update.rejected {
                rejections.push(rejection_audit(symbol, signal, obstruction));
            }
        }
    }
    VariantScan {
        sources,
        rejections,
    }
}

/// 把一次确认来源转换为稳定时间身份。
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
        range_length_bars: signal.range.length_bars,
        upper: signal.range.upper,
        lower: signal.range.lower,
    }
}

/// 序列化 V30 冻结的当前与旧横盘净空证据。
fn rejection_audit(
    symbol: &ResearchWindowSymbolCandles,
    signal: StrictVisualBreakoutSignal,
    obstruction: StrictVisualPriorRangeObstruction,
) -> RejectionAudit {
    RejectionAudit {
        symbol: symbol.symbol.clone(),
        direction: signal.direction,
        breakout_time_ms: symbol.candles[signal.breakout_index].timestamp_ms,
        signal_time_ms: symbol.candles[signal.signal_index].timestamp_ms,
        current_range_start_time_ms: symbol.candles[signal.range.start_index].timestamp_ms,
        current_range_length_bars: signal.range.length_bars,
        current_upper: signal.range.upper,
        current_lower: signal.range.lower,
        prior_range_start_time_ms: symbol.candles[obstruction.prior.range.start_index].timestamp_ms,
        prior_range_departure_time_ms: symbol.candles[obstruction.prior.departure_index]
            .timestamp_ms,
        prior_range_length_bars: obstruction.prior.range.length_bars,
        prior_upper: obstruction.prior.range.upper,
        prior_lower: obstruction.prior.range.lower,
        clearance: obstruction.clearance,
        current_range_height: obstruction.current_range_height,
        clearance_in_range_heights: obstruction.clearance_in_range_heights,
    }
}

/// 目标样本必须按币种与当前数据库信号时间精确命中。
fn target_rejected(rejections: &[RejectionAudit], signal_time_ms: i64) -> bool {
    rejections
        .iter()
        .any(|item| item.symbol == TARGET_SYMBOL && item.signal_time_ms == signal_time_ms)
}

/// 多空分离后按相邻不超过一小时的确认链归并市场事件。
fn direction_hour_event_clusters(sources: &[SourceAudit]) -> usize {
    let mut events = sources
        .iter()
        .map(|source| {
            (
                match source.direction {
                    Direction::Long => 0_u8,
                    Direction::Short => 1_u8,
                },
                source.signal_time_ms,
            )
        })
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

/// 按预热与评价窗口完整性隔离成员，禁止补值。
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

/// 原子发布机器报告，避免长扫描留下半份 JSON。
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
