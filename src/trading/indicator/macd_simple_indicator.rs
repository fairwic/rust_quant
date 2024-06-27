use ta::indicators::MovingAverageConvergenceDivergence;
use ta::Next;
use crate::trading::model::market::candles::CandlesEntity;

pub struct MacdSimpleIndicator {}

impl MacdSimpleIndicator {
    pub fn calculate_macd(candles: &[CandlesEntity], fast_period: usize, slow_period: usize, signal_period: usize) -> Vec<(i64, f64, f64)> {
        let mut macd = MovingAverageConvergenceDivergence::new(fast_period, slow_period, signal_period).unwrap();
        let mut macd_values = Vec::with_capacity(candles.len());
        for candle in candles {
            let price = candle.c.parse::<f64>().unwrap_or(0.0);
            let macd_value = macd.next(price);
            macd_values.push((candle.ts, macd_value.macd, macd_value.signal));
        }
        macd_values
    }
}