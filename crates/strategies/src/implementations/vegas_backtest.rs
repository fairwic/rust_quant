use rust_quant_indicators::trend::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::{
    IndicatorCombine, VegasIndicatorSignalValue, VegasStrategy,
};

use crate::framework::backtest::adapter::IndicatorStrategyBacktest;
use crate::framework::backtest::conversions::{convert_domain_signal, to_domain_basic_risk_config};
use crate::framework::backtest::trait_impl::BackTestAbleStrategyTrait;
use crate::framework::backtest::types::{BasicRiskStrategyConfig, SignalResult};
use crate::strategy_common::get_multi_indicator_values;
use crate::CandleItem;
use crate::StrategyType;
use std::sync::Arc;

/// Vegas 策略回测适配器
///
/// 将 indicators 包中的 `VegasStrategy` 接入通用回测框架，
/// 让 orchestration 在新增策略时无需编写重复逻辑。
#[derive(Debug, Clone)]
pub struct VegasBacktestAdapter {
    strategy: VegasStrategy,
    signal_weights: SignalWeightsConfig,
}

impl VegasBacktestAdapter {
    pub fn new(strategy: VegasStrategy) -> Self {
        let signal_weights = strategy.signal_weights.clone().unwrap_or_default();
        Self {
            strategy,
            signal_weights,
        }
    }

    pub fn strategy(&self) -> &VegasStrategy {
        &self.strategy
    }

    pub fn strategy_mut(&mut self) -> &mut VegasStrategy {
        &mut self.strategy
    }
}

impl IndicatorStrategyBacktest for VegasBacktestAdapter {
    type IndicatorCombine = IndicatorCombine;
    type IndicatorValues = VegasIndicatorSignalValue;

    fn min_data_length(&self) -> usize {
        self.strategy.min_k_line_num.max(1)
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {
        self.strategy.get_indicator_combine()
    }

    fn build_indicator_values(
        indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues {
        get_multi_indicator_values(indicator_combine, candle)
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let domain_risk = to_domain_basic_risk_config(risk_config);
        let domain_signal =
            self.strategy
                .get_trade_signal(candles, values, &self.signal_weights, &domain_risk);
        convert_domain_signal(domain_signal)
    }
}
