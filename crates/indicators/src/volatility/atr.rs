use std::collections::VecDeque;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtrError {
    #[error("Invalid period: {0}, must be greater than 0")]
    InvalidPeriod(usize),
}

#[derive(Debug, Clone)]
pub struct ATR {
    period: usize,
    alpha: f64,
    current: f64,
    prev_close: Option<f64>,
    buffer: VecDeque<f64>,
}

impl ATR {
    pub fn new(period: usize) -> Result<Self, AtrError> {
        if period == 0 {
            return Err(AtrError::InvalidPeriod(0));
        }
        Ok(Self {
            period,
            alpha: 1.0 / period as f64,
            current: 0.0,
            prev_close: None,
            buffer: VecDeque::with_capacity(period + 1),
        })
    }

    pub fn reset(&mut self) {
        self.current = 0.0;
        self.prev_close = None;
        self.buffer.clear();
    }

    /// 处理K线数据，始终返回f64：
    /// - 数据不足时返回0.00
    /// - 有效数据返回实际ATR值
    ///
    /// 只需传入最新K线的高低收盘价，指标内部会维护所需的历史数据
    pub fn next(&mut self, high: f64, low: f64, close: f64) -> f64 {
        let tr = self.calculate_tr(high, low, close);
        self.buffer.push_back(tr);

        if self.buffer.len() < self.period {
            self.prev_close = Some(close);
            return 0.0;
        }

        // 初始化阶段计算SMA
        if self.current == 0.0 {
            self.current = self.buffer.iter().take(self.period).sum::<f64>() / self.period as f64;
        } else {
            self.current = self.alpha.mul_add(tr, (1.0 - self.alpha) * self.current);
        }

        // 维护缓冲区大小
        if self.buffer.len() > self.period {
            self.buffer.pop_front();
        }

        self.prev_close = Some(close);
        self.current
    }

    /// 获取当前ATR值（可能为0.00）
    pub fn value(&self) -> f64 {
        self.current
    }

    /// 检查是否有足够的数据来计算有效的ATR值
    pub fn is_ready(&self) -> bool {
        self.buffer.len() >= self.period
    }

    /// 获取当前ATR值，如果数据不足返回None（类似PineScript的na）
    pub fn value_optional(&self) -> Option<f64> {
        if self.is_ready() && self.current > 0.0 {
            Some(self.current)
        } else {
            None
        }
    }

    fn calculate_tr(&self, high: f64, low: f64, close: f64) -> f64 {
        match self.prev_close {
            None => high - low,
            Some(prev_close) => {
                let tr1 = high - low;
                let tr2 = (high - prev_close).abs();
                let tr3 = (low - prev_close).abs();
                tr1.max(tr2).max(tr3)
            }
        }
    }
}

impl fmt::Display for ATR {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ATR({}): {:.4}", self.period, self.current)
    }
}

// 测试用例
#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    #[test]
    fn test_initial_phase() {
        let mut atr = ATR::new(3).unwrap();

        // 前2根K线应该返回0.00
        assert_eq!(atr.next(10.0, 8.0, 9.0), 0.0);
        assert_eq!(atr.next(11.0, 9.0, 10.0), 0.0);

        // 第3根K线开始计算有效值
        let val = atr.next(12.0, 10.0, 11.0);
        assert!(approx_eq!(f64, val, 2.0, epsilon = 0.001));
    }

    #[test]
    fn test_full_calculation() {
        let test_data = vec![
            // (high, low, close, expected)
            (10.0, 8.0, 9.0, 0.0),                               // Bar 1
            (11.0, 9.0, 10.0, 0.0),                              // Bar 2
            (12.0, 10.0, 11.0, 2.0),                             // Bar 3: SMA(2, 2, 2) = 2.0
            (13.0, 11.0, 12.0, 2.0 * (2.0 / 3.0) + (2.0 / 3.0)), // Bar 4: RMA计算
        ];

        let mut atr = ATR::new(3).unwrap();

        for (idx, (h, l, c, expected)) in test_data.iter().enumerate() {
            let result = atr.next(*h, *l, *c);
            println!("Bar {}: {:.4} (expected: {:.4})", idx + 1, result, expected);
            assert!(approx_eq!(f64, result, *expected, epsilon = 0.001));
        }
    }
}
