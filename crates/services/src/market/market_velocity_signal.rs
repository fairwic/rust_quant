use anyhow::{anyhow, Result};
use chrono::SecondsFormat;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use rust_quant_domain::{BasicRiskConfig, SignalDirection};
use rust_quant_strategies::strategy_common::SignalResult;
use serde_json::json;
use std::time::Duration;
use tracing::info;

use crate::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalDispatchResponse,
    StrategySignalSubmitRequest,
};
use crate::strategy::strategy_signal_payload::{
    build_strategy_signal_submit_request, StrategySignalPayloadBuildOptions,
};

pub use super::market_velocity_entry::MarketVelocityEntryConfirmation;
use super::market_velocity_entry::MarketVelocityEntryConfirmationConfig;

const ENTRY_TRIGGER_FILTER_VERSION: &str = "entry_trigger_allowlist_v1";
const DEFAULT_ENTRY_TRIGGER_ALLOWLIST: &[&str] = &["breakout_previous_high", "reclaim_ema"];
const DEFAULT_SYMBOL_BLOCKLIST: &[&str] = &[];
const DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET: &str = "momentum_03sl_20r_v5";
const DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_momentum_03sl_20r_v5";
const DEFAULT_MARKET_VELOCITY_ENTRY_FILTER_MODE: &str = "rank_radar_4h_trend_15m_momentum";
const DEFAULT_STOP_LOSS_PCT: f64 = 0.03;
const DEFAULT_TAKE_PROFIT_R: f64 = 2.0;
const DEFAULT_MAX_HOLDING_HOURS: u32 = 48;
const DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT: f64 = 4.0;
const DEFAULT_TREND_MIN_AVERAGE_DISTANCE_PCT: f64 = 0.0;

#[derive(Clone, Debug, PartialEq)]
pub struct MarketVelocityStrategySignalConfig {
    pub strategy_slug: String,
    pub strategy_preset: String,
    pub entry_rule_version: String,
    pub min_delta_rank: i32,
    pub max_new_rank: i32,
    pub stop_loss_pct: f64,
    pub take_profit_r: f64,
    pub max_holding_hours: u32,
    pub automation_mode: String,
    pub live_order_allowed: bool,
    pub paper_trade_required: bool,
    pub require_technical_confirmation: bool,
    pub require_entry_confirmation: bool,
    pub chasing_risk_top_rank: i32,
    pub chasing_risk_price_change_pct: f64,
    pub trend_min_average_distance_pct: f64,
    pub entry_confirmation_period: usize,
    pub entry_confirmation_fetch_limit: u32,
    pub entry_max_average_distance_pct: f64,
    pub entry_min_volume_ratio: f64,
    pub entry_trigger_allowlist: Vec<String>,
    pub entry_trigger_blocklist: Vec<String>,
    pub symbol_blocklist: Vec<String>,
}

impl Default for MarketVelocityStrategySignalConfig {
    fn default() -> Self {
        Self {
            strategy_slug: "market_velocity".to_string(),
            strategy_preset: DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET.to_string(),
            entry_rule_version: DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION.to_string(),
            min_delta_rank: 15,
            max_new_rank: 30,
            stop_loss_pct: DEFAULT_STOP_LOSS_PCT,
            take_profit_r: DEFAULT_TAKE_PROFIT_R,
            max_holding_hours: DEFAULT_MAX_HOLDING_HOURS,
            automation_mode: "signal_only".to_string(),
            live_order_allowed: false,
            paper_trade_required: true,
            require_technical_confirmation: true,
            require_entry_confirmation: true,
            chasing_risk_top_rank: 10,
            chasing_risk_price_change_pct: 8.0,
            trend_min_average_distance_pct: DEFAULT_TREND_MIN_AVERAGE_DISTANCE_PCT,
            entry_confirmation_period: 20,
            entry_confirmation_fetch_limit: 80,
            entry_max_average_distance_pct: DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT,
            entry_min_volume_ratio: 1.0,
            entry_trigger_allowlist: default_entry_trigger_allowlist(),
            entry_trigger_blocklist: Vec::new(),
            symbol_blocklist: default_symbol_blocklist(),
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
            strategy_preset: parse_env_string(
                "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET",
                DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET,
            ),
            entry_rule_version: parse_env_string(
                "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION",
                DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION,
            ),
            min_delta_rank: parse_env_i32("MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK", 15)?,
            max_new_rank: parse_env_i32("MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK", 30)?,
            stop_loss_pct: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT",
                DEFAULT_STOP_LOSS_PCT,
            )?,
            take_profit_r: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R",
                DEFAULT_TAKE_PROFIT_R,
            )?,
            max_holding_hours: parse_env_u32(
                "MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS",
                DEFAULT_MAX_HOLDING_HOURS,
            )?,
            automation_mode: std::env::var("MARKET_VELOCITY_SIGNAL_AUTOMATION_MODE")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "signal_only".to_string()),
            live_order_allowed: parse_env_bool("MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED", false)?,
            paper_trade_required: parse_env_bool(
                "MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED",
                true,
            )?,
            require_technical_confirmation: parse_env_bool(
                "MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION",
                true,
            )?,
            require_entry_confirmation: parse_env_bool(
                "MARKET_VELOCITY_SIGNAL_REQUIRE_ENTRY_CONFIRMATION",
                true,
            )?,
            chasing_risk_top_rank: parse_env_i32("MARKET_VELOCITY_CHASING_RISK_TOP_RANK", 10)?,
            chasing_risk_price_change_pct: parse_env_f64(
                "MARKET_VELOCITY_CHASING_RISK_PRICE_CHANGE_PCT",
                8.0,
            )?,
            trend_min_average_distance_pct: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT",
                DEFAULT_TREND_MIN_AVERAGE_DISTANCE_PCT,
            )?,
            entry_confirmation_period: parse_env_usize("MARKET_VELOCITY_ENTRY_PERIOD", 20)?,
            entry_confirmation_fetch_limit: parse_env_u32("MARKET_VELOCITY_ENTRY_FETCH_LIMIT", 80)?,
            entry_max_average_distance_pct: parse_env_f64(
                "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT",
                DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT,
            )?,
            entry_min_volume_ratio: parse_env_f64("MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO", 1.0)?,
            entry_trigger_allowlist: parse_env_entry_trigger_list(
                "MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST",
                DEFAULT_ENTRY_TRIGGER_ALLOWLIST,
            )?,
            entry_trigger_blocklist: parse_env_entry_trigger_list(
                "MARKET_VELOCITY_ENTRY_TRIGGER_BLOCKLIST",
                &[],
            )?,
            symbol_blocklist: parse_env_symbol_list(
                "MARKET_VELOCITY_SYMBOL_BLOCKLIST",
                DEFAULT_SYMBOL_BLOCKLIST,
            )?,
        })
    }

    pub fn entry_confirmation_config(&self) -> MarketVelocityEntryConfirmationConfig {
        MarketVelocityEntryConfirmationConfig {
            period: self.entry_confirmation_period,
            max_average_distance_pct: self.entry_max_average_distance_pct,
            min_volume_ratio: self.entry_min_volume_ratio,
        }
    }
}

fn market_velocity_execution_policy_stage(
    config: &MarketVelocityStrategySignalConfig,
) -> &'static str {
    let mode = config.automation_mode.trim().to_ascii_lowercase();
    if mode.contains("dry_run") || mode.contains("dry-run") {
        "execution_task_dry_run"
    } else if config.live_order_allowed {
        "live_execution_allowed"
    } else {
        "signal_only_paper"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketVelocityStrategySignalBlocker {
    UnsupportedEventType,
    RankDeltaTooWeak,
    RankOutsideTradeWindow,
    PriceDirectionNotUp,
    MissingCurrentPrice,
    SymbolFiltered,
    InvalidStopLossConfig,
    InvalidRiskRewardConfig,
    ChasingRisk,
    TechnicalConfirmationMissing,
    TechnicalTrendNotConfirmed,
    EntryTimingMissing,
    EntryTimingNotConfirmed,
    EntryTimingOverextended,
    EntryTriggerFiltered,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarketVelocityStrategySignalDecision {
    Submit(StrategySignalSubmitRequest),
    Blocked(MarketVelocityStrategySignalBlocker),
}

pub async fn dispatch_market_velocity_strategy_signal_if_enabled(
    event: &MarketRankEvent,
) -> Result<Option<StrategySignalDispatchResponse>> {
    dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled(event, None).await
}

pub async fn dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled(
    event: &MarketRankEvent,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
) -> Result<Option<StrategySignalDispatchResponse>> {
    if !market_velocity_signal_dispatch_is_enabled() {
        return Ok(None);
    }

    let config = MarketVelocityStrategySignalConfig::from_env()?;
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        event,
        &config,
        entry_confirmation,
    )?;
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

pub fn market_velocity_signal_dispatch_is_enabled() -> bool {
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

pub fn market_velocity_strategy_signal_needs_entry_confirmation(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
) -> Result<bool> {
    Ok(pre_entry_signal_blocker(event, config)?.is_none() && config.require_entry_confirmation)
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
    build_market_velocity_strategy_signal_request_with_entry_confirmation(event, config, None)
}

pub fn build_market_velocity_strategy_signal_request_with_entry_confirmation(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
) -> Result<MarketVelocityStrategySignalDecision> {
    if let Some(blocker) = pre_entry_signal_blocker(event, config)? {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(blocker));
    }

    if let Some(blocker) = entry_confirmation_blocker(entry_confirmation, config) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(blocker));
    }

    build_market_velocity_strategy_signal_submit_request(event, config, entry_confirmation)
}

fn pre_entry_signal_blocker(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
) -> Result<Option<MarketVelocityStrategySignalBlocker>> {
    if !matches!(
        event.event_type,
        MarketRankEventType::RankVelocity | MarketRankEventType::TopEntry
    ) {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::UnsupportedEventType,
        ));
    }

    if !matches!(event.delta_rank, Some(delta) if delta >= config.min_delta_rank) {
        return Ok(Some(MarketVelocityStrategySignalBlocker::RankDeltaTooWeak));
    }

    if symbol_is_blocked(&event.symbol, config) {
        return Ok(Some(MarketVelocityStrategySignalBlocker::SymbolFiltered));
    }

    if !matches!(event.new_rank, Some(rank) if rank > 0 && rank <= config.max_new_rank) {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::RankOutsideTradeWindow,
        ));
    }

    if event.price_direction.trim().to_ascii_lowercase() != "up" {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::PriceDirectionNotUp,
        ));
    }

    let Some(entry_price) = decimal_to_positive_f64(event.current_price) else {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::MissingCurrentPrice,
        ));
    };
    if !(0.0..1.0).contains(&config.stop_loss_pct) {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }
    if config.take_profit_r <= 0.0 || !config.take_profit_r.is_finite() {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::InvalidRiskRewardConfig,
        ));
    }
    if config.max_holding_hours == 0 {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::InvalidRiskRewardConfig,
        ));
    }

    let selected_stop_loss_price = round_price(entry_price * (1.0 - config.stop_loss_pct));
    if selected_stop_loss_price <= 0.0 || selected_stop_loss_price >= entry_price {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }

    if is_chasing_risk(event, config) {
        return Ok(Some(MarketVelocityStrategySignalBlocker::ChasingRisk));
    }

    if let Some(blocker) = technical_confirmation_blocker(event, config) {
        return Ok(Some(blocker));
    }

    Ok(None)
}

fn build_market_velocity_strategy_signal_submit_request(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
) -> Result<MarketVelocityStrategySignalDecision> {
    let entry_price = decimal_to_positive_f64(event.current_price)
        .ok_or_else(|| anyhow!("market velocity event current_price is missing"))?;
    let selected_stop_loss_price = round_price(entry_price * (1.0 - config.stop_loss_pct));
    let selected_take_profit_price =
        round_price(entry_price + (entry_price - selected_stop_loss_price) * config.take_profit_r);

    if selected_stop_loss_price <= 0.0 || selected_stop_loss_price >= entry_price {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }
    if selected_take_profit_price <= entry_price {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::InvalidRiskRewardConfig,
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
    let confidence = market_velocity_confidence(event);
    let generated_at = Some(event.detected_at.to_rfc3339_opts(SecondsFormat::Secs, true));
    let event_type = event.event_type.as_str();
    let period = market_velocity_strategy_signal_period(event, entry_confirmation);
    let config_id = rank_event_id.unwrap_or_else(|| event.detected_at.timestamp_millis());
    let client_order_id =
        market_velocity_client_order_id(rank_event_id, event.detected_at.timestamp_millis());
    let signal = market_velocity_signal_result(
        config,
        entry_price,
        selected_stop_loss_price,
        selected_take_profit_price,
        event.detected_at.timestamp_millis(),
    );
    let risk_config = market_velocity_risk_config(config);
    let payload_overlay = json!({
        "source": "rust_quant",
        "source_signal_type": "market_velocity",
        "rank_event_id": rank_event_id,
        "event_type": event_type,
        "strategy_slug": strategy_slug,
        "paper_strategy_preset": config.strategy_preset.trim(),
        "entry_rule_version": config.entry_rule_version.trim(),
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
        "entry_filter": {
            "status": "confirmed",
            "mode": DEFAULT_MARKET_VELOCITY_ENTRY_FILTER_MODE,
            "entry_rule_version": config.entry_rule_version.trim(),
            "paper_strategy_preset": config.strategy_preset.trim(),
            "technical_confirmation_required": config.require_technical_confirmation,
            "entry_confirmation_required": config.require_entry_confirmation,
            "min_delta_rank": config.min_delta_rank,
            "max_new_rank": config.max_new_rank,
            "anti_chase_top_rank": config.chasing_risk_top_rank,
            "anti_chase_price_change_pct": config.chasing_risk_price_change_pct,
            "trend_min_average_distance_pct": config.trend_min_average_distance_pct,
            "entry_max_average_distance_pct": config.entry_max_average_distance_pct,
            "entry_min_volume_ratio": config.entry_min_volume_ratio,
            "entry_trigger_filter_version": ENTRY_TRIGGER_FILTER_VERSION,
            "entry_trigger_allowlist": &config.entry_trigger_allowlist,
            "entry_trigger_blocklist": &config.entry_trigger_blocklist,
            "symbol_blocklist": &config.symbol_blocklist,
        },
        "entry_confirmation": entry_confirmation,
        "side": "buy",
        "position_side": "long",
        "trade_side": "open",
        "order_type": "market",
        "auto_execution_allowed": config.live_order_allowed,
        "execution_policy": {
            "mode": &config.automation_mode,
            "live_order_allowed": config.live_order_allowed,
            "paper_trade_required": config.paper_trade_required,
            "production_stage": market_velocity_execution_policy_stage(config),
        },
        "risk_plan": {
            "entry_price": entry_price,
            "selected_stop_loss_price": selected_stop_loss_price,
            "selected_take_profit_price": selected_take_profit_price,
            "direction": "long",
            "protective_stop_loss_required": true,
            "stop_loss_source": "market_velocity_default_stop_loss_pct",
            "stop_loss_percent": config.stop_loss_pct,
            "target_r": config.take_profit_r,
            "max_holding_hours": config.max_holding_hours,
            "reward_to_risk_mode": "fixed_r",
        },
        "detected_at": generated_at.as_deref(),
    });

    let mut request = build_strategy_signal_submit_request(
        &symbol,
        &period,
        &signal,
        &risk_config,
        config_id,
        strategy_slug,
        Some(&exchange),
        "buy",
        "long",
        &client_order_id,
        StrategySignalPayloadBuildOptions {
            source_signal_type: "market_velocity".to_string(),
            external_id_override: Some(external_id),
            payload_overlay: Some(payload_overlay),
        },
    )?;
    request.title = format!("Market Velocity long signal {symbol}");
    request.summary = Some(format!(
        "{} ranking improved from {:?} to {:?}, delta {:?}, price direction {}",
        symbol, event.old_rank, event.new_rank, event.delta_rank, event.price_direction
    ));
    request.confidence = Some(confidence);

    Ok(MarketVelocityStrategySignalDecision::Submit(request))
}

fn market_velocity_strategy_signal_period(
    event: &MarketRankEvent,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
) -> String {
    entry_confirmation
        .map(|confirmation| confirmation.timeframe.trim())
        .filter(|value| !value.is_empty())
        .or_else(|| event.timeframe.as_deref().map(str::trim))
        .map(normalize_market_velocity_timeframe)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "15m".to_string())
}

fn normalize_market_velocity_timeframe(timeframe: &str) -> String {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "15分钟" | "15min" | "15mins" | "15minute" | "15minutes" | "15m" => "15m".to_string(),
        "1小时" | "1h" | "60m" | "60min" => "1h".to_string(),
        "4小时" | "4h" | "240m" | "240min" => "4H".to_string(),
        other => other.to_string(),
    }
}

fn market_velocity_client_order_id(rank_event_id: Option<i64>, signal_ts: i64) -> String {
    match rank_event_id {
        Some(id) => format!("rqmv{id}{signal_ts}"),
        None => format!("rqmv{signal_ts}"),
    }
}

fn market_velocity_signal_result(
    config: &MarketVelocityStrategySignalConfig,
    entry_price: f64,
    selected_stop_loss_price: f64,
    selected_take_profit_price: f64,
    signal_ts: i64,
) -> SignalResult {
    SignalResult {
        should_buy: true,
        should_sell: false,
        open_price: entry_price,
        signal_kline_stop_loss_price: Some(selected_stop_loss_price),
        stop_loss_source: Some("market_velocity_fixed_03sl".to_string()),
        best_open_price: None,
        atr_take_profit_ratio_price: None,
        atr_stop_loss_price: None,
        long_signal_take_profit_price: Some(selected_take_profit_price),
        short_signal_take_profit_price: None,
        ts: signal_ts,
        single_value: None,
        single_result: None,
        is_ema_short_trend: None,
        is_ema_long_trend: None,
        atr_take_profit_level_1: None,
        atr_take_profit_level_2: None,
        atr_take_profit_level_3: None,
        filter_reasons: Vec::new(),
        dynamic_adjustments: vec!["market_velocity_fixed_risk".to_string()],
        dynamic_config_snapshot: Some(
            json!({
                "strategy_preset": config.strategy_preset.trim(),
                "entry_rule_version": config.entry_rule_version.trim(),
                "stop_loss_pct": config.stop_loss_pct,
                "take_profit_r": config.take_profit_r,
                "max_holding_hours": config.max_holding_hours,
            })
            .to_string(),
        ),
        direction: SignalDirection::Long,
    }
}

fn market_velocity_risk_config(config: &MarketVelocityStrategySignalConfig) -> BasicRiskConfig {
    BasicRiskConfig {
        max_loss_percent: config.stop_loss_pct,
        atr_take_profit_ratio: None,
        fix_signal_kline_take_profit_ratio: Some(config.take_profit_r),
        is_move_stop_loss: None,
        is_used_signal_k_line_stop_loss: Some(true),
        max_hold_time: Some(i64::from(config.max_holding_hours) * 60 * 60),
        max_leverage: None,
    }
}

fn is_chasing_risk(event: &MarketRankEvent, config: &MarketVelocityStrategySignalConfig) -> bool {
    if config.chasing_risk_top_rank <= 0 || config.chasing_risk_price_change_pct <= 0.0 {
        return false;
    }

    let rank_is_chasing_zone =
        matches!(event.new_rank, Some(rank) if rank > 0 && rank <= config.chasing_risk_top_rank);
    let price_is_extended = event
        .price_change_pct
        .and_then(decimal_to_f64)
        .is_some_and(|value| value >= config.chasing_risk_price_change_pct);
    rank_is_chasing_zone && price_is_extended
}

fn technical_confirmation_blocker(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
) -> Option<MarketVelocityStrategySignalBlocker> {
    if !config.require_technical_confirmation {
        return None;
    }

    if event.technical_snapshot_status.trim() != "captured" {
        return Some(MarketVelocityStrategySignalBlocker::TechnicalConfirmationMissing);
    }
    let Some(snapshot) = event.technical_snapshot.as_ref() else {
        return Some(MarketVelocityStrategySignalBlocker::TechnicalConfirmationMissing);
    };

    if !moving_average_state_is_positive(&snapshot.ma_state)
        || !moving_average_state_is_positive(&snapshot.ema_state)
    {
        return Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed);
    }

    if config.trend_min_average_distance_pct > 0.0 {
        let Some(ma_distance_pct) = decimal_to_f64(snapshot.ma_distance_pct) else {
            return Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed);
        };
        let Some(ema_distance_pct) = decimal_to_f64(snapshot.ema_distance_pct) else {
            return Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed);
        };
        if ma_distance_pct < config.trend_min_average_distance_pct
            || ema_distance_pct < config.trend_min_average_distance_pct
        {
            return Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed);
        }
    }

    None
}

fn entry_confirmation_blocker(
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
    config: &MarketVelocityStrategySignalConfig,
) -> Option<MarketVelocityStrategySignalBlocker> {
    if !config.require_entry_confirmation {
        return None;
    }

    let Some(confirmation) = entry_confirmation else {
        return Some(MarketVelocityStrategySignalBlocker::EntryTimingMissing);
    };
    if !confirmation.timeframe.eq_ignore_ascii_case("15m")
        || confirmation.period != config.entry_confirmation_period
        || confirmation.trigger.trim().is_empty()
        || confirmation.latest_close <= confirmation.ma_value
        || confirmation.latest_close <= confirmation.ema_value
    {
        return Some(MarketVelocityStrategySignalBlocker::EntryTimingNotConfirmed);
    }

    if config.entry_max_average_distance_pct > 0.0
        && (confirmation.ma_distance_pct > config.entry_max_average_distance_pct
            || confirmation.ema_distance_pct > config.entry_max_average_distance_pct)
    {
        return Some(MarketVelocityStrategySignalBlocker::EntryTimingOverextended);
    }

    if config.entry_min_volume_ratio > 0.0 {
        match confirmation.volume_ratio {
            Some(ratio) if ratio >= config.entry_min_volume_ratio => {}
            _ => return Some(MarketVelocityStrategySignalBlocker::EntryTimingNotConfirmed),
        }
    }

    if !entry_trigger_allowed(&confirmation.trigger, config) {
        return Some(MarketVelocityStrategySignalBlocker::EntryTriggerFiltered);
    }

    None
}

fn entry_trigger_allowed(trigger: &str, config: &MarketVelocityStrategySignalConfig) -> bool {
    let normalized = normalize_entry_trigger(trigger);
    if !config.entry_trigger_allowlist.is_empty()
        && !config
            .entry_trigger_allowlist
            .iter()
            .any(|allowed| normalize_entry_trigger(allowed) == normalized)
    {
        return false;
    }

    !config
        .entry_trigger_blocklist
        .iter()
        .any(|blocked| normalize_entry_trigger(blocked) == normalized)
}

fn moving_average_state_is_positive(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "above" | "breakout_up"
    )
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

fn default_entry_trigger_allowlist() -> Vec<String> {
    DEFAULT_ENTRY_TRIGGER_ALLOWLIST
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn default_symbol_blocklist() -> Vec<String> {
    DEFAULT_SYMBOL_BLOCKLIST
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn symbol_is_blocked(symbol: &str, config: &MarketVelocityStrategySignalConfig) -> bool {
    let normalized = normalize_symbol(symbol);
    config
        .symbol_blocklist
        .iter()
        .any(|blocked| normalize_symbol(blocked) == normalized)
}

fn parse_env_entry_trigger_list(key: &str, default: &[&str]) -> Result<Vec<String>> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    }
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }

    let mut triggers = Vec::new();
    for trigger in value.split(',').map(normalize_entry_trigger) {
        if trigger.is_empty() || triggers.contains(&trigger) {
            continue;
        }
        triggers.push(trigger);
    }
    if triggers.is_empty() {
        return Err(anyhow!("{key} must contain at least one entry trigger"));
    }
    Ok(triggers)
}

fn normalize_entry_trigger(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn parse_env_symbol_list(key: &str, default: &[&str]) -> Result<Vec<String>> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    }
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }

    let mut symbols = Vec::new();
    for symbol in value.split(',').map(normalize_symbol) {
        if symbol.is_empty() || symbols.contains(&symbol) {
            continue;
        }
        symbols.push(symbol);
    }
    if symbols.is_empty() {
        return Err(anyhow!("{key} must contain at least one symbol"));
    }
    Ok(symbols)
}

fn normalize_symbol(value: &str) -> String {
    value.trim().to_ascii_uppercase()
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

fn parse_env_u32(key: &str, default: u32) -> Result<u32> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_env_usize(key: &str, default: usize) -> Result<usize> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<usize>()
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

fn parse_env_bool(key: &str, default: bool) -> Result<bool> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => Ok(default),
        "1" | "true" | "yes" | "y" | "on" | "enabled" => Ok(true),
        "0" | "false" | "no" | "n" | "off" | "disabled" => Ok(false),
        _ => Err(anyhow!("{key} must be a boolean")),
    }
}

fn parse_env_string(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests;
