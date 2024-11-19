use super::sma;
use ndarray::Array1;
/// Rolling Moving Average implementation based on PineScript
pub struct RMA {
    period: usize,
    current_value: Option<f64>,
    history: Vec<f64>,
}

impl RMA {
    pub fn new(period: usize) -> Self {
        RMA {
            period,
            current_value: None,
            history: Vec::with_capacity(period),
        }
    }

    pub fn next(&mut self, value: f64) -> f64 {
        // 收集数据直到满足计算SMA所需的周期
        self.history.push(value);

        // 第一次计算时使用SMA初始化
        if self.current_value.is_none() {
            if self.history.len() >= self.period {
                // 计算初始的SMA值
                let sum: f64 = self.history.iter().sum();
                let sma = sum / self.period as f64;
                self.current_value = Some(sma);

                // 保持最后一个值用于下次计算
                let last_value = *self.history.last().unwrap();
                self.history.clear();
                self.history.push(last_value);

                return sma;
            }
            // 在收集足够数据之前，返回初始累积平均值
            let sum: f64 = self.history.iter().sum();
            return sum / self.history.len() as f64;
        }

        // 使用 Wilder 的计算公式: (current + (period-1) * previous) / period
        let prev_value = self.current_value.unwrap();
        let new_value = (value + (self.period as f64 - 1.0) * prev_value) / self.period as f64;
        self.current_value = Some(new_value);

        new_value
    }
}