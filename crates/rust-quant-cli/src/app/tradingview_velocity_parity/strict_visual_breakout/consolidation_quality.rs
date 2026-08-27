use super::super::indicators::nearest_rank;
use super::nested_child::StrictVisualNestedRangeScope;
use super::{Candle, StrictVisualBreakoutResearchVariant, StrictVisualRangeEvidence};
use serde::Serialize;

pub const STRICT_VISUAL_CANDIDATE_LENGTHS: [usize; 8] = [8, 12, 16, 24, 32, 48, 64, 96];
const MAXIMUM_WIDTH_RATIO: f64 = 0.03;
/// V13 真实极差门禁与冻结 Pine 保持一致，避免百分位边界掩盖极端影线。
const MAXIMUM_RAW_TO_BAND_RATIO: f64 = 1.5;
const MINIMUM_CONTAINMENT_RATIO: f64 = 0.80;
const MAXIMUM_DIRECTION_EFFICIENCY: f64 = 0.35;
const MINIMUM_EDGE_TRANSITIONS: usize = 3;
const MAXIMUM_UPPER_DRIFT: f64 = 0.25;
const MAXIMUM_LOWER_DRIFT: f64 = 0.35;
const MINIMUM_TOUCH_GAP: usize = 2;
const TOUCH_BAND_HEIGHT_RATIO: f64 = 0.10;
const TRANSITION_BAND_HEIGHT_RATIO: f64 = 0.20;

/// V31 只接受单根实体严格小于冻结横盘高度一半的区间。
///
/// 该阈值表达“价格接受不能由一根 K 线完成大半个区间穿越”，不是根据交易结果拟合的参数。
pub const STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT: f64 = 0.50;

/// 父区间纵向价格占用；仅使用候选完成时已经存在的实体中心。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualParentVerticalOccupancy {
    pub upper_half_bars: usize,
    pub lower_half_bars: usize,
    pub dominant_half_ratio: f64,
}

/// L1 实体包络边界的全部信号时可见诊断，不包含任何成交后字段。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualBodyWickEnvelope {
    /// High/Low 排序后每侧允许留在视觉区间外的极端影线数量。
    pub allowed_sweep_wicks_per_side: usize,
    /// 忽略极端扫高后的稳健影线上沿。
    pub wick_upper: f64,
    /// 忽略极端扫低后的稳健影线下沿。
    pub wick_lower: f64,
    /// 窗口内全部 K 线实体上端的最大值。
    pub body_upper: f64,
    /// 窗口内全部 K 线实体下端的最小值。
    pub body_lower: f64,
    /// 最终视觉上沿，保证包含全部实体。
    pub upper: f64,
    /// 最终视觉下沿，保证包含全部实体。
    pub lower: f64,
    /// 最终视觉上沿之外仍保留的扫高影线数量。
    pub upper_wicks_outside: usize,
    /// 最终视觉下沿之外仍保留的扫低影线数量。
    pub lower_wicks_outside: usize,
}

/// L1 边界候选的信号时质量门禁审计；只解释为何接受或拒绝，不含任何成交后结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StrictVisualCandidateQualityAudit {
    pub width_ratio: f64,
    pub raw_to_band_ratio: f64,
    pub maximum_internal_body_ratio: f64,
    pub upper_touch_groups: usize,
    pub lower_touch_groups: usize,
    pub containment_ratio: f64,
    pub upper_drift_ratio: f64,
    pub lower_drift_ratio: f64,
    pub direction_efficiency: f64,
    pub edge_transition_count: usize,
    pub rejection_reasons: Vec<&'static str>,
    pub passed: bool,
}

/// 同一已确认窗口的双层边界：交易核心保持 V31，视觉父区间只提供外壳。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualDualBoundaryEvidence {
    pub trade_core: StrictVisualRangeEvidence,
    pub visual_parent: StrictVisualBodyWickEnvelope,
}

/// 以“100% 实体包络 + 约 90% 影线端点覆盖”形成候选边界。
///
/// 小样本额外保证每侧至少允许一根极端扫单影线留在边界外：8 根窗口因此允许
/// 1 根（实际覆盖 87.5%），24 根允许 2 根（覆盖 91.7%）。
pub fn strict_visual_body_wick_envelope(
    history: &[Candle],
) -> Option<StrictVisualBodyWickEnvelope> {
    if history.len() < 2 || history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }
    let allowed_sweep_wicks_per_side = (history.len() / 10).max(1).min(history.len() - 1);
    let mut highs = history.iter().map(|candle| candle.high).collect::<Vec<_>>();
    let mut lows = history.iter().map(|candle| candle.low).collect::<Vec<_>>();
    highs.sort_by(f64::total_cmp);
    lows.sort_by(f64::total_cmp);
    let wick_upper = highs[history.len() - allowed_sweep_wicks_per_side - 1];
    let wick_lower = lows[allowed_sweep_wicks_per_side];
    let body_upper = history
        .iter()
        .map(|candle| candle.open.max(candle.close))
        .fold(f64::MIN, f64::max);
    let body_lower = history
        .iter()
        .map(|candle| candle.open.min(candle.close))
        .fold(f64::MAX, f64::min);
    let upper = wick_upper.max(body_upper);
    let lower = wick_lower.min(body_lower);
    Some(StrictVisualBodyWickEnvelope {
        allowed_sweep_wicks_per_side,
        wick_upper,
        wick_lower,
        body_upper,
        body_lower,
        upper,
        lower,
        upper_wicks_outside: history.iter().filter(|candle| candle.high > upper).count(),
        lower_wicks_outside: history.iter().filter(|candle| candle.low < lower).count(),
    })
}

/// 只使用核心边界最后确认时已经完成的窗口，冻结包含核心的视觉父区间。
pub(super) fn strict_visual_parent_for_range(
    candles: &[Candle],
    range: StrictVisualRangeEvidence,
) -> Option<StrictVisualBodyWickEnvelope> {
    let history = candles.get(range.start_index..=range.boundary_confirmation_index)?;
    let mut parent = strict_visual_body_wick_envelope(history)?;
    parent.upper = parent.upper.max(range.upper);
    parent.lower = parent.lower.min(range.lower);
    parent.upper_wicks_outside = history
        .iter()
        .filter(|candle| candle.high > parent.upper)
        .count();
    parent.lower_wicks_outside = history
        .iter()
        .filter(|candle| candle.low < parent.lower)
        .count();
    Some(parent)
}

/// 计算视觉父区间上下半侧各自承载的实体中心数量。
pub fn strict_visual_parent_vertical_occupancy(
    candles: &[Candle],
    range: StrictVisualRangeEvidence,
) -> Option<StrictVisualParentVerticalOccupancy> {
    let history = candles.get(range.start_index..=range.boundary_confirmation_index)?;
    let parent = strict_visual_parent_for_range(candles, range)?;
    let middle = (parent.upper + parent.lower) * 0.5;
    let upper_half_bars = history
        .iter()
        .filter(|candle| (candle.open + candle.close) * 0.5 >= middle)
        .count();
    let lower_half_bars = history.len() - upper_half_bars;
    let dominant_half_ratio = upper_half_bars.max(lower_half_bars) as f64 / history.len() as f64;
    Some(StrictVisualParentVerticalOccupancy {
        upper_half_bars,
        lower_half_bars,
        dominant_half_ratio,
    })
}

pub(super) fn can_upgrade_parent(
    candles: &[Candle],
    active: StrictVisualRangeEvidence,
    candidate: StrictVisualRangeEvidence,
    variant: StrictVisualBreakoutResearchVariant,
) -> bool {
    candidate.upper >= active.lower
        && candidate.lower <= active.upper
        && candidate.start_index < active.start_index
        && variant
            .parent_upgrade_max_dominant_half_ratio()
            .is_none_or(|maximum| {
                strict_visual_parent_vertical_occupancy(candles, candidate)
                    .is_some_and(|occupancy| occupancy.dominant_half_ratio <= maximum)
            })
}

/// 当前完成棒是否足以重置仍在区间内的 V31 活动横盘。
pub(super) fn internal_impulse_ratio(
    candle: Candle,
    range: StrictVisualRangeEvidence,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<f64> {
    internal_impulse_ratio_for_bounds(
        candle,
        range.upper,
        range.lower,
        range.upper - range.lower,
        variant,
    )
}

fn internal_impulse_ratio_for_bounds(
    candle: Candle,
    upper: f64,
    lower: f64,
    guard_height: f64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<f64> {
    if guard_height <= 0.0 {
        return None;
    }
    let ratio = (candle.close - candle.open).abs() / guard_height;
    let fully_contained = candle.high <= upper && candle.low >= lower;
    (ratio >= STRICT_VISUAL_MAX_INTERNAL_BODY_TO_RANGE_HEIGHT
        && (!variant.exempts_fully_contained_internal_body() || !fully_contained))
        .then_some(ratio)
}

/// 当前完成棒只选择最长有效父区间，保持冻结视觉指标的离散扫描顺序。
pub(super) fn longest_candidate(
    candles: &[Candle],
    index: usize,
    tick_size: f64,
    minimum_start_index: Option<usize>,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<StrictVisualRangeEvidence> {
    let mut selected = None;
    for length in STRICT_VISUAL_CANDIDATE_LENGTHS {
        if length <= index + 1 {
            if let Some(candidate) = evaluate_candidate(candles, index, length, tick_size, variant)
                .filter(|candidate| {
                    minimum_start_index.is_none_or(|minimum| candidate.start_index >= minimum)
                })
            {
                selected = Some(candidate);
            }
        }
    }
    selected
}

/// 只选择严格晚于父确认、完整位于冻结父视觉外壳内的最长 V31 子候选。
pub(super) fn longest_nested_candidate(
    candles: &[Candle],
    index: usize,
    tick_size: f64,
    minimum_start_index: Option<usize>,
    variant: StrictVisualBreakoutResearchVariant,
    scope: StrictVisualNestedRangeScope,
) -> Option<StrictVisualRangeEvidence> {
    if index <= scope.parent.first_confirmation_index {
        return None;
    }
    let mut selected = None;
    for length in STRICT_VISUAL_CANDIDATE_LENGTHS {
        if length > index + 1 {
            continue;
        }
        let Some(candidate) = evaluate_candidate(candles, index, length, tick_size, variant) else {
            continue;
        };
        let causal = candidate.start_index > scope.parent.start_index
            && candidate.first_confirmation_index > scope.parent.first_confirmation_index
            && minimum_start_index.is_none_or(|minimum| candidate.start_index >= minimum);
        let contained =
            candidate.upper <= scope.visual_upper && candidate.lower >= scope.visual_lower;
        let distinct = (candidate.upper - scope.parent.upper).abs() >= tick_size
            || (candidate.lower - scope.parent.lower).abs() >= tick_size;
        if causal && contained && distinct {
            selected = Some(candidate);
        }
    }
    selected
}

/// 固定长度同时验证边界、往返、路径与漂移；V31 额外排除内部实体冲击。
fn evaluate_candidate(
    candles: &[Candle],
    index: usize,
    length: usize,
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<StrictVisualRangeEvidence> {
    let start = (index + 1).checked_sub(length)?;
    let history = candles.get(start..=index)?;
    if history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }
    let highs: Vec<f64> = history.iter().map(|candle| candle.high).collect();
    let lows: Vec<f64> = history.iter().map(|candle| candle.low).collect();
    let (upper_rank, lower_rank) = variant.boundary_percentile_ranks();
    let upper = nearest_rank(&highs, upper_rank)?;
    let lower = nearest_rank(&lows, lower_rank)?;
    evaluate_candidate_with_boundaries(
        history,
        start,
        index,
        tick_size,
        variant,
        upper,
        lower,
        upper - lower,
    )
}

/// L1 只读比较入口：按冻结变体原百分位口径评价一个固定窗口。
pub fn evaluate_strict_visual_percentile_candidate_for_research(
    candles: &[Candle],
    index: usize,
    length: usize,
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<StrictVisualRangeEvidence> {
    evaluate_candidate(candles, index, length, tick_size, variant)
}

/// L1 只读双层合同：候选身份完全由原 V31 核心决定，视觉父区间不得参与交易门禁。
pub fn evaluate_strict_visual_dual_boundary_candidate_for_research(
    candles: &[Candle],
    index: usize,
    length: usize,
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<StrictVisualDualBoundaryEvidence> {
    let trade_core = evaluate_candidate(candles, index, length, tick_size, variant)?;
    let visual_parent = strict_visual_parent_for_range(candles, trade_core)?;
    Some(StrictVisualDualBoundaryEvidence {
        trade_core,
        visual_parent,
    })
}

/// L1 只读比较入口：只替换边界估计器，其他横盘质量门保持冻结。
pub fn evaluate_strict_visual_body_wick_candidate_for_research(
    candles: &[Candle],
    index: usize,
    length: usize,
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<(StrictVisualRangeEvidence, StrictVisualBodyWickEnvelope)> {
    let start = (index + 1).checked_sub(length)?;
    let history = candles.get(start..=index)?;
    let envelope = strict_visual_body_wick_envelope(history)?;
    let base_height = envelope.wick_upper - envelope.wick_lower;
    let evidence = evaluate_candidate_with_boundaries(
        history,
        start,
        index,
        tick_size,
        variant,
        envelope.upper,
        envelope.lower,
        base_height,
    )?;
    Some((evidence, envelope))
}

/// L1 只读诊断入口：即使候选被旧质量门拒绝，也返回各门禁的信号时证据。
pub fn audit_strict_visual_body_wick_candidate_for_research(
    candles: &[Candle],
    index: usize,
    length: usize,
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
) -> Option<(
    StrictVisualBodyWickEnvelope,
    StrictVisualCandidateQualityAudit,
)> {
    let start = (index + 1).checked_sub(length)?;
    let history = candles.get(start..=index)?;
    let envelope = strict_visual_body_wick_envelope(history)?;
    let audit = audit_candidate_quality(
        history,
        tick_size,
        variant,
        envelope.upper,
        envelope.lower,
        envelope.wick_upper - envelope.wick_lower,
    )?;
    Some((envelope, audit))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_candidate_with_boundaries(
    history: &[Candle],
    start: usize,
    index: usize,
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
    upper: f64,
    lower: f64,
    internal_impulse_guard_height: f64,
) -> Option<StrictVisualRangeEvidence> {
    let length = history.len();
    let audit = audit_candidate_quality(
        history,
        tick_size,
        variant,
        upper,
        lower,
        internal_impulse_guard_height,
    )?;
    audit.passed.then_some(StrictVisualRangeEvidence {
        start_index: start,
        first_confirmation_index: index,
        boundary_confirmation_index: index,
        length_bars: length,
        upper,
        lower,
        containment_ratio: audit.containment_ratio,
        direction_efficiency: audit.direction_efficiency,
        edge_transition_count: audit.edge_transition_count,
        upper_touch_groups: audit.upper_touch_groups,
        lower_touch_groups: audit.lower_touch_groups,
        upper_drift_ratio: audit.upper_drift_ratio,
        lower_drift_ratio: audit.lower_drift_ratio,
    })
}

fn audit_candidate_quality(
    history: &[Candle],
    tick_size: f64,
    variant: StrictVisualBreakoutResearchVariant,
    upper: f64,
    lower: f64,
    internal_impulse_guard_height: f64,
) -> Option<StrictVisualCandidateQualityAudit> {
    let length = history.len();
    let highs = history.iter().map(|candle| candle.high).collect::<Vec<_>>();
    let lows = history.iter().map(|candle| candle.low).collect::<Vec<_>>();
    let height = upper - lower;
    let middle = (upper + lower) * 0.5;
    if height <= 0.0 || internal_impulse_guard_height <= 0.0 || middle <= 0.0 {
        return None;
    }
    let width_ratio = height / middle;
    let raw_high = highs.iter().copied().fold(f64::MIN, f64::max);
    let raw_low = lows.iter().copied().fold(f64::MAX, f64::min);
    let raw_to_band_ratio = (raw_high - raw_low) / height;
    let maximum_internal_body_ratio = history
        .iter()
        .map(|candle| (candle.close - candle.open).abs() / internal_impulse_guard_height)
        .fold(0.0, f64::max);
    let has_internal_body_impulse = history.iter().any(|candle| {
        internal_impulse_ratio_for_bounds(
            *candle,
            upper,
            lower,
            internal_impulse_guard_height,
            variant,
        )
        .is_some()
    });
    let touch_band = (tick_size * 2.0).max(height * TOUCH_BAND_HEIGHT_RATIO);
    let upper_touch_groups = touch_groups(history, |candle| candle.high >= upper - touch_band);
    let lower_touch_groups = touch_groups(history, |candle| candle.low <= lower + touch_band);
    let contained = history
        .iter()
        .filter(|candle| candle.close <= upper + touch_band && candle.close >= lower - touch_band)
        .count();
    let containment_ratio = contained as f64 / length as f64;
    let half = length / 2;
    let upper_drift_ratio =
        (nearest_rank(&highs[half..], 80.0)? - nearest_rank(&highs[..half], 80.0)?).abs() / height;
    let lower_drift_ratio =
        (nearest_rank(&lows[half..], 20.0)? - nearest_rank(&lows[..half], 20.0)?).abs() / height;
    let direction_efficiency = close_direction_efficiency(history)?;
    let edge_transition_count = edge_transition_count(history, upper, lower, tick_size);
    let mut rejection_reasons = Vec::new();
    if width_ratio > MAXIMUM_WIDTH_RATIO {
        rejection_reasons.push("width_ratio");
    }
    if variant.uses_raw_range_guard() && raw_to_band_ratio > MAXIMUM_RAW_TO_BAND_RATIO {
        rejection_reasons.push("raw_to_band_ratio");
    }
    if variant.uses_internal_impulse_body_guard() && has_internal_body_impulse {
        rejection_reasons.push("internal_body_impulse");
    }
    if upper_touch_groups < 2 {
        rejection_reasons.push("upper_touch_groups");
    }
    if lower_touch_groups < 2 {
        rejection_reasons.push("lower_touch_groups");
    }
    if containment_ratio < MINIMUM_CONTAINMENT_RATIO {
        rejection_reasons.push("containment_ratio");
    }
    if upper_drift_ratio > MAXIMUM_UPPER_DRIFT {
        rejection_reasons.push("upper_drift_ratio");
    }
    if lower_drift_ratio > MAXIMUM_LOWER_DRIFT {
        rejection_reasons.push("lower_drift_ratio");
    }
    if direction_efficiency > MAXIMUM_DIRECTION_EFFICIENCY {
        rejection_reasons.push("direction_efficiency");
    }
    if edge_transition_count < MINIMUM_EDGE_TRANSITIONS {
        rejection_reasons.push("edge_transition_count");
    }
    let passed = rejection_reasons.is_empty();
    Some(StrictVisualCandidateQualityAudit {
        width_ratio,
        raw_to_band_ratio,
        maximum_internal_body_ratio,
        upper_touch_groups,
        lower_touch_groups,
        containment_ratio,
        upper_drift_ratio,
        lower_drift_ratio,
        direction_efficiency,
        edge_transition_count,
        rejection_reasons,
        passed,
    })
}

fn touch_groups(history: &[Candle], touches: impl Fn(&Candle) -> bool) -> usize {
    let mut groups = 0;
    let mut last_touch = None;
    for (chronological_index, candle) in history.iter().enumerate() {
        if touches(candle) {
            if last_touch.is_none_or(|last| chronological_index - last >= MINIMUM_TOUCH_GAP) {
                groups += 1;
            }
            last_touch = Some(chronological_index);
        }
    }
    groups
}

fn close_direction_efficiency(history: &[Candle]) -> Option<f64> {
    let first = history.first()?.close;
    let last = history.last()?.close;
    let path = history
        .windows(2)
        .map(|pair| (pair[1].close - pair[0].close).abs())
        .sum::<f64>();
    Some(if path <= f64::EPSILON {
        0.0
    } else {
        (last - first).abs() / path
    })
}

fn edge_transition_count(history: &[Candle], upper: f64, lower: f64, tick_size: f64) -> usize {
    let band = (tick_size * 2.0).max((upper - lower) * TRANSITION_BAND_HEIGHT_RATIO);
    let mut previous_edge = 0_i8;
    let mut transitions = 0;
    for candle in history {
        let touches_upper = candle.high >= upper - band;
        let touches_lower = candle.low <= lower + band;
        let edge = match (touches_upper, touches_lower) {
            (true, false) => 1,
            (false, true) => -1,
            _ => 0,
        };
        if edge != 0 {
            if previous_edge != 0 && edge != previous_edge {
                transitions += 1;
            }
            previous_edge = edge;
        }
    }
    transitions
}

#[cfg(test)]
mod tests {
    use super::super::{StrictVisualConsolidationState, StrictVisualRangeEvent};
    use super::*;

    fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 100.0,
        }
    }

    fn edge_candle(index: usize, upper: bool) -> Candle {
        let (open, high, low, close) = if upper {
            (100.4, 101.0, 100.2, 100.7)
        } else {
            (99.6, 99.8, 99.0, 99.3)
        };
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 100.0,
        }
    }

    #[test]
    fn v31_rejects_a_window_with_one_half_height_internal_body() {
        let mut candles = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect::<Vec<_>>();
        candles[4] = Candle {
            timestamp_ms: 4 * 900_000,
            open: 99.5,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 100.0,
        };

        assert!(longest_candidate(
            &candles,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay,
        )
        .is_some());
        assert!(longest_candidate(
            &candles,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
        )
        .is_none());
        assert!(longest_candidate(
            &candles,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption,
        )
        .is_some());
    }

    #[test]
    fn v42_only_exempts_a_large_body_when_the_whole_candle_is_inside_the_core() {
        let range = StrictVisualRangeEvidence {
            start_index: 0,
            first_confirmation_index: 7,
            boundary_confirmation_index: 7,
            length_bars: 8,
            upper: 101.0,
            lower: 99.0,
            containment_ratio: 1.0,
            direction_efficiency: 0.0,
            edge_transition_count: 3,
            upper_touch_groups: 2,
            lower_touch_groups: 2,
            upper_drift_ratio: 0.0,
            lower_drift_ratio: 0.0,
        };
        let contained = candle(8, 99.4, 100.8, 99.2, 100.6);
        let piercing = candle(8, 99.4, 101.1, 99.2, 100.6);

        assert!(internal_impulse_ratio(
            contained,
            range,
            StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
        )
        .is_some());
        assert!(internal_impulse_ratio(
            contained,
            range,
            StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption,
        )
        .is_none());
        assert!(internal_impulse_ratio(
            piercing,
            range,
            StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption,
        )
        .is_some());
    }

    fn active_range_for_upgrade() -> StrictVisualRangeEvidence {
        StrictVisualRangeEvidence {
            start_index: 4,
            first_confirmation_index: 6,
            boundary_confirmation_index: 6,
            length_bars: 3,
            upper: 101.0,
            lower: 99.0,
            containment_ratio: 1.0,
            direction_efficiency: 0.0,
            edge_transition_count: 3,
            upper_touch_groups: 2,
            lower_touch_groups: 2,
            upper_drift_ratio: 0.0,
            lower_drift_ratio: 0.0,
        }
    }

    #[test]
    fn v43_rejects_only_a_one_sided_parent_upgrade() {
        let one_sided = (0..8)
            .map(|index| {
                if index % 2 == 0 {
                    candle(index, 100.4, 101.0, 100.2, 100.7)
                } else {
                    candle(index, 100.45, 100.55, 99.0, 100.55)
                }
            })
            .collect::<Vec<_>>();
        let audit = audit_candidate_quality(
            &one_sided,
            0.01,
            StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade,
            101.0,
            99.0,
            2.0,
        )
        .expect("valid audit");
        assert!(audit.passed, "{audit:?}");
        let candidate = longest_candidate(
            &one_sided,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade,
        )
        .expect("existing quality gates should accept the one-sided candidate");
        let occupancy = strict_visual_parent_vertical_occupancy(&one_sided, candidate)
            .expect("valid occupancy");
        assert_eq!(occupancy.dominant_half_ratio, 1.0);

        let mut v42 = StrictVisualConsolidationState {
            active: Some(active_range_for_upgrade()),
            ..StrictVisualConsolidationState::default()
        };
        let mut v43 = StrictVisualConsolidationState {
            active: Some(active_range_for_upgrade()),
            ..StrictVisualConsolidationState::default()
        };
        assert!(matches!(
            v42.update_for_variant(
                &one_sided,
                7,
                0.01,
                StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption,
            ),
            Some(StrictVisualRangeEvent::ParentUpgraded(_))
        ));
        assert!(v43
            .update_for_variant(
                &one_sided,
                7,
                0.01,
                StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade,
            )
            .is_none());

        let balanced = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect::<Vec<_>>();
        assert!(matches!(
            v43.update_for_variant(
                &balanced,
                7,
                0.01,
                StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade,
            ),
            Some(StrictVisualRangeEvent::ParentUpgraded(_))
        ));
    }

    #[test]
    fn eight_bar_envelope_keeps_one_sweep_wick_outside_each_side() {
        let mut candles = (0..8)
            .map(|index| candle(index, 100.0, 100.6, 99.8, 100.4))
            .collect::<Vec<_>>();
        candles[2] = candle(2, 100.1, 105.0, 99.8, 100.2);
        candles[5] = candle(5, 100.2, 100.6, 95.0, 100.0);

        let envelope = strict_visual_body_wick_envelope(&candles).expect("valid envelope");
        assert_eq!(envelope.allowed_sweep_wicks_per_side, 1);
        assert_eq!(envelope.wick_upper, 100.6);
        assert_eq!(envelope.wick_lower, 99.8);
        assert_eq!(envelope.upper_wicks_outside, 1);
        assert_eq!(envelope.lower_wicks_outside, 1);
        assert!(candles.iter().all(|item| {
            item.open.max(item.close) <= envelope.upper
                && item.open.min(item.close) >= envelope.lower
        }));
    }

    #[test]
    fn body_edge_expands_boundary_without_absorbing_its_remaining_wick() {
        let mut candles = (0..8)
            .map(|index| candle(index, 100.0, 100.6, 99.8, 100.4))
            .collect::<Vec<_>>();
        candles[3] = candle(3, 100.4, 101.2, 99.9, 101.0);

        let envelope = strict_visual_body_wick_envelope(&candles).expect("valid envelope");
        assert_eq!(envelope.wick_upper, 100.6);
        assert_eq!(envelope.body_upper, 101.0);
        assert_eq!(envelope.upper, 101.0);
        assert_eq!(envelope.upper_wicks_outside, 1);
    }

    #[test]
    fn dual_boundary_keeps_the_trade_core_inside_the_visual_parent() {
        let mut candles = (0..8)
            .map(|index| {
                if index % 2 == 0 {
                    candle(index, 100.4, 101.2, 100.2, 100.7)
                } else {
                    candle(index, 99.6, 99.8, 99.0, 99.3)
                }
            })
            .collect::<Vec<_>>();
        candles[0].high = 101.4;

        let dual = evaluate_strict_visual_dual_boundary_candidate_for_research(
            &candles,
            7,
            8,
            0.01,
            StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
        )
        .expect("valid dual boundary");

        assert_eq!(dual.trade_core.upper, 101.4);
        assert_eq!(dual.visual_parent.wick_upper, 101.2);
        assert_eq!(dual.visual_parent.upper, dual.trade_core.upper);
        assert!(dual.visual_parent.lower <= dual.trade_core.lower);
    }

    #[test]
    fn nested_candidate_is_a_later_distinct_range_inside_the_frozen_parent() {
        let mut candles = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect::<Vec<_>>();
        candles.extend((8..16).map(|index| {
            if index % 2 == 0 {
                candle(index, 100.3, 100.8, 100.1, 100.6)
            } else {
                candle(index, 99.7, 99.9, 99.2, 99.4)
            }
        }));
        let parent = longest_candidate(
            &candles,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
        )
        .expect("valid parent range");
        let scope = StrictVisualNestedRangeScope {
            parent,
            visual_upper: 101.0,
            visual_lower: 99.0,
        };

        assert!(longest_nested_candidate(
            &candles,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V41NestedChildRange,
            scope,
        )
        .is_none());
        let child = longest_nested_candidate(
            &candles,
            15,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V41NestedChildRange,
            scope,
        )
        .expect("later local range should be retained as a child");
        assert_eq!(child.start_index, 8);
        assert_eq!(child.first_confirmation_index, 15);
        assert_eq!(child.upper, 100.8);
        assert_eq!(child.lower, 99.2);
    }

    #[test]
    fn nested_candidate_does_not_duplicate_the_parent_core() {
        let candles = (0..16)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect::<Vec<_>>();
        let parent = longest_candidate(
            &candles,
            7,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
        )
        .expect("valid parent range");
        let scope = StrictVisualNestedRangeScope {
            parent,
            visual_upper: 101.0,
            visual_lower: 99.0,
        };

        assert!(longest_nested_candidate(
            &candles,
            15,
            0.01,
            None,
            StrictVisualBreakoutResearchVariant::V41NestedChildRange,
            scope,
        )
        .is_none());
    }
}
