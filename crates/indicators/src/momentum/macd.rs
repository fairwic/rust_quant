use rust_quant_market::models::CandlesEntity;
use ta::indicators::MovingAverageConvergenceDivergence;
use ta::Next;

/// 基于收盘价计算的单根 MACD 值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacdCloseValue {
    /// 快慢 EMA 的差值。
    pub macd_line: f64,
    /// MACD 线的信号 EMA。
    pub signal_line: f64,
    /// MACD 线减去信号线；正值表示多头动量占优。
    pub histogram: f64,
}

pub struct MacdSimpleIndicator {}
impl MacdSimpleIndicator {
    /// 计算 计算 macd，并把公式边界留在回测策略内部。
    pub fn calculate_macd(
        candles: &[CandlesEntity],
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
    ) -> Vec<(i64, f64, f64)> {
        let mut macd =
            MovingAverageConvergenceDivergence::new(fast_period, slow_period, signal_period)
                .unwrap();
        let mut macd_values = Vec::with_capacity(candles.len());
        for candle in candles {
            let price = candle.o.parse::<f64>().unwrap();
            let macd_value = macd.next(price);
            // 打印调试信息
            // warn!("time: {:?}, Price: {}, MACD: {}, Signal: {}", time_util::mill_time_to_datetime(candle.ts), price, macd_value.macd, macd_value.signal);
            macd_values.push((candle.ts, macd_value.macd, macd_value.signal));
        }
        macd_values
    }

    /// 按收盘价顺序计算 MACD，并在慢线与信号线预热完成前返回空值。
    ///
    /// 研究回放使用该入口，避免复用历史 `calculate_macd` 的开盘价口径。输入中的非正数
    /// 或非有限值会使对应位置失败关闭，但不会让后续值读取未来价格。
    pub fn calculate_close_series(
        closes: impl IntoIterator<Item = f64>,
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
    ) -> Option<Vec<Option<MacdCloseValue>>> {
        let mut macd =
            MovingAverageConvergenceDivergence::new(fast_period, slow_period, signal_period)
                .ok()?;
        let warmup_samples = slow_period.checked_add(signal_period)?.checked_sub(1)?;
        let mut sample_count = 0usize;
        let values = closes
            .into_iter()
            .map(|close| {
                if !close.is_finite() || close <= 0.0 {
                    return None;
                }
                sample_count += 1;
                let output = macd.next(close);
                (sample_count >= warmup_samples).then_some(MacdCloseValue {
                    macd_line: output.macd,
                    signal_line: output.signal,
                    histogram: output.macd - output.signal,
                })
            })
            .collect();
        Some(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_series_waits_for_standard_macd_warmup() {
        let values =
            MacdSimpleIndicator::calculate_close_series(std::iter::repeat_n(100.0, 40), 12, 26, 9)
                .expect("valid MACD periods");

        assert!(values[..33].iter().all(Option::is_none));
        assert_eq!(values[33].expect("34th sample is ready").histogram, 0.0);
    }

    #[test]
    fn close_series_uses_close_sequence_without_lookahead() {
        let mut closes = vec![100.0; 40];
        closes.extend([90.0, 92.0, 95.0]);
        let before_future =
            MacdSimpleIndicator::calculate_close_series(closes.iter().copied(), 12, 26, 9)
                .expect("valid MACD periods");

        closes.push(130.0);
        let after_future = MacdSimpleIndicator::calculate_close_series(closes, 12, 26, 9)
            .expect("valid MACD periods");

        assert_eq!(before_future, after_future[..before_future.len()]);
    }
}
