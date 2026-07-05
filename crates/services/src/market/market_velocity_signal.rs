pub use super::market_velocity_entry::MarketVelocityEntryConfirmation;
use super::market_velocity_entry::MarketVelocityEntryConfirmationConfig;
use super::market_velocity_signal_config_parse::*;
use crate::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalDispatchResponse,
    StrategySignalSubmitRequest,
};
use crate::strategy::strategy_signal_payload::{
    build_strategy_signal_submit_request, StrategySignalPayloadBuildOptions,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use rust_quant_domain::{BasicRiskConfig, SignalDirection};
use rust_quant_strategies::strategy_common::SignalResult;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;
use tracing::info;
const ENTRY_TRIGGER_FILTER_VERSION: &str = "entry_trigger_allowlist_v1";
const DEFAULT_ENTRY_TRIGGER_ALLOWLIST: &[&str] =
    &["reclaim_ema", "reclaim_ma", "pullback_hold_ema"];
const DEFAULT_SYMBOL_BLOCKLIST: &[&str] = &[];
const DEFAULT_MARKET_VELOCITY_STRATEGY_PRESET: &str =
    "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1";
const DEFAULT_MARKET_VELOCITY_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1";
const DEFAULT_MARKET_VELOCITY_ENTRY_FILTER_MODE: &str = "rank_radar_4h15m_reclaim_ma_pullback";
const DEFAULT_MIN_DELTA_RANK: i32 = 18;
const DEFAULT_MAX_DELTA_RANK: i32 = 42;
const DEFAULT_MIN_PRICE_CHANGE_PCT: f64 = 5.0;
const DEFAULT_MAX_PRICE_CHANGE_PCT: f64 = 10.0;
const DEFAULT_STOP_LOSS_PCT: f64 = 0.0375;
const DEFAULT_TAKE_PROFIT_R: f64 = 1.7;
const DEFAULT_MAX_HOLDING_HOURS: u32 = 48;
const DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT: f64 = 5.5;
const DEFAULT_ENTRY_MIN_VOLUME_RATIO: f64 = 1.0;
const DEFAULT_ENTRY_MAX_SIGNAL_PULLBACK_PCT: Option<f64> = None;
const DEFAULT_ENTRY_RSI_DELTA_LOOKBACK_CANDLES: usize = 3;
const DEFAULT_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES: usize = 12;
const DEFAULT_ENTRY_RETEST_AFTER_SIGNAL: bool = false;
const DEFAULT_ENTRY_RETEST_MAX_WAIT_CANDLES: usize = 8;
const DEFAULT_TREND_MIN_AVERAGE_DISTANCE_PCT: f64 = 0.0;
const DEFAULT_MARKET_VELOCITY_AUTOMATION_MODE: &str = "live_execution_authorized";
const DEFAULT_MARKET_VELOCITY_LIVE_ORDER_ALLOWED: bool = true;
const DEFAULT_MARKET_VELOCITY_PAPER_TRADE_REQUIRED: bool = false;
pub const MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_SLUG: &str = "market_velocity_breakdown_short";
pub const MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_PRESET: &str =
    "research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6";
pub const MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_ENTRY_RULE_VERSION: &str =
    "rank_radar_15m_short_r04_10r_15msup_brkdn_d5_100_p2_12_vol10_d14_v6";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketVelocityFvgEntryMode {
    Off,
    M15ImpulseRetrace,
}

impl MarketVelocityFvgEntryMode {
    pub(super) fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "m15_impulse_retrace" | "15m_impulse_retrace" | "15m-impulse-retrace" => {
                Ok(Self::M15ImpulseRetrace)
            }
            other => Err(anyhow!(
                "unsupported market velocity fvg entry mode: {other}"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::M15ImpulseRetrace => "m15_impulse_retrace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketVelocityStopLossMode {
    FixedPct,
    StructureOrFixed,
    StructureWithCap,
}

impl MarketVelocityStopLossMode {
    pub fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "fixed_pct" | "fixed" | "pct" => Ok(Self::FixedPct),
            "structure_or_fixed" | "structure" | "hybrid_structure" => Ok(Self::StructureOrFixed),
            "structure_with_cap" | "structure_capped" => Ok(Self::StructureWithCap),
            other => Err(anyhow!(
                "unsupported market velocity stop loss mode: {other}"
            )),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FixedPct => "fixed_pct",
            Self::StructureOrFixed => "structure_or_fixed",
            Self::StructureWithCap => "structure_with_cap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketVelocitySignalTradeDirection {
    Long,
    Short,
}

impl MarketVelocitySignalTradeDirection {
    /// 从配置字符串解析交易方向，保持旧配置缺省为多头。
    pub fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "long" | "buy" => Ok(Self::Long),
            "short" | "sell" => Ok(Self::Short),
            other => Err(anyhow!(
                "unsupported market velocity signal trade direction: {other}"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    fn order_side(self) -> &'static str {
        match self {
            Self::Long => "buy",
            Self::Short => "sell",
        }
    }

    fn expected_price_direction(self) -> &'static str {
        match self {
            Self::Long => "up",
            Self::Short => "down",
        }
    }

    fn source_signal_type(self, strategy_slug: &str) -> String {
        match self {
            Self::Long => "market_velocity".to_string(),
            Self::Short => strategy_slug.trim().to_string(),
        }
    }

    fn signal_direction(self) -> SignalDirection {
        match self {
            Self::Long => SignalDirection::Long,
            Self::Short => SignalDirection::Short,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MarketVelocitySelectedEntry {
    pub entry_price: f64,
    pub entry_ts: DateTime<Utc>,
    pub trigger: String,
    pub entry_path: String,
    pub signal_pullback_pct: Option<f64>,
    pub structure_stop_loss_price: Option<f64>,
    pub structure_stop_loss_source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarketVelocityStrategySignalConfig {
    /// 策略slug，用于配置运行参数。
    pub strategy_slug: String,
    /// 策略preset，用于配置运行参数。
    pub strategy_preset: String,
    /// 入场ruleversion，用于配置运行参数。
    pub entry_rule_version: String,
    /// 交易方向；默认沿用旧动量多头，显式 short 时生成做空信号。
    pub trade_direction: MarketVelocitySignalTradeDirection,
    /// 最小delta排名，用于控制策略触发门槛。
    pub min_delta_rank: i32,
    /// 最大delta排名；为空时不限制。
    pub max_delta_rank: Option<i32>,
    /// 最小价格涨跌幅百分比；为空时不限制。
    pub min_price_change_pct: Option<f64>,
    /// 最大价格涨跌幅百分比；为空时不限制，用于避免追高。
    pub max_price_change_pct: Option<f64>,
    /// 止损百分比；在 structure_with_cap 模式下也作为最大风险上限。
    pub stop_loss_pct: f64,
    /// 止损模式。
    pub stop_loss_mode: MarketVelocityStopLossMode,
    /// 结构止损最小百分比；0 表示不对结构锚点额外放宽。
    pub structure_stop_min_pct: f64,
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
    /// 15m 入场 RSI 下限；为空时不启用。
    pub entry_min_rsi: Option<f64>,
    /// 15m 入场 RSI 上限；为空时不启用。
    pub entry_max_rsi: Option<f64>,
    /// 15m 入场 RSI 增量下限；为空时不启用。
    pub entry_min_rsi_delta: Option<f64>,
    /// RSI 增量回看 K 线数量。
    pub entry_rsi_delta_lookback_candles: usize,
    /// 是否要求 15m 布林上轨突破。
    pub entry_bollinger_breakout: bool,
    /// 布林带宽扩张百分比下限；为空时不启用。
    pub entry_min_bollinger_bandwidth_expansion_pct: Option<f64>,
    /// 入场前 recent drawdown 百分比下限；为空时不启用。
    pub entry_min_recent_drawdown_pct: Option<f64>,
    /// recent drawdown 回看 K 线数量。
    pub entry_recent_drawdown_lookback_candles: usize,
    /// signal 当前价到实际入场价允许的最大回撤百分比；为空时不启用。
    pub entry_max_signal_pullback_pct: Option<f64>,
    /// 前高回踩确认允许的价格容差百分比。
    pub entry_retest_tolerance_pct: f64,
    /// 是否等待原始入场信号后的结构回踩确认。
    pub entry_retest_after_signal: bool,
    /// 原始入场信号后等待结构回踩确认的最大 15m K 线数量。
    pub entry_retest_max_wait_candles: usize,
    /// 回踩确认后下一根入场开盘相对确认收盘的最小 gap 百分比；为空时不启用。
    pub entry_retest_min_entry_open_gap_pct: Option<f64>,
    /// 回踩确认后下一根开盘若弱于确认收盘，允许用更高确认量能作为例外通行；为空时不启用。
    pub entry_retest_open_fade_min_volume_ratio: Option<f64>,
    /// FVG 入场模式。
    pub fvg_entry_mode: MarketVelocityFvgEntryMode,
    /// FVG 查找向前回看 K 线数量。
    pub fvg_lookback_candles: usize,
    /// FVG 最大等待 K 线数量。
    pub fvg_max_wait_candles: usize,
    /// m15 impulse retrace 挂单距离 FVG 下沿的百分比。
    pub fvg_impulse_retrace_fill_pct: f64,
    /// m15 impulse retrace 从 signal 到允许 fill 之间至少等待的完整 15m K 线数量。
    pub fvg_impulse_retrace_min_wait_candles: usize,
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
            trade_direction: MarketVelocitySignalTradeDirection::Long,
            min_delta_rank: DEFAULT_MIN_DELTA_RANK,
            max_delta_rank: Some(DEFAULT_MAX_DELTA_RANK),
            min_price_change_pct: Some(DEFAULT_MIN_PRICE_CHANGE_PCT),
            max_price_change_pct: Some(DEFAULT_MAX_PRICE_CHANGE_PCT),
            stop_loss_pct: DEFAULT_STOP_LOSS_PCT,
            stop_loss_mode: MarketVelocityStopLossMode::FixedPct,
            structure_stop_min_pct: 0.0,
            take_profit_r: DEFAULT_TAKE_PROFIT_R,
            runner_target_r: None,
            runner_fraction: 0.0,
            runner_stop_r: 0.0,
            max_holding_hours: DEFAULT_MAX_HOLDING_HOURS,
            automation_mode: DEFAULT_MARKET_VELOCITY_AUTOMATION_MODE.to_string(),
            live_order_allowed: DEFAULT_MARKET_VELOCITY_LIVE_ORDER_ALLOWED,
            paper_trade_required: DEFAULT_MARKET_VELOCITY_PAPER_TRADE_REQUIRED,
            require_technical_confirmation: true,
            require_entry_confirmation: true,
            trend_min_average_distance_pct: DEFAULT_TREND_MIN_AVERAGE_DISTANCE_PCT,
            entry_confirmation_period: 20,
            entry_confirmation_fetch_limit: 80,
            entry_max_average_distance_pct: DEFAULT_ENTRY_MAX_AVERAGE_DISTANCE_PCT,
            entry_min_volume_ratio: DEFAULT_ENTRY_MIN_VOLUME_RATIO,
            entry_min_rsi: None,
            entry_max_rsi: None,
            entry_min_rsi_delta: None,
            entry_rsi_delta_lookback_candles: DEFAULT_ENTRY_RSI_DELTA_LOOKBACK_CANDLES,
            entry_bollinger_breakout: false,
            entry_min_bollinger_bandwidth_expansion_pct: None,
            entry_min_recent_drawdown_pct: None,
            entry_recent_drawdown_lookback_candles: DEFAULT_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES,
            entry_max_signal_pullback_pct: DEFAULT_ENTRY_MAX_SIGNAL_PULLBACK_PCT,
            entry_retest_tolerance_pct: 0.3,
            entry_retest_after_signal: DEFAULT_ENTRY_RETEST_AFTER_SIGNAL,
            entry_retest_max_wait_candles: DEFAULT_ENTRY_RETEST_MAX_WAIT_CANDLES,
            entry_retest_min_entry_open_gap_pct: None,
            entry_retest_open_fade_min_volume_ratio: None,
            fvg_entry_mode: MarketVelocityFvgEntryMode::Off,
            fvg_lookback_candles: 40,
            fvg_max_wait_candles: 24,
            fvg_impulse_retrace_fill_pct: 20.0,
            fvg_impulse_retrace_min_wait_candles: 0,
            entry_trigger_allowlist: default_entry_trigger_allowlist(),
            entry_trigger_blocklist: Vec::new(),
            symbol_blocklist: default_symbol_blocklist(),
        }
    }
}
impl MarketVelocityStrategySignalConfig {
    /// 从外部输入转换为内部模型，隔离 行情与市场数据 的字段适配细节。
    pub fn from_env() -> Result<Self> {
        let mut parsed = Self {
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
            trade_direction: parse_env_signal_trade_direction(
                "MARKET_VELOCITY_SIGNAL_TRADE_DIRECTION",
                MarketVelocitySignalTradeDirection::Long,
            )?,
            min_delta_rank: parse_env_i32(
                "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK",
                DEFAULT_MIN_DELTA_RANK,
            )?,
            max_delta_rank: parse_env_optional_i32_with_default(
                "MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK",
                DEFAULT_MAX_DELTA_RANK,
            )?,
            min_price_change_pct: parse_env_optional_f64(
                "MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT",
            )?
            .or(Some(DEFAULT_MIN_PRICE_CHANGE_PCT)),
            max_price_change_pct: parse_env_optional_f64_with_default(
                "MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT",
                DEFAULT_MAX_PRICE_CHANGE_PCT,
            )?,
            stop_loss_pct: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT",
                DEFAULT_STOP_LOSS_PCT,
            )?,
            stop_loss_mode: parse_env_stop_loss_mode(
                "MARKET_VELOCITY_SIGNAL_STOP_LOSS_MODE",
                MarketVelocityStopLossMode::FixedPct,
            )?,
            structure_stop_min_pct: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_STRUCTURE_STOP_MIN_PCT",
                0.0,
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
            automation_mode: DEFAULT_MARKET_VELOCITY_AUTOMATION_MODE.to_string(),
            live_order_allowed: DEFAULT_MARKET_VELOCITY_LIVE_ORDER_ALLOWED,
            paper_trade_required: DEFAULT_MARKET_VELOCITY_PAPER_TRADE_REQUIRED,
            require_technical_confirmation: parse_env_bool(
                "MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION",
                true,
            )?,
            require_entry_confirmation: parse_env_bool(
                "MARKET_VELOCITY_SIGNAL_REQUIRE_ENTRY_CONFIRMATION",
                true,
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
            entry_min_volume_ratio: parse_env_f64(
                "MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO",
                DEFAULT_ENTRY_MIN_VOLUME_RATIO,
            )?,
            entry_min_rsi: parse_env_optional_f64("MARKET_VELOCITY_ENTRY_MIN_RSI")?,
            entry_max_rsi: parse_env_optional_f64("MARKET_VELOCITY_ENTRY_MAX_RSI")?,
            entry_min_rsi_delta: parse_env_optional_f64("MARKET_VELOCITY_ENTRY_MIN_RSI_DELTA")?,
            entry_rsi_delta_lookback_candles: parse_env_usize(
                "MARKET_VELOCITY_ENTRY_RSI_DELTA_LOOKBACK_CANDLES",
                DEFAULT_ENTRY_RSI_DELTA_LOOKBACK_CANDLES,
            )?,
            entry_bollinger_breakout: parse_env_bool(
                "MARKET_VELOCITY_ENTRY_BOLLINGER_BREAKOUT",
                false,
            )?,
            entry_min_bollinger_bandwidth_expansion_pct: parse_env_optional_f64(
                "MARKET_VELOCITY_ENTRY_MIN_BOLLINGER_BANDWIDTH_EXPANSION_PCT",
            )?,
            entry_min_recent_drawdown_pct: parse_env_optional_f64(
                "MARKET_VELOCITY_ENTRY_MIN_RECENT_DRAWDOWN_PCT",
            )?,
            entry_recent_drawdown_lookback_candles: parse_env_usize(
                "MARKET_VELOCITY_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES",
                DEFAULT_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES,
            )?,
            entry_max_signal_pullback_pct: parse_env_optional_f64(
                "MARKET_VELOCITY_SIGNAL_ENTRY_MAX_SIGNAL_PULLBACK_PCT",
            )?
            .or(DEFAULT_ENTRY_MAX_SIGNAL_PULLBACK_PCT),
            entry_retest_tolerance_pct: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_TOLERANCE_PCT",
                0.3,
            )?,
            entry_retest_after_signal: parse_env_bool(
                "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_AFTER_SIGNAL",
                DEFAULT_ENTRY_RETEST_AFTER_SIGNAL,
            )?,
            entry_retest_max_wait_candles: parse_env_usize(
                "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_MAX_WAIT_CANDLES",
                DEFAULT_ENTRY_RETEST_MAX_WAIT_CANDLES,
            )?,
            entry_retest_min_entry_open_gap_pct: parse_env_optional_f64(
                "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_MIN_ENTRY_OPEN_GAP_PCT",
            )?,
            entry_retest_open_fade_min_volume_ratio: parse_env_optional_f64(
                "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_OPEN_FADE_MIN_VOLUME_RATIO",
            )?,
            fvg_entry_mode: parse_env_fvg_entry_mode(
                "MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE",
                MarketVelocityFvgEntryMode::Off,
            )?,
            fvg_lookback_candles: parse_env_usize(
                "MARKET_VELOCITY_SIGNAL_FVG_LOOKBACK_CANDLES",
                40,
            )?,
            fvg_max_wait_candles: parse_env_usize(
                "MARKET_VELOCITY_SIGNAL_FVG_MAX_WAIT_CANDLES",
                24,
            )?,
            fvg_impulse_retrace_fill_pct: parse_env_f64(
                "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT",
                20.0,
            )?,
            fvg_impulse_retrace_min_wait_candles: parse_env_usize(
                "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_MIN_WAIT_CANDLES",
                0,
            )?,
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
        };
        apply_market_velocity_trade_direction_safety(&mut parsed);
        Ok(parsed)
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
        if let Some(value) = json_signal_trade_direction(config, "trade_direction")? {
            parsed.trade_direction = value;
        } else if let Some(value) = json_signal_trade_direction(config, "direction")? {
            parsed.trade_direction = value;
        }
        if let Some(value) = json_i32(config, "min_delta_rank")? {
            parsed.min_delta_rank = value;
        }
        if json_value_is_null(config, "max_delta_rank") {
            parsed.max_delta_rank = None;
        } else if let Some(value) = json_i32(config, "max_delta_rank")? {
            parsed.max_delta_rank = Some(value);
        }
        if let Some(value) = json_f64(config, "min_price_change_pct")? {
            parsed.min_price_change_pct = Some(value);
        }
        if json_value_is_null(config, "max_price_change_pct") {
            parsed.max_price_change_pct = None;
        } else if let Some(value) = json_f64(config, "max_price_change_pct")? {
            parsed.max_price_change_pct = Some(value);
        }
        if let Some(value) = json_f64(config, "stop_loss_pct")? {
            parsed.stop_loss_pct = value;
        }
        if let Some(value) = json_stop_loss_mode(config, "stop_loss_mode")? {
            parsed.stop_loss_mode = value;
        }
        if let Some(value) = json_f64(config, "structure_stop_min_pct")? {
            parsed.structure_stop_min_pct = value;
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
        if let Some(value) = json_f64(config, "entry_min_rsi")? {
            parsed.entry_min_rsi = Some(value);
        }
        if let Some(value) = json_f64(config, "entry_max_rsi")? {
            parsed.entry_max_rsi = Some(value);
        }
        if let Some(value) = json_f64(config, "entry_min_rsi_delta")? {
            parsed.entry_min_rsi_delta = Some(value);
        }
        if let Some(value) = json_usize(config, "entry_rsi_delta_lookback_candles")? {
            parsed.entry_rsi_delta_lookback_candles = value;
        }
        if let Some(value) = json_bool(config, "entry_bollinger_breakout")? {
            parsed.entry_bollinger_breakout = value;
        }
        if let Some(value) = json_f64(config, "entry_min_bollinger_bandwidth_expansion_pct")? {
            parsed.entry_min_bollinger_bandwidth_expansion_pct = Some(value);
        }
        if let Some(value) = json_f64(config, "entry_min_recent_drawdown_pct")? {
            parsed.entry_min_recent_drawdown_pct = Some(value);
        }
        if let Some(value) = json_usize(config, "entry_recent_drawdown_lookback_candles")? {
            parsed.entry_recent_drawdown_lookback_candles = value;
        }
        if let Some(value) = json_f64(config, "entry_max_signal_pullback_pct")? {
            parsed.entry_max_signal_pullback_pct = Some(value);
        }
        if let Some(value) = json_f64(config, "entry_retest_tolerance_pct")? {
            parsed.entry_retest_tolerance_pct = value;
        }
        if let Some(value) = json_bool(config, "entry_retest_after_signal")? {
            parsed.entry_retest_after_signal = value;
        }
        if let Some(value) = json_usize(config, "entry_retest_max_wait_candles")? {
            parsed.entry_retest_max_wait_candles = value;
        }
        if let Some(value) = json_f64(config, "entry_retest_min_entry_open_gap_pct")? {
            parsed.entry_retest_min_entry_open_gap_pct = Some(value);
        }
        if let Some(value) = json_f64(config, "entry_retest_open_fade_min_volume_ratio")? {
            parsed.entry_retest_open_fade_min_volume_ratio = Some(value);
        }
        if let Some(value) = json_fvg_entry_mode(config, "fvg_entry_mode")? {
            parsed.fvg_entry_mode = value;
        }
        if let Some(value) = json_usize(config, "fvg_lookback_candles")? {
            parsed.fvg_lookback_candles = value;
        }
        if let Some(value) = json_usize(config, "fvg_max_wait_candles")? {
            parsed.fvg_max_wait_candles = value;
        }
        if let Some(value) = json_f64(config, "fvg_impulse_retrace_fill_pct")? {
            parsed.fvg_impulse_retrace_fill_pct = value;
        }
        if let Some(value) = json_usize(config, "fvg_impulse_retrace_min_wait_candles")? {
            parsed.fvg_impulse_retrace_min_wait_candles = value;
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
        if let Some(value) = json_stop_loss_mode(risk_config, "stop_loss_mode")? {
            parsed.stop_loss_mode = value;
        }
        if let Some(value) = json_f64(risk_config, "structure_stop_min_pct")? {
            parsed.structure_stop_min_pct = value;
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
        apply_market_velocity_trade_direction_safety(&mut parsed);
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

    pub fn hybrid_live_entry_enabled(&self) -> bool {
        self.fvg_entry_mode != MarketVelocityFvgEntryMode::Off
            || self.entry_retest_after_signal
            || self.entry_max_signal_pullback_pct.is_some()
            || self.entry_min_rsi.is_some()
            || self.entry_max_rsi.is_some()
            || self.entry_min_rsi_delta.is_some()
            || self.entry_bollinger_breakout
            || self.entry_min_bollinger_bandwidth_expansion_pct.is_some()
            || self.entry_min_recent_drawdown_pct.is_some()
    }
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn market_velocity_execution_policy_stage(
    config: &MarketVelocityStrategySignalConfig,
) -> &'static str {
    if config.paper_trade_required {
        return "paper_signal_only";
    }
    if market_velocity_auto_execution_allowed(config) {
        return "live_execution_allowed";
    }
    "signal_only"
}

/// Only the verified v6 breakdown-short cutover is allowed to keep live flags; every other short config stays paper-only.
fn apply_market_velocity_trade_direction_safety(config: &mut MarketVelocityStrategySignalConfig) {
    if matches!(
        config.trade_direction,
        MarketVelocitySignalTradeDirection::Short
    ) && !market_velocity_breakdown_short_live_cutover_authorized(config)
    {
        config.automation_mode = "signal_only".to_string();
        config.live_order_allowed = false;
        config.paper_trade_required = true;
    }
}
/// 判断破位做空是否匹配当前已验证的 v6 实盘切换合同。
pub fn market_velocity_breakdown_short_live_cutover_authorized(
    config: &MarketVelocityStrategySignalConfig,
) -> bool {
    matches!(
        config.trade_direction,
        MarketVelocitySignalTradeDirection::Short
    ) && config
        .strategy_slug
        .trim()
        .eq_ignore_ascii_case(MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_SLUG)
        && config.strategy_preset.trim() == MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_PRESET
        && config.entry_rule_version.trim()
            == MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_ENTRY_RULE_VERSION
        && market_velocity_auto_execution_allowed(config)
}

/// Normalize runtime safety flags so direct struct construction follows the same rules as env/json config parsing.
fn normalized_market_velocity_strategy_signal_config(
    config: &MarketVelocityStrategySignalConfig,
) -> MarketVelocityStrategySignalConfig {
    let mut config = config.clone();
    apply_market_velocity_trade_direction_safety(&mut config);
    config
}

/// Web task generation should only auto-execute when the strategy policy explicitly authorizes live orders.
fn market_velocity_auto_execution_allowed(config: &MarketVelocityStrategySignalConfig) -> bool {
    config.live_order_allowed
        && !config.paper_trade_required
        && config
            .automation_mode
            .trim()
            .eq_ignore_ascii_case(DEFAULT_MARKET_VELOCITY_AUTOMATION_MODE)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketVelocityStrategySignalBlocker {
    UnsupportedEventType,
    RankDeltaTooWeak,
    PriceDirectionNotUp,
    MissingCurrentPrice,
    SymbolFiltered,
    InvalidStopLossConfig,
    InvalidRiskRewardConfig,
    TechnicalConfirmationMissing,
    TechnicalTrendNotConfirmed,
    EntryTimingMissing,
    EntryTimingNotConfirmed,
    EntryTimingOverextended,
    EntryTriggerFiltered,
    PriceChangeTooLow,
    PriceChangeTooHigh,
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

pub fn market_velocity_signal_direct_dispatch_allowed(
    config: &MarketVelocityStrategySignalConfig,
) -> bool {
    !config.require_entry_confirmation && !config.hybrid_live_entry_enabled()
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
    build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
        event,
        config,
        entry_confirmation,
        None,
    )
}

pub fn build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
    event: &MarketRankEvent,
    config: &MarketVelocityStrategySignalConfig,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
    selected_entry: Option<&MarketVelocitySelectedEntry>,
) -> Result<MarketVelocityStrategySignalDecision> {
    let config = normalized_market_velocity_strategy_signal_config(config);
    if let Some(blocker) = pre_entry_signal_blocker(event, &config)? {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(blocker));
    }
    if let Some(blocker) = entry_confirmation_blocker(entry_confirmation, &config) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(blocker));
    }
    build_market_velocity_strategy_signal_submit_request(
        event,
        &config,
        entry_confirmation,
        selected_entry,
    )
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
    if matches!(
        (event.delta_rank, config.max_delta_rank),
        (Some(delta), Some(max_delta_rank)) if delta > max_delta_rank
    ) {
        return Ok(Some(MarketVelocityStrategySignalBlocker::RankDeltaTooWeak));
    }
    if let Some(min_price_change_pct) = config.min_price_change_pct {
        let price_change_pct = event
            .price_change_pct
            .and_then(decimal_to_f64)
            .unwrap_or_default()
            .abs();
        if price_change_pct < min_price_change_pct {
            return Ok(Some(MarketVelocityStrategySignalBlocker::PriceChangeTooLow));
        }
    }
    if let Some(max_price_change_pct) = config.max_price_change_pct {
        let price_change_pct = event
            .price_change_pct
            .and_then(decimal_to_f64)
            .unwrap_or_default()
            .abs();
        if price_change_pct > max_price_change_pct {
            return Ok(Some(
                MarketVelocityStrategySignalBlocker::PriceChangeTooHigh,
            ));
        }
    }
    if symbol_is_blocked(&event.symbol, config) {
        return Ok(Some(MarketVelocityStrategySignalBlocker::SymbolFiltered));
    }
    if event.price_direction.trim().to_ascii_lowercase()
        != config.trade_direction.expected_price_direction()
    {
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
    if !(0.0..1.0).contains(&config.structure_stop_min_pct) && config.structure_stop_min_pct != 0.0
    {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }
    if config.stop_loss_mode == MarketVelocityStopLossMode::StructureWithCap
        && config.structure_stop_min_pct > config.stop_loss_pct
    {
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
    let selected_stop_loss_price =
        market_velocity_stop_loss_price_for_direction(entry_price, config.stop_loss_pct, config);
    if !market_velocity_stop_loss_is_loss_side(entry_price, selected_stop_loss_price, config) {
        return Ok(Some(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
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
    selected_entry: Option<&MarketVelocitySelectedEntry>,
) -> Result<MarketVelocityStrategySignalDecision> {
    let event_current_price = decimal_to_positive_f64(event.current_price)
        .ok_or_else(|| anyhow!("market velocity event current_price is missing"))?;
    let entry_price = selected_entry
        .map(|entry| entry.entry_price)
        .filter(|price| price.is_finite() && *price > 0.0)
        .unwrap_or(event_current_price);
    let selected_stop_loss =
        select_market_velocity_stop_loss(config, entry_price, entry_confirmation, selected_entry)?;
    let selected_take_profit_price =
        market_velocity_take_profit_price(config, entry_price, selected_stop_loss.price);
    if !market_velocity_stop_loss_is_loss_side(entry_price, selected_stop_loss.price, config) {
        return Ok(MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::InvalidStopLossConfig,
        ));
    }
    if !market_velocity_take_profit_is_profit_side(entry_price, selected_take_profit_price, config)
    {
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
    let direction = config.trade_direction.label();
    let side = config.trade_direction.order_side();
    let source_signal_type = config.trade_direction.source_signal_type(strategy_slug);
    let rank_event_id = event.id;
    let strategy_preset_id = market_velocity_external_id_segment(&config.strategy_preset);
    let entry_rule_version_id = market_velocity_external_id_segment(&config.entry_rule_version);
    let external_id = rank_event_id
        .map(|id| {
            format!(
                "rust_quant:{source_signal_type}:{id}:{strategy_preset_id}:{entry_rule_version_id}"
            )
        })
        .unwrap_or_else(|| {
            format!(
                "rust_quant:{source_signal_type}:{}:{}:{}:{strategy_preset_id}:{entry_rule_version_id}",
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
    let signal_ts = selected_entry
        .map(|entry| entry.entry_ts.timestamp_millis())
        .unwrap_or_else(|| event.detected_at.timestamp_millis());
    let client_order_id = market_velocity_client_order_id(rank_event_id, signal_ts);
    let signal = market_velocity_signal_result(
        config,
        entry_price,
        selected_stop_loss.price,
        selected_stop_loss.source.as_str(),
        selected_stop_loss.pct,
        selected_take_profit_price,
        signal_ts,
    );
    let risk_config = market_velocity_risk_config(config, selected_stop_loss.pct);
    let selected_entry_payload = selected_entry.map(|entry| {
        let (structure_stop_loss_price, structure_stop_loss_source) =
            market_velocity_structure_stop_loss(
                config,
                entry_price,
                entry_confirmation,
                Some(entry),
            )
            .map(|(price, source)| (Some(price), Some(source)))
            .unwrap_or((
                entry.structure_stop_loss_price,
                entry.structure_stop_loss_source.clone(),
            ));
        json!({
            "entry_price": entry.entry_price,
            "entry_ts": entry.entry_ts,
            "trigger": entry.trigger,
            "entry_path": entry.entry_path,
            "signal_pullback_pct": entry.signal_pullback_pct,
            "structure_stop_loss_price": structure_stop_loss_price,
            "structure_stop_loss_source": structure_stop_loss_source,
        })
    });
    let mut risk_plan = json!({
        "entry_price": entry_price,
        "selected_stop_loss_price": selected_stop_loss.price,
        "selected_stop_loss_source": selected_stop_loss.source,
        "selected_stop_loss_percent": selected_stop_loss.pct,
        "selected_take_profit_price": selected_take_profit_price,
        "direction": direction,
        "protective_stop_loss_required": true,
        "structure_stop_min_pct": config.structure_stop_min_pct,
        "stop_loss_source": selected_stop_loss.source,
        "stop_loss_percent": selected_stop_loss.pct,
        "configured_stop_loss_percent": config.stop_loss_pct,
        "stop_loss_selection_mode": config.stop_loss_mode.label(),
        "target_r": config.take_profit_r,
        "max_holding_hours": config.max_holding_hours,
        "reward_to_risk_mode": "fixed_r",
    });
    if let Some(take_profit_legs) = market_velocity_take_profit_legs(
        config,
        entry_price,
        selected_stop_loss.price,
        selected_take_profit_price,
    ) {
        risk_plan["take_profit_legs"] = take_profit_legs;
    }
    let entry_filter = json!({
        "status": "confirmed",
        "mode": DEFAULT_MARKET_VELOCITY_ENTRY_FILTER_MODE,
        "entry_rule_version": config.entry_rule_version.trim(),
        "paper_strategy_preset": config.strategy_preset.trim(),
        "technical_confirmation_required": config.require_technical_confirmation,
        "entry_confirmation_required": config.require_entry_confirmation,
        "min_delta_rank": config.min_delta_rank,
        "max_delta_rank": config.max_delta_rank,
        "min_price_change_pct": config.min_price_change_pct,
        "max_price_change_pct": config.max_price_change_pct,
        "trend_min_average_distance_pct": config.trend_min_average_distance_pct,
        "entry_max_average_distance_pct": config.entry_max_average_distance_pct,
        "entry_min_volume_ratio": config.entry_min_volume_ratio,
        "entry_min_rsi": config.entry_min_rsi,
        "entry_max_rsi": config.entry_max_rsi,
        "entry_min_rsi_delta": config.entry_min_rsi_delta,
        "entry_rsi_delta_lookback_candles": config.entry_rsi_delta_lookback_candles,
        "entry_bollinger_breakout": config.entry_bollinger_breakout,
        "entry_min_bollinger_bandwidth_expansion_pct": config.entry_min_bollinger_bandwidth_expansion_pct,
        "entry_min_recent_drawdown_pct": config.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": config.entry_recent_drawdown_lookback_candles,
        "entry_max_signal_pullback_pct": config.entry_max_signal_pullback_pct,
        "entry_retest_tolerance_pct": config.entry_retest_tolerance_pct,
        "entry_retest_after_signal": config.entry_retest_after_signal,
        "entry_retest_max_wait_candles": config.entry_retest_max_wait_candles,
        "entry_retest_min_entry_open_gap_pct": config.entry_retest_min_entry_open_gap_pct,
        "entry_retest_open_fade_min_volume_ratio": config.entry_retest_open_fade_min_volume_ratio,
        "fvg_entry_mode": config.fvg_entry_mode.label(),
        "fvg_lookback_candles": config.fvg_lookback_candles,
        "fvg_max_wait_candles": config.fvg_max_wait_candles,
        "fvg_impulse_retrace_fill_pct": config.fvg_impulse_retrace_fill_pct,
        "fvg_impulse_retrace_min_wait_candles": config.fvg_impulse_retrace_min_wait_candles,
        "entry_trigger_filter_version": ENTRY_TRIGGER_FILTER_VERSION,
        "trade_direction": direction,
        "entry_trigger_allowlist": &config.entry_trigger_allowlist,
        "entry_trigger_blocklist": &config.entry_trigger_blocklist,
        "symbol_blocklist": &config.symbol_blocklist,
    });
    let execution_policy = json!({
        "mode": config.automation_mode.trim(),
        "live_order_allowed": config.live_order_allowed,
        "paper_trade_required": config.paper_trade_required,
        "production_stage": market_velocity_execution_policy_stage(config),
    });
    let payload_overlay = json!({
        "source": "rust_quant",
        "source_signal_type": &source_signal_type,
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
        "current_price": event_current_price,
        "previous_price": event.previous_price.and_then(decimal_to_f64),
        "price_change_pct": event.price_change_pct.and_then(decimal_to_f64),
        "price_direction": &event.price_direction,
        "technical_snapshot_status": &event.technical_snapshot_status,
        "technical_snapshot": &event.technical_snapshot,
        "entry_filter": entry_filter,
        "entry_confirmation": entry_confirmation,
        "selected_entry": selected_entry_payload,
        "side": side,
        "position_side": direction,
        "trade_side": "open",
        "order_type": "market",
        "auto_execution_allowed": market_velocity_auto_execution_allowed(config),
        "execution_policy": execution_policy,
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
        side,
        direction,
        &client_order_id,
        StrategySignalPayloadBuildOptions {
            source_signal_type,
            external_id_override: Some(external_id),
            payload_overlay: Some(payload_overlay),
        },
    )?;
    request.title = format!("Market Velocity {direction} signal {symbol}");
    request.summary = Some(format!(
        "{} ranking improved from {:?} to {:?}, delta {:?}, price direction {}",
        symbol, event.old_rank, event.new_rank, event.delta_rank, event.price_direction
    ));
    request.confidence = Some(confidence);
    Ok(MarketVelocityStrategySignalDecision::Submit(request))
}
/// Keep Core/Web idempotency stable while making same-event strategy versions distinguishable.
fn market_velocity_external_id_segment(value: &str) -> String {
    let segment: String = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if segment.is_empty() {
        "unversioned".to_string()
    } else {
        segment
    }
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
    selected_stop_loss_source: &str,
    selected_stop_loss_pct: f64,
    selected_take_profit_price: f64,
    signal_ts: i64,
) -> SignalResult {
    let is_short = config.trade_direction == MarketVelocitySignalTradeDirection::Short;
    SignalResult {
        should_buy: !is_short,
        should_sell: is_short,
        open_price: entry_price,
        signal_kline_stop_loss_price: Some(selected_stop_loss_price),
        stop_loss_source: Some(selected_stop_loss_source.to_string()),
        best_open_price: None,
        atr_take_profit_ratio_price: None,
        atr_stop_loss_price: None,
        long_signal_take_profit_price: (!is_short).then_some(selected_take_profit_price),
        short_signal_take_profit_price: is_short.then_some(selected_take_profit_price),
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
                "configured_stop_loss_pct": config.stop_loss_pct,
                "selected_stop_loss_pct": selected_stop_loss_pct,
                "stop_loss_mode": config.stop_loss_mode.label(),
                "structure_stop_min_pct": config.structure_stop_min_pct,
                "take_profit_r": config.take_profit_r,
                "max_holding_hours": config.max_holding_hours,
            })
            .to_string(),
        ),
        direction: config.trade_direction.signal_direction(),
    }
}

fn market_velocity_fixed_stop_loss_source(stop_loss_pct: f64) -> String {
    let basis_points = (stop_loss_pct * 10_000.0).round() as i64;
    let tag = format!("{basis_points:04}")
        .trim_end_matches('0')
        .to_string();
    format!("market_velocity_fixed_{tag}sl")
}
/// 提供市场动量风控配置的集中实现，避免行情数据调用方重复处理相同细节。
fn market_velocity_risk_config(
    config: &MarketVelocityStrategySignalConfig,
    selected_stop_loss_pct: f64,
) -> BasicRiskConfig {
    BasicRiskConfig {
        max_loss_percent: selected_stop_loss_pct,
        atr_take_profit_ratio: None,
        fix_signal_kline_take_profit_ratio: Some(config.take_profit_r),
        is_move_stop_loss: None,
        is_used_signal_k_line_stop_loss: Some(true),
        max_hold_time: Some(i64::from(config.max_holding_hours) * 60 * 60),
        max_leverage: None,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct MarketVelocitySelectedStopLoss {
    price: f64,
    pct: f64,
    source: String,
}

fn market_velocity_stop_loss_price_for_direction(
    entry_price: f64,
    stop_loss_pct: f64,
    config: &MarketVelocityStrategySignalConfig,
) -> f64 {
    let multiplier = match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => 1.0 - stop_loss_pct,
        MarketVelocitySignalTradeDirection::Short => 1.0 + stop_loss_pct,
    };
    round_price(entry_price * multiplier)
}

fn market_velocity_stop_loss_is_loss_side(
    entry_price: f64,
    stop_loss_price: f64,
    config: &MarketVelocityStrategySignalConfig,
) -> bool {
    if !stop_loss_price.is_finite() || stop_loss_price <= 0.0 {
        return false;
    }
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => stop_loss_price < entry_price,
        MarketVelocitySignalTradeDirection::Short => stop_loss_price > entry_price,
    }
}

fn market_velocity_take_profit_price(
    config: &MarketVelocityStrategySignalConfig,
    entry_price: f64,
    stop_loss_price: f64,
) -> f64 {
    let risk_per_unit = (entry_price - stop_loss_price).abs();
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => {
            round_price(entry_price + risk_per_unit * config.take_profit_r)
        }
        MarketVelocitySignalTradeDirection::Short => {
            round_price(entry_price - risk_per_unit * config.take_profit_r)
        }
    }
}

fn market_velocity_take_profit_is_profit_side(
    entry_price: f64,
    take_profit_price: f64,
    config: &MarketVelocityStrategySignalConfig,
) -> bool {
    if !take_profit_price.is_finite() || take_profit_price <= 0.0 {
        return false;
    }
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => take_profit_price > entry_price,
        MarketVelocitySignalTradeDirection::Short => take_profit_price < entry_price,
    }
}

fn market_velocity_should_use_structure_stop(
    entry_price: f64,
    structure_price: f64,
    fixed_price: f64,
    config: &MarketVelocityStrategySignalConfig,
) -> bool {
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => {
            structure_price < entry_price && structure_price > fixed_price
        }
        MarketVelocitySignalTradeDirection::Short => {
            structure_price > entry_price && structure_price < fixed_price
        }
    }
}

fn select_market_velocity_stop_loss(
    config: &MarketVelocityStrategySignalConfig,
    entry_price: f64,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
    selected_entry: Option<&MarketVelocitySelectedEntry>,
) -> Result<MarketVelocitySelectedStopLoss> {
    let fixed_price =
        market_velocity_stop_loss_price_for_direction(entry_price, config.stop_loss_pct, config);
    let fixed_source = market_velocity_fixed_stop_loss_source(config.stop_loss_pct);
    let structure = market_velocity_structure_stop_loss(
        config,
        entry_price,
        entry_confirmation,
        selected_entry,
    )
    .map(|(price, source)| {
        apply_structure_stop_min_pct_floor(
            entry_price,
            price,
            source,
            config.structure_stop_min_pct,
            config,
        )
    });
    let (price, source) = match (config.stop_loss_mode, structure) {
        (
            MarketVelocityStopLossMode::StructureOrFixed,
            Some((structure_price, structure_source)),
        ) if market_velocity_should_use_structure_stop(
            entry_price,
            structure_price,
            fixed_price,
            config,
        ) =>
        {
            (structure_price, structure_source)
        }
        (
            MarketVelocityStopLossMode::StructureWithCap,
            Some((structure_price, structure_source)),
        ) => apply_structure_stop_max_pct_cap(
            entry_price,
            structure_price,
            structure_source,
            config.stop_loss_pct,
            config,
        ),
        _ => (fixed_price, fixed_source),
    };
    let pct = round_ratio((entry_price - price).abs() / entry_price);
    if !pct.is_finite() || pct <= 0.0 || pct >= 1.0 {
        return Err(anyhow!(
            "invalid market velocity selected stop loss percent"
        ));
    }
    Ok(MarketVelocitySelectedStopLoss { price, pct, source })
}

fn apply_structure_stop_min_pct_floor(
    entry_price: f64,
    structure_price: f64,
    structure_source: String,
    structure_stop_min_pct: f64,
    config: &MarketVelocityStrategySignalConfig,
) -> (f64, String) {
    if structure_stop_min_pct <= 0.0 {
        return (structure_price, structure_source);
    }
    let floor_price =
        market_velocity_stop_loss_price_for_direction(entry_price, structure_stop_min_pct, config);
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Short if structure_price < floor_price => {
            (floor_price, format!("{structure_source}+min_pct_floor"))
        }
        MarketVelocitySignalTradeDirection::Long if structure_price > floor_price => {
            (floor_price, format!("{structure_source}+min_pct_floor"))
        }
        _ => (structure_price, structure_source),
    }
}

fn apply_structure_stop_max_pct_cap(
    entry_price: f64,
    structure_price: f64,
    structure_source: String,
    stop_loss_pct: f64,
    config: &MarketVelocityStrategySignalConfig,
) -> (f64, String) {
    let cap_price =
        market_velocity_stop_loss_price_for_direction(entry_price, stop_loss_pct, config);
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Short if structure_price > cap_price => {
            (cap_price, format!("{structure_source}+max_pct_cap"))
        }
        MarketVelocitySignalTradeDirection::Long if structure_price < cap_price => {
            (cap_price, format!("{structure_source}+max_pct_cap"))
        }
        _ => (structure_price, structure_source),
    }
}

fn market_velocity_structure_stop_loss(
    config: &MarketVelocityStrategySignalConfig,
    entry_price: f64,
    entry_confirmation: Option<&MarketVelocityEntryConfirmation>,
    selected_entry: Option<&MarketVelocitySelectedEntry>,
) -> Option<(f64, String)> {
    let selected_entry = selected_entry?;
    let explicit_price = selected_entry
        .structure_stop_loss_price
        .filter(|price| market_velocity_stop_loss_is_loss_side(entry_price, *price, config));
    if let Some(price) = explicit_price {
        let source = selected_entry
            .structure_stop_loss_source
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "selected_entry_structure".to_string());
        return Some((round_price(price), source));
    }
    let confirmation = entry_confirmation?;
    let base_trigger = market_velocity_base_entry_trigger(
        selected_entry.trigger.as_str(),
        confirmation.trigger.as_str(),
    );
    match base_trigger.as_str() {
        "reclaim_ema" => Some((
            round_price(confirmation.ema_value),
            "entry_confirmation_ema".to_string(),
        )),
        "reclaim_ma" => Some((
            round_price(confirmation.ma_value),
            "entry_confirmation_ma".to_string(),
        )),
        "breakout_previous_high" => confirmation
            .previous_high
            .map(round_price)
            .map(|price| (price, "entry_confirmation_previous_high".to_string())),
        "reject_ema" | "breakdown_range_low" => Some((
            round_price(confirmation.ema_value),
            "entry_confirmation_ema".to_string(),
        )),
        "reject_ma" => Some((
            round_price(confirmation.ma_value),
            "entry_confirmation_ma".to_string(),
        )),
        _ => None,
    }
    .filter(|(price, _)| market_velocity_stop_loss_is_loss_side(entry_price, *price, config))
}

fn market_velocity_base_entry_trigger(
    selected_entry_trigger: &str,
    confirmation_trigger: &str,
) -> String {
    let base = selected_entry_trigger
        .split_once('+')
        .map(|(base, _)| base)
        .unwrap_or(selected_entry_trigger)
        .trim();
    if base.is_empty() {
        confirmation_trigger.trim().to_string()
    } else {
        base.to_string()
    }
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
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => {
            if !moving_average_state_is_positive(&snapshot.ma_state)
                || !moving_average_state_is_positive(&snapshot.ema_state)
            {
                return Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed);
            }
        }
        MarketVelocitySignalTradeDirection::Short => {
            if !moving_average_state_is_negative(&snapshot.ma_state)
                || !moving_average_state_is_negative(&snapshot.ema_state)
            {
                return Some(MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed);
            }
        }
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
        || !entry_confirmation_price_structure_is_confirmed(confirmation, config)
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

fn moving_average_state_is_negative(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "below" | "breakdown_down"
    )
}

fn entry_confirmation_price_structure_is_confirmed(
    confirmation: &MarketVelocityEntryConfirmation,
    config: &MarketVelocityStrategySignalConfig,
) -> bool {
    match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => {
            confirmation.latest_close > confirmation.ma_value
                && confirmation.latest_close > confirmation.ema_value
        }
        MarketVelocitySignalTradeDirection::Short => {
            confirmation.latest_close < confirmation.ma_value
                && confirmation.latest_close < confirmation.ema_value
        }
    }
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
const MARKET_VELOCITY_PRICE_ROUND_SCALE: f64 = 1_000_000.0;
const MARKET_VELOCITY_LOW_PRICE_ROUND_SCALE: f64 = 1_000_000_000_000.0;
const MARKET_VELOCITY_LOW_PRICE_THRESHOLD: f64 = 1.0;

fn round_price(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let scale = if value.abs() < MARKET_VELOCITY_LOW_PRICE_THRESHOLD {
        MARKET_VELOCITY_LOW_PRICE_ROUND_SCALE
    } else {
        MARKET_VELOCITY_PRICE_ROUND_SCALE
    };
    (value * scale).round() / scale
}

fn round_ratio(value: f64) -> f64 {
    (value * MARKET_VELOCITY_PRICE_ROUND_SCALE).round() / MARKET_VELOCITY_PRICE_ROUND_SCALE
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
    let runner_price = match config.trade_direction {
        MarketVelocitySignalTradeDirection::Long => {
            round_price(entry_price + risk_per_unit * runner_target_r)
        }
        MarketVelocitySignalTradeDirection::Short => {
            round_price(entry_price - risk_per_unit * runner_target_r)
        }
    };
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
        .abs()
        .max(0.0)
        .min(10.0)
        * 0.005;
    let confidence = 0.55 + delta_component + price_component;
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
fn normalize_entry_trigger(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
fn normalize_symbol(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}
#[cfg(test)]
mod tests;
