use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;

use super::executor::SuperTrendBacktestAdapter;
use super::types::{
    SuperTrendAction, SuperTrendBacktestTuning, SuperTrendDecision, SuperTrendDirection,
    SuperTrendSignalSnapshot, SuperTrendThresholds,
};

/// SuperTrend策略核心逻辑
pub struct SuperTrendStrategy;

impl SuperTrendStrategy {
    /// 评估当前是否应该开仓
    pub fn evaluate(
        thresholds: &SuperTrendThresholds,
        snapshot: &SuperTrendSignalSnapshot,
    ) -> SuperTrendDecision {
        let mut reasons = Vec::new();

        // 检测趋势翻转
        let direction_changed = snapshot.current_direction != snapshot.prev_direction
            && snapshot.prev_direction != SuperTrendDirection::Flat;

        if !direction_changed {
            reasons.push(format!(
                "NO_CHANGE: direction={:?}, prev={:?}",
                snapshot.current_direction, snapshot.prev_direction
            ));
            return SuperTrendDecision {
                action: SuperTrendAction::Hold,
                reasons,
            };
        }

        // 趋势翻转发生
        match snapshot.current_direction {
            SuperTrendDirection::Up => {
                if !thresholds.allow_long {
                    reasons.push("LONG_DISABLED".to_string());
                    return SuperTrendDecision {
                        action: SuperTrendAction::Flat,
                        reasons,
                    };
                }
                reasons.push(format!(
                    "SUPERTREND_LONG: price={:.2} > st_line={:.2}",
                    snapshot.price, snapshot.supertrend_line
                ));
                reasons.push(format!("TREND_FLIP: Red→Green, atr={:.2}", snapshot.atr));
                SuperTrendDecision {
                    action: SuperTrendAction::Long,
                    reasons,
                }
            }
            SuperTrendDirection::Down => {
                if !thresholds.allow_short {
                    reasons.push("SHORT_DISABLED".to_string());
                    return SuperTrendDecision {
                        action: SuperTrendAction::Flat,
                        reasons,
                    };
                }
                reasons.push(format!(
                    "SUPERTREND_SHORT: price={:.2} < st_line={:.2}",
                    snapshot.price, snapshot.supertrend_line
                ));
                reasons.push(format!("TREND_FLIP: Green→Red, atr={:.2}", snapshot.atr));
                SuperTrendDecision {
                    action: SuperTrendAction::Short,
                    reasons,
                }
            }
            SuperTrendDirection::Flat => {
                reasons.push("INIT: waiting for first trend".to_string());
                SuperTrendDecision {
                    action: SuperTrendAction::Flat,
                    reasons,
                }
            }
        }
    }

    /// 运行回测（便捷方法）
    pub fn run_test(
        inst_id: &str,
        candles: &[CandleItem],
        risk_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        Self::run_test_with_tuning(
            inst_id,
            candles,
            risk_config,
            SuperTrendBacktestTuning::default(),
        )
    }

    /// 运行回测（带参数调优）
    pub fn run_test_with_tuning(
        inst_id: &str,
        candles: &[CandleItem],
        risk_config: BasicRiskStrategyConfig,
        tuning: SuperTrendBacktestTuning,
    ) -> BackTestResult {
        let adapter = SuperTrendBacktestAdapterWrapper::new(tuning);
        run_indicator_strategy_backtest(inst_id, adapter, candles, risk_config)
    }
}

/// 包装器：实现 IndicatorStrategyBacktest trait
struct SuperTrendBacktestAdapterWrapper {
    adapter: SuperTrendBacktestAdapter,
}

impl SuperTrendBacktestAdapterWrapper {
    fn new(tuning: SuperTrendBacktestTuning) -> Self {
        Self {
            adapter: SuperTrendBacktestAdapter::new(tuning),
        }
    }
}

impl IndicatorStrategyBacktest for SuperTrendBacktestAdapterWrapper {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.adapter.tuning.atr_period.max(20)
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        _candle: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _values: &mut Self::IndicatorValues,
        _risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let idx = candles.len() - 1;
        self.adapter
            .get_signal(candles, idx)
            .unwrap_or_else(|| SignalResult::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supertrend_long_on_flip_to_up() {
        let thresholds = SuperTrendThresholds::default();
        let snapshot = SuperTrendSignalSnapshot {
            price: 100.0,
            atr: 1.0,
            supertrend_line: 97.0,
            current_direction: SuperTrendDirection::Up,
            prev_direction: SuperTrendDirection::Down,
            basic_band: 99.0,
            upper_band: 102.0,
            lower_band: 96.0,
        };
        let decision = SuperTrendStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, SuperTrendAction::Long));
    }

    #[test]
    fn supertrend_short_on_flip_to_down() {
        let thresholds = SuperTrendThresholds::default();
        let snapshot = SuperTrendSignalSnapshot {
            price: 100.0,
            atr: 1.0,
            supertrend_line: 103.0,
            current_direction: SuperTrendDirection::Down,
            prev_direction: SuperTrendDirection::Up,
            basic_band: 101.0,
            upper_band: 104.0,
            lower_band: 98.0,
        };
        let decision = SuperTrendStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, SuperTrendAction::Short));
    }

    #[test]
    fn no_signal_when_direction_unchanged() {
        let thresholds = SuperTrendThresholds::default();
        let snapshot = SuperTrendSignalSnapshot {
            price: 100.0,
            atr: 1.0,
            supertrend_line: 97.0,
            current_direction: SuperTrendDirection::Up,
            prev_direction: SuperTrendDirection::Up,
            basic_band: 99.0,
            upper_band: 102.0,
            lower_band: 96.0,
        };
        let decision = SuperTrendStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, SuperTrendAction::Hold));
    }
}
