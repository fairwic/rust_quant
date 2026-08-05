#[path = "tradingview_velocity_top60/args.rs"]
mod cli_args;
#[path = "tradingview_velocity_top60/ema_short_exit_counterfactual.rs"]
mod ema_short_exit_counterfactual;
#[path = "tradingview_velocity_top60/research_runtime.rs"]
mod research_runtime;
#[path = "tradingview_velocity_top60/strict_visual_exit_counterfactual.rs"]
mod strict_visual_exit_counterfactual;
#[path = "tradingview_velocity_top60/strict_visual_range_height_net_break_even.rs"]
mod strict_visual_range_height_net_break_even;
#[path = "tradingview_velocity_top60/strict_visual_structural_failure_counterfactual.rs"]
mod strict_visual_structural_failure_counterfactual;
#[path = "tradingview_velocity_top60/trade_anatomy.rs"]
mod trade_anatomy;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use cli_args::{parse_args, Args};
use ema_short_exit_counterfactual::{
    build_ema_short_completed_close_one_r_net_be,
    build_ema_short_structure_break_failed_retest_net_be, EmaShortCompletedCloseOneRNetBeReport,
    ExitCounterfactualInput,
};
use research_runtime::{
    build_candidate_ledger, cache_key, dataset_fingerprint, elapsed_ms, executable_fingerprint,
    load_baseline_cache, store_baseline_cache, CandidateLedger, PhaseTimings,
    ResearchRuntimeDiagnostics, BASELINE_CACHE_SCHEMA_VERSION,
};
use rust_quant_cli::app::tradingview_velocity_parity::{
    load_frozen_top60_from_quant_core, replay_with_anchor_upthrust_variant,
    replay_with_ema_short_variant, replay_with_ema_trend_long_variant,
    replay_with_sell_climax_base_reclaim_variant, replay_with_strict_visual_breakout_variant,
    verify_v10_pine_source, verify_v11_pine_source, verify_v12_pine_source, verify_v13_pine_source,
    verify_v14_pine_source, verify_v15_pine_source, verify_v16_pine_source, verify_v17_pine_source,
    verify_v18_pine_source, verify_v19_pine_source, verify_v20_pine_source, verify_v2_pine_source,
    verify_v8_pine_source, verify_v9_pine_source, AnchorUpthrustResearchVariant, Candle, Direction,
    EmaShortResearchVariant, EmaTrendLongResearchVariant, FrozenSymbolCandles, Metrics,
    ParityRuleVersion, ReplayConfig, ReplayReport, SellClimaxBaseReclaimResearchVariant,
    SignalFamily, StrictVisualBreakoutResearchVariant, Trade, FROZEN_UNIVERSE_MANIFEST_SHA256,
    FROZEN_UNIVERSE_VERSION, FROZEN_WARMUP_DAYS,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Instant;
use strict_visual_exit_counterfactual::{
    build_strict_visual_completed_close_one_r_net_break_even,
    build_strict_visual_net_break_even_activation, StrictVisualNetBreakEvenActivationReport,
};
use strict_visual_range_height_net_break_even::{
    build_strict_visual_range_height_net_break_even, build_strict_visual_two_close_failure_l1,
    StrictVisualRangeHeightNetBreakEvenReport, StrictVisualTwoCloseFailureL1Report,
};
use strict_visual_structural_failure_counterfactual::{
    build_strict_visual_one_r_range_upper_failure_exit, StrictVisualStructuralFailureReport,
};
use trade_anatomy::{build_ema_short_trade_anatomy, AnatomyInput, EmaShortTradeAnatomyReport};

const COST_FEE_BPS_PER_SIDE: f64 = 5.0;
const COST_SLIPPAGE_BPS_PER_SIDE: f64 = 3.0;
const EFFECTIVE_EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;

/// 冻结 Top60 的 Research-only 汇总结果；不把独立单币回放冒充容量受限组合回测。
#[derive(Debug, Serialize)]
struct Top60ResearchReport {
    report_scope: &'static str,
    generated_at_ms: i64,
    strategy_version: &'static str,
    pine_source_fnv1a32: &'static str,
    ema_short_research_variant: &'static str,
    ema_trend_long_research_variant: &'static str,
    sell_climax_base_reclaim_research_variant: &'static str,
    anchor_upthrust_research_variant: &'static str,
    strict_visual_breakout_research_variant: &'static str,
    universe_version: &'static str,
    universe_manifest_sha256: &'static str,
    dataset_fingerprint_sha256: String,
    data_source: &'static str,
    volume_field: &'static str,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    manifest_evaluation_end_ms: i64,
    evaluation_window_was_clipped: bool,
    warmup_days: i64,
    expected_symbols: usize,
    included_symbols: usize,
    excluded_symbols: Vec<ExcludedSymbol>,
    full_universe_complete: bool,
    zero_cost: AggregateMetrics,
    cost_adjusted: AggregateMetrics,
    time_direction_clusters_60m: usize,
    closed_trade_family_counts: BTreeMap<String, usize>,
    confirmed_range_next_bar_low_volume_bearish: ConfirmedRangeNextBarAudit,
    anchor_upthrust_failed_acceptance_family: FamilyCohortAudit,
    anchor_upthrust_right_side_confirmation_family: FamilyCohortAudit,
    sell_climax_base_reclaim_family: FamilyCohortAudit,
    strict_visual_breakout_family: FamilyCohortAudit,
    strict_visual_breakout_short_family: FamilyCohortAudit,
    ema_short_trade_anatomy_v1: EmaShortTradeAnatomyReport,
    ema_short_completed_close_1r_net_be_v1: Option<EmaShortCompletedCloseOneRNetBeReport>,
    ema_short_structure_break_failed_retest_net_be_v1:
        Option<EmaShortCompletedCloseOneRNetBeReport>,
    strict_visual_net_break_even_activation_v1: Option<StrictVisualNetBreakEvenActivationReport>,
    strict_visual_completed_close_1r_net_break_even_v2:
        Option<StrictVisualNetBreakEvenActivationReport>,
    strict_visual_one_r_range_upper_close_failure_exit_v3:
        Option<StrictVisualStructuralFailureReport>,
    strict_visual_range_height_1_0_net_break_even_v4:
        Option<StrictVisualRangeHeightNetBreakEvenReport>,
    strict_visual_one_height_two_close_failure_l1_v5: Option<StrictVisualTwoCloseFailureL1Report>,
    candidate_ledger: CandidateLedger,
    per_symbol: Vec<SymbolResult>,
    research_runtime: ResearchRuntimeDiagnostics,
    interpretation_limits: Vec<&'static str>,
}

/// 同一数据加载中按声明顺序执行的单变量批次；每个元素仍是完整单变体报告。
#[derive(Debug, Serialize)]
struct Top60VariantBatchReport {
    report_scope: &'static str,
    generated_at_ms: i64,
    variant_axis: &'static str,
    variant_order: Vec<&'static str>,
    variant_count: usize,
    cache_hits: usize,
    shared_phase_timings: PhaseTimings,
    elapsed_before_output_ms: u64,
    variants: Vec<Value>,
    interpretation_limits: Vec<&'static str>,
}

/// 数据不满足完整回放条件的冻结成员及其可审计原因。
#[derive(Debug, Clone, Serialize)]
struct ExcludedSymbol {
    symbol: String,
    evaluation_loaded: usize,
    evaluation_expected: usize,
    evaluation_missing: usize,
    warmup_loaded: usize,
    warmup_expected: usize,
    reason: String,
}

/// 单币固定 1 单位结果；交易明细使用零成本路径，成本路径不会改变信号或成交时刻。
#[derive(Debug, Serialize)]
struct SymbolResult {
    symbol: String,
    tick_size: f64,
    zero_cost: MetricSnapshot,
    cost_adjusted: MetricSnapshot,
    blocked_signal_count: usize,
    open_position_at_end: bool,
    pending_entry_at_end: bool,
    trades: Vec<Trade>,
}

/// 可安全序列化的指标快照；无亏损时用布尔字段表达无限 PF，避免 JSON 非有限数。
#[derive(Debug, Clone, Serialize)]
struct MetricSnapshot {
    trades: usize,
    wins: usize,
    losses: usize,
    net_pnl: f64,
    gross_profit: f64,
    gross_loss: f64,
    profit_factor: Option<f64>,
    profit_factor_is_infinite: bool,
    win_rate_percent: f64,
    average_net_r: f64,
    max_drawdown: f64,
}

/// 所有可回放成员的独立单币结果汇总，不包含资金容量与相关性约束。
#[derive(Debug, Serialize)]
struct AggregateMetrics {
    symbols: usize,
    trades: usize,
    wins: usize,
    losses: usize,
    net_pnl: f64,
    gross_profit: f64,
    gross_loss: f64,
    profit_factor: Option<f64>,
    profit_factor_is_infinite: bool,
    win_rate_percent: f64,
    average_net_r: f64,
    chronological_closed_equity_max_drawdown: f64,
    max_single_symbol_intrabar_drawdown: f64,
    profitable_symbols: usize,
    losing_symbols: usize,
    flat_symbols: usize,
    open_positions_at_end: usize,
    pending_entries_at_end: usize,
}

/// 已确认箱体多单在入场棒收盘后才可观察到的“缩量阴线”诊断，不作为原信号前视门禁。
#[derive(Debug, Serialize)]
struct ConfirmedRangeNextBarAudit {
    definition: &'static str,
    missing_next_bar_or_history: usize,
    zero_cost: CohortMetrics,
    cost_adjusted: CohortMetrics,
}

/// 严格同一后验样本在一种成本口径下的交易结果。
#[derive(Debug, Default, Serialize)]
struct CohortMetrics {
    trades: usize,
    wins: usize,
    losses: usize,
    win_rate_percent: f64,
    net_r: f64,
    average_net_r: f64,
    profit_factor_r: Option<f64>,
    profit_factor_r_is_infinite: bool,
}

/// 新增家族在零成本与成本压力下的独立贡献，避免被主策略总量稀释。
#[derive(Debug, Serialize)]
struct FamilyCohortAudit {
    family: &'static str,
    zero_cost: CohortMetrics,
    cost_adjusted: CohortMetrics,
}

/// 多变体共享同一份行情、成员筛选和窗口，避免每个阈值重新读库。
struct SharedRunContext<'a> {
    args: &'a Args,
    eligible_symbols: &'a [FrozenSymbolCandles],
    excluded_symbols: &'a [ExcludedSymbol],
    evaluation_start_ms: i64,
    replay_end_ms: i64,
    manifest_evaluation_end_ms: i64,
    expected_symbols: usize,
    full_universe_complete: bool,
    dataset_fingerprint_sha256: &'a str,
    executable_fingerprint_sha256: &'a str,
}

/// 从 quant_core 只读加载冻结 Top60，并按显式 Pine 版本同时跑零成本与成本后口径。
#[tokio::main]
async fn main() -> Result<()> {
    let total_started_at = Instant::now();
    let args = parse_args(std::env::args().skip(1))?;
    let mut shared_phase_timings = PhaseTimings::default();

    let phase_started_at = Instant::now();
    verify_selected_pine_source(args.rule_version)?;
    shared_phase_timings.pine_source_verification_ms = elapsed_ms(phase_started_at);

    let phase_started_at = Instant::now();
    let mut dataset = load_frozen_top60_from_quant_core().await?;
    shared_phase_timings.data_load_ms = elapsed_ms(phase_started_at);
    if dataset.universe_version != FROZEN_UNIVERSE_VERSION
        || dataset.manifest_sha256 != FROZEN_UNIVERSE_MANIFEST_SHA256
    {
        bail!("加载结果的冻结币池 identity 与编译时常量不一致");
    }

    let phase_started_at = Instant::now();
    let dataset_fingerprint_sha256 = dataset_fingerprint(&dataset);
    let executable_fingerprint_sha256 = executable_fingerprint()?;
    shared_phase_timings.dataset_fingerprint_ms = elapsed_ms(phase_started_at);

    let phase_started_at = Instant::now();
    let evaluation_start_ms = dataset.window_start_ms;
    let evaluation_end_ms = dataset
        .window_end_ms
        .checked_sub(1)
        .context("冻结评价上界下溢")?;
    let replay_end_ms = if args.allow_partial_diagnostic {
        modal_snapshot_end(&dataset.symbols, dataset.window_start_ms)
            .context("冻结 Top60 没有可用于确定数据库快照上界的正式窗口 K 线")?
    } else {
        evaluation_end_ms
    };
    let expected_symbols = dataset.coverage.expected_symbol_count;
    let mut excluded_symbols = Vec::new();
    let mut eligible_symbols = Vec::new();

    for symbol in std::mem::take(&mut dataset.symbols) {
        let clipped_coverage =
            replay_window_coverage(&symbol.candles, evaluation_start_ms, replay_end_ms)?;
        if !clipped_coverage.is_complete || !symbol.warmup_is_complete {
            excluded_symbols.push(ExcludedSymbol {
                symbol: symbol.symbol,
                evaluation_loaded: clipped_coverage.loaded,
                evaluation_expected: clipped_coverage.expected,
                evaluation_missing: clipped_coverage.expected - clipped_coverage.loaded,
                warmup_loaded: symbol.warmup_loaded_candle_count,
                warmup_expected: symbol.warmup_expected_candle_count,
                reason: match (clipped_coverage.is_complete, symbol.warmup_is_complete) {
                    (false, false) => "评价窗口与预热窗口均不完整",
                    (false, true) => "评价窗口不完整",
                    (true, false) => "60 天指标预热不完整",
                    (true, true) => unreachable!("完整成员不会进入排除分支"),
                }
                .to_owned(),
            });
            continue;
        }
        eligible_symbols.push(symbol);
    }

    let included_symbols = eligible_symbols.len();
    let full_universe_complete =
        included_symbols == expected_symbols && replay_end_ms == evaluation_end_ms;
    if !full_universe_complete && !args.allow_partial_diagnostic {
        bail!(
            "Top60 严格回放失败：完整成员 {}/{}，实际末根={}，manifest 末根={}；如只需数据诊断，显式使用 --allow-partial-diagnostic",
            included_symbols,
            expected_symbols,
            replay_end_ms,
            evaluation_end_ms
        );
    }
    if eligible_symbols.is_empty() {
        bail!("冻结 Top60 没有任何同时具备完整评价窗口和 60 天预热的成员");
    }
    shared_phase_timings.eligibility_ms = elapsed_ms(phase_started_at);

    let context = SharedRunContext {
        args: &args,
        eligible_symbols: &eligible_symbols,
        excluded_symbols: &excluded_symbols,
        evaluation_start_ms,
        replay_end_ms,
        manifest_evaluation_end_ms: evaluation_end_ms,
        expected_symbols,
        full_universe_complete,
        dataset_fingerprint_sha256: &dataset_fingerprint_sha256,
        executable_fingerprint_sha256: &executable_fingerprint_sha256,
    };
    let variant_order = args
        .anchor_upthrust_variants
        .iter()
        .map(|variant| variant.slug())
        .collect::<Vec<_>>();
    let mut variant_values = Vec::with_capacity(variant_order.len());
    let mut cache_hits = 0_usize;

    for anchor_upthrust_variant in args.anchor_upthrust_variants.iter().copied() {
        let baseline_cache_key_sha256 = is_baseline_run(&args, anchor_upthrust_variant)
            .then(|| baseline_cache_key(&context, anchor_upthrust_variant))
            .transpose()?;
        if args.baseline_cache_enabled {
            if let Some(key) = baseline_cache_key_sha256.as_deref() {
                if let Some(mut cached) = load_baseline_cache(&args.baseline_cache_dir, key)? {
                    patch_cached_runtime(
                        &mut cached,
                        ResearchRuntimeDiagnostics {
                            cache_schema_version: BASELINE_CACHE_SCHEMA_VERSION,
                            dataset_fingerprint_sha256: dataset_fingerprint_sha256.clone(),
                            executable_fingerprint_sha256: executable_fingerprint_sha256.clone(),
                            baseline_cache_key_sha256: Some(key.to_owned()),
                            baseline_cache_hit: true,
                            baseline_cache_reused_at_ms: Some(Utc::now().timestamp_millis()),
                            phase_timings: shared_phase_timings.clone(),
                        },
                    )?;
                    cache_hits += 1;
                    variant_values.push(cached);
                    continue;
                }
            }
        }

        let runtime = ResearchRuntimeDiagnostics {
            cache_schema_version: BASELINE_CACHE_SCHEMA_VERSION,
            dataset_fingerprint_sha256: dataset_fingerprint_sha256.clone(),
            executable_fingerprint_sha256: executable_fingerprint_sha256.clone(),
            baseline_cache_key_sha256: baseline_cache_key_sha256.clone(),
            baseline_cache_hit: false,
            baseline_cache_reused_at_ms: None,
            phase_timings: shared_phase_timings.clone(),
        };
        let report = build_variant_report(&context, anchor_upthrust_variant, runtime)?;
        let phase_started_at = Instant::now();
        let mut value = serde_json::to_value(report)?;
        value["research_runtime"]["phase_timings"]["serialization_ms"] =
            json!(elapsed_ms(phase_started_at));
        if args.baseline_cache_enabled {
            if let Some(key) = baseline_cache_key_sha256.as_deref() {
                store_baseline_cache(&args.baseline_cache_dir, key, &value)?;
            }
        }
        variant_values.push(value);
    }

    let elapsed_before_output_ms = elapsed_ms(total_started_at);
    let output_value = if variant_values.len() == 1 {
        variant_values.pop().context("单变体报告缺失")?
    } else {
        serde_json::to_value(Top60VariantBatchReport {
            report_scope: "single_process_multi_variant_batch",
            generated_at_ms: Utc::now().timestamp_millis(),
            variant_axis: "anchor_upthrust_research_variant",
            variant_order,
            variant_count: variant_values.len(),
            cache_hits,
            shared_phase_timings: shared_phase_timings.clone(),
            elapsed_before_output_ms,
            variants: variant_values,
            interpretation_limits: vec![
                "all variants share one data load, one eligibility pass, and one immutable dataset fingerprint",
                "each non-cached variant still performs its own broker replay because entry conflicts and position paths may differ",
                "variant order is declaration order and must not be ranked after looking at outcomes without a preregistered selection rule",
            ],
        })?
    };
    let output_serialization_started_at = Instant::now();
    let report_json = serde_json::to_string_pretty(&output_value)?;
    let output_serialization_ms = elapsed_ms(output_serialization_started_at);
    if let Some(output) = args.output.as_ref() {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建报告目录失败：{}", parent.display()))?;
        }
        std::fs::write(output, format!("{report_json}\n"))
            .with_context(|| format!("写入 Top60 报告失败：{}", output.display()))?;
        println!("{}", output.display());
    } else {
        println!("{report_json}");
    }
    eprintln!(
        "Top60 phases(ms): pine={} load={} fingerprint={} eligibility={} output={} total={} variants={} cache_hits={}",
        shared_phase_timings.pine_source_verification_ms,
        shared_phase_timings.data_load_ms,
        shared_phase_timings.dataset_fingerprint_ms,
        shared_phase_timings.eligibility_ms,
        output_serialization_ms,
        elapsed_ms(total_started_at),
        args.anchor_upthrust_variants.len(),
        cache_hits,
    );
    Ok(())
}

/// 构建一个完整单变体报告；共享阶段只读，回放与分析耗时只归属于当前变体。
fn build_variant_report(
    context: &SharedRunContext<'_>,
    anchor_upthrust_variant: AnchorUpthrustResearchVariant,
    mut runtime: ResearchRuntimeDiagnostics,
) -> Result<Top60ResearchReport> {
    let replay_started_at = Instant::now();
    let mut zero_cost_reports = Vec::with_capacity(context.eligible_symbols.len());
    let mut cost_adjusted_reports = Vec::with_capacity(context.eligible_symbols.len());
    for symbol in context.eligible_symbols {
        let baseline_config = replay_config(
            context.args.rule_version,
            symbol.symbol.clone(),
            symbol.tick_size,
            context.evaluation_start_ms,
            context.replay_end_ms,
        );
        zero_cost_reports.push(replay_selected_variant(
            &symbol.candles,
            baseline_config.clone(),
            context.args.ema_short_variant,
            context.args.ema_trend_long_variant,
            context.args.sell_climax_base_reclaim_variant,
            anchor_upthrust_variant,
            context.args.strict_visual_breakout_variant,
        ));
        cost_adjusted_reports.push(replay_selected_variant(
            &symbol.candles,
            ReplayConfig {
                fee_bps_per_side: COST_FEE_BPS_PER_SIDE,
                slippage_bps_per_side: COST_SLIPPAGE_BPS_PER_SIDE,
                ..baseline_config
            },
            context.args.ema_short_variant,
            context.args.ema_trend_long_variant,
            context.args.sell_climax_base_reclaim_variant,
            anchor_upthrust_variant,
            context.args.strict_visual_breakout_variant,
        ));
    }
    runtime.phase_timings.replay_ms = elapsed_ms(replay_started_at);

    let ledger_started_at = Instant::now();
    let candidate_ledger = build_candidate_ledger(&zero_cost_reports, &cost_adjusted_reports)?;
    runtime.phase_timings.candidate_ledger_ms = elapsed_ms(ledger_started_at);

    let analysis_started_at = Instant::now();
    let time_direction_clusters_60m = time_direction_cluster_count(&zero_cost_reports);
    let family_trade_counts = family_counts(&zero_cost_reports);
    let zero_cost = aggregate_metrics(&zero_cost_reports);
    let cost_adjusted = aggregate_metrics(&cost_adjusted_reports);
    let confirmed_range_next_bar_low_volume_bearish = confirmed_range_next_bar_audit(
        context.eligible_symbols,
        &zero_cost_reports,
        &cost_adjusted_reports,
    )?;
    let anchor_upthrust_failed_acceptance_family = family_cohort_audit(
        &zero_cost_reports,
        &cost_adjusted_reports,
        SignalFamily::AnchorUpthrustFailedAcceptanceShort,
        "anchor_upthrust_failed_acceptance_short",
    )?;
    let anchor_upthrust_right_side_confirmation_family = family_cohort_audit(
        &zero_cost_reports,
        &cost_adjusted_reports,
        SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort,
        "anchor_upthrust_failed_acceptance_right_side_short",
    )?;
    let sell_climax_base_reclaim_family = family_cohort_audit(
        &zero_cost_reports,
        &cost_adjusted_reports,
        SignalFamily::SellClimaxBaseReclaimLong,
        "sell_climax_base_reclaim_long",
    )?;
    let strict_visual_breakout_family = family_cohort_audit(
        &zero_cost_reports,
        &cost_adjusted_reports,
        SignalFamily::StrictVisualConsolidationBreakLong,
        "strict_visual_consolidation_break_long",
    )?;
    let strict_visual_breakout_short_family = family_cohort_audit(
        &zero_cost_reports,
        &cost_adjusted_reports,
        SignalFamily::StrictVisualConsolidationBreakShort,
        "strict_visual_consolidation_break_short",
    )?;
    let anatomy_inputs = context
        .eligible_symbols
        .iter()
        .zip(&zero_cost_reports)
        .zip(&cost_adjusted_reports)
        .map(|((symbol, zero_cost), cost_adjusted)| AnatomyInput {
            symbol: &symbol.symbol,
            candles: &symbol.candles,
            zero_cost,
            cost_adjusted,
        })
        .collect::<Vec<_>>();
    let ema_short_trade_anatomy_v1 = build_ema_short_trade_anatomy(&anatomy_inputs)?;
    let exit_counterfactual_inputs = context
        .eligible_symbols
        .iter()
        .zip(&zero_cost_reports)
        .zip(&cost_adjusted_reports)
        .map(
            |((symbol, zero_cost), cost_adjusted)| ExitCounterfactualInput {
                symbol: &symbol.symbol,
                tick_size: symbol.tick_size,
                candles: &symbol.candles,
                zero_cost,
                cost_adjusted,
            },
        )
        .collect::<Vec<_>>();
    let ema_short_completed_close_1r_net_be_v1 = (context.args.ema_short_variant
        == EmaShortResearchVariant::StructureBreak)
        .then(|| build_ema_short_completed_close_one_r_net_be(&exit_counterfactual_inputs))
        .transpose()?;
    let ema_short_structure_break_failed_retest_net_be_v1 = (context.args.ema_short_variant
        == EmaShortResearchVariant::StructureBreak)
        .then(|| build_ema_short_structure_break_failed_retest_net_be(&exit_counterfactual_inputs))
        .transpose()?;
    let strict_visual_net_break_even_activation_v1 = matches!(
        context.args.strict_visual_breakout_variant,
        StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation
            | StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr
    )
    .then(|| build_strict_visual_net_break_even_activation(&exit_counterfactual_inputs))
    .transpose()?;
    let strict_visual_completed_close_1r_net_break_even_v2 = matches!(
        context.args.strict_visual_breakout_variant,
        StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation
            | StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr
    )
    .then(|| build_strict_visual_completed_close_one_r_net_break_even(&exit_counterfactual_inputs))
    .transpose()?;
    let strict_visual_one_r_range_upper_close_failure_exit_v3 = matches!(
        context.args.strict_visual_breakout_variant,
        StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation
            | StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr
    )
    .then(|| {
        build_strict_visual_one_r_range_upper_failure_exit(
            &exit_counterfactual_inputs,
            &candidate_ledger,
        )
    })
    .transpose()?;
    let strict_visual_range_height_1_0_net_break_even_v4 =
        (context.args.strict_visual_breakout_variant
            == StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr)
            .then(|| build_strict_visual_range_height_net_break_even(&exit_counterfactual_inputs))
            .transpose()?;
    let strict_visual_one_height_two_close_failure_l1_v5 =
        (context.args.strict_visual_breakout_variant
            == StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr)
            .then(|| build_strict_visual_two_close_failure_l1(&exit_counterfactual_inputs))
            .transpose()?;
    runtime.phase_timings.analysis_ms = elapsed_ms(analysis_started_at);

    let per_symbol = zero_cost_reports
        .into_iter()
        .zip(cost_adjusted_reports)
        .map(|(zero, cost)| SymbolResult {
            symbol: zero.symbol.clone(),
            tick_size: zero.tick_size,
            zero_cost: metric_snapshot(&zero.metrics),
            cost_adjusted: metric_snapshot(&cost.metrics),
            blocked_signal_count: zero.blocked_signals.len(),
            open_position_at_end: zero.open_position_at_end,
            pending_entry_at_end: zero.pending_entry_at_end,
            trades: zero.trades,
        })
        .collect::<Vec<_>>();
    Ok(Top60ResearchReport {
        report_scope: if context.full_universe_complete {
            "strict_top60"
        } else {
            "partial_data_diagnostic"
        },
        generated_at_ms: Utc::now().timestamp_millis(),
        strategy_version: strategy_version(context.args, anchor_upthrust_variant),
        pine_source_fnv1a32: context.args.rule_version.pine_source_fnv1a32(),
        ema_short_research_variant: context.args.ema_short_variant.slug(),
        ema_trend_long_research_variant: context.args.ema_trend_long_variant.slug(),
        sell_climax_base_reclaim_research_variant: context
            .args
            .sell_climax_base_reclaim_variant
            .slug(),
        anchor_upthrust_research_variant: anchor_upthrust_variant.slug(),
        strict_visual_breakout_research_variant: context
            .args
            .strict_visual_breakout_variant
            .slug(),
        universe_version: FROZEN_UNIVERSE_VERSION,
        universe_manifest_sha256: FROZEN_UNIVERSE_MANIFEST_SHA256,
        dataset_fingerprint_sha256: context.dataset_fingerprint_sha256.to_owned(),
        data_source: "quant_core per-symbol confirmed OKX 15m tables",
        volume_field: "vol_ccy",
        evaluation_start_ms: context.evaluation_start_ms,
        evaluation_end_ms: context.replay_end_ms,
        manifest_evaluation_end_ms: context.manifest_evaluation_end_ms,
        evaluation_window_was_clipped: context.replay_end_ms
            != context.manifest_evaluation_end_ms,
        warmup_days: FROZEN_WARMUP_DAYS,
        expected_symbols: context.expected_symbols,
        included_symbols: context.eligible_symbols.len(),
        excluded_symbols: context.excluded_symbols.to_vec(),
        full_universe_complete: context.full_universe_complete,
        zero_cost,
        cost_adjusted,
        time_direction_clusters_60m,
        closed_trade_family_counts: family_trade_counts,
        confirmed_range_next_bar_low_volume_bearish,
        anchor_upthrust_failed_acceptance_family,
        anchor_upthrust_right_side_confirmation_family,
        sell_climax_base_reclaim_family,
        strict_visual_breakout_family,
        strict_visual_breakout_short_family,
        ema_short_trade_anatomy_v1,
        ema_short_completed_close_1r_net_be_v1,
        ema_short_structure_break_failed_retest_net_be_v1,
        strict_visual_net_break_even_activation_v1,
        strict_visual_completed_close_1r_net_break_even_v2,
        strict_visual_one_r_range_upper_close_failure_exit_v3,
        strict_visual_range_height_1_0_net_break_even_v4,
        strict_visual_one_height_two_close_failure_l1_v5,
        candidate_ledger,
        per_symbol,
        research_runtime: runtime,
        interpretation_limits: vec![
            "current-live frozen Top60 has survivorship bias and is not point-in-time OOS",
            "fixed 1 unit per symbol; aggregate is not a capacity/correlation constrained portfolio",
            "tick_size comes from the current exchange_symbols row because the manifest did not freeze it",
            "when the local database snapshot ends before the manifest boundary, all symbols use the modal common end and the report marks the window as clipped",
            "cost mode is a post-hoc 5 bps fee plus 3 bps slippage-equivalent stress on each side; it does not move fills or protective levels",
            "60m time-direction clusters are not the formal correlation/sector/market-state effective-event metric",
            "EMA short trade anatomy covers executed closed trades only; blocked raw candidates are outside the sample",
            "trade-anatomy forward bars are outcome labels only and never alter replay entry or exit behavior",
            "candidate-ledger L1 threshold scans must not read outcome or blocked_events as signal-time features",
            "baseline cache reuse requires identical data, executable, Pine identity, window, costs, variants, and universe-completeness mode",
            "the +1R net-break-even study is an isolated exit counterfactual over frozen D0 trades, so it cannot release capacity or manufacture later entries",
            "the structure-confirmed net-break-even study freezes the first post-1R close below the prior 20-bar low and only activates after a later failed retest; no lookback, tolerance, or timeout is selected from outcomes",
            "the strict-visual net-break-even study is an isolated exit counterfactual; completed-high activation starts on the next candle and cannot release capacity or manufacture later entries",
        ],
    })
}

/// 只有所有研究轴均为冻结基线时才缓存，候选变体始终重新完整回放。
fn is_baseline_run(args: &Args, anchor_upthrust_variant: AnchorUpthrustResearchVariant) -> bool {
    args.ema_short_variant == EmaShortResearchVariant::Baseline
        && args.ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline
        && !args.sell_climax_base_reclaim_variant.is_enabled()
        && !args.strict_visual_breakout_variant.is_enabled()
        && anchor_upthrust_variant == AnchorUpthrustResearchVariant::Baseline
}

/// 缓存键绑定所有会改变回放结果或结论适用范围的输入。
fn baseline_cache_key(
    context: &SharedRunContext<'_>,
    anchor_upthrust_variant: AnchorUpthrustResearchVariant,
) -> Result<String> {
    let identity = json!({
        "cache_schema_version": BASELINE_CACHE_SCHEMA_VERSION,
        "dataset_fingerprint_sha256": context.dataset_fingerprint_sha256,
        "executable_fingerprint_sha256": context.executable_fingerprint_sha256,
        "pine_source_fnv1a32": context.args.rule_version.pine_source_fnv1a32(),
        "strategy_version": strategy_version(context.args, anchor_upthrust_variant),
        "ema_short_variant": context.args.ema_short_variant.slug(),
        "ema_trend_long_variant": context.args.ema_trend_long_variant.slug(),
        "sell_climax_base_reclaim_variant": context.args.sell_climax_base_reclaim_variant.slug(),
        "anchor_upthrust_variant": anchor_upthrust_variant.slug(),
        "strict_visual_breakout_variant": context.args.strict_visual_breakout_variant.slug(),
        "evaluation_start_ms": context.evaluation_start_ms,
        "evaluation_end_ms": context.replay_end_ms,
        "manifest_evaluation_end_ms": context.manifest_evaluation_end_ms,
        "full_universe_complete": context.full_universe_complete,
        "expected_symbols": context.expected_symbols,
        "included_symbols": context.eligible_symbols.len(),
        "fee_bps_per_side": COST_FEE_BPS_PER_SIDE,
        "slippage_bps_per_side": COST_SLIPPAGE_BPS_PER_SIDE,
    });
    Ok(cache_key(&serde_json::to_string(&identity)?))
}

/// 缓存命中只替换运行诊断，历史计算时间与交易结果保持原样可审计。
fn patch_cached_runtime(value: &mut Value, runtime: ResearchRuntimeDiagnostics) -> Result<()> {
    let object = value
        .as_object_mut()
        .context("Top60 baseline 缓存根节点不是 JSON object")?;
    object.insert(
        "research_runtime".to_owned(),
        serde_json::to_value(runtime)?,
    );
    Ok(())
}

/// 把当前只启用的研究轴映射到可审计策略版本。
fn strategy_version(
    args: &Args,
    anchor_upthrust_variant: AnchorUpthrustResearchVariant,
) -> &'static str {
    if args.strict_visual_breakout_variant.is_enabled() {
        args.strict_visual_breakout_variant
            .strategy_version(args.rule_version)
    } else if anchor_upthrust_variant.is_enabled() {
        anchor_upthrust_variant.strategy_version(args.rule_version)
    } else if args.sell_climax_base_reclaim_variant.is_enabled() {
        args.sell_climax_base_reclaim_variant
            .strategy_version(args.rule_version)
    } else if args.ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline {
        args.ema_short_variant.strategy_version(args.rule_version)
    } else {
        args.ema_trend_long_variant
            .strategy_version(args.rule_version)
    }
}

/// 在同一回放中只选择一个 Research 维度，避免两个消融相互污染。
fn replay_selected_variant(
    candles: &[Candle],
    config: ReplayConfig,
    ema_short_variant: EmaShortResearchVariant,
    ema_trend_long_variant: EmaTrendLongResearchVariant,
    sell_climax_base_reclaim_variant: SellClimaxBaseReclaimResearchVariant,
    anchor_upthrust_variant: AnchorUpthrustResearchVariant,
    strict_visual_breakout_variant: StrictVisualBreakoutResearchVariant,
) -> ReplayReport {
    if strict_visual_breakout_variant.is_enabled() {
        replay_with_strict_visual_breakout_variant(candles, config, strict_visual_breakout_variant)
    } else if anchor_upthrust_variant.is_enabled() {
        replay_with_anchor_upthrust_variant(candles, config, anchor_upthrust_variant)
    } else if sell_climax_base_reclaim_variant.is_enabled() {
        replay_with_sell_climax_base_reclaim_variant(
            candles,
            config,
            sell_climax_base_reclaim_variant,
        )
    } else if ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline {
        replay_with_ema_short_variant(candles, config, ema_short_variant)
    } else {
        replay_with_ema_trend_long_variant(candles, config, ema_trend_long_variant)
    }
}

/// 校验本次回放绑定的 Pine 快照，禁止版本名与源码身份分离。
fn verify_selected_pine_source(rule_version: ParityRuleVersion) -> Result<()> {
    match rule_version {
        ParityRuleVersion::Current3cbbc9d8 => verify_v2_pine_source(),
        ParityRuleVersion::CandidateV8 => verify_v8_pine_source(),
        ParityRuleVersion::CandidateV9 => verify_v9_pine_source(),
        ParityRuleVersion::CandidateV10 => verify_v10_pine_source(),
        ParityRuleVersion::CandidateV11 => verify_v11_pine_source(),
        ParityRuleVersion::CandidateV12 => verify_v12_pine_source(),
        ParityRuleVersion::CandidateV13 => verify_v13_pine_source(),
        ParityRuleVersion::CandidateV14 => verify_v14_pine_source(),
        ParityRuleVersion::CandidateV15 => verify_v15_pine_source(),
        ParityRuleVersion::CandidateV16 => verify_v16_pine_source(),
        ParityRuleVersion::CandidateV17 => verify_v17_pine_source(),
        ParityRuleVersion::CandidateV18 => verify_v18_pine_source(),
        ParityRuleVersion::CandidateV19 => verify_v19_pine_source(),
        ParityRuleVersion::CandidateV20 => verify_v20_pine_source(),
        _ => bail!(
            "Top60 数据库 runner 仅支持 current-v2、candidate-v8、candidate-v9、candidate-v10、candidate-v11、candidate-v12、candidate-v13、candidate-v14、candidate-v15、candidate-v16、candidate-v17、candidate-v18、candidate-v19 或 candidate-v20"
        ),
    }
}

/// 构造显式版本配置；V8/V9 A/B 共享同一评价窗口与成交成本。
fn replay_config(
    rule_version: ParityRuleVersion,
    symbol: String,
    tick_size: f64,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
) -> ReplayConfig {
    match rule_version {
        ParityRuleVersion::Current3cbbc9d8 => {
            ReplayConfig::current_pine_v2(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV8 => {
            ReplayConfig::current_pine_v8(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV9 => {
            ReplayConfig::current_pine_v9(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV10 => ReplayConfig::current_pine_v10(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV11 => ReplayConfig::current_pine_v11(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV12 => ReplayConfig::current_pine_v12(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV13 => ReplayConfig::current_pine_v13(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV14 => ReplayConfig::current_pine_v14(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV15 => ReplayConfig::current_pine_v15(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV16 => ReplayConfig::current_pine_v16(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV17 => ReplayConfig::current_pine_v17(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV18 => ReplayConfig::current_pine_v18(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV19 => ReplayConfig::current_pine_v19(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV20 => ReplayConfig::current_pine_v20(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        _ => unreachable!("parse_args and verify_selected_pine_source reject other versions"),
    }
}

/// 一个成员在统一正式窗口内的 15m 连续覆盖结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReplayWindowCoverage {
    expected: usize,
    loaded: usize,
    is_complete: bool,
}

/// 使用成员中最常见的末根时间冻结本机数据库快照上界，避免少数停牌币把全体窗口截短。
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

/// 校验单币在统一快照窗口中逐根连续；起点、尾部或内部任何缺口都失败关闭。
fn replay_window_coverage(
    candles: &[Candle],
    start_ms: i64,
    end_ms: i64,
) -> Result<ReplayWindowCoverage> {
    if end_ms < start_ms
        || start_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
        || end_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
    {
        bail!("Top60 实际评价窗口没有对齐 15m");
    }
    let expected =
        usize::try_from((end_ms - start_ms) / CANDLE_INTERVAL_MS + 1).context("评价根数溢出")?;
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

/// 统计“信号后下一根缩量阴线”后验队列；该条件到入场棒收盘才完整可见。
fn confirmed_range_next_bar_audit(
    symbols: &[FrozenSymbolCandles],
    zero_cost_reports: &[ReplayReport],
    cost_adjusted_reports: &[ReplayReport],
) -> Result<ConfirmedRangeNextBarAudit> {
    let (zero_cost, zero_missing) = confirmed_range_next_bar_cohort(symbols, zero_cost_reports)?;
    let (cost_adjusted, cost_missing) =
        confirmed_range_next_bar_cohort(symbols, cost_adjusted_reports)?;
    if zero_missing != cost_missing || zero_cost.trades != cost_adjusted.trades {
        bail!("箱体突破后验队列在零成本与成本后路径上发生样本漂移");
    }
    Ok(ConfirmedRangeNextBarAudit {
        definition: "closed confirmed_range_acceptance_long trades whose t+1 candle closed below open, vol_ccy[t+1] <= 50% of signal vol_ccy, and vol_ccy[t+1] <= the median vol_ccy of the 20 completed candles ending at t; t+1 is the entry candle, so this is ex-post only",
        missing_next_bar_or_history: zero_missing,
        zero_cost,
        cost_adjusted,
    })
}

/// 在同源已完成 K 线上定位每笔信号及其下一根，拒绝缺失历史被默认为零量。
fn confirmed_range_next_bar_cohort(
    symbols: &[FrozenSymbolCandles],
    reports: &[ReplayReport],
) -> Result<(CohortMetrics, usize)> {
    if symbols.len() != reports.len() {
        bail!("箱体突破后验队列的行情与回放成员数量不一致");
    }
    let mut selected = Vec::new();
    let mut missing = 0;
    for (symbol, report) in symbols.iter().zip(reports) {
        if symbol.symbol != report.symbol {
            bail!(
                "箱体突破后验队列成员顺序不一致：{} != {}",
                symbol.symbol,
                report.symbol
            );
        }
        for trade in report.trades.iter().filter(|trade| {
            trade
                .families
                .contains(&SignalFamily::ConfirmedRangeAcceptanceLong)
        }) {
            let Ok(signal_index) = symbol
                .candles
                .binary_search_by_key(&trade.signal_time_ms, |candle| candle.timestamp_ms)
            else {
                missing += 1;
                continue;
            };
            let Some(next) = symbol.candles.get(signal_index + 1) else {
                missing += 1;
                continue;
            };
            if signal_index < 19 || next.timestamp_ms != trade.signal_time_ms + CANDLE_INTERVAL_MS {
                missing += 1;
                continue;
            }
            let mut prior_volumes = symbol.candles[signal_index - 19..=signal_index]
                .iter()
                .map(|candle| candle.volume)
                .collect::<Vec<_>>();
            prior_volumes.sort_by(f64::total_cmp);
            let median_volume = (prior_volumes[9] + prior_volumes[10]) / 2.0;
            if is_low_volume_bearish_next(symbol.candles[signal_index], *next, median_volume) {
                selected.push(trade);
            }
        }
    }
    Ok((cohort_metrics(&selected), missing))
}

/// “无量阴线”预注册为相对信号量能减半，且不高于信号时已知 20 根量能中位数。
fn is_low_volume_bearish_next(signal: Candle, next: Candle, median_volume: f64) -> bool {
    next.close < next.open && next.volume <= signal.volume * 0.5 && next.volume <= median_volume
}

/// 对同一信号家族提取两条成本路径，样本数量漂移时立即拒绝报告。
fn family_cohort_audit(
    zero_cost_reports: &[ReplayReport],
    cost_adjusted_reports: &[ReplayReport],
    family: SignalFamily,
    family_name: &'static str,
) -> Result<FamilyCohortAudit> {
    let zero_cost_trades = zero_cost_reports
        .iter()
        .flat_map(|report| &report.trades)
        .filter(|trade| trade.families.contains(&family))
        .collect::<Vec<_>>();
    let cost_adjusted_trades = cost_adjusted_reports
        .iter()
        .flat_map(|report| &report.trades)
        .filter(|trade| trade.families.contains(&family))
        .collect::<Vec<_>>();
    if zero_cost_trades.len() != cost_adjusted_trades.len() {
        bail!("{family_name} 在零成本与成本路径上的样本数量发生漂移");
    }
    Ok(FamilyCohortAudit {
        family: family_name,
        zero_cost: cohort_metrics(&zero_cost_trades),
        cost_adjusted: cohort_metrics(&cost_adjusted_trades),
    })
}

/// 把已筛选交易转换为 R 口径胜率、期望与 PF，避免价格尺度污染跨币比较。
fn cohort_metrics(trades: &[&Trade]) -> CohortMetrics {
    let wins = trades.iter().filter(|trade| trade.net_r > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.net_r < 0.0).count();
    let net_r = trades.iter().map(|trade| trade.net_r).sum::<f64>();
    let gross_profit_r = trades.iter().map(|trade| trade.net_r.max(0.0)).sum::<f64>();
    let gross_loss_r = trades
        .iter()
        .map(|trade| (-trade.net_r).max(0.0))
        .sum::<f64>();
    CohortMetrics {
        trades: trades.len(),
        wins,
        losses,
        win_rate_percent: if trades.is_empty() {
            0.0
        } else {
            wins as f64 / trades.len() as f64 * 100.0
        },
        net_r,
        average_net_r: if trades.is_empty() {
            0.0
        } else {
            net_r / trades.len() as f64
        },
        profit_factor_r: (gross_loss_r > 0.0).then_some(gross_profit_r / gross_loss_r),
        profit_factor_r_is_infinite: gross_loss_r == 0.0 && gross_profit_r > 0.0,
    }
}

/// 把内部非有限 PF 显式拆成可安全写入 JSON 的值与状态。
fn metric_snapshot(metrics: &Metrics) -> MetricSnapshot {
    let raw_pf = metrics.profit_factor;
    MetricSnapshot {
        trades: metrics.trades,
        wins: metrics.wins,
        losses: metrics.losses,
        net_pnl: metrics.net_pnl,
        gross_profit: metrics.gross_profit,
        gross_loss: metrics.gross_loss,
        profit_factor: raw_pf.filter(|value| value.is_finite()),
        profit_factor_is_infinite: raw_pf.is_some_and(|value| value.is_infinite()),
        win_rate_percent: metrics.win_rate_percent,
        average_net_r: metrics.average_net_r,
        max_drawdown: metrics.max_drawdown,
    }
}

/// 汇总独立单币逐笔结果；该口径没有共享资金、容量或相关性约束。
fn aggregate_metrics(
    reports: &[rust_quant_cli::app::tradingview_velocity_parity::ReplayReport],
) -> AggregateMetrics {
    let trades = reports
        .iter()
        .flat_map(|report| {
            report
                .trades
                .iter()
                .map(move |trade| (trade.exit_time_ms, report.symbol.as_str(), trade))
        })
        .collect::<Vec<_>>();
    let trade_count = trades.len();
    let wins = trades
        .iter()
        .filter(|(_, _, trade)| trade.net_pnl > 0.0)
        .count();
    let losses = trades
        .iter()
        .filter(|(_, _, trade)| trade.net_pnl < 0.0)
        .count();
    let net_pnl = trades.iter().map(|(_, _, trade)| trade.net_pnl).sum();
    let gross_profit = trades
        .iter()
        .map(|(_, _, trade)| trade.net_pnl.max(0.0))
        .sum();
    let gross_loss = trades
        .iter()
        .map(|(_, _, trade)| (-trade.net_pnl).max(0.0))
        .sum();
    let average_net_r = if trade_count > 0 {
        trades.iter().map(|(_, _, trade)| trade.net_r).sum::<f64>() / trade_count as f64
    } else {
        0.0
    };
    let (profit_factor, profit_factor_is_infinite) = if gross_loss > 0.0 {
        (Some(gross_profit / gross_loss), false)
    } else if gross_profit > 0.0 {
        (None, true)
    } else {
        (None, false)
    };
    let profitable_symbols = reports
        .iter()
        .filter(|report| report.metrics.net_pnl > 0.0)
        .count();
    let losing_symbols = reports
        .iter()
        .filter(|report| report.metrics.net_pnl < 0.0)
        .count();

    AggregateMetrics {
        symbols: reports.len(),
        trades: trade_count,
        wins,
        losses,
        net_pnl,
        gross_profit,
        gross_loss,
        profit_factor,
        profit_factor_is_infinite,
        win_rate_percent: if trade_count > 0 {
            wins as f64 / trade_count as f64 * 100.0
        } else {
            0.0
        },
        average_net_r,
        chronological_closed_equity_max_drawdown: chronological_closed_equity_drawdown(trades),
        max_single_symbol_intrabar_drawdown: reports
            .iter()
            .map(|report| report.metrics.max_drawdown)
            .fold(0.0_f64, f64::max),
        profitable_symbols,
        losing_symbols,
        flat_symbols: reports.len() - profitable_symbols - losing_symbols,
        open_positions_at_end: reports
            .iter()
            .filter(|report| report.open_position_at_end)
            .count(),
        pending_entries_at_end: reports
            .iter()
            .filter(|report| report.pending_entry_at_end)
            .count(),
    }
}

/// 按退出时刻和稳定币种顺序累计固定 1 单位收益，只计算已平仓权益回撤。
fn chronological_closed_equity_drawdown(mut trades: Vec<(i64, &str, &Trade)>) -> f64 {
    trades.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(right.1))
            .then_with(|| left.2.signal_time_ms.cmp(&right.2.signal_time_ms))
    });
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for (_, _, trade) in trades {
        equity += trade.net_pnl;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    max_drawdown
}

/// 将同方向且相隔不超过 60 分钟的信号粗略归并；它不是正式相关性事件指标。
fn time_direction_cluster_count(
    reports: &[rust_quant_cli::app::tradingview_velocity_parity::ReplayReport],
) -> usize {
    let mut signals = reports
        .iter()
        .flat_map(|report| {
            report
                .trades
                .iter()
                .map(|trade| (trade.signal_time_ms, trade.direction))
        })
        .collect::<Vec<_>>();
    signals.sort_by_key(|(timestamp_ms, direction)| {
        (*timestamp_ms, matches!(direction, Direction::Short))
    });
    let mut last_long = None;
    let mut last_short = None;
    let mut clusters = 0;
    for (timestamp_ms, direction) in signals {
        let last = match direction {
            Direction::Long => &mut last_long,
            Direction::Short => &mut last_short,
        };
        if last.is_none_or(|previous| timestamp_ms - previous > EFFECTIVE_EVENT_CLUSTER_MS) {
            clusters += 1;
        }
        *last = Some(timestamp_ms);
    }
    clusters
}

/// 按已平仓交易携带的全部信号家族计数；一笔多家族交易会贡献多个计数。
fn family_counts(
    reports: &[rust_quant_cli::app::tradingview_velocity_parity::ReplayReport],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for family in reports
        .iter()
        .flat_map(|report| report.trades.iter())
        .flat_map(|trade| trade.families.iter().copied())
    {
        *counts.entry(family_name(family).to_owned()).or_default() += 1;
    }
    counts
}

/// 返回稳定的报告键名，避免 Rust 调试格式变化破坏研究产物。
fn family_name(family: SignalFamily) -> &'static str {
    match family {
        SignalFamily::RsiBullishDivergence => "rsi_bullish_divergence",
        SignalFamily::RsiBearishDivergence => "rsi_bearish_divergence",
        SignalFamily::RsiOversoldPattern => "rsi_oversold_pattern",
        SignalFamily::RsiOverboughtPattern => "rsi_overbought_pattern",
        SignalFamily::EmaTrendLong => "ema_trend_long",
        SignalFamily::EmaTrendShort => "ema_trend_short",
        SignalFamily::ConfirmedRangeAcceptanceLong => "confirmed_range_acceptance_long",
        SignalFamily::LargeHorizontalRangeBreakLong => "large_horizontal_range_break_long",
        SignalFamily::StrictVisualConsolidationBreakLong => {
            "strict_visual_consolidation_break_long"
        }
        SignalFamily::StrictVisualConsolidationBreakShort => {
            "strict_visual_consolidation_break_short"
        }
        SignalFamily::LargeAscendingTriangleBreakLong => "large_ascending_triangle_break_long",
        SignalFamily::AnchorFalseBreakShort => "anchor_false_break_short",
        SignalFamily::AnchorUpthrustFailedAcceptanceShort => {
            "anchor_upthrust_failed_acceptance_short"
        }
        SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort => {
            "anchor_upthrust_failed_acceptance_right_side_short"
        }
        SignalFamily::TransitionLiquiditySweepShort => "transition_liquidity_sweep_short",
        SignalFamily::EmaCompressionExpansionLong => "ema_compression_expansion_long",
        SignalFamily::EmaCompressionExpansionShort => "ema_compression_expansion_short",
        SignalFamily::ThreeBarBullishEngulfingLong => "three_bar_bullish_engulfing_long",
        SignalFamily::EffortNoResultShort => "effort_no_result_short",
        SignalFamily::BollingerLowerReclaimLong => "bollinger_lower_reclaim_long",
        SignalFamily::Ema596ReclaimDepartureLong => "ema596_reclaim_departure_long",
        SignalFamily::RangeSqueezeBreakAcceptanceLong => "range_squeeze_break_acceptance_long",
        SignalFamily::RangeSqueezeBreakAcceptanceShort => "range_squeeze_break_acceptance_short",
        SignalFamily::RangeSqueezeRightSideTriggerLong => "range_squeeze_right_side_trigger_long",
        SignalFamily::RangeSqueezeRightSideTriggerShort => "range_squeeze_right_side_trigger_short",
        SignalFamily::RangeSqueezeRightSideTriggerAblationLong => {
            "range_squeeze_right_side_trigger_ablation_long"
        }
        SignalFamily::RangeSqueezeRightSideTriggerAblationShort => {
            "range_squeeze_right_side_trigger_ablation_short"
        }
        SignalFamily::SellClimaxBaseReclaimLong => "sell_climax_base_reclaim_long",
    }
}

#[cfg(test)]
#[path = "tradingview_velocity_top60/tests.rs"]
mod tests;
