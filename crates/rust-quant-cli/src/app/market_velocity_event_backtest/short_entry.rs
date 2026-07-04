use super::ComputedCandle;

const SIDEWAYS_RANGE_BREAK_LOOKBACK_CANDLES: usize = 8;
const SIDEWAYS_RANGE_BREAK_MAX_WIDTH_PCT: f64 = 3.0;
const SIDEWAYS_RANGE_BREAK_MIN_CLOSE_BELOW_LOW_PCT: f64 = 0.2;
const LONG_SUPPORT_BREAK_LOOKBACK_CANDLES: usize = 48;
const LONG_SUPPORT_BREAK_RECENT_TOUCH_LOOKBACK_CANDLES: usize = 12;
const LONG_SUPPORT_BREAK_TOUCH_TOLERANCE_PCT: f64 = 0.6;
const LONG_SUPPORT_BREAK_MIN_TOUCH_GROUPS: usize = 3;
const LONG_SUPPORT_BREAK_MAX_PRIOR_BODY_BREAKS: usize = 4;
const SUPPORT_TOUCH_GROUP_MIN_GAP_CANDLES: usize = 2;

/// 识别窄幅横盘后的放量下沿破位；该触发只给 short-side 动量异动研究打标签。
pub(super) fn sideways_range_breakdown_candidate(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> bool {
    narrow_recent_range_breakdown_candidate(candles, latest_idx)
        || long_15m_support_breakdown_candidate(candles, latest_idx)
}

/// 识别最近 8 根内的窄幅横盘下沿跌破，保留原短窗口触发语义。
fn narrow_recent_range_breakdown_candidate(candles: &[ComputedCandle], latest_idx: usize) -> bool {
    let Some(start) = latest_idx.checked_sub(SIDEWAYS_RANGE_BREAK_LOOKBACK_CANDLES) else {
        return false;
    };
    let Some(latest) = candles.get(latest_idx).map(|value| &value.candle) else {
        return false;
    };
    if latest.close >= latest.open {
        return false;
    }
    let range = &candles[start..latest_idx];
    let Some((range_high, range_low)) = high_low_for_computed_range(range) else {
        return false;
    };
    let Some(range_width_pct) = pct_distance(range_high, range_low, range_low) else {
        return false;
    };
    let Some(close_break_pct) = pct_distance(range_low, latest.close, range_low) else {
        return false;
    };
    range_width_pct <= SIDEWAYS_RANGE_BREAK_MAX_WIDTH_PCT
        && latest.close < range_low
        && latest.low < range_low
        && close_break_pct >= SIDEWAYS_RANGE_BREAK_MIN_CLOSE_BELOW_LOW_PCT
}

/// 识别更长 15m 横盘支撑被跌破的场景，覆盖多次回踩同一支撑后向下破位的形态。
fn long_15m_support_breakdown_candidate(candles: &[ComputedCandle], latest_idx: usize) -> bool {
    let Some(start) = latest_idx.checked_sub(LONG_SUPPORT_BREAK_LOOKBACK_CANDLES) else {
        return false;
    };
    let Some(latest) = candles.get(latest_idx).map(|value| &value.candle) else {
        return false;
    };
    if latest.close >= latest.open {
        return false;
    }
    let history = &candles[start..latest_idx];
    let Some(support_level) = long_15m_support_level(history, latest.close) else {
        return false;
    };
    let Some(close_break_pct) = pct_distance(support_level, latest.close, support_level) else {
        return false;
    };
    latest.close < support_level
        && latest.low < support_level
        && close_break_pct >= SIDEWAYS_RANGE_BREAK_MIN_CLOSE_BELOW_LOW_PCT
}

/// 从更长 15m 历史中选择被多次触碰且最近仍被测试过的水平支撑位。
fn long_15m_support_level(candles: &[ComputedCandle], latest_close: f64) -> Option<f64> {
    let recent_start = candles
        .len()
        .saturating_sub(LONG_SUPPORT_BREAK_RECENT_TOUCH_LOOKBACK_CANDLES);
    let mut best_level = None;
    let mut best_touch_groups = 0;
    for candidate in candles
        .iter()
        .map(|value| candle_body_low(&value.candle))
        .filter(|candidate| valid_positive(*candidate) && latest_close < *candidate)
    {
        let touch_groups = support_touch_groups(candles, candidate);
        if touch_groups < LONG_SUPPORT_BREAK_MIN_TOUCH_GROUPS {
            continue;
        }
        if !candles[recent_start..]
            .iter()
            .any(|candle| candle_touches_support(candle, candidate))
        {
            continue;
        }
        let prior_body_breaks = candles
            .iter()
            .filter(|candle| prior_close_breaks_support(candle, candidate))
            .count();
        if prior_body_breaks > LONG_SUPPORT_BREAK_MAX_PRIOR_BODY_BREAKS {
            continue;
        }
        if touch_groups > best_touch_groups {
            best_level = Some(candidate);
            best_touch_groups = touch_groups;
        }
    }
    best_level
}

/// 按触碰簇计数，避免连续几根贴线 K 线被重复算成多个独立支撑确认。
fn support_touch_groups(candles: &[ComputedCandle], support_level: f64) -> usize {
    let mut groups = 0;
    let mut last_touch_idx = None;
    for (idx, candle) in candles.iter().enumerate() {
        if !candle_touches_support(candle, support_level) {
            continue;
        }
        let starts_new_group = match last_touch_idx {
            Some(last_idx) => idx.saturating_sub(last_idx) > SUPPORT_TOUCH_GROUP_MIN_GAP_CANDLES,
            None => true,
        };
        if starts_new_group {
            groups += 1;
        }
        last_touch_idx = Some(idx);
    }
    groups
}

/// 判断单根 K 线是否在支撑位附近完成触碰且未明显收在支撑下方。
fn candle_touches_support(candle: &ComputedCandle, support_level: f64) -> bool {
    if !valid_positive(support_level) {
        return false;
    }
    let tolerance = support_level * LONG_SUPPORT_BREAK_TOUCH_TOLERANCE_PCT / 100.0;
    let body_low = candle_body_low(&candle.candle);
    candle.candle.low <= support_level + tolerance
        && body_low <= support_level + tolerance
        && candle.candle.close >= support_level - tolerance
}

/// 统计最新破位前已经明确收破支撑的次数，避免把长期下跌趋势误判成横盘破位。
fn prior_close_breaks_support(candle: &ComputedCandle, support_level: f64) -> bool {
    if !valid_positive(support_level) {
        return false;
    }
    candle.candle.close
        < support_level * (1.0 - SIDEWAYS_RANGE_BREAK_MIN_CLOSE_BELOW_LOW_PCT / 100.0)
}

/// 取实体下沿作为候选支撑位，降低单根长下影线对水平位的干扰。
fn candle_body_low(candle: &super::BacktestCandle) -> f64 {
    candle.open.min(candle.close)
}

fn high_low_for_computed_range(candles: &[ComputedCandle]) -> Option<(f64, f64)> {
    let mut high = f64::NEG_INFINITY;
    let mut low = f64::INFINITY;
    for candle in candles {
        if !candle.candle.high.is_finite() || !candle.candle.low.is_finite() {
            return None;
        }
        high = high.max(candle.candle.high);
        low = low.min(candle.candle.low);
    }
    (valid_positive(high) && valid_positive(low) && high > low).then_some((high, low))
}

fn pct_distance(anchor: f64, value: f64, denominator: f64) -> Option<f64> {
    if !anchor.is_finite() || !value.is_finite() || !valid_positive(denominator) {
        return None;
    }
    Some((anchor - value).abs() / denominator * 100.0)
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        build_computed_candles, BacktestCandle, MS_15M,
    };

    fn ohlcv(
        idx: usize,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> BacktestCandle {
        BacktestCandle {
            ts: MS_15M * idx as i64,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    fn repeated_support_history(touch_indices: &[usize]) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for idx in 0..LONG_SUPPORT_BREAK_LOOKBACK_CANDLES {
            if touch_indices.contains(&idx) {
                candles.push(ohlcv(idx, 102.0, 105.6, 99.7, 100.25, 10.0));
                continue;
            }
            let base = 104.0 + (idx % 6) as f64 * 0.45;
            candles.push(ohlcv(idx, base + 0.6, base + 2.2, base - 0.4, base, 10.0));
        }
        candles
    }

    fn isolated_support_history(touch_indices: &[usize]) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for idx in 0..LONG_SUPPORT_BREAK_LOOKBACK_CANDLES {
            if touch_indices.contains(&idx) {
                candles.push(ohlcv(idx, 102.0, 105.6, 99.7, 100.25, 10.0));
                continue;
            }
            let base = 110.0 + idx as f64 * 2.0;
            candles.push(ohlcv(idx, base + 0.8, base + 2.2, base - 0.3, base, 10.0));
        }
        candles
    }

    #[test]
    fn long_15m_support_breakdown_accepts_repeated_level_without_narrow_recent_range() {
        let mut candles = repeated_support_history(&[2, 13, 24, 37, 45]);
        candles.push(ohlcv(48, 100.4, 100.7, 98.4, 98.85, 28.0));
        let computed = build_computed_candles(candles, 3);

        assert!(!narrow_recent_range_breakdown_candidate(&computed, 48));
        assert!(sideways_range_breakdown_candidate(&computed, 48));
    }

    #[test]
    fn long_15m_support_breakdown_rejects_single_recent_touch() {
        let mut candles = isolated_support_history(&[45]);
        candles.push(ohlcv(48, 100.4, 100.7, 98.4, 98.85, 28.0));
        let computed = build_computed_candles(candles, 3);

        assert!(!narrow_recent_range_breakdown_candidate(&computed, 48));
        assert!(!sideways_range_breakdown_candidate(&computed, 48));
    }
}
