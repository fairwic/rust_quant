use super::directional_reversal::{
    exhaustion_volume_dominance_filter_reason, opposite_net_move_filter_reason,
};
use super::kline_shape::direct_kline_momentum_shape_filter_reason;
use super::{
    valid_positive, ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
};

/// 聚合 15m 快动量研究过滤；所有门禁都必须由显式参数开启。
pub(super) fn fast_momentum_entry_filter_reason(
    candles: &[ComputedCandle],
    completed_count: usize,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<&'static str> {
    let latest_idx = completed_count.checked_sub(1)?;
    let latest = candles.get(latest_idx)?;
    if let Some(reason) = direct_kline_momentum_shape_filter_reason(latest, direction, args) {
        return Some(reason);
    }
    if !args.entry_defer_long_lower_wick_reversal
        && !args.entry_long_bullish_hammer_reversal
        && !args.entry_extreme_volume_continuation
    {
        if let Some(reason) =
            opposite_net_move_filter_reason(candles, completed_count, direction, args)
        {
            return Some(reason);
        }
    }
    if let Some(reason) =
        exhaustion_volume_dominance_filter_reason(candles, completed_count, direction, args)
    {
        return Some(reason);
    }
    if let Some(reason) = macd_recovery_filter_reason(candles, completed_count, direction, args) {
        return Some(reason);
    }
    if args.entry_min_rsi.is_some()
        || args.entry_max_rsi.is_some()
        || args.entry_min_rsi_delta.is_some()
    {
        let Some(latest_rsi) = latest.rsi14 else {
            return Some("rsi_not_ready");
        };
        if args
            .entry_min_rsi
            .is_some_and(|min_rsi| latest_rsi < min_rsi)
        {
            return Some("rsi_below_min");
        }
        if args
            .entry_max_rsi
            .is_some_and(|max_rsi| latest_rsi > max_rsi)
        {
            return Some("rsi_above_max");
        }
        if let Some(min_delta) = args.entry_min_rsi_delta {
            let Some(previous_idx) = latest_idx.checked_sub(args.entry_rsi_delta_lookback_candles)
            else {
                return Some("rsi_delta_not_ready");
            };
            let Some(previous_rsi) = candles.get(previous_idx).and_then(|candle| candle.rsi14)
            else {
                return Some("rsi_delta_not_ready");
            };
            if latest_rsi - previous_rsi < min_delta {
                return Some("rsi_delta_not_confirmed");
            }
        }
    }
    if args.entry_bollinger_breakout {
        let breakout_ok = match direction {
            MarketVelocityTradeDirection::Long => latest
                .bollinger_upper
                .is_some_and(|upper| latest.candle.close > upper),
            MarketVelocityTradeDirection::Short => latest
                .bollinger_lower
                .is_some_and(|lower| latest.candle.close < lower),
            MarketVelocityTradeDirection::Both => false,
        };
        if !breakout_ok {
            return Some("bollinger_breakout_not_confirmed");
        }
    }
    if let Some(min_expansion_pct) = args.entry_min_bollinger_bandwidth_expansion_pct {
        let Some(previous_idx) = latest_idx.checked_sub(1) else {
            return Some("bollinger_bandwidth_not_ready");
        };
        let Some(latest_bandwidth) = latest.bollinger_bandwidth_pct else {
            return Some("bollinger_bandwidth_not_ready");
        };
        let Some(previous_bandwidth) = candles
            .get(previous_idx)
            .and_then(|candle| candle.bollinger_bandwidth_pct)
        else {
            return Some("bollinger_bandwidth_not_ready");
        };
        if !valid_positive(previous_bandwidth) {
            return Some("bollinger_bandwidth_not_ready");
        }
        let expansion_pct = (latest_bandwidth - previous_bandwidth) / previous_bandwidth * 100.0;
        if expansion_pct < min_expansion_pct {
            return Some("bollinger_bandwidth_expansion_not_confirmed");
        }
    }
    if let Some(min_drawdown_pct) = args.entry_min_recent_drawdown_pct {
        let Some(drawdown_pct) = recent_entry_drawdown_pct(
            candles,
            latest_idx,
            args.entry_recent_drawdown_lookback_candles,
        ) else {
            return Some("recent_drawdown_not_ready");
        };
        if drawdown_pct < min_drawdown_pct {
            return Some("recent_drawdown_not_confirmed");
        }
    }
    None
}

/// 要求前一根 MACD 柱仍为负值、当前柱体开始回升，表示下行动量正在衰减。
fn macd_recovery_filter_reason(
    candles: &[ComputedCandle],
    completed_count: usize,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<&'static str> {
    if !args.entry_require_macd_negative_histogram_improving {
        return None;
    }
    if direction != MarketVelocityTradeDirection::Long {
        return Some("macd_recovery_direction_not_supported");
    }
    let latest_idx = match completed_count.checked_sub(1) {
        Some(idx) => idx,
        None => return Some("macd_recovery_not_ready"),
    };
    let previous_idx = match latest_idx.checked_sub(1) {
        Some(idx) => idx,
        None => return Some("macd_recovery_not_ready"),
    };
    let Some(previous_histogram) = candles
        .get(previous_idx)
        .and_then(|candle| candle.macd_histogram)
    else {
        return Some("macd_recovery_not_ready");
    };
    let Some(latest_histogram) = candles
        .get(latest_idx)
        .and_then(|candle| candle.macd_histogram)
    else {
        return Some("macd_recovery_not_ready");
    };
    if previous_histogram < 0.0 && latest_histogram > previous_histogram {
        None
    } else {
        Some("macd_negative_histogram_not_improving")
    }
}

/// 计算当前突破 K 线之前的回看跌幅，避免把连续拉升末端当作首轮机会。
fn recent_entry_drawdown_pct(
    candles: &[ComputedCandle],
    latest_idx: usize,
    lookback_candles: usize,
) -> Option<f64> {
    let start = latest_idx.checked_sub(lookback_candles)?;
    let history = candles.get(start..latest_idx)?;
    if history.is_empty() {
        return None;
    }
    let mut highest_high = f64::NEG_INFINITY;
    let mut lowest_low = f64::INFINITY;
    for candle in history {
        if !candle.candle.high.is_finite() || !candle.candle.low.is_finite() {
            return None;
        }
        highest_high = highest_high.max(candle.candle.high);
        lowest_low = lowest_low.min(candle.candle.low);
    }
    valid_positive(highest_high).then_some((highest_high - lowest_low) / highest_high * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    fn candle(ts: i64, histogram: Option<f64>) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts,
                open: 99.0,
                high: 102.0,
                low: 98.0,
                close: 101.0,
                volume: 10.0,
            },
            sma: Some(100.0),
            ema: Some(100.0),
            previous_volume_avg: Some(5.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            bollinger_middle: Some(100.0),
            bollinger_upper: Some(105.0),
            bollinger_lower: Some(95.0),
            bollinger_bandwidth_pct: Some(10.0),
            macd_line: histogram,
            macd_signal_line: Some(0.0),
            macd_histogram: histogram,
        }
    }

    fn args() -> MarketVelocityEventBacktestArgs {
        MarketVelocityEventBacktestArgs {
            entry_require_macd_negative_histogram_improving: true,
            ..MarketVelocityEventBacktestArgs::default()
        }
    }

    #[test]
    fn negative_histogram_improvement_confirms_long_recovery() {
        let candles = vec![candle(0, Some(-2.0)), candle(1, Some(-1.0))];

        assert_eq!(
            macd_recovery_filter_reason(
                &candles,
                candles.len(),
                MarketVelocityTradeDirection::Long,
                &args(),
            ),
            None
        );
    }

    #[test]
    fn positive_previous_histogram_does_not_describe_negative_momentum_recovery() {
        let candles = vec![candle(0, Some(0.2)), candle(1, Some(0.4))];

        assert_eq!(
            macd_recovery_filter_reason(
                &candles,
                candles.len(),
                MarketVelocityTradeDirection::Long,
                &args(),
            ),
            Some("macd_negative_histogram_not_improving")
        );
    }

    #[test]
    fn more_negative_histogram_is_blocked() {
        let candles = vec![candle(0, Some(-1.0)), candle(1, Some(-2.0))];

        assert_eq!(
            macd_recovery_filter_reason(
                &candles,
                candles.len(),
                MarketVelocityTradeDirection::Long,
                &args(),
            ),
            Some("macd_negative_histogram_not_improving")
        );
    }

    #[test]
    fn future_candle_cannot_change_signal_time_macd_decision() {
        let mut candles = vec![candle(0, Some(-2.0)), candle(1, Some(-1.0))];
        let before =
            macd_recovery_filter_reason(&candles, 2, MarketVelocityTradeDirection::Long, &args());
        candles.push(candle(2, Some(-5.0)));

        assert_eq!(
            before,
            macd_recovery_filter_reason(&candles, 2, MarketVelocityTradeDirection::Long, &args(),)
        );
    }
}
