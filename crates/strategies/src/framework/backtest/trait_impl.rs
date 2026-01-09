use super::adapter::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use super::types::{BackTestResult, BasicRiskStrategyConfig};
use crate::implementations::nwe_strategy::NweStrategy;
use crate::implementations::vegas_backtest::VegasBacktestAdapter;
use crate::CandleItem;

/// 通用回测策略能力接口，便于不同策略复用统一回测与落库流程
pub trait BackTestAbleStrategyTrait: IndicatorStrategyBacktest + Sized {
    fn strategy_type(&self) -> crate::StrategyType;
    fn config_json(&self) -> Option<String>;
    fn run_test(
        &mut self,
        inst_id: &str,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(inst_id, self, candles, risk_strategy_config)
    }
}

impl BackTestAbleStrategyTrait for NweStrategy {
    fn strategy_type(&self) -> crate::StrategyType {
        crate::StrategyType::Nwe
    }

    fn config_json(&self) -> Option<String> {
        serde_json::to_string(&self.config).ok()
    }
}

impl BackTestAbleStrategyTrait for VegasBacktestAdapter {
    fn strategy_type(&self) -> crate::StrategyType {
        crate::StrategyType::Vegas
    }

    fn config_json(&self) -> Option<String> {
        serde_json::to_string(self.strategy()).ok()
    }
}
