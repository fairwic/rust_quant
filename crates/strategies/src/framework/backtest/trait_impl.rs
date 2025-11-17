use crate::implementations::nwe_strategy::NweStrategy;
use crate::CandleItem;
use super::types::{BackTestResult, BasicRiskStrategyConfig};
use super::engine::run_back_test_generic;

/// 通用回测策略能力接口，便于不同策略复用统一回测与落库流程
pub trait BackTestAbleStrategyTrait {
    fn strategy_type(&self) -> crate::StrategyType;
    fn config_json(&self) -> Option<String>;
    fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult;
}

impl BackTestAbleStrategyTrait for NweStrategy {
    fn strategy_type(&self) -> crate::StrategyType {
        crate::StrategyType::Nwe
    }

    fn config_json(&self) -> Option<String> {
        serde_json::to_string(&self.config).ok()
    }

    fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        NweStrategy::run_test(self, candles, risk_strategy_config)
    }
}

