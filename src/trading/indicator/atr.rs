use super::rma;
use crate::trading::indicator::rma::RMA;
use ndarray::Array1;

/// ATR implementation matching PineScript with period control
pub struct ATR {
    period: usize,
    prev_close: Option<f64>,
    rma: RMA,
    history: Vec<f64>, // 存储历史 TR 值
}

impl ATR {
    pub fn new(period: usize) -> Self {
        ATR {
            period,
            prev_close: None,
            rma: RMA::new(period),
            history: Vec::with_capacity(period),
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

    /// 重置计算器状态
    pub fn reset(&mut self) {
        self.prev_close = None;
        self.rma = RMA::new(self.period);
        self.history.clear();
    }

    /// 批量计算一组数据的 ATR，只使用最后 period 个数据点
    pub fn calculate_batch(&mut self, data: &[(f64, f64, f64)]) -> Vec<f64> {
        // 重置状态
        self.reset();

        // 如果数据少于周期，返回空结果
        if data.len() < self.period {
            return Vec::new();
        }

        // 只取最后 period 个数据点
        let start_idx = data.len() - self.period;
        let relevant_data = &data[start_idx..];

        let mut results = Vec::with_capacity(self.period);

        // 计算每个数据点的 ATR
        for &(high, low, close) in relevant_data {
            let atr = self.next(high, low, close);
            results.push(atr);
        }

        results
    }

    pub fn next(&mut self, high: f64, low: f64, close: f64) -> f64 {
        // 计算 True Range
        let tr = self.true_range(high, low, close);

        // 添加到历史记录
        self.history.push(tr);

        // 如果历史记录超过周期，移除最旧的数据
        if self.history.len() > self.period {
            self.history.remove(0);
        }

        // 使用 RMA 计算 ATR
        let atr = if self.history.len() == self.period {
            // 当积累了足够的数据时，计算 RMA
            self.rma.next(tr)
        } else {
            // 数据不足时返回简单平均
            self.history.iter().sum::<f64>() / self.history.len() as f64
        };

        // 更新前一个收盘价
        self.prev_close = Some(close);

        atr
    }

    /// 获取当前的历史数据长度
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// 检查是否已经积累了足够的数据
    pub fn is_ready(&self) -> bool {
        self.history.len() >= self.period
    }
}