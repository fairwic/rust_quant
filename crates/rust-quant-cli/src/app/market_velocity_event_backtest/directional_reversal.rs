use super::{
    BacktestCandle, ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
    MS_15M,
};

pub(super) const VOLUME_ATR_PERIOD: usize = 14;
pub(super) const VOLUME_ATR_AVERAGE_CANDLES: usize = 20;
pub(super) const VOLUME_ATR_BASE_VOLUME_RATIO: f64 = 1.5;
pub(super) const VOLUME_ATR_STRONG_VOLUME_RATIO: f64 = 2.0;
pub(super) const VOLUME_ATR_EXTREME_VOLUME_RATIO: f64 = 3.0;
pub(super) const VOLUME_ATR_BASE_MULTIPLIER: f64 = 1.5;
pub(super) const VOLUME_ATR_STRONG_MULTIPLIER: f64 = 2.0;
pub(super) const VOLUME_ATR_EXTREME_MULTIPLIER: f64 = 3.0;
pub(super) const CONTINUATION_MIN_BODY_RATIO: f64 = 0.60;
pub(super) const CONTINUATION_MIN_RANGE_ATR_RATIO: f64 = 1.20;
pub(super) const CONTINUATION_MAX_CLOSE_FROM_LOW_RATIO: f64 = 0.25;
pub(super) const CONTINUATION_INVALIDATION_ATR: f64 = 0.50;
pub(super) const REVERSAL_CONFIRMATION_MIN_BODY_RATIO: f64 = 0.45;
pub(super) const REVERSAL_CONFIRMATION_MIN_CLOSE_POSITION_RATIO: f64 = 0.65;
pub(super) const OPPOSITE_DURATION_MIN_R_SQUARED: f64 = 0.70;
pub(super) const EXHAUSTION_VOLUME_LOOKBACK_CANDLES: usize = 96;
pub(super) const EXHAUSTION_CURRENT_CLUSTER_CANDLES: usize = 3;
pub(super) const EXHAUSTION_SWING_RADIUS_CANDLES: usize = 3;
pub(super) const BTC_REGIME_LOOKBACK_CANDLES: usize = 96;
pub(super) const BTC_BROAD_DIRECTION_LOOKBACK_CANDLES: usize = 384;

/// 只读取实际入场前已经完成的基准 K 线，返回固定窗口首尾收盘的绝对净涨跌幅。
pub(super) fn benchmark_abs_net_move_pct_before_entry(
    candles: &[BacktestCandle],
    entry_ts: i64,
    lookback_candles: usize,
) -> Option<f64> {
    let completed_count = candles.partition_point(|candle| candle.ts + MS_15M <= entry_ts);
    let start = completed_count.checked_sub(lookback_candles)?;
    let history = candles.get(start..completed_count)?;
    let first_close = history.first()?.close;
    let last_close = history.last()?.close;
    if !valid_positive(first_close) || !valid_positive(last_close) {
        return None;
    }
    Some(((last_close / first_close) - 1.0).abs() * 100.0)
}

/// 返回基准币相对交易方向的净幅：做多取原始净涨幅，做空取原始净跌幅。
pub(super) fn benchmark_directional_net_move_pct_before_entry(
    candles: &[BacktestCandle],
    entry_ts: i64,
    lookback_candles: usize,
    direction: MarketVelocityTradeDirection,
) -> Option<f64> {
    let completed_count = candles.partition_point(|candle| candle.ts + MS_15M <= entry_ts);
    let start = completed_count.checked_sub(lookback_candles)?;
    let history = candles.get(start..completed_count)?;
    let first_close = history.first()?.close;
    let last_close = history.last()?.close;
    if !valid_positive(first_close) || !valid_positive(last_close) {
        return None;
    }
    let raw_net_move_pct = ((last_close / first_close) - 1.0) * 100.0;
    match direction {
        MarketVelocityTradeDirection::Long => Some(raw_net_move_pct),
        MarketVelocityTradeDirection::Short => Some(-raw_net_move_pct),
        MarketVelocityTradeDirection::Both => None,
    }
}

/// 检查触发 K 线前的反向趋势：固定净幅度与长时间整体趋势满足任一条件即可。
pub(super) fn opposite_net_move_filter_reason(
    candles: &[ComputedCandle],
    completed_count: usize,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<&'static str> {
    if args.entry_min_opposite_net_move_pct.is_none()
        && args.entry_min_opposite_duration_candles.is_none()
    {
        return None;
    }
    let latest_idx = completed_count.checked_sub(1)?;
    let mut ready = false;
    let mut confirmed = false;
    if let Some(min_move_pct) = args.entry_min_opposite_net_move_pct {
        if let Some(net_move_confirmed) =
            opposite_net_move_confirmed(candles, latest_idx, direction, args, min_move_pct)
        {
            ready = true;
            confirmed |= net_move_confirmed;
        }
    }
    if let Some(duration_candles) = args.entry_min_opposite_duration_candles {
        if let Some(duration_confirmed) =
            opposite_duration_confirmed(candles, latest_idx, direction, duration_candles)
        {
            ready = true;
            confirmed |= duration_confirmed;
        }
    }
    if confirmed {
        return None;
    }
    match (ready, args.entry_min_opposite_duration_candles.is_some()) {
        (false, true) => Some("opposite_move_not_ready"),
        (false, false) => Some("opposite_net_move_not_ready"),
        (true, true) => Some("opposite_move_not_confirmed"),
        (true, false) => Some("opposite_net_move_not_confirmed"),
    }
}

/// 计算固定窗口首尾净幅度是否达到门槛；None 表示信号时点历史不足。
fn opposite_net_move_confirmed(
    candles: &[ComputedCandle],
    latest_idx: usize,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
    min_move_pct: f64,
) -> Option<bool> {
    let history = history_before_latest(
        candles,
        latest_idx,
        args.entry_opposite_move_lookback_candles,
    )?;
    let first_open = history.first()?.candle.open;
    let last_close = history.last()?.candle.close;
    if !valid_positive(first_open) || !valid_positive(last_close) {
        return None;
    }
    let move_pct = match direction {
        MarketVelocityTradeDirection::Long => (first_open - last_close) / first_open * 100.0,
        MarketVelocityTradeDirection::Short => (last_close - first_open) / first_open * 100.0,
        MarketVelocityTradeDirection::Both => return Some(false),
    };
    Some(move_pct >= min_move_pct)
}

/// 用线性回归检查长时间反向趋势，不使用固定百分比门槛。
fn opposite_duration_confirmed(
    candles: &[ComputedCandle],
    latest_idx: usize,
    direction: MarketVelocityTradeDirection,
    duration_candles: usize,
) -> Option<bool> {
    let history = history_before_latest(candles, latest_idx, duration_candles)?;
    let sample_count = history.len() as f64;
    let mean_x = (sample_count - 1.0) / 2.0;
    let mean_y = history
        .iter()
        .map(|computed| computed.candle.close)
        .sum::<f64>()
        / sample_count;
    let first_close = history.first()?.candle.close;
    let last_close = history.last()?.candle.close;
    if !valid_positive(mean_y) || !valid_positive(first_close) || !valid_positive(last_close) {
        return None;
    }
    let (covariance, variance_x, variance_y) = history.iter().enumerate().fold(
        (0.0, 0.0, 0.0),
        |(covariance, variance_x, variance_y), (idx, computed)| {
            let x_distance = idx as f64 - mean_x;
            let y_distance = computed.candle.close - mean_y;
            (
                covariance + x_distance * y_distance,
                variance_x + x_distance * x_distance,
                variance_y + y_distance * y_distance,
            )
        },
    );
    if !valid_positive(variance_x) || !valid_positive(variance_y) {
        return Some(false);
    }
    let slope = covariance / variance_x;
    let r_squared = covariance * covariance / (variance_x * variance_y);
    // R² 约束的是整段走势的方向一致性而非涨跌幅，因此能容纳途中反弹，
    // 同时避免把持续很久但没有方向的横盘误判为“时间耗尽”。
    let net_move_is_opposite = match direction {
        MarketVelocityTradeDirection::Long => last_close < first_close,
        MarketVelocityTradeDirection::Short => last_close > first_close,
        MarketVelocityTradeDirection::Both => false,
    };
    Some(
        r_squared >= OPPOSITE_DURATION_MIN_R_SQUARED
            && net_move_is_opposite
            && match direction {
                MarketVelocityTradeDirection::Long => slope < 0.0,
                MarketVelocityTradeDirection::Short => slope > 0.0,
                MarketVelocityTradeDirection::Both => false,
            },
    )
}

/// 返回触发 K 线之前固定数量的完整历史，确保任何分支都不会读取未来 K 线。
fn history_before_latest(
    candles: &[ComputedCandle],
    latest_idx: usize,
    history_candles: usize,
) -> Option<&[ComputedCandle]> {
    let start = latest_idx.checked_sub(history_candles)?;
    candles
        .get(start..latest_idx)
        .filter(|history| history.len() == history_candles)
}

/// 检查当前反转簇的量能是否至少达到历史最强已确认极值簇。
///
/// 历史极值需要左右各三根 K 线确认，且整个比较区间截止于信号 K 线；因此回放和
/// 实盘在同一信号时点会得到相同结论，后续出现的放量 K 线不能反向改变入场决定。
pub(super) fn exhaustion_volume_dominance_filter_reason(
    candles: &[ComputedCandle],
    completed_count: usize,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<&'static str> {
    let dominance_ratio = args.entry_min_exhaustion_volume_dominance_ratio?;
    let Some(latest_idx) = completed_count.checked_sub(1) else {
        return Some("exhaustion_volume_not_ready");
    };
    let Some(current_start) = latest_idx
        .checked_add(1)
        .and_then(|count| count.checked_sub(EXHAUSTION_CURRENT_CLUSTER_CANDLES))
    else {
        return Some("exhaustion_volume_not_ready");
    };
    let Some(history_start) = latest_idx.checked_sub(EXHAUSTION_VOLUME_LOOKBACK_CANDLES) else {
        return Some("exhaustion_volume_not_ready");
    };
    let Some(first_pivot) = history_start.checked_add(EXHAUSTION_SWING_RADIUS_CANDLES) else {
        return Some("exhaustion_volume_not_ready");
    };
    let Some(last_pivot) = current_start.checked_sub(EXHAUSTION_SWING_RADIUS_CANDLES + 1) else {
        return Some("exhaustion_volume_not_ready");
    };
    if first_pivot > last_pivot {
        return Some("exhaustion_volume_not_ready");
    }
    let Some(current_volume) = cluster_max_volume(candles, current_start, latest_idx) else {
        return Some("exhaustion_volume_not_ready");
    };

    let mut strongest_historical_volume: Option<f64> = None;
    for pivot_idx in first_pivot..=last_pivot {
        if !is_confirmed_opposite_extreme(candles, pivot_idx, direction) {
            continue;
        }
        let cluster_start = pivot_idx - EXHAUSTION_SWING_RADIUS_CANDLES;
        let cluster_end = pivot_idx + EXHAUSTION_SWING_RADIUS_CANDLES;
        let Some(cluster_volume) = cluster_max_volume(candles, cluster_start, cluster_end) else {
            return Some("exhaustion_volume_not_ready");
        };
        strongest_historical_volume = Some(
            strongest_historical_volume
                .map(|volume| volume.max(cluster_volume))
                .unwrap_or(cluster_volume),
        );
    }
    let Some(strongest_historical_volume) = strongest_historical_volume else {
        return None;
    };
    (current_volume < strongest_historical_volume * dominance_ratio)
        .then_some("weaker_volume_than_previous_exhaustion_extreme")
}

/// 判断 pivot 是否为方向相反走势中的已确认局部极值。
fn is_confirmed_opposite_extreme(
    candles: &[ComputedCandle],
    pivot_idx: usize,
    direction: MarketVelocityTradeDirection,
) -> bool {
    let start = pivot_idx - EXHAUSTION_SWING_RADIUS_CANDLES;
    let end = pivot_idx + EXHAUSTION_SWING_RADIUS_CANDLES;
    let Some(window) = candles.get(start..=end) else {
        return false;
    };
    let Some(pivot) = candles.get(pivot_idx) else {
        return false;
    };
    match direction {
        MarketVelocityTradeDirection::Long => {
            window
                .iter()
                .all(|candle| pivot.candle.low <= candle.candle.low)
                && window
                    .iter()
                    .any(|candle| pivot.candle.low < candle.candle.low)
        }
        MarketVelocityTradeDirection::Short => {
            window
                .iter()
                .all(|candle| pivot.candle.high >= candle.candle.high)
                && window
                    .iter()
                    .any(|candle| pivot.candle.high > candle.candle.high)
        }
        MarketVelocityTradeDirection::Both => false,
    }
}

/// 返回闭区间内的最大有效成交量；任何异常量能都使门禁失败关闭。
fn cluster_max_volume(candles: &[ComputedCandle], start: usize, end: usize) -> Option<f64> {
    candles
        .get(start..=end)?
        .iter()
        .try_fold(0.0_f64, |maximum, computed| {
            valid_positive(computed.candle.volume).then_some(maximum.max(computed.candle.volume))
        })
}

/// 用信号时点已完成 K 线的量比选择 ATR14 止盈距离，并换算成现有回测器使用的 R。
pub(super) fn volume_atr_target_r(
    candles: &[BacktestCandle],
    volume_event_ts: i64,
    atr_completed_at_ts: i64,
    entry_price: f64,
    stop_loss_pct: f64,
) -> Option<f64> {
    let volume_idx = completed_candle_count_raw(candles, volume_event_ts).checked_sub(1)?;
    let volume_ratio = volume_ratio_at(candles, volume_idx, VOLUME_ATR_AVERAGE_CANDLES)?;
    if volume_ratio < VOLUME_ATR_BASE_VOLUME_RATIO {
        return None;
    }
    let atr_idx = completed_candle_count_raw(candles, atr_completed_at_ts).checked_sub(1)?;
    let atr = atr_at(candles, atr_idx, VOLUME_ATR_PERIOD)?;
    let atr_multiplier = if volume_ratio >= VOLUME_ATR_EXTREME_VOLUME_RATIO {
        VOLUME_ATR_EXTREME_MULTIPLIER
    } else if volume_ratio >= VOLUME_ATR_STRONG_VOLUME_RATIO {
        VOLUME_ATR_STRONG_MULTIPLIER
    } else {
        VOLUME_ATR_BASE_MULTIPLIER
    };
    let risk_distance = entry_price * stop_loss_pct;
    (valid_positive(risk_distance) && valid_positive(atr))
        .then_some(atr * atr_multiplier / risk_distance)
}

/// 在不读取任何未来 K 线的前提下，把原始 ATR 距离映射到预注册的风险收益带。
pub(super) fn volume_atr_target_r_with_policy(
    candles: &[BacktestCandle],
    volume_event_ts: i64,
    atr_completed_at_ts: i64,
    entry_price: f64,
    stop_loss_pct: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<f64> {
    let raw_target_r = volume_atr_target_r(
        candles,
        volume_event_ts,
        atr_completed_at_ts,
        entry_price,
        stop_loss_pct,
    )?;
    let mut target_r = raw_target_r * args.volume_atr_target_scale;
    if let Some(min_target_r) = args.volume_atr_min_target_r {
        target_r = target_r.max(min_target_r);
    }
    if let Some(max_target_r) = args.volume_atr_max_target_r {
        target_r = target_r.min(max_target_r);
    }
    Some(target_r)
}

pub(super) fn is_bearish_continuation_setup(
    candles: &[ComputedCandle],
    candle_idx: usize,
    min_volume_ratio: f64,
) -> bool {
    let Some(latest) = candles.get(candle_idx) else {
        return false;
    };
    let candle = &latest.candle;
    let range = candle.high - candle.low;
    if candle.close >= candle.open || !valid_positive(range) {
        return false;
    }
    let body_ratio = (candle.open - candle.close) / range;
    let close_from_low_ratio = (candle.close - candle.low) / range;
    let volume_ratio = latest
        .previous_volume_avg
        .filter(|average| valid_positive(*average))
        .map(|average| candle.volume / average);
    let Some(atr) = atr_at_computed(candles, candle_idx, VOLUME_ATR_PERIOD) else {
        return false;
    };
    body_ratio >= CONTINUATION_MIN_BODY_RATIO
        && range / atr >= CONTINUATION_MIN_RANGE_ATR_RATIO
        && close_from_low_ratio <= CONTINUATION_MAX_CLOSE_FROM_LOW_RATIO
        && (min_volume_ratio <= 0.0 || volume_ratio.is_some_and(|ratio| ratio >= min_volume_ratio))
}

/// 对称识别上涨末端的放量大阳线；此时做空仍属于接飞刀，必须等待转弱确认。
pub(super) fn is_bullish_continuation_setup(
    candles: &[ComputedCandle],
    candle_idx: usize,
    min_volume_ratio: f64,
) -> bool {
    let Some(latest) = candles.get(candle_idx) else {
        return false;
    };
    let candle = &latest.candle;
    let range = candle.high - candle.low;
    if candle.close <= candle.open || !valid_positive(range) {
        return false;
    }
    let body_ratio = (candle.close - candle.open) / range;
    let close_from_high_ratio = (candle.high - candle.close) / range;
    let volume_ratio = latest
        .previous_volume_avg
        .filter(|average| valid_positive(*average))
        .map(|average| candle.volume / average);
    let Some(atr) = atr_at_computed(candles, candle_idx, VOLUME_ATR_PERIOD) else {
        return false;
    };
    body_ratio >= CONTINUATION_MIN_BODY_RATIO
        && range / atr >= CONTINUATION_MIN_RANGE_ATR_RATIO
        && close_from_high_ratio <= CONTINUATION_MAX_CLOSE_FROM_LOW_RATIO
        && (min_volume_ratio <= 0.0 || volume_ratio.is_some_and(|ratio| ratio >= min_volume_ratio))
}

/// 要求放量信号本身已经形成方向相反的实体突破，避免把衰竭量误当成反转事实。
pub(super) fn opposite_reversal_confirmation_filter_reason(
    candles: &[ComputedCandle],
    candle_idx: usize,
    direction: MarketVelocityTradeDirection,
) -> Option<&'static str> {
    let current = candles.get(candle_idx).map(|item| &item.candle)?;
    let previous = candles
        .get(candle_idx.checked_sub(1)?)
        .map(|item| &item.candle)?;
    let confirmed = match direction {
        MarketVelocityTradeDirection::Long => bullish_reversal_confirmation(current, previous),
        MarketVelocityTradeDirection::Short => bearish_reversal_confirmation(current, previous),
        MarketVelocityTradeDirection::Both => false,
    };
    (!confirmed).then_some("opposite_reversal_candle_not_confirmed")
}

/// 反转确认还必须穿过短期平均成本，避免仅凭一根突破 K 线判定趋势已经反向。
pub(super) fn reversal_average_reclaim_filter_reason(
    candles: &[ComputedCandle],
    candle_idx: usize,
    direction: MarketVelocityTradeDirection,
) -> Option<&'static str> {
    let latest = candles.get(candle_idx)?;
    let (Some(sma), Some(ema)) = (latest.sma, latest.ema) else {
        return Some("reversal_average_not_ready");
    };
    let reclaimed = match direction {
        MarketVelocityTradeDirection::Long => {
            latest.candle.close > sma && latest.candle.close > ema
        }
        MarketVelocityTradeDirection::Short => {
            latest.candle.close < sma && latest.candle.close < ema
        }
        MarketVelocityTradeDirection::Both => false,
    };
    (!reclaimed).then_some("reversal_average_not_reclaimed")
}

pub(super) fn deferred_long_confirmation_entry_idx(
    candles: &[ComputedCandle],
    setup_idx: usize,
    max_wait_candles: usize,
    min_volume_ratio: f64,
) -> Result<usize, &'static str> {
    let setup = candles
        .get(setup_idx)
        .ok_or("deferred_reversal_missing_setup")?;
    let setup_atr = atr_at_computed(candles, setup_idx, VOLUME_ATR_PERIOD)
        .ok_or("deferred_reversal_atr_not_ready")?;
    let last_confirmation_idx = setup_idx
        .saturating_add(max_wait_candles)
        .min(candles.len().saturating_sub(2));
    for confirmation_idx in setup_idx + 1..=last_confirmation_idx {
        if is_bearish_continuation_setup(candles, confirmation_idx, min_volume_ratio) {
            return Err("deferred_reversal_reanchored");
        }
        let confirmation = &candles[confirmation_idx].candle;
        if confirmation.close < setup.candle.low - setup_atr * CONTINUATION_INVALIDATION_ATR {
            return Err("deferred_reversal_invalidated");
        }
        let previous = &candles[confirmation_idx - 1].candle;
        if bullish_reversal_confirmation(confirmation, previous) {
            return Ok(confirmation_idx + 1);
        }
    }
    Err("deferred_reversal_confirmation_not_found")
}

pub(super) fn deferred_short_confirmation_entry_idx(
    candles: &[ComputedCandle],
    setup_idx: usize,
    max_wait_candles: usize,
    min_volume_ratio: f64,
) -> Result<usize, &'static str> {
    let setup = candles
        .get(setup_idx)
        .ok_or("deferred_short_reversal_missing_setup")?;
    let setup_atr = atr_at_computed(candles, setup_idx, VOLUME_ATR_PERIOD)
        .ok_or("deferred_short_reversal_atr_not_ready")?;
    let last_confirmation_idx = setup_idx
        .saturating_add(max_wait_candles)
        .min(candles.len().saturating_sub(2));
    for confirmation_idx in setup_idx + 1..=last_confirmation_idx {
        if is_bullish_continuation_setup(candles, confirmation_idx, min_volume_ratio) {
            return Err("deferred_short_reversal_reanchored");
        }
        let confirmation = &candles[confirmation_idx].candle;
        if confirmation.high > setup.candle.high + setup_atr * CONTINUATION_INVALIDATION_ATR {
            return Err("deferred_short_reversal_invalidated");
        }
        let previous = &candles[confirmation_idx - 1].candle;
        if bearish_reversal_confirmation(confirmation, previous) {
            return Ok(confirmation_idx + 1);
        }
    }
    Err("deferred_short_reversal_confirmation_not_found")
}

fn bullish_reversal_confirmation(candle: &BacktestCandle, previous: &BacktestCandle) -> bool {
    let range = candle.high - candle.low;
    if candle.close <= candle.open || !valid_positive(range) {
        return false;
    }
    let body_ratio = (candle.close - candle.open) / range;
    let close_position_ratio = (candle.close - candle.low) / range;
    body_ratio >= REVERSAL_CONFIRMATION_MIN_BODY_RATIO
        && close_position_ratio >= REVERSAL_CONFIRMATION_MIN_CLOSE_POSITION_RATIO
        && candle.close > previous.high
}

fn bearish_reversal_confirmation(candle: &BacktestCandle, previous: &BacktestCandle) -> bool {
    let range = candle.high - candle.low;
    if candle.close >= candle.open || !valid_positive(range) {
        return false;
    }
    let body_ratio = (candle.open - candle.close) / range;
    let close_position_ratio = (candle.high - candle.close) / range;
    body_ratio >= REVERSAL_CONFIRMATION_MIN_BODY_RATIO
        && close_position_ratio >= REVERSAL_CONFIRMATION_MIN_CLOSE_POSITION_RATIO
        && candle.close < previous.low
}

fn atr_at_computed(candles: &[ComputedCandle], latest_idx: usize, period: usize) -> Option<f64> {
    let start = latest_idx.checked_add(1)?.checked_sub(period)?;
    let window = candles.get(start..=latest_idx)?;
    if window.len() != period {
        return None;
    }
    let mut tr_sum = 0.0;
    for (offset, computed) in window.iter().enumerate() {
        let candle = &computed.candle;
        if !valid_ohlc(candle) {
            return None;
        }
        let previous_close = if offset == 0 {
            start
                .checked_sub(1)
                .and_then(|idx| candles.get(idx))
                .map(|previous| previous.candle.close)
        } else {
            Some(window[offset - 1].candle.close)
        };
        tr_sum += true_range(candle, previous_close);
    }
    let atr = tr_sum / period as f64;
    valid_positive(atr).then_some(atr)
}

fn completed_candle_count_raw(candles: &[BacktestCandle], event_ts: i64) -> usize {
    candles.partition_point(|candle| candle.ts.saturating_add(MS_15M) <= event_ts)
}

fn volume_ratio_at(candles: &[BacktestCandle], latest_idx: usize, period: usize) -> Option<f64> {
    let start = latest_idx.checked_sub(period)?;
    let previous = candles.get(start..latest_idx)?;
    let latest_volume = candles.get(latest_idx)?.volume;
    if previous.len() != period || !valid_positive(latest_volume) {
        return None;
    }
    let mut volume_sum = 0.0;
    for candle in previous {
        if !valid_positive(candle.volume) {
            return None;
        }
        volume_sum += candle.volume;
    }
    let average = volume_sum / period as f64;
    valid_positive(average).then_some(latest_volume / average)
}

fn atr_at(candles: &[BacktestCandle], latest_idx: usize, period: usize) -> Option<f64> {
    let start = latest_idx.checked_add(1)?.checked_sub(period)?;
    let window = candles.get(start..=latest_idx)?;
    if window.len() != period {
        return None;
    }
    let mut tr_sum = 0.0;
    for (offset, candle) in window.iter().enumerate() {
        if !valid_ohlc(candle) {
            return None;
        }
        let previous_close = if offset == 0 {
            start
                .checked_sub(1)
                .and_then(|idx| candles.get(idx))
                .map(|previous| previous.close)
        } else {
            Some(window[offset - 1].close)
        };
        let true_range = true_range(candle, previous_close);
        if !valid_positive(true_range) {
            return None;
        }
        tr_sum += true_range;
    }
    let atr = tr_sum / period as f64;
    valid_positive(atr).then_some(atr)
}

fn true_range(candle: &BacktestCandle, previous_close: Option<f64>) -> f64 {
    let high_low = candle.high - candle.low;
    previous_close
        .filter(|close| valid_positive(*close))
        .map(|close| {
            high_low
                .max((candle.high - close).abs())
                .max((candle.low - close).abs())
        })
        .unwrap_or(high_low)
}

fn valid_ohlc(candle: &BacktestCandle) -> bool {
    valid_positive(candle.open)
        && valid_positive(candle.high)
        && valid_positive(candle.low)
        && valid_positive(candle.close)
        && candle.high >= candle.low
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
