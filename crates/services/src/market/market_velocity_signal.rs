use anyhow::{anyhow, Result};
use chrono::SecondsFormat;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use serde_json::json;
use std::time::Duration;
use tracing::info;

use crate::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalDispatchResponse,
    StrategySignalSubmitRequest,
};

#[derive(Clone, Debug, PartialEq)]
pub struct MarketVelocityStrategySignalConfig {
    pub strategy_slug: String,
    pub min_delta_rank: i32,
    pub max_new_rank: i32,
    pub stop_loss_pct: f64,
}

impl Default for MarketVelocityStrategySignalConfig {
    fn default() -> Self {
        Self {
            strategy_slug: "market_velocity".to_string(),
            min_delta_rank: 3,
            max_new_rank: 50,
            stop_loss_pct: 0.02,
        }
    }
}

impl MarketVelocityStrategySignalConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            strategy_slug: std::env::var("MARKET_VELOCITY_STRATEGY_SLUG")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "market_velocity".to_string()),
            min_delta_rank: parse_env_i32("MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK", 3)?,
            max_new_rank: parse_env_i32("MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK", 50)?,
            stop_loss_pct: parse_env_f64("MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT", 0.02)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketVelocityStrategySignalBlocker {
    UnsupportedEventType,
    RankDeltaTooWeak,
    RankOutsideTradeWindow,
    PriceDirectionNotUp,
    MissingCurrentPrice,
    InvalidStopLossConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarketVelocityStrategySignalDecision {
    Submit(StrategySignalSubmitRequest),
    Blocked(MarketVelocityStrategySignalBlocker),
}

pub async fn dispatch_market_velocity_strategy_signal_if_enabled(
    event: &MarketRankEvent,
) -> Result<Option<StrategySignalDispatchResponse>> {
    if !should_dispatch_market_velocity_signal_to_quant_web() {
        return Ok(None);
    }

    let config = MarketVelocityStrategySignalConfig::from_env()?;
    let decision = build_market_velocity_strategy_signal_request(event, &config)?;
    let request = match decision {
        MarketVelocityStrategySignalDecision::Submit(request) => request,
        MarketVelocityStrategySignalDecision::Blocked(blocker) => {
            info!(
                "Market Velocity event not promoted to quant_web strategy signal: symbol={}, event_id={:?}, blocker={:?}",
                event.symbol, event.id, blocker
            );
            return Ok(None);
        }
    };

    let external_id = request.external_id.clone();
    let client = ExecutionTaskClient::new(market_velocity_execution_task_config_from_env()?)?;
    let timeout_secs = parse_env_u64("MARKET_VELOCITY_SIGNAL_DISPATCH_TIMEOUT_SECS", 5)?;
    let response = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client.submit_strategy_signal(request),
    )
    .await
    .map_err(|_| anyhow!("submit market velocity strategy signal timeout"))??;
    info!(
        "Submitted Market Velocity strategy signal to rust_quan_web: external_id={}, generated_tasks={}",
        external_id,
        response.generated_tasks.len()
    );
    Ok(Some(response))
}

fn should_dispatch_market_velocity_signal_to_quant_web() -> bool {
    should_dispatch_market_velocity_signal_to_quant_web_from_env(
        std::env::var("MARKET_VELOCITY_SIGNAL_DISPATCH_MODE")
            .ok()
            .as_deref(),
        std::env::var("STRATEGY_SIGNAL_DISPATCH_MODE")
            .ok()
            .as_deref(),
        std::env::var("RUST_QUAN_WEB_BASE_URL").ok().as_deref(),
        std::env::var("QUANT_WEB_BASE_URL").ok().as_deref(),
    )
}

fn should_dispatch_market_velocity_signal_to_quant_web_from_env(
    market_velocity_mode: Option<&str>,
    strategy_signal_mode: Option<&str>,
    rust_quan_web_base_url: Option<&str>,
    quant_web_base_url: Option<&str>,
) -> bool {
    let mode = market_velocity_mode
        .or(strategy_signal_mode)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if matches!(
        mode.as_str(),
        "disabled" | "disable" | "false" | "0" | "legacy" | "legacy_local" | "local" | "direct"
    ) {
        return false;
    }
    if matches!(
        mode.as_str(),
        "web" | "quant_web" | "execution_tasks" | "enabled" | "true" | "1"
    ) {
        return true;
    }

    rust_quan_web_base_url
        .or(quant_web_base_url)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
}

fn market_velocity_execution_task_config_from_env() -> Result<ExecutionTaskConfig> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .map_err(|_| anyhow!("未配置 RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL"))?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_default();
    Ok(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })
}

pub fn build_market_velocity_strategy_signal_request(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
) -> Result<MarketVelocityStrategySignalDecision> {
    if !matches!(
        event.event_type,
        MarketRankEventType::RankVelocity | MarketRankEventType::TopEntry
    ) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::UnsupportedEventType,
        ));
    }

    if !matches!(event.delta_rank, Some(delta) if delta >= config.min_delta_rank) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::RankDeltaTooWeak,
        ));
    }

    if !matches!(event.new_rank, Some(rank) if rank > 0 && rank <= config.max_new_rank) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::RankOutsideTradeWindow,
        ));
    }

    if event.price_direction.trim().to_ascii_lowercase() != "up" {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::PriceDirectionNotUp,
        ));
    }

    let Some(entry_price) = decimal_to_positive_f64(event.current_price) else {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::MissingCurrentPrice,
        ));
    };
    if !(0.0..1.0).contains(&config.stop_loss_pct) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }

    let selected_stop_loss_price = round_price(entry_price * (1.0 - config.stop_loss_pct));
    if selected_stop_loss_price <= 0.0 || selected_stop_loss_price >= entry_price {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }

    let exchange = event.exchange.trim().to_ascii_lowercase();
    if exchange.is_empty() {
        return Err(anyhow!("market velocity event exchange is empty"));
    }
    let symbol = event.symbol.trim().to_ascii_uppercase();
    if symbol.is_empty() {
        return Err(anyhow!("market velocity event symbol is empty"));
    }
    let strategy_slug = config.strategy_slug.trim();
    if strategy_slug.is_empty() {
        return Err(anyhow!("market velocity strategy_slug is empty"));
    }

    let rank_event_id = event.id;
    let external_id = rank_event_id
        .map(|id| format!("rust_quant:market_velocity:{id}"))
        .unwrap_or_else(|| {
            format!(
                "rust_quant:market_velocity:{}:{}:{}",
                exchange,
                symbol,
                event.detected_at.timestamp_millis()
            )
        });
    let strategy_key = format!("{strategy_slug}:{exchange}:{symbol}");
    let confidence = market_velocity_confidence(event);
    let generated_at = Some(event.detected_at.to_rfc3339_opts(SecondsFormat::Secs, true));
    let event_type = event.event_type.as_str();
    let payload_json = json!({
        "source": "rust_quant",
        "source_signal_type": "market_velocity",
        "rank_event_id": rank_event_id,
        "event_type": event_type,
        "strategy_slug": strategy_slug,
        "strategy_key": &strategy_key,
        "exchange": &exchange,
        "symbol": &symbol,
        "timeframe": event.timeframe.as_deref(),
        "old_rank": event.old_rank,
        "new_rank": event.new_rank,
        "delta_rank": event.delta_rank,
        "volume_24h_quote": event.volume_24h_quote.and_then(decimal_to_f64),
        "current_price": entry_price,
        "previous_price": event.previous_price.and_then(decimal_to_f64),
        "price_change_pct": event.price_change_pct.and_then(decimal_to_f64),
        "price_direction": &event.price_direction,
        "technical_snapshot_status": &event.technical_snapshot_status,
        "technical_snapshot": &event.technical_snapshot,
        "side": "buy",
        "position_side": "long",
        "trade_side": "open",
        "order_type": "market",
        "risk_plan": {
            "entry_price": entry_price,
            "selected_stop_loss_price": selected_stop_loss_price,
            "direction": "long",
            "protective_stop_loss_required": true,
            "stop_loss_source": "market_velocity_default_stop_loss_pct",
            "stop_loss_percent": config.stop_loss_pct,
        },
        "detected_at": generated_at.as_deref(),
    })
    .to_string();

    Ok(MarketVelocityStrategySignalDecision::Submit(
        StrategySignalSubmitRequest {
            source: "rust_quant".to_string(),
            external_id,
            strategy_slug: strategy_slug.to_string(),
            strategy_key,
            symbol: symbol.clone(),
            signal_type: "entry".to_string(),
            direction: "long".to_string(),
            title: format!("Market Velocity long signal {symbol}"),
            summary: Some(format!(
                "{} ranking improved from {:?} to {:?}, delta {:?}, price direction {}",
                symbol, event.old_rank, event.new_rank, event.delta_rank, event.price_direction
            )),
            confidence: Some(confidence),
            payload_json,
            generated_at,
        },
    ))
}

fn decimal_to_f64(value: Decimal) -> Option<f64> {
    value.to_f64().filter(|number| number.is_finite())
}

fn decimal_to_positive_f64(value: Option<Decimal>) -> Option<f64> {
    value
        .and_then(decimal_to_f64)
        .filter(|number| *number > 0.0)
}

fn round_price(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn market_velocity_confidence(event: &MarketRankEvent) -> f64 {
    let delta_component = event.delta_rank.unwrap_or_default().max(0).min(20) as f64 * 0.01;
    let price_component = event
        .price_change_pct
        .and_then(decimal_to_f64)
        .unwrap_or_default()
        .max(0.0)
        .min(10.0)
        * 0.005;
    let top_rank_component = if matches!(event.new_rank, Some(rank) if rank <= 50) {
        0.05
    } else {
        0.0
    };
    let confidence = 0.55 + delta_component + price_component + top_rank_component;
    ((confidence.min(0.95)) * 100.0).round() / 100.0
}

fn parse_env_i32(key: &str, default: i32) -> Result<i32> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<i32>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_env_u64(key: &str, default: u64) -> Result<u64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_env_f64(key: &str, default: f64) -> Result<f64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .map_err(|error| anyhow!("{key} must be a number: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use rust_decimal::Decimal;
    use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
    use serde_json::Value;

    use super::{
        build_market_velocity_strategy_signal_request,
        dispatch_market_velocity_strategy_signal_if_enabled,
        should_dispatch_market_velocity_signal_to_quant_web_from_env,
        MarketVelocityStrategySignalBlocker, MarketVelocityStrategySignalConfig,
        MarketVelocityStrategySignalDecision,
    };

    fn rank_event(
        event_type: MarketRankEventType,
        price_direction: &str,
        current_price: Option<Decimal>,
    ) -> MarketRankEvent {
        MarketRankEvent {
            id: Some(991),
            exchange: "okx".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            event_type,
            timeframe: Some("15分钟".to_string()),
            old_rank: Some(44),
            new_rank: Some(31),
            delta_rank: Some(13),
            volume_24h_quote: Some(Decimal::new(120_000_000, 0)),
            current_price,
            previous_price: Some(Decimal::new(3200, 0)),
            price_change_pct: Some(Decimal::new(625, 2)),
            price_direction: price_direction.to_string(),
            technical_snapshot_status: "captured".to_string(),
            technical_snapshot: None,
            detected_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
            source: "scanner_service".to_string(),
            notification_state: "pending".to_string(),
        }
    }

    #[test]
    fn rank_velocity_up_event_builds_quant_web_strategy_signal() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        let decision = build_market_velocity_strategy_signal_request(&event, &config)
            .expect("valid market velocity event should be evaluated");

        let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
            panic!("strong rank velocity event should submit a strategy signal");
        };
        assert_eq!(request.source, "rust_quant");
        assert_eq!(request.external_id, "rust_quant:market_velocity:991");
        assert_eq!(request.strategy_slug, "market_velocity");
        assert_eq!(request.strategy_key, "market_velocity:okx:ETH-USDT-SWAP");
        assert_eq!(request.symbol, "ETH-USDT-SWAP");
        assert_eq!(request.signal_type, "entry");
        assert_eq!(request.direction, "long");
        assert_eq!(request.confidence, Some(0.76));

        let payload: Value =
            serde_json::from_str(&request.payload_json).expect("payload should be valid json");
        assert_eq!(payload["source_signal_type"], "market_velocity");
        assert_eq!(payload["rank_event_id"], 991);
        assert_eq!(payload["event_type"], "rank_velocity");
        assert_eq!(payload["side"], "buy");
        assert_eq!(payload["position_side"], "long");
        assert_eq!(payload["order_type"], "market");
        assert_eq!(payload["risk_plan"]["entry_price"], 3400.0);
        assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3332.0);
        assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    }

    #[test]
    fn top_exit_or_down_price_event_does_not_build_strategy_signal() {
        let config = MarketVelocityStrategySignalConfig::default();
        let top_exit = rank_event(
            MarketRankEventType::TopExit,
            "down",
            Some(Decimal::new(3000, 0)),
        );
        let down = rank_event(
            MarketRankEventType::RankVelocity,
            "down",
            Some(Decimal::new(3000, 0)),
        );

        assert_eq!(
            build_market_velocity_strategy_signal_request(&top_exit, &config)
                .expect("top exit should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::UnsupportedEventType
            )
        );
        assert_eq!(
            build_market_velocity_strategy_signal_request(&down, &config)
                .expect("down price should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::PriceDirectionNotUp
            )
        );
    }

    #[test]
    fn dispatch_gate_matches_strategy_signal_web_mode_contract() {
        assert!(
            should_dispatch_market_velocity_signal_to_quant_web_from_env(
                Some("web"),
                None,
                None,
                None,
            )
        );
        assert!(
            should_dispatch_market_velocity_signal_to_quant_web_from_env(
                None,
                None,
                Some("http://127.0.0.1:5557"),
                None,
            )
        );
        assert!(
            !should_dispatch_market_velocity_signal_to_quant_web_from_env(
                Some("disabled"),
                None,
                Some("http://127.0.0.1:5557"),
                None,
            )
        );
        assert!(
            should_dispatch_market_velocity_signal_to_quant_web_from_env(
                None,
                Some("execution_tasks"),
                None,
                None,
            )
        );
    }

    #[tokio::test]
    #[ignore = "requires seeded rust_quan_web Market Velocity runtime fixture and a running Web backend"]
    async fn market_velocity_synthetic_event_dispatches_to_running_quant_web() {
        std::env::set_var("MARKET_VELOCITY_SIGNAL_DISPATCH_MODE", "web");
        std::env::set_var("MARKET_VELOCITY_STRATEGY_SLUG", "market_velocity");
        std::env::set_var(
            "RUST_QUAN_WEB_BASE_URL",
            std::env::var("RUST_QUAN_WEB_BASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "http://127.0.0.1:8001".to_string()),
        );
        std::env::set_var(
            "EXECUTION_EVENT_SECRET",
            std::env::var("EXECUTION_EVENT_SECRET")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "local-dev-secret".to_string()),
        );

        let event_id = Utc::now().timestamp_micros();
        let mut event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );
        event.id = Some(event_id);
        event.exchange = "binance".to_string();
        event.symbol = "ETHUSDT".to_string();
        event.detected_at = Utc::now();

        let response = dispatch_market_velocity_strategy_signal_if_enabled(&event)
            .await
            .expect("synthetic Market Velocity event should dispatch to running quant_web")
            .expect("dispatch should be enabled by test env");

        assert_eq!(
            response.inbox.external_id,
            format!("rust_quant:market_velocity:{event_id}")
        );
        assert_eq!(response.inbox.strategy_slug, "market_velocity");
        assert_eq!(response.inbox.symbol, "ETHUSDT");
        assert_eq!(response.generated_tasks.len(), 1);
        let task = response
            .generated_tasks
            .first()
            .expect("running Web backend should generate one task for runtime fixture");
        assert_eq!(task.symbol, "ETHUSDT");
        assert_eq!(task.task_type, "execute_signal");
        assert_eq!(task.task_status, "pending");
    }
}
