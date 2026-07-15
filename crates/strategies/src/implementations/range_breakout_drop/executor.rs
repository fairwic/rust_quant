use super::strategy::RangeBreakoutDropStrategy;
use super::types::{RangeBreakoutDropSignalSnapshot, RangeBreakoutDropThresholds};
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
use serde_json::Value;

/// 震荡突破下跌策略执行器
pub struct RangeBreakoutDropStrategyExecutor;

impl RangeBreakoutDropStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RangeBreakoutDropStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StrategyExecutor for RangeBreakoutDropStrategyExecutor {
    fn name(&self) -> &'static str {
        "RangeBreakoutDrop"
    }

    fn strategy_type(&self) -> StrategyType {
        // 使用自定义策略类型，分配一个唯一ID
        StrategyType::Custom(1001)
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(strategy_config) else {
            return false;
        };
        strategy_key(&value).is_some_and(|key| key == "range_breakout_drop_v1")
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
        let thresholds: RangeBreakoutDropThresholds =
            serde_json::from_value(strategy_config.parameters.clone())?;

        let price = snap.as_ref().map(|candle| candle.c).unwrap_or_default();
        let ts = snap.as_ref().map(|candle| candle.ts).unwrap_or_default();

        // 从策略配置中获取快照
        let snapshot_value = strategy_config.parameters.get("snapshot");
        let Some(snapshot_value) = snapshot_value else {
            return Ok(RangeBreakoutDropStrategy::flat_missing_snapshot(price, ts));
        };

        let mut snapshot: RangeBreakoutDropSignalSnapshot =
            serde_json::from_value(snapshot_value.clone())?;

        if snapshot.price <= 0.0 {
            snapshot.price = price;
        }

        let decision = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot);
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
