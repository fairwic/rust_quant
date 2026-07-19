use super::{
    base_entry_trigger, moving_average_distance_pct, ComputedCandle,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
};

/// 保存延迟回踩确认后的真实下一根开盘，避免以确认 K 线收盘价回填成交。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RetestEntrySignal {
    /// 下一根可成交 K 线的时间戳。
    pub(super) entry_ts: i64,
    /// 下一根可成交 K 线的开盘价。
    pub(super) entry_price: f64,
    /// 下一根可成交 K 线在完整序列中的下标。
    pub(super) entry_idx: usize,
    /// 保留基础触发器与回踩确认链路的审计标签。
    pub(super) trigger: String,
    /// 回踩使用的突破结构价位。
    pub(super) structure_stop_loss_price: Option<f64>,
    /// 突破结构价位的来源。
    pub(super) structure_stop_loss_source: Option<String>,
}

/// 在初始信号后寻找回踩守住确认；反转策略允许双向等待，旧 long 回踩保持原行为。
pub(super) fn find_retest_entry_after_signal(
    candles: &[ComputedCandle],
    signal_idx: usize,
    direction: MarketVelocityTradeDirection,
    original_trigger: &str,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<RetestEntrySignal, String> {
    let signal = candles
        .get(signal_idx)
        .ok_or_else(|| "entry_retest_missing_signal".to_string())?;
    let base_trigger = base_entry_trigger(original_trigger);
    let opposite_reversal = base_trigger == "opposite_move_momentum_reversal";
    if direction == MarketVelocityTradeDirection::Short && !opposite_reversal {
        return Err("entry_retest_short_not_supported".to_string());
    }
    let previous = || {
        signal_idx
            .checked_sub(1)
            .and_then(|previous_idx| candles.get(previous_idx))
    };
    let retest_level = match (base_trigger.as_str(), direction) {
        ("breakout_previous_high", MarketVelocityTradeDirection::Long) => {
            previous().map(|candle| candle.candle.high)
        }
        ("reclaim_ema", MarketVelocityTradeDirection::Long) => signal.ema,
        ("opposite_move_momentum_reversal", MarketVelocityTradeDirection::Long) => {
            previous().map(|candle| candle.candle.high)
        }
        ("opposite_move_momentum_reversal", MarketVelocityTradeDirection::Short) => {
            previous().map(|candle| candle.candle.low)
        }
        _ => return Err("entry_retest_unsupported_trigger".to_string()),
    }
    .filter(|level| level.is_finite() && *level > 0.0)
    .ok_or_else(|| "entry_retest_invalid_level".to_string())?;
    let last_confirmation_idx =
        (signal_idx + args.entry_retest_max_wait_candles).min(candles.len().saturating_sub(1));
    for confirmation_idx in signal_idx + 1..=last_confirmation_idx {
        let confirmation = &candles[confirmation_idx];
        let matches = if opposite_reversal {
            opposite_reversal_retest_matches(confirmation, retest_level, direction, args)
        } else {
            legacy_long_retest_matches(confirmation, retest_level, args)
        };
        if !matches {
            continue;
        }
        let entry_idx = confirmation_idx + 1;
        let Some(entry) = candles.get(entry_idx) else {
            return Err("entry_retest_no_next_entry_candle".to_string());
        };
        apply_entry_open_fade_guard(entry, confirmation, args)?;
        let trigger = if opposite_reversal {
            format!("{original_trigger}+retest_after_signal")
        } else {
            format!("{base_trigger}+retest_after_signal")
        };
        return Ok(RetestEntrySignal {
            entry_ts: entry.candle.ts,
            entry_price: entry.candle.open,
            entry_idx,
            trigger,
            structure_stop_loss_price: Some(retest_level),
            structure_stop_loss_source: Some(
                match (base_trigger.as_str(), direction) {
                    ("reclaim_ema", _) => "entry_confirmation_ema",
                    ("breakout_previous_high", _) => "entry_confirmation_previous_high",
                    ("opposite_move_momentum_reversal", MarketVelocityTradeDirection::Long) => {
                        "opposite_reversal_previous_high"
                    }
                    ("opposite_move_momentum_reversal", MarketVelocityTradeDirection::Short) => {
                        "opposite_reversal_previous_low"
                    }
                    _ => "entry_confirmation_structure",
                }
                .to_string(),
            ),
        });
    }
    Err("entry_retest_no_pullback_confirmation".to_string())
}

/// 回踩后的开盘若反向跳空，可沿用既有成交量救援门槛阻断不可执行入场。
fn apply_entry_open_fade_guard(
    entry: &ComputedCandle,
    confirmation: &ComputedCandle,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<(), String> {
    let Some(min_gap_pct) = args.entry_retest_min_entry_open_gap_pct else {
        return Ok(());
    };
    let gap_pct = moving_average_distance_pct(entry.candle.open, confirmation.candle.close)
        .ok_or_else(|| "entry_retest_invalid_entry_gap".to_string())?;
    if gap_pct >= min_gap_pct {
        return Ok(());
    }
    let volume_ratio = confirmation
        .previous_volume_avg
        .filter(|average| *average > 0.0)
        .map(|average| confirmation.candle.volume / average);
    if args
        .entry_retest_open_fade_min_volume_ratio
        .is_some_and(|minimum| volume_ratio.is_some_and(|ratio| ratio >= minimum))
    {
        return Ok(());
    }
    Err("entry_retest_entry_open_faded_confirmation".to_string())
}

/// 反转量能只在初始耗竭信号验证；回踩确认看价格守位，避免错误要求二次放量。
fn opposite_reversal_retest_matches(
    confirmation: &ComputedCandle,
    retest_level: f64,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> bool {
    let candle = &confirmation.candle;
    let tolerance_pct = args.entry_retest_tolerance_pct / 100.0;
    let price_holds = match direction {
        MarketVelocityTradeDirection::Long => {
            candle.low <= retest_level * (1.0 + tolerance_pct)
                && candle.close >= retest_level
                && candle.close > candle.open
        }
        MarketVelocityTradeDirection::Short => {
            candle.high >= retest_level * (1.0 - tolerance_pct)
                && candle.close <= retest_level
                && candle.close < candle.open
        }
        MarketVelocityTradeDirection::Both => false,
    };
    price_holds && averages_hold(confirmation, direction, args)
}

/// 保留已有 long breakout/EMA 回踩的成交量确认，避免 v10 改变其他研究 preset。
fn legacy_long_retest_matches(
    confirmation: &ComputedCandle,
    retest_level: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> bool {
    let candle = &confirmation.candle;
    let tolerance = 1.0 + args.entry_retest_tolerance_pct / 100.0;
    if candle.low > retest_level * tolerance
        || candle.close < retest_level
        || candle.close <= candle.open
        || !averages_hold(confirmation, MarketVelocityTradeDirection::Long, args)
    {
        return false;
    }
    let volume_ratio = confirmation
        .previous_volume_avg
        .filter(|average| *average > 0.0)
        .map(|average| candle.volume / average);
    args.entry_min_volume_ratio <= 0.0
        || volume_ratio.is_some_and(|ratio| ratio >= args.entry_min_volume_ratio)
}

/// 要求回踩收盘仍处于两条 15m 均线的入场方向一侧，并沿用最大乖离限制。
fn averages_hold(
    confirmation: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> bool {
    let (Some(sma), Some(ema)) = (confirmation.sma, confirmation.ema) else {
        return false;
    };
    let aligned = match direction {
        MarketVelocityTradeDirection::Long => {
            confirmation.candle.close > sma && confirmation.candle.close > ema
        }
        MarketVelocityTradeDirection::Short => {
            confirmation.candle.close < sma && confirmation.candle.close < ema
        }
        MarketVelocityTradeDirection::Both => false,
    };
    if !aligned {
        return false;
    }
    let Some(sma_distance) = moving_average_distance_pct(confirmation.candle.close, sma) else {
        return false;
    };
    let Some(ema_distance) = moving_average_distance_pct(confirmation.candle.close, ema) else {
        return false;
    };
    args.entry_max_distance_pct <= 0.0
        || (sma_distance.abs() <= args.entry_max_distance_pct
            && ema_distance.abs() <= args.entry_max_distance_pct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    fn candle(ts: i64, open: f64, high: f64, low: f64, close: f64) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts,
                open,
                high,
                low,
                close,
                volume: 5.0,
            },
            sma: Some(if close >= open { 99.0 } else { 101.0 }),
            ema: Some(if close >= open { 99.2 } else { 100.8 }),
            previous_volume_avg: Some(100.0),
            previous_range_avg: None,
            rsi14: None,
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
        }
    }

    #[test]
    fn long_opposite_reversal_accepts_low_volume_retest_and_enters_next_open() {
        let candles = vec![
            candle(0, 99.0, 100.0, 98.0, 99.0),
            candle(1, 99.0, 102.0, 98.5, 101.5),
            candle(2, 100.1, 101.0, 99.9, 100.8),
            candle(3, 100.9, 102.0, 100.5, 101.5),
        ];
        let args = MarketVelocityEventBacktestArgs {
            entry_retest_max_wait_candles: 3,
            entry_retest_tolerance_pct: 0.3,
            entry_min_volume_ratio: 1.5,
            entry_max_distance_pct: 14.0,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let entry = find_retest_entry_after_signal(
            &candles,
            1,
            MarketVelocityTradeDirection::Long,
            "opposite_move_momentum_reversal",
            &args,
        )
        .unwrap();
        assert_eq!(entry.entry_ts, 3);
        assert_eq!(entry.entry_price, 100.9);
    }

    #[test]
    fn short_opposite_reversal_uses_previous_low_symmetrically() {
        let candles = vec![
            candle(0, 101.0, 102.0, 100.0, 101.0),
            candle(1, 101.0, 101.5, 98.0, 98.5),
            candle(2, 99.9, 100.1, 99.0, 99.2),
            candle(3, 99.1, 99.5, 97.5, 98.0),
        ];
        let args = MarketVelocityEventBacktestArgs {
            entry_retest_max_wait_candles: 3,
            entry_retest_tolerance_pct: 0.3,
            entry_max_distance_pct: 14.0,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let entry = find_retest_entry_after_signal(
            &candles,
            1,
            MarketVelocityTradeDirection::Short,
            "opposite_move_momentum_reversal",
            &args,
        )
        .unwrap();
        assert_eq!(entry.entry_ts, 3);
        assert_eq!(entry.entry_price, 99.1);
    }

    #[test]
    fn opposite_reversal_rejects_retest_that_does_not_hold_breakout_level() {
        let candles = vec![
            candle(0, 99.0, 100.0, 98.0, 99.0),
            candle(1, 99.0, 102.0, 98.5, 101.5),
            candle(2, 100.1, 100.5, 98.5, 99.5),
            candle(3, 99.6, 100.0, 98.8, 99.0),
        ];
        let args = MarketVelocityEventBacktestArgs {
            entry_retest_max_wait_candles: 2,
            entry_retest_tolerance_pct: 0.3,
            ..MarketVelocityEventBacktestArgs::default()
        };
        assert_eq!(
            find_retest_entry_after_signal(
                &candles,
                1,
                MarketVelocityTradeDirection::Long,
                "opposite_move_momentum_reversal",
                &args,
            ),
            Err("entry_retest_no_pullback_confirmation".to_string())
        );
    }
}
