use super::super::model::{
    BlockedSignal, Candle, Direction, IndicatorPoint, IndicatorSeries, SignalFamily,
};

/// 返回信号棒之前固定窗口的最高/最低价，当前棒不会参与锚区定义。
pub(super) fn prior_extremes(
    candles: &[Candle],
    index: usize,
    length: usize,
) -> Option<(f64, f64)> {
    let history = index
        .checked_sub(length)
        .and_then(|start| candles.get(start..index))?;
    Some((
        history
            .iter()
            .map(|candle| candle.high)
            .fold(f64::NEG_INFINITY, f64::max),
        history
            .iter()
            .map(|candle| candle.low)
            .fold(f64::INFINITY, f64::min),
    ))
}

/// 计算三根 K 线 EMA 变化除以 `3 * ATR` 的无量纲斜率。
pub(super) fn slope(
    indicators: &IndicatorSeries,
    index: usize,
    atr: f64,
    select: impl Fn(&IndicatorPoint) -> Option<f64>,
) -> Option<f64> {
    let previous = index.checked_sub(3)?;
    Some((select(&indicators.points[index])? - select(&indicators.points[previous])?) / (3.0 * atr))
}

/// 只有历史窗口全部就绪时才返回数值，避免以零填充伪造压缩状态。
pub(super) fn prior_values(
    values: &[Option<f64>],
    current: usize,
    length: usize,
) -> Option<Vec<f64>> {
    let start = current.checked_sub(length)?;
    let slice = values.get(start..current)?;
    slice.iter().copied().collect()
}

/// 将价格距离换算为 tick；止损向外取整，目标向内取整以复刻 Pine。
pub(super) fn distance_ticks(distance: f64, tick_size: f64, outward: bool) -> i64 {
    let raw = distance / tick_size;
    let ticks = if outward { raw.ceil() } else { raw.floor() };
    (ticks as i64).max(1)
}

/// 将价格向下对齐到交易所 tick。
pub(super) fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

/// 将价格向上对齐到交易所 tick。
pub(super) fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

/// 仅在条件成立时追加命中的信号家族。
pub(super) fn append_family(families: &mut Vec<SignalFamily>, matched: bool, family: SignalFamily) {
    if matched {
        families.push(family);
    }
}

/// 判断上一次事件是否仍位于不含边界的回看窗口内。
pub(super) fn is_recent(last_index: Option<usize>, current: usize, window: usize) -> bool {
    last_index.is_some_and(|last| current - last < window)
}

/// 构造带时间、方向和原因的阻塞证据。
pub(super) fn blocked(timestamp_ms: i64, direction: Direction, reason: &str) -> BlockedSignal {
    BlockedSignal {
        signal_time_ms: timestamp_ms,
        direction: Some(direction),
        reason: reason.to_string(),
    }
}

/// 计算非空窗口的算术平均值。
pub(super) fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// 返回非空窗口的最小值。
pub(super) fn minimum(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::INFINITY, f64::min)
}

/// 返回非空窗口的最大值。
pub(super) fn maximum(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
