use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, QueryBuilder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

use crate::app::exchange_symbol_sync::{
    run_exchange_symbol_sync_from_env, ExchangeSymbolSyncRequest,
};
use rust_quant_orchestration::infra::strategy_config::BackTestConfig;
use rust_quant_orchestration::workflow::backtest_runner;

const DEFAULT_INTERNAL_ADDR: &str = "127.0.0.1:5322";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_BACKTEST_SIGNAL_LIMIT: i64 = 100;
const MAX_KLINE_LIMIT: i64 = 2_000;
const DEFAULT_KLINE_LIMIT: i64 = 500;
const DEFAULT_KLINE_EXCHANGE: &str = "binance";

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

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let request = read_request(&mut stream).await?;
    let route = route_path(&request.path);
    let response = match (request.method.as_str(), route) {
        ("POST", "/internal/backtests/run") => handle_backtest_run_body(&request.body).await,
        ("GET", "/internal/backtests/latest") => handle_latest_backtest_path(&request.path).await,
        ("GET", "/internal/klines") => handle_market_klines_path(&request.path).await,
        ("POST", "/internal/exchange-symbols/sync") => {
            handle_exchange_symbol_sync_body(&request.body).await
        }
        ("GET", "/internal/health") => json_response(200, json!({ "status": "ok" })),
        ("POST", _) => json_response(404, json!({ "error": "not found" })),
        _ => json_response(405, json!({ "error": "method not allowed" })),
    };
    write_response(&mut stream, response).await
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

fn seconds_to_legacy_millis(timestamp: i64) -> i64 {
    if timestamp > 10_000_000_000 {
        timestamp
    } else {
        timestamp.saturating_mul(1_000)
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
