use super::{reversal_retest::RetestEntrySignal, ComputedCandle, MarketVelocityTradeDirection};

/// 在极端量 setup 后等待收盘收回 setup 开盘价，并从确认后的下一根开盘入场。
///
/// 确认只读取 setup 之后已经完成的 K 线；确认 K 本身不作为成交价，避免收盘确认的
/// 回看偏差。结构止损保持为空，让 V3 继续沿用冻结的固定百分比风险模型。
pub(super) fn find_setup_open_reclaim_entry_after_signal(
    candles: &[ComputedCandle],
    setup_idx: usize,
    direction: MarketVelocityTradeDirection,
    original_trigger: &str,
    max_wait_candles: usize,
) -> Result<RetestEntrySignal, String> {
    if max_wait_candles == 0 {
        return Err("setup_open_reclaim_invalid_wait".to_string());
    }
    let setup = candles
        .get(setup_idx)
        .ok_or_else(|| "setup_open_reclaim_missing_setup".to_string())?;
    let setup_open = setup.candle.open;
    if !setup_open.is_finite() || setup_open <= 0.0 {
        return Err("setup_open_reclaim_invalid_setup_open".to_string());
    }
    if direction == MarketVelocityTradeDirection::Both {
        return Err("setup_open_reclaim_invalid_direction".to_string());
    }

    // 最后一根确认 K 后还必须存在下一根 K 的开盘，才能形成可成交入场。
    let Some(last_entry_eligible_confirmation_idx) = candles.len().checked_sub(2) else {
        return Err("setup_open_reclaim_no_next_entry_candle".to_string());
    };
    let first_confirmation_idx = setup_idx.saturating_add(1);
    let last_confirmation_idx = setup_idx
        .saturating_add(max_wait_candles)
        .min(last_entry_eligible_confirmation_idx);
    if first_confirmation_idx > last_confirmation_idx {
        return Err("setup_open_reclaim_no_next_entry_candle".to_string());
    }

    for confirmation_idx in first_confirmation_idx..=last_confirmation_idx {
        let confirmation = &candles[confirmation_idx];
        let reclaimed = match direction {
            MarketVelocityTradeDirection::Long => confirmation.candle.close > setup_open,
            MarketVelocityTradeDirection::Short => confirmation.candle.close < setup_open,
            MarketVelocityTradeDirection::Both => false,
        };
        if !reclaimed {
            continue;
        }
        let entry_idx = confirmation_idx + 1;
        let entry = &candles[entry_idx];
        return Ok(RetestEntrySignal {
            entry_ts: entry.candle.ts,
            entry_price: entry.candle.open,
            entry_idx,
            trigger: format!("{original_trigger}+setup_open_reclaim_3"),
            structure_stop_loss_price: None,
            structure_stop_loss_source: None,
        });
    }

    Err("setup_open_reclaim_not_confirmed".to_string())
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
                volume: 100.0,
            },
            sma: None,
            ema: None,
            previous_volume_avg: None,
            previous_range_avg: None,
            rsi14: None,
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: None,
            macd_signal_line: None,
            macd_histogram: None,
        }
    }

    #[test]
    fn long_reclaim_enters_only_at_next_candle_open() {
        let candles = vec![
            candle(0, 100.0, 101.0, 94.0, 95.0),
            candle(1, 95.0, 102.0, 94.0, 101.0),
            candle(2, 101.5, 103.0, 100.0, 102.0),
        ];

        let entry = find_setup_open_reclaim_entry_after_signal(
            &candles,
            0,
            MarketVelocityTradeDirection::Long,
            "opposite_move_momentum_reversal",
            3,
        )
        .unwrap();

        assert_eq!(entry.entry_ts, 2);
        assert_eq!(entry.entry_price, 101.5);
        assert_eq!(entry.entry_idx, 2);
    }

    #[test]
    fn intrabar_touch_without_close_reclaim_does_not_confirm() {
        let candles = vec![
            candle(0, 100.0, 101.0, 94.0, 95.0),
            candle(1, 95.0, 102.0, 94.0, 99.0),
            candle(2, 99.0, 100.0, 97.0, 98.0),
        ];

        assert_eq!(
            find_setup_open_reclaim_entry_after_signal(
                &candles,
                0,
                MarketVelocityTradeDirection::Long,
                "opposite_move_momentum_reversal",
                3,
            ),
            Err("setup_open_reclaim_not_confirmed".to_string())
        );
    }

    #[test]
    fn short_reclaim_is_directionally_symmetric() {
        let candles = vec![
            candle(0, 100.0, 106.0, 99.0, 105.0),
            candle(1, 105.0, 106.0, 98.0, 99.0),
            candle(2, 98.5, 99.0, 96.0, 97.0),
        ];

        let entry = find_setup_open_reclaim_entry_after_signal(
            &candles,
            0,
            MarketVelocityTradeDirection::Short,
            "opposite_move_momentum_reversal",
            3,
        )
        .unwrap();

        assert_eq!(entry.entry_ts, 2);
        assert_eq!(entry.entry_price, 98.5);
    }
}
