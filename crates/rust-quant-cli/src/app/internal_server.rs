use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use rust_quant_domain::Timeframe;
use rust_quant_infrastructure::repositories::{PostgresCandleRepository, SqlxCandleRepository};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

mod market_rank_technical_context;

use market_rank_technical_context::{
    build_market_rank_technical_context, MarketRankTechnicalContext, MarketRankTechnicalSource,
};

use crate::app::exchange_symbol_sync::{
    run_exchange_symbol_sync_from_env, ExchangeSymbolSyncRequest,
};
use rust_quant_orchestration::infra::strategy_config::BackTestConfig;
use rust_quant_orchestration::workflow::backtest_runner;
use rust_quant_services::market::{should_use_quant_core_candle_source, CandleService};

const DEFAULT_INTERNAL_ADDR: &str = "127.0.0.1:5322";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
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

#[derive(Debug, Clone)]
pub struct InternalHttpJsonResponse {
    pub status_code: u16,
    pub body: Value,
}

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

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let request = read_request(&mut stream).await?;
    let route = route_path(&request.path);
    let response = match (request.method.as_str(), route) {
        ("POST", "/internal/backtests/run") => handle_backtest_run_body(&request.body).await,
        ("GET", "/internal/backtests/latest") => handle_latest_backtest_path(&request.path).await,
        ("GET", "/internal/klines") => handle_market_klines_path(&request.path).await,
        ("POST", "/internal/klines/sync") => handle_kline_sync_body(&request.body).await,
        ("GET", "/internal/market-rank-events") => {
            handle_market_rank_events_path(&request.path).await
        }
        ("POST", "/internal/exchange-symbols/sync") => {
            handle_exchange_symbol_sync_body(&request.body).await
        }
        ("GET", "/internal/health") => json_response(200, json!({ "status": "ok" })),
        ("POST", _) => json_response(404, json!({ "error": "not found" })),
        _ => json_response(405, json!({ "error": "method not allowed" })),
    };
    write_response(&mut stream, response).await
}

async fn sync_kline_request(request: &KlineSyncRequest) -> Result<i64> {
    let service = create_kline_sync_candle_service()?;
    let period = kline_sync_period_for_job(&request.timeframe)?;
    let timeframe = Timeframe::from_str(&period)
        .map_err(|error| anyhow::anyhow!("无效的K线周期: {}", error))?;
    let latest_candle = service
        .get_latest_candle(&request.symbol, timeframe)
        .await?;
    let after = latest_candle.and_then(|candle| {
        candle
            .timestamp
            .checked_add(1)
            .and_then(|timestamp| u64::try_from(timestamp).ok())
    });

    let candles = service
        .fetch_candles_from_crypto_exc_all(
            &request.exchange,
            &request.symbol,
            &period,
            after,
            None,
            request.limit as u32,
        )
        .await?;
    if candles.is_empty() {
        return Ok(0);
    }

    Ok(service.save_candles(candles).await? as i64)
}

fn create_kline_sync_candle_service() -> Result<CandleService> {
    if should_use_quant_core_candle_source()? {
        let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
            .context("CANDLE_SOURCE=quant_core 时必须设置 QUANT_CORE_DATABASE_URL")?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_lazy(&database_url)?;
        let repository = PostgresCandleRepository::new(pool);
        return Ok(CandleService::new(Box::new(repository)));
    }

    let pool = rust_quant_core::database::get_db_pool();
    let repository = SqlxCandleRepository::new(pool.clone());
    Ok(CandleService::new(Box::new(repository)))
}

async fn fetch_market_klines_response(
    pool: &PgPool,
    query: &MarketKlineQuery,
) -> Result<Vec<MarketKlineItem>> {
    let mut rows = fetch_unified_market_klines(pool, query).await?;
    if rows.is_empty() {
        rows = fetch_legacy_market_klines(pool, query).await?;
    }
    rows.sort_by_key(|item| item.time);
    Ok(rows)
}

async fn fetch_market_rank_events_response(
    pool: &PgPool,
    query: &MarketRankEventsQuery,
) -> Result<Vec<MarketRankEventItem>> {
    if market_rank_sort_requires_legacy_volume_before_limit(query.sort.as_deref()) {
        return fetch_volume_15m_market_rank_events_response(pool, query).await;
    }

    if market_rank_sort_can_use_recent_query(query.sort.as_deref()) {
        return fetch_recent_market_rank_events_response(pool, query).await;
    }

    let sql = r#"
        SELECT
            latest.id,
            latest.exchange,
            latest.symbol,
            latest.event_type,
            latest.timeframe,
            latest.old_rank,
            latest.new_rank,
            latest.delta_rank,
            CASE
                WHEN latest.old_rank IS NOT NULL
                     AND latest.old_rank > 0
                     AND latest.delta_rank IS NOT NULL
                THEN ABS(latest.delta_rank)::FLOAT8 / latest.old_rank::FLOAT8 * 100.0
                ELSE NULL
            END AS rank_change_pct,
            latest.volume_24h_quote::FLOAT8 AS volume_24h_quote,
            previous.previous_volume_24h_quote,
            CASE
                WHEN previous.previous_volume_24h_quote IS NOT NULL
                     AND previous.previous_volume_24h_quote > 0
                     AND latest.volume_24h_quote IS NOT NULL
                THEN (latest.volume_24h_quote::FLOAT8 - previous.previous_volume_24h_quote)
                     / previous.previous_volume_24h_quote * 100.0
                ELSE NULL
            END AS volume_24h_change_pct,
            NULL::FLOAT8 AS volume_15m_quote,
            NULL::FLOAT8 AS volume_15m_change_pct,
            latest.current_price::FLOAT8 AS current_price,
            latest.previous_price::FLOAT8 AS previous_price,
            latest.price_change_pct::FLOAT8 AS price_change_pct,
            latest.price_direction,
            latest.price_change_pct::FLOAT8 AS price_change_24h_pct,
            latest.technical_timeframe,
            latest.technical_period,
            latest.technical_close_price::FLOAT8 AS technical_close_price,
            latest.technical_ma_value::FLOAT8 AS technical_ma_value,
            latest.technical_ema_value::FLOAT8 AS technical_ema_value,
            latest.technical_ma_distance_pct::FLOAT8 AS technical_ma_distance_pct,
            latest.technical_ema_distance_pct::FLOAT8 AS technical_ema_distance_pct,
            latest.technical_ma_state,
            latest.technical_ema_state,
            latest.technical_candle_count,
            latest.technical_snapshot_at,
            latest.technical_snapshot_status,
            latest.detected_at,
            latest.source,
            latest.notification_state
        FROM (
            SELECT DISTINCT ON (UPPER(symbol))
                id,
                exchange,
                symbol,
                event_type,
                timeframe,
                old_rank,
                new_rank,
                delta_rank,
                volume_24h_quote,
                current_price,
                previous_price,
                price_change_pct,
                price_direction,
                technical_timeframe,
                technical_period,
                technical_close_price,
                technical_ma_value,
                technical_ema_value,
                technical_ma_distance_pct,
                technical_ema_distance_pct,
                technical_ma_state,
                technical_ema_state,
                technical_candle_count,
                technical_snapshot_at,
                technical_snapshot_status,
                detected_at,
                source,
                notification_state
            FROM market_rank_events
            WHERE LOWER(exchange) = LOWER($1)
              AND ($2::TEXT IS NULL OR UPPER(symbol) = UPPER($2))
              AND ($3::TEXT IS NULL OR event_type = $3)
              AND ($4::TEXT IS NULL OR LOWER(COALESCE(timeframe, '')) = LOWER($4))
              AND detected_at >= NOW() - ($5::INTEGER * INTERVAL '1 minute')
            ORDER BY UPPER(symbol), detected_at DESC, id DESC
        ) latest
        LEFT JOIN LATERAL (
            SELECT previous.volume_24h_quote::FLOAT8 AS previous_volume_24h_quote
            FROM market_rank_events previous
            WHERE previous.exchange = latest.exchange
              AND previous.symbol = latest.symbol
              AND previous.volume_24h_quote IS NOT NULL
              AND previous.detected_at <= latest.detected_at - INTERVAL '15 minutes'
              AND previous.detected_at >= latest.detected_at - INTERVAL '24 hours'
            ORDER BY previous.detected_at ASC, previous.id ASC
            LIMIT 1
        ) previous ON TRUE
        WHERE latest.new_rank <= 50 OR latest.old_rank <= 50
        "#;
    let result = sqlx::query_as::<_, MarketRankEventItem>(sql)
        .bind(&query.exchange)
        .bind(query.symbol.as_deref())
        .bind(query.event_type.as_deref())
        .bind(query.timeframe.as_deref())
        .bind(query.lookback_minutes as i32)
        .fetch_all(pool)
        .await;

    match result {
        Ok(mut rows) => {
            if market_rank_sort_requires_legacy_volume_before_limit(query.sort.as_deref()) {
                attach_legacy_volume_15m(pool, &mut rows).await?;
                return Ok(finalize_market_rank_rows(
                    rows,
                    query.sort.as_deref(),
                    query.limit,
                ));
            }

            let mut rows = finalize_market_rank_rows(rows, query.sort.as_deref(), query.limit);
            attach_legacy_volume_15m(pool, &mut rows).await?;
            Ok(rows)
        }
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

async fn fetch_volume_15m_market_rank_events_response(
    pool: &PgPool,
    query: &MarketRankEventsQuery,
) -> Result<Vec<MarketRankEventItem>> {
    let mut recent_query = query.clone();
    recent_query.sort = None;
    recent_query.limit = MAX_MARKET_RANK_EVENT_LIMIT;

    let mut rows = fetch_recent_market_rank_events_response(pool, &recent_query).await?;
    attach_legacy_volume_15m(pool, &mut rows).await?;
    Ok(finalize_market_rank_rows(
        rows,
        query.sort.as_deref(),
        query.limit,
    ))
}

async fn fetch_recent_market_rank_events_response(
    pool: &PgPool,
    query: &MarketRankEventsQuery,
) -> Result<Vec<MarketRankEventItem>> {
    let sql = recent_market_rank_events_sql(query.sort.as_deref());

    let result = sqlx::query_as::<_, MarketRankEventItem>(&sql)
        .bind(&query.exchange)
        .bind(query.symbol.as_deref())
        .bind(query.event_type.as_deref())
        .bind(query.timeframe.as_deref())
        .bind(query.lookback_minutes as i32)
        .bind(query.limit)
        .fetch_all(pool)
        .await;

    match result {
        Ok(rows) => Ok(finalize_market_rank_rows(
            rows,
            query.sort.as_deref(),
            query.limit,
        )),
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

fn recent_market_rank_events_sql(sort: Option<&str>) -> String {
    format!(
        r#"
        WITH latest AS (
            SELECT DISTINCT ON (UPPER(symbol))
                id,
                exchange,
                symbol,
                event_type,
                timeframe,
                old_rank,
                new_rank,
                delta_rank,
                volume_24h_quote,
                current_price,
                previous_price,
                price_change_pct,
                price_direction,
                technical_timeframe,
                technical_period,
                technical_close_price,
                technical_ma_value,
                technical_ema_value,
                technical_ma_distance_pct,
                technical_ema_distance_pct,
                technical_ma_state,
                technical_ema_state,
                technical_candle_count,
                technical_snapshot_at,
                technical_snapshot_status,
                detected_at,
                source,
                notification_state
            FROM market_rank_events
            WHERE LOWER(exchange) = LOWER($1)
              AND ($2::TEXT IS NULL OR UPPER(symbol) = UPPER($2))
              AND ($3::TEXT IS NULL OR event_type = $3)
              AND ($4::TEXT IS NULL OR LOWER(COALESCE(timeframe, '')) = LOWER($4))
              AND detected_at >= NOW() - ($5::INTEGER * INTERVAL '1 minute')
              AND (new_rank <= 50 OR old_rank <= 50)
            ORDER BY UPPER(symbol), detected_at DESC, id DESC
        )
        SELECT
            id,
            exchange,
            symbol,
            event_type,
            timeframe,
            old_rank,
            new_rank,
            delta_rank,
            CASE
                WHEN old_rank IS NOT NULL
                     AND old_rank > 0
                     AND delta_rank IS NOT NULL
                THEN ABS(delta_rank)::FLOAT8 / old_rank::FLOAT8 * 100.0
                ELSE NULL
            END AS rank_change_pct,
            volume_24h_quote::FLOAT8 AS volume_24h_quote,
            NULL::FLOAT8 AS previous_volume_24h_quote,
            NULL::FLOAT8 AS volume_24h_change_pct,
            NULL::FLOAT8 AS volume_15m_quote,
            NULL::FLOAT8 AS volume_15m_change_pct,
            current_price::FLOAT8 AS current_price,
            previous_price::FLOAT8 AS previous_price,
            price_change_pct::FLOAT8 AS price_change_pct,
            price_direction,
            price_change_pct::FLOAT8 AS price_change_24h_pct,
            technical_timeframe,
            technical_period,
            technical_close_price::FLOAT8 AS technical_close_price,
            technical_ma_value::FLOAT8 AS technical_ma_value,
            technical_ema_value::FLOAT8 AS technical_ema_value,
            technical_ma_distance_pct::FLOAT8 AS technical_ma_distance_pct,
            technical_ema_distance_pct::FLOAT8 AS technical_ema_distance_pct,
            technical_ma_state,
            technical_ema_state,
            technical_candle_count,
            technical_snapshot_at,
            technical_snapshot_status,
            detected_at,
            source,
            notification_state
        FROM latest
        ORDER BY {}
        LIMIT $6
        "#,
        market_rank_recent_order_clause(sort)
    )
}

fn market_rank_sort_can_use_recent_query(sort: Option<&str>) -> bool {
    matches!(sort, None | Some("detected_at") | Some("delta_rank"))
}

fn market_rank_recent_order_clause(sort: Option<&str>) -> &'static str {
    match sort {
        None | Some("delta_rank") => {
            "rank_change_pct DESC NULLS LAST, ABS(COALESCE(delta_rank, 0)) DESC, detected_at DESC, id DESC"
        }
        _ => "detected_at DESC, id DESC",
    }
}

fn market_rank_sort_requires_legacy_volume_before_limit(sort: Option<&str>) -> bool {
    matches!(sort, Some("volume_15m"))
}

async fn attach_legacy_volume_15m(pool: &PgPool, rows: &mut [MarketRankEventItem]) -> Result<()> {
    for row in rows {
        let stats = fetch_legacy_volume_15m_stats(pool, &row.symbol, row.detected_at).await?;
        row.volume_15m_quote = stats.as_ref().and_then(|item| item.volume_15m_quote);
        row.volume_15m_change_pct = stats.as_ref().and_then(|item| item.volume_15m_change_pct);
    }
    Ok(())
}

async fn fetch_legacy_volume_15m_stats(
    pool: &PgPool,
    symbol: &str,
    detected_at: DateTime<Utc>,
) -> Result<Option<CandleVolume15mStats>> {
    let table_name = PostgresCandleRepository::quoted_table_name(symbol, Timeframe::M15)?;
    let detected_at_millis = detected_at.timestamp_millis();
    let detected_at_secs = detected_at.timestamp();
    let sql = format!(
        r#"
        WITH latest AS (
            SELECT
                ts,
                NULLIF(vol_ccy, '')::FLOAT8 AS volume_15m_quote
            FROM {table_name}
            WHERE (
                ts > 10000000000
                AND ts > $1::BIGINT - 1800000
                AND ts <= $1::BIGINT
            ) OR (
                ts <= 10000000000
                AND ts > $2::BIGINT - 1800
                AND ts <= $2::BIGINT
            )
            ORDER BY ts DESC
            LIMIT 1
        ),
        baseline AS (
            SELECT AVG(NULLIF(c.vol_ccy, '')::FLOAT8) AS avg_volume_15m_quote
            FROM {table_name} c
            JOIN latest ON TRUE
            WHERE c.ts < latest.ts
              AND c.ts >= latest.ts - CASE WHEN latest.ts > 10000000000 THEN 7200000 ELSE 7200 END
        )
        SELECT
            latest.volume_15m_quote,
            CASE
                WHEN baseline.avg_volume_15m_quote IS NOT NULL
                     AND baseline.avg_volume_15m_quote > 0
                THEN (latest.volume_15m_quote - baseline.avg_volume_15m_quote)
                     / baseline.avg_volume_15m_quote * 100.0
                ELSE NULL
            END AS volume_15m_change_pct
        FROM latest
        LEFT JOIN baseline ON TRUE
        "#
    );

    let result = sqlx::query_as::<_, CandleVolume15mStats>(&sql)
        .bind(detected_at_millis)
        .bind(detected_at_secs)
        .fetch_optional(pool)
        .await;

    match result {
        Ok(stats) => Ok(stats),
        Err(err) if is_undefined_table_error(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn finalize_market_rank_rows(
    mut rows: Vec<MarketRankEventItem>,
    sort: Option<&str>,
    limit: i64,
) -> Vec<MarketRankEventItem> {
    for row in &mut rows {
        row.technical_context = build_market_rank_technical_context(MarketRankTechnicalSource {
            timeframe: row.technical_timeframe.as_deref(),
            period: row.technical_period,
            close_price: row.technical_close_price,
            ma_value: row.technical_ma_value,
            ema_value: row.technical_ema_value,
            ma_distance_pct: row.technical_ma_distance_pct,
            ema_distance_pct: row.technical_ema_distance_pct,
            ma_state: row.technical_ma_state.as_deref(),
            ema_state: row.technical_ema_state.as_deref(),
            candle_count: row.technical_candle_count,
            snapshot_at: row.technical_snapshot_at,
            snapshot_status: row.technical_snapshot_status.as_deref(),
        });
        if row.rank_change_pct.is_none() {
            row.rank_change_pct = compute_rank_change_pct(row.old_rank, row.delta_rank);
        }
        if row.volume_24h_change_pct.is_none() {
            row.volume_24h_change_pct =
                compute_change_pct(row.volume_24h_quote, row.previous_volume_24h_quote);
        }
    }
    rows.retain(is_market_rank_top_boundary_row);

    rows.sort_by(compare_market_rank_latest);

    let mut seen = HashSet::new();
    rows.retain(|row| seen.insert(row.symbol.trim().to_ascii_uppercase()));

    match sort {
        None | Some("delta_rank") => rows.sort_by(compare_market_rank_by_rank_change_pct),
        Some("volume_24h") => rows.sort_by(compare_market_rank_by_volume_24h_change_pct),
        Some("volume_15m") => rows.sort_by(compare_market_rank_by_volume_15m_change_pct),
        _ => rows.sort_by(compare_market_rank_latest),
    }

    rows.truncate(limit.max(0) as usize);
    rows
}

fn is_market_rank_top_boundary_row(row: &MarketRankEventItem) -> bool {
    rank_is_within_top_boundary(row.new_rank) || rank_is_within_top_boundary(row.old_rank)
}

fn rank_is_within_top_boundary(rank: Option<i32>) -> bool {
    rank.is_some_and(|value| value > 0 && value <= MARKET_RANK_TOP_BOUNDARY)
}

fn compute_rank_change_pct(old_rank: Option<i32>, delta_rank: Option<i32>) -> Option<f64> {
    let old_rank = old_rank?;
    let delta_rank = delta_rank?;
    if old_rank <= 0 {
        return None;
    }
    Some(delta_rank.abs() as f64 / old_rank as f64 * 100.0)
}

fn compute_change_pct(current: Option<f64>, previous: Option<f64>) -> Option<f64> {
    let current = current?;
    let previous = previous?;
    if !current.is_finite() || !previous.is_finite() || previous <= 0.0 {
        return None;
    }
    Some((current - previous) / previous * 100.0)
}

fn compare_market_rank_latest(left: &MarketRankEventItem, right: &MarketRankEventItem) -> Ordering {
    right
        .detected_at
        .cmp(&left.detected_at)
        .then_with(|| right.id.cmp(&left.id))
}

fn compare_market_rank_by_rank_change_pct(
    left: &MarketRankEventItem,
    right: &MarketRankEventItem,
) -> Ordering {
    compare_optional_f64_desc(left.rank_change_pct, right.rank_change_pct)
        .then_with(|| {
            i32::abs(right.delta_rank.unwrap_or(0)).cmp(&i32::abs(left.delta_rank.unwrap_or(0)))
        })
        .then_with(|| compare_market_rank_latest(left, right))
}

fn compare_market_rank_by_volume_24h_change_pct(
    left: &MarketRankEventItem,
    right: &MarketRankEventItem,
) -> Ordering {
    compare_optional_f64_abs_desc(left.volume_24h_change_pct, right.volume_24h_change_pct)
        .then_with(|| compare_market_rank_latest(left, right))
}

fn compare_market_rank_by_volume_15m_change_pct(
    left: &MarketRankEventItem,
    right: &MarketRankEventItem,
) -> Ordering {
    compare_optional_f64_abs_desc(left.volume_15m_change_pct, right.volume_15m_change_pct)
        .then_with(|| compare_market_rank_latest(left, right))
}

fn compare_optional_f64_desc(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (finite_f64(left), finite_f64(right)) {
        (Some(left), Some(right)) => right.partial_cmp(&left).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn finite_f64(value: Option<f64>) -> Option<f64> {
    value.filter(|item| item.is_finite())
}

fn compare_optional_f64_abs_desc(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (finite_f64(left), finite_f64(right)) {
        (Some(left), Some(right)) => right
            .abs()
            .partial_cmp(&left.abs())
            .unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

async fn fetch_unified_market_klines(
    pool: &PgPool,
    query: &MarketKlineQuery,
) -> Result<Vec<MarketKlineItem>> {
    let result = sqlx::query_as::<_, MarketKlineItem>(
        r#"
        SELECT
            EXTRACT(EPOCH FROM open_time)::BIGINT AS time,
            open_price::FLOAT8 AS open,
            high_price::FLOAT8 AS high,
            low_price::FLOAT8 AS low,
            close_price::FLOAT8 AS close,
            COALESCE(volume, quote_volume, 0)::FLOAT8 AS volume,
            'UTC+8'::TEXT AS timezone
        FROM market_candles
        WHERE LOWER(exchange) = LOWER($1)
          AND UPPER(symbol) = UPPER($2)
          AND UPPER(timeframe) = UPPER($3)
          AND ($4::BIGINT IS NULL OR EXTRACT(EPOCH FROM open_time)::BIGINT < $4)
          AND ($5::BIGINT IS NULL OR EXTRACT(EPOCH FROM open_time)::BIGINT > $5)
        ORDER BY open_time DESC
        LIMIT $6
        "#,
    )
    .bind(&query.exchange)
    .bind(&query.symbol)
    .bind(&query.timeframe)
    .bind(query.before)
    .bind(query.after)
    .bind(query.limit)
    .fetch_all(pool)
    .await;

    match result {
        Ok(rows) => Ok(rows),
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

async fn fetch_legacy_market_klines(
    pool: &PgPool,
    query: &MarketKlineQuery,
) -> Result<Vec<MarketKlineItem>> {
    let table_name = legacy_kline_table_name(&query.symbol, &query.timeframe)?;
    let mut query_builder = QueryBuilder::<Postgres>::new(format!(
        r#"
        SELECT
            CASE WHEN ts > 10000000000 THEN ts / 1000 ELSE ts END AS time,
            NULLIF(o, '')::FLOAT8 AS open,
            NULLIF(h, '')::FLOAT8 AS high,
            NULLIF(l, '')::FLOAT8 AS low,
            NULLIF(c, '')::FLOAT8 AS close,
            COALESCE(NULLIF(vol, ''), NULLIF(vol_ccy, ''), '0')::FLOAT8 AS volume,
            'UTC+8'::TEXT AS timezone
        FROM {table_name}
        WHERE 1=1
        "#
    ));

    if let Some(before) = query.before {
        query_builder
            .push(" AND ts < ")
            .push_bind(seconds_to_legacy_millis(before));
    }
    if let Some(after) = query.after {
        query_builder
            .push(" AND ts > ")
            .push_bind(seconds_to_legacy_millis(after));
    }
    query_builder
        .push(" ORDER BY ts DESC LIMIT ")
        .push_bind(query.limit);

    let result = query_builder
        .build_query_as::<MarketKlineItem>()
        .fetch_all(pool)
        .await;

    match result {
        Ok(rows) => Ok(rows),
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

fn legacy_kline_table_name(symbol: &str, timeframe: &str) -> Result<String> {
    let symbol = normalize_legacy_symbol(symbol)?;
    let timeframe = normalize_legacy_timeframe(timeframe)?;
    Ok(format!("\"{}_candles_{}\"", symbol, timeframe))
}

fn normalize_legacy_symbol(raw: &str) -> Result<String> {
    let upper = raw.trim().to_ascii_uppercase();
    let normalized = if upper.contains('-') {
        upper
    } else if upper.len() > 4 && upper.ends_with("USDT") {
        format!("{}-USDT-SWAP", &upper[..upper.len() - 4])
    } else {
        anyhow::bail!("unsupported symbol for legacy kline table: {raw}");
    };

    let lower = normalized.to_ascii_lowercase();
    if lower
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
    {
        Ok(lower)
    } else {
        anyhow::bail!("illegal legacy kline symbol: {raw}");
    }
}

fn normalize_legacy_timeframe(raw: &str) -> Result<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1m" => Ok("1m"),
        "3m" => Ok("3m"),
        "5m" => Ok("5m"),
        "15m" => Ok("15m"),
        "30m" => Ok("30m"),
        "1h" => Ok("1h"),
        "2h" => Ok("2h"),
        "4h" => Ok("4h"),
        "6h" => Ok("6h"),
        "12h" => Ok("12h"),
        "1d" | "1dutc" => Ok("1dutc"),
        "1w" => Ok("1w"),
        "1mn" | "1mnutc" => Ok("1m"),
        _ => anyhow::bail!("unsupported timeframe for legacy kline table: {raw}"),
    }
}

fn normalize_kline_sync_timeframe(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1m" => Ok("1M".to_string()),
        "3m" => Ok("3M".to_string()),
        "5m" => Ok("5M".to_string()),
        "15m" => Ok("15M".to_string()),
        "30m" => Ok("30M".to_string()),
        "1h" => Ok("1H".to_string()),
        "2h" => Ok("2H".to_string()),
        "4h" => Ok("4H".to_string()),
        "6h" => Ok("6H".to_string()),
        "12h" => Ok("12H".to_string()),
        "1d" | "1dutc" => Ok("1DUTC".to_string()),
        "1w" => Ok("1W".to_string()),
        other if other.is_empty() => Err("timeframe is required".to_string()),
        other => Err(format!("unsupported timeframe: {other}")),
    }
}

fn kline_sync_period_for_job(timeframe: &str) -> Result<String> {
    let period = match timeframe {
        "1M" => "1m",
        "3M" => "3m",
        "5M" => "5m",
        "15M" => "15m",
        "30M" => "30m",
        "1DUTC" => "1Dutc",
        value => value,
    };
    Ok(period.to_string())
}

fn seconds_to_legacy_millis(timestamp: i64) -> i64 {
    if timestamp > 10_000_000_000 {
        timestamp
    } else {
        timestamp.saturating_mul(1_000)
    }
}

fn normalize_market_rank_event_type(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "rank_velocity" | "rankvelocity" => Ok("rank_velocity".to_string()),
        "top_entry" | "topentry" => Ok("top_entry".to_string()),
        "top_exit" | "topexit" => Ok("top_exit".to_string()),
        other => Err(format!("unsupported eventType: {other}")),
    }
}

fn normalize_market_rank_sort(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase().replace(['-', '.'], "_");
    match normalized.as_str() {
        "" | "time" | "latest" | "detected_at" => Ok("detected_at".to_string()),
        "delta" | "delta_rank" | "rank_delta" | "rank_movement" | "volatility" => {
            Ok("delta_rank".to_string())
        }
        "volume_24h" | "volume24h" | "volume_24h_quote" => Ok("volume_24h".to_string()),
        "volume_15m" | "volume15m" | "volume_15m_quote" => Ok("volume_15m".to_string()),
        other => Err(format!("unsupported sort: {other}")),
    }
}

fn is_undefined_table_error(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| code == "42P01")
}

async fn fetch_latest_backtest_response(
    pool: &PgPool,
    query: &LatestBacktestQuery,
) -> Result<LatestBacktestResponse> {
    let Some(log) = fetch_latest_backtest_log(pool, query).await? else {
        return Ok(LatestBacktestResponse {
            summary: default_latest_backtest_summary(None),
            strategy_detail: Value::Null,
            risk_config_detail: Value::Null,
            signals: Vec::new(),
            signal_total: 0,
        });
    };

    let (signals, signal_total) = fetch_latest_backtest_signals(pool, log.id, query).await?;
    Ok(LatestBacktestResponse {
        summary: latest_backtest_summary_from_log(&log),
        strategy_detail: parse_json_value_or_string(&log.strategy_detail),
        risk_config_detail: parse_json_value_or_string(&log.risk_config_detail),
        signals,
        signal_total,
    })
}

async fn fetch_latest_backtest_log(
    pool: &PgPool,
    query: &LatestBacktestQuery,
) -> Result<Option<LatestBacktestLogRow>> {
    let row = sqlx::query_as::<_, LatestBacktestLogRow>(
        r#"
        SELECT
            log.id::INT4 AS id,
            log.strategy_type,
            log.inst_type,
            log.time,
            log.final_fund,
            log.profit,
            log.win_rate,
            log.open_positions_num,
            log.one_bar_after_win_rate,
            log.two_bar_after_win_rate,
            log.three_bar_after_win_rate,
            log.four_bar_after_win_rate,
            log.five_bar_after_win_rate,
            log.ten_bar_after_win_rate,
            log.kline_start_time,
            log.kline_end_time,
            log.kline_nums,
            log.sharpe_ratio,
            log.annual_return,
            log.total_return,
            log.max_drawdown,
            log.volatility,
            log.strategy_detail,
            log.risk_config_detail,
            log.created_at
        FROM back_test_log log
        WHERE LOWER(log.strategy_type) = $1
          AND UPPER(log.inst_type) = $2
          AND UPPER(log.time) = $3
          AND EXISTS (
              SELECT 1
              FROM back_test_detail detail
              WHERE detail.back_test_id = log.id
              LIMIT 1
          )
        ORDER BY log.created_at DESC, log.id DESC
        LIMIT 1
        "#,
    )
    .bind(&query.strategy_key)
    .bind(&query.symbol)
    .bind(&query.timeframe)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

async fn fetch_latest_backtest_signals(
    pool: &PgPool,
    back_test_id: i32,
    query: &LatestBacktestQuery,
) -> Result<(Vec<LatestBacktestSignalItem>, i64)> {
    let rows = sqlx::query_as::<_, LatestBacktestSignalRow>(
        r#"
        SELECT
            id::INT4 AS id,
            back_test_id::INT4 AS back_test_id,
            time,
            option_type,
            close_type,
            open_position_time,
            close_position_time,
            open_price,
            close_price,
            profit_loss,
            quantity,
            COALESCE(signal_value, '') AS signal_value,
            signal_result
        FROM back_test_detail
        WHERE back_test_id = $1
          AND LOWER(option_type) <> 'close'
        ORDER BY open_position_time DESC
        LIMIT $2
        "#,
    )
    .bind(back_test_id)
    .bind(query.limit)
    .fetch_all(pool)
    .await?;

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM back_test_detail WHERE back_test_id = $1 AND LOWER(option_type) <> 'close'",
    )
    .bind(back_test_id)
    .fetch_one(pool)
    .await?;

    let signals = rows
        .into_iter()
        .map(|row| LatestBacktestSignalItem {
            id: row.id,
            back_test_id: row.back_test_id,
            time: row.time,
            option_type: row.option_type,
            close_type: row.close_type,
            open_position_time: row.open_position_time,
            close_position_time: row.close_position_time,
            open_price: row.open_price,
            close_price: row.close_price,
            profit_loss: row.profit_loss,
            quantity: row.quantity,
            signal_value: if query.include_signal_payload {
                parse_json_value_or_string(&row.signal_value)
            } else {
                Value::Null
            },
            signal_result: if query.include_signal_payload {
                row.signal_result
            } else {
                None
            },
        })
        .collect::<Vec<_>>();

    Ok((signals, total.0))
}

fn validate_backtest_request(request: &BacktestRunRequest) -> Result<(), &'static str> {
    if request.strategy_key.trim().is_empty() {
        return Err("strategyKey is required");
    }
    if request.symbol.trim().is_empty() {
        return Err("symbol is required");
    }
    if request.timeframe.trim().is_empty() {
        return Err("timeframe is required");
    }
    Ok(())
}

fn validate_backtest_runtime_contract(request: &BacktestRunRequest) -> Result<(), String> {
    if request.dry_run {
        return Ok(());
    }

    let source = std::env::var("STRATEGY_CONFIG_SOURCE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !source.is_empty()
        && !matches!(
            source.as_str(),
            "quant_core" | "postgres" | "strategy_config" | "legacy_pg"
        )
    {
        return Err(format!(
            "STRATEGY_CONFIG_SOURCE={} is not supported for non-dry-run backtests",
            source
        ));
    }

    let quant_core_database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .unwrap_or_default()
        .trim()
        .to_string();
    if quant_core_database_url.is_empty() {
        return Err("QUANT_CORE_DATABASE_URL is required for non-dry-run backtests".to_string());
    }

    Ok(())
}

fn backtest_config_from_request(request: &BacktestRunRequest) -> BackTestConfig {
    let mut config = BackTestConfig::default();
    config.strategy_config_id = request
        .strategy_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(candle_limit) = read_usize_override(
        &request.config_overrides,
        &["kline_nums", "klineNums", "candle_limit", "candleLimit"],
    ) {
        config.candle_limit = candle_limit;
    }
    if let Some(max_concurrent) = read_usize_override(
        &request.config_overrides,
        &["max_concurrent", "maxConcurrent"],
    ) {
        config.max_concurrent = max_concurrent;
    }

    config.enable_random_test = false;
    config.enable_random_test_vegas = false;
    config.enable_specified_test_vegas = false;
    config.enable_random_test_nwe = false;
    config.enable_specified_test_nwe = false;

    if request.strategy_key.trim().eq_ignore_ascii_case("nwe") {
        config.enable_specified_test_nwe = true;
    } else {
        config.enable_specified_test_vegas = true;
    }
    config
}

fn read_usize_override(overrides: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .filter_map(|key| overrides.get(*key))
        .find_map(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn backtest_response_body(run_id: &str, status: &str, request: &BacktestRunRequest) -> Value {
    json!({
        "runId": run_id,
        "status": status,
        "strategyConfigId": request.strategy_config_id,
        "strategyKey": request.strategy_key,
        "symbol": request.symbol,
        "timeframe": request.timeframe,
        "configOverrides": request.config_overrides,
        "dryRun": request.dry_run
    })
}

fn latest_backtest_summary_from_log(log: &LatestBacktestLogRow) -> LatestBacktestSummary {
    LatestBacktestSummary {
        has_backtest: true,
        back_test_log_id: Some(log.id),
        strategy_type: Some(log.strategy_type.clone()),
        inst_type: Some(log.inst_type.clone()),
        time: Some(log.time.clone()),
        final_fund: Some(log.final_fund),
        profit: log.profit,
        win_rate: Some(log.win_rate.clone()),
        open_positions_num: Some(log.open_positions_num),
        one_bar_after_win_rate: log.one_bar_after_win_rate,
        two_bar_after_win_rate: log.two_bar_after_win_rate,
        three_bar_after_win_rate: log.three_bar_after_win_rate,
        four_bar_after_win_rate: log.four_bar_after_win_rate,
        five_bar_after_win_rate: log.five_bar_after_win_rate,
        ten_bar_after_win_rate: log.ten_bar_after_win_rate,
        kline_start_time: Some(log.kline_start_time),
        kline_end_time: Some(log.kline_end_time),
        kline_nums: Some(log.kline_nums),
        sharpe_ratio: log.sharpe_ratio,
        annual_return: log.annual_return,
        total_return: log.total_return,
        max_drawdown: log.max_drawdown,
        volatility: log.volatility,
        created_at: Some(log.created_at),
    }
}

fn default_latest_backtest_summary(back_test_log_id: Option<i32>) -> LatestBacktestSummary {
    LatestBacktestSummary {
        has_backtest: false,
        back_test_log_id,
        strategy_type: None,
        inst_type: None,
        time: None,
        final_fund: None,
        profit: None,
        win_rate: None,
        open_positions_num: None,
        one_bar_after_win_rate: None,
        two_bar_after_win_rate: None,
        three_bar_after_win_rate: None,
        four_bar_after_win_rate: None,
        five_bar_after_win_rate: None,
        ten_bar_after_win_rate: None,
        kline_start_time: None,
        kline_end_time: None,
        kline_nums: None,
        sharpe_ratio: None,
        annual_return: None,
        total_return: None,
        max_drawdown: None,
        volatility: None,
        created_at: None,
    }
}

fn parse_json_value_or_string(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }

    serde_json::from_str::<Value>(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_string()))
}

fn json_response(status_code: u16, body: Value) -> InternalHttpJsonResponse {
    InternalHttpJsonResponse { status_code, body }
}

fn route_path(path: &str) -> &str {
    path.split_once('?').map(|(path, _)| path).unwrap_or(path)
}

fn required_query_param(query: &str, names: &[&str]) -> Result<String, String> {
    let value = query_param(query, names)
        .ok_or_else(|| format!("{} is required", names.first().copied().unwrap_or("param")))?;
    if value.trim().is_empty() {
        return Err(format!(
            "{} is required",
            names.first().copied().unwrap_or("param")
        ));
    }
    Ok(value)
}

fn query_param(query: &str, names: &[&str]) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(raw_name, raw_value)| {
            let name = raw_name.trim();
            if names.iter().any(|candidate| name == *candidate) {
                Some(raw_value.trim().replace('+', " "))
            } else {
                None
            }
        })
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("连接提前关闭");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            anyhow::bail!("HTTP header too large");
        }
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
    };

    let header_bytes = &buffer[..header_end];
    let header = std::str::from_utf8(header_bytes).context("HTTP header不是UTF-8")?;
    let mut lines = header.lines();
    let request_line = lines.next().context("缺少HTTP request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().context("缺少HTTP method")?.to_string();
    let path = request_parts.next().context("缺少HTTP path")?.to_string();
    let content_length = parse_content_length(header)?;
    if content_length > MAX_BODY_BYTES {
        anyhow::bail!("HTTP body too large");
    }

    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("HTTP body读取不完整");
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = buffer[body_start..body_start + content_length].to_vec();

    Ok(HttpRequest { method, path, body })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(header: &str) -> Result<usize> {
    for line in header.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .context("Content-Length格式错误");
        }
    }
    Ok(0)
}

async fn write_response(stream: &mut TcpStream, response: InternalHttpJsonResponse) -> Result<()> {
    let body = serde_json::to_vec(&response.body)?;
    let reason = reason_phrase(response.status_code);
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: application/json; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status_code,
        reason,
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await?;
    Ok(())
}

fn reason_phrase(status_code: u16) -> &'static str {
    match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

#[cfg(test)]
#[path = "internal_server_tests.rs"]
mod tests;
