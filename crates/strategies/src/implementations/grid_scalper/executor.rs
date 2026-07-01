use crate::framework::backtest::types::{BacktestResult, BasicRiskStrategyConfig};
use rust_quant_domain::value_objects::CandleItem;
use rust_quant_domain::SignalResult;

use super::strategy::GridScalperStrategy;
use super::types::{GridScalperBacktestTuning, GridScalperSignalSnapshot};

/// 网格 Scalper 回测适配器
pub struct GridScalperBacktestAdapter {
    pub tuning: GridScalperBacktestTuning,
    cooldown_counter: usize,
    last_trade_ts: i64,
}

impl GridScalperBacktestAdapter {
    pub fn new(tuning: GridScalperBacktestTuning) -> Self {
        Self {
            tuning,
            cooldown_counter: 0,
            last_trade_ts: 0,
        }
    }

    pub fn get_signal(&mut self, candles: &[CandleItem], idx: usize) -> Option<SignalResult> {
        // 冷却期检查
        if self.cooldown_counter > 0 {
            self.cooldown_counter -= 1;
            return None;
        }

        let snapshot = self.build_snapshot(candles, idx);
        let thresholds = self.tuning.thresholds();
        let decision = GridScalperStrategy::evaluate(&thresholds, &snapshot);

        if matches!(
            decision.action,
            super::types::GridAction::BuyGrid | super::types::GridAction::SellGrid
        ) {
            self.cooldown_counter = self.tuning.grid_cooldown;
            self.last_trade_ts = candles[idx].ts;
            Some(decision.to_signal(snapshot.price, &thresholds))
        } else {
            None
        }
    }

    fn build_snapshot(&self, candles: &[CandleItem], idx: usize) -> GridScalperSignalSnapshot {
        let current = &candles[idx];
        let price = current.c;

        // 计算ATR
        let atr = self.compute_atr(candles, idx);

        // 检测震荡模式
        let lookback = self.tuning.ranging_lookback.min(idx);
        let range_start = idx.saturating_sub(lookback);
        let recent_candles = &candles[range_start..=idx];
        let high = recent_candles
            .iter()
            .map(|c| c.h)
            .fold(f64::NEG_INFINITY, f64::max);
        let low = recent_candles
            .iter()
            .map(|c| c.l)
            .fold(f64::INFINITY, f64::min);
        let recent_range_pct = (high - low) / low.max(1.0);
        let in_ranging_mode = recent_range_pct < self.tuning.ranging_threshold_pct;

        // 计算网格中心和边界
        let grid_center = (high + low) / 2.0;
        let half_width = grid_center * self.tuning.grid_width_pct / 2.0;
        let grid_upper = grid_center + half_width;
        let grid_lower = grid_center - half_width;

        let price_to_center_pct = (price - grid_center) / grid_center;

        GridScalperSignalSnapshot {
            price,
            atr,
            grid_center,
            grid_upper,
            grid_lower,
            in_ranging_mode,
            price_to_center_pct,
            recent_range_pct,
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

/// 便捷函数：运行网格策略回测（简化版，直接统计信号）
pub fn run_grid_backtest(
    inst_id: &str,
    candles: &[CandleItem],
    _risk_config: BasicRiskStrategyConfig,
    tuning: GridScalperBacktestTuning,
) -> BacktestResult {
    let mut adapter = GridScalperBacktestAdapter::new(tuning);
    let mut result = BacktestResult::default();
    result.inst_id = inst_id.to_string();

    for (idx, _candle) in candles.iter().enumerate().skip(20) {
        if let Some(signal) = adapter.get_signal(candles, idx) {
            result.signals.push(signal);
        }
    }

    result
}
