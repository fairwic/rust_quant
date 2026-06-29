use super::strategy::BtcEthLiquidityScalperStrategy;
use super::types::BtcEthLiquidityScalperConfig;
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
use serde_json::Value;

pub struct BtcEthLiquidityScalperStrategyExecutor;

impl BtcEthLiquidityScalperStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BtcEthLiquidityScalperStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StrategyExecutor for BtcEthLiquidityScalperStrategyExecutor {
    fn name(&self) -> &'static str {
        "BtcEthLiquidityScalper"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::BtcEthLiquidityScalper
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(strategy_config) else {
            return false;
        };
        // 只识别 v1 key，避免手写无版本别名绕过策略版本审计。
        strategy_key(&value).is_some_and(|key| matches!(key, "btc_eth_liquidity_scalper_v1"))
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
        let config: BtcEthLiquidityScalperConfig =
            serde_json::from_value(strategy_config.parameters.clone())?;
        let price = snap.as_ref().map(|candle| candle.c).unwrap_or_default();
        let ts = snap.as_ref().map(|candle| candle.ts).unwrap_or_default();
        let Some(mut snapshot) = config.snapshot.clone() else {
            return Ok(BtcEthLiquidityScalperStrategy::flat_missing_snapshot(
                price, ts,
            ));
        };
        if snapshot.price <= 0.0 {
            snapshot.price = price;
        }
        let decision = BtcEthLiquidityScalperStrategy::evaluate(&config, &snapshot);
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
