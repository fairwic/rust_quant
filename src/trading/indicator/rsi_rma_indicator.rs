use crate::trading::indicator::sma::Sma;

/// RMA (Relative Moving Average) implementation matching TradingView's ta.rma()
#[derive(Debug, Clone)]
struct TvRma {
    length: usize,
    alpha: f64,
    sum: Option<f64>,
    values: Vec<f64>,
}

impl TvRma {
    fn new(length: usize) -> Self {
        Self {
            length,
            alpha: 1.0 / length as f64,
            sum: None,
            values: Vec::with_capacity(length * 2), // 保留更多历史数据用于SMA计算
        }
    }

    // 完全按照Pine Script的方式计算SMA
    fn calculate_pine_sma(&self) -> f64 {
        let mut sum = 0.0;
        let start = self.values.len().saturating_sub(self.length);
        // 从最新的数据开始向前计算
        for i in (start..self.values.len()).rev() {
            sum += self.values[i] / self.length as f64;
        }
        sum
    }

    fn next(&mut self, value: f64) -> f64 {
        // 保存值用于SMA计算
        self.values.push(value);

        match self.sum {
            None => {
                // 当收集到足够的数据时，使用Pine Script方式计算SMA
                if self.values.len() >= self.length {
                    let sma = self.calculate_pine_sma();
                    self.sum = Some(sma);
                    if self.values.len() > self.length * 2 {
                        // 保持数据量在合理范围内
                        self.values.drain(0..self.length);
                    }
                    sma
                } else {
                    value // 在收集数据阶段返回当前值
                }
            }
            Some(prev_sum) => {
                // 使用Pine Script的RMA公式
                let new_sum = self.alpha * value + (1.0 - self.alpha) * prev_sum;
                self.sum = Some(new_sum);
                new_sum
            }
        }
    }
}

/// RSI indicator using RMA (Relative Moving Average) for calculations
/// Implements the exact same logic as TradingView's Pine Script RSI
#[derive(Debug, Clone)]
pub struct RsiIndicator {
    length: usize,
    up_rma: TvRma,
    down_rma: TvRma,
    prev_value: Option<f64>,
    debug: bool,
}

impl RsiIndicator {
    pub fn new(length: usize) -> Self {
        Self {
            length,
            up_rma: TvRma::new(length),
            down_rma: TvRma::new(length),
            prev_value: None,
            debug: false,
        }
    }

    /// Calculate next RSI value based on a new price input
    /// Follows TradingView Pine Script logic exactly:
    /// change = ta.change(source)
    /// up = ta.rma(math.max(change, 0), length)
    /// down = ta.rma(-math.min(change, 0), length)
    /// rsi = down == 0 ? 100 : up == 0 ? 0 : 100 - (100 / (1 + up / down))
    pub fn next(&mut self, value: f64) -> f64 {
        // Calculate price change (ta.change equivalent)
        let change = match self.prev_value {
            Some(prev) => value - prev,
            None => {
                self.prev_value = Some(value);
                if self.debug {
                    println!("First value: {}, no change calculated yet", value);
                }
                0.0
            }
        };
        self.prev_value = Some(value);

        if self.debug {
            println!("Price: {:.2}, Change: {:.2}", value, change);
        }

        // Exactly match Pine Script's math.max and math.min behavior
        let up = if change > 0.0 { change } else { 0.0 };
        let down = if change < 0.0 { -change } else { 0.0 };

        if self.debug {
            println!("Up movement: {:.2}, Down movement: {:.2}", up, down);
        }

        // Apply RMA exactly as TradingView does
        let up_avg = self.up_rma.next(up);
        let down_avg = self.down_rma.next(down);

        if self.debug {
            println!("Up RMA: {:.2}, Down RMA: {:.2}", up_avg, down_avg);
        }

        // Calculate RSI using exact Pine Script formula
        let rsi = if down_avg == 0.0 {
            100.0
        } else if up_avg == 0.0 {
            0.0
        } else {
            100.0 - (100.0 / (1.0 + up_avg / down_avg))
        };

        if self.debug {
            println!("RSI: {:.2}\n", rsi);
        }

        rsi
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pine_sma_calculation() {
        let mut rma = TvRma::new(14);
        let values = vec![
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
            45.61, 46.28,
        ];

        println!("===== Pine Script SMA Test =====");
        for value in values {
            let result = rma.next(value);
            println!("Value: {:.2}, Result: {:.2}", value, result);
        }
    }

    #[test]
    fn test_rsi_calculation() {
        let mut rsi = RsiIndicator::new(14);

        // TradingView documentation example data
        let prices = vec![
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
            45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64, 46.21, 46.25, 45.71, 46.45,
            45.78, 45.35,
        ];

        println!("===== RSI Calculation Test =====");
        for (i, &price) in prices.iter().enumerate() {
            let rsi_value = rsi.next(price);
            println!("#{}\tPrice: {:.2}\tRSI: {:.2}", i + 1, price, rsi_value);
        }
    }
}
