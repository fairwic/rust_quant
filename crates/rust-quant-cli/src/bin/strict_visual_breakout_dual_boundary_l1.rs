//! V39 视觉父区间与 V31 交易核心的 outcome-blind L1 身份审计。
//!
//! 只验证同一完成窗口的双层边界，不构造订单或读取任何成交后结果。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    evaluate_strict_visual_dual_boundary_candidate_for_research,
    evaluate_strict_visual_percentile_candidate_for_research,
    load_research_window_symbols_from_quant_core, Candle, ResearchWindowSymbolCandles,
    StrictVisualBreakoutResearchVariant, StrictVisualDualBoundaryEvidence,
    StrictVisualRangeEvidence, STRICT_VISUAL_CANDIDATE_LENGTHS,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const EVALUATION_START_MS: i64 = 1_785_168_000_000;
const EVALUATION_END_EXCLUSIVE_MS: i64 = 1_786_248_900_000;
const MINIMUM_CHANGED_RATIO: f64 = 0.10;
const MINIMUM_CHANGED_SYMBOLS: usize = 8;
const MINIMUM_CHANGED_EVENT_CLUSTERS: usize = 10;

const TARGET_SYMBOL: &str = "BTC-USDT-SWAP";
const TARGET_END_TIME_MS: i64 = 1_785_148_200_000;
const TARGET_LENGTH_BARS: usize = 48;
const TARGET_BODY_0915_TIME_MS: i64 = 1_785_114_900_000;
const TARGET_BODY_1415_TIME_MS: i64 = 1_785_132_900_000;
const TARGET_CORE_UPPER: f64 = 65_511.5;
const TARGET_CORE_LOWER: f64 = 65_050.0;
const TARGET_PARENT_UPPER: f64 = 65_573.1;
const TARGET_PARENT_LOWER: f64 = 64_991.6;

struct Args {
    symbols: Vec<String>,
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

#[derive(Serialize)]
struct TargetBodyAudit {
    time_ms: i64,
    open: f64,
    close: f64,
    included_by_visual_parent: bool,
}

#[derive(Serialize)]
struct TargetAudit {
    symbol_present: bool,
    end_time_present: bool,
    window_start_time_ms: Option<i64>,
    baseline: Option<StrictVisualRangeEvidence>,
    dual: Option<StrictVisualDualBoundaryEvidence>,
    body_0915: Option<TargetBodyAudit>,
    body_1415: Option<TargetBodyAudit>,
    passed: bool,
}

#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    single_variable: &'static str,
    label_boundary: &'static str,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    evaluated_windows: usize,
    baseline_valid_windows: usize,
    dual_valid_windows: usize,
    identity_presence_violations: usize,
    trade_core_field_violations: usize,
    visual_parent_changed_windows: usize,
    visual_parent_changed_ratio: f64,
    visual_parent_changed_symbols: usize,
    visual_parent_changed_hour_event_clusters: usize,
    body_inclusion_violations: usize,
    trade_core_containment_violations: usize,
    sweep_allowance_violations: usize,
    target: TargetAudit,
    l1_gate_passed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating V39 warmup start")?;
    let loaded = load_research_window_symbols_from_quant_core(
        &args.symbols,
        warmup_start_ms,
        EVALUATION_START_MS,
        EVALUATION_END_EXCLUSIVE_MS,
    )
    .await?;
    let dataset_fingerprint_sha256 = dataset_fingerprint(
        &loaded,
        warmup_start_ms,
        EVALUATION_START_MS,
        EVALUATION_END_EXCLUSIVE_MS,
    );
    let (included, excluded_symbols) = partition_complete_symbols(loaded);
    if included.is_empty() {
        bail!("no symbol has complete warmup and evaluation coverage");
    }

    let variant = StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard;
    let mut evaluated_windows = 0;
    let mut baseline_valid_windows = 0;
    let mut dual_valid_windows = 0;
    let mut identity_presence_violations = 0;
    let mut trade_core_field_violations = 0;
    let mut visual_parent_changed_windows = 0;
    let mut body_inclusion_violations = 0;
    let mut trade_core_containment_violations = 0;
    let mut sweep_allowance_violations = 0;
    let mut changed_symbols = BTreeSet::new();
    let mut changed_event_clusters = BTreeSet::new();

    for symbol in &included {
        for (index, candle) in symbol.candles.iter().enumerate() {
            if candle.timestamp_ms < EVALUATION_START_MS {
                continue;
            }
            if candle.timestamp_ms >= EVALUATION_END_EXCLUSIVE_MS {
                break;
            }
            for length in STRICT_VISUAL_CANDIDATE_LENGTHS {
                if length > index + 1 {
                    continue;
                }
                evaluated_windows += 1;
                let baseline = evaluate_strict_visual_percentile_candidate_for_research(
                    &symbol.candles,
                    index,
                    length,
                    symbol.tick_size,
                    variant,
                );
                let dual = evaluate_strict_visual_dual_boundary_candidate_for_research(
                    &symbol.candles,
                    index,
                    length,
                    symbol.tick_size,
                    variant,
                );
                baseline_valid_windows += usize::from(baseline.is_some());
                dual_valid_windows += usize::from(dual.is_some());
                if baseline.is_some() != dual.is_some() {
                    identity_presence_violations += 1;
                    continue;
                }
                let (Some(baseline), Some(dual)) = (baseline, dual) else {
                    continue;
                };
                trade_core_field_violations += usize::from(baseline != dual.trade_core);
                let history = history(&symbol.candles, index, length);
                audit_parent_contract(
                    history,
                    dual,
                    &mut body_inclusion_violations,
                    &mut trade_core_containment_violations,
                    &mut sweep_allowance_violations,
                );
                if prices_differ(
                    dual.visual_parent.upper,
                    dual.trade_core.upper,
                    symbol.tick_size,
                ) || prices_differ(
                    dual.visual_parent.lower,
                    dual.trade_core.lower,
                    symbol.tick_size,
                ) {
                    visual_parent_changed_windows += 1;
                    changed_symbols.insert(symbol.symbol.as_str());
                    changed_event_clusters.insert(candle.timestamp_ms / EVENT_CLUSTER_MS);
                }
            }
        }
    }

    let visual_parent_changed_ratio = ratio(visual_parent_changed_windows, baseline_valid_windows);
    let target = audit_target(&included, variant);
    let l1_gate_passed = target.passed
        && baseline_valid_windows == dual_valid_windows
        && identity_presence_violations == 0
        && trade_core_field_violations == 0
        && visual_parent_changed_ratio >= MINIMUM_CHANGED_RATIO
        && changed_symbols.len() >= MINIMUM_CHANGED_SYMBOLS
        && changed_event_clusters.len() >= MINIMUM_CHANGED_EVENT_CLUSTERS
        && body_inclusion_violations == 0
        && trade_core_containment_violations == 0
        && sweep_allowance_violations == 0;
    let report = L1Report {
        schema_version: "strict_visual_v39_dual_boundary_l1_v1",
        research_level: "L1_OUTCOME_BLIND_PARITY",
        report_scope: "fixed_completed_candle_windows_only",
        baseline_variant: variant.slug(),
        single_variable: "visual_parent_attached_to_unchanged_trade_core",
        label_boundary:
            "No order, entry, exit, stop, target, MFE, MAE, final R, PnL, or win/loss field was read or serialized.",
        dataset_fingerprint_sha256,
        evaluation_start_ms: EVALUATION_START_MS,
        evaluation_end_exclusive_ms: EVALUATION_END_EXCLUSIVE_MS,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        evaluated_windows,
        baseline_valid_windows,
        dual_valid_windows,
        identity_presence_violations,
        trade_core_field_violations,
        visual_parent_changed_windows,
        visual_parent_changed_ratio,
        visual_parent_changed_symbols: changed_symbols.len(),
        visual_parent_changed_hour_event_clusters: changed_event_clusters.len(),
        body_inclusion_violations,
        trade_core_containment_violations,
        sweep_allowance_violations,
        target,
        l1_gate_passed,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "dual_boundary_l1: included={} baseline={} dual={} changed={:.2}% identity_violations={} target={} gate={} output={}",
        report.included_symbols,
        report.baseline_valid_windows,
        report.dual_valid_windows,
        report.visual_parent_changed_ratio * 100.0,
        report.identity_presence_violations + report.trade_core_field_violations,
        report.target.passed,
        report.l1_gate_passed,
        args.output.display(),
    );
    if !report.l1_gate_passed {
        bail!("V39 dual-boundary L1 gate failed; stop before TradingView parity work");
    }
    Ok(())
}

fn history(candles: &[Candle], index: usize, length: usize) -> &[Candle] {
    &candles[index + 1 - length..=index]
}

fn audit_parent_contract(
    history: &[Candle],
    dual: StrictVisualDualBoundaryEvidence,
    body_inclusion_violations: &mut usize,
    trade_core_containment_violations: &mut usize,
    sweep_allowance_violations: &mut usize,
) {
    if history.iter().any(|candle| {
        candle.open.max(candle.close) > dual.visual_parent.upper
            || candle.open.min(candle.close) < dual.visual_parent.lower
    }) {
        *body_inclusion_violations += 1;
    }
    if dual.visual_parent.upper < dual.trade_core.upper
        || dual.visual_parent.lower > dual.trade_core.lower
    {
        *trade_core_containment_violations += 1;
    }
    if dual.visual_parent.upper_wicks_outside > dual.visual_parent.allowed_sweep_wicks_per_side
        || dual.visual_parent.lower_wicks_outside > dual.visual_parent.allowed_sweep_wicks_per_side
    {
        *sweep_allowance_violations += 1;
    }
}

fn audit_target(
    symbols: &[ResearchWindowSymbolCandles],
    variant: StrictVisualBreakoutResearchVariant,
) -> TargetAudit {
    let Some(symbol) = symbols.iter().find(|symbol| symbol.symbol == TARGET_SYMBOL) else {
        return missing_target(false);
    };
    let Some(end_index) = symbol
        .candles
        .iter()
        .position(|candle| candle.timestamp_ms == TARGET_END_TIME_MS)
    else {
        return missing_target(true);
    };
    let baseline = evaluate_strict_visual_percentile_candidate_for_research(
        &symbol.candles,
        end_index,
        TARGET_LENGTH_BARS,
        symbol.tick_size,
        variant,
    );
    let dual = evaluate_strict_visual_dual_boundary_candidate_for_research(
        &symbol.candles,
        end_index,
        TARGET_LENGTH_BARS,
        symbol.tick_size,
        variant,
    );
    let body_0915 = target_body(symbol, TARGET_BODY_0915_TIME_MS, dual);
    let body_1415 = target_body(symbol, TARGET_BODY_1415_TIME_MS, dual);
    let passed = baseline.is_some_and(|core| {
        prices_equal(core.upper, TARGET_CORE_UPPER, symbol.tick_size)
            && prices_equal(core.lower, TARGET_CORE_LOWER, symbol.tick_size)
    }) && dual.is_some_and(|value| {
        value.trade_core == baseline.expect("baseline checked above")
            && prices_equal(
                value.visual_parent.upper,
                TARGET_PARENT_UPPER,
                symbol.tick_size,
            )
            && prices_equal(
                value.visual_parent.lower,
                TARGET_PARENT_LOWER,
                symbol.tick_size,
            )
            && value.visual_parent.upper_wicks_outside
                <= value.visual_parent.allowed_sweep_wicks_per_side
            && value.visual_parent.lower_wicks_outside
                <= value.visual_parent.allowed_sweep_wicks_per_side
    }) && body_0915
        .as_ref()
        .is_some_and(|body| body.included_by_visual_parent)
        && body_1415
            .as_ref()
            .is_some_and(|body| body.included_by_visual_parent);
    TargetAudit {
        symbol_present: true,
        end_time_present: true,
        window_start_time_ms: end_index
            .checked_add(1)
            .and_then(|end| end.checked_sub(TARGET_LENGTH_BARS))
            .map(|start| symbol.candles[start].timestamp_ms),
        baseline,
        dual,
        body_0915,
        body_1415,
        passed,
    }
}

fn target_body(
    symbol: &ResearchWindowSymbolCandles,
    time_ms: i64,
    dual: Option<StrictVisualDualBoundaryEvidence>,
) -> Option<TargetBodyAudit> {
    let candle = symbol
        .candles
        .iter()
        .find(|candle| candle.timestamp_ms == time_ms)?;
    Some(TargetBodyAudit {
        time_ms,
        open: candle.open,
        close: candle.close,
        included_by_visual_parent: dual.is_some_and(|value| {
            candle.open.max(candle.close) <= value.visual_parent.upper
                && candle.open.min(candle.close) >= value.visual_parent.lower
        }),
    })
}

fn missing_target(symbol_present: bool) -> TargetAudit {
    TargetAudit {
        symbol_present,
        end_time_present: false,
        window_start_time_ms: None,
        baseline: None,
        dual: None,
        body_0915: None,
        body_1415: None,
        passed: false,
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
    let mut output = None;
    let mut values = values.into_iter();
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--symbols" => {
                symbols = Some(parse_symbols(&values.next().context("missing symbols")?)?)
            }
            "--output" => output = Some(PathBuf::from(values.next().context("missing output")?)),
            other => bail!("unsupported argument: {other}"),
        }
    }
    Ok(Args {
        symbols: symbols.context("missing --symbols")?,
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

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn prices_differ(left: f64, right: f64, tick_size: f64) -> bool {
    (left - right).abs() >= tick_size * 0.5
}

fn prices_equal(left: f64, right: f64, tick_size: f64) -> bool {
    !prices_differ(left, right, tick_size)
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
