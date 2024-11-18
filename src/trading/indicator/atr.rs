use super::rma;
use crate::trading::indicator::rma::RMA;
use ndarray::Array1;
use technical_indicators::indicators::;

/// ATR implementation matching PineScript
pub struct ATR {
    period: usize,
    prev_close: Option<f64>,
    rma: RMA, // 持有RMA实例而不是重新创建
}

impl ATR {
    pub fn new(period: usize) -> Self {
        ATR {
            period,
            prev_close: None,
            rma: RMA::new(period),
        }
    }

    fn true_range(&self, high: f64, low: f64, close: f64) -> f64 {
        match self.prev_close {
            Some(prev_close) => {
                let range1 = high - low;
                let range2 = (high - prev_close).abs();
                let range3 = (low - prev_close).abs();
                range1.max(range2).max(range3)
            }
            None => high - low,
        }
    }

    pub fn next(&mut self, high: f64, low: f64, close: f64) -> f64 {
        let tr = self.true_range(high, low, close);
        println!("true_range:{:?}", tr);

        let ma = MovingAverage::new(14, MovingAverageType::RMA);
        let value = ma.calculate(&prices);
        // 使用RMA计算ATR
        let atr = self.rma.next(tr);

        // 更新前一个收盘价
        self.prev_close = Some(close);
        println!("记录上一个收盘价:{:?}", self.prev_close);

        atr
    }
}
