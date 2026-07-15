use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;

use super::executor::BbRsiBacktestAdapter;
use super::types::{
    BbRsiAction, BbRsiBacktestTuning, BbRsiDecision, BbRsiSignalSnapshot, BbRsiThresholds,
};

/// Bollinger Bands + RSI策略核心逻辑
pub struct BbRsiStrategy;

impl BbRsiStrategy {
    /// 评估当前是否应该开仓
    pub fn evaluate(thresholds: &BbRsiThresholds, snapshot: &BbRsiSignalSnapshot) -> BbRsiDecision {
        let mut reasons = Vec::new();

        // 计算价格相对布林带的位置
        let price_below_lower =
            snapshot.price < snapshot.bb_lower * (1.0 + thresholds.bb_breakout_pct / 100.0);
        let price_above_upper =
            snapshot.price > snapshot.bb_upper * (1.0 - thresholds.bb_breakout_pct / 100.0);

        // 检查做多条件：价格触及下轨 + RSI超卖
        if price_below_lower && snapshot.rsi < thresholds.rsi_oversold {
            if !thresholds.allow_long {
                reasons.push("LONG_DISABLED".to_string());
                return BbRsiDecision {
                    action: BbRsiAction::Flat,
                    reasons,
                };
            }

            reasons.push(format!(
                "BB_LOWER_TOUCH: price={:.2} < lower={:.2}",
                snapshot.price, snapshot.bb_lower
            ));
            reasons.push(format!(
                "RSI_OVERSOLD: rsi={:.1} < {:.1}",
                snapshot.rsi, thresholds.rsi_oversold
            ));
            reasons.push(format!(
                "BB_WIDTH: {:.2} (volatility indicator)",
                snapshot.bb_width
            ));

            return BbRsiDecision {
                action: BbRsiAction::Long,
                reasons,
            };
        }

        // 检查做空条件：价格触及上轨 + RSI超买
        if price_above_upper && snapshot.rsi > thresholds.rsi_overbought {
            if !thresholds.allow_short {
                reasons.push("SHORT_DISABLED".to_string());
                return BbRsiDecision {
                    action: BbRsiAction::Flat,
                    reasons,
                };
            }

            reasons.push(format!(
                "BB_UPPER_TOUCH: price={:.2} > upper={:.2}",
                snapshot.price, snapshot.bb_upper
            ));
            reasons.push(format!(
                "RSI_OVERBOUGHT: rsi={:.1} > {:.1}",
                snapshot.rsi, thresholds.rsi_overbought
            ));
            reasons.push(format!(
                "BB_WIDTH: {:.2} (volatility indicator)",
                snapshot.bb_width
            ));

            return BbRsiDecision {
                action: BbRsiAction::Short,
                reasons,
            };
        }

        // 无信号
        if price_below_lower {
            reasons.push(format!(
                "PRICE_AT_LOWER but RSI={:.1} not oversold (need <{:.1})",
                snapshot.rsi, thresholds.rsi_oversold
            ));
        } else if price_above_upper {
            reasons.push(format!(
                "PRICE_AT_UPPER but RSI={:.1} not overbought (need >{:.1})",
                snapshot.rsi, thresholds.rsi_overbought
            ));
        } else {
            reasons.push(format!(
                "PRICE_IN_RANGE: lower={:.2} < price={:.2} < upper={:.2}, RSI={:.1}",
                snapshot.bb_lower, snapshot.price, snapshot.bb_upper, snapshot.rsi
            ));
        }

        BbRsiDecision {
            action: BbRsiAction::Flat,
            reasons,
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
            BbRsiBacktestTuning::default(),
        )
    }

    /// 运行回测（带参数调优）
    pub fn run_test_with_tuning(
        inst_id: &str,
        candles: &[CandleItem],
        risk_config: BasicRiskStrategyConfig,
        tuning: BbRsiBacktestTuning,
    ) -> BackTestResult {
        let adapter = BbRsiBacktestAdapter::new(tuning);
        run_indicator_strategy_backtest(inst_id, adapter, candles, risk_config)
    }
}

// 直接在BbRsiBacktestAdapter上实现trait
impl IndicatorStrategyBacktest for BbRsiBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.tuning
            .bb_period
            .max(self.tuning.rsi_period + 1)
            .max(20)
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
        self.get_signal(candles, idx)
            .unwrap_or_else(|| SignalResult::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_signal_on_lower_band_and_oversold_rsi() {
        let thresholds = BbRsiThresholds::default();
        let snapshot = BbRsiSignalSnapshot {
            price: 97.9,
            rsi: 25.0, // 超卖
            atr: 1.0,
            bb_upper: 102.0,
            bb_middle: 100.0,
            bb_lower: 98.0, // 价格严格跌破下轨
            bb_width: 4.0,
            price_bb_position: 0.0,
        };
        let decision = BbRsiStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, BbRsiAction::Long));
    }

    #[test]
    fn short_signal_on_upper_band_and_overbought_rsi() {
        let thresholds = BbRsiThresholds::default();
        let snapshot = BbRsiSignalSnapshot {
            price: 102.1, // 严格突破上轨
            rsi: 75.0,    // 超买
            atr: 1.0,
            bb_upper: 102.0,
            bb_middle: 100.0,
            bb_lower: 98.0,
            bb_width: 4.0,
            price_bb_position: 1.0,
        };
        let decision = BbRsiStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, BbRsiAction::Short));
    }

    #[test]
    fn no_signal_when_price_at_band_but_rsi_neutral() {
        let thresholds = BbRsiThresholds::default();
        let snapshot = BbRsiSignalSnapshot {
            price: 98.0, // 触及下轨
            rsi: 50.0,   // RSI中性，不超卖
            atr: 1.0,
            bb_upper: 102.0,
            bb_middle: 100.0,
            bb_lower: 98.0,
            bb_width: 4.0,
            price_bb_position: 0.0,
        };
        let decision = BbRsiStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, BbRsiAction::Flat));
    }
}
