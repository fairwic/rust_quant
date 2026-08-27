//! 对 V32～V36 五个独立单变量执行 outcome-blind L1 因果覆盖审计。
//!
//! 本入口不输出最终盈亏、MFE、MAE 或组合资金曲线；V34 仅检查冻结仓位路径上是否在退出前出现
//! 预注册的盈利放量完成棒，不能用该字段反向调整成交量阈值。

use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, cost_covered_break_even_price, is_opposing_sweep_reclaimed,
    load_research_window_symbols_from_quant_core, prior_three_bars_have_supporting_wick,
    remaining_net_reward_r, Candle, Direction, ParityRuleVersion, ResearchWindowSymbolCandles,
    StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal, StrictVisualLongEntryEvent,
    StrictVisualPriorRangeClearanceState, MIN_REMAINING_NET_REWARD_R,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const BTC_SYMBOL: &str = "BTC-USDT-SWAP";
const ETH_SYMBOL: &str = "ETH-USDT-SWAP";
const V32_BTC_BREAKOUT_MS: i64 = 1_785_399_300_000;
const V33_BTC_SIGNAL_MS: i64 = 1_785_355_200_000;
const V34_BTC_ENTRY_MS: i64 = 1_784_525_400_000;
const V34_BTC_ACTIVATION_MS: i64 = 1_784_529_000_000;
const V35_BTC_SIGNAL_MS: i64 = 1_785_400_200_000;
const V35_ETH_SIGNAL_MS: i64 = 1_785_794_400_000;
const V36_ETH_SIGNAL_ONE_MS: i64 = 1_784_852_100_000;
const V36_ETH_SIGNAL_TWO_MS: i64 = 1_785_295_800_000;

#[derive(Clone, Copy, PartialEq, Eq)]
/// 一次运行只审计一个冻结变量，禁止在同一结果中混合五项改动。
enum Study {
    Baseline,
    V32,
    V33,
    V34,
    V35,
    V36,
}

impl Study {
    /// 把稳定 CLI 名称解析为单变量研究身份。
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "v32" => Ok(Self::V32),
            "v33" => Ok(Self::V33),
            "v34" => Ok(Self::V34),
            "v35" => Ok(Self::V35),
            "v36" => Ok(Self::V36),
            other => bail!("unsupported L1 study: {other}"),
        }
    }

    /// 返回该研究实际推进状态机时使用的 Research-only 版本。
    const fn variant(self) -> StrictVisualBreakoutResearchVariant {
        match self {
            Self::Baseline => StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
            Self::V32 => StrictVisualBreakoutResearchVariant::V32PriorThreeWickDirectChase,
            Self::V33 => StrictVisualBreakoutResearchVariant::V33OpposingConfirmationWickGuard,
            Self::V34 => StrictVisualBreakoutResearchVariant::V34VolumeProfitProtection,
            Self::V35 => StrictVisualBreakoutResearchVariant::V35RemainingNetReward50,
            Self::V36 => StrictVisualBreakoutResearchVariant::V36PreRangeExtremeTradeBoundary96,
        }
    }
}

/// L1 运行所需的显式币种、半开窗口、单变量与输出位置。
struct Args {
    symbols: Vec<String>,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    study: Study,
    output: PathBuf,
}

#[derive(Serialize)]
/// 因预热或评价窗口不完整而未进入研究的币种证据。
struct ExcludedSymbol {
    symbol: String,
    warmup_expected: usize,
    warmup_loaded: usize,
    evaluation_expected: usize,
    evaluation_loaded: usize,
    reason: &'static str,
}

#[derive(Clone, Serialize)]
/// 一个不含后续盈亏字段的确认来源账本行。
struct SourceAudit {
    symbol: String,
    direction: Direction,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    range_start_time_ms: i64,
    range_length_bars: usize,
    visual_upper: f64,
    visual_lower: f64,
}

#[derive(Clone, Serialize)]
/// 单变量确实改变某个来源时保存的信号时证据。
struct AffectedAudit {
    symbol: String,
    direction: Direction,
    reason: &'static str,
    breakout_time_ms: i64,
    signal_time_ms: i64,
    entry_time_ms: Option<i64>,
    affected_time_ms: i64,
    primary_value: Option<f64>,
    secondary_value: Option<f64>,
    visual_boundary: Option<f64>,
    trade_boundary: Option<f64>,
}

#[derive(Default)]
/// 单币扫描的强来源、候选与受影响来源汇总。
struct Scan {
    strong_sources: usize,
    candidates: Vec<SourceAudit>,
    affected: Vec<AffectedAudit>,
}

#[derive(Serialize)]
/// L1 outcome-blind 报告；禁止出现最终 R、MFE、MAE 或退出结果。
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
    baseline_strong_sources: usize,
    baseline_confirmed_sources: usize,
    candidate_strong_sources: usize,
    candidate_sources: usize,
    candidate_long_sources: usize,
    candidate_short_sources: usize,
    candidate_distinct_symbols: usize,
    affected_sources: usize,
    affected_distinct_symbols: usize,
    affected_ratio: f64,
    target_checks: Vec<TargetCheck>,
    l1_gate_passed: bool,
    affected: Vec<AffectedAudit>,
    candidate_source_ledger: Vec<SourceAudit>,
}

#[derive(Serialize)]
/// 用户指定图表样本是否被预注册规则命中的断言。
struct TargetCheck {
    label: &'static str,
    passed: bool,
}

#[tokio::main]
/// 加载本地只读行情并原子输出一项单变量 L1 审计。
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

    let baseline = scan_universe(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        Study::Baseline,
    );
    let candidate = scan_universe(
        &included,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
        args.study,
    );
    let denominator = match args.study {
        Study::V32 | Study::V36 => baseline.strong_sources,
        Study::Baseline | Study::V33 | Study::V34 | Study::V35 => baseline.candidates.len(),
    };
    let affected_ratio = if denominator == 0 {
        0.0
    } else {
        candidate.affected.len() as f64 / denominator as f64
    };
    let candidate_long_sources = candidate
        .candidates
        .iter()
        .filter(|source| source.direction == Direction::Long)
        .count();
    let candidate_short_sources = candidate.candidates.len() - candidate_long_sources;
    let candidate_distinct_symbols = distinct_symbols_from_sources(&candidate.candidates);
    let affected_distinct_symbols = distinct_symbols_from_affected(&candidate.affected);
    let target_checks = target_checks(args.study, &baseline.candidates, &candidate.affected);
    let all_targets_pass = target_checks.iter().all(|check| check.passed);
    let l1_gate_passed = match args.study {
        Study::V32 => {
            all_targets_pass
                && candidate.affected.len() >= 3
                && affected_distinct_symbols >= 2
                && candidate
                    .affected
                    .iter()
                    .any(|item| item.direction == Direction::Long)
                && candidate
                    .affected
                    .iter()
                    .any(|item| item.direction == Direction::Short)
                && affected_ratio <= 0.60
        }
        Study::V33 => {
            all_targets_pass
                && candidate.affected.len() >= 3
                && affected_ratio <= 0.40
                && candidate_long_sources > 0
                && candidate_short_sources > 0
                && candidate_distinct_symbols >= 8
        }
        Study::V34 => {
            all_targets_pass && candidate.affected.len() >= 3 && affected_distinct_symbols >= 2
        }
        Study::V35 => {
            all_targets_pass
                && candidate.affected.len() >= 3
                && affected_ratio <= 0.60
                && candidate_long_sources > 0
                && candidate_short_sources > 0
                && candidate_distinct_symbols >= 8
        }
        Study::V36 => {
            all_targets_pass
                && candidate.affected.len() >= 3
                && affected_ratio <= 0.60
                && candidate_long_sources > 0
                && candidate_short_sources > 0
                && candidate_distinct_symbols >= 8
        }
        Study::Baseline => false,
    };
    let report = L1Report {
        schema_version: "strict_visual_breakout_v32_v36_l1_v1",
        research_level: "L1_OUTCOME_BLIND",
        report_scope: "signal_time_and_preregistered_activation_only_no_outcome_fields",
        baseline_variant: StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard.slug(),
        candidate_variant: args.study.variant().slug(),
        single_variable: single_variable(args.study),
        dataset_fingerprint_sha256: fingerprint,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        requested_symbols: args.symbols.len(),
        included_symbols: included.len(),
        excluded_symbols,
        baseline_strong_sources: baseline.strong_sources,
        baseline_confirmed_sources: baseline.candidates.len(),
        candidate_strong_sources: candidate.strong_sources,
        candidate_sources: candidate.candidates.len(),
        candidate_long_sources,
        candidate_short_sources,
        candidate_distinct_symbols,
        affected_sources: candidate.affected.len(),
        affected_distinct_symbols,
        affected_ratio,
        target_checks,
        l1_gate_passed,
        affected: candidate.affected,
        candidate_source_ledger: candidate.candidates,
    };
    write_json_atomically(&args.output, &report)
}

/// 按固定成员顺序汇总单币扫描，保持数据指纹与结果确定性。
fn scan_universe(
    symbols: &[ResearchWindowSymbolCandles],
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    study: Study,
) -> Scan {
    let mut aggregate = Scan::default();
    for symbol in symbols {
        let scan = scan_symbol(
            symbol,
            evaluation_start_ms,
            evaluation_end_exclusive_ms,
            study,
        );
        aggregate.strong_sources += scan.strong_sources;
        aggregate.candidates.extend(scan.candidates);
        aggregate.affected.extend(scan.affected);
    }
    aggregate
}

/// 只使用当前及此前完成棒推进一个币种的严格横盘状态机。
fn scan_symbol(
    symbol: &ResearchWindowSymbolCandles,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
    study: Study,
) -> Scan {
    let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
    let volume_events = indicators
        .points
        .iter()
        .map(|point| point.volume_event)
        .collect::<Vec<_>>();
    let mut state = StrictVisualPriorRangeClearanceState::default();
    let mut scan = Scan::default();
    for (index, candle) in symbol.candles.iter().copied().enumerate() {
        if candle.timestamp_ms >= evaluation_end_exclusive_ms {
            break;
        }
        let point = &indicators.points[index];
        let update = state.update(
            &symbol.candles,
            index,
            symbol.tick_size,
            point.atr14,
            point.volume_event,
            point.filtered_volume_ratio,
            None,
            study.variant(),
        );
        if candle.timestamp_ms < evaluation_start_ms {
            continue;
        }
        if matches!(
            update.event,
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
                | Some(StrictVisualLongEntryEvent::DirectSignal(_))
        ) {
            scan.strong_sources += 1;
        }
        if let Some((source, evidence)) = update.trade_boundary_rejected {
            scan.affected.push(affected(
                symbol,
                source,
                "trade_boundary_not_cleared",
                candle.timestamp_ms,
                None,
                Some(evidence.trade_breakout_boundary - evidence.visual_boundary),
                Some(evidence.visual_boundary),
                Some(evidence.trade_breakout_boundary),
            ));
            continue;
        }
        let Some(source) = update.confirmed else {
            continue;
        };
        if study == Study::V32
            && matches!(
                update.event,
                Some(StrictVisualLongEntryEvent::DirectSignal(_))
            )
        {
            let maximum_wick_ratio = prior_supporting_wick_ratio(
                &symbol.candles,
                source.breakout_index,
                source.direction,
                source.source_atr,
            );
            debug_assert!(prior_three_bars_have_supporting_wick(
                &symbol.candles,
                source.breakout_index,
                source.direction,
                source.source_atr,
            ));
            scan.affected.push(affected(
                symbol,
                source,
                "prior_three_wick_direct_chase",
                candle.timestamp_ms,
                Some(maximum_wick_ratio),
                None,
                None,
                None,
            ));
        }
        if study == Study::V33
            && is_opposing_sweep_reclaimed(candle, source.direction, source.source_atr)
        {
            let (wick_ratio, close_location) = opposing_sweep_metrics(candle, source);
            scan.affected.push(affected(
                symbol,
                source,
                "opposing_confirmation_sweep_reclaimed",
                candle.timestamp_ms,
                Some(wick_ratio),
                Some(close_location),
                None,
                None,
            ));
            continue;
        }
        if study == Study::V35 {
            let Some((entry_index, entry, stop, target)) = entry_plan(symbol, source) else {
                continue;
            };
            let reward = remaining_net_reward_r(source.direction, entry, target, stop, 3.0, 6.0)
                .unwrap_or(f64::NEG_INFINITY);
            if reward < MIN_REMAINING_NET_REWARD_R {
                scan.affected.push(affected_with_entry(
                    symbol,
                    source,
                    "remaining_net_reward_below_050r",
                    symbol.candles[entry_index].timestamp_ms,
                    symbol.candles[entry_index].timestamp_ms,
                    Some(reward),
                ));
                continue;
            }
        }
        if study == Study::V34 {
            if let Some((entry_time_ms, activation_time_ms)) =
                volume_activation(symbol, &volume_events, source)
            {
                scan.affected.push(affected_with_entry(
                    symbol,
                    source,
                    "profitable_volume_event_activates_net_break_even",
                    entry_time_ms,
                    activation_time_ms,
                    None,
                ));
            }
        }
        scan.candidates.push(source_audit(symbol, source));
    }
    scan
}

/// 用实际下一开盘与冻结结构构造和执行回放相同的初始止损、目标。
fn entry_plan(
    symbol: &ResearchWindowSymbolCandles,
    source: StrictVisualBreakoutSignal,
) -> Option<(usize, f64, f64, f64)> {
    let entry_index = source.signal_index.checked_add(1)?;
    let entry = symbol.candles.get(entry_index)?.open;
    let structural_stop = source.breakout_candle_extreme_stop_price?;
    let minimum_risk = (source.source_atr / symbol.tick_size).ceil().max(1.0) * symbol.tick_size;
    let stop = match source.direction {
        Direction::Long => structural_stop.min(entry - minimum_risk),
        Direction::Short => structural_stop.max(entry + minimum_risk),
    };
    let raw_target = source.measured_move_target_price?;
    let target = match source.direction {
        Direction::Long => round_down(raw_target, symbol.tick_size),
        Direction::Short => round_up(raw_target, symbol.tick_size),
    };
    let valid = match source.direction {
        Direction::Long => stop < entry,
        Direction::Short => stop > entry,
    };
    valid.then_some((entry_index, entry, stop, target))
}

/// 在冻结止损或目标先触发前寻找首根盈利放量完成棒。
fn volume_activation(
    symbol: &ResearchWindowSymbolCandles,
    volume_events: &[bool],
    source: StrictVisualBreakoutSignal,
) -> Option<(i64, i64)> {
    let (entry_index, entry, stop, target) = entry_plan(symbol, source)?;
    let break_even =
        cost_covered_break_even_price(source.direction, entry, 3.0, 6.0, symbol.tick_size)?;
    for index in entry_index..symbol.candles.len() {
        let candle = symbol.candles[index];
        if barrier_touched(candle, source.direction, stop, target) {
            return None;
        }
        if index > entry_index && volume_events[index] {
            let profitable = match source.direction {
                Direction::Long => candle.close >= break_even,
                Direction::Short => candle.close <= break_even,
            };
            if profitable {
                return Some((
                    symbol.candles[entry_index].timestamp_ms,
                    candle.timestamp_ms,
                ));
            }
        }
    }
    None
}

/// 判断当前棒是否已经触及冻结止损或目标，避免事后补造保护机会。
fn barrier_touched(candle: Candle, direction: Direction, stop: f64, target: f64) -> bool {
    match direction {
        Direction::Long => candle.low <= stop || candle.high >= target,
        Direction::Short => candle.high >= stop || candle.low <= target,
    }
}

/// 返回突破棒前三根方向承接影线相对来源 ATR 的最大比例。
fn prior_supporting_wick_ratio(
    candles: &[Candle],
    breakout_index: usize,
    direction: Direction,
    atr: f64,
) -> f64 {
    candles[breakout_index - 3..breakout_index]
        .iter()
        .map(|candle| match direction {
            Direction::Long => candle.open.min(candle.close) - candle.low,
            Direction::Short => candle.high - candle.open.max(candle.close),
        })
        .map(|wick| wick.max(0.0) / atr)
        .fold(0.0, f64::max)
}

/// 返回反方向影线 ATR 比与收盘位置，用于 L1 可解释审计。
fn opposing_sweep_metrics(candle: Candle, source: StrictVisualBreakoutSignal) -> (f64, f64) {
    match source.direction {
        Direction::Short => (
            (candle.open.min(candle.close) - candle.low) / source.source_atr,
            (candle.close - candle.low) / candle.range(),
        ),
        Direction::Long => (
            (candle.high - candle.open.max(candle.close)) / source.source_atr,
            (candle.high - candle.close) / candle.range(),
        ),
    }
}

/// 构造不含入场时点的受影响来源审计行。
fn affected(
    symbol: &ResearchWindowSymbolCandles,
    source: StrictVisualBreakoutSignal,
    reason: &'static str,
    affected_time_ms: i64,
    primary_value: Option<f64>,
    secondary_value: Option<f64>,
    visual_boundary: Option<f64>,
    trade_boundary: Option<f64>,
) -> AffectedAudit {
    AffectedAudit {
        symbol: symbol.symbol.clone(),
        direction: source.direction,
        reason,
        breakout_time_ms: symbol.candles[source.breakout_index].timestamp_ms,
        signal_time_ms: symbol.candles[source.signal_index].timestamp_ms,
        entry_time_ms: None,
        affected_time_ms,
        primary_value,
        secondary_value,
        visual_boundary,
        trade_boundary,
    }
}

/// 在基础受影响审计行上补充实际下一开盘与激活时间。
fn affected_with_entry(
    symbol: &ResearchWindowSymbolCandles,
    source: StrictVisualBreakoutSignal,
    reason: &'static str,
    entry_time_ms: i64,
    affected_time_ms: i64,
    primary_value: Option<f64>,
) -> AffectedAudit {
    let mut audit = affected(
        symbol,
        source,
        reason,
        affected_time_ms,
        primary_value,
        None,
        None,
        None,
    );
    audit.entry_time_ms = Some(entry_time_ms);
    audit
}

/// 把冻结突破来源转换为稳定、无结果标签的候选账本行。
fn source_audit(
    symbol: &ResearchWindowSymbolCandles,
    source: StrictVisualBreakoutSignal,
) -> SourceAudit {
    SourceAudit {
        symbol: symbol.symbol.clone(),
        direction: source.direction,
        breakout_time_ms: symbol.candles[source.breakout_index].timestamp_ms,
        signal_time_ms: symbol.candles[source.signal_index].timestamp_ms,
        range_start_time_ms: symbol.candles[source.range.start_index].timestamp_ms,
        range_length_bars: source.range.length_bars,
        visual_upper: source.range.upper,
        visual_lower: source.range.lower,
    }
}

/// 按预注册时点检查每个研究必须命中的用户目标样本。
fn target_checks(
    study: Study,
    baseline: &[SourceAudit],
    affected: &[AffectedAudit],
) -> Vec<TargetCheck> {
    match study {
        Study::V32 => vec![TargetCheck {
            label: "BTC 2026-07-30 16:15 breakout direct chase",
            passed: affected.iter().any(|item| {
                item.symbol == BTC_SYMBOL && item.breakout_time_ms == V32_BTC_BREAKOUT_MS
            }),
        }],
        Study::V33 => vec![TargetCheck {
            label: "BTC 2026-07-30 04:00 confirmation rejected",
            passed: affected
                .iter()
                .any(|item| item.symbol == BTC_SYMBOL && item.signal_time_ms == V33_BTC_SIGNAL_MS),
        }],
        Study::V34 => vec![TargetCheck {
            label: "BTC 2026-07-20 13:30 entry protected at 14:30",
            passed: affected.iter().any(|item| {
                item.symbol == BTC_SYMBOL
                    && item.entry_time_ms == Some(V34_BTC_ENTRY_MS)
                    && item.affected_time_ms == V34_BTC_ACTIVATION_MS
            }),
        }],
        Study::V35 => vec![
            TargetCheck {
                label: "BTC-L-1785400200000 rejected",
                passed: affected.iter().any(|item| {
                    item.symbol == BTC_SYMBOL && item.signal_time_ms == V35_BTC_SIGNAL_MS
                }),
            },
            TargetCheck {
                label: "ETH 2026-08-04 06:00 signal rejected",
                passed: affected.iter().any(|item| {
                    item.symbol == ETH_SYMBOL && item.signal_time_ms == V35_ETH_SIGNAL_MS
                }),
            },
        ],
        Study::V36 => {
            let target_breakouts = [V36_ETH_SIGNAL_ONE_MS, V36_ETH_SIGNAL_TWO_MS]
                .into_iter()
                .filter_map(|signal_time| {
                    baseline
                        .iter()
                        .find(|source| {
                            source.symbol == ETH_SYMBOL && source.signal_time_ms == signal_time
                        })
                        .map(|source| (signal_time, source.breakout_time_ms))
                })
                .collect::<Vec<_>>();
            [
                (
                    "ETH 2026-07-24 08:15 source rejected",
                    V36_ETH_SIGNAL_ONE_MS,
                ),
                (
                    "ETH 2026-07-29 11:30 source rejected",
                    V36_ETH_SIGNAL_TWO_MS,
                ),
            ]
            .into_iter()
            .map(|(label, signal_time)| TargetCheck {
                label,
                passed: target_breakouts
                    .iter()
                    .find(|(time, _)| *time == signal_time)
                    .is_some_and(|(_, breakout_time)| {
                        affected.iter().any(|item| {
                            item.symbol == ETH_SYMBOL && item.breakout_time_ms == *breakout_time
                        })
                    }),
            })
            .collect()
        }
        Study::Baseline => Vec::new(),
    }
}

/// 返回报告中可审计的唯一变量名称。
const fn single_variable(study: Study) -> &'static str {
    match study {
        Study::V32 => "prior_three_supporting_wick_switches_only_entry_timing_to_direct_chase",
        Study::V33 => "first_confirmation_opposing_sweep_reclaim_guard",
        Study::V34 => "post_entry_profitable_volume_event_net_break_even_protection",
        Study::V35 => "actual_open_remaining_net_reward_minimum_050r",
        Study::V36 => "visual_boundary_separate_from_96bar_pre_range_extreme_trade_boundary",
        Study::Baseline => "none",
    }
}

/// 统计候选来源覆盖的不同币种数。
fn distinct_symbols_from_sources(items: &[SourceAudit]) -> usize {
    items
        .iter()
        .map(|item| item.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

/// 统计规则实际影响的不同币种数。
fn distinct_symbols_from_affected(items: &[AffectedAudit]) -> usize {
    items
        .iter()
        .map(|item| item.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

/// 只保留预热与评价窗口同时完整的成员，其余成员记录明确原因。
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

/// 对窗口身份、币种顺序、tick 与完整 OHLCV 生成稳定 SHA-256。
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

/// 解析冻结 CLI 合同，不允许从命令行传入阈值或成本。
fn parse_args(values: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut symbols = None;
    let mut evaluation_start_ms = None;
    let mut evaluation_end_exclusive_ms = None;
    let mut study = None;
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
            "--study" => study = Some(Study::parse(&values.next().context("missing study")?)?),
            "--output" => output = Some(PathBuf::from(values.next().context("missing output")?)),
            other => bail!("unsupported argument: {other}"),
        }
    }
    Ok(Args {
        symbols: symbols.context("missing --symbols")?,
        evaluation_start_ms: evaluation_start_ms.context("missing --evaluation-start-ms")?,
        evaluation_end_exclusive_ms: evaluation_end_exclusive_ms
            .context("missing --evaluation-end-exclusive-ms")?,
        study: study.context("missing --study")?,
        output: output.context("missing --output")?,
    })
}

/// 解析并验证显式、非空且不重复的 OKX 交易对列表。
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

/// 解析 Unix 毫秒时间戳并保留原错误上下文。
fn parse_i64(raw: &str) -> Result<i64> {
    raw.parse::<i64>()
        .context("timestamp must be Unix milliseconds")
}

/// 按交易所 tick 向下对齐价格。
fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

/// 按交易所 tick 向上对齐价格。
fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

/// 先写同目录临时文件再原子替换，避免留下半份机器报告。
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
