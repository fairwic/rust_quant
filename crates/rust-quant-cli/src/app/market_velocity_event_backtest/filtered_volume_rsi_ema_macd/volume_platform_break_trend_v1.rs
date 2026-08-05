use super::super::{
    ComputedCandle, IsolatedStrategyFamilySignalEvidence, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection, PlatformBreakdownSignalEvidence, MS_15M,
};
use super::isolated_family_common::{
    current_volume_gate, fixed_one_r_signal, ISOLATED_FILTERED_VOLUME_MIN_RATIO,
};
use super::FilteredVolumeRsiEmaMacdSignal;

/// 平台只由破位 K 之前精确 20 根已完成 K 线定义。
pub(crate) const PLATFORM_LOOKBACK_CANDLES: usize = 20;
/// 平台完整宽度最多为破位 K ATR14 的四倍。
pub(crate) const PLATFORM_MAX_RANGE_ATR: f64 = 4.0;
/// 破位后必须有两根已完成 K 线继续收在平台外。
pub(crate) const PLATFORM_CONFIRMATION_CANDLES: usize = 2;
/// 破位实体至少占完整振幅 60%。
pub(crate) const PLATFORM_MIN_BODY_RANGE_RATIO: f64 = 0.60;
/// 破位实体相对开盘价至少为 1%。
pub(crate) const PLATFORM_MIN_BODY_OPEN_RATIO: f64 = 0.01;
/// 独立趋势家族的冻结最小假设标识。
pub(crate) const PLATFORM_BREAK_TREND_HYPOTHESIS: &str =
    "abnormal_volume_platform_break_two_close_acceptance_plus_ema696_trend";

const LONG_TRIGGER: &str = "volume_platform_break_ema_trend_long";
const SHORT_TRIGGER: &str = "volume_platform_break_ema_trend_short";

/// 只在新的平台破位完成两根接受确认且长期 EMA 同向时产生一次顺势信号。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - ISOLATED_FILTERED_VOLUME_MIN_RATIO).abs() > f64::EPSILON {
        return Err("platform_break_trend_ratio_policy_mismatch");
    }
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("platform_break_trend_not_ready")?;
    let break_idx = latest_idx
        .checked_sub(PLATFORM_CONFIRMATION_CANDLES)
        .ok_or("platform_break_trend_not_ready")?;
    let break_candle = candles
        .get(break_idx)
        .ok_or("platform_break_trend_not_ready")?;
    let first_confirmation = candles
        .get(break_idx + 1)
        .ok_or("platform_break_trend_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("platform_break_trend_not_ready")?;
    if first_confirmation.candle.ts != break_candle.candle.ts + MS_15M
        || latest.candle.ts != first_confirmation.candle.ts + MS_15M
    {
        return Err("platform_break_trend_confirmation_not_continuous");
    }

    let (volume, weekly_volume) = current_volume_gate(candles, break_idx)
        .map_err(|_| "platform_break_trend_volume_not_confirmed")?;
    let atr14 = break_candle
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("platform_break_trend_atr_not_ready")?;
    let (platform_low, platform_high) =
        platform_bounds(candles, break_idx).ok_or("platform_break_trend_platform_not_ready")?;
    let platform_range_atr = (platform_high - platform_low) / atr14;
    if !platform_range_atr.is_finite() || platform_range_atr > PLATFORM_MAX_RANGE_ATR {
        return Err("platform_break_trend_platform_too_wide");
    }

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
        return Err("platform_break_trend_break_body_not_confirmed");
    }

    let direction = if close < open
        && close < platform_low
        && first_confirmation.candle.close < platform_low
        && latest.candle.close < platform_low
    {
        MarketVelocityTradeDirection::Short
    } else if close > open
        && close > platform_high
        && first_confirmation.candle.close > platform_high
        && latest.candle.close > platform_high
    {
        MarketVelocityTradeDirection::Long
    } else {
        return Err("platform_break_trend_acceptance_not_confirmed");
    };
    let (bearish_ema, bullish_ema, ema696_recent) = long_term_ema_confirmation(candles, latest_idx);
    let ema_confirmed = match direction {
        MarketVelocityTradeDirection::Long => bullish_ema,
        MarketVelocityTradeDirection::Short => bearish_ema,
        MarketVelocityTradeDirection::Both => false,
    };
    if !ema_confirmed {
        return Err("platform_break_trend_ema_not_confirmed");
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
        platform_high,
        platform_low,
        platform_range_atr,
        atr_reference_ts_ms: None,
        platform_reference_atr14: None,
        close_center_shift_atr: None,
        close_regression_r_squared: None,
        fitted_close_drift_atr: None,
        upper_touch_count: None,
        lower_touch_count: None,
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
            hypothesis: PLATFORM_BREAK_TREND_HYPOTHESIS,
            prior_96_net_move_pct: None,
            platform_breakdown: Some(platform_breakdown),
            long_term_ema_confirmed: true,
            ema696_recent,
        },
    )
}

/// 计算破位 K 之前精确 20 根的最高/最低边界。
fn platform_bounds(candles: &[ComputedCandle], break_idx: usize) -> Option<(f64, f64)> {
    let start = break_idx.checked_sub(PLATFORM_LOOKBACK_CANDLES)?;
    let platform = candles.get(start..break_idx)?;
    if platform.len() != PLATFORM_LOOKBACK_CANDLES {
        return None;
    }
    let low = platform
        .iter()
        .map(|candle| candle.candle.low)
        .filter(|value| value.is_finite() && *value > 0.0)
        .min_by(f64::total_cmp)?;
    let high = platform
        .iter()
        .map(|candle| candle.candle.high)
        .filter(|value| value.is_finite() && *value > 0.0)
        .max_by(f64::total_cmp)?;
    (high > low).then_some((low, high))
}

/// 最近三根必须保持同向 EMA 顺序，同时最近四个 EMA696 必须逐根同向变化。
pub(super) fn long_term_ema_confirmation(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> (bool, bool, Vec<f64>) {
    let Some(first_ordered_idx) = latest_idx.checked_sub(2) else {
        return (false, false, Vec::new());
    };
    let Some(previous_idx) = first_ordered_idx.checked_sub(1) else {
        return (false, false, Vec::new());
    };
    let recent = (previous_idx..=latest_idx)
        .filter_map(|idx| candles.get(idx).and_then(|candle| candle.ema696))
        .collect::<Vec<_>>();
    if recent.len() != 4
        || recent
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return (false, false, recent);
    }
    let Some(ordered) = candles.get(first_ordered_idx..=latest_idx) else {
        return (false, false, recent);
    };
    let bearish_order = ordered.iter().all(|candle| {
        ordered_emas(candle).is_some_and(|(ema12, ema144, ema169, ema696)| {
            ema12 < ema144 && ema144 < ema169 && ema169 < ema696
        })
    });
    let bullish_order = ordered.iter().all(|candle| {
        ordered_emas(candle).is_some_and(|(ema12, ema144, ema169, ema696)| {
            ema12 > ema144 && ema144 > ema169 && ema169 > ema696
        })
    });
    let ema696_falling = recent.windows(2).all(|window| window[1] < window[0]);
    let ema696_rising = recent.windows(2).all(|window| window[1] > window[0]);
    (
        bearish_order && ema696_falling,
        bullish_order && ema696_rising,
        recent,
    )
}

/// 只接受四条均线都已预热且为正数的单根快照。
fn ordered_emas(candle: &ComputedCandle) -> Option<(f64, f64, f64, f64)> {
    let values = (
        candle.ema12?,
        candle.ema144?,
        candle.ema169?,
        candle.ema696?,
    );
    (values.0.is_finite()
        && values.0 > 0.0
        && values.1.is_finite()
        && values.1 > 0.0
        && values.2.is_finite()
        && values.2 > 0.0
        && values.3.is_finite()
        && values.3 > 0.0)
        .then_some(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::market_volume_platform_break_trend_v1_research_args;
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
        candles[break_idx + 1].candle = BacktestCandle {
            ts: (break_idx + 1) as i64 * MS_15M,
            open: 98.1,
            high: 98.8,
            low: 97.7,
            close: 98.2,
            volume: 10.0,
        };
        candles[latest_idx].candle = BacktestCandle {
            ts: latest_idx as i64 * MS_15M,
            open: 98.3,
            high: 98.8,
            low: 97.9,
            close: 98.4,
            volume: 10.0,
        };
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
    fn requires_platform_acceptance_and_ema_but_ignores_rsi_and_macd() {
        let mut candles = bearish_setup();
        let args = market_volume_platform_break_trend_v1_research_args().unwrap();
        let before = signal(&candles, candles.len(), &args).unwrap();
        let latest_idx = candles.len() - 1;
        candles[latest_idx].rsi14 = None;
        candles[latest_idx].macd_line = None;
        let after = signal(&candles, candles.len(), &args).unwrap();

        assert_eq!(before.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(before.trigger, after.trigger);
        assert_eq!(before.direction, after.direction);
        let family = before.evidence.isolated_family.as_ref().unwrap();
        assert_eq!(family.family, "volume_platform_break_trend");
        assert!(family.long_term_ema_confirmed);
        assert_eq!(
            family
                .platform_breakdown
                .as_ref()
                .map(|platform| platform.direction),
            Some("bearish")
        );
    }

    #[test]
    fn flat_ema696_rejects_an_otherwise_valid_break() {
        let mut candles = bearish_setup();
        let latest_idx = candles.len() - 1;
        for idx in (latest_idx - 3)..=latest_idx {
            candles[idx].ema696 = Some(103.0);
        }
        let args = market_volume_platform_break_trend_v1_research_args().unwrap();

        assert_eq!(
            signal(&candles, candles.len(), &args),
            Err("platform_break_trend_ema_not_confirmed")
        );
    }

    #[test]
    fn future_candle_cannot_change_two_close_confirmation() {
        let mut candles = bearish_setup();
        let completed_count = candles.len();
        let args = market_volume_platform_break_trend_v1_research_args().unwrap();
        let before = signal(&candles, completed_count, &args).unwrap();
        let mut future = candle(completed_count);
        future.candle.close = 200.0;
        candles.push(future);
        let after = signal(&candles, completed_count, &args).unwrap();

        assert_eq!(before, after);
    }
}
