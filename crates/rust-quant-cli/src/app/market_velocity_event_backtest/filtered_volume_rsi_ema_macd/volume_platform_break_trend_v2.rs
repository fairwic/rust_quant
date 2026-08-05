use super::super::{
    ComputedCandle, IsolatedStrategyFamilySignalEvidence, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection, PlatformBreakdownSignalEvidence, MS_15M,
};
use super::isolated_family_common::{
    current_volume_gate, fixed_one_r_signal, ISOLATED_FILTERED_VOLUME_MIN_RATIO,
};
use super::volume_platform_break_trend_v1::{
    long_term_ema_confirmation, PLATFORM_CONFIRMATION_CANDLES, PLATFORM_LOOKBACK_CANDLES,
    PLATFORM_MAX_RANGE_ATR, PLATFORM_MIN_BODY_OPEN_RATIO, PLATFORM_MIN_BODY_RANGE_RATIO,
};
use super::FilteredVolumeRsiEmaMacdSignal;

/// 前五根与后五根收盘均值最多允许偏移一个破位前 ATR。
pub(crate) const PLATFORM_V2_MAX_CENTER_SHIFT_ATR: f64 = 1.0;
/// 收盘回归达到该拟合度后，显著单向漂移会被识别为趋势而非平台。
pub(crate) const PLATFORM_V2_TREND_R_SQUARED_MIN: f64 = 0.70;
/// 高拟合窗口的拟合首尾漂移最多允许 0.75 个破位前 ATR。
pub(crate) const PLATFORM_V2_MAX_FITTED_DRIFT_ATR: f64 = 0.75;
/// 上下沿触碰区各占平台完整宽度的 10%。
pub(crate) const PLATFORM_V2_TOUCH_ZONE_WIDTH_RATIO: f64 = 0.10;
/// 同侧两次有效触碰的索引至少相隔三根，拒绝相邻极值伪装多次触碰。
pub(crate) const PLATFORM_V2_MIN_TOUCH_SEPARATION_CANDLES: usize = 3;
/// V2 的最小假设标识；用于证明只改变平台质量定义。
pub(crate) const PLATFORM_BREAK_TREND_V2_HYPOTHESIS: &str =
    "horizontal_multi_touch_platform_break_two_close_acceptance_plus_ema696_trend";

const LONG_TRIGGER: &str = "horizontal_platform_break_ema_trend_long_v2";
const SHORT_TRIGGER: &str = "horizontal_platform_break_ema_trend_short_v2";

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlatformQuality {
    low: f64,
    high: f64,
    reference_atr14: f64,
    reference_atr_ts_ms: i64,
    range_atr: f64,
    center_shift_atr: f64,
    regression_r_squared: f64,
    fitted_drift_atr: f64,
    upper_touch_count: usize,
    lower_touch_count: usize,
}

/// 保留 V1 破位与趋势确认，仅把“20 根极值窗口”收紧为水平、分散多次触碰的平台。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - ISOLATED_FILTERED_VOLUME_MIN_RATIO).abs() > f64::EPSILON {
        return Err("platform_break_trend_v2_ratio_policy_mismatch");
    }
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("platform_break_trend_v2_not_ready")?;
    let break_idx = latest_idx
        .checked_sub(PLATFORM_CONFIRMATION_CANDLES)
        .ok_or("platform_break_trend_v2_not_ready")?;
    let break_candle = candles
        .get(break_idx)
        .ok_or("platform_break_trend_v2_not_ready")?;
    let first_confirmation = candles
        .get(break_idx + 1)
        .ok_or("platform_break_trend_v2_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("platform_break_trend_v2_not_ready")?;
    if first_confirmation.candle.ts != break_candle.candle.ts + MS_15M
        || latest.candle.ts != first_confirmation.candle.ts + MS_15M
    {
        return Err("platform_break_trend_v2_confirmation_not_continuous");
    }

    let (volume, weekly_volume) = current_volume_gate(candles, break_idx)
        .map_err(|_| "platform_break_trend_v2_volume_not_confirmed")?;
    let platform = platform_quality(candles, break_idx)?;
    let open = break_candle.candle.open;
    let close = break_candle.candle.close;
    let range = break_candle.candle.high - break_candle.candle.low;
    let body = (close - open).abs();
    if !open.is_finite()
        || open <= 0.0
        || !range.is_finite()
        || range <= 0.0
        || body / range < PLATFORM_MIN_BODY_RANGE_RATIO
        || body / open < PLATFORM_MIN_BODY_OPEN_RATIO
    {
        return Err("platform_break_trend_v2_break_body_not_confirmed");
    }

    let direction = if close < open
        && close < platform.low
        && first_confirmation.candle.close < platform.low
        && latest.candle.close < platform.low
    {
        MarketVelocityTradeDirection::Short
    } else if close > open
        && close > platform.high
        && first_confirmation.candle.close > platform.high
        && latest.candle.close > platform.high
    {
        MarketVelocityTradeDirection::Long
    } else {
        return Err("platform_break_trend_v2_acceptance_not_confirmed");
    };
    let (bearish_ema, bullish_ema, ema696_recent) = long_term_ema_confirmation(candles, latest_idx);
    let ema_confirmed = match direction {
        MarketVelocityTradeDirection::Long => bullish_ema,
        MarketVelocityTradeDirection::Short => bearish_ema,
        MarketVelocityTradeDirection::Both => false,
    };
    if !ema_confirmed {
        return Err("platform_break_trend_v2_ema_not_confirmed");
    }
    let direction_label = match direction {
        MarketVelocityTradeDirection::Long => "bullish",
        MarketVelocityTradeDirection::Short => "bearish",
        MarketVelocityTradeDirection::Both => unreachable!(),
    };
    let platform_breakdown = PlatformBreakdownSignalEvidence {
        direction: direction_label,
        break_ts_ms: break_candle.candle.ts,
        confirmed_ts_ms: latest.candle.ts,
        platform_high: platform.high,
        platform_low: platform.low,
        platform_range_atr: platform.range_atr,
        atr_reference_ts_ms: Some(platform.reference_atr_ts_ms),
        platform_reference_atr14: Some(platform.reference_atr14),
        close_center_shift_atr: Some(platform.center_shift_atr),
        close_regression_r_squared: Some(platform.regression_r_squared),
        fitted_close_drift_atr: Some(platform.fitted_drift_atr),
        upper_touch_count: Some(platform.upper_touch_count),
        lower_touch_count: Some(platform.lower_touch_count),
        break_body_range_ratio: body / range,
        break_body_open_ratio: body / open,
        filtered_volume_ratio: volume.ratio,
        current_volume_ccy: weekly_volume.current,
        weekly_volume_ccy_p90: weekly_volume.p90,
    };

    fixed_one_r_signal(
        latest,
        direction,
        match direction {
            MarketVelocityTradeDirection::Long => LONG_TRIGGER,
            MarketVelocityTradeDirection::Short => SHORT_TRIGGER,
            MarketVelocityTradeDirection::Both => unreachable!(),
        },
        volume,
        weekly_volume,
        Vec::new(),
        None,
        IsolatedStrategyFamilySignalEvidence {
            family: "volume_platform_break_trend",
            hypothesis: PLATFORM_BREAK_TREND_V2_HYPOTHESIS,
            prior_96_net_move_pct: None,
            platform_breakdown: Some(platform_breakdown),
            long_term_ema_confirmed: true,
            ema696_recent,
        },
    )
}

/// 使用破位前一根 ATR 归一化宽度，并校验水平性与上下沿分散触碰。
fn platform_quality(
    candles: &[ComputedCandle],
    break_idx: usize,
) -> Result<PlatformQuality, &'static str> {
    let start = break_idx
        .checked_sub(PLATFORM_LOOKBACK_CANDLES)
        .ok_or("platform_break_trend_v2_platform_not_ready")?;
    let platform = candles
        .get(start..break_idx)
        .filter(|window| window.len() == PLATFORM_LOOKBACK_CANDLES)
        .ok_or("platform_break_trend_v2_platform_not_ready")?;
    let reference = candles
        .get(
            break_idx
                .checked_sub(1)
                .ok_or("platform_break_trend_v2_atr_not_ready")?,
        )
        .ok_or("platform_break_trend_v2_atr_not_ready")?;
    let reference_atr14 = reference
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("platform_break_trend_v2_atr_not_ready")?;
    let low = platform
        .iter()
        .map(|candle| candle.candle.low)
        .filter(|value| value.is_finite() && *value > 0.0)
        .min_by(f64::total_cmp)
        .ok_or("platform_break_trend_v2_platform_not_ready")?;
    let high = platform
        .iter()
        .map(|candle| candle.candle.high)
        .filter(|value| value.is_finite() && *value > 0.0)
        .max_by(f64::total_cmp)
        .ok_or("platform_break_trend_v2_platform_not_ready")?;
    let width = high - low;
    let range_atr = width / reference_atr14;
    if !width.is_finite()
        || width <= 0.0
        || !range_atr.is_finite()
        || range_atr > PLATFORM_MAX_RANGE_ATR
    {
        return Err("platform_break_trend_v2_platform_too_wide");
    }

    let first_mean = mean_close(&platform[..5])?;
    let last_mean = mean_close(&platform[PLATFORM_LOOKBACK_CANDLES - 5..])?;
    let center_shift_atr = (last_mean - first_mean).abs() / reference_atr14;
    if center_shift_atr > PLATFORM_V2_MAX_CENTER_SHIFT_ATR {
        return Err("platform_break_trend_v2_center_shift_rejected");
    }
    let (regression_r_squared, fitted_drift_atr) =
        close_regression_quality(platform, reference_atr14)?;
    if regression_r_squared >= PLATFORM_V2_TREND_R_SQUARED_MIN
        && fitted_drift_atr > PLATFORM_V2_MAX_FITTED_DRIFT_ATR
    {
        return Err("platform_break_trend_v2_regression_drift_rejected");
    }

    let zone = width * PLATFORM_V2_TOUCH_ZONE_WIDTH_RATIO;
    let upper_touches = platform
        .iter()
        .enumerate()
        .filter_map(|(idx, candle)| (candle.candle.high >= high - zone).then_some(idx))
        .collect::<Vec<_>>();
    let lower_touches = platform
        .iter()
        .enumerate()
        .filter_map(|(idx, candle)| (candle.candle.low <= low + zone).then_some(idx))
        .collect::<Vec<_>>();
    if !has_dispersed_touch_pair(&upper_touches) || !has_dispersed_touch_pair(&lower_touches) {
        return Err("platform_break_trend_v2_dispersed_touches_not_confirmed");
    }

    Ok(PlatformQuality {
        low,
        high,
        reference_atr14,
        reference_atr_ts_ms: reference.candle.ts,
        range_atr,
        center_shift_atr,
        regression_r_squared,
        fitted_drift_atr,
        upper_touch_count: upper_touches.len(),
        lower_touch_count: lower_touches.len(),
    })
}

/// 计算一段 K 线的收盘均值；任何非有限价格都让平台候选失败关闭。
fn mean_close(candles: &[ComputedCandle]) -> Result<f64, &'static str> {
    let mut sum = 0.0;
    for candle in candles {
        let close = candle.candle.close;
        if !close.is_finite() || close <= 0.0 {
            return Err("platform_break_trend_v2_close_invalid");
        }
        sum += close;
    }
    Ok(sum / candles.len() as f64)
}

/// 返回收盘线性回归 R² 与拟合首尾漂移的 ATR 倍数。
fn close_regression_quality(
    platform: &[ComputedCandle],
    reference_atr14: f64,
) -> Result<(f64, f64), &'static str> {
    let mean_x = (platform.len() - 1) as f64 / 2.0;
    let mean_y = mean_close(platform)?;
    let mut ss_xx = 0.0;
    let mut ss_yy = 0.0;
    let mut ss_xy = 0.0;
    for (idx, candle) in platform.iter().enumerate() {
        let dx = idx as f64 - mean_x;
        let dy = candle.candle.close - mean_y;
        ss_xx += dx * dx;
        ss_yy += dy * dy;
        ss_xy += dx * dy;
    }
    if ss_xx <= f64::EPSILON {
        return Err("platform_break_trend_v2_regression_not_ready");
    }
    let slope = ss_xy / ss_xx;
    let r_squared = if ss_yy <= f64::EPSILON {
        0.0
    } else {
        (ss_xy * ss_xy / (ss_xx * ss_yy)).clamp(0.0, 1.0)
    };
    let fitted_drift_atr = slope.abs() * (platform.len() - 1) as f64 / reference_atr14;
    Ok((r_squared, fitted_drift_atr))
}

/// 同侧至少存在一对索引间距不小于三根的触碰。
fn has_dispersed_touch_pair(touches: &[usize]) -> bool {
    touches.iter().enumerate().any(|(left_idx, left)| {
        touches[left_idx + 1..]
            .iter()
            .any(|right| right.saturating_sub(*left) >= PLATFORM_V2_MIN_TOUCH_SEPARATION_CANDLES)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::market_volume_platform_break_trend_v2_research_args;
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    fn bearish_setup() -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        let break_idx = latest_idx - PLATFORM_CONFIRMATION_CANDLES;
        candles[break_idx].candle = BacktestCandle {
            ts: break_idx as i64 * MS_15M,
            open: 100.0,
            high: 100.2,
            low: 97.5,
            close: 98.0,
            volume: 25.0,
        };
        candles[break_idx].volume_ccy = Some(200.0);
        candles[break_idx + 1].candle.close = 98.2;
        candles[latest_idx].candle.close = 98.4;
        for (offset, idx) in ((latest_idx - 3)..=latest_idx).enumerate() {
            candles[idx].ema696 = Some(104.0 - offset as f64);
        }
        for (offset, idx) in (break_idx..=latest_idx).enumerate() {
            candles[idx].ema12 = Some(90.0 - offset as f64 * 0.5);
            candles[idx].ema144 = Some(95.0 - offset as f64 * 0.5);
            candles[idx].ema169 = Some(98.0 - offset as f64 * 0.5);
        }
        candles
    }

    #[test]
    fn valid_horizontal_multi_touch_platform_keeps_v1_break_and_ema_contract() {
        let candles = bearish_setup();
        let args = market_volume_platform_break_trend_v2_research_args().unwrap();
        let signal = signal(&candles, candles.len(), &args).unwrap();
        let platform = signal
            .evidence
            .isolated_family
            .as_ref()
            .and_then(|family| family.platform_breakdown.as_ref())
            .unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(
            platform.atr_reference_ts_ms,
            Some((candles.len() - 4) as i64 * MS_15M)
        );
        assert_eq!(platform.upper_touch_count, Some(20));
        assert_eq!(platform.lower_touch_count, Some(20));
    }

    #[test]
    fn width_uses_pre_break_atr_instead_of_break_candle_atr() {
        let mut candles = bearish_setup();
        let latest_idx = candles.len() - 1;
        let break_idx = latest_idx - PLATFORM_CONFIRMATION_CANDLES;
        candles[break_idx - 1].atr14 = Some(0.4);
        candles[break_idx].atr14 = Some(100.0);
        let args = market_volume_platform_break_trend_v2_research_args().unwrap();

        assert_eq!(
            signal(&candles, candles.len(), &args),
            Err("platform_break_trend_v2_platform_too_wide")
        );
    }

    #[test]
    fn high_r_squared_downward_center_is_not_a_platform() {
        let mut candles = bearish_setup();
        let break_idx = candles.len() - 1 - PLATFORM_CONFIRMATION_CANDLES;
        let start = break_idx - PLATFORM_LOOKBACK_CANDLES;
        for offset in 0..PLATFORM_LOOKBACK_CANDLES {
            let close = 101.0 - offset as f64 * 0.10;
            candles[start + offset].candle.open = close;
            candles[start + offset].candle.close = close;
            candles[start + offset].candle.high = close + 0.3;
            candles[start + offset].candle.low = close - 0.3;
        }

        assert_eq!(
            platform_quality(&candles, break_idx),
            Err("platform_break_trend_v2_regression_drift_rejected")
        );
    }

    #[test]
    fn adjacent_extremes_do_not_count_as_dispersed_touches() {
        let mut candles = bearish_setup();
        let break_idx = candles.len() - 1 - PLATFORM_CONFIRMATION_CANDLES;
        let start = break_idx - PLATFORM_LOOKBACK_CANDLES;
        for offset in 0..PLATFORM_LOOKBACK_CANDLES {
            candles[start + offset].candle.high = 100.2;
            candles[start + offset].candle.low = 99.8;
        }
        candles[start].candle.high = 101.0;
        candles[start + 1].candle.high = 101.0;
        candles[start + 8].candle.low = 99.0;
        candles[start + 12].candle.low = 99.0;

        assert_eq!(
            platform_quality(&candles, break_idx),
            Err("platform_break_trend_v2_dispersed_touches_not_confirmed")
        );
    }
}
