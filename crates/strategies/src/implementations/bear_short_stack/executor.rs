use super::strategy::BearShortStackStrategy;
use super::types::{BearShortPreset, BearShortStackConfig};
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
use serde_json::Value;

pub struct BearShortStackStrategyExecutor;

impl BearShortStackStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BearShortStackStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StrategyExecutor for BearShortStackStrategyExecutor {
    fn name(&self) -> &'static str {
        "BearShortStack"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::BearShortStack
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(strategy_config) else {
            return false;
        };
        strategy_key(&value).is_some_and(is_bear_short_key)
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
        let mut config: BearShortStackConfig =
            serde_json::from_value(strategy_config.parameters.clone())?;
        apply_strategy_key_preset(&mut config);
        let price = snap.as_ref().map(|candle| candle.c).unwrap_or_default();
        let ts = snap.as_ref().map(|candle| candle.ts).unwrap_or_default();
        let Some(mut snapshot) = config.snapshot.clone() else {
            return Ok(BearShortStackStrategy::flat_missing_snapshot(price, ts));
        };
        if snapshot.price <= 0.0 {
            snapshot.price = price;
        }
        let decision = BearShortStackStrategy::evaluate(&config, &snapshot);
        Ok(decision.to_signal(snapshot.price, ts))
    }
}

fn apply_strategy_key_preset(config: &mut BearShortStackConfig) {
    // 子策略 key 是产品侧入口，执行前落到具体 preset，保证信号 payload 能保留真实做空语义。
    if config.strategy_key.as_deref() == Some("exhaustion_fade_short_v1") {
        config.preset = BearShortPreset::ExhaustionFade;
    }
}

fn strategy_key(value: &Value) -> Option<&str> {
    value
        .get("strategy_key")
        .or_else(|| value.get("strategy_type"))
        .or_else(|| value.get("strategy_name"))
        .and_then(Value::as_str)
}

fn is_bear_short_key(key: &str) -> bool {
    // 允许父策略 key 和两个子预设 key，但不接受无版本别名。
    matches!(
        key,
        "bear_short_stack_v1" | "bear_breakdown_short_v1" | "exhaustion_fade_short_v1"
    )
}
