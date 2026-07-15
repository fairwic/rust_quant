use super::strategy::KeltnerChannelScalperStrategy;
use super::types::KeltnerChannelScalperConfig;
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
use serde_json::Value;

/// Keltner Channel 1m scalp research executor；只识别带版本 strategy key。
pub struct KeltnerChannelScalperStrategyExecutor;

impl KeltnerChannelScalperStrategyExecutor {
    /// 创建 research executor；实际信号仍要求配置中带版本化 strategy key。
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeltnerChannelScalperStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StrategyExecutor for KeltnerChannelScalperStrategyExecutor {
    fn name(&self) -> &'static str {
        "KeltnerChannelScalper1m"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::KeltnerChannelScalper1mV1Research
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(strategy_config) else {
            return false;
        };
        strategy_key(&value)
            .is_some_and(|key| matches!(key, "keltner_channel_scalper_1m_v1_research"))
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
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandleItem>,
    ) -> Result<SignalResult> {
        let config: KeltnerChannelScalperConfig =
            serde_json::from_value(strategy_config.parameters.clone())?;
        let price = snap.as_ref().map(|candle| candle.c).unwrap_or_default();
        let ts = snap.as_ref().map(|candle| candle.ts).unwrap_or_default();
        let Some(mut snapshot) = config.snapshot.clone() else {
            return Ok(KeltnerChannelScalperStrategy::flat_missing_snapshot(
                price, ts,
            ));
        };
        if snapshot.symbol.is_empty() {
            snapshot.symbol = inst_id.to_string();
        }
        if snapshot.timeframe.is_empty() {
            snapshot.timeframe = period.to_string();
        }
        if snapshot.price <= 0.0 {
            snapshot.price = price;
        }
        let decision = KeltnerChannelScalperStrategy::evaluate(&config.thresholds, &snapshot);
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
