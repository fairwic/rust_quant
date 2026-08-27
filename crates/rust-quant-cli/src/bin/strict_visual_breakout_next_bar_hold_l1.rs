//! 对 V23 首次接受确认追加一根完成棒守稳，执行 outcome-blind L1 覆盖审计。
//!
//! 本入口只读取信号时及紧邻下一根已完成 K 线，不构造订单、退出、MFE、MAE 或 R。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_research_window_symbols_from_quant_core,
    strict_visual_next_bar_hold_required_close, strict_visual_short_signal_gates_pass, Candle,
    Direction, ParityRuleVersion, ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant,
    StrictVisualBreakoutSignal, StrictVisualLongEntryEvent, StrictVisualLongEntryState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_INITIAL_CONFIRMATION_MS: i64 = 1_785_460_500_000;

/// 显式 L1 数据身份；不接受阈值开关，避免运行后调参。
struct Args {
    symbols: Vec<String>,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    output: PathBuf,
}

/// 因覆盖不足未进入扫描的成员及其可审计原因。
#[derive(Serialize)]
struct ExcludedSymbol {
    symbol: String,
    warmup_expected: usize,
    warmup_loaded: usize,
    evaluation_expected: usize,
    evaluation_loaded: usize,
    reason: &'static str,
}

/// 一个已完成额外守稳判定的来源；不含任何成交后字段。
#[derive(Clone, Serialize)]
struct HoldAudit {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    initial_confirmation_time_ms: i64,
    hold_time_ms: i64,
    original_signal_index: usize,
    hold_index: usize,
    range_upper: f64,
    range_lower: f64,
    baseline_required_acceptance_close: f64,
    hold_required_acceptance_close: f64,
    initial_confirmation_close: f64,
    hold_open: f64,
    hold_high: f64,
    hold_low: f64,
    hold_close: f64,
    passed: bool,
}

/// L1 机器报告只衡量来源覆盖、拒绝比例和目标样本，不读取结果标签。
#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_contract: &'static str,
    label_boundary: &'static str,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    raw_confirmed_sources: usize,
    resolved_sources: usize,
    passed_sources: usize,
    rejected_sources: usize,
    right_censored_sources: usize,
    rejected_source_percent: f64,
    passed_long_sources: usize,
    passed_short_sources: usize,
    passed_distinct_symbols: usize,
    passed_direction_hour_event_clusters: usize,
    target_source_found: bool,
    target_source_rejected: bool,
    l1_gate_passed: bool,
    audits: Vec<HoldAudit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating L1 warmup start")?;
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

    let variant = StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay;
    let mut raw_confirmed_sources = 0;
    let mut right_censored_sources = 0;
    let mut audits = Vec::new();
    for symbol in &included {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualLongEntryState::default();
        let mut pending: Option<StrictVisualBreakoutSignal> = None;
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= args.evaluation_end_exclusive_ms {
                break;
            }

            let point = &indicators.points[index];
            if point.atr14.is_none_or(|atr| atr <= 0.0) {
                continue;
            }

            if let Some(source) = pending.take() {
                audits.push(audit_hold(symbol, source, index, candle)?);
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
            let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) = event else {
                continue;
            };
            if candle.timestamp_ms < args.evaluation_start_ms {
                continue;
            }
            if signal.direction == Direction::Short
                && !strict_visual_short_signal_gates_pass(
                    &signal,
                    indicators.points[signal.breakout_index].ema144,
                    variant,
                )
            {
                continue;
            }
            raw_confirmed_sources += 1;
            pending = Some(signal);
        }
        if pending.is_some() {
            right_censored_sources += 1;
        }
    }

    let resolved_sources = audits.len();
    let passed_sources = audits.iter().filter(|audit| audit.passed).count();
    let rejected_sources = resolved_sources.saturating_sub(passed_sources);
    let rejected_source_percent = if resolved_sources == 0 {
        0.0
    } else {
        rejected_sources as f64 / resolved_sources as f64 * 100.0
    };
    let passed_long_sources = audits
        .iter()
        .filter(|audit| audit.passed && audit.direction == Direction::Long)
        .count();
    let passed_short_sources = audits
        .iter()
        .filter(|audit| audit.passed && audit.direction == Direction::Short)
        .count();
    let passed_distinct_symbols = audits
        .iter()
        .filter(|audit| audit.passed)
        .map(|audit| audit.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let passed_direction_hour_event_clusters = direction_hour_event_clusters(&audits);
    let target = audits.iter().find(|audit| {
        audit.symbol == TARGET_SYMBOL
            && audit.initial_confirmation_time_ms == TARGET_INITIAL_CONFIRMATION_MS
    });
    let target_source_found = target.is_some();
    let target_source_rejected = target.is_some_and(|audit| !audit.passed);
    let l1_gate_passed = target_source_rejected
        && rejected_sources >= 3
        && (10.0..=60.0).contains(&rejected_source_percent)
        && passed_sources >= 30
        && passed_long_sources >= 15
        && passed_short_sources >= 15
        && passed_distinct_symbols >= 8
        && passed_direction_hour_event_clusters >= 10
        && raw_confirmed_sources == resolved_sources + right_censored_sources;

    let report = L1Report {
        schema_version: "strict_visual_next_bar_hold_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "explicit_latest_confirmed_window",
        baseline_variant: variant.slug(),
        candidate_contract: "v28_acceptance_retention_75pct_next_completed_bar_hold",
        label_boundary: "No order, entry price, exit, MFE, MAE, final R, or win/loss field was read or serialized.",
        dataset_fingerprint_sha256,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        raw_confirmed_sources,
        resolved_sources,
        passed_sources,
        rejected_sources,
        right_censored_sources,
        rejected_source_percent,
        passed_long_sources,
        passed_short_sources,
        passed_distinct_symbols,
        passed_direction_hour_event_clusters,
        target_source_found,
        target_source_rejected,
        l1_gate_passed,
        audits,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "next_bar_hold_l1: included={} resolved={} passed={} rejected={} rejected_pct={:.2} target_rejected={} gate={} output={}",
        report.included_symbols,
        report.resolved_sources,
        report.passed_sources,
        report.rejected_sources,
        report.rejected_source_percent,
        report.target_source_rejected,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !l1_gate_passed {
        bail!("V28 L1 gate failed; stop before implementation or outcome replay");
    }
    Ok(())
}

/// 紧邻下一根完成棒必须继续守住首次确认时冻结的绝对接受价。
fn audit_hold(
    symbol: &ResearchWindowSymbolCandles,
    source: StrictVisualBreakoutSignal,
    index: usize,
    candle: Candle,
) -> Result<HoldAudit> {
    if index != source.signal_index + 1 {
        bail!(
            "{} hold bar is not adjacent to initial confirmation",
            symbol.symbol
        );
    }
    let initial_confirmation = symbol.candles[source.signal_index];
    let hold_required_acceptance_close = strict_visual_next_bar_hold_required_close(source);
    let passed = match source.direction {
        Direction::Long => candle.close >= hold_required_acceptance_close,
        Direction::Short => candle.close <= hold_required_acceptance_close,
    };
    Ok(HoldAudit {
        symbol: symbol.symbol.clone(),
        direction: source.direction,
        breakout_time_ms: symbol.candles[source.breakout_index].timestamp_ms,
        initial_confirmation_time_ms: initial_confirmation.timestamp_ms,
        hold_time_ms: candle.timestamp_ms,
        original_signal_index: source.signal_index,
        hold_index: index,
        range_upper: source.range.upper,
        range_lower: source.range.lower,
        baseline_required_acceptance_close: source.required_acceptance_close,
        hold_required_acceptance_close,
        initial_confirmation_close: initial_confirmation.close,
        hold_open: candle.open,
        hold_high: candle.high,
        hold_low: candle.low,
        hold_close: candle.close,
        passed,
    })
}

/// 按预热和评价窗口完整性隔离可用成员，缺口不会被补值。
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

/// 多空分离后按相邻不超过一小时的信号链归并市场事件。
fn direction_hour_event_clusters(audits: &[HoldAudit]) -> usize {
    let mut passed = audits
        .iter()
        .filter(|audit| audit.passed)
        .map(|audit| (audit.hold_time_ms, audit.direction))
        .collect::<Vec<_>>();
    passed.sort_by_key(|(time, direction)| (*direction as u8, *time));
    let mut clusters = 0;
    let mut previous: Option<(i64, Direction)> = None;
    for (time, direction) in passed {
        if previous.is_none_or(|(last, last_direction)| {
            last_direction != direction || time - last > EVENT_CLUSTER_MS
        }) {
            clusters += 1;
        }
        previous = Some((time, direction));
    }
    clusters
}

/// 数据指纹绑定成员顺序、窗口和全部已确认 OHLCV。
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

/// 解析显式研究参数，不提供可在看结果后修改的策略参数。
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

/// 先写临时文件再原子发布，避免留下半份研究报告。
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
