use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;

use super::executor::RsiDivergenceBacktestAdapter;
use super::types::{
    DivergenceAction, DivergenceType, RsiDivergenceBacktestTuning, RsiDivergenceDecision,
    RsiDivergenceSignalSnapshot, RsiDivergenceThresholds,
};

/// RSI Divergence策略核心逻辑
pub struct RsiDivergenceStrategy;

impl RsiDivergenceStrategy {
    /// 评估当前是否存在背离信号
    pub fn evaluate(
        thresholds: &RsiDivergenceThresholds,
        snapshot: &RsiDivergenceSignalSnapshot,
    ) -> RsiDivergenceDecision {
        let mut reasons = Vec::new();

        match snapshot.divergence_type {
            DivergenceType::BullishRegular => {
                if !thresholds.allow_long {
                    reasons.push("LONG_DISABLED".to_string());
                    return RsiDivergenceDecision {
                        action: DivergenceAction::Flat,
                        reasons,
                    };
                }

                // 验证RSI在超卖区域
                if snapshot.rsi > thresholds.rsi_oversold {
                    reasons.push(format!(
                        "RSI_NOT_OVERSOLD: rsi={:.1} > threshold={:.1}",
                        snapshot.rsi, thresholds.rsi_oversold
                    ));
                    return RsiDivergenceDecision {
                        action: DivergenceAction::Flat,
                        reasons,
                    };
                }

                reasons.push(format!(
                    "BULLISH_DIV: price_low {:.2} → {:.2} (新低)",
                    snapshot.prev_price_low, snapshot.current_price_low
                ));
                reasons.push(format!(
                    "RSI_higher: {:.1} → {:.1} (未新低)",
                    snapshot.prev_rsi_low, snapshot.current_rsi_low
                ));
                reasons.push(format!(
                    "RSI={:.1} < {:.1} (超卖)",
                    snapshot.rsi, thresholds.rsi_oversold
                ));

                RsiDivergenceDecision {
                    action: DivergenceAction::Long,
                    reasons,
                }
            }

            DivergenceType::BearishRegular => {
                if !thresholds.allow_short {
                    reasons.push("SHORT_DISABLED".to_string());
                    return RsiDivergenceDecision {
                        action: DivergenceAction::Flat,
                        reasons,
                    };
                }

                // 验证RSI在超买区域
                if snapshot.rsi < thresholds.rsi_overbought {
                    reasons.push(format!(
                        "RSI_NOT_OVERBOUGHT: rsi={:.1} < threshold={:.1}",
                        snapshot.rsi, thresholds.rsi_overbought
                    ));
                    return RsiDivergenceDecision {
                        action: DivergenceAction::Flat,
                        reasons,
                    };
                }

                reasons.push(format!(
                    "BEARISH_DIV: price_high {:.2} → {:.2} (新高)",
                    snapshot.prev_price_high, snapshot.current_price_high
                ));
                reasons.push(format!(
                    "RSI_lower: {:.1} → {:.1} (未新高)",
                    snapshot.prev_rsi_high, snapshot.current_rsi_high
                ));
                reasons.push(format!(
                    "RSI={:.1} > {:.1} (超买)",
                    snapshot.rsi, thresholds.rsi_overbought
                ));

                RsiDivergenceDecision {
                    action: DivergenceAction::Short,
                    reasons,
                }
            }

            DivergenceType::BullishHidden | DivergenceType::BearishHidden => {
                if !thresholds.enable_hidden_divergence {
                    reasons.push("HIDDEN_DIVERGENCE_DISABLED".to_string());
                    return RsiDivergenceDecision {
                        action: DivergenceAction::Flat,
                        reasons,
                    };
                }
                // 隐藏背离逻辑（趋势延续信号，暂不实现）
                reasons.push("HIDDEN_DIV: not_implemented".to_string());
                RsiDivergenceDecision {
                    action: DivergenceAction::Flat,
                    reasons,
                }
            }

            DivergenceType::None => {
                reasons.push("NO_DIVERGENCE".to_string());
                RsiDivergenceDecision {
                    action: DivergenceAction::Flat,
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
            RsiDivergenceBacktestTuning::default(),
        )
    }

    /// 运行回测（带参数调优）
    pub fn run_test_with_tuning(
        inst_id: &str,
        candles: &[CandleItem],
        risk_config: BasicRiskStrategyConfig,
        tuning: RsiDivergenceBacktestTuning,
    ) -> BackTestResult {
        let adapter = RsiDivergenceBacktestAdapterWrapper::new(tuning);
        run_indicator_strategy_backtest(inst_id, adapter, candles, risk_config)
    }

    /// 运行启用价格/RSI枢轴时间配对的回测。
    pub fn run_test_with_tuning_and_pivot_pair_lag(
        inst_id: &str,
        candles: &[CandleItem],
        risk_config: BasicRiskStrategyConfig,
        tuning: RsiDivergenceBacktestTuning,
        pivot_pair_max_lag: usize,
    ) -> BackTestResult {
        let adapter = RsiDivergenceBacktestAdapterWrapper::new_with_pivot_pair_lag(
            tuning,
            pivot_pair_max_lag,
        );
        run_indicator_strategy_backtest(inst_id, adapter, candles, risk_config)
    }
}

/// 包装器：实现 IndicatorStrategyBacktest trait
struct RsiDivergenceBacktestAdapterWrapper {
    adapter: RsiDivergenceBacktestAdapter,
}

impl RsiDivergenceBacktestAdapterWrapper {
    fn new(tuning: RsiDivergenceBacktestTuning) -> Self {
        Self {
            adapter: RsiDivergenceBacktestAdapter::new(tuning),
        }
    }

    fn new_with_pivot_pair_lag(
        tuning: RsiDivergenceBacktestTuning,
        pivot_pair_max_lag: usize,
    ) -> Self {
        Self {
            adapter: RsiDivergenceBacktestAdapter::new_with_pivot_pair_lag(
                tuning,
                pivot_pair_max_lag,
            ),
        }
    }
}

impl IndicatorStrategyBacktest for RsiDivergenceBacktestAdapterWrapper {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.adapter.tuning.rsi_period + self.adapter.tuning.lookback_period + 10
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
    fn bullish_divergence_triggers_long() {
        let thresholds = RsiDivergenceThresholds::default();
        let snapshot = RsiDivergenceSignalSnapshot {
            price: 100.0,
            rsi: 25.0, // 超卖
            atr: 1.0,
            divergence_type: DivergenceType::BullishRegular,
            price_low_idx: 10,
            price_high_idx: 0,
            rsi_low_idx: 8,
            rsi_high_idx: 0,
            current_price_low: 98.0, // 价格新低
            current_price_high: 0.0,
            prev_price_low: 100.0,
            prev_price_high: 0.0,
            current_rsi_low: 28.0, // RSI未新低
            current_rsi_high: 0.0,
            prev_rsi_low: 25.0,
            prev_rsi_high: 0.0,
        };
        let decision = RsiDivergenceStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, DivergenceAction::Long));
    }

    #[test]
    fn bearish_divergence_triggers_short() {
        let thresholds = RsiDivergenceThresholds::default();
        let snapshot = RsiDivergenceSignalSnapshot {
            price: 100.0,
            rsi: 75.0, // 超买
            atr: 1.0,
            divergence_type: DivergenceType::BearishRegular,
            price_low_idx: 0,
            price_high_idx: 10,
            rsi_low_idx: 0,
            rsi_high_idx: 8,
            current_price_low: 0.0,
            current_price_high: 102.0, // 价格新高
            prev_price_low: 0.0,
            prev_price_high: 100.0,
            current_rsi_low: 0.0,
            current_rsi_high: 68.0, // RSI未新高
            prev_rsi_low: 0.0,
            prev_rsi_high: 72.0,
        };
        let decision = RsiDivergenceStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, DivergenceAction::Short));
    }
}
