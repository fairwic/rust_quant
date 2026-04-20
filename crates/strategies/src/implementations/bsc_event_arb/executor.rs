use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
use serde_json::Value;

use super::strategy::BscEventArbStrategy;
use super::types::BscEventArbStrategyConfig;
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::SignalResult;
use crate::StrategyType;

pub struct BscEventArbStrategyExecutor;

impl BscEventArbStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BscEventArbStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StrategyExecutor for BscEventArbStrategyExecutor {
    fn name(&self) -> &'static str {
        "BscEventArb"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::BscEventArb
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(strategy_config) else {
            return false;
        };
        value.get("strategy_name").and_then(Value::as_str) == Some("bsc_event_arb")
            || value.get("strategy_type").and_then(Value::as_str) == Some("bsc_event_arb")
            || value.get("bsc_event_arb").and_then(Value::as_bool) == Some(true)
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
        let config: BscEventArbStrategyConfig =
            serde_json::from_value(strategy_config.parameters.clone())?;
        let price = snap.as_ref().map(|candle| candle.c).unwrap_or_default();
        let ts = snap.as_ref().map(|candle| candle.ts).unwrap_or_default();

        let Some(mut snapshot) = config.snapshot.clone() else {
            return Ok(BscEventArbStrategy::flat_missing_snapshot(price, ts));
        };
        if snapshot.price_usd <= 0.0 {
            snapshot.price_usd = price;
        }

        let decision = BscEventArbStrategy::evaluate(&config, &snapshot);
        Ok(decision.to_signal(snapshot.price_usd, ts))
    }
}
