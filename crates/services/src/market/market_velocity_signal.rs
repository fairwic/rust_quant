pub use super::market_velocity_entry::MarketVelocityEntryConfirmation;
use super::market_velocity_entry::MarketVelocityEntryConfirmationConfig;
use crate::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalDispatchResponse,
    StrategySignalSubmitRequest,
};
use crate::strategy::strategy_signal_payload::{
    build_strategy_signal_submit_request, StrategySignalPayloadBuildOptions,
};
use anyhow::{anyhow, Result};
use chrono::SecondsFormat;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use rust_quant_domain::{BasicRiskConfig, SignalDirection};
use rust_quant_strategies::strategy_common::SignalResult;
use serde_json::{json, Value};
use std::time::Duration;
use tracing::info;
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
    /// 策略slug，用于配置运行参数。
    pub strategy_slug: String,
    /// 策略preset，用于配置运行参数。
    pub strategy_preset: String,
    /// 入场ruleversion，用于配置运行参数。
    pub entry_rule_version: String,
    /// 最小delta排名，用于控制策略触发门槛。
    pub min_delta_rank: i32,
    /// 最大new排名，用于控制策略触发门槛。
    pub max_new_rank: i32,
    /// 止损百分比。
    pub stop_loss_pct: f64,
    /// 止盈收益r，用于配置运行参数。
    pub take_profit_r: f64,
    /// runnertargetR 倍数；为空时表示该条件不启用。
    pub runner_target_r: Option<f64>,
    /// runnerfraction，用于配置运行参数。
    pub runner_fraction: f64,
    /// runner止损r，用于配置运行参数。
    pub runner_stop_r: f64,
    /// 小时级时长。
    pub max_holding_hours: u32,
    /// automation模式，用于配置运行参数。
    pub automation_mode: String,
    /// 是否允许该操作。
    pub live_order_allowed: bool,
    /// 模拟盘traderequired，用于配置运行参数。
    pub paper_trade_required: bool,
    /// requiretechnicalconfirmation，用于配置运行参数。
    pub require_technical_confirmation: bool,
    /// require入场confirmation，用于配置运行参数。
    pub require_entry_confirmation: bool,
    /// chasing风险top排名，用于配置运行参数。
    pub chasing_risk_top_rank: i32,
    /// chasingrisk价格变化百分比。
    pub chasing_risk_price_change_pct: f64,
    /// 趋势过滤的最小平均距离百分比。
    pub trend_min_average_distance_pct: f64,
    /// 入场confirmation周期，用于配置运行参数。
    pub entry_confirmation_period: usize,
    /// 入场confirmationfetchlimit，用于配置运行参数。
    pub entry_confirmation_fetch_limit: u32,
    /// 入场最大平均距离百分比。
    pub entry_max_average_distance_pct: f64,
    /// 入场最小volume 比例。
    pub entry_min_volume_ratio: f64,
    /// 列表数据。
    pub entry_trigger_allowlist: Vec<String>,
    /// 列表数据。
    pub entry_trigger_blocklist: Vec<String>,
    /// 列表数据。
    pub symbol_blocklist: Vec<String>,
}
impl Default for MarketVelocityStrategySignalConfig {
    /// 提供默认参数，保证 行情与市场数据 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            strategy_slug: "market_velocity".to_string(),
            strategy_preset: DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET.to_string(),
            entry_rule_version: DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION.to_string(),
            min_delta_rank: 15,
            max_new_rank: 30,
            stop_loss_pct: DEFAULT_STOP_LOSS_PCT,
            take_profit_r: DEFAULT_TAKE_PROFIT_R,
            runner_target_r: None,
            runner_fraction: 0.0,
            runner_stop_r: 0.0,
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
    /// 从外部输入转换为内部模型，隔离 行情与市场数据 的字段适配细节。
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
            runner_target_r: parse_env_optional_f64("MARKET_VELOCITY_SIGNAL_RUNNER_TARGET_R")?,
            runner_fraction: parse_env_f64("MARKET_VELOCITY_SIGNAL_RUNNER_FRACTION", 0.0)?,
            runner_stop_r: parse_env_f64("MARKET_VELOCITY_SIGNAL_RUNNER_STOP_R", 0.0)?,
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
    /// 从外部输入转换为内部模型，隔离 行情与市场数据 的字段适配细节。
    pub fn from_strategy_config_json(config: &Value, risk_config: &Value) -> Result<Self> {
        let mut parsed = Self::default();
        if let Some(value) = json_string(config, "strategy_slug")? {
            parsed.strategy_slug = value;
        }
        if let Some(value) = json_string(config, "strategy_preset")? {
            parsed.strategy_preset = value;
        }
        if let Some(value) = json_string(config, "entry_rule_version")? {
            parsed.entry_rule_version = value;
        }
        if let Some(value) = json_i32(config, "min_delta_rank")? {
            parsed.min_delta_rank = value;
        }
        if let Some(value) = json_i32(config, "max_new_rank")? {
            parsed.max_new_rank = value;
        }
        if let Some(value) = json_f64(config, "stop_loss_pct")? {
            parsed.stop_loss_pct = value;
        }
        if let Some(value) = json_f64(config, "take_profit_r")? {
            parsed.take_profit_r = value;
        }
        if let Some(value) = json_u32(config, "max_holding_hours")? {
            parsed.max_holding_hours = value;
        }
        if let Some(value) = json_string(config, "automation_mode")? {
            parsed.automation_mode = value;
        }
        if let Some(value) = json_bool(config, "live_order_allowed")? {
            parsed.live_order_allowed = value;
        }
        if let Some(value) = json_bool(config, "paper_trade_required")? {
            parsed.paper_trade_required = value;
        }
        if let Some(value) = json_bool(config, "require_technical_confirmation")? {
            parsed.require_technical_confirmation = value;
        }
        if let Some(value) = json_bool(config, "require_entry_confirmation")? {
            parsed.require_entry_confirmation = value;
        }
        if let Some(value) = json_i32(config, "chasing_risk_top_rank")? {
            parsed.chasing_risk_top_rank = value;
        }
        if let Some(value) = json_f64(config, "chasing_risk_price_change_pct")? {
            parsed.chasing_risk_price_change_pct = value;
        }
        if let Some(value) = json_f64(config, "trend_min_average_distance_pct")? {
            parsed.trend_min_average_distance_pct = value;
        }
        if let Some(value) = json_usize(config, "entry_confirmation_period")? {
            parsed.entry_confirmation_period = value;
        }
        if let Some(value) = json_u32(config, "entry_confirmation_fetch_limit")? {
            parsed.entry_confirmation_fetch_limit = value;
        }
        if let Some(value) = json_f64(config, "entry_max_average_distance_pct")? {
            parsed.entry_max_average_distance_pct = value;
        }
        if let Some(value) = json_f64(config, "entry_min_volume_ratio")? {
            parsed.entry_min_volume_ratio = value;
        }
        if let Some(value) = json_entry_trigger_list(config, "entry_trigger_allowlist")? {
            parsed.entry_trigger_allowlist = value;
        }
        if let Some(value) = json_entry_trigger_list(config, "entry_trigger_blocklist")? {
            parsed.entry_trigger_blocklist = value;
        }
        if let Some(value) = json_symbol_list(config, "symbol_blocklist")? {
            parsed.symbol_blocklist = value;
        }
        if let Some(value) = json_f64_any(risk_config, &["max_loss_percent", "stop_loss_pct"])? {
            parsed.stop_loss_pct = value;
        }
        if let Some(value) = json_f64_any(
            risk_config,
            &[
                "fix_signal_kline_take_profit_ratio",
                "fixed_signal_kline_take_profit_ratio",
                "take_profit_r",
            ],
        )? {
            parsed.take_profit_r = value;
        }
        if let Some(value) = json_f64(risk_config, "runner_target_r")? {
            parsed.runner_target_r = Some(value);
        }
        if let Some(value) = json_f64(risk_config, "runner_fraction")? {
            parsed.runner_fraction = value;
        }
        if let Some(value) = json_f64(risk_config, "runner_stop_r")? {
            parsed.runner_stop_r = value;
        }
        if let Some(value) = json_i64(risk_config, "max_hold_time")? {
            parsed.max_holding_hours = max_holding_hours_from_seconds(value)?;
        }
        if let Some(value) = json_u32(risk_config, "max_holding_hours")? {
            parsed.max_holding_hours = value;
        }
        Ok(parsed)
    }
    /// 提供入场确认配置的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn entry_confirmation_config(&self) -> MarketVelocityEntryConfirmationConfig {
        MarketVelocityEntryConfirmationConfig {
            period: self.entry_confirmation_period,
            max_average_distance_pct: self.entry_max_average_distance_pct,
            min_volume_ratio: self.entry_min_volume_ratio,
        }
    }
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
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
#[derive(Clone, Debug, PartialEq)]
struct MarketVelocityStrategySignalLogContext {
    external_id: String,
    source_signal_type: String,
    rank_event_id: Option<i64>,
    exchange: String,
    symbol: String,
    entry_rule_version: Option<String>,
    production_stage: Option<String>,
}
pub async fn dispatch_market_velocity_strategy_signal_if_enabled(
    event: &MarketRankEvent,
) -> Result<Option<StrategySignalDispatchResponse>> {
    dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled(event, None).await
}
/// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled(
    event: &MarketRankEvent,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
) -> Result<Option<StrategySignalDispatchResponse>> {
    if !market_velocity_signal_dispatch_is_enabled() {
        return Ok(None);
    }
    let config = MarketVelocityStrategySignalConfig::from_env()?;
    dispatch_market_velocity_strategy_signal_with_config_and_entry_confirmation(
        event,
        &config,
        entry_confirmation,
    )
    .await
}
/// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn dispatch_market_velocity_strategy_signal_with_config_and_entry_confirmation(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
) -> Result<Option<StrategySignalDispatchResponse>> {
    if !market_velocity_signal_dispatch_is_enabled() {
        return Ok(None);
    }
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        event,
        config,
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
    let log_context = market_velocity_strategy_signal_log_context(&request);
    info!(
        external_id = %log_context.external_id,
        source_signal_type = %log_context.source_signal_type,
        rank_event_id = ?log_context.rank_event_id,
        exchange = %log_context.exchange,
        symbol = %log_context.symbol,
        entry_rule_version = ?log_context.entry_rule_version,
        production_stage = ?log_context.production_stage,
        "Market Velocity strategy signal promoted to quant_web"
    );
    let client = ExecutionTaskClient::new(market_velocity_execution_task_config_from_env()?)?;
    let timeout_secs = parse_env_u64("MARKET_VELOCITY_SIGNAL_DISPATCH_TIMEOUT_SECS", 5)?;
    let response = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client.submit_strategy_signal(request),
    )
    .await
    .map_err(|_| anyhow!("submit market velocity strategy signal timeout"))??;
    let generated_task_ids: Vec<i64> = response
        .generated_tasks
        .iter()
        .map(|task| task.id)
        .collect();
    info!(
        external_id = %log_context.external_id,
        source_signal_type = %log_context.source_signal_type,
        rank_event_id = ?log_context.rank_event_id,
        exchange = %log_context.exchange,
        symbol = %log_context.symbol,
        strategy_signal_id = response.inbox.id,
        generated_task_count = response.generated_tasks.len(),
        generated_task_ids = ?generated_task_ids,
        "Submitted Market Velocity strategy signal to rust_quan_web"
    );
    Ok(Some(response))
}
fn market_velocity_strategy_signal_log_context(
    request: &StrategySignalSubmitRequest,
) -> MarketVelocityStrategySignalLogContext {
    let payload = serde_json::from_str::<Value>(&request.payload_json).unwrap_or(Value::Null);
    let source_signal_type = payload
        .get("source_signal_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let exchange = payload
        .get("exchange")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| request.strategy_key.split(':').nth(1).unwrap_or(""))
        .to_ascii_lowercase();
    let symbol = payload
        .get("symbol")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&request.symbol)
        .to_ascii_uppercase();
    let entry_rule_version = payload
        .get("entry_rule_version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let production_stage = payload
        .get("execution_policy")
        .and_then(|value| value.get("production_stage"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    MarketVelocityStrategySignalLogContext {
        external_id: request.external_id.clone(),
        source_signal_type,
        rank_event_id: payload.get("rank_event_id").and_then(Value::as_i64),
        exchange,
        symbol,
        entry_rule_version,
        production_stage,
    }
}
/// 提供市场动量信号dispatchisenabled的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 判断 行情与市场数据 条件是否满足，给上层流程提供布尔决策。
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
/// 提供市场动量执行task配置from环境变量的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
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
/// 提供pre入场信号blocker的集中实现，避免行情数据调用方重复处理相同细节。
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
    if !runner_config_valid(config) {
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
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
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
    let mut risk_plan = json!({
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
    });
    if let Some(take_profit_legs) = market_velocity_take_profit_legs(
        config,
        entry_price,
        selected_stop_loss_price,
        selected_take_profit_price,
    ) {
        risk_plan["take_profit_legs"] = take_profit_legs;
    }
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
        "risk_plan": risk_plan,
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
/// 提供市场动量策略信号period的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn normalize_market_velocity_timeframe(timeframe: &str) -> String {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "15分钟" | "15min" | "15mins" | "15minute" | "15minutes" | "15m" => "15m".to_string(),
        "1小时" | "1h" | "60m" | "60min" => "1h".to_string(),
        "4小时" | "4h" | "240m" | "240min" => "4H".to_string(),
        other => other.to_string(),
    }
}
/// 提供市场动量client订单ID的集中实现，避免行情数据调用方重复处理相同细节。
fn market_velocity_client_order_id(rank_event_id: Option<i64>, signal_ts: i64) -> String {
    match rank_event_id {
        Some(id) => format!("rqmv{id}{signal_ts}"),
        None => format!("rqmv{signal_ts}"),
    }
}
/// 提供市场动量信号结果的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供市场动量风控配置的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 判断 行情与市场数据 条件是否满足，给上层流程提供布尔决策。
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
/// 提供technical确认blocker的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供入场确认blocker的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供入场触发allowed的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供movingaverage状态ispositive的集中实现，避免行情数据调用方重复处理相同细节。
fn moving_average_state_is_positive(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "above" | "breakout_up"
    )
}
fn decimal_to_f64(value: Decimal) -> Option<f64> {
    value.to_f64().filter(|number| number.is_finite())
}
/// 提供小数topositivef64的集中实现，避免行情数据调用方重复处理相同细节。
fn decimal_to_positive_f64(value: Option<Decimal>) -> Option<f64> {
    value
        .and_then(decimal_to_f64)
        .filter(|number| *number > 0.0)
}
fn round_price(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
/// 执行 Runner配置valid步骤，串起行情数据需要的状态推进和错误处理。
fn runner_config_valid(config: &MarketVelocityStrategySignalConfig) -> bool {
    match config.runner_target_r {
        Some(target_r) => {
            target_r.is_finite()
                && target_r > config.take_profit_r
                && config.runner_fraction.is_finite()
                && config.runner_fraction > 0.0
                && config.runner_fraction < 1.0
                && config.runner_stop_r.is_finite()
                && config.runner_stop_r >= 0.0
                && config.runner_stop_r < config.take_profit_r
        }
        None => config.runner_fraction == 0.0 && config.runner_stop_r == 0.0,
    }
}
/// 提供市场动量take盈利legs的集中实现，避免行情数据调用方重复处理相同细节。
fn market_velocity_take_profit_legs(
    config: &MarketVelocityStrategySignalConfig,
    entry_price: f64,
    selected_stop_loss_price: f64,
    selected_take_profit_price: f64,
) -> Option<Value> {
    let runner_target_r = config.runner_target_r?;
    if !runner_config_valid(config) {
        return None;
    }
    let risk_per_unit = entry_price - selected_stop_loss_price;
    if !risk_per_unit.is_finite() || risk_per_unit <= 0.0 {
        return None;
    }
    let runner_price = round_price(entry_price + risk_per_unit * runner_target_r);
    Some(json!([
        {
            "leg_index": 1,
            "target_r": config.take_profit_r,
            "fraction": round_fraction(1.0 - config.runner_fraction),
            "price": selected_take_profit_price,
            "stop_after_fill_r": config.runner_stop_r,
            "role": "base_take_profit",
        },
        {
            "leg_index": 2,
            "target_r": runner_target_r,
            "fraction": round_fraction(config.runner_fraction),
            "price": runner_price,
            "role": "runner_take_profit",
        }
    ]))
}
fn round_fraction(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
/// 提供市场动量confidence的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供默认入场触发allowlist的集中实现，避免行情数据调用方重复处理相同细节。
fn default_entry_trigger_allowlist() -> Vec<String> {
    DEFAULT_ENTRY_TRIGGER_ALLOWLIST
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}
/// 提供默认交易对blocklist的集中实现，避免行情数据调用方重复处理相同细节。
fn default_symbol_blocklist() -> Vec<String> {
    DEFAULT_SYMBOL_BLOCKLIST
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}
/// 提供交易对isblocked的集中实现，避免行情数据调用方重复处理相同细节。
fn symbol_is_blocked(symbol: &str, config: &MarketVelocityStrategySignalConfig) -> bool {
    let normalized = normalize_symbol(symbol);
    config
        .symbol_blocklist
        .iter()
        .any(|blocked| normalize_symbol(blocked) == normalized)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
fn json_field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|field| !field.is_null())
}
/// 提供JSONstring的集中实现，避免行情数据调用方重复处理相同细节。
fn json_string(value: &Value, key: &str) -> Result<Option<String>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    let text = match field {
        Value::String(value) => value.trim().to_string(),
        _ => return Err(anyhow!("{key} must be a string")),
    };
    Ok((!text.is_empty()).then_some(text))
}
/// 提供JSONi64的集中实现，避免行情数据调用方重复处理相同细节。
fn json_i64(value: &Value, key: &str) -> Result<Option<i64>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    match field {
        Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| anyhow!("{key} must be an integer"))
            .map(Some),
        Value::String(value) => value
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|error| anyhow!("{key} must be an integer: {error}")),
        _ => Err(anyhow!("{key} must be an integer")),
    }
}
/// 提供JSONi32的集中实现，避免行情数据调用方重复处理相同细节。
fn json_i32(value: &Value, key: &str) -> Result<Option<i32>> {
    json_i64(value, key)?
        .map(|value| {
            i32::try_from(value).map_err(|error| anyhow!("{key} is out of i32 range: {error}"))
        })
        .transpose()
}
/// 提供JSONu32的集中实现，避免行情数据调用方重复处理相同细节。
fn json_u32(value: &Value, key: &str) -> Result<Option<u32>> {
    json_i64(value, key)?
        .map(|value| {
            u32::try_from(value).map_err(|error| anyhow!("{key} is out of u32 range: {error}"))
        })
        .transpose()
}
/// 提供JSONusize的集中实现，避免行情数据调用方重复处理相同细节。
fn json_usize(value: &Value, key: &str) -> Result<Option<usize>> {
    json_i64(value, key)?
        .map(|value| {
            usize::try_from(value).map_err(|error| anyhow!("{key} is out of usize range: {error}"))
        })
        .transpose()
}
/// 提供JSONf64的集中实现，避免行情数据调用方重复处理相同细节。
fn json_f64(value: &Value, key: &str) -> Result<Option<f64>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    match field {
        Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| anyhow!("{key} must be a number"))
            .map(Some),
        Value::String(value) => value
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|error| anyhow!("{key} must be a number: {error}")),
        _ => Err(anyhow!("{key} must be a number")),
    }
}
/// 提供JSONf64any的集中实现，避免行情数据调用方重复处理相同细节。
fn json_f64_any(value: &Value, keys: &[&str]) -> Result<Option<f64>> {
    for key in keys {
        if let Some(value) = json_f64(value, key)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}
/// 提供JSONbool的集中实现，避免行情数据调用方重复处理相同细节。
fn json_bool(value: &Value, key: &str) -> Result<Option<bool>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    match field {
        Value::Bool(value) => Ok(Some(*value)),
        Value::String(value) => parse_bool_text(value, key).map(Some),
        _ => Err(anyhow!("{key} must be a boolean")),
    }
}
fn json_entry_trigger_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
    json_string_list(value, key, normalize_entry_trigger)
}
fn json_symbol_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
    json_string_list(value, key, normalize_symbol)
}
/// 提供JSONstringlist的集中实现，避免行情数据调用方重复处理相同细节。
fn json_string_list(
    value: &Value,
    key: &str,
    normalize: fn(&str) -> String,
) -> Result<Option<Vec<String>>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    let raw_items = match field {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(value) => Ok(value.as_str()),
                _ => Err(anyhow!("{key} must be an array of strings")),
            })
            .collect::<Result<Vec<_>>>()?,
        Value::String(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            if matches!(normalized.as_str(), "" | "all" | "*" | "none") {
                return Ok(Some(Vec::new()));
            }
            value.split(',').collect()
        }
        _ => return Err(anyhow!("{key} must be an array of strings or csv string")),
    };
    let mut values = Vec::new();
    for item in raw_items.into_iter().map(normalize) {
        if item.is_empty() || values.contains(&item) {
            continue;
        }
        values.push(item);
    }
    Ok(Some(values))
}
/// 计算最大holdinghours来源seconds，并把公式边界留在行情数据内部。
fn max_holding_hours_from_seconds(seconds: i64) -> Result<u32> {
    if seconds <= 0 {
        return Err(anyhow!("max_hold_time must be positive"));
    }
    let hours = (seconds + 3_599) / 3_600;
    u32::try_from(hours).map_err(|error| anyhow!("max_hold_time is out of u32 range: {error}"))
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_env_optional_f64(key: &str) -> Result<Option<f64>> {
    std::env::var(key)
        .ok()
        .map(|value| {
            let value = value.trim();
            if value.is_empty() {
                Ok(None)
            } else {
                value
                    .parse::<f64>()
                    .map(Some)
                    .map_err(|error| anyhow!("{key} must be a number: {error}"))
            }
        })
        .transpose()
        .map(Option::flatten)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_env_bool(key: &str, default: bool) -> Result<bool> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    if value.trim().is_empty() {
        return Ok(default);
    }
    parse_bool_text(&value, key)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_bool_text(value: &str, key: &str) -> Result<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" | "enabled" => Ok(true),
        "0" | "false" | "no" | "n" | "off" | "disabled" => Ok(false),
        _ => Err(anyhow!("{key} must be a boolean")),
    }
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_env_string(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}
#[cfg(test)]
mod tests;
