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
const DEFAULT_ENTRY_TRIGGER_ALLOWLIST: &[&str] = &["breakout_previous_high"];
const DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET: &str = "stop_reentry_025sl_24r_v1";
const DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1";
const DEFAULT_MARKET_VELOCITY_ENTRY_FILTER_MODE: &str = "rank_radar_4h_trend_15m_stop_reentry";
const DEFAULT_STOP_LOSS_PCT: f64 = 0.025;
const DEFAULT_TAKE_PROFIT_R: f64 = 2.4;
const DEFAULT_MAX_HOLDING_HOURS: u32 = 48;
const DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT: f64 = 1.5;

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
    pub entry_confirmation_period: usize,
    pub entry_confirmation_fetch_limit: u32,
    pub entry_max_average_distance_pct: f64,
    pub entry_min_volume_ratio: f64,
    pub entry_trigger_allowlist: Vec<String>,
    pub entry_trigger_blocklist: Vec<String>,
}

impl Default for MarketVelocityStrategySignalConfig {
    fn default() -> Self {
        Self {
            strategy_slug: "market_velocity".to_string(),
            strategy_preset: DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET.to_string(),
            entry_rule_version: DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION.to_string(),
            min_delta_rank: 10,
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
            entry_confirmation_period: 20,
            entry_confirmation_fetch_limit: 80,
            entry_max_average_distance_pct: DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT,
            entry_min_volume_ratio: 1.0,
            entry_trigger_allowlist: default_entry_trigger_allowlist(),
            entry_trigger_blocklist: Vec::new(),
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
            min_delta_rank: parse_env_i32("MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK", 10)?,
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
            "entry_max_average_distance_pct": config.entry_max_average_distance_pct,
            "entry_min_volume_ratio": config.entry_min_volume_ratio,
            "entry_trigger_filter_version": ENTRY_TRIGGER_FILTER_VERSION,
            "entry_trigger_allowlist": &config.entry_trigger_allowlist,
            "entry_trigger_blocklist": &config.entry_trigger_blocklist,
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
        stop_loss_source: Some("market_velocity_stop_reentry_025sl".to_string()),
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
        dynamic_adjustments: vec!["market_velocity_stop_reentry".to_string()],
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

    if moving_average_state_is_positive(&snapshot.ma_state)
        && moving_average_state_is_positive(&snapshot.ema_state)
    {
        None
    } else {
        Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed)
    }
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
mod tests {
    use chrono::{DateTime, Utc};
    use rust_decimal::Decimal;
    use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
    use serde_json::{json, Value};

    use super::{
        build_market_velocity_strategy_signal_request,
        build_market_velocity_strategy_signal_request_with_entry_confirmation,
        dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled,
        should_dispatch_market_velocity_signal_to_quant_web_from_env,
        MarketVelocityEntryConfirmation, MarketVelocityStrategySignalBlocker,
        MarketVelocityStrategySignalConfig, MarketVelocityStrategySignalDecision,
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
            new_rank: Some(30),
            delta_rank: Some(13),
            volume_24h_quote: Some(Decimal::new(120_000_000, 0)),
            current_price,
            previous_price: Some(Decimal::new(3200, 0)),
            price_change_pct: Some(Decimal::new(625, 2)),
            price_direction: price_direction.to_string(),
            technical_snapshot_status: "captured".to_string(),
            technical_snapshot: Some(rust_quant_domain::entities::MarketRankTechnicalSnapshot {
                timeframe: "4h".to_string(),
                period: 20,
                close_price: Decimal::new(3400, 0),
                ma_value: Decimal::new(3300, 0),
                ema_value: Decimal::new(3320, 0),
                ma_distance_pct: Decimal::new(303, 2),
                ema_distance_pct: Decimal::new(241, 2),
                ma_state: "above".to_string(),
                ema_state: "breakout_up".to_string(),
                candle_count: 80,
                snapshot_at: DateTime::from_timestamp(1_774_814_400, 0)
                    .expect("valid test timestamp"),
            }),
            detected_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
            source: "scanner_service".to_string(),
            notification_state: "pending".to_string(),
        }
    }

    fn entry_confirmation() -> MarketVelocityEntryConfirmation {
        entry_confirmation_with_trigger("breakout_previous_high")
    }

    fn entry_confirmation_with_trigger(trigger: &str) -> MarketVelocityEntryConfirmation {
        MarketVelocityEntryConfirmation {
            timeframe: "15m".to_string(),
            period: 20,
            trigger: trigger.to_string(),
            latest_close: 3400.0,
            previous_close: Some(3330.0),
            previous_high: Some(3388.0),
            ma_value: 3350.0,
            ema_value: 3348.0,
            ma_distance_pct: 1.49,
            ema_distance_pct: 1.49,
            volume_ratio: Some(1.2),
            candle_count: 80,
            snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
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

        let confirmation = entry_confirmation();
        let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&confirmation),
        )
        .expect("valid market velocity event should be evaluated");

        let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
            panic!("strong rank velocity event should submit a strategy signal");
        };
        assert_eq!(request.source, "rust_quant");
        assert_eq!(request.external_id, "rust_quant:market_velocity:991");
        assert_eq!(request.strategy_slug, "market_velocity");
        assert_eq!(
            request.strategy_key,
            "market_velocity:ETH-USDT-SWAP:15m:991"
        );
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
        assert_eq!(payload["auto_execution_allowed"], false);
        assert_eq!(payload["execution_policy"]["mode"], "signal_only");
        assert_eq!(payload["execution_policy"]["live_order_allowed"], false);
        assert_eq!(payload["execution_policy"]["paper_trade_required"], true);
        assert_eq!(
            payload["execution_policy"]["production_stage"],
            "signal_only_paper"
        );
        assert_eq!(
            payload["paper_strategy_preset"],
            "stop_reentry_025sl_24r_v1"
        );
        assert_eq!(
            payload["entry_rule_version"],
            "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1"
        );
        assert_eq!(payload["risk_plan"]["entry_price"], 3400.0);
        assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3315.0);
        assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3604.0);
        assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.025);
        assert_eq!(payload["risk_plan"]["target_r"], 2.4);
        assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
        assert_eq!(payload["risk_plan"]["reward_to_risk_mode"], "fixed_r");
        assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
        assert_eq!(payload["entry_filter"]["status"], "confirmed");
        assert_eq!(
            payload["entry_filter"]["mode"],
            "rank_radar_4h_trend_15m_stop_reentry"
        );
        assert_eq!(
            payload["entry_filter"]["entry_rule_version"],
            "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1"
        );
        assert_eq!(
            payload["entry_filter"]["paper_strategy_preset"],
            "stop_reentry_025sl_24r_v1"
        );
        assert_eq!(payload["entry_filter"]["min_delta_rank"], 10);
        assert_eq!(payload["entry_filter"]["max_new_rank"], 30);
        assert_eq!(
            payload["entry_filter"]["entry_trigger_filter_version"],
            "entry_trigger_allowlist_v1"
        );
        assert_eq!(
            payload["entry_filter"]["entry_trigger_allowlist"],
            json!(["breakout_previous_high"])
        );
        assert_eq!(
            payload["entry_filter"]["entry_trigger_blocklist"],
            json!([])
        );
        assert_eq!(payload["entry_confirmation"]["timeframe"], "15m");
        assert_eq!(
            payload["entry_confirmation"]["trigger"],
            "breakout_previous_high"
        );
    }

    #[test]
    fn market_velocity_payload_reuses_strategy_signal_live_entry_contract() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("valid market velocity event should be evaluated");

        let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
            panic!("strong rank velocity event should submit a strategy signal");
        };

        let payload: Value =
            serde_json::from_str(&request.payload_json).expect("payload should be valid json");
        assert_eq!(payload["source_signal_type"], "market_velocity");
        assert_eq!(payload["strategy_type"], "market_velocity");
        assert_eq!(
            payload["strategy_key"],
            "market_velocity:ETH-USDT-SWAP:15m:991"
        );
        assert_eq!(payload["client_order_id"], "rqmv9911774814400000");
        assert_eq!(payload["signal"]["should_buy"], true);
        assert_eq!(payload["signal"]["should_sell"], false);
        assert_eq!(payload["signal"]["open_price"], 3400.0);
        assert_eq!(payload["signal"]["signal_kline_stop_loss_price"], 3315.0);
        assert_eq!(payload["signal"]["long_signal_take_profit_price"], 3604.0);
        assert_eq!(
            payload["signal"]["stop_loss_source"],
            "market_velocity_stop_reentry_025sl"
        );
        assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3315.0);
        assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3604.0);
        assert_eq!(payload["risk_plan"]["target_r"], 2.4);
        assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
    }

    #[test]
    fn default_market_velocity_signal_payload_uses_stop_reentry_profit_preset() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("valid market velocity event should be evaluated");

        let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
            panic!("default production event should submit a strategy signal");
        };
        let payload: Value =
            serde_json::from_str(&request.payload_json).expect("payload should be valid json");

        assert_eq!(config.stop_loss_pct, 0.025);
        assert_eq!(config.take_profit_r, 2.4);
        assert_eq!(config.max_holding_hours, 48);
        assert_eq!(config.entry_max_average_distance_pct, 1.5);
        assert_eq!(
            payload["paper_strategy_preset"],
            "stop_reentry_025sl_24r_v1"
        );
        assert_eq!(
            payload["entry_rule_version"],
            "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1"
        );
        assert_eq!(
            payload["entry_filter"]["mode"],
            "rank_radar_4h_trend_15m_stop_reentry"
        );
        assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3315.0);
        assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3604.0);
        assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.025);
        assert_eq!(payload["risk_plan"]["target_r"], 2.4);
        assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
        assert_eq!(
            payload["entry_filter"]["entry_max_average_distance_pct"],
            1.5
        );
    }

    #[test]
    fn market_velocity_default_entry_filter_blocks_overextended_15m_confirmation() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );
        let mut confirmation = entry_confirmation();
        confirmation.ema_distance_pct = 1.51;

        assert_eq!(
            build_market_velocity_strategy_signal_request_with_entry_confirmation(
                &event,
                &config,
                Some(&confirmation),
            )
            .expect("valid market velocity event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::EntryTimingOverextended
            )
        );
    }

    #[test]
    fn market_velocity_default_entry_trigger_filter_blocks_weak_trigger() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );
        let confirmation = entry_confirmation_with_trigger("pullback_hold_ema");

        assert_eq!(
            build_market_velocity_strategy_signal_request_with_entry_confirmation(
                &event,
                &config,
                Some(&confirmation),
            )
            .expect("event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::EntryTriggerFiltered
            )
        );
    }

    #[test]
    fn market_velocity_default_entry_trigger_filter_blocks_lower_win_rate_trigger() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );
        let confirmation = entry_confirmation_with_trigger("reclaim_ema");

        assert_eq!(
            build_market_velocity_strategy_signal_request_with_entry_confirmation(
                &event,
                &config,
                Some(&confirmation),
            )
            .expect("event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::EntryTriggerFiltered
            )
        );
    }

    #[test]
    fn market_velocity_entry_trigger_blocklist_has_precedence() {
        let config = MarketVelocityStrategySignalConfig {
            entry_trigger_allowlist: vec![
                "breakout_previous_high".to_string(),
                "reclaim_ema".to_string(),
            ],
            entry_trigger_blocklist: vec!["reclaim_ema".to_string()],
            ..MarketVelocityStrategySignalConfig::default()
        };
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        assert_eq!(
            build_market_velocity_strategy_signal_request_with_entry_confirmation(
                &event,
                &config,
                Some(&entry_confirmation_with_trigger("reclaim_ema")),
            )
            .expect("event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::EntryTriggerFiltered
            )
        );
    }

    #[test]
    fn dry_run_execution_task_mode_marks_payload_stage_as_dry_run() {
        let config = MarketVelocityStrategySignalConfig {
            automation_mode: "execution_task_dry_run".to_string(),
            live_order_allowed: true,
            paper_trade_required: false,
            ..MarketVelocityStrategySignalConfig::default()
        };
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("valid market velocity event should be evaluated");

        let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
            panic!("dry-run execution task mode should submit a strategy signal");
        };
        let payload: Value =
            serde_json::from_str(&request.payload_json).expect("payload should be valid json");

        assert_eq!(payload["auto_execution_allowed"], true);
        assert_eq!(
            payload["execution_policy"]["mode"],
            "execution_task_dry_run"
        );
        assert_eq!(payload["execution_policy"]["live_order_allowed"], true);
        assert_eq!(payload["execution_policy"]["paper_trade_required"], false);
        assert_eq!(
            payload["execution_policy"]["production_stage"],
            "execution_task_dry_run"
        );
    }

    #[test]
    fn live_execution_authorized_mode_marks_payload_as_live_allowed() {
        let config = MarketVelocityStrategySignalConfig {
            automation_mode: "live_execution_authorized".to_string(),
            live_order_allowed: true,
            paper_trade_required: false,
            ..MarketVelocityStrategySignalConfig::default()
        };
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("valid market velocity event should be evaluated");

        let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
            panic!("live execution authorized mode should submit a strategy signal");
        };
        let payload: Value =
            serde_json::from_str(&request.payload_json).expect("payload should be valid json");

        assert_eq!(payload["auto_execution_allowed"], true);
        assert_eq!(
            payload["execution_policy"]["mode"],
            "live_execution_authorized"
        );
        assert_eq!(payload["execution_policy"]["live_order_allowed"], true);
        assert_eq!(payload["execution_policy"]["paper_trade_required"], false);
        assert_eq!(
            payload["execution_policy"]["production_stage"],
            "live_execution_allowed"
        );
        assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
        assert_eq!(
            payload["entry_confirmation"]["trigger"],
            "breakout_previous_high"
        );
    }

    #[test]
    fn market_velocity_blocks_missing_technical_confirmation() {
        let config = MarketVelocityStrategySignalConfig::default();
        let mut event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );
        event.technical_snapshot = None;

        assert_eq!(
            build_market_velocity_strategy_signal_request_with_entry_confirmation(
                &event,
                &config,
                Some(&entry_confirmation()),
            )
            .expect("event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::TechnicalConfirmationMissing
            )
        );
    }

    #[test]
    fn market_velocity_blocks_chasing_top_rank_after_large_price_jump() {
        let config = MarketVelocityStrategySignalConfig::default();
        let mut event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );
        event.new_rank = Some(8);
        event.price_change_pct = Some(Decimal::new(850, 2));

        assert_eq!(
            build_market_velocity_strategy_signal_request_with_entry_confirmation(
                &event,
                &config,
                Some(&entry_confirmation()),
            )
            .expect("event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::ChasingRisk
            )
        );
    }

    #[test]
    fn market_velocity_blocks_missing_15m_entry_confirmation() {
        let config = MarketVelocityStrategySignalConfig::default();
        let event = rank_event(
            MarketRankEventType::RankVelocity,
            "up",
            Some(Decimal::new(3400, 0)),
        );

        assert_eq!(
            build_market_velocity_strategy_signal_request(&event, &config)
                .expect("event should be evaluated"),
            MarketVelocityStrategySignalDecision::Blocked(
                MarketVelocityStrategySignalBlocker::EntryTimingMissing
            )
        );
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

        let confirmation = entry_confirmation();
        let response = dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled(
            &event,
            Some(&confirmation),
        )
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
