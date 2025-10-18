use std::collections::VecDeque;
use std::fmt;
use thiserror::Error;

use crate::trading::indicator::atr::ATR;

#[derive(Debug, Error)]
pub enum AtrError {
    #[error("Invalid period: {0}, must be greater than 0")]
    InvalidPeriod(usize),
}

#[derive(Debug, Clone)]
pub struct ATRStopLoos {
    period: usize,
    multi: f64,
    atr: ATR,
}

impl ATRStopLoos {
    pub fn new(period: usize, multi: f64) -> Result<Self, AtrError> {
        if period == 0 {
            return Err(AtrError::InvalidPeriod(0));
        }
        Ok(Self {
            period,
            multi,
            atr: ATR::new(period).unwrap(),
        })
    }

    pub fn reset(&mut self) {
        self.atr.reset();
    }

    /// 处理K线数据，始终返回f64：
    /// - 数据不足时返回0.00
    /// - 有效数据返回实际ATR值
    ///
    /// 只需传入最新K线的高低收盘价，指标内部会维护所需的历史数据
    pub fn next(&mut self, high: f64, low: f64, close: f64) -> (f64, f64, f64) {
        let atr_value = self.atr.next(high, low, close);
        let short_stop = high + self.multi * atr_value;
        let long_stop = low - self.multi * atr_value;
        (short_stop, long_stop, atr_value)
    }
}
#[test]
fn test_atr_stop_loos() {
    let mut atr = ATRStopLoos::new(3, 1.0).unwrap();
    let result = atr.next(10.0, 8.0, 9.0);
    let result = atr.next(11.0, 8.0, 10.0);
    let result = atr.next(12.0, 8.0, 11.0);
    let result = atr.next(13.0, 8.0, 12.0);
    println!("result:{:#?}", result);
}
