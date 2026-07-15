use super::strategy::SmartMoneyConceptsStrategy;
use super::types::SmartMoneyConceptsConfig;
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
use serde_json::Value;

/// Smart Money Concepts research executor；只识别带版本 key，避免和未来生产版本混淆。
pub struct SmartMoneyConceptsStrategyExecutor;

impl SmartMoneyConceptsStrategyExecutor {
    /// 创建 research executor；实际信号仍要求配置中带版本化 strategy key。
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmartMoneyConceptsStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StrategyExecutor for SmartMoneyConceptsStrategyExecutor {
    fn name(&self) -> &'static str {
        "SmartMoneyConcepts"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::SmartMoneyConceptsV1Research
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(strategy_config) else {
            return false;
        };
        strategy_key(&value).is_some_and(|key| matches!(key, "smart_money_concepts_v1_research"))
    }

    async fn initialize_data(
        &self,
        _strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandleItem>,
    ) -> Result<StrategyDataResult> {
        Ok(StrategyDataResult {
            hash_key: format!("{}_{}_{}", self.name(), inst_id, period),
            last_timestamp: candles.last().map(|candle| candle.ts).unwrap_or_default(),
        })
    }

    async fn execute(
        &self,
        _inst_id: &str,
        _period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandleItem>,
    ) -> Result<SignalResult> {
        let config: SmartMoneyConceptsConfig =
            serde_json::from_value(strategy_config.parameters.clone())?;
        let price = snap.as_ref().map(|candle| candle.c).unwrap_or_default();
        let ts = snap.as_ref().map(|candle| candle.ts).unwrap_or_default();
        let Some(mut snapshot) = config.snapshot.clone() else {
            return Ok(SmartMoneyConceptsStrategy::flat_missing_snapshot(price, ts));
        };
        if snapshot.price <= 0.0 {
            snapshot.price = price;
        }
        let decision = SmartMoneyConceptsStrategy::evaluate(&config.thresholds, &snapshot);
        Ok(decision.to_signal(snapshot.price, ts))
    }
}

fn strategy_key(value: &Value) -> Option<&str> {
    value
        .get("strategy_key")
        .or_else(|| value.get("strategy_type"))
        .or_else(|| value.get("strategy_name"))
        .and_then(Value::as_str)
}
