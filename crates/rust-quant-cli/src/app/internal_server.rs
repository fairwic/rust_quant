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
mod strategy_configs;

pub use backtest_details::{backtest_detail_list_query_from_path, BacktestDetailListQuery};
pub use backtest_logs::{
    backtest_log_list_query_from_path, core_backtest_run_list_query_from_path, BacktestLogListQuery,
};
pub use http::InternalHttpJsonResponse;
pub use strategy_configs::{
    strategy_config_list_query_from_path, strategy_config_upsert_request_from_body,
    StrategyConfigListQuery, StrategyConfigUpsertRequest,
};

use auth::authorize_internal_request;
use http::{
    json_response, query_param, read_request, required_query_param, route_path, write_response,
};
use json_helpers::parse_json_value_or_string;
use market_rank_technical_context::{
    build_market_rank_technical_context, MarketRankTechnicalContext, MarketRankTechnicalSource,
};

use crate::app::exchange_symbol_sync::{
    run_exchange_symbol_sync_from_env, ExchangeSymbolSyncRequest,
};
use rust_quant_orchestration::infra::strategy_config::BackTestConfig;
use rust_quant_orchestration::workflow::backtest_runner;
use rust_quant_services::market::{should_use_quant_core_candle_source, CandleService};
use rust_quant_services::rust_quan_web::{run_account_snapshot_sync, AccountSnapshotSyncConfig};

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
    pub strategy_key: String,
    pub symbol: String,
    pub timeframe: String,
    pub limit: i64,
    pub include_signal_payload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketKlineQuery {
    pub exchange: String,
    pub symbol: String,
    pub timeframe: String,
    pub limit: i64,
    pub before: Option<i64>,
    pub after: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRankEventsQuery {
    pub exchange: String,
    pub symbol: Option<String>,
    pub event_type: Option<String>,
    pub timeframe: Option<String>,
    pub sort: Option<String>,
    pub limit: i64,
    pub lookback_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlineSyncRequest {
    pub exchange: String,
    pub symbol: String,
    pub timeframe: String,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct LatestBacktestLogRow {
    id: i32,
    strategy_type: String,
    inst_type: String,
    time: String,
    final_fund: f64,
    profit: Option<f64>,
    win_rate: String,
    open_positions_num: i32,
    one_bar_after_win_rate: Option<f64>,
    two_bar_after_win_rate: Option<f64>,
    three_bar_after_win_rate: Option<f64>,
    four_bar_after_win_rate: Option<f64>,
    five_bar_after_win_rate: Option<f64>,
    ten_bar_after_win_rate: Option<f64>,
    kline_start_time: i64,
    kline_end_time: i64,
    kline_nums: i32,
    sharpe_ratio: Option<f64>,
    annual_return: Option<f64>,
    total_return: Option<f64>,
    max_drawdown: Option<f64>,
    volatility: Option<f64>,
    strategy_detail: String,
    risk_config_detail: String,
    created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct LatestBacktestSignalRow {
    id: i32,
    back_test_id: i32,
    time: String,
    option_type: String,
    close_type: String,
    open_position_time: NaiveDateTime,
    close_position_time: NaiveDateTime,
    open_price: String,
    close_price: Option<String>,
    profit_loss: String,
    quantity: String,
    signal_value: String,
    signal_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct MarketKlineItem {
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    timezone: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct MarketRankEventItem {
    id: i64,
    exchange: String,
    symbol: String,
    event_type: String,
    timeframe: Option<String>,
    old_rank: Option<i32>,
    new_rank: Option<i32>,
    delta_rank: Option<i32>,
    rank_change_pct: Option<f64>,
    volume_24h_quote: Option<f64>,
    previous_volume_24h_quote: Option<f64>,
    volume_24h_change_pct: Option<f64>,
    volume_15m_quote: Option<f64>,
    volume_15m_change_pct: Option<f64>,
    current_price: Option<f64>,
    previous_price: Option<f64>,
    price_change_pct: Option<f64>,
    price_direction: String,
    price_change_24h_pct: Option<f64>,
    #[serde(skip_serializing)]
    technical_timeframe: Option<String>,
    #[serde(skip_serializing)]
    technical_period: Option<i32>,
    #[serde(skip_serializing)]
    technical_close_price: Option<f64>,
    #[serde(skip_serializing)]
    technical_ma_value: Option<f64>,
    #[serde(skip_serializing)]
    technical_ema_value: Option<f64>,
    #[serde(skip_serializing)]
    technical_ma_distance_pct: Option<f64>,
    #[serde(skip_serializing)]
    technical_ema_distance_pct: Option<f64>,
    #[serde(skip_serializing)]
    technical_ma_state: Option<String>,
    #[serde(skip_serializing)]
    technical_ema_state: Option<String>,
    #[serde(skip_serializing)]
    technical_candle_count: Option<i32>,
    #[serde(skip_serializing)]
    technical_snapshot_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    technical_snapshot_status: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    technical_context: Option<MarketRankTechnicalContext>,
    detected_at: DateTime<Utc>,
    source: String,
    notification_state: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CandleVolume15mStats {
    volume_15m_quote: Option<f64>,
    volume_15m_change_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
struct LatestBacktestSummary {
    has_backtest: bool,
    back_test_log_id: Option<i32>,
    strategy_type: Option<String>,
    inst_type: Option<String>,
    time: Option<String>,
    final_fund: Option<f64>,
    profit: Option<f64>,
    win_rate: Option<String>,
    open_positions_num: Option<i32>,
    one_bar_after_win_rate: Option<f64>,
    two_bar_after_win_rate: Option<f64>,
    three_bar_after_win_rate: Option<f64>,
    four_bar_after_win_rate: Option<f64>,
    five_bar_after_win_rate: Option<f64>,
    ten_bar_after_win_rate: Option<f64>,
    kline_start_time: Option<i64>,
    kline_end_time: Option<i64>,
    kline_nums: Option<i32>,
    sharpe_ratio: Option<f64>,
    annual_return: Option<f64>,
    total_return: Option<f64>,
    max_drawdown: Option<f64>,
    volatility: Option<f64>,
    created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Serialize)]
struct LatestBacktestSignalItem {
    id: i32,
    back_test_id: i32,
    time: String,
    option_type: String,
    close_type: String,
    open_position_time: NaiveDateTime,
    close_position_time: NaiveDateTime,
    open_price: String,
    close_price: Option<String>,
    profit_loss: String,
    quantity: String,
    signal_value: Value,
    signal_result: Option<String>,
}

#[derive(Debug, Serialize)]
struct LatestBacktestResponse {
    summary: LatestBacktestSummary,
    strategy_detail: Value,
    risk_config_detail: Value,
    signals: Vec<LatestBacktestSignalItem>,
    signal_total: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BacktestRunRequest {
    #[serde(default)]
    strategy_config_id: Option<String>,
    #[serde(default)]
    strategy_key: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    timeframe: String,
    #[serde(alias = "config", default)]
    config_overrides: Value,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct RawKlineSyncRequest {
    #[serde(default)]
    exchange: Option<String>,
    symbol: String,
    #[serde(alias = "interval", alias = "period")]
    timeframe: String,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExchangeAccountSnapshotSyncRequest {
    buyer_email: String,
    exchange: String,
    #[serde(default)]
    combos: Vec<RawExchangeAccountSnapshotSyncCombo>,
    #[serde(default)]
    account_wide: bool,
    #[serde(default)]
    include_fills: Option<bool>,
    #[serde(default)]
    report_reconciliation: Option<bool>,
    #[serde(default)]
    trigger_source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExchangeAccountSnapshotSyncCombo {
    combo_id: i64,
    symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExchangeAccountSnapshotSyncRequest {
    buyer_email: String,
    exchange: ExchangeId,
    combos: Vec<ExchangeAccountSnapshotSyncCombo>,
    account_wide: bool,
    include_fills: bool,
    report_reconciliation: bool,
    trigger_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExchangeAccountSnapshotSyncCombo {
    combo_id: i64,
    symbol: String,
}

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
            credential_ref: Some(format!(
                "{}:account-wide",
                safe_internal_ref_component(&request.trigger_source)
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
            credential_ref: Some(format!(
                "{}:{}",
                safe_internal_ref_component(&request.trigger_source),
                combo.combo_id
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

pub fn backtest_config_from_body(body: &[u8]) -> Result<BackTestConfig, String> {
    let request = serde_json::from_slice::<BacktestRunRequest>(body)
        .map_err(|err| format!("invalid json body: {err}"))?;
    validate_backtest_request(&request).map_err(str::to_string)?;
    Ok(backtest_config_from_request(&request))
}

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

fn safe_internal_ref_component(raw: &str) -> String {
    let component: String = raw
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .take(48)
        .collect();
    if component.is_empty() {
        "web_api_credential_ready".to_string()
    } else {
        component
    }
}

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

include!("internal_server/kline_sync_section.rs");
include!("internal_server/market_read_models_section.rs");
include!("internal_server/latest_backtest_section.rs");
#[cfg(test)]
#[path = "internal_server_tests.rs"]
mod tests;
