use crate::strategy_common::SignalResult;
use crate::CandleItem;

use super::strategy::RsiDivergenceStrategy;
use super::types::{DivergenceType, RsiDivergenceBacktestTuning, RsiDivergenceSignalSnapshot};

/// RSI Divergence回测适配器
pub struct RsiDivergenceBacktestAdapter {
    pub tuning: RsiDivergenceBacktestTuning,
    cooldown_remaining: usize, // 🔧 新增：冷却期计数器
    pivot_pair_max_lag: Option<usize>,
}

impl RsiDivergenceBacktestAdapter {
    pub fn new(tuning: RsiDivergenceBacktestTuning) -> Self {
        Self {
            tuning,
            cooldown_remaining: 0, // 🔧 初始化冷却期
            pivot_pair_max_lag: None,
        }
    }

    /// 构造启用价格/RSI枢轴时间配对的回测适配器。
    pub fn new_with_pivot_pair_lag(
        tuning: RsiDivergenceBacktestTuning,
        pivot_pair_max_lag: usize,
    ) -> Self {
        Self {
            tuning,
            cooldown_remaining: 0,
            pivot_pair_max_lag: Some(pivot_pair_max_lag),
        }
    }

    pub fn get_signal(&mut self, candles: &[CandleItem], idx: usize) -> Option<SignalResult> {
        // 🔧 冷却期检查：避免同一峰值附近重复开仓
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return None;
        }

        let snapshot = self.build_snapshot(candles, idx)?;
        let thresholds = self.tuning.thresholds();
        let decision = RsiDivergenceStrategy::evaluate(&thresholds, &snapshot);

        if matches!(
            decision.action,
            super::types::DivergenceAction::Long | super::types::DivergenceAction::Short
        ) {
            let ts = candles[idx].ts;
            // 🔧 触发信号后设置冷却期：5根K线 = 25分钟
            self.cooldown_remaining = 5;
            Some(decision.to_signal(&snapshot, &thresholds, ts))
        } else {
            None
        }
    }

    fn build_snapshot(
        &self,
        candles: &[CandleItem],
        idx: usize,
    ) -> Option<RsiDivergenceSignalSnapshot> {
        // 需要足够的历史数据
        let min_bars = self.tuning.rsi_period + self.tuning.lookback_period + 5;
        if idx < min_bars {
            return None;
        }

        let current = &candles[idx];
        let price = current.c;

        // 计算RSI序列
        let rsi_series = self.compute_rsi_series(candles, idx);
        if rsi_series.len() < self.tuning.lookback_period {
            return None;
        }

        let rsi = *rsi_series.last().unwrap_or(&50.0);
        let atr = self.compute_atr(candles, idx);

        // 检测背离：在最近 lookback_period 根K线内找价格和RSI的峰值/谷值
        let lookback = self.tuning.lookback_period.min(rsi_series.len());
        let start_idx = idx.saturating_sub(lookback);

        // 找价格的两个低点和两个高点
        let (price_lows, price_highs) = find_pivots(&candles[start_idx..=idx], 2);
        let (rsi_lows, rsi_highs) = find_pivots_from_values(&rsi_series, 2);

        // 检测背离
        let divergence_type = detect_divergence_with_pair_lag(
            &price_lows,
            &price_highs,
            &rsi_lows,
            &rsi_highs,
            self.tuning.rsi_period,
            start_idx,
            self.pivot_pair_max_lag,
        );

        // 提取数据用于snapshot
        let (current_price_low, prev_price_low, price_low_idx) =
            extract_two(&price_lows, |p| p.value);
        let (current_price_high, prev_price_high, price_high_idx) =
            extract_two(&price_highs, |p| p.value);
        let (current_rsi_low, prev_rsi_low, rsi_low_idx) = extract_two(&rsi_lows, |p| p.value);
        let (current_rsi_high, prev_rsi_high, rsi_high_idx) = extract_two(&rsi_highs, |p| p.value);

        Some(RsiDivergenceSignalSnapshot {
            price,
            rsi,
            atr,
            divergence_type,
            price_low_idx,
            price_high_idx,
            rsi_low_idx,
            rsi_high_idx,
            current_price_low,
            current_price_high,
            prev_price_low,
            prev_price_high,
            current_rsi_low,
            current_rsi_high,
            prev_rsi_low,
            prev_rsi_high,
        })
    }

    /// 计算Wilder RSI序列
    fn compute_rsi_series(&self, candles: &[CandleItem], idx: usize) -> Vec<f64> {
        let period = self.tuning.rsi_period;
        if idx < period + 1 {
            return vec![];
        }

        let closes: Vec<f64> = candles[..=idx].iter().map(|c| c.c).collect();
        let mut rsi_values = Vec::new();

        // 计算初始平均涨跌
        let mut avg_gain = 0.0;
        let mut avg_loss = 0.0;
        for i in 1..=period {
            let diff = closes[i] - closes[i - 1];
            if diff > 0.0 {
                avg_gain += diff;
            } else {
                avg_loss += -diff;
            }
        }
        avg_gain /= period as f64;
        avg_loss /= period as f64;

        // Wilder平滑
        for i in (period + 1)..closes.len() {
            let diff = closes[i] - closes[i - 1];
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };
            avg_gain = (avg_gain * (period - 1) as f64 + gain) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 + loss) / period as f64;

            let rsi = if avg_loss == 0.0 {
                100.0
            } else {
                let rs = avg_gain / avg_loss;
                100.0 - 100.0 / (1.0 + rs)
            };
            rsi_values.push(rsi);
        }

        rsi_values
    }

    /// 计算ATR
    fn compute_atr(&self, candles: &[CandleItem], idx: usize) -> f64 {
        let period = self.tuning.atr_period.min(idx);
        if period == 0 || idx == 0 {
            return candles[idx].h - candles[idx].l;
        }
        let start = idx.saturating_sub(period).max(1);
        let mut sum = 0.0;
        for i in start..=idx {
            let tr = (candles[i].h - candles[i].l)
                .max((candles[i].h - candles[i - 1].c).abs())
                .max((candles[i].l - candles[i - 1].c).abs());
            sum += tr;
        }
        sum / (idx - start + 1) as f64
    }
}

/// 峰值/谷值信息
#[derive(Debug, Clone, Copy)]
struct Pivot {
    idx: usize,
    value: f64,
}

/// 从K线序列中找价格枢轴点（简化版：使用滑动窗口最值）
fn find_pivots(candles: &[CandleItem], num_pivots: usize) -> (Vec<Pivot>, Vec<Pivot>) {
    let window = 5; // 🔧 修复: 左右各5根K线 (25分钟窗口，减少噪音)
    let mut lows = Vec::new();
    let mut highs = Vec::new();

    if candles.len() < 2 * window + 1 {
        return (lows, highs);
    }

    // 🔧 修复前视偏差: 确保峰值右侧至少有window根K线确认
    // 原代码: window..(candles.len() - window) 会使用未来数据
    // 修复后: 只检测到 candles.len() - window - window，确保右侧有足够K线
    for i in window..(candles.len().saturating_sub(window * 2)) {
        let is_low = (i - window..i)
            .chain(i + 1..=i + window)
            .all(|j| candles[j].l >= candles[i].l);
        let is_high = (i - window..i)
            .chain(i + 1..=i + window)
            .all(|j| candles[j].h <= candles[i].h);

        if is_low {
            lows.push(Pivot {
                idx: i,
                value: candles[i].l,
            });
        }
        if is_high {
            highs.push(Pivot {
                idx: i,
                value: candles[i].h,
            });
        }
    }

    // 只保留最近的num_pivots个
    let low_start = lows.len().saturating_sub(num_pivots);
    let high_start = highs.len().saturating_sub(num_pivots);

    (lows[low_start..].to_vec(), highs[high_start..].to_vec())
}

/// 从数值序列中找枢轴点
fn find_pivots_from_values(values: &[f64], num_pivots: usize) -> (Vec<Pivot>, Vec<Pivot>) {
    let window = 5; // 🔧 修复: 与价格峰值检测保持一致
    let mut lows = Vec::new();
    let mut highs = Vec::new();

    if values.len() < 2 * window + 1 {
        return (lows, highs);
    }

    // 🔧 修复前视偏差: 确保峰值右侧至少有window根数据确认
    for i in window..(values.len().saturating_sub(window * 2)) {
        let is_low = (i - window..i)
            .chain(i + 1..=i + window)
            .all(|j| values[j] >= values[i]);
        let is_high = (i - window..i)
            .chain(i + 1..=i + window)
            .all(|j| values[j] <= values[i]);

        if is_low {
            lows.push(Pivot {
                idx: i,
                value: values[i],
            });
        }
        if is_high {
            highs.push(Pivot {
                idx: i,
                value: values[i],
            });
        }
    }

    let low_start = lows.len().saturating_sub(num_pivots);
    let high_start = highs.len().saturating_sub(num_pivots);

    (lows[low_start..].to_vec(), highs[high_start..].to_vec())
}

/// 从枢轴序列中提取最近两个（当前和前一个）
fn extract_two<T, F: Fn(&Pivot) -> T>(pivots: &[Pivot], f: F) -> (T, T, usize)
where
    T: Default + Copy,
{
    let n = pivots.len();
    if n >= 2 {
        (f(&pivots[n - 1]), f(&pivots[n - 2]), pivots[n - 1].idx)
    } else if n == 1 {
        (f(&pivots[0]), T::default(), pivots[0].idx)
    } else {
        (T::default(), T::default(), 0)
    }
}

/// 检测背离
fn detect_divergence_with_pair_lag(
    price_lows: &[Pivot],
    price_highs: &[Pivot],
    rsi_lows: &[Pivot],
    rsi_highs: &[Pivot],
    rsi_period: usize,
    price_window_start_idx: usize,
    pivot_pair_max_lag: Option<usize>,
) -> DivergenceType {
    // 常规看涨背离：价格新低 + RSI未新低
    if price_lows.len() >= 2 && rsi_lows.len() >= 2 {
        let n = price_lows.len();
        let rn = rsi_lows.len();
        let price_new_low = price_lows[n - 1].value < price_lows[n - 2].value;
        let rsi_higher_low = rsi_lows[rn - 1].value > rsi_lows[rn - 2].value;
        let paired = pivots_are_paired(
            &price_lows[n - 2..],
            &rsi_lows[rn - 2..],
            rsi_period,
            price_window_start_idx,
            pivot_pair_max_lag,
        );

        if price_new_low && rsi_higher_low && paired {
            return DivergenceType::BullishRegular;
        }
    }

    // 常规看跌背离：价格新高 + RSI未新高
    if price_highs.len() >= 2 && rsi_highs.len() >= 2 {
        let n = price_highs.len();
        let rn = rsi_highs.len();
        let price_new_high = price_highs[n - 1].value > price_highs[n - 2].value;
        let rsi_lower_high = rsi_highs[rn - 1].value < rsi_highs[rn - 2].value;
        let paired = pivots_are_paired(
            &price_highs[n - 2..],
            &rsi_highs[rn - 2..],
            rsi_period,
            price_window_start_idx,
            pivot_pair_max_lag,
        );

        if price_new_high && rsi_lower_high && paired {
            return DivergenceType::BearishRegular;
        }
    }

    DivergenceType::None
}

fn pivots_are_paired(
    price_pivots: &[Pivot],
    rsi_pivots: &[Pivot],
    rsi_period: usize,
    price_window_start_idx: usize,
    pivot_pair_max_lag: Option<usize>,
) -> bool {
    let Some(max_lag) = pivot_pair_max_lag else {
        return true;
    };
    if price_pivots.len() != rsi_pivots.len() {
        return false;
    }

    price_pivots
        .iter()
        .zip(rsi_pivots.iter())
        .all(|(price, rsi)| {
            let price_idx = price_window_start_idx + price.idx;
            let rsi_idx = rsi_period + 1 + rsi.idx;
            price_idx.abs_diff(rsi_idx) <= max_lag
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_lag_filter_rejects_stale_rsi_pivots() {
        let price_lows = [
            Pivot {
                idx: 28,
                value: 100.0,
            },
            Pivot {
                idx: 30,
                value: 98.0,
            },
        ];
        let rsi_lows = [
            Pivot {
                idx: 5,
                value: 25.0,
            },
            Pivot {
                idx: 7,
                value: 30.0,
            },
        ];

        let divergence =
            detect_divergence_with_pair_lag(&price_lows, &[], &rsi_lows, &[], 6, 10, Some(5));

        assert_eq!(divergence, DivergenceType::None);
    }

    #[test]
    fn paired_lag_filter_accepts_close_pivots() {
        let price_lows = [
            Pivot {
                idx: 18,
                value: 100.0,
            },
            Pivot {
                idx: 20,
                value: 98.0,
            },
        ];
        let rsi_lows = [
            Pivot {
                idx: 21,
                value: 25.0,
            },
            Pivot {
                idx: 23,
                value: 30.0,
            },
        ];

        let divergence =
            detect_divergence_with_pair_lag(&price_lows, &[], &rsi_lows, &[], 6, 10, Some(2));

        assert_eq!(divergence, DivergenceType::BullishRegular);
    }
}
