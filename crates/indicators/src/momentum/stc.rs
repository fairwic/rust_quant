use std::collections::VecDeque;

use ta::indicators::ExponentialMovingAverage;
use ta::Next;

/// Schaff Trend Cycle (STC) indicator
///
/// 实现与 TradingView Pine Script 版本一致的 STC 计算流程：
/// macd = ema(fast) - ema(slow)
/// k = stoch(macd, cycle_length)
/// d = ema(k, d1_length)
/// kd = stoch(d, cycle_length)
/// stc = ema(kd, d2_length)
/// stc := clamp(stc, 0, 100)
#[derive(Debug, Clone)]
pub struct StcIndicator {
    fast_ema: ExponentialMovingAverage,
    slow_ema: ExponentialMovingAverage,
    d1_ema: ExponentialMovingAverage,
    d2_ema: ExponentialMovingAverage,
    cycle_length: usize,
    macd_history: VecDeque<f64>,
    d_history: VecDeque<f64>,
}

impl StcIndicator {
    pub fn new(
        fast_length: usize,
        slow_length: usize,
        cycle_length: usize,
        d1_length: usize,
        d2_length: usize,
    ) -> Self {
        assert!(fast_length > 0, "fast_length must be > 0");
        assert!(slow_length > 0, "slow_length must be > 0");
        assert!(cycle_length > 0, "cycle_length must be > 0");
        assert!(d1_length > 0, "d1_length must be > 0");
        assert!(d2_length > 0, "d2_length must be > 0");

        let fast_ema = ExponentialMovingAverage::new(fast_length).expect("fast_length > 0");
        let slow_ema = ExponentialMovingAverage::new(slow_length).expect("slow_length > 0");
        let d1_ema = ExponentialMovingAverage::new(d1_length).expect("d1_length > 0");
        let d2_ema = ExponentialMovingAverage::new(d2_length).expect("d2_length > 0");

        Self {
            fast_ema,
            slow_ema,
            d1_ema,
            d2_ema,
            cycle_length,
            macd_history: VecDeque::with_capacity(cycle_length),
            d_history: VecDeque::with_capacity(cycle_length),
        }
    }

    /// 计算下一个 STC 值，返回区间大致为 [0, 100]
    pub fn next(&mut self, price: f64) -> f64 {
        let fast = self.fast_ema.next(price);
        let slow = self.slow_ema.next(price);
        let macd = fast - slow;

        Self::push_with_capacity(&mut self.macd_history, macd, self.cycle_length);
        let k = Self::stochastic(&self.macd_history);
        let d = self.d1_ema.next(k);

        Self::push_with_capacity(&mut self.d_history, d, self.cycle_length);
        let kd = Self::stochastic(&self.d_history);
        let stc = self.d2_ema.next(kd);

        stc.clamp(0.0, 100.0)
    }

    fn stochastic(values: &VecDeque<f64>) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let last = *values.back().unwrap_or(&0.0);
        let (min, max) = values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |acc, &v| {
            (acc.0.min(v), acc.1.max(v))
        });
        let range = max - min;
        if range.abs() < f64::EPSILON {
            0.0
        } else {
            ((last - min) / range) * 100.0
        }
    }

    fn push_with_capacity(buffer: &mut VecDeque<f64>, value: f64, capacity: usize) {
        if capacity == 0 {
            return;
        }
        if buffer.len() == capacity {
            buffer.pop_front();
        }
        buffer.push_back(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stc_runs_and_produces_finite_values() {
        let mut stc = StcIndicator::new(23, 50, 10, 3, 3);
        let prices = vec![
            100.0, 101.0, 102.5, 101.5, 103.0, 104.0, 103.5, 105.0, 104.5, 106.0, 107.0,
        ];

        for p in prices {
            let v = stc.next(p);
            assert!(v.is_finite());
        }
    }
}


