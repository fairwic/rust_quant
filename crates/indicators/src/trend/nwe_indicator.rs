use std::collections::VecDeque;

/// Nadaraya–Watson Envelope (non-repainting)
/// - Gaussian-kernel weighted mean as the centerline ("out")
/// - Envelope width is SMA of absolute error |src - out| over (window - 1), scaled by `mult`
///
/// Notes:
/// - Returns (0.0, 0.0) until enough data is accumulated to form a stable estimate
///   (i.e., until the MAE window is full).
/// - For intermediate warm-up (less than full window), the kernel mean uses
///   only available weights and re-normalizes by the partial weight sum.
#[derive(Debug, Clone)]
pub struct NweIndicator {
    bandwidth_h: f64,
    mult: f64,
    window: usize,           // max bars back for kernel mean (<= 500 is recommended)
    mae_period: usize,       // typically window - 1
    weights: Vec<f64>,       // precomputed Gaussian weights w[i] for i in [0, window)
    values: VecDeque<f64>,   // recent closes, newest at back
    abs_errs: VecDeque<f64>, // buffer for |src - out| to compute MAE
    abs_err_sum: f64,        // rolling sum for MAE
}

impl NweIndicator {
    pub fn new(bandwidth_h: f64, mult: f64, window: usize) -> Self {
        let win = window.clamp(2, 500);
        let mae_period = (win - 1).max(1);
        let weights = Self::precompute_weights(bandwidth_h, win);

        Self {
            bandwidth_h,
            mult,
            window: win,
            mae_period,
            weights,
            values: VecDeque::with_capacity(win + 1),
            abs_errs: VecDeque::with_capacity(mae_period + 1),
            abs_err_sum: 0.0,
        }
    }

    fn precompute_weights(h: f64, window: usize) -> Vec<f64> {
        // Gaussian: exp(-(x^2) / (2 h^2)) with x = lag index
        // i = 0 corresponds to current bar (largest weight)
        if h <= 0.0 {
            // Degenerate: make only i=0 significant to avoid div-by-zero; others near 0
            let mut w = vec![0.0; window];
            w[0] = 1.0;
            return w;
        }
        let denom = 2.0 * h * h;
        (0..window)
            .map(|i| (-(i as f64 * i as f64) / denom).exp())
            .collect()
    }

    /// Push one new close value and get (upper, lower).
    /// Returns (0.0, 0.0) until the MAE window is fully populated.
    pub fn next(&mut self, close: f64) -> (f64, f64) {
        self.values.push_back(close);
        if self.values.len() > self.window {
            self.values.pop_front();
        }

        // Compute kernel mean using available values and corresponding weights
        let m = self.values.len();
        let (out, ok) = self.kernel_mean(m);
        if !ok {
            return (0.0, 0.0);
        }

        // Update MAE buffer
        let abs_err = (close - out).abs();
        self.abs_errs.push_back(abs_err);
        self.abs_err_sum += abs_err;
        if self.abs_errs.len() > self.mae_period {
            if let Some(removed) = self.abs_errs.pop_front() {
                self.abs_err_sum -= removed;
            }
        }

        // Require full MAE period to produce stable envelope
        if self.abs_errs.len() < self.mae_period {
            return (0.0, 0.0);
        }

        // MAE = 平均绝对误差 × 倍数（对齐 PineScript: ta.sma(abs(src - out), 499) * mult）
        let mae = (self.abs_err_sum / self.mae_period as f64) * self.mult;
        let upper = out + mae;
        let lower = out - mae;
        (upper, lower)
    }

    /// 获取当前状态的调试信息
    pub fn debug_info(&self) -> NweDebugInfo {
        NweDebugInfo {
            bandwidth_h: self.bandwidth_h,
            mult: self.mult,
            window: self.window,
            mae_period: self.mae_period,
            values_len: self.values.len(),
            abs_errs_len: self.abs_errs.len(),
            abs_err_sum: self.abs_err_sum,
            first_weight: self.weights.first().copied().unwrap_or(0.0),
            last_weight: self.weights.last().copied().unwrap_or(0.0),
            weights_sum: self.weights.iter().sum(),
        }
    }

    fn kernel_mean(&self, available: usize) -> (f64, bool) {
        if available == 0 {
            return (0.0, false);
        }
        // values: oldest .. newest; newest index = available - 1
        // weights: w[0] aligns with newest (lag 0)
        let mut sum = 0.0;
        let mut sumw = 0.0;

        // Iterate lag j from 0..available
        // 对齐 PineScript: src[j] × weights[j], j=0 是最新
        for j in 0..available {
            let price = self.values[available - 1 - j]; // newest first
            let w = self.weights[j];
            sum += price * w;
            sumw += w;
        }

        if sumw == 0.0 {
            return (0.0, false);
        }
        (sum / sumw, true)
    }

    pub fn reset(&mut self) {
        self.values.clear();
        self.abs_errs.clear();
        self.abs_err_sum = 0.0;
    }
}

/// NWE 指标调试信息
#[derive(Debug, Clone)]
pub struct NweDebugInfo {
    pub bandwidth_h: f64,
    pub mult: f64,
    pub window: usize,
    pub mae_period: usize,
    pub values_len: usize,
    pub abs_errs_len: usize,
    pub abs_err_sum: f64,
    pub first_weight: f64,
    pub last_weight: f64,
    pub weights_sum: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nwe_basic_progression() {
        let mut nwe = NweIndicator::new(8.0, 3.0, 500);
        // Feed a simple ramp; initial outputs will be (0,0) until MAE window fills
        for i in 0..600 {
            let (_u, _l) = nwe.next(i as f64);
        }
        // After warm-up, envelopes should be non-zero
        let (u, l) = nwe.next(600.0);
        assert!(u > l);
    }
}
