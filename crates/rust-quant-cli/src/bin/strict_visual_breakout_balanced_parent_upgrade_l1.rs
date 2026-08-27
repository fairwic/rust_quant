//! V43 父区间升级纵向占用均衡的 outcome-blind L1 扫描。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    load_research_window_symbols_from_quant_core, strict_visual_body_wick_envelope,
    ResearchWindowSymbolCandles, StrictVisualBreakoutResearchVariant,
    StrictVisualConsolidationState, StrictVisualRangeEvent, StrictVisualRangeEvidence,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const MAXIMUM_DOMINANT_HALF_RATIO: f64 = 0.75;
const TARGET_SYMBOL: &str = "ETH-USDT-SWAP";
const TARGET_START_MS: i64 = 1_785_931_200_000;
const TARGET_END_EXCLUSIVE_MS: i64 = 1_786_107_600_000;

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
struct UpgradeAudit {
    symbol: String,
    upgrade_time_ms: i64,
    range_start_time_ms: i64,
    first_confirmation_time_ms: i64,
    boundary_confirmation_time_ms: i64,
    length_bars: usize,
    trade_core_upper: f64,
    trade_core_lower: f64,
    visual_parent_upper: f64,
    visual_parent_lower: f64,
    upper_half_bars: usize,
    lower_half_bars: usize,
    dominant_half_ratio: f64,
}

#[derive(Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    report_scope: &'static str,
    baseline_variant: &'static str,
    candidate_variant: &'static str,
    single_variable: &'static str,
    maximum_dominant_half_ratio: f64,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    requested_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    target_symbol: &'static str,
    target_start_ms: i64,
    target_end_exclusive_ms: i64,
    baseline_upgrades: usize,
    baseline_one_sided_upgrades: usize,
    candidate_upgrades: usize,
    candidate_balanced_upgrades: usize,
    candidate_one_sided_upgrade_violations: usize,
    rejected_one_sided_upgrades: usize,
    rejected_one_sided_distinct_symbols: usize,
    target_baseline_one_sided_upgrades: usize,
    target_rejected_one_sided_upgrades: usize,
    causal_violations: usize,
    l1_gate_passed: bool,
    baseline_upgrade_ledger: Vec<UpgradeAudit>,
    candidate_upgrade_ledger: Vec<UpgradeAudit>,
    rejected_one_sided_ledger: Vec<UpgradeAudit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating V43 warmup start")?;
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

    let (baseline_upgrade_ledger, baseline_causal_violations) = scan_variant(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption,
    );
    let (candidate_upgrade_ledger, candidate_causal_violations) = scan_variant(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade,
    );
    let candidate_identities = candidate_upgrade_ledger
        .iter()
        .map(upgrade_identity)
        .collect::<BTreeSet<_>>();
    let baseline_one_sided = baseline_upgrade_ledger
        .iter()
        .filter(|upgrade| upgrade.dominant_half_ratio > MAXIMUM_DOMINANT_HALF_RATIO)
        .cloned()
        .collect::<Vec<_>>();
    let rejected_one_sided_ledger = baseline_one_sided
        .iter()
        .filter(|upgrade| !candidate_identities.contains(&upgrade_identity(upgrade)))
        .cloned()
        .collect::<Vec<_>>();
    let rejected_one_sided_distinct_symbols = rejected_one_sided_ledger
        .iter()
        .map(|upgrade| upgrade.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let target_baseline_one_sided_upgrades = baseline_one_sided
        .iter()
        .filter(|upgrade| is_target(upgrade))
        .count();
    let target_rejected_one_sided_upgrades = rejected_one_sided_ledger
        .iter()
        .filter(|upgrade| is_target(upgrade))
        .count();
    let candidate_one_sided_upgrade_violations = candidate_upgrade_ledger
        .iter()
        .filter(|upgrade| upgrade.dominant_half_ratio > MAXIMUM_DOMINANT_HALF_RATIO)
        .count();
    let candidate_balanced_upgrades =
        candidate_upgrade_ledger.len() - candidate_one_sided_upgrade_violations;
    let causal_violations = baseline_causal_violations + candidate_causal_violations;
    let l1_gate_passed = target_baseline_one_sided_upgrades > 0
        && target_rejected_one_sided_upgrades > 0
        && rejected_one_sided_ledger.len() >= 5
        && rejected_one_sided_distinct_symbols >= 3
        && candidate_balanced_upgrades > 0
        && candidate_one_sided_upgrade_violations == 0
        && causal_violations == 0;
    let report = L1Report {
        schema_version: "strict_visual_v43_balanced_parent_upgrade_l1_v1",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        report_scope: "completed_candles_and_range_events_only_no_trade_outcomes",
        baseline_variant: StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption
            .slug(),
        candidate_variant: StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade.slug(),
        single_variable:
            "leftward_parent_upgrade_requires_visual_parent_dominant_half_ratio_lte_0_75",
        maximum_dominant_half_ratio: MAXIMUM_DOMINANT_HALF_RATIO,
        dataset_fingerprint_sha256: fingerprint,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        target_symbol: TARGET_SYMBOL,
        target_start_ms: TARGET_START_MS,
        target_end_exclusive_ms: TARGET_END_EXCLUSIVE_MS,
        baseline_upgrades: baseline_upgrade_ledger.len(),
        baseline_one_sided_upgrades: baseline_one_sided.len(),
        candidate_upgrades: candidate_upgrade_ledger.len(),
        candidate_balanced_upgrades,
        candidate_one_sided_upgrade_violations,
        rejected_one_sided_upgrades: rejected_one_sided_ledger.len(),
        rejected_one_sided_distinct_symbols,
        target_baseline_one_sided_upgrades,
        target_rejected_one_sided_upgrades,
        causal_violations,
        l1_gate_passed,
        baseline_upgrade_ledger,
        candidate_upgrade_ledger,
        rejected_one_sided_ledger,
    };
    write_json_atomically(&args.output, &report)?;
    println!(
        "balanced_parent_upgrade_l1: included={} upgrades={}->{} one_sided={} rejected={} symbols={} target={}/{} balanced_kept={} violations={} gate={}",
        report.included_symbols,
        report.baseline_upgrades,
        report.candidate_upgrades,
        report.baseline_one_sided_upgrades,
        report.rejected_one_sided_upgrades,
        report.rejected_one_sided_distinct_symbols,
        report.target_rejected_one_sided_upgrades,
        report.target_baseline_one_sided_upgrades,
        report.candidate_balanced_upgrades,
        report.candidate_one_sided_upgrade_violations + report.causal_violations,
        report.l1_gate_passed,
    );
    if !report.l1_gate_passed {
        bail!("V43 L1 gate failed; stop before outcome replay");
    }
    Ok(())
}

fn scan_variant(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    variant: StrictVisualBreakoutResearchVariant,
) -> (Vec<UpgradeAudit>, usize) {
    let mut upgrades = Vec::new();
    let mut causal_violations = 0;
    for symbol in symbols {
        let mut state = StrictVisualConsolidationState::default();
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms >= evaluation_end_exclusive_ms {
                break;
            }
            let event = state.update_for_variant(&symbol.candles, index, symbol.tick_size, variant);
            let Some(StrictVisualRangeEvent::ParentUpgraded(range)) = event else {
                continue;
            };
            if range.first_confirmation_index > index
                || range.start_index > range.first_confirmation_index
                || range.boundary_confirmation_index != index
            {
                causal_violations += 1;
            }
            if candle.timestamp_ms >= evaluation_start_ms {
                upgrades.push(upgrade_audit(symbol, candle.timestamp_ms, range));
            }
        }
    }
    (upgrades, causal_violations)
}

fn upgrade_audit(
    symbol: &ResearchWindowSymbolCandles,
    upgrade_time_ms: i64,
    range: StrictVisualRangeEvidence,
) -> UpgradeAudit {
    let history = &symbol.candles[range.start_index..=range.boundary_confirmation_index];
    let mut parent = strict_visual_body_wick_envelope(history).expect("valid confirmed range");
    parent.upper = parent.upper.max(range.upper);
    parent.lower = parent.lower.min(range.lower);
    let middle = (parent.upper + parent.lower) * 0.5;
    let upper_half_bars = history
        .iter()
        .filter(|candle| (candle.open + candle.close) * 0.5 >= middle)
        .count();
    let lower_half_bars = history.len() - upper_half_bars;
    UpgradeAudit {
        symbol: symbol.symbol.clone(),
        upgrade_time_ms,
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        length_bars: range.length_bars,
        trade_core_upper: range.upper,
        trade_core_lower: range.lower,
        visual_parent_upper: parent.upper,
        visual_parent_lower: parent.lower,
        upper_half_bars,
        lower_half_bars,
        dominant_half_ratio: upper_half_bars.max(lower_half_bars) as f64 / history.len() as f64,
    }
}

type UpgradeIdentity = (String, i64, i64, i64, usize);

fn upgrade_identity(upgrade: &UpgradeAudit) -> UpgradeIdentity {
    (
        upgrade.symbol.clone(),
        upgrade.upgrade_time_ms,
        upgrade.range_start_time_ms,
        upgrade.boundary_confirmation_time_ms,
        upgrade.length_bars,
    )
}

fn is_target(upgrade: &UpgradeAudit) -> bool {
    upgrade.symbol == TARGET_SYMBOL
        && upgrade.upgrade_time_ms >= TARGET_START_MS
        && upgrade.upgrade_time_ms < TARGET_END_EXCLUSIVE_MS
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
