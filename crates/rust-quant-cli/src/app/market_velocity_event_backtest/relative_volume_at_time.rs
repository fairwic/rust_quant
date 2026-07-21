use super::{BacktestCandle, ComputedCandle};

const MS_1D: i64 = 24 * 60 * 60 * 1_000;
const RVAT_DAYS: usize = 10;

/// 计算 TradingView Regular RVAT10 同口径量比，只读取当前 K 线之前十个同 UTC 时点。
pub(super) fn relative_volume_at_time_10d_ratio(
    candles: &[ComputedCandle],
    current_idx: usize,
) -> Option<f64> {
    let current = candles.get(current_idx)?;
    relative_volume_at_time_10d_ratio_with_lookup(
        current.candle.ts,
        current.candle.volume,
        |target_ts| {
            let idx = candles[..current_idx]
                .binary_search_by_key(&target_ts, |candle| candle.candle.ts)
                .ok()?;
            candles.get(idx).map(|candle| candle.candle.volume)
        },
    )
}

/// 为成交诊断复算与信号完全相同的 RVAT10，避免报告继续展示旧连续均量。
pub(super) fn relative_volume_at_time_10d_ratio_raw(
    candles: &[BacktestCandle],
    current_idx: usize,
) -> Option<f64> {
    let current = candles.get(current_idx)?;
    relative_volume_at_time_10d_ratio_with_lookup(current.ts, current.volume, |target_ts| {
        let idx = candles[..current_idx]
            .binary_search_by_key(&target_ts, |candle| candle.ts)
            .ok()?;
        candles.get(idx).map(|candle| candle.volume)
    })
}

/// 共享十日均量计算，并由调用方限定可见历史，防止当前或未来 K 线泄漏进分母。
fn relative_volume_at_time_10d_ratio_with_lookup(
    current_ts: i64,
    current_volume: f64,
    mut volume_at: impl FnMut(i64) -> Option<f64>,
) -> Option<f64> {
    if !current_volume.is_finite() || current_volume <= 0.0 {
        return None;
    }
    let mut historical_sum = 0.0;
    for day in 1..=RVAT_DAYS {
        let offset = (day as i64).checked_mul(MS_1D)?;
        let target_ts = current_ts.checked_sub(offset)?;
        let volume = volume_at(target_ts)?;
        if !volume.is_finite() || volume <= 0.0 {
            return None;
        }
        historical_sum += volume;
    }
    let average = historical_sum / RVAT_DAYS as f64;
    (average > 0.0).then_some(current_volume / average)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(ts: i64, volume: f64) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume,
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
    fn rvat10_uses_exact_same_time_over_previous_ten_days() {
        let slot_offset = 8 * 60 * 60 * 1_000 + 15 * 60 * 1_000;
        let mut candles = (0..10)
            .map(|day| candle(day * MS_1D + slot_offset, 10.0))
            .collect::<Vec<_>>();
        candles.push(candle(10 * MS_1D + slot_offset, 20.0));

        assert_eq!(relative_volume_at_time_10d_ratio(&candles, 10), Some(2.0));
    }

    #[test]
    fn rvat10_fails_closed_when_a_same_time_bar_is_missing() {
        let slot_offset = 8 * 60 * 60 * 1_000;
        let mut candles = (0..10)
            .filter(|day| *day != 4)
            .map(|day| candle(day * MS_1D + slot_offset, 10.0))
            .collect::<Vec<_>>();
        candles.push(candle(10 * MS_1D + slot_offset, 20.0));
        let current_idx = candles.len() - 1;

        assert_eq!(
            relative_volume_at_time_10d_ratio(&candles, current_idx),
            None
        );
    }
}
