use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Map, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::exchange::OrderPlacementRequest;
use crate::rust_quan_web::{ExecutionTask, ExecutionTaskReportRequest};

const REDACTED: &str = "***REDACTED***";
const AUDIT_ENDPOINT_PLACE_ORDER: &str = "trade.place_order";
const AUDIT_ENDPOINT_REPORT_RESULT: &str = "web.report_result";
const UPSERT_WORKER_CHECKPOINT_SQL: &str = r#"
            INSERT INTO execution_worker_checkpoints (
                worker_id,
                worker_kind,
                worker_status,
                lease_owner,
                checkpoint_key,
                checkpoint_value,
                last_task_id,
                last_heartbeat_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
            ON CONFLICT (worker_id) DO UPDATE SET
                worker_kind = EXCLUDED.worker_kind,
                worker_status = EXCLUDED.worker_status,
                lease_owner = EXCLUDED.lease_owner,
                checkpoint_key = EXCLUDED.checkpoint_key,
                checkpoint_value = EXCLUDED.checkpoint_value,
                last_task_id = EXCLUDED.last_task_id,
                last_heartbeat_at = NOW(),
                updated_at = NOW()
            "#;
const INSERT_EXCHANGE_REQUEST_AUDIT_SQL: &str = r#"
            INSERT INTO exchange_request_audit_logs (
                request_id,
                exchange,
                symbol,
                endpoint,
                request_status,
                latency_ms,
                request_payload,
                response_payload,
                error_message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#;
const LIST_REPORT_RESULT_REPLAY_CANDIDATES_SQL: &str = r#"
            SELECT failed.request_id, failed.request_payload
            FROM exchange_request_audit_logs failed
            WHERE failed.endpoint = $1
              AND failed.request_status = 'failed'
              AND failed.request_payload #>> '{replay,action}' = 'retry_report_result_only'
              AND failed.request_payload #>> '{replay,place_order_allowed}' = 'false'
              AND NOT EXISTS (
                  SELECT 1
                  FROM exchange_request_audit_logs replayed
                  WHERE replayed.endpoint = failed.endpoint
                    AND replayed.request_id = failed.request_id
                    AND replayed.request_status = 'replayed'
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM exchange_request_audit_logs recent_failed
                  WHERE recent_failed.endpoint = failed.endpoint
                    AND recent_failed.request_id = failed.request_id
                    AND recent_failed.request_status = 'failed'
                    AND recent_failed.created_at > failed.created_at
                    AND recent_failed.created_at >= NOW() - ($3::bigint * INTERVAL '1 second')
              )
            ORDER BY failed.created_at ASC
            LIMIT $2
            "#;
const LIVE_AUDIT_READINESS_TABLE_SQL: &str = r#"
            SELECT
                to_regclass('public.execution_worker_checkpoints') IS NOT NULL AS has_worker_checkpoints,
                to_regclass('public.exchange_request_audit_logs') IS NOT NULL AS has_exchange_audit_logs
            "#;

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionWorkerCheckpoint {
    pub worker_id: String,
    pub worker_kind: String,
    pub worker_status: String,
    pub lease_owner: String,
    pub checkpoint_key: String,
    pub checkpoint_value: Value,
    pub last_task_id: Option<String>,
}

impl ExecutionWorkerCheckpoint {
    pub fn heartbeat(
        worker_id: impl Into<String>,
        worker_status: impl Into<String>,
        last_task_id: Option<i64>,
        checkpoint_value: Value,
    ) -> Self {
        let worker_id = worker_id.into();
        Self {
            worker_id: worker_id.clone(),
            worker_kind: "execution".to_string(),
            worker_status: worker_status.into(),
            lease_owner: worker_id,
            checkpoint_key: "execution_worker".to_string(),
            checkpoint_value: redact_audit_payload(checkpoint_value),
            last_task_id: last_task_id.map(|value| value.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeRequestAuditLog {
    pub request_id: String,
    pub exchange: String,
    pub symbol: String,
    pub endpoint: String,
    pub request_status: String,
    pub latency_ms: Option<i32>,
    pub request_payload: Value,
    pub response_payload: Value,
    pub error_message: String,
}

impl ExchangeRequestAuditLog {
    pub fn success(
        task: &ExecutionTask,
        request: &OrderPlacementRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        response_payload: Value,
    ) -> Self {
        Self {
            request_id: request_id(task, request),
            exchange: request.exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(request.exchange),
            endpoint: AUDIT_ENDPOINT_PLACE_ORDER.to_string(),
            request_status: "completed".to_string(),
            latency_ms,
            request_payload: order_request_payload(task, request, dry_run),
            response_payload: redact_audit_payload(response_payload),
            error_message: String::new(),
        }
    }

    pub fn live_mutation_preflight(
        task: &ExecutionTask,
        request: &OrderPlacementRequest,
        dry_run: bool,
    ) -> Self {
        Self {
            request_id: request_id(task, request),
            exchange: request.exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(request.exchange),
            endpoint: format!("{AUDIT_ENDPOINT_PLACE_ORDER}.preflight"),
            request_status: "completed".to_string(),
            latency_ms: None,
            request_payload: order_request_payload(task, request, dry_run),
            response_payload: redact_audit_payload(json!({
                "stage": "live_execution_audit_preflight",
                "place_order_allowed": false,
                "mutation_allowed": false,
                "audit_write_confirmed": true,
            })),
            error_message: String::new(),
        }
    }

    pub fn failed(
        task: &ExecutionTask,
        request: &OrderPlacementRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id(task, request),
            exchange: request.exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(request.exchange),
            endpoint: AUDIT_ENDPOINT_PLACE_ORDER.to_string(),
            request_status: "failed".to_string(),
            latency_ms,
            request_payload: order_request_payload(task, request, dry_run),
            response_payload: json!({}),
            error_message: redact_error_message(error_message.into()),
        }
    }

    pub fn report_result_failed(
        report: &ExecutionTaskReportRequest,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: report_request_id(report),
            exchange: report.exchange.clone(),
            symbol: String::new(),
            endpoint: AUDIT_ENDPOINT_REPORT_RESULT.to_string(),
            request_status: "failed".to_string(),
            latency_ms: None,
            request_payload: report_result_request_payload(report),
            response_payload: json!({
                "replay_action": "retry_report_result_only",
                "place_order_allowed": false,
            }),
            error_message: redact_error_message(error_message.into()),
        }
    }

    pub fn report_result_replayed(
        report: &ExecutionTaskReportRequest,
        latency_ms: Option<i32>,
        response_payload: Value,
    ) -> Self {
        Self {
            request_id: report_request_id(report),
            exchange: report.exchange.clone(),
            symbol: String::new(),
            endpoint: AUDIT_ENDPOINT_REPORT_RESULT.to_string(),
            request_status: "replayed".to_string(),
            latency_ms,
            request_payload: report_result_request_payload(report),
            response_payload: redact_audit_payload(json!({
                "replay_status": "completed",
                "place_order_allowed": false,
                "response": response_payload,
            })),
            error_message: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportResultReplayCandidate {
    pub request_id: String,
    pub report: ExecutionTaskReportRequest,
}

#[async_trait]
pub trait ExecutionAuditRepository: Send + Sync {
    fn can_audit_live_mutations(&self) -> bool {
        false
    }

    async fn verify_live_audit_ready(&self) -> Result<()> {
        if self.can_audit_live_mutations() {
            Ok(())
        } else {
            Err(anyhow!(
                "QUANT_CORE_DATABASE_URL is required for live execution audit"
            ))
        }
    }

    async fn upsert_worker_checkpoint(&self, checkpoint: &ExecutionWorkerCheckpoint) -> Result<()>;

    async fn insert_exchange_request_audit(&self, audit: &ExchangeRequestAuditLog) -> Result<()>;

    async fn list_report_result_replay_candidates(
        &self,
        _limit: u32,
        _failure_backoff_seconds: u64,
    ) -> Result<Vec<ReportResultReplayCandidate>> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Default)]
pub struct NoopExecutionAuditRepository;

#[async_trait]
impl ExecutionAuditRepository for NoopExecutionAuditRepository {
    async fn upsert_worker_checkpoint(
        &self,
        _checkpoint: &ExecutionWorkerCheckpoint,
    ) -> Result<()> {
        Ok(())
    }

    async fn insert_exchange_request_audit(&self, _audit: &ExchangeRequestAuditLog) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PostgresExecutionAuditRepository {
    pool: PgPool,
}

impl PostgresExecutionAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_env() -> Result<Option<Self>> {
        let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
            .or_else(|_| std::env::var("QUANT_CORE_POSTGRES_URL"))
            .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
            .ok();
        let Some(database_url) = database_url else {
            return Ok(None);
        };
        let max_connections = std::env::var("QUANT_CORE_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(5);
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect_lazy(&database_url)?;
        Ok(Some(Self::new(pool)))
    }
}

#[async_trait]
impl ExecutionAuditRepository for PostgresExecutionAuditRepository {
    fn can_audit_live_mutations(&self) -> bool {
        true
    }

    async fn verify_live_audit_ready(&self) -> Result<()> {
        let row = sqlx::query(LIVE_AUDIT_READINESS_TABLE_SQL)
            .fetch_one(&self.pool)
            .await
            .context("connect quant_core live audit database")?;
        let has_worker_checkpoints: bool = row.try_get("has_worker_checkpoints")?;
        let has_exchange_audit_logs: bool = row.try_get("has_exchange_audit_logs")?;
        if !has_worker_checkpoints || !has_exchange_audit_logs {
            return Err(anyhow!(
                "quant_core live audit tables are not ready: execution_worker_checkpoints={}, exchange_request_audit_logs={}",
                has_worker_checkpoints,
                has_exchange_audit_logs
            ));
        }

        let probe_id = live_audit_preflight_id();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("open quant_core live audit readiness transaction")?;
        sqlx::query(UPSERT_WORKER_CHECKPOINT_SQL)
            .bind(&probe_id)
            .bind("execution")
            .bind("live_audit_preflight")
            .bind(&probe_id)
            .bind("live_audit_preflight")
            .bind(json!({
                "stage": "live_audit_preflight",
                "mutation_allowed": false,
            }))
            .bind(None::<&str>)
            .execute(&mut *tx)
            .await
            .context("write execution_worker_checkpoints live audit preflight")?;
        sqlx::query(INSERT_EXCHANGE_REQUEST_AUDIT_SQL)
            .bind(&probe_id)
            .bind("preflight")
            .bind("preflight")
            .bind("live_audit_preflight")
            .bind("preflight")
            .bind(None::<i32>)
            .bind(json!({
                "stage": "live_audit_preflight",
                "place_order_allowed": false,
                "mutation_allowed": false,
            }))
            .bind(json!({}))
            .bind("")
            .execute(&mut *tx)
            .await
            .context("write exchange_request_audit_logs live audit preflight")?;
        tx.rollback()
            .await
            .context("rollback quant_core live audit readiness transaction")?;
        Ok(())
    }

    async fn upsert_worker_checkpoint(&self, checkpoint: &ExecutionWorkerCheckpoint) -> Result<()> {
        sqlx::query(UPSERT_WORKER_CHECKPOINT_SQL)
            .bind(&checkpoint.worker_id)
            .bind(&checkpoint.worker_kind)
            .bind(&checkpoint.worker_status)
            .bind(&checkpoint.lease_owner)
            .bind(&checkpoint.checkpoint_key)
            .bind(&checkpoint.checkpoint_value)
            .bind(checkpoint.last_task_id.as_deref())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn insert_exchange_request_audit(&self, audit: &ExchangeRequestAuditLog) -> Result<()> {
        sqlx::query(INSERT_EXCHANGE_REQUEST_AUDIT_SQL)
            .bind(&audit.request_id)
            .bind(&audit.exchange)
            .bind(&audit.symbol)
            .bind(&audit.endpoint)
            .bind(&audit.request_status)
            .bind(audit.latency_ms)
            .bind(&audit.request_payload)
            .bind(&audit.response_payload)
            .bind(&audit.error_message)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_report_result_replay_candidates(
        &self,
        limit: u32,
        failure_backoff_seconds: u64,
    ) -> Result<Vec<ReportResultReplayCandidate>> {
        let rows = sqlx::query(LIST_REPORT_RESULT_REPLAY_CANDIDATES_SQL)
            .bind(AUDIT_ENDPOINT_REPORT_RESULT)
            .bind(i64::from(limit.clamp(1, 100)))
            .bind(i64::try_from(failure_backoff_seconds).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter()
            .map(|row| {
                let request_id: String = row.try_get("request_id")?;
                let request_payload: Value = row.try_get("request_payload")?;
                report_result_replay_candidate_from_payload(request_id, &request_payload)
            })
            .collect()
    }
}

fn live_audit_preflight_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("live-audit-preflight-{}-{millis}", std::process::id())
}

pub fn redact_audit_payload(payload: Value) -> Value {
    match payload {
        Value::Object(map) => Value::Object(redact_object(map)),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_audit_payload).collect())
        }
        other => other,
    }
}

fn redact_object(map: Map<String, Value>) -> Map<String, Value> {
    map.into_iter()
        .map(|(key, value)| {
            let value = if is_sensitive_key(&key) {
                Value::String(REDACTED.to_string())
            } else {
                redact_audit_payload(value)
            };
            (key, value)
        })
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.contains("apikey")
        || normalized.contains("apisecret")
        || normalized.contains("passphrase")
        || normalized.contains("accesstoken")
        || normalized.contains("secret")
        || normalized.contains("signature")
        || normalized == "authorization"
        || normalized == "token"
}

pub(crate) fn redact_error_message(message: String) -> String {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("api_key")
        || normalized.contains("api key")
        || normalized.contains("api_secret")
        || normalized.contains("api secret")
        || normalized.contains("passphrase")
        || normalized.contains("access_token")
        || normalized.contains("authorization")
        || normalized.contains("bearer ")
        || normalized.contains("secret")
    {
        "redacted sensitive error".to_string()
    } else if normalized.contains("signature") {
        let redacted = redact_signature_material(&message);
        if redacted == message {
            "redacted sensitive error".to_string()
        } else {
            redacted
        }
    } else {
        message
    }
}

fn redact_signature_material(message: &str) -> String {
    redact_signature_parameters(&redact_signed_urls(message))
}

fn redact_signed_urls(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let mut result = String::with_capacity(message.len());
    let mut cursor = 0;

    while let Some(url_start) = next_url_start(&lower, cursor) {
        let url_end = url_token_end(message, url_start);
        result.push_str(&message[cursor..url_start]);

        let token = &message[url_start..url_end];
        if token.to_ascii_lowercase().contains("signature=") {
            result.push_str("[signed_url_redacted]");
        } else {
            result.push_str(token);
        }
        cursor = url_end;
    }

    result.push_str(&message[cursor..]);
    result
}

fn next_url_start(lower: &str, cursor: usize) -> Option<usize> {
    let tail = &lower[cursor..];
    let http = tail.find("http://");
    let https = tail.find("https://");
    match (http, https) {
        (Some(http), Some(https)) => Some(cursor + http.min(https)),
        (Some(http), None) => Some(cursor + http),
        (None, Some(https)) => Some(cursor + https),
        (None, None) => None,
    }
}

fn url_token_end(message: &str, start: usize) -> usize {
    message[start..]
        .char_indices()
        .find_map(|(offset, ch)| {
            matches!(ch, ' ' | '\n' | '\t' | '\r' | ')' | '"' | '\'' | ']' | '}')
                .then_some(start + offset)
        })
        .unwrap_or(message.len())
}

fn redact_signature_parameters(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let mut result = String::with_capacity(message.len());
    let mut cursor = 0;

    while let Some(relative_start) = lower[cursor..].find("signature=") {
        let key_start = cursor + relative_start;
        let value_start = key_start + "signature=".len();
        let value_end = signature_value_end(message, value_start);

        result.push_str(&message[cursor..key_start]);
        result.push_str("[signed_param_redacted]");
        cursor = value_end;
    }

    result.push_str(&message[cursor..]);
    result
}

fn signature_value_end(message: &str, value_start: usize) -> usize {
    message[value_start..]
        .char_indices()
        .find_map(|(offset, ch)| {
            matches!(
                ch,
                '&' | ' ' | '\n' | '\t' | '\r' | ')' | '"' | '\'' | ',' | ';'
            )
            .then_some(value_start + offset)
        })
        .unwrap_or(message.len())
}

fn request_id(task: &ExecutionTask, request: &OrderPlacementRequest) -> String {
    request
        .client_order_id
        .as_ref()
        .map(|client_order_id| format!("task-{}-{}", task.id, client_order_id))
        .unwrap_or_else(|| format!("task-{}", task.id))
}

fn order_request_payload(
    task: &ExecutionTask,
    request: &OrderPlacementRequest,
    dry_run: bool,
) -> Value {
    redact_audit_payload(json!({
        "dry_run": dry_run,
        "task": {
            "id": task.id,
            "news_signal_id": task.news_signal_id,
            "combo_id": task.combo_id,
            "strategy_slug": task.strategy_slug,
            "task_type": task.task_type,
            "request_payload_json": task.request_payload_json,
        },
        "order": {
            "exchange": request.exchange,
            "symbol": request.instrument.symbol_for(request.exchange),
            "side": request.side,
            "order_type": request.order_type,
            "size": request.size,
            "price": request.price,
            "margin_mode": request.margin_mode,
            "margin_coin": request.margin_coin,
            "position_side": request.position_side,
            "trade_side": request.trade_side,
            "client_order_id": request.client_order_id,
            "reduce_only": request.reduce_only,
            "time_in_force": request.time_in_force,
        }
    }))
}

fn report_request_id(report: &ExecutionTaskReportRequest) -> String {
    let external_order_id = report.external_order_id.trim();
    let candidate = if external_order_id.is_empty() {
        format!("report-task-{}", report.task_id)
    } else {
        format!("report-task-{}-{}", report.task_id, external_order_id)
    };
    candidate.chars().take(128).collect()
}

fn report_result_request_payload(report: &ExecutionTaskReportRequest) -> Value {
    let raw_payload_json = report
        .raw_payload_json
        .as_deref()
        .map(redact_report_raw_payload_json)
        .unwrap_or(Value::Null);
    redact_audit_payload(json!({
        "replay": {
            "action": "retry_report_result_only",
            "place_order_allowed": false,
            "reason": "web_report_result_failed",
        },
        "report": {
            "task_id": report.task_id,
            "execution_status": report.execution_status,
            "exchange": report.exchange,
            "external_order_id": report.external_order_id,
            "order_side": report.order_side,
            "order_status": report.order_status,
            "filled_qty": report.filled_qty,
            "filled_quote": report.filled_quote,
            "fee_amount": report.fee_amount,
            "profit_usdt": report.profit_usdt,
            "executed_at": report.executed_at,
            "error_message": report.error_message,
            "raw_payload_json": raw_payload_json,
        }
    }))
}

fn report_result_replay_candidate_from_payload(
    request_id: String,
    request_payload: &Value,
) -> Result<ReportResultReplayCandidate> {
    let replay = request_payload
        .get("replay")
        .ok_or_else(|| anyhow!("report replay payload missing replay section"))?;
    if replay.get("action").and_then(Value::as_str) != Some("retry_report_result_only") {
        return Err(anyhow!(
            "report replay payload action is not retry_report_result_only"
        ));
    }
    if replay
        .get("place_order_allowed")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Err(anyhow!("report replay payload allows place_order"));
    }

    let report = request_payload
        .get("report")
        .ok_or_else(|| anyhow!("report replay payload missing report section"))?;

    Ok(ReportResultReplayCandidate {
        request_id,
        report: ExecutionTaskReportRequest {
            task_id: required_i64(report, "task_id")?,
            execution_status: required_string(report, "execution_status")?,
            exchange: required_string(report, "exchange")?,
            external_order_id: required_string(report, "external_order_id")?,
            order_side: required_string(report, "order_side")?,
            order_status: required_string(report, "order_status")?,
            filled_qty: optional_f64(report, "filled_qty"),
            filled_quote: optional_f64(report, "filled_quote"),
            fee_amount: optional_f64(report, "fee_amount"),
            profit_usdt: optional_f64(report, "profit_usdt"),
            executed_at: optional_string(report, "executed_at"),
            error_message: optional_string(report, "error_message"),
            raw_payload_json: report
                .get("raw_payload_json")
                .filter(|value| !value.is_null())
                .map(|value| {
                    value
                        .as_str()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| value.to_string())
                }),
        },
    })
}

fn required_i64(value: &Value, field: &str) -> Result<i64> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("report replay payload missing numeric field: {field}"))
}

fn required_string(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("report replay payload missing string field: {field}"))
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn optional_f64(value: &Value, field: &str) -> Option<f64> {
    value.get(field).and_then(Value::as_f64)
}

fn redact_report_raw_payload_json(raw: &str) -> Value {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => redact_audit_payload(value),
        Err(_) if contains_sensitive_marker(raw) => Value::String(REDACTED.to_string()),
        Err(_) => Value::String(raw.to_string()),
    }
}

fn contains_sensitive_marker(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("api_key")
        || normalized.contains("api key")
        || normalized.contains("api_secret")
        || normalized.contains("api secret")
        || normalized.contains("passphrase")
        || normalized.contains("access_token")
        || normalized.contains("authorization")
        || normalized.contains("bearer ")
        || normalized.contains("secret")
}

#[cfg(test)]
mod tests;
