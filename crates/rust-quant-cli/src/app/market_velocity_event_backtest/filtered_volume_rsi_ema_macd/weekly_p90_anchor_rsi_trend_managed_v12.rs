use super::super::{
    ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
    PlatformBreakdownSignalEvidence, TrendManagedExitSignalEvidence,
};
use super::weekly_base_volume_v3::{filtered_volume_evidence, weekly_volume_ccy_evidence};
use super::weekly_p90_anchor_rsi_divergence_v9::FILTERED_VOLUME_V9_MIN_RATIO;
use super::weekly_p90_anchor_rsi_divergence_wick_or_touch_v11::signal as v11_signal;
use super::FilteredVolumeRsiEmaMacdSignal;

/// 平台由破位 K 之前精确 20 根已完成 K 线定义。
pub(crate) const TREND_PLATFORM_LOOKBACK_CANDLES: usize = 20;
/// 平台宽度不得超过破位时 ATR14 的四倍。
pub(crate) const TREND_PLATFORM_MAX_RANGE_ATR: f64 = 4.0;
/// 破位后必须有两根已完成 K 线保持在原平台外。
pub(crate) const TREND_PLATFORM_CONFIRMATION_CANDLES: usize = 2;
/// 破位实体至少占完整振幅 60%。
pub(crate) const TREND_PLATFORM_MIN_BODY_RANGE_RATIO: f64 = 0.60;
/// 破位实体相对开盘价至少为 1%。
pub(crate) const TREND_PLATFORM_MIN_BODY_OPEN_RATIO: f64 = 0.01;
/// 逆势交易默认只取一个冻结 ATR。
pub(crate) const COUNTERTREND_TARGET_ATR_MULTIPLIER: f64 = 1.0;
/// 逆势恢复量比目标档位至少需要四倍过滤量比。
pub(crate) const COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO: f64 = 4.0;
/// 逆势例外只观察锚点 p 之前精确 96 根已完成 K 线。
pub(crate) const COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES: usize = 96;
/// 逆势例外要求历史净移动沿趋势方向至少 8%。
pub(crate) const COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT: f64 = 8.0;

/// 完整复用 v11 入场，只在锚点 p 完成时冻结趋势关系并替换本笔 ATR 止盈距离。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_countertrend_target(
        candles,
        completed_count,
        args,
        COUNTERTREND_TARGET_ATR_MULTIPLIER,
        "countertrend_one_atr",
    )
}

/// 复用 V12 的完整趋势证据，只允许后续研究版本替换预先冻结的逆势默认目标。
pub(super) fn signal_with_countertrend_target(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
    countertrend_target_atr_multiplier: f64,
    countertrend_target_policy: &'static str,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - FILTERED_VOLUME_V9_MIN_RATIO).abs() > f64::EPSILON {
        return Err("filtered_volume_v12_ratio_policy_mismatch");
    }
    if !positive(countertrend_target_atr_multiplier) {
        return Err("filtered_volume_trend_countertrend_target_invalid");
    }
    let pivot_idx = completed_count
        .checked_sub(1)
        .ok_or("filtered_volume_v12_anchor_setup_not_found")?;
    let mut setup = v11_signal(candles, completed_count, args)
        .map_err(|_| "filtered_volume_v12_anchor_setup_not_found")?;
    let volume_tier_target = setup
        .evidence
        .take_profit_atr_multiplier
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("filtered_volume_v12_target_tier_missing")?;
    let trend = trend_managed_exit_evidence(
        candles,
        pivot_idx,
        setup.direction,
        setup.evidence.filtered_volume_ratio,
        volume_tier_target,
        countertrend_target_atr_multiplier,
        countertrend_target_policy,
    )?;
    setup.evidence.take_profit_atr_multiplier = Some(trend.selected_take_profit_atr_multiplier);
    setup.evidence.trend_managed_exit = Some(trend);
    Ok(setup)
}

fn trend_managed_exit_evidence(
    candles: &[ComputedCandle],
    pivot_idx: usize,
    direction: MarketVelocityTradeDirection,
    signal_volume_ratio: f64,
    volume_tier_target: f64,
    countertrend_target_atr_multiplier: f64,
    countertrend_target_policy: &'static str,
) -> Result<TrendManagedExitSignalEvidence, &'static str> {
    let (long_term_bearish, long_term_bullish, ema696_recent) =
        long_term_ema_confirmation(candles, pivot_idx);
    let (bearish_platform, bullish_platform) = active_platform_breakdowns(candles, pivot_idx);
    let bearish = long_term_bearish || bearish_platform.is_some();
    let bullish = long_term_bullish || bullish_platform.is_some();
    let market_regime = match (bearish, bullish) {
        (true, false) => "bearish",
        (false, true) => "bullish",
        (true, true) => "conflict_neutral",
        (false, false) => "neutral",
    };
    let trade_trend_relation = match (market_regime, direction) {
        ("bearish", MarketVelocityTradeDirection::Short)
        | ("bullish", MarketVelocityTradeDirection::Long) => "with_trend",
        ("bearish", MarketVelocityTradeDirection::Long)
        | ("bullish", MarketVelocityTradeDirection::Short) => "countertrend",
        _ => "neutral",
    };
    let prior_96_net_move_pct =
        prior_net_move_pct(candles, pivot_idx, COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES);
    let countertrend_extreme_move_exception = trade_trend_relation == "countertrend"
        && signal_volume_ratio >= COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO
        && match market_regime {
            "bearish" => prior_96_net_move_pct
                .is_some_and(|value| value <= -COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT),
            "bullish" => prior_96_net_move_pct
                .is_some_and(|value| value >= COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT),
            _ => false,
        };
    let (selected_target, target_policy) = if trade_trend_relation != "countertrend" {
        (volume_tier_target, "volume_tier")
    } else if countertrend_extreme_move_exception {
        (volume_tier_target, "countertrend_extreme_volume_tier")
    } else {
        (
            countertrend_target_atr_multiplier,
            countertrend_target_policy,
        )
    };

    Ok(TrendManagedExitSignalEvidence {
        market_regime,
        trade_trend_relation,
        long_term_bearish_confirmed: long_term_bearish,
        long_term_bullish_confirmed: long_term_bullish,
        ema696_recent,
        bearish_platform_breakdown: bearish_platform,
        bullish_platform_breakdown: bullish_platform,
        prior_96_net_move_pct,
        countertrend_extreme_move_exception,
        volume_tier_take_profit_atr_multiplier: volume_tier_target,
        selected_take_profit_atr_multiplier: selected_target,
        target_policy,
    })
}

/// 三根有序均线同时要求 EMA696 各自低于/高于前一根，避免把一次价格穿越误判为长期趋势。
fn long_term_ema_confirmation(
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
    if recent.len() != 4 || recent.iter().any(|value| !positive(*value)) {
        return (false, false, recent);
    }
    let ordered = (first_ordered_idx..=latest_idx)
        .filter_map(|idx| candles.get(idx))
        .collect::<Vec<_>>();
    if ordered.len() != 3 {
        return (false, false, recent);
    }
    let bearish_order = ordered.iter().all(|candle| {
        matches!(
            (
                candle.ema12,
                candle.ema144,
                candle.ema169,
                candle.ema696
            ),
            (Some(ema12), Some(ema144), Some(ema169), Some(ema696))
                if positive(ema12)
                    && ema12 < ema144
                    && ema144 < ema169
                    && ema169 < ema696
        )
    });
    let bullish_order = ordered.iter().all(|candle| {
        matches!(
            (
                candle.ema12,
                candle.ema144,
                candle.ema169,
                candle.ema696
            ),
            (Some(ema12), Some(ema144), Some(ema169), Some(ema696))
                if positive(ema696)
                    && ema12 > ema144
                    && ema144 > ema169
                    && ema169 > ema696
        )
    });
    let ema696_falling = recent.windows(2).all(|window| window[1] < window[0]);
    let ema696_rising = recent.windows(2).all(|window| window[1] > window[0]);
    (
        bearish_order && ema696_falling,
        bullish_order && ema696_rising,
        recent,
    )
}

/// 从锚点 p 向后寻找最近仍未被任何后续收盘收回的平台破位，且只在两根确认完成后激活。
fn active_platform_breakdowns(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> (
    Option<PlatformBreakdownSignalEvidence>,
    Option<PlatformBreakdownSignalEvidence>,
) {
    let Some(latest_candidate_idx) = latest_idx.checked_sub(TREND_PLATFORM_CONFIRMATION_CANDLES)
    else {
        return (None, None);
    };
    let first_candidate_idx = TREND_PLATFORM_LOOKBACK_CANDLES
        .max(super::weekly_base_volume_v3::WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES);
    if latest_candidate_idx < first_candidate_idx {
        return (None, None);
    }

    let mut suffix_max_close = f64::NEG_INFINITY;
    let mut suffix_min_close = f64::INFINITY;
    let mut bearish = None;
    let mut bullish = None;
    for idx in (first_candidate_idx..=latest_idx).rev() {
        if idx <= latest_candidate_idx {
            let Some(candidate) = candles.get(idx) else {
                continue;
            };
            let open = candidate.candle.open;
            let close = candidate.candle.close;
            let range = candidate.candle.high - candidate.candle.low;
            let body = (close - open).abs();
            let Some(atr14) = candidate.atr14.filter(|value| positive(*value)) else {
                update_suffix(
                    candidate.candle.close,
                    &mut suffix_max_close,
                    &mut suffix_min_close,
                );
                continue;
            };
            if positive(open)
                && positive(range)
                && body / range >= TREND_PLATFORM_MIN_BODY_RANGE_RATIO
                && body / open >= TREND_PLATFORM_MIN_BODY_OPEN_RATIO
            {
                if let Some((platform_low, platform_high)) = platform_bounds(candles, idx) {
                    let platform_range_atr = (platform_high - platform_low) / atr14;
                    if platform_range_atr <= TREND_PLATFORM_MAX_RANGE_ATR {
                        if bearish.is_none()
                            && close < open
                            && close < platform_low
                            && suffix_max_close <= platform_low
                        {
                            bearish = platform_breakdown_evidence(
                                candles,
                                idx,
                                "bearish",
                                platform_low,
                                platform_high,
                                platform_range_atr,
                                body / range,
                                body / open,
                            );
                        }
                        if bullish.is_none()
                            && close > open
                            && close > platform_high
                            && suffix_min_close >= platform_high
                        {
                            bullish = platform_breakdown_evidence(
                                candles,
                                idx,
                                "bullish",
                                platform_low,
                                platform_high,
                                platform_range_atr,
                                body / range,
                                body / open,
                            );
                        }
                    }
                }
            }
        }
        if let Some(candidate) = candles.get(idx) {
            update_suffix(
                candidate.candle.close,
                &mut suffix_max_close,
                &mut suffix_min_close,
            );
        }
        if bearish.is_some() && bullish.is_some() {
            break;
        }
    }
    (bearish, bullish)
}

fn platform_bounds(candles: &[ComputedCandle], break_idx: usize) -> Option<(f64, f64)> {
    let start = break_idx.checked_sub(TREND_PLATFORM_LOOKBACK_CANDLES)?;
    let history = candles.get(start..break_idx)?;
    if history.len() != TREND_PLATFORM_LOOKBACK_CANDLES {
        return None;
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for candle in history {
        if !positive(candle.candle.high)
            || !positive(candle.candle.low)
            || candle.candle.high < candle.candle.low
        {
            return None;
        }
        low = low.min(candle.candle.low);
        high = high.max(candle.candle.high);
    }
    (positive(low) && high >= low).then_some((low, high))
}

#[allow(clippy::too_many_arguments)]
fn platform_breakdown_evidence(
    candles: &[ComputedCandle],
    break_idx: usize,
    direction: &'static str,
    platform_low: f64,
    platform_high: f64,
    platform_range_atr: f64,
    body_range_ratio: f64,
    body_open_ratio: f64,
) -> Option<PlatformBreakdownSignalEvidence> {
    let volume = filtered_volume_evidence(candles, break_idx, FILTERED_VOLUME_V9_MIN_RATIO).ok()?;
    if volume.ratio < FILTERED_VOLUME_V9_MIN_RATIO {
        return None;
    }
    let weekly = weekly_volume_ccy_evidence(candles, break_idx).ok()?;
    let confirmed_idx = break_idx.checked_add(TREND_PLATFORM_CONFIRMATION_CANDLES)?;
    Some(PlatformBreakdownSignalEvidence {
        direction,
        break_ts_ms: candles.get(break_idx)?.candle.ts,
        confirmed_ts_ms: candles.get(confirmed_idx)?.candle.ts,
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
        break_body_range_ratio: body_range_ratio,
        break_body_open_ratio: body_open_ratio,
        filtered_volume_ratio: volume.ratio,
        current_volume_ccy: weekly.current,
        weekly_volume_ccy_p90: weekly.p90,
    })
}

fn update_suffix(close: f64, suffix_max: &mut f64, suffix_min: &mut f64) {
    if close.is_finite() {
        *suffix_max = suffix_max.max(close);
        *suffix_min = suffix_min.min(close);
    }
}

/// p 自身不进入 96 根窗口，避免用锚点价格重复证明当前逆势信号。
fn prior_net_move_pct(
    candles: &[ComputedCandle],
    pivot_idx: usize,
    lookback: usize,
) -> Option<f64> {
    let start = pivot_idx.checked_sub(lookback)?;
    let history = candles.get(start..pivot_idx)?;
    if history.len() != lookback {
        return None;
    }
    let first_open = history.first()?.candle.open;
    let last_close = history.last()?.candle.close;
    if !positive(first_open) || !positive(last_close) {
        return None;
    }
    Some((last_close / first_open - 1.0) * 100.0)
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{BacktestCandle, MS_15M};

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

    #[test]
    fn price_below_ema696_without_falling_ema696_is_not_long_term_bearish() {
        let mut candles = (0..10).map(candle).collect::<Vec<_>>();
        for idx in 7..=9 {
            candles[idx].ema12 = Some(96.0);
            candles[idx].ema144 = Some(97.0);
            candles[idx].ema169 = Some(98.0);
            candles[idx].ema696 = Some(100.0);
            candles[idx].candle.close = 90.0;
        }

        let (bearish, bullish, _) = long_term_ema_confirmation(&candles, 9);

        assert!(!bearish);
        assert!(!bullish);
    }

    #[test]
    fn three_ordered_bars_and_three_falling_ema696_steps_confirm_bearish() {
        let mut candles = (0..10).map(candle).collect::<Vec<_>>();
        for (offset, idx) in (6..=9).enumerate() {
            candles[idx].ema696 = Some(103.0 - offset as f64);
        }
        for idx in 7..=9 {
            candles[idx].ema12 = Some(96.0);
            candles[idx].ema144 = Some(97.0);
            candles[idx].ema169 = Some(98.0);
        }

        let (bearish, bullish, recent) = long_term_ema_confirmation(&candles, 9);

        assert!(bearish);
        assert!(!bullish);
        assert_eq!(recent, vec![103.0, 102.0, 101.0, 100.0]);
    }

    #[test]
    fn prior_96_window_excludes_pivot_candle() {
        let mut candles = (0..100).map(candle).collect::<Vec<_>>();
        candles[3].candle.open = 100.0;
        candles[98].candle.close = 91.0;
        candles[99].candle.close = 200.0;

        assert!((prior_net_move_pct(&candles, 99, 96).unwrap() + 9.0).abs() < 1e-12);
    }

    #[test]
    fn countertrend_defaults_to_one_atr_and_extreme_move_restores_the_volume_tier() {
        let mut candles = (0..100).map(candle).collect::<Vec<_>>();
        for (offset, idx) in (96..=99).enumerate() {
            candles[idx].ema696 = Some(103.0 - offset as f64);
        }
        for idx in 97..=99 {
            candles[idx].ema12 = Some(96.0);
            candles[idx].ema144 = Some(97.0);
            candles[idx].ema169 = Some(98.0);
        }

        let ordinary = trend_managed_exit_evidence(
            &candles,
            99,
            MarketVelocityTradeDirection::Long,
            3.0,
            2.7,
            COUNTERTREND_TARGET_ATR_MULTIPLIER,
            "countertrend_one_atr",
        )
        .unwrap();
        assert_eq!(ordinary.market_regime, "bearish");
        assert_eq!(ordinary.trade_trend_relation, "countertrend");
        assert_eq!(ordinary.selected_take_profit_atr_multiplier, 1.0);
        assert!(!ordinary.countertrend_extreme_move_exception);

        let widened = trend_managed_exit_evidence(
            &candles,
            99,
            MarketVelocityTradeDirection::Long,
            3.0,
            2.7,
            1.5,
            "countertrend_one_point_five_atr",
        )
        .unwrap();
        assert_eq!(widened.selected_take_profit_atr_multiplier, 1.5);
        assert_eq!(widened.target_policy, "countertrend_one_point_five_atr");
        assert!(!widened.countertrend_extreme_move_exception);

        candles[3].candle.open = 100.0;
        candles[98].candle.close = 91.0;
        let exceptional = trend_managed_exit_evidence(
            &candles,
            99,
            MarketVelocityTradeDirection::Long,
            4.0,
            3.6,
            COUNTERTREND_TARGET_ATR_MULTIPLIER,
            "countertrend_one_atr",
        )
        .unwrap();
        assert!(exceptional.countertrend_extreme_move_exception);
        assert_eq!(exceptional.selected_take_profit_atr_multiplier, 3.6);
    }

    #[test]
    fn platform_breakdown_requires_two_closes_and_remains_active_until_reclaimed() {
        let mut candles = (0..700).map(candle).collect::<Vec<_>>();
        for item in &mut candles {
            item.candle.high = 101.0;
            item.candle.low = 99.0;
            item.candle.volume = 10.0;
            item.volume_ccy = Some(10.0);
            item.atr14 = Some(2.0);
        }
        candles[697].candle.open = 100.0;
        candles[697].candle.high = 100.2;
        candles[697].candle.low = 97.5;
        candles[697].candle.close = 98.0;
        candles[697].candle.volume = 30.0;
        candles[697].volume_ccy = Some(30.0);
        candles[698].candle.close = 98.5;
        candles[699].candle.close = 98.7;

        let (bearish, bullish) = active_platform_breakdowns(&candles, 699);
        let bearish = bearish.expect("bearish platform breakdown should be active");
        assert_eq!(bearish.break_ts_ms, candles[697].candle.ts);
        assert_eq!(bearish.confirmed_ts_ms, candles[699].candle.ts);
        assert!(bullish.is_none());

        candles[699].candle.close = 99.1;
        let (reclaimed_bearish, _) = active_platform_breakdowns(&candles, 699);
        assert!(reclaimed_bearish.is_none());
    }
}
