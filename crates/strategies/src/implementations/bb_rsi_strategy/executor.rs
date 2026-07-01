use crate::strategy_common::SignalResult;
use crate::CandleItem;

use super::strategy::BbRsiStrategy;
use super::types::{BbRsiBacktestTuning, BbRsiSignalSnapshot};

/// Bollinger Bands + RSI回测适配器
pub struct BbRsiBacktestAdapter {
    pub tuning: BbRsiBacktestTuning,
    cooldown_remaining: usize, // 冷却期计数器
}

impl BbRsiBacktestAdapter {
    pub fn new(tuning: BbRsiBacktestTuning) -> Self {
        Self {
            tuning,
            cooldown_remaining: 0,
        }
    }

    pub fn get_signal(&mut self, candles: &[CandleItem], idx: usize) -> Option<SignalResult> {
        // 冷却期检查
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return None;
        }

        let snapshot = self.build_snapshot(candles, idx)?;
        let thresholds = self.tuning.thresholds();
        let decision = BbRsiStrategy::evaluate(&thresholds, &snapshot);

        if matches!(
            decision.action,
            super::types::BbRsiAction::Long | super::types::BbRsiAction::Short
        ) {
            let ts = candles[idx].ts;
            // 触发信号后设置冷却期
            self.cooldown_remaining = self.tuning.cooldown_candles;
            Some(decision.to_signal(&snapshot, &thresholds, ts))
        } else {
            None
        }
    }

    fn build_snapshot(&self, candles: &[CandleItem], idx: usize) -> Option<BbRsiSignalSnapshot> {
        // 需要足够的历史数据
        let min_bars = self
            .tuning
            .bb_period
            .max(self.tuning.rsi_period + 1)
            .max(20);
        if idx < min_bars {
            return None;
        }

        let current = &candles[idx];
        let price = current.c;

        // 计算布林带
        let (bb_upper, bb_middle, bb_lower) = self.compute_bollinger_bands(candles, idx)?;
        let bb_width = bb_upper - bb_lower;

        // 价格在布林带中的位置 (0=下轨, 0.5=中轨, 1=上轨)
        let price_bb_position = if bb_width > 0.0 {
            ((price - bb_lower) / bb_width).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // 计算RSI
        let rsi = self.compute_rsi(candles, idx)?;

        // 计算ATR
        let atr = self.compute_atr(candles, idx);

        Some(BbRsiSignalSnapshot {
            price,
            rsi,
            atr,
            bb_upper,
            bb_middle,
            bb_lower,
            bb_width,
            price_bb_position,
        })
    }

    /// 计算布林带 (SMA ± std_dev × stddev)
    fn compute_bollinger_bands(
        &self,
        candles: &[CandleItem],
        idx: usize,
    ) -> Option<(f64, f64, f64)> {
        let period = self.tuning.bb_period;
        if idx < period {
            return None;
        }

        let start = idx + 1 - period;
        let closes: Vec<f64> = candles[start..=idx].iter().map(|c| c.c).collect();

        // 计算SMA
        let sma: f64 = closes.iter().sum::<f64>() / period as f64;

        // 计算标准差
        let variance: f64 = closes.iter().map(|c| (c - sma).powi(2)).sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();

        let bb_upper = sma + self.tuning.bb_std_dev * std_dev;
        let bb_lower = sma - self.tuning.bb_std_dev * std_dev;

        Some((bb_upper, sma, bb_lower))
    }

    /// 计算RSI (Wilder平滑法)
    fn compute_rsi(&self, candles: &[CandleItem], idx: usize) -> Option<f64> {
        let period = self.tuning.rsi_period;
        if idx < period + 1 {
            return None;
        }

        let start = idx + 1 - period - 1;
        let closes: Vec<f64> = candles[start..=idx].iter().map(|c| c.c).collect();

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
        if period + 1 < closes.len() {
            for i in (period + 1)..closes.len() {
                let diff = closes[i] - closes[i - 1];
                let gain = if diff > 0.0 { diff } else { 0.0 };
                let loss = if diff < 0.0 { -diff } else { 0.0 };
                avg_gain = (avg_gain * (period - 1) as f64 + gain) / period as f64;
                avg_loss = (avg_loss * (period - 1) as f64 + loss) / period as f64;
            }
        }

        let rsi = if avg_loss == 0.0 {
            100.0
        } else {
            let rs = avg_gain / avg_loss;
            100.0 - 100.0 / (1.0 + rs)
        };

        Some(rsi)
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
