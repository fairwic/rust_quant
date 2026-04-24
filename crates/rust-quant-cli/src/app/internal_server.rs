use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

use rust_quant_orchestration::infra::strategy_config::BackTestConfig;
use rust_quant_orchestration::workflow::backtest_runner;

const DEFAULT_INTERNAL_ADDR: &str = "127.0.0.1:5322";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct InternalHttpJsonResponse {
    pub status_code: u16,
    pub body: Value,
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

pub fn backtest_config_from_body(body: &[u8]) -> Result<BackTestConfig, String> {
    let request = serde_json::from_slice::<BacktestRunRequest>(body)
        .map_err(|err| format!("invalid json body: {err}"))?;
    validate_backtest_request(&request).map_err(str::to_string)?;
    Ok(backtest_config_from_request(&request))
}

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let request = read_request(&mut stream).await?;
    let response = match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/internal/backtests/run") => handle_backtest_run_body(&request.body).await,
        ("GET", "/internal/health") => json_response(200, json!({ "status": "ok" })),
        ("POST", _) => json_response(404, json!({ "error": "not found" })),
        _ => json_response(405, json!({ "error": "method not allowed" })),
    };
    write_response(&mut stream, response).await
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

fn json_response(status_code: u16, body: Value) -> InternalHttpJsonResponse {
    InternalHttpJsonResponse { status_code, body }
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
