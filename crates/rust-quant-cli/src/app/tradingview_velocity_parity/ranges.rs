mod range_squeeze_v15;

pub use range_squeeze_v15::{range_squeeze_box_v15, RangeSqueezeBoxV15};

use super::indicators::nearest_rank;
use super::model::Candle;

const TICK_MULTIPLIER_FOR_SIDEWAYS: f64 = 2.0;

/// 逆势交易在信号时点之前已经确认的 8 根横盘区。
#[derive(Debug, Clone, Copy)]
pub struct SidewaysZone {
    pub high: f64,
    pub low: f64,
    /// 横盘区最后一根已完成 K 线的索引；V4 用它验证离开区间后的首次收盘破位。
    pub end_index: usize,
}

/// V26 在突破棒出现前冻结的最长有效父横盘及其信号时证据。
#[derive(Debug, Clone, Copy)]
pub struct ActiveParentHorizontalRange {
    /// 重复压力簇的 P90 上沿，孤立长影线不会单独抬高该边界。
    pub high: f64,
    /// 重复支撑簇的 P10 下沿，孤立长影线不会单独压低该边界。
    pub low: f64,
    /// 父横盘首根完成 K 线在当前回放序列中的索引。
    pub start_index: usize,
    /// 父横盘末根完成 K 线索引，必须等于突破棒索引减一。
    pub end_index: usize,
    /// 父横盘包含的 15 分钟 K 线数量，范围固定为 8～96。
    pub length_bars: usize,
    /// 完整父横盘的收盘方向效率，V26 沿用 V25B 的 0.35 上限。
    pub direction_efficiency: f64,
    /// 父横盘按时间顺序完成的上下边界切换次数；只使用突破前完成棒计算。
    pub edge_transition_count: usize,
}

/// 20 根稳健箱体的冻结边界与质量证据。
#[derive(Debug, Clone, Copy)]
pub struct ConfirmedRange {
    pub upper: f64,
    pub raw_high: f64,
}

/// 96～240 根大型水平箱体的识别结果。
#[derive(Debug, Clone, Copy)]
pub struct LargeHorizontalRange {
    pub raw_high: f64,
}

/// 96～192 根大型上升三角的识别结果。
#[derive(Debug, Clone, Copy)]
pub struct LargeAscendingTriangle {
    pub resistance: f64,
}

/// 从信号 K 线之前的 48 根历史中，选择结束位置最近的有效 8 根横盘区。
pub fn nearest_sideways_zone(
    candles: &[Candle],
    current: usize,
    tick_size: f64,
) -> Option<SidewaysZone> {
    const LENGTH: usize = 8;
    const LOOKBACK: usize = 48;
    for end_offset in 1..=LOOKBACK - LENGTH + 1 {
        if let Some(zone) = sideways_window(candles, current, end_offset, tick_size) {
            return Some(zone);
        }
    }
    None
}

/// 选择最近的稳定横盘，但只在当前棒是横盘结束后的首次收盘上破时返回。
///
/// 盘中影线允许越界；任何更早的完成棒已经收在上沿之上都会让旧横盘失效，避免把趋势延续
/// 重新命名为“首次突破”。
pub fn nearest_fresh_horizontal_upside_breakout_zone(
    candles: &[Candle],
    current: usize,
    tick_size: f64,
) -> Option<SidewaysZone> {
    nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency(
        candles, current, tick_size, None,
    )
}

/// 在 V23/V24 首次突破合同上，可选地拒绝收盘路径带有明显方向性的 8 根候选区。
///
/// 旧边界检查只比较极值，孤立长影线可能把 V 型修复伪装成上下沿稳定；方向效率改看完成棒
/// 的收盘路径。`None` 完整保留 V23/V24 行为，避免研究门禁反向覆盖旧版本结果。
pub fn nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency(
    candles: &[Candle],
    current: usize,
    tick_size: f64,
    direction_efficiency_max: Option<f64>,
) -> Option<SidewaysZone> {
    const LENGTH: usize = 8;
    const LOOKBACK: usize = 48;
    debug_assert!(direction_efficiency_max.is_none_or(|limit| (0.0..=1.0).contains(&limit)));
    let breakout = candles
        .get(current)
        .copied()
        .filter(|candle| candle.is_valid())?;
    for end_offset in 1..=LOOKBACK - LENGTH + 1 {
        let Some(zone) = sideways_window(candles, current, end_offset, tick_size) else {
            continue;
        };
        if !horizontal_boundaries_are_stable(candles, zone) || breakout.close <= zone.high {
            continue;
        }
        if let Some(limit) = direction_efficiency_max {
            let efficiency = horizontal_close_direction_efficiency(candles, zone)?;
            if efficiency > limit {
                continue;
            }
        }
        let first_after_zone = zone.end_index.checked_add(1)?;
        if candles
            .get(first_after_zone..current)?
            .iter()
            .all(|candle| candle.close <= zone.high)
        {
            return Some(zone);
        }
    }
    None
}

/// 先选择紧贴突破棒的最长有效父横盘，再验证当前收盘是否真正突破其稳健上沿。
///
/// 选择过程不能读取当前棒价格，否则一个尚未突破父横盘的收盘可能反向挑中更短、更低的
/// 微型区间。8 根只是成形下限；96 根是 15 分钟研究版本固定的 24 小时上限。
pub fn active_parent_horizontal_upside_breakout_zone(
    candles: &[Candle],
    current: usize,
    tick_size: f64,
    direction_efficiency_max: f64,
) -> Option<ActiveParentHorizontalRange> {
    const MIN_LENGTH: usize = 8;
    const MAX_LENGTH: usize = 96;
    debug_assert!((0.0..=1.0).contains(&direction_efficiency_max));
    let breakout = candles
        .get(current)
        .copied()
        .filter(|candle| candle.is_valid())?;
    let maximum_length = current.min(MAX_LENGTH);
    if maximum_length < MIN_LENGTH || tick_size <= 0.0 {
        return None;
    }

    let mut selected = None;
    for length in MIN_LENGTH..=maximum_length {
        if let Some(candidate) = active_parent_horizontal_window(
            candles,
            current,
            length,
            tick_size,
            direction_efficiency_max,
        ) {
            // 长度单调递增，覆盖式选择确保嵌套微区间不能替代仍有效的父横盘。
            selected = Some(candidate);
        }
    }

    selected.filter(|range| breakout.close > range.high)
}

/// 识别信号棒之前固定 20 根的稳健箱体。
pub fn confirmed_anchor_range(
    candles: &[Candle],
    current: usize,
    previous_atr: f64,
) -> Option<ConfirmedRange> {
    const LENGTH: usize = 20;
    const HALF: usize = 10;
    const RECENT_CONTRACTION: usize = 5;
    if current < LENGTH || previous_atr <= 0.0 {
        return None;
    }

    let history = chronological_history(candles, current, LENGTH)?;
    let highs: Vec<f64> = history.iter().map(|candle| candle.high).collect();
    let lows: Vec<f64> = history.iter().map(|candle| candle.low).collect();
    let upper = nearest_rank(&highs, 85.0)?;
    let lower = nearest_rank(&lows, 15.0)?;
    let raw_high = highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let height = upper - lower;
    let middle = (upper + lower) * 0.5;
    if height <= 0.0 || middle <= 0.0 {
        return None;
    }
    let width_ratio = height / middle;
    let touch_band = (middle * 0.0015).max(previous_atr * 0.35);
    let upper_touches = touch_groups(&history, 2, |candle| candle.high >= upper - touch_band);
    let lower_touches = touch_groups(&history, 2, |candle| candle.low <= lower + touch_band);
    let contained = history
        .iter()
        .filter(|candle| candle.close >= lower - touch_band && candle.close <= upper + touch_band)
        .count();
    let containment_ratio = contained as f64 / LENGTH as f64;

    let early_highs = &highs[..HALF];
    let recent_highs = &highs[HALF..];
    let early_lows = &lows[..HALF];
    let recent_lows = &lows[HALF..];
    let upper_drift_ratio =
        (nearest_rank(recent_highs, 80.0)? - nearest_rank(early_highs, 80.0)?).abs() / height;
    let lower_drift_ratio =
        (nearest_rank(recent_lows, 20.0)? - nearest_rank(early_lows, 20.0)?).abs() / height;

    let true_ranges: Vec<f64> = (current - LENGTH..current)
        .map(|index| true_range(candles, index))
        .collect();
    let earlier_tr_mean = mean(&true_ranges[..LENGTH - RECENT_CONTRACTION]);
    let recent_tr_mean = mean(&true_ranges[LENGTH - RECENT_CONTRACTION..]);
    if earlier_tr_mean <= 0.0 {
        return None;
    }
    let contraction_ratio = recent_tr_mean / earlier_tr_mean;

    (width_ratio <= 0.03
        && upper_touches >= 2
        && lower_touches >= 2
        && containment_ratio >= 0.80
        && upper_drift_ratio <= 0.25
        && lower_drift_ratio <= 0.35
        && contraction_ratio <= 1.0)
        .then_some(ConfirmedRange { upper, raw_high })
}

/// 在固定窗口集合中选择最长的有效大型水平箱体。
pub fn longest_large_horizontal_range(
    candles: &[Candle],
    current: usize,
    atr: f64,
    current_close: f64,
) -> Option<LargeHorizontalRange> {
    let mut selected = None;
    for length in (96..=240).step_by(24) {
        let Some(candidate) = large_horizontal_range(candles, current, length, atr) else {
            continue;
        };
        if current_close > candidate.raw_high {
            selected = Some(candidate);
        }
    }
    selected
}

/// 在固定窗口集合中选择最长的有效大型上升三角。
pub fn longest_large_ascending_triangle(
    candles: &[Candle],
    current: usize,
    atr: f64,
    current_close: f64,
) -> Option<LargeAscendingTriangle> {
    let mut selected = None;
    for length in (96..=192).step_by(24) {
        let Some(candidate) = large_ascending_triangle(candles, current, length, atr) else {
            continue;
        };
        if current_close > candidate.resistance {
            selected = Some(candidate);
        }
    }
    selected
}

/// 只读取信号棒之前的窗口；`end_offset` 保证开仓后的 K 线不能移动结构边界。
fn sideways_window(
    candles: &[Candle],
    current: usize,
    end_offset: usize,
    tick_size: f64,
) -> Option<SidewaysZone> {
    const LENGTH: usize = 8;
    let start_offset = end_offset + LENGTH - 1;
    if current < start_offset {
        return None;
    }
    let start = current - start_offset;
    let end = current - end_offset + 1;
    let history = candles.get(start..end)?;
    if history.len() != LENGTH || history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }
    let high = history
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = history
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let height = high - low;
    if height <= 0.0 || low <= 0.0 {
        return None;
    }
    let width_ratio = height / low;
    let touch_band = (tick_size * TICK_MULTIPLIER_FOR_SIDEWAYS).max(height * 0.10);
    let upper_touches = touch_groups(history, 2, |candle| candle.high >= high - touch_band);
    let lower_touches = touch_groups(history, 2, |candle| candle.low <= low + touch_band);

    (width_ratio <= 0.03 && upper_touches >= 2 && lower_touches >= 2).then_some(SidewaysZone {
        high,
        low,
        end_index: end - 1,
    })
}

/// 比较横盘前后半段的真实上下沿；持续抬高的阶梯即使宽度较窄也不能成为 V23 锚区。
fn horizontal_boundaries_are_stable(candles: &[Candle], zone: SidewaysZone) -> bool {
    const LENGTH: usize = 8;
    const HALF: usize = LENGTH / 2;
    const UPPER_DRIFT_MAX: f64 = 0.25;
    const LOWER_DRIFT_MAX: f64 = 0.35;
    let Some(start) = zone
        .end_index
        .checked_add(1)
        .and_then(|end| end.checked_sub(LENGTH))
    else {
        return false;
    };
    let Some(history) = candles.get(start..=zone.end_index) else {
        return false;
    };
    let height = zone.high - zone.low;
    if history.len() != LENGTH || height <= 0.0 {
        return false;
    }
    let early = &history[..HALF];
    let recent = &history[HALF..];
    let early_high = early
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let recent_high = recent
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let early_low = early
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let recent_low = recent
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    (recent_high - early_high).abs() / height <= UPPER_DRIFT_MAX
        && (recent_low - early_low).abs() / height <= LOWER_DRIFT_MAX
}

/// 计算 8 根收盘净位移占实际路径的比例；越接近 1 越像单向推进，完全静止路径记为 0。
fn horizontal_close_direction_efficiency(candles: &[Candle], zone: SidewaysZone) -> Option<f64> {
    const LENGTH: usize = 8;
    let start = zone.end_index.checked_add(1)?.checked_sub(LENGTH)?;
    let history = candles.get(start..=zone.end_index)?;
    (history.len() == LENGTH).then_some(())?;
    close_direction_efficiency(history)
}

/// 验证一个紧贴当前棒的可变长度父横盘；当前突破棒不参与任何边界或质量计算。
fn active_parent_horizontal_window(
    candles: &[Candle],
    current: usize,
    length: usize,
    tick_size: f64,
    direction_efficiency_max: f64,
) -> Option<ActiveParentHorizontalRange> {
    const WIDTH_RATIO_MAX: f64 = 0.03;
    const CONTAINMENT_RATIO_MIN: f64 = 0.80;
    const UPPER_DRIFT_MAX: f64 = 0.25;
    const LOWER_DRIFT_MAX: f64 = 0.35;
    let start = current.checked_sub(length)?;
    let end_index = current.checked_sub(1)?;
    let history = candles.get(start..current)?;
    if history.len() != length || history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }

    let highs: Vec<f64> = history.iter().map(|candle| candle.high).collect();
    let lows: Vec<f64> = history.iter().map(|candle| candle.low).collect();
    let high = nearest_rank(&highs, 90.0)?;
    let low = nearest_rank(&lows, 10.0)?;
    let height = high - low;
    let middle = (high + low) * 0.5;
    if height <= 0.0 || middle <= 0.0 {
        return None;
    }

    let touch_band = (tick_size * TICK_MULTIPLIER_FOR_SIDEWAYS).max(height * 0.10);
    let upper_touches = touch_groups(history, 2, |candle| candle.high >= high - touch_band);
    let lower_touches = touch_groups(history, 2, |candle| candle.low <= low + touch_band);
    let contained = history
        .iter()
        .filter(|candle| candle.close >= low - touch_band && candle.close <= high + touch_band)
        .count();
    let containment_ratio = contained as f64 / length as f64;

    let half = length / 2;
    let early = &history[..half];
    let recent = &history[half..];
    let upper_drift_ratio = (nearest_rank(
        &recent.iter().map(|candle| candle.high).collect::<Vec<_>>(),
        80.0,
    )? - nearest_rank(
        &early.iter().map(|candle| candle.high).collect::<Vec<_>>(),
        80.0,
    )?)
    .abs()
        / height;
    let lower_drift_ratio = (nearest_rank(
        &recent.iter().map(|candle| candle.low).collect::<Vec<_>>(),
        20.0,
    )? - nearest_rank(
        &early.iter().map(|candle| candle.low).collect::<Vec<_>>(),
        20.0,
    )?)
    .abs()
        / height;
    let direction_efficiency = close_direction_efficiency(history)?;
    let edge_transition_count = horizontal_edge_transition_count(history, high, low, tick_size);

    (height / middle <= WIDTH_RATIO_MAX
        && upper_touches >= 2
        && lower_touches >= 2
        && containment_ratio >= CONTAINMENT_RATIO_MIN
        && upper_drift_ratio <= UPPER_DRIFT_MAX
        && lower_drift_ratio <= LOWER_DRIFT_MAX
        && direction_efficiency <= direction_efficiency_max)
        .then_some(ActiveParentHorizontalRange {
            high,
            low,
            start_index: start,
            end_index,
            length_bars: length,
            direction_efficiency,
            edge_transition_count,
        })
}

/// 计算任意已完成窗口的收盘净位移占实际路径比例，完全静止的路径记为 0。
fn close_direction_efficiency(history: &[Candle]) -> Option<f64> {
    if history.is_empty() || history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }
    let first_close = history.first()?.close;
    let last_close = history.last()?.close;
    let path_length = history
        .windows(2)
        .map(|pair| (pair[1].close - pair[0].close).abs())
        .sum::<f64>();
    if path_length <= f64::EPSILON {
        Some(0.0)
    } else {
        Some((last_close - first_close).abs() / path_length)
    }
}

/// 对一个冻结长度验证水平边界、独立触碰、收敛与区间容纳率。
fn large_horizontal_range(
    candles: &[Candle],
    current: usize,
    length: usize,
    atr: f64,
) -> Option<LargeHorizontalRange> {
    if atr <= 0.0 {
        return None;
    }
    let history = chronological_history(candles, current, length)?;
    let segment_length = length / 3;
    let highs: Vec<f64> = history.iter().map(|candle| candle.high).collect();
    let lows: Vec<f64> = history.iter().map(|candle| candle.low).collect();
    let upper = nearest_rank(&highs, 90.0)?;
    let lower = nearest_rank(&lows, 10.0)?;
    let raw_high = highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let height = upper - lower;
    let middle = (upper + lower) * 0.5;
    if height <= 0.0 || middle <= 0.0 {
        return None;
    }
    let width_ratio = height / middle;
    let touch_band = (middle * 0.002).max(atr * 0.50);
    let upper_touches = touch_groups(&history, 8, |candle| candle.high >= upper - touch_band);
    let lower_touches = touch_groups(&history, 8, |candle| candle.low <= lower + touch_band);
    let contained = history
        .iter()
        .filter(|candle| candle.close >= lower - touch_band && candle.close <= upper + touch_band)
        .count();
    let containment_ratio = contained as f64 / length as f64;

    let high_segments = three_segments(&highs, segment_length)?;
    let low_segments = three_segments(&lows, segment_length)?;
    let upper_levels = [
        nearest_rank(high_segments[0], 90.0)?,
        nearest_rank(high_segments[1], 90.0)?,
        nearest_rank(high_segments[2], 90.0)?,
    ];
    let lower_levels = [
        nearest_rank(low_segments[0], 10.0)?,
        nearest_rank(low_segments[1], 10.0)?,
        nearest_rank(low_segments[2], 10.0)?,
    ];
    let upper_drift_ratio = spread(&upper_levels) / height;
    let lower_drift_ratio = spread(&lower_levels) / height;

    (width_ratio <= 0.03
        && upper_touches >= 2
        && lower_touches >= 2
        && containment_ratio >= 0.90
        && upper_drift_ratio <= 0.45
        && lower_drift_ratio <= 0.45)
        .then_some(LargeHorizontalRange { raw_high })
}

/// 对一个冻结长度验证近似水平压力、逐段抬高低点与宽度收缩。
fn large_ascending_triangle(
    candles: &[Candle],
    current: usize,
    length: usize,
    atr: f64,
) -> Option<LargeAscendingTriangle> {
    if atr <= 0.0 {
        return None;
    }
    let history = chronological_history(candles, current, length)?;
    let segment_length = length / 3;
    let segments = [
        &history[..segment_length],
        &history[segment_length..2 * segment_length],
        &history[2 * segment_length..],
    ];
    let highs = segments.map(|segment| {
        segment
            .iter()
            .map(|candle| candle.high)
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let lows = segments.map(|segment| {
        segment
            .iter()
            .map(|candle| candle.low)
            .fold(f64::INFINITY, f64::min)
    });
    let resistance = highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let lowest_segment_peak = highs.iter().copied().fold(f64::INFINITY, f64::min);
    if resistance <= 0.0 {
        return None;
    }
    let resistance_spread_ratio = (resistance - lowest_segment_peak) / resistance;
    let touch_band = (resistance * 0.003).max(atr * 0.50);
    let upper_touches = touch_groups(&history, 8, |candle| candle.high >= resistance - touch_band);
    let low_step_minimum = atr * 0.25;
    let lows_rising = lows[1] - lows[0] >= low_step_minimum
        && lows[2] - lows[1] >= low_step_minimum
        && lows[2] - lows[0] >= atr;
    let early_width = resistance - lows[0];
    let recent_width = resistance - lows[2];
    if early_width <= 0.0 || recent_width <= 0.0 {
        return None;
    }
    let contraction_ratio = recent_width / early_width;

    (resistance_spread_ratio <= 0.005
        && upper_touches >= 3
        && lows_rising
        && contraction_ratio <= 0.70)
        .then_some(LargeAscendingTriangle { resistance })
}

/// 返回 `[current-length,current)` 的已完成历史，明确排除信号棒自身。
fn chronological_history(candles: &[Candle], current: usize, length: usize) -> Option<Vec<Candle>> {
    if current < length {
        return None;
    }
    let history = candles.get(current - length..current)?;
    history
        .iter()
        .all(|candle| candle.is_valid())
        .then(|| history.to_vec())
}

/// 计算含前收盘跳空的真实波幅，价格单位与输入 OHLC 相同。
fn true_range(candles: &[Candle], index: usize) -> f64 {
    let candle = candles[index];
    if index == 0 {
        return candle.range();
    }
    let previous_close = candles[index - 1].close;
    candle
        .range()
        .max((candle.high - previous_close).abs())
        .max((candle.low - previous_close).abs())
}

/// 统计横盘在上下边界之间真实切换的次数，避免把先筑底、后抬高的单向修复当作震荡。
///
/// 边界带固定为区间高度的 20%，同时保留两 tick 的最低容差。同一根 K 线同时触及两侧时
/// 无法从 OHLC 还原盘中顺序，因此不补造成一次切换；连续停留在同一侧也只算一个事件。
fn horizontal_edge_transition_count(
    candles: &[Candle],
    upper: f64,
    lower: f64,
    tick_size: f64,
) -> usize {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Edge {
        Upper,
        Lower,
    }

    let height = upper - lower;
    if height <= 0.0 || tick_size <= 0.0 {
        return 0;
    }
    let edge_band = (tick_size * TICK_MULTIPLIER_FOR_SIDEWAYS).max(height * 0.20);
    let mut previous_edge = None;
    let mut transitions = 0usize;
    for candle in candles.iter().copied() {
        let touches_upper = candle.high >= upper - edge_band;
        let touches_lower = candle.low <= lower + edge_band;
        let edge = match (touches_upper, touches_lower) {
            (true, false) => Some(Edge::Upper),
            (false, true) => Some(Edge::Lower),
            _ => None,
        };
        let Some(edge) = edge else {
            continue;
        };
        if previous_edge.is_some_and(|previous| previous != edge) {
            transitions += 1;
        }
        previous_edge = Some(edge);
    }
    transitions
}

/// 按最小 K 线间隔合并触碰，避免连续贴边被误计为多组独立确认。
fn touch_groups(
    candles: &[Candle],
    minimum_gap: usize,
    mut touches: impl FnMut(Candle) -> bool,
) -> usize {
    let mut groups = 0usize;
    let mut last_touch = None;
    for (index, candle) in candles.iter().copied().enumerate() {
        if touches(candle) {
            if last_touch
                .map(|previous| index - previous >= minimum_gap)
                .unwrap_or(true)
            {
                groups += 1;
            }
            last_touch = Some(index);
        }
    }
    groups
}

/// 保持时间顺序切成早、中、晚三段，用于验证结构漂移而非全窗均值。
fn three_segments(values: &[f64], segment_length: usize) -> Option<[&[f64]; 3]> {
    (values.len() >= segment_length * 3).then(|| {
        [
            &values[..segment_length],
            &values[segment_length..2 * segment_length],
            &values[2 * segment_length..],
        ]
    })
}

fn spread(values: &[f64]) -> f64 {
    let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
    maximum - minimum
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_candles(count: usize) -> Vec<Candle> {
        (0..count)
            .map(|index| Candle {
                timestamp_ms: index as i64 * 900_000,
                open: 100.0,
                high: if index % 3 == 0 { 101.0 } else { 100.6 },
                low: if index % 3 == 1 { 99.0 } else { 99.4 },
                close: 100.0,
                volume: 10.0,
            })
            .collect()
    }

    #[test]
    fn sideways_zone_uses_only_prior_candles() {
        let mut candles = flat_candles(60);
        candles[59].high = 200.0;
        candles[59].close = 150.0;
        let zone = nearest_sideways_zone(&candles, 59, 0.1).expect("prior range should exist");

        assert!(zone.high < 110.0);
        assert!(zone.low > 90.0);
    }

    #[test]
    fn recent_horizontal_anchor_accepts_the_first_completed_upside_break() {
        let mut candles = flat_candles(9);
        candles[8] = Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.5,
            high: 102.0,
            low: 100.4,
            close: 101.5,
            volume: 100.0,
        };

        let zone = nearest_fresh_horizontal_upside_breakout_zone(&candles, 8, 0.1)
            .expect("the first close above a stable range should freeze that range");
        assert_eq!(zone.end_index, 7);
        assert_eq!(zone.high, 101.0);
        assert_eq!(zone.low, 99.0);
        assert!(
            nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency(
                &candles,
                8,
                0.1,
                Some(0.30),
            )
            .is_some()
        );
    }

    #[test]
    fn active_parent_is_selected_before_testing_the_breakout_close() {
        let mut candles = (0..16)
            .map(|index| {
                let is_parent_half = index < 8;
                Candle {
                    timestamp_ms: index as i64 * 900_000,
                    open: 100.0,
                    high: if index % 2 == 0 {
                        if is_parent_half {
                            101.0
                        } else {
                            100.6
                        }
                    } else if is_parent_half {
                        100.7
                    } else {
                        100.3
                    },
                    low: if index % 2 == 1 {
                        if is_parent_half {
                            99.0
                        } else {
                            99.4
                        }
                    } else if is_parent_half {
                        99.3
                    } else {
                        99.7
                    },
                    close: 100.0,
                    volume: 10.0,
                }
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            timestamp_ms: 16 * 900_000,
            open: 100.5,
            high: 100.9,
            low: 100.4,
            close: 100.8,
            volume: 100.0,
        });

        assert!(
            nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency(
                &candles,
                16,
                0.1,
                Some(0.35),
            )
            .is_some(),
            "V25B reproduces the nested eight-bar false breakout"
        );
        assert!(
            active_parent_horizontal_upside_breakout_zone(&candles, 16, 0.1, 0.35).is_none(),
            "the close remains below the valid parent upper boundary"
        );
    }

    #[test]
    fn active_parent_accepts_a_close_above_the_longest_valid_range() {
        let mut candles = (0..16)
            .map(|index| Candle {
                timestamp_ms: index as i64 * 900_000,
                open: 100.0,
                high: if index % 2 == 0 { 101.0 } else { 100.7 },
                low: if index % 2 == 1 { 99.0 } else { 99.3 },
                close: 100.0,
                volume: 10.0,
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            timestamp_ms: 16 * 900_000,
            open: 100.6,
            high: 101.2,
            low: 100.5,
            close: 101.1,
            volume: 100.0,
        });

        let range = active_parent_horizontal_upside_breakout_zone(&candles, 16, 0.1, 0.35)
            .expect("a close above the parent upper should freeze the full range");
        assert_eq!(range.start_index, 0);
        assert_eq!(range.end_index, 15);
        assert_eq!(range.length_bars, 16);
        assert_eq!(range.high, 101.0);
        assert_eq!(range.low, 99.0);
        assert_eq!(range.direction_efficiency, 0.0);
    }

    #[test]
    fn recent_horizontal_anchor_rejects_a_range_broken_before_the_current_bar() {
        let mut candles = flat_candles(8);
        candles.extend([
            Candle {
                timestamp_ms: 8 * 900_000,
                open: 100.5,
                high: 101.8,
                low: 100.4,
                close: 101.2,
                volume: 10.0,
            },
            Candle {
                timestamp_ms: 9 * 900_000,
                open: 101.2,
                high: 102.0,
                low: 101.1,
                close: 101.6,
                volume: 10.0,
            },
            Candle {
                timestamp_ms: 10 * 900_000,
                open: 101.6,
                high: 102.4,
                low: 101.5,
                close: 102.1,
                volume: 100.0,
            },
        ]);

        assert!(nearest_fresh_horizontal_upside_breakout_zone(&candles, 10, 0.1).is_none());
    }

    #[test]
    fn recent_horizontal_anchor_rejects_a_directional_staircase() {
        let levels = [100.0, 100.4, 100.0, 100.4, 101.0, 101.5, 101.2, 101.6];
        let mut candles = levels
            .into_iter()
            .enumerate()
            .map(|(index, level)| Candle {
                timestamp_ms: index as i64 * 900_000,
                open: level,
                high: level + 0.4,
                low: level - 0.4,
                close: level,
                volume: 10.0,
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 101.9,
            high: 102.4,
            low: 101.8,
            close: 102.1,
            volume: 100.0,
        });

        assert!(sideways_window(&candles, 8, 1, 0.1).is_some());
        assert!(nearest_fresh_horizontal_upside_breakout_zone(&candles, 8, 0.1).is_none());
    }

    #[test]
    fn direction_efficiency_rejects_the_icp_recovery_mislabeled_as_horizontal() {
        let rows = [
            (2.128, 2.132, 2.111, 2.118),
            (2.119, 2.149, 2.119, 2.135),
            (2.134, 2.136, 2.119, 2.129),
            (2.130, 2.134, 2.122, 2.125),
            (2.124, 2.129, 2.107, 2.123),
            (2.122, 2.136, 2.120, 2.127),
            (2.127, 2.138, 2.125, 2.138),
            (2.137, 2.150, 2.137, 2.144),
            (2.143, 2.149, 2.139, 2.141),
            (2.141, 2.145, 2.128, 2.142),
            (2.141, 2.141, 2.130, 2.131),
            (2.131, 2.150, 2.125, 2.150),
            (2.149, 2.166, 2.141, 2.164),
        ];
        let candles = rows
            .into_iter()
            .enumerate()
            .map(|(index, (open, high, low, close))| Candle {
                timestamp_ms: index as i64 * 900_000,
                open,
                high,
                low,
                close,
                volume: 10.0,
            })
            .collect::<Vec<_>>();

        let v24_zone = nearest_fresh_horizontal_upside_breakout_zone(&candles, 12, 0.001)
            .expect("V24 should reproduce the ICP false horizontal before V25 filters it");
        let efficiency = horizontal_close_direction_efficiency(&candles, v24_zone)
            .expect("the completed eight-bar window has a causal close path");

        assert_eq!(v24_zone.end_index, 7);
        assert!((efficiency - 0.52).abs() < 1e-12);
        for limit in [0.30, 0.35, 0.40] {
            assert!(
                nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency(
                    &candles,
                    12,
                    0.001,
                    Some(limit),
                )
                .is_none()
            );
        }
    }

    #[test]
    fn edge_transitions_reject_sequential_support_then_resistance_touches() {
        let candles = [
            (99.6, 100.55, 99.0, 99.6),
            (99.8, 100.55, 99.5, 100.3),
            (99.7, 100.55, 99.0, 99.5),
            (99.8, 100.55, 99.5, 100.2),
            (100.2, 101.0, 99.45, 100.4),
            (100.0, 100.55, 99.45, 99.8),
            (100.2, 101.0, 99.45, 100.5),
            (100.0, 100.55, 99.45, 99.9),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (open, high, low, close))| Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 10.0,
        })
        .collect::<Vec<_>>();

        assert_eq!(
            horizontal_edge_transition_count(&candles, 101.0, 99.0, 0.1),
            1
        );
    }

    #[test]
    fn edge_transitions_accept_four_alternating_boundary_events() {
        let candles = [
            (100.2, 101.0, 99.45, 100.5),
            (100.0, 100.55, 99.45, 99.8),
            (99.6, 100.55, 99.0, 99.5),
            (99.8, 100.55, 99.5, 100.2),
            (100.2, 101.0, 99.45, 100.4),
            (100.0, 100.55, 99.45, 99.8),
            (99.6, 100.55, 99.0, 99.6),
            (99.8, 100.55, 99.5, 100.1),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (open, high, low, close))| Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 10.0,
        })
        .collect::<Vec<_>>();

        assert_eq!(
            horizontal_edge_transition_count(&candles, 101.0, 99.0, 0.1),
            3
        );
    }

    #[test]
    fn confirmed_range_rejects_directional_drift() {
        let mut candles = flat_candles(30);
        for (index, candle) in candles.iter_mut().enumerate().take(29).skip(9) {
            let shift = (index - 9) as f64 * 0.5;
            candle.open += shift;
            candle.high += shift;
            candle.low += shift;
            candle.close += shift;
        }
        assert!(confirmed_anchor_range(&candles, 29, 1.0).is_none());
    }
}
