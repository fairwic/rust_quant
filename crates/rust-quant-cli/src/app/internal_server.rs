use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use crypto_exc_all::ExchangeId;
use rust_quant_domain::Timeframe;
use rust_quant_infrastructure::repositories::{PostgresCandleRepository, SqlxCandleRepository};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::str::FromStr;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};
mod auth;
mod backtest_details;
mod backtest_logs;
mod http;
mod json_helpers;
mod market_rank_technical_context;
mod strategy_catalog;
mod strategy_configs;
use crate::app::exchange_symbol_sync::{
    run_exchange_symbol_sync_from_env, ExchangeSymbolSyncRequest,
};
use crate::app::market_velocity_event_backtest::market_velocity_paper_strategy_preset_manifest;
use auth::authorize_internal_request;
pub use backtest_details::{backtest_detail_list_query_from_path, BacktestDetailListQuery};
pub use backtest_logs::{
    backtest_log_list_query_from_path, core_backtest_run_list_query_from_path, BacktestLogListQuery,
};
pub use http::InternalHttpJsonResponse;
use http::{
    json_response, query_param, read_request, required_query_param, route_path, write_response,
};
use json_helpers::parse_json_value_or_string;
use market_rank_technical_context::{
    build_market_rank_technical_context, MarketRankTechnicalContext, MarketRankTechnicalSource,
};
use rust_quant_orchestration::infra::strategy_config::BackTestConfig;
use rust_quant_orchestration::workflow::backtest_runner;
use rust_quant_services::market::{should_use_quant_core_candle_source, CandleService};
use rust_quant_services::rust_quan_web::{run_account_snapshot_sync, AccountSnapshotSyncConfig};
pub use strategy_catalog::standard_strategy_catalog_items;
pub use strategy_configs::{
    strategy_config_list_query_from_path, strategy_config_risk_config_update_value,
    strategy_config_upsert_request_from_body, StrategyConfigListQuery, StrategyConfigUpsertRequest,
};
const DEFAULT_INTERNAL_ADDR: &str = "127.0.0.1:5322";
const MAX_BACKTEST_SIGNAL_LIMIT: i64 = 100;
const MAX_KLINE_LIMIT: i64 = 2_000;
const DEFAULT_KLINE_LIMIT: i64 = 500;
const DEFAULT_KLINE_EXCHANGE: &str = "binance";
const MAX_MARKET_RANK_EVENT_LIMIT: i64 = 200;
const DEFAULT_MARKET_RANK_EVENT_LIMIT: i64 = 50;
const DEFAULT_MARKET_RANK_EVENT_EXCHANGE: &str = "okx";
const DEFAULT_MARKET_RANK_EVENT_LOOKBACK_MINUTES: i64 = 120;
const MAX_MARKET_RANK_EVENT_LOOKBACK_MINUTES: i64 = 1_440;
const MARKET_RANK_TOP_BOUNDARY: i32 = 50;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestBacktestQuery {
    /// 策略运行键。
    pub strategy_key: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 周期。
    pub timeframe: String,
    /// 查询数量上限。
    pub limit: i64,
    /// include信号载荷，用于当前结构体的业务数据。
    pub include_signal_payload: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketKlineQuery {
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 周期。
    pub timeframe: String,
    /// 查询数量上限。
    pub limit: i64,
    /// 查询结束边界；为空时不限制结束时间。
    pub before: Option<i64>,
    /// 查询开始边界；为空时不限制开始时间。
    pub after: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRankEventsQuery {
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: Option<String>,
    /// 类型标识。
    pub event_type: Option<String>,
    /// 时间周期；为空时使用默认周期。
    pub timeframe: Option<String>,
    /// 排序字段；为空时使用默认排序。
    pub sort: Option<String>,
    /// 查询数量上限。
    pub limit: i64,
    /// lookback 分钟数。
    pub lookback_minutes: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlineSyncRequest {
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 周期。
    pub timeframe: String,
    /// 查询数量上限。
    pub limit: i64,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct LatestBacktestLogRow {
    /// 唯一标识。
    id: i32,
    /// 类型标识。
    strategy_type: String,
    /// 类型标识。
    inst_type: String,
    /// 时间字段。
    time: String,
    /// 金额数值。
    final_fund: f64,
    /// 收益值；为空时表示没有收益数据。
    profit: Option<f64>,
    /// 胜率。
    win_rate: String,
    /// 未平仓仓位数量。
    open_positions_num: i32,
    /// onebarafterwin 费率；为空时使用默认值或表示不限制。
    one_bar_after_win_rate: Option<f64>,
    /// twobarafterwin 费率；为空时使用默认值或表示不限制。
    two_bar_after_win_rate: Option<f64>,
    /// threebarafterwin 费率；为空时使用默认值或表示不限制。
    three_bar_after_win_rate: Option<f64>,
    /// fourbarafterwin 费率；为空时使用默认值或表示不限制。
    four_bar_after_win_rate: Option<f64>,
    /// fivebarafterwin 费率；为空时使用默认值或表示不限制。
    five_bar_after_win_rate: Option<f64>,
    /// tenbarafterwin 费率；为空时使用默认值或表示不限制。
    ten_bar_after_win_rate: Option<f64>,
    /// 开始时间。
    kline_start_time: i64,
    /// 结束时间。
    kline_end_time: i64,
    /// klinenums，用于展示或持久化查询结果。
    kline_nums: i32,
    /// Sharpe 比率；为空时使用默认值或表示不限制。
    sharpe_ratio: Option<f64>,
    /// 年化收益率；为空时表示样本不足。
    annual_return: Option<f64>,
    /// 总收益率；为空时表示样本不足。
    total_return: Option<f64>,
    /// 最大回撤；为空时使用默认值或表示不限制。
    max_drawdown: Option<f64>,
    /// 波动率；为空时表示样本不足。
    volatility: Option<f64>,
    /// 策略详情，用于展示或持久化查询结果。
    strategy_detail: String,
    /// 风险配置详情，用于展示或持久化查询结果。
    risk_config_detail: String,
    /// 创建时间。
    created_at: NaiveDateTime,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct LatestBacktestSignalRow {
    /// 唯一标识。
    id: i32,
    /// backtest ID。
    back_test_id: i32,
    /// 时间字段。
    time: String,
    /// 类型标识。
    option_type: String,
    /// 类型标识。
    close_type: String,
    /// 开仓时间。
    open_position_time: NaiveDateTime,
    /// 平仓时间。
    close_position_time: NaiveDateTime,
    /// 价格数值。
    open_price: String,
    /// 离场价格。
    close_price: Option<String>,
    /// 收益亏损，用于展示或持久化查询结果。
    profit_loss: String,
    /// 数量。
    quantity: String,
    /// 信号值，用于展示或持久化查询结果。
    signal_value: String,
    /// 信号结果；为空时使用默认值或表示不限制。
    signal_result: Option<String>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct MarketKlineItem {
    /// 时间字段。
    time: i64,
    /// 开盘价。
    open: f64,
    /// 最高价。
    high: f64,
    /// 最低价。
    low: f64,
    /// 收盘价。
    close: f64,
    /// 成交量。
    volume: f64,
    /// 时区设置。
    timezone: String,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct MarketRankEventItem {
    /// 唯一标识。
    id: i64,
    /// 交易所名称。
    exchange: String,
    /// 交易对或资产符号。
    symbol: String,
    /// 类型标识。
    event_type: String,
    /// 时间周期；为空时使用默认周期。
    timeframe: Option<String>,
    /// 旧排名；为空时表示没有上一期排名。
    old_rank: Option<i32>,
    /// 新排名；为空时表示没有当前排名。
    new_rank: Option<i32>,
    /// 排名变化值；为空时表示无法计算排名变化。
    delta_rank: Option<i32>,
    /// 排名变化百分比。
    rank_change_pct: Option<f64>,
    /// 24 小时计价成交额；为空时表示没有成交额。
    volume_24h_quote: Option<f64>,
    /// 上一期 24 小时计价成交额；为空时表示没有对比基准。
    previous_volume_24h_quote: Option<f64>,
    /// 24 小时成交量变化百分比。
    volume_24h_change_pct: Option<f64>,
    /// 15 分钟计价成交额；为空时表示没有成交额。
    volume_15m_quote: Option<f64>,
    /// 15 分钟成交量变化百分比。
    volume_15m_change_pct: Option<f64>,
    /// 价格数值。
    current_price: Option<f64>,
    /// 价格数值。
    previous_price: Option<f64>,
    /// 价格涨跌幅百分比。
    price_change_pct: Option<f64>,
    /// 价格方向。
    price_direction: String,
    /// 24 小时价格涨跌幅百分比。
    price_change_24h_pct: Option<f64>,
    #[serde(skip_serializing)]
    /// 技术指标周期；为空时使用默认周期。
    technical_timeframe: Option<String>,
    #[serde(skip_serializing)]
    /// 技术指标计算周期；为空时使用默认周期。
    technical_period: Option<i32>,
    #[serde(skip_serializing)]
    /// 离场价格。
    technical_close_price: Option<f64>,
    #[serde(skip_serializing)]
    /// 技术 MA 指标值；为空时表示未计算。
    technical_ma_value: Option<f64>,
    #[serde(skip_serializing)]
    /// 技术 EMA 指标值；为空时表示未计算。
    technical_ema_value: Option<f64>,
    #[serde(skip_serializing)]
    /// 价格相对 MA 的距离百分比。
    technical_ma_distance_pct: Option<f64>,
    #[serde(skip_serializing)]
    /// 价格相对 EMA 的距离百分比。
    technical_ema_distance_pct: Option<f64>,
    #[serde(skip_serializing)]
    /// 状态值。
    technical_ma_state: Option<String>,
    #[serde(skip_serializing)]
    /// 状态值。
    technical_ema_state: Option<String>,
    #[serde(skip_serializing)]
    /// 技术上下文使用的 K 线数量。
    technical_candle_count: Option<i32>,
    #[serde(skip_serializing)]
    /// 时间字段。
    technical_snapshot_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    /// 状态值。
    technical_snapshot_status: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 技术上下文；为空时不附加技术解释。
    technical_context: Option<MarketRankTechnicalContext>,
    /// 时间字段。
    detected_at: DateTime<Utc>,
    /// 数据来源。
    source: String,
    /// 状态值。
    notification_state: String,
}
#[derive(Debug, Clone, sqlx::FromRow)]
struct CandleVolume15mStats {
    /// 15 分钟计价成交额；为空时表示没有成交额。
    volume_15m_quote: Option<f64>,
    /// 15 分钟成交量变化百分比。
    volume_15m_change_pct: Option<f64>,
}
#[derive(Debug, Serialize)]
struct LatestBacktestSummary {
    /// hasbacktest。
    has_backtest: bool,
    /// backtestlog ID；为空时使用默认值或表示不限制。
    back_test_log_id: Option<i32>,
    /// 类型标识。
    strategy_type: Option<String>,
    /// 类型标识。
    inst_type: Option<String>,
    /// 时间字段。
    time: Option<String>,
    /// 金额数值。
    final_fund: Option<f64>,
    /// 收益值；为空时表示没有收益数据。
    profit: Option<f64>,
    /// 胜率；为空时使用默认值或表示不限制。
    win_rate: Option<String>,
    /// 未平仓仓位数量。
    open_positions_num: Option<i32>,
    /// onebarafterwin 费率；为空时使用默认值或表示不限制。
    one_bar_after_win_rate: Option<f64>,
    /// twobarafterwin 费率；为空时使用默认值或表示不限制。
    two_bar_after_win_rate: Option<f64>,
    /// threebarafterwin 费率；为空时使用默认值或表示不限制。
    three_bar_after_win_rate: Option<f64>,
    /// fourbarafterwin 费率；为空时使用默认值或表示不限制。
    four_bar_after_win_rate: Option<f64>,
    /// fivebarafterwin 费率；为空时使用默认值或表示不限制。
    five_bar_after_win_rate: Option<f64>,
    /// tenbarafterwin 费率；为空时使用默认值或表示不限制。
    ten_bar_after_win_rate: Option<f64>,
    /// 开始时间。
    kline_start_time: Option<i64>,
    /// 结束时间。
    kline_end_time: Option<i64>,
    /// K 线数量；为空时使用默认数量。
    kline_nums: Option<i32>,
    /// Sharpe 比率；为空时使用默认值或表示不限制。
    sharpe_ratio: Option<f64>,
    /// 年化收益率；为空时表示样本不足。
    annual_return: Option<f64>,
    /// 总收益率；为空时表示样本不足。
    total_return: Option<f64>,
    /// 最大回撤；为空时使用默认值或表示不限制。
    max_drawdown: Option<f64>,
    /// 波动率；为空时表示样本不足。
    volatility: Option<f64>,
    /// 创建时间。
    created_at: Option<NaiveDateTime>,
}
#[derive(Debug, Serialize)]
struct LatestBacktestSignalItem {
    /// 唯一标识。
    id: i32,
    /// backtest ID。
    back_test_id: i32,
    /// 时间字段。
    time: String,
    /// 类型标识。
    option_type: String,
    /// 类型标识。
    close_type: String,
    /// 开仓时间。
    open_position_time: NaiveDateTime,
    /// 平仓时间。
    close_position_time: NaiveDateTime,
    /// 价格数值。
    open_price: String,
    /// 离场价格。
    close_price: Option<String>,
    /// 收益亏损，用于记录新闻或情报分析结果。
    profit_loss: String,
    /// 数量。
    quantity: String,
    /// 信号值，用于记录新闻或情报分析结果。
    signal_value: Value,
    /// 信号结果；为空时使用默认值或表示不限制。
    signal_result: Option<String>,
}
#[derive(Debug, Serialize)]
struct LatestBacktestResponse {
    /// 摘要。
    summary: LatestBacktestSummary,
    /// 策略详情，用于返回接口响应。
    strategy_detail: Value,
    /// 风险配置详情，用于返回接口响应。
    risk_config_detail: Value,
    /// 列表数据。
    signals: Vec<LatestBacktestSignalItem>,
    /// 信号总数。
    signal_total: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BacktestRunRequest {
    #[serde(default)]
    /// 策略config ID；为空时使用默认值或表示不限制。
    strategy_config_id: Option<String>,
    #[serde(default)]
    /// 策略Key，用于构建接口请求。
    strategy_key: String,
    #[serde(default)]
    /// 交易对或资产符号。
    symbol: String,
    #[serde(default)]
    /// 周期。
    timeframe: String,
    #[serde(alias = "config", default)]
    /// 配置overrides，用于构建接口请求。
    config_overrides: Value,
    #[serde(default)]
    /// Dry-runrun，用于构建接口请求。
    dry_run: bool,
}
#[derive(Debug, Deserialize)]
struct RawKlineSyncRequest {
    #[serde(default)]
    /// 交易所名称。
    exchange: Option<String>,
    /// 交易对或资产符号。
    symbol: String,
    #[serde(alias = "interval", alias = "period")]
    /// 周期。
    timeframe: String,
    #[serde(default)]
    /// 查询数量上限。
    limit: Option<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExchangeAccountSnapshotSyncRequest {
    /// 买家邮箱。
    buyer_email: String,
    /// 交易所名称。
    exchange: String,
    #[serde(default)]
    /// API 凭证 ID。
    credential_id: Option<i64>,
    #[serde(default)]
    /// 列表数据。
    combos: Vec<RawExchangeAccountSnapshotSyncCombo>,
    #[serde(default)]
    /// 是否按账户全量范围查询。
    account_wide: bool,
    #[serde(default)]
    /// 是否包含成交明细；为空时使用默认值。
    include_fills: Option<bool>,
    #[serde(default)]
    /// 是否输出对账报告；为空时使用默认值。
    report_reconciliation: Option<bool>,
    #[serde(default)]
    /// trigger来源；为空时使用默认值或表示不限制。
    trigger_source: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExchangeAccountSnapshotSyncCombo {
    /// combo ID。
    combo_id: i64,
    /// 交易对或资产符号。
    symbol: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExchangeAccountSnapshotSyncRequest {
    /// 买家邮箱。
    buyer_email: String,
    /// 交易所名称。
    exchange: ExchangeId,
    /// API 凭证 ID。
    credential_id: i64,
    /// 列表数据。
    combos: Vec<ExchangeAccountSnapshotSyncCombo>,
    /// 是否按账户全量范围查询。
    account_wide: bool,
    /// 是否包含成交明细。
    include_fills: bool,
    /// 是否生成对账报告。
    report_reconciliation: bool,
    /// 触发来源。
    trigger_source: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExchangeAccountSnapshotSyncCombo {
    /// combo ID。
    combo_id: i64,
    /// 交易对或资产符号。
    symbol: String,
}
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
pub async fn run_internal_server() -> Result<()> {
    let addr =
        std::env::var("QUANT_INTERNAL_ADDR").unwrap_or_else(|_| DEFAULT_INTERNAL_ADDR.to_string());
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("绑定 rust_quant internal server 失败: {addr}"))?;
    info!(addr = %addr, "rust_quant internal server started");
    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream).await {
                error!(peer = %peer, error = %err, "处理 internal request 失败");
            }
        });
    }
}
/// 提供最新回测查询from路径的集中实现，避免量化核心调用方重复处理相同细节。
pub fn latest_backtest_query_from_path(path: &str) -> Result<LatestBacktestQuery, String> {
    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let strategy_key = required_query_param(query, &["strategyKey", "strategy_key"])?
        .trim()
        .to_ascii_lowercase();
    let symbol = required_query_param(query, &["symbol"])?
        .trim()
        .to_ascii_uppercase();
    let timeframe = required_query_param(query, &["timeframe", "period"])?
        .trim()
        .to_ascii_uppercase();
    let limit = query_param(query, &["limit"])
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, MAX_BACKTEST_SIGNAL_LIMIT);
    let include_signal_payload =
        query_param(query, &["includeSignalPayload", "include_signal_payload"])
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
            .unwrap_or(false);
    if strategy_key.is_empty() {
        return Err("strategyKey is required".to_string());
    }
    if symbol.is_empty() {
        return Err("symbol is required".to_string());
    }
    if timeframe.is_empty() {
        return Err("timeframe is required".to_string());
    }
    Ok(LatestBacktestQuery {
        strategy_key,
        symbol,
        timeframe,
        limit,
        include_signal_payload,
    })
}
/// 提供市场K 线查询from路径的集中实现，避免量化核心调用方重复处理相同细节。
pub fn market_kline_query_from_path(path: &str) -> Result<MarketKlineQuery, String> {
    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let exchange = query_param(query, &["exchange"])
        .unwrap_or_else(|| DEFAULT_KLINE_EXCHANGE.to_string())
        .trim()
        .to_ascii_lowercase();
    let symbol = required_query_param(query, &["symbol"])?
        .trim()
        .to_ascii_uppercase();
    let timeframe = required_query_param(query, &["timeframe", "interval", "period"])?
        .trim()
        .to_ascii_uppercase();
    let limit = query_param(query, &["limit"])
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_KLINE_LIMIT)
        .clamp(1, MAX_KLINE_LIMIT);
    let before = query_param(query, &["before"]).and_then(|value| value.parse::<i64>().ok());
    let after = query_param(query, &["after"]).and_then(|value| value.parse::<i64>().ok());
    if exchange.is_empty() {
        return Err("exchange is required".to_string());
    }
    if symbol.is_empty() {
        return Err("symbol is required".to_string());
    }
    if timeframe.is_empty() {
        return Err("timeframe is required".to_string());
    }
    Ok(MarketKlineQuery {
        exchange,
        symbol,
        timeframe,
        limit,
        before,
        after,
    })
}
/// 提供市场rank事件查询from路径的集中实现，避免量化核心调用方重复处理相同细节。
pub fn market_rank_events_query_from_path(path: &str) -> Result<MarketRankEventsQuery, String> {
    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let exchange = query_param(query, &["exchange"])
        .unwrap_or_else(|| DEFAULT_MARKET_RANK_EVENT_EXCHANGE.to_string())
        .trim()
        .to_ascii_lowercase();
    let symbol = query_param(query, &["symbol"])
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty());
    let event_type = query_param(query, &["eventType", "event_type"])
        .map(|value| normalize_market_rank_event_type(&value))
        .transpose()?;
    let timeframe = query_param(query, &["timeframe", "period"])
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let sort = query_param(query, &["sort", "orderBy", "order_by"])
        .map(|value| normalize_market_rank_sort(&value))
        .transpose()?;
    let limit = query_param(query, &["limit"])
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_MARKET_RANK_EVENT_LIMIT)
        .clamp(1, MAX_MARKET_RANK_EVENT_LIMIT);
    let lookback_minutes = query_param(
        query,
        &[
            "lookbackMinutes",
            "lookback_minutes",
            "recentMinutes",
            "recent_minutes",
        ],
    )
    .and_then(|value| value.parse::<i64>().ok())
    .unwrap_or(DEFAULT_MARKET_RANK_EVENT_LOOKBACK_MINUTES)
    .clamp(1, MAX_MARKET_RANK_EVENT_LOOKBACK_MINUTES);
    if exchange.is_empty() {
        return Err("exchange is required".to_string());
    }
    Ok(MarketRankEventsQuery {
        exchange,
        symbol,
        event_type,
        timeframe,
        sort,
        limit,
        lookback_minutes,
    })
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_market_klines_path(path: &str) -> InternalHttpJsonResponse {
    let query = match market_kline_query_from_path(path) {
        Ok(query) => query,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    match fetch_market_klines_response(rust_quant_core::database::get_db_pool(), &query).await {
        Ok(items) => json_response(
            200,
            serde_json::to_value(items).unwrap_or_else(|err| {
                json!({
                    "error": format!("serialize market klines response failed: {err}")
                })
            }),
        ),
        Err(err) => json_response(500, json!({ "error": err.to_string() })),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_market_rank_events_path(path: &str) -> InternalHttpJsonResponse {
    let query = match market_rank_events_query_from_path(path) {
        Ok(query) => query,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    match fetch_market_rank_events_response(rust_quant_core::database::get_db_pool(), &query).await
    {
        Ok(items) => json_response(
            200,
            serde_json::to_value(items).unwrap_or_else(|err| {
                json!({
                    "error": format!("serialize market rank events response failed: {err}")
                })
            }),
        ),
        Err(err) => json_response(500, json!({ "error": err.to_string() })),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_strategy_config_list_path(path: &str) -> InternalHttpJsonResponse {
    let query = match strategy_config_list_query_from_path(path) {
        Ok(query) => query,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    let pool = match strategy_configs::create_quant_core_internal_pool() {
        Ok(pool) => pool,
        Err(error) => return json_response(500, json!({ "error": error.to_string() })),
    };
    match strategy_configs::fetch_strategy_config_list_response(&pool, &query).await {
        Ok((items, total)) => json_response(200, json!({ "items": items, "total": total })),
        Err(error) => json_response(500, json!({ "error": error.to_string() })),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_strategy_config_upsert_body(body: &[u8]) -> InternalHttpJsonResponse {
    let request = match strategy_config_upsert_request_from_body(body) {
        Ok(request) => request,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    let pool = match strategy_configs::create_quant_core_internal_pool() {
        Ok(pool) => pool,
        Err(error) => return json_response(500, json!({ "error": error.to_string() })),
    };
    match strategy_configs::upsert_strategy_config_response(&pool, &request).await {
        Ok(item) => json_response(200, item),
        Err(error) => json_response(500, json!({ "error": error.to_string() })),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_strategy_catalog_path() -> InternalHttpJsonResponse {
    let items = standard_strategy_catalog_items();
    let total = items.len();
    json_response(200, json!({ "items": items, "total": total }))
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_latest_backtest_path(path: &str) -> InternalHttpJsonResponse {
    let query = match latest_backtest_query_from_path(path) {
        Ok(query) => query,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    match fetch_latest_backtest_response(rust_quant_core::database::get_db_pool(), &query).await {
        Ok(response) => json_response(
            200,
            serde_json::to_value(response).unwrap_or_else(|err| {
                json!({
                    "summary": default_latest_backtest_summary(None),
                    "strategy_detail": Value::Null,
                    "risk_config_detail": Value::Null,
                    "signals": [],
                    "signal_total": 0,
                    "error": format!("serialize latest backtest response failed: {err}")
                })
            }),
        ),
        Err(err) => json_response(
            500,
            json!({
                "error": err.to_string()
            }),
        ),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_backtest_run_body(body: &[u8]) -> InternalHttpJsonResponse {
    let request = match serde_json::from_slice::<BacktestRunRequest>(body) {
        Ok(request) => request,
        Err(err) => {
            return json_response(
                400,
                json!({
                    "error": format!("invalid json body: {err}")
                }),
            );
        }
    };
    if let Err(message) = validate_backtest_request(&request) {
        return json_response(400, json!({ "error": message }));
    }
    if let Err(message) = validate_backtest_runtime_contract(&request) {
        return json_response(400, json!({ "error": message }));
    }
    let run_id = format!("rq-backtest-{}", Utc::now().timestamp_millis());
    if request.dry_run {
        return json_response(200, backtest_response_body(&run_id, "dry_run", &request));
    }
    let config = backtest_config_from_request(&request);
    let targets = vec![(request.symbol.clone(), request.timeframe.clone())];
    match backtest_runner::run_backtest_runner_with_config(&targets, config).await {
        Ok(()) => json_response(200, backtest_response_body(&run_id, "completed", &request)),
        Err(err) => json_response(
            500,
            json!({
                "runId": run_id,
                "status": "failed",
                "error": err.to_string(),
                "strategyKey": request.strategy_key,
                "symbol": request.symbol,
                "timeframe": request.timeframe,
                "dryRun": false
            }),
        ),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_kline_sync_body(body: &[u8]) -> InternalHttpJsonResponse {
    let request = match kline_sync_request_from_body(body) {
        Ok(request) => request,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    match sync_kline_request(&request).await {
        Ok(saved_count) => json_response(
            200,
            json!({
                "status": "completed",
                "exchange": request.exchange,
                "symbol": request.symbol,
                "timeframe": request.timeframe,
                "limit": request.limit,
                "savedCount": saved_count,
            }),
        ),
        Err(err) => json_response(
            500,
            json!({
                "status": "failed",
                "exchange": request.exchange,
                "symbol": request.symbol,
                "timeframe": request.timeframe,
                "limit": request.limit,
                "error": err.to_string()
            }),
        ),
    }
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_exchange_account_snapshot_sync_body(body: &[u8]) -> InternalHttpJsonResponse {
    let request = match exchange_account_snapshot_sync_request_from_body(body) {
        Ok(request) => request,
        Err(message) => return json_response(400, json!({ "error": message })),
    };
    let requested = request.combos.len() + usize::from(request.account_wide);
    let mut accepted = 0_usize;
    let mut skipped = 0_usize;
    if request.account_wide {
        accepted += 1;
        let config = AccountSnapshotSyncConfig {
            buyer_email: request.buyer_email.clone(),
            exchange: request.exchange,
            symbol: "ACCOUNT-WIDE".to_string(),
            combo_id: 0,
            task_id: 0,
            credential_id: Some(request.credential_id),
            credential_ref: Some(account_snapshot_sync_credential_ref(
                request.credential_id,
                &request.trigger_source,
                "account-wide",
            )),
            report_reconciliation: false,
            include_fills: request.include_fills,
            account_wide: true,
        };
        tokio::spawn(async move {
            match run_account_snapshot_sync(config).await {
                Ok(result) => {
                    info!(result = %result, "Core 账户级快照异步同步完成");
                }
                Err(err) => {
                    error!(error = %err, "Core 账户级快照异步同步失败");
                }
            }
        });
    }
    for combo in request.combos.clone() {
        if combo.symbol.trim().is_empty() || combo.combo_id <= 0 {
            skipped += 1;
            continue;
        }
        accepted += 1;
        let config = AccountSnapshotSyncConfig {
            buyer_email: request.buyer_email.clone(),
            exchange: request.exchange,
            symbol: combo.symbol,
            combo_id: combo.combo_id,
            task_id: combo.combo_id,
            credential_id: Some(request.credential_id),
            credential_ref: Some(account_snapshot_sync_credential_ref(
                request.credential_id,
                &request.trigger_source,
                &combo.combo_id.to_string(),
            )),
            report_reconciliation: request.report_reconciliation,
            include_fills: request.include_fills,
            account_wide: false,
        };
        tokio::spawn(async move {
            match run_account_snapshot_sync(config).await {
                Ok(result) => {
                    info!(result = %result, "Core 账户快照异步同步完成");
                }
                Err(err) => {
                    error!(error = %err, "Core 账户快照异步同步失败");
                }
            }
        });
    }
    json_response(
        202,
        json!({
            "status": "accepted",
            "buyer_email": request.buyer_email,
            "exchange": request.exchange.as_str(),
            "requested": requested,
            "accepted": accepted,
            "skipped": skipped,
            "account_wide": request.account_wide,
            "include_fills": request.include_fills,
            "report_reconciliation": request.report_reconciliation,
            "mutation_allowed": false,
            "place_order_allowed": false,
        }),
    )
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn handle_exchange_symbol_sync_body(body: &[u8]) -> InternalHttpJsonResponse {
    let request = if body.is_empty() {
        ExchangeSymbolSyncRequest {
            sources: None,
            trigger_source: Some("manual".to_string()),
            submit_signals: None,
        }
    } else {
        match serde_json::from_slice::<ExchangeSymbolSyncRequest>(body) {
            Ok(mut request) => {
                if request.trigger_source.is_none() {
                    request.trigger_source = Some("manual".to_string());
                }
                request
            }
            Err(err) => {
                return json_response(
                    400,
                    json!({
                        "error": format!("invalid json body: {err}")
                    }),
                );
            }
        }
    };
    match run_exchange_symbol_sync_from_env(request).await {
        Ok(response) => json_response(
            200,
            serde_json::to_value(response).unwrap_or_else(|err| {
                json!({
                    "status": "failed",
                    "error": format!("serialize exchange symbol sync response failed: {err}")
                })
            }),
        ),
        Err(err) => json_response(
            500,
            json!({
                "status": "failed",
                "error": err.to_string()
            }),
        ),
    }
}
/// 提供回测配置from请求体的集中实现，避免量化核心调用方重复处理相同细节。
pub fn backtest_config_from_body(body: &[u8]) -> Result<BackTestConfig, String> {
    let request = serde_json::from_slice::<BacktestRunRequest>(body)
        .map_err(|err| format!("invalid json body: {err}"))?;
    validate_backtest_request(&request).map_err(str::to_string)?;
    Ok(backtest_config_from_request(&request))
}
/// 提供K 线同步requestfrom请求体的集中实现，避免量化核心调用方重复处理相同细节。
pub fn kline_sync_request_from_body(body: &[u8]) -> Result<KlineSyncRequest, String> {
    let request = serde_json::from_slice::<RawKlineSyncRequest>(body)
        .map_err(|err| format!("invalid json body: {err}"))?;
    let exchange = request
        .exchange
        .unwrap_or_else(|| DEFAULT_KLINE_EXCHANGE.to_string())
        .trim()
        .to_ascii_lowercase();
    let symbol = request.symbol.trim().to_ascii_uppercase();
    let timeframe = normalize_kline_sync_timeframe(&request.timeframe)?;
    let limit = request
        .limit
        .unwrap_or(DEFAULT_KLINE_LIMIT)
        .clamp(1, MAX_KLINE_LIMIT);
    if exchange.is_empty() {
        return Err("exchange is required".to_string());
    }
    if symbol.is_empty() {
        return Err("symbol is required".to_string());
    }
    Ok(KlineSyncRequest {
        exchange,
        symbol,
        timeframe,
        limit,
    })
}
/// 提供交易所account快照同步requestfrom请求体的集中实现，避免量化核心调用方重复处理相同细节。
fn exchange_account_snapshot_sync_request_from_body(
    body: &[u8],
) -> Result<ExchangeAccountSnapshotSyncRequest, String> {
    let request = serde_json::from_slice::<RawExchangeAccountSnapshotSyncRequest>(body)
        .map_err(|err| format!("invalid json body: {err}"))?;
    let buyer_email = request.buyer_email.trim().to_string();
    if buyer_email.is_empty() {
        return Err("buyer_email is required".to_string());
    }
    let exchange = ExchangeId::from_str(request.exchange.trim())
        .map_err(|err| format!("unsupported exchange: {err}"))?;
    let credential_id = match request.credential_id {
        None => return Err("credential_id is required".to_string()),
        Some(id) if id <= 0 => return Err("credential_id must be a positive integer".to_string()),
        Some(id) => id,
    };
    if request.combos.is_empty() && !request.account_wide {
        return Err("combos must include at least one item".to_string());
    }
    let combos = request
        .combos
        .into_iter()
        .map(|combo| {
            let symbol = combo.symbol.trim().to_ascii_uppercase();
            if combo.combo_id <= 0 {
                return Err("combo_id must be a positive integer".to_string());
            }
            if symbol.is_empty() {
                return Err("symbol is required".to_string());
            }
            Ok(ExchangeAccountSnapshotSyncCombo {
                combo_id: combo.combo_id,
                symbol,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExchangeAccountSnapshotSyncRequest {
        buyer_email,
        exchange,
        credential_id,
        combos,
        account_wide: request.account_wide,
        include_fills: request.include_fills.unwrap_or(true),
        report_reconciliation: request.report_reconciliation.unwrap_or(false),
        trigger_source: request
            .trigger_source
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "web_api_credential_ready".to_string()),
    })
}
fn account_snapshot_sync_credential_ref(
    credential_id: i64,
    _trigger_source: &str,
    _suffix: &str,
) -> String {
    format!("web_api_credential_id_{credential_id}")
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let request = read_request(&mut stream).await?;
    let route = route_path(&request.path);
    if let Some(response) =
        authorize_internal_request(request.method.as_str(), route, &request.headers)
    {
        return write_response(&mut stream, response).await;
    }
    let response = match (request.method.as_str(), route) {
        ("POST", "/internal/backtests/run") | ("POST", "/api/internal/backtests/run") => {
            handle_backtest_run_body(&request.body).await
        }
        ("GET", "/internal/backtests/latest") | ("GET", "/api/internal/backtests/latest") => {
            handle_latest_backtest_path(&request.path).await
        }
        ("GET", "/api/internal/backtests/logs") => {
            backtest_logs::handle_backtest_log_list_path(&request.path).await
        }
        ("GET", "/api/internal/backtests/details") => {
            backtest_details::handle_backtest_detail_list_path(&request.path).await
        }
        ("GET", "/api/internal/core/backtest-runs") => {
            backtest_logs::handle_core_backtest_run_list_path(&request.path).await
        }
        ("GET", "/internal/klines") | ("GET", "/api/internal/klines") => {
            handle_market_klines_path(&request.path).await
        }
        ("POST", "/internal/klines/sync") | ("POST", "/api/internal/klines/sync") => {
            handle_kline_sync_body(&request.body).await
        }
        ("POST", "/internal/exchange-account-snapshots/sync")
        | ("POST", "/api/internal/exchange-account-snapshots/sync") => {
            handle_exchange_account_snapshot_sync_body(&request.body).await
        }
        ("GET", "/internal/market-rank-events") | ("GET", "/api/internal/market-rank-events") => {
            handle_market_rank_events_path(&request.path).await
        }
        ("GET", "/api/internal/strategy-configs") => {
            handle_strategy_config_list_path(&request.path).await
        }
        ("GET", "/api/internal/strategy-catalog") => handle_strategy_catalog_path().await,
        ("GET", "/api/internal/market-velocity/paper-strategy-preset-manifest") => {
            handle_market_velocity_paper_strategy_preset_manifest_path(&request.path).await
        }
        ("POST", "/api/internal/strategy-configs") => {
            handle_strategy_config_upsert_body(&request.body).await
        }
        ("POST", "/internal/exchange-symbols/sync")
        | ("POST", "/api/internal/exchange-symbols/sync") => {
            handle_exchange_symbol_sync_body(&request.body).await
        }
        ("GET", "/internal/health") | ("GET", "/api/internal/health") => {
            json_response(200, json!({ "status": "ok" }))
        }
        ("POST", _) => json_response(404, json!({ "error": "not found" })),
        _ => json_response(405, json!({ "error": "method not allowed" })),
    };
    write_response(&mut stream, response).await
}
async fn handle_market_velocity_paper_strategy_preset_manifest_path(
    path: &str,
) -> InternalHttpJsonResponse {
    let query = path.split_once('?').map(|(_, query)| query).unwrap_or("");
    let preset = match required_query_param(query, &["preset", "paperStrategyPreset"]) {
        Ok(value) => value,
        Err(error) => return json_response(400, json!({ "error": error })),
    };
    match market_velocity_paper_strategy_preset_manifest(&preset) {
        Ok(manifest) => json_response(
            200,
            json!({
                "productSlug": manifest.product_slug,
                "symbol": manifest.symbol,
                "channel": manifest.channel,
                "manifestHash": manifest.manifest_hash,
                "strategyKey": manifest.strategy_key,
                "humanLabel": manifest.human_label,
                "riskLevel": manifest.risk_level,
                "manifestStatus": manifest.manifest_status,
                "manifestJson": manifest.manifest_json,
                "canonicalJson": manifest.canonical_json,
            }),
        ),
        Err(error) => json_response(400, json!({ "error": error.to_string() })),
    }
}
include!("internal_server/kline_sync_section.rs");
include!("internal_server/market_read_models_section.rs");
include!("internal_server/latest_backtest_section.rs");
#[cfg(test)]
#[path = "internal_server_tests.rs"]
mod tests;
