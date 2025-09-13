use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub strategy_config_id: i64,
    pub strategy_config: String,  // Vegas策略配置JSON
    pub risk_config: String,      // 风险配置JSON
}

impl StrategyConfig {
    /// 创建新的策略配置
    pub fn new(
        strategy_config_id: i64,
        strategy_config: String,
        risk_config: String,
    ) -> Self {
        Self {
            strategy_config_id,
            strategy_config,
            risk_config,
        }
    }
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            strategy_config_id: 1,
            strategy_config: "{}".to_string(),
            risk_config: "{}".to_string(),
        }
    }
}
