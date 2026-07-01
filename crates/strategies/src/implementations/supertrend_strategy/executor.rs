use crate::strategy_common::SignalResult;
use crate::CandleItem;

use super::strategy::SuperTrendStrategy;
use super::types::{SuperTrendBacktestTuning, SuperTrendDirection, SuperTrendSignalSnapshot};

/// SuperTrend回测适配器
pub struct SuperTrendBacktestAdapter {
    pub tuning: SuperTrendBacktestTuning,
    prev_direction: SuperTrendDirection,
    prev_supertrend: f64,
    prev_upper_band: f64,
    prev_lower_band: f64,
}

impl SuperTrendBacktestAdapter {
    pub fn new(tuning: SuperTrendBacktestTuning) -> Self {
        Self {
            tuning,
            prev_direction: SuperTrendDirection::Flat,
            prev_supertrend: 0.0,
            prev_upper_band: 0.0,
            prev_lower_band: 0.0,
        }
    }

    pub fn get_signal(&mut self, candles: &[CandleItem], idx: usize) -> Option<SignalResult> {
        let snapshot = self.build_snapshot(candles, idx);
        let thresholds = self.tuning.thresholds();
        let decision = SuperTrendStrategy::evaluate(&thresholds, &snapshot);

        // 更新状态
        self.prev_direction = snapshot.current_direction;
        self.prev_supertrend = snapshot.supertrend_line;

        if matches!(
            decision.action,
            super::types::SuperTrendAction::Long | super::types::SuperTrendAction::Short
        ) {
            Some(decision.to_signal(&snapshot, &thresholds))
        } else {
            None
        }
    }

    fn build_snapshot(&mut self, candles: &[CandleItem], idx: usize) -> SuperTrendSignalSnapshot {
        let current = &candles[idx];
        let price = current.c;

        // 计算ATR
        let atr = self.compute_atr(candles, idx);

        // 计算基础带和上下轨
        let basic_band = (current.h + current.l) / 2.0;
        let mut upper_band = basic_band + self.tuning.atr_multiplier * atr;
        let mut lower_band = basic_band - self.tuning.atr_multiplier * atr;

        // SuperTrend规则：带线不能低于前值（上涨趋势）或高于前值（下跌趋势）
        if idx > 0 {
            if current.c > self.prev_upper_band {
                lower_band = lower_band.max(self.prev_lower_band);
            }
            if current.c < self.prev_lower_band {
                upper_band = upper_band.min(self.prev_upper_band);
            }
        }

        // 保存当前带线用于下一根
        self.prev_upper_band = upper_band;
        self.prev_lower_band = lower_band;

        // 确定当前趋势方向和SuperTrend线位置
        let (current_direction, supertrend_line) =
            if self.prev_direction == SuperTrendDirection::Flat {
                // 初始化：价格在上轨上方→多头，下轨下方→空头
                if price > upper_band {
                    (SuperTrendDirection::Up, lower_band)
                } else if price < lower_band {
                    (SuperTrendDirection::Down, upper_band)
                } else {
                    (SuperTrendDirection::Flat, basic_band)
                }
            } else if self.prev_direction == SuperTrendDirection::Up {
                // 当前多头，检查是否跌破SuperTrend线
                if price <= self.prev_supertrend {
                    (SuperTrendDirection::Down, upper_band)
                } else {
                    (SuperTrendDirection::Up, lower_band)
                }
            } else {
                // 当前空头，检查是否突破SuperTrend线
                if price >= self.prev_supertrend {
                    (SuperTrendDirection::Up, lower_band)
                } else {
                    (SuperTrendDirection::Down, upper_band)
                }
            };

        SuperTrendSignalSnapshot {
            price,
            atr,
            supertrend_line,
            current_direction,
            prev_direction: self.prev_direction,
            basic_band,
            upper_band,
            lower_band,
        }
    }

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
