use crate::framework::backtest::adapter::IndicatorStrategyBacktest;
use crate::framework::backtest::conversions::{convert_domain_signal, to_domain_basic_risk_config};
use crate::framework::backtest::types::{BasicRiskStrategyConfig, SignalResult};
use crate::strategy_common::get_multi_indicator_values;
use crate::CandleItem;
use rust_quant_indicators::trend::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::{
    IndicatorCombine, VegasIndicatorSignalValue, VegasStrategy,
};
/// Vegas 策略回测适配器
///
/// 将 indicators 包中的 `VegasStrategy` 接入通用回测框架，
/// 让 orchestration 在新增策略时无需编写重复逻辑。
#[derive(Debug, Clone)]
pub struct VegasBacktestAdapter {
    /// 策略标识或策略配置。
    strategy: VegasStrategy,
    /// 信号weights，用于交易策略计算。
    signal_weights: SignalWeightsConfig,
}
impl VegasBacktestAdapter {
    /// 初始化new，确保回测策略依赖和内部状态可直接使用。
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
    /// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
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
