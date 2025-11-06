use rust_quant_common::utils::time as time_util;
use rust_quant_market::models::candles::CandlesEntity;
use ta::indicators::MovingAverageConvergenceDivergence;
use ta::Next;

pub struct MacdSimpleIndicator {}

impl MacdSimpleIndicator {
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
}
