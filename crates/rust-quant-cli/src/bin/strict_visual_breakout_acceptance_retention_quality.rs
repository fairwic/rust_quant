//! V23 横盘突破确认保留率质量诊断（只读、不改策略、不写库）。
//!
//! 本工具严格分两阶段运行：L1 只输出信号时可见的候选账本与覆盖；L1 通过后，
//! L2 才按预注册的 16 根结构路径标签统计质量率。唯一研究变量是确认收盘保留了
//! 多少原始突破越界幅度，固定阈值为 25% / 50% / 75% / 100%。

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use chrono::{TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_frozen_top60_from_quant_core, load_frozen_universe_spec,
    strict_visual_breakout_body_strength_for_variant, volume_take_profit_atr, Candle, Direction,
    ParityRuleVersion, StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal,
    StrictVisualLongEntryState,
};
use serde::Serialize;

#[path = "strict_visual_breakout_acceptance_retention_quality/quality_statistics.rs"]
mod quality_statistics;

use quality_statistics::{
    l2_threshold_report, quality_window_report, retention_distribution, threshold_coverage_report,
};

/// 独立结构质量标签固定观察 16 根 15m 完成棒，即 4 小时。
const QUALITY_HORIZON_BARS: usize = 16;
/// 发现窗与验证窗的冻结切分点：2026-03-01 00:00:00 Asia/Shanghai。
const VALIDATION_START_MS: i64 = 1_772_294_400_000;
/// 同方向、同一小时的跨币种候选归为同一市场事件。
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
/// 上海时区相对 UTC 的固定偏移。
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
/// 本轮唯一变量的全部预注册阈值；第一个是 V23 当前基线。
const RETENTION_THRESHOLDS: [f64; 4] = [0.25, 0.50, 0.75, 1.00];
/// 浮点边界容差只用于避免 0.25 的二进制表示误删合同内候选。
const RETENTION_EPSILON: f64 = 1e-12;

/// 运行阶段；L1 禁止读取候选后的 K 线结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ResearchPhase {
    L1,
    L2,
}

impl ResearchPhase {
    /// 从显式环境变量读取阶段，默认保持在 outcome-blind L1。
    fn from_env() -> Result<Self> {
        match std::env::var("STRICT_VISUAL_QUALITY_PHASE")
            .unwrap_or_else(|_| "l1".to_owned())
            .as_str()
        {
            "l1" => Ok(Self::L1),
            "l2" => Ok(Self::L2),
            other => bail!("STRICT_VISUAL_QUALITY_PHASE 只允许 l1 或 l2，实际为 {other}"),
        }
    }
}

/// 候选按确认完成时间落入发现窗或冻结验证窗。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WindowSplit {
    Discovery,
    Validation,
}

/// 不依赖当前交易止损或保本规则的突破结构结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum QualityOutcome {
    /// 先到冻结边界外一个完整横盘高度，且此前没有完成收盘回区。
    Quality,
    /// 到达量度目标前先有完成收盘重新进入原横盘。
    FailedCloseBack,
    /// 完整 16 根内既未到目标，也未收盘回区。
    Unresolved,
    /// 数据末端不足 16 根，禁止使用不完整路径评价。
    ForwardIncomplete,
    /// 缺失目标或边界等冻结证据；正常 V23 不应出现。
    InvalidEvidence,
}

/// 单个确认候选的信号时证据与可选未来结构标签。
#[derive(Debug, Clone, Serialize)]
struct CandidateLedgerRow {
    /// 冻结币池交易对。
    symbol: String,
    /// 多头或空头。
    direction: Direction,
    /// 发现窗或验证窗。
    split: WindowSplit,
    /// 横盘首根完成棒时间。
    range_start_time_ms: i64,
    /// 横盘首次被确认的完成棒时间，即 setup 已知时点。
    setup_time_ms: i64,
    /// 强突破来源棒完成时间。
    breakout_time_ms: i64,
    /// 首次合法回踩确认完成时间。
    signal_time_ms: i64,
    /// 便于人工核图的上海时间。
    signal_time_bjt: String,
    /// 同方向 60 分钟事件簇身份。
    event_cluster: String,
    /// 突破至确认相隔完成棒数量，只允许 1～5。
    acceptance_age_bars: usize,
    /// 冻结横盘长度。
    range_length_bars: usize,
    /// 冻结视觉上沿。
    range_upper: f64,
    /// 冻结视觉下沿。
    range_lower: f64,
    /// 冻结横盘高度。
    range_height: f64,
    /// 横盘高度相对突破来源 ATR。
    range_height_atr: f64,
    /// 横盘完成收盘容纳率；只作账本证据，不参与本轮筛选。
    containment_ratio: f64,
    /// 横盘方向效率；只作账本证据，不参与本轮筛选。
    direction_efficiency: f64,
    /// 横盘上下边界独立切换次数；只作账本证据，不参与本轮筛选。
    edge_transition_count: usize,
    /// 突破棒开盘价。
    breakout_open: f64,
    /// 突破棒最高价。
    breakout_high: f64,
    /// 突破棒最低价。
    breakout_low: f64,
    /// 突破棒收盘价。
    breakout_close: f64,
    /// 正确的实体 / 整根高低振幅。
    breakout_body_ratio: f64,
    /// 按方向计算的实体 / 开盘价。
    breakout_directional_move_ratio: f64,
    /// 突破收盘超出冻结边界的方向距离。
    breakout_excess: f64,
    /// 首次合法确认棒收盘价。
    confirmation_close: f64,
    /// 本轮唯一变量：确认仍保留的突破越界幅度比例。
    acceptance_retention_ratio: f64,
    /// 突破棒来源 ATR。
    source_atr: f64,
    /// V23 仅作诊断的过滤量比；不参与本轮筛选。
    source_volume_ratio: f64,
    /// 突破棒极值外一 tick 的冻结结构止损。
    frozen_breakout_extreme_stop: Option<f64>,
    /// 冻结一个横盘高度量度目标。
    frozen_measured_move_target: Option<f64>,
    /// 候选在 V23 原确认时点已具备入场资格；本研究不补造或延后信号。
    eligibility: &'static str,
    /// L2 才填充；L1 序列化时完全省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    quality_outcome: Option<QualityOutcome>,
    /// 首次目标或失败事件时间；未决与 L1 为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome_time_ms: Option<i64>,
    /// 同棒既触目标又收盘回区时为 true，最终仍保守记失败。
    #[serde(skip_serializing_if = "Option::is_none")]
    same_bar_ambiguous: Option<bool>,
}

/// 未来结构路径的内部判定结果。
#[derive(Debug, Clone, Copy)]
struct QualityPath {
    outcome: QualityOutcome,
    outcome_time_ms: Option<i64>,
    same_bar_ambiguous: bool,
}

/// 一个阈值在某个时间窗的 outcome-blind 覆盖。
#[derive(Debug, Clone, Serialize)]
struct CoverageStats {
    candidates: usize,
    long_candidates: usize,
    short_candidates: usize,
    symbols: usize,
    shanghai_months: usize,
    direction_hour_event_clusters: usize,
}

/// 预注册阈值的 L1 覆盖判定。
#[derive(Debug, Clone, Serialize)]
struct ThresholdCoverageReport {
    minimum_retention_ratio: f64,
    discovery: CoverageStats,
    validation: CoverageStats,
    validation_gate_passed: bool,
    gate_failures: Vec<String>,
}

/// 一个比例的 outcome-blind 分位摘要。
#[derive(Debug, Clone, Serialize)]
struct RetentionDistribution {
    count: usize,
    minimum: f64,
    p10: f64,
    p25: f64,
    median: f64,
    p75: f64,
    p90: f64,
    p95: f64,
    maximum: f64,
}

/// Wilson 二项比例近似 95% 区间。
#[derive(Debug, Clone, Serialize)]
struct ConfidenceInterval {
    lower_percent: f64,
    upper_percent: f64,
}

/// 一个阈值、时间窗和方向上的结构结果。
#[derive(Debug, Clone, Serialize)]
struct QualityStats {
    candidates: usize,
    forward_complete: usize,
    resolved: usize,
    quality: usize,
    failed_close_back: usize,
    unresolved: usize,
    forward_incomplete: usize,
    invalid_evidence: usize,
    same_bar_ambiguous_failures: usize,
    quality_rate_resolved_percent: f64,
    quality_rate_complete_percent: f64,
    quality_rate_resolved_wilson_95: ConfidenceInterval,
}

/// 同一窗口的全体、多头和空头镜像统计。
#[derive(Debug, Clone, Serialize)]
struct QualityWindowReport {
    all: QualityStats,
    long: QualityStats,
    short: QualityStats,
}

/// 按方向加 60 分钟事件簇配对后的候选相对基线质量率提升。
#[derive(Debug, Clone, Serialize)]
struct PairedClusterLift {
    paired_clusters: usize,
    mean_lift_percent_points: f64,
    standard_error_percent_points: f64,
    approximate_95_lower_percent_points: f64,
    approximate_95_upper_percent_points: f64,
}

/// 单个阈值的 L2 固定标签报告与联合门禁。
#[derive(Debug, Clone, Serialize)]
struct L2ThresholdReport {
    minimum_retention_ratio: f64,
    coverage: ThresholdCoverageReport,
    discovery: QualityWindowReport,
    validation: QualityWindowReport,
    discovery_lift_vs_baseline_percent_points: f64,
    validation_lift_vs_baseline_percent_points: f64,
    validation_long_lift_vs_baseline_percent_points: f64,
    validation_short_lift_vs_baseline_percent_points: f64,
    validation_paired_event_cluster_lift: PairedClusterLift,
    joint_gate_passed: bool,
    joint_gate_failures: Vec<String>,
}

/// 冻结数据身份与当前可用成员覆盖。
#[derive(Debug, Serialize)]
struct DatasetReport {
    universe_version: String,
    manifest_sha256: String,
    expected_symbols: usize,
    included_symbols: usize,
    excluded_symbols: usize,
    included_symbol_names: Vec<String>,
    excluded_symbol_names: Vec<String>,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    validation_start_ms: i64,
}

/// L1 只读信号账本输出，不包含未来标签。
#[derive(Debug, Serialize)]
struct L1Report {
    schema_version: &'static str,
    generated_on: &'static str,
    phase: ResearchPhase,
    baseline_variant: String,
    single_variable: &'static str,
    dataset: DatasetReport,
    retention_distribution: RetentionDistribution,
    thresholds: Vec<ThresholdCoverageReport>,
    candidate_ledger: Vec<CandidateLedgerRow>,
}

/// L2 固定标签诊断输出；仍然不是完整策略晋级回测。
#[derive(Debug, Serialize)]
struct L2Report {
    schema_version: &'static str,
    generated_on: &'static str,
    phase: ResearchPhase,
    research_status: &'static str,
    promotion_status: &'static str,
    baseline_variant: String,
    single_variable: &'static str,
    quality_label: &'static str,
    dataset: DatasetReport,
    retention_distribution: RetentionDistribution,
    thresholds: Vec<L2ThresholdReport>,
    selected_threshold: Option<f64>,
    decision: String,
    candidate_ledger: Vec<CandidateLedgerRow>,
}

/// 计算确认保留率；多空完全镜像，非正分母返回 None。
fn acceptance_retention_ratio(
    direction: Direction,
    boundary: f64,
    confirmation_close: f64,
    breakout_excess: f64,
) -> Option<f64> {
    if breakout_excess <= 0.0 {
        return None;
    }
    let retained = match direction {
        Direction::Long => confirmation_close - boundary,
        Direction::Short => boundary - confirmation_close,
    };
    Some(retained / breakout_excess)
}

/// 在冻结 16 根窗口内先判完成收盘回区，再判量度目标；同棒冲突保守归失败。
fn classify_quality_path(
    candles: &[Candle],
    signal_index: usize,
    direction: Direction,
    boundary: f64,
    target: Option<f64>,
) -> QualityPath {
    let Some(target) = target else {
        return QualityPath {
            outcome: QualityOutcome::InvalidEvidence,
            outcome_time_ms: None,
            same_bar_ambiguous: false,
        };
    };
    if candles.len().saturating_sub(signal_index + 1) < QUALITY_HORIZON_BARS {
        return QualityPath {
            outcome: QualityOutcome::ForwardIncomplete,
            outcome_time_ms: None,
            same_bar_ambiguous: false,
        };
    }

    for candle in candles
        .iter()
        .skip(signal_index + 1)
        .take(QUALITY_HORIZON_BARS)
    {
        let (closed_back, hit_target) = match direction {
            Direction::Long => (candle.close <= boundary, candle.high >= target),
            Direction::Short => (candle.close >= boundary, candle.low <= target),
        };
        if closed_back {
            return QualityPath {
                outcome: QualityOutcome::FailedCloseBack,
                outcome_time_ms: Some(candle.timestamp_ms),
                same_bar_ambiguous: hit_target,
            };
        }
        if hit_target {
            return QualityPath {
                outcome: QualityOutcome::Quality,
                outcome_time_ms: Some(candle.timestamp_ms),
                same_bar_ambiguous: false,
            };
        }
    }
    QualityPath {
        outcome: QualityOutcome::Unresolved,
        outcome_time_ms: None,
        same_bar_ambiguous: false,
    }
}

/// 对一个交易对逐棒回放 V23；L1 只构造候选，L2 才调用未来路径分类。
fn collect_symbol_candidates(
    symbol: &str,
    candles: &[Candle],
    tick_size: f64,
    window_start_ms: i64,
    phase: ResearchPhase,
) -> Vec<CandidateLedgerRow> {
    let rule_version = ParityRuleVersion::CandidateV20;
    let variant = StrictVisualBreakoutResearchVariant::V23UnifiedRetestNoShortGates;
    let indicators = compute_indicators(candles, rule_version);
    let mut state = StrictVisualLongEntryState::default();
    let mut rows = Vec::new();

    for index in 0..candles.len() {
        let Some(point) = indicators.get(index) else {
            continue;
        };
        let take_profit_atr = point.filtered_volume_ratio.and_then(volume_take_profit_atr);
        let event = state.update(
            candles,
            index,
            tick_size,
            point.atr14,
            point.volume_event,
            point.filtered_volume_ratio,
            take_profit_atr,
            variant,
        );
        let Some(signal) = event.and_then(|event| event.entry_signal()) else {
            continue;
        };
        let signal_candle = candles[signal.signal_index];
        if signal_candle.timestamp_ms < window_start_ms {
            continue;
        }
        let Some(row) = candidate_row(symbol, candles, signal, variant, phase) else {
            continue;
        };
        rows.push(row);
    }
    rows
}

/// 把一个 V23 确认来源转换为可审计账本行；所有门禁字段均来自确认时已完成数据。
fn candidate_row(
    symbol: &str,
    candles: &[Candle],
    signal: StrictVisualBreakoutSignal,
    variant: StrictVisualBreakoutResearchVariant,
    phase: ResearchPhase,
) -> Option<CandidateLedgerRow> {
    let breakout = *candles.get(signal.breakout_index)?;
    let confirmation = *candles.get(signal.signal_index)?;
    let range_height = signal.range.upper - signal.range.lower;
    if range_height <= 0.0 || signal.source_atr <= 0.0 {
        return None;
    }
    let boundary = match signal.direction {
        Direction::Long => signal.range.upper,
        Direction::Short => signal.range.lower,
    };
    let retention = acceptance_retention_ratio(
        signal.direction,
        boundary,
        confirmation.close,
        signal.breakout_excess,
    )?;
    let strength =
        strict_visual_breakout_body_strength_for_variant(breakout, signal.direction, variant);
    let signal_time_ms = confirmation.timestamp_ms;
    let split = if signal_time_ms < VALIDATION_START_MS {
        WindowSplit::Discovery
    } else {
        WindowSplit::Validation
    };
    let event_cluster = format!(
        "{}:{}",
        direction_slug(signal.direction),
        signal_time_ms.div_euclid(EVENT_CLUSTER_MS) * EVENT_CLUSTER_MS
    );
    let quality_path = (phase == ResearchPhase::L2).then(|| {
        classify_quality_path(
            candles,
            signal.signal_index,
            signal.direction,
            boundary,
            signal.measured_move_target_price,
        )
    });

    Some(CandidateLedgerRow {
        symbol: symbol.to_owned(),
        direction: signal.direction,
        split,
        range_start_time_ms: candles.get(signal.range.start_index)?.timestamp_ms,
        setup_time_ms: candles
            .get(signal.range.first_confirmation_index)?
            .timestamp_ms,
        breakout_time_ms: breakout.timestamp_ms,
        signal_time_ms,
        signal_time_bjt: shanghai_time(signal_time_ms),
        event_cluster,
        acceptance_age_bars: signal.signal_index.saturating_sub(signal.breakout_index),
        range_length_bars: signal.range.length_bars,
        range_upper: signal.range.upper,
        range_lower: signal.range.lower,
        range_height,
        range_height_atr: range_height / signal.source_atr,
        containment_ratio: signal.range.containment_ratio,
        direction_efficiency: signal.range.direction_efficiency,
        edge_transition_count: signal.range.edge_transition_count,
        breakout_open: breakout.open,
        breakout_high: breakout.high,
        breakout_low: breakout.low,
        breakout_close: breakout.close,
        breakout_body_ratio: strength.body_ratio,
        breakout_directional_move_ratio: strength.directional_move_ratio,
        breakout_excess: signal.breakout_excess,
        confirmation_close: confirmation.close,
        acceptance_retention_ratio: retention,
        source_atr: signal.source_atr,
        source_volume_ratio: signal.source_volume_ratio,
        frozen_breakout_extreme_stop: signal.breakout_candle_extreme_stop_price,
        frozen_measured_move_target: signal.measured_move_target_price,
        eligibility: "v23_original_confirmation_candidate",
        quality_outcome: quality_path.map(|path| path.outcome),
        outcome_time_ms: quality_path.and_then(|path| path.outcome_time_ms),
        same_bar_ambiguous: quality_path.map(|path| path.same_bar_ambiguous),
    })
}

/// 方向用于稳定事件簇身份，不依赖 Debug 格式。
const fn direction_slug(direction: Direction) -> &'static str {
    match direction {
        Direction::Long => "long",
        Direction::Short => "short",
    }
}

/// 毫秒时间戳转无歧义的上海时间字符串。
fn shanghai_time(timestamp_ms: i64) -> String {
    Utc.timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "invalid".to_owned())
}

/// 构造数据身份，明确列出本地缺失成员，不自动回补 K 线。
fn dataset_report(
    dataset: &rust_quant_cli::app::tradingview_velocity_parity::FrozenUniverseData,
) -> Result<DatasetReport> {
    let spec = load_frozen_universe_spec()?;
    let included: BTreeSet<String> = dataset
        .symbols
        .iter()
        .map(|symbol| symbol.symbol.clone())
        .collect();
    let excluded: Vec<String> = spec
        .symbols
        .iter()
        .filter(|symbol| !included.contains(*symbol))
        .cloned()
        .collect();
    Ok(DatasetReport {
        universe_version: dataset.universe_version.clone(),
        manifest_sha256: dataset.manifest_sha256.clone(),
        expected_symbols: spec.symbols.len(),
        included_symbols: dataset.symbols.len(),
        excluded_symbols: excluded.len(),
        included_symbol_names: dataset
            .symbols
            .iter()
            .map(|symbol| symbol.symbol.clone())
            .collect(),
        excluded_symbol_names: excluded,
        evaluation_start_ms: dataset.window_start_ms,
        evaluation_end_ms: dataset.window_end_ms,
        validation_start_ms: VALIDATION_START_MS,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let phase = ResearchPhase::from_env()?;
    let variant = StrictVisualBreakoutResearchVariant::V23UnifiedRetestNoShortGates;
    let dataset = load_frozen_top60_from_quant_core()
        .await
        .context("加载冻结 Top60 quant_core 数据失败")?;
    let dataset_identity = dataset_report(&dataset)?;
    let mut rows = Vec::new();
    for symbol in &dataset.symbols {
        rows.extend(collect_symbol_candidates(
            &symbol.symbol,
            &symbol.candles,
            symbol.tick_size,
            dataset.window_start_ms,
            phase,
        ));
    }
    rows.sort_by(|left, right| {
        left.signal_time_ms
            .cmp(&right.signal_time_ms)
            .then_with(|| left.symbol.cmp(&right.symbol))
            .then_with(|| direction_slug(left.direction).cmp(direction_slug(right.direction)))
    });

    let distribution = retention_distribution(&rows);
    match phase {
        ResearchPhase::L1 => {
            let thresholds = RETENTION_THRESHOLDS
                .into_iter()
                .map(|threshold| threshold_coverage_report(&rows, threshold))
                .collect();
            let report = L1Report {
                schema_version: "strict_visual_v23_acceptance_retention_quality_l1_v1",
                generated_on: "2026-08-08",
                phase,
                baseline_variant: variant.slug().to_owned(),
                single_variable: "acceptance_retention_ratio",
                dataset: dataset_identity,
                retention_distribution: distribution,
                thresholds,
                candidate_ledger: rows,
            };
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
        }
        ResearchPhase::L2 => {
            let baseline_discovery =
                quality_window_report(&rows, RETENTION_THRESHOLDS[0], WindowSplit::Discovery);
            let baseline_validation =
                quality_window_report(&rows, RETENTION_THRESHOLDS[0], WindowSplit::Validation);
            let thresholds: Vec<L2ThresholdReport> = RETENTION_THRESHOLDS
                .into_iter()
                .map(|threshold| {
                    l2_threshold_report(&rows, threshold, &baseline_discovery, &baseline_validation)
                })
                .collect();
            let selected_threshold = thresholds
                .iter()
                .skip(1)
                .find(|report| report.joint_gate_passed)
                .map(|report| report.minimum_retention_ratio);
            let decision = selected_threshold.map_or_else(
                || {
                    "没有预注册阈值通过联合门禁；确认保留率不能稳定识别高质量突破，停止追阈值。"
                        .to_owned()
                },
                |threshold| {
                    format!(
                        "最先通过联合门禁的是 retention>={threshold:.2}；只允许进入下一轮冻结成本回放，不修改策略。"
                    )
                },
            );
            let report = L2Report {
                schema_version: "strict_visual_v23_acceptance_retention_quality_l2_v1",
                generated_on: "2026-08-08",
                phase,
                research_status: if selected_threshold.is_some() {
                    "structural_quality_candidate_found"
                } else {
                    "stopped_no_stable_discriminator"
                },
                promotion_status: "research_only_not_promoted",
                baseline_variant: variant.slug().to_owned(),
                single_variable: "acceptance_retention_ratio",
                quality_label:
                    "within_16_bars_measured_move_before_completed_close_back_inside_range",
                dataset: dataset_identity,
                retention_distribution: distribution,
                thresholds,
                selected_threshold,
                decision,
                candidate_ledger: rows,
            };
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造只关心结构路径的最小合法 K 线。
    fn candle(timestamp_ms: i64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms,
            open: close,
            high,
            low,
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn retention_ratio_is_symmetric_for_long_and_short() {
        let long = acceptance_retention_ratio(Direction::Long, 100.0, 101.0, 2.0);
        let short = acceptance_retention_ratio(Direction::Short, 100.0, 99.0, 2.0);
        assert_eq!(long, Some(0.5));
        assert_eq!(short, Some(0.5));
    }

    #[test]
    fn quality_path_requires_target_before_completed_close_back() {
        let mut candles = vec![candle(0, 101.0, 99.0, 100.5)];
        candles.extend((1..=QUALITY_HORIZON_BARS).map(|index| {
            if index == 2 {
                candle(index as i64, 111.0, 100.5, 108.0)
            } else {
                candle(index as i64, 109.0, 100.5, 105.0)
            }
        }));
        let result = classify_quality_path(&candles, 0, Direction::Long, 100.0, Some(110.0));
        assert_eq!(result.outcome, QualityOutcome::Quality);
        assert_eq!(result.outcome_time_ms, Some(2));
    }

    #[test]
    fn same_bar_target_and_close_back_is_conservative_failure() {
        let mut candles = vec![candle(0, 101.0, 99.0, 100.5)];
        candles.extend((1..=QUALITY_HORIZON_BARS).map(|index| {
            if index == 1 {
                candle(index as i64, 111.0, 99.0, 99.5)
            } else {
                candle(index as i64, 109.0, 100.5, 105.0)
            }
        }));
        let result = classify_quality_path(&candles, 0, Direction::Long, 100.0, Some(110.0));
        assert_eq!(result.outcome, QualityOutcome::FailedCloseBack);
        assert!(result.same_bar_ambiguous);
    }

    #[test]
    fn incomplete_forward_window_is_not_labeled() {
        let candles = vec![
            candle(0, 101.0, 99.0, 100.5),
            candle(1, 111.0, 100.5, 108.0),
        ];
        let result = classify_quality_path(&candles, 0, Direction::Long, 100.0, Some(110.0));
        assert_eq!(result.outcome, QualityOutcome::ForwardIncomplete);
    }
}
