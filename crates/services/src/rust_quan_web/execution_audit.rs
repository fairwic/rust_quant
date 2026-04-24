use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Map, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::exchange::OrderPlacementRequest;
use crate::rust_quan_web::ExecutionTask;

const REDACTED: &str = "***REDACTED***";
const AUDIT_ENDPOINT_PLACE_ORDER: &str = "trade.place_order";
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
}

#[async_trait]
pub trait ExecutionAuditRepository: Send + Sync {
    async fn upsert_worker_checkpoint(&self, checkpoint: &ExecutionWorkerCheckpoint) -> Result<()>;

    async fn insert_exchange_request_audit(&self, audit: &ExchangeRequestAuditLog) -> Result<()>;
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
            .connect_lazy(&database_url)?;
        Ok(Some(Self::new(pool)))
    }
}

#[async_trait]
impl ExecutionAuditRepository for PostgresExecutionAuditRepository {
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

fn redact_error_message(message: String) -> String {
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
    } else {
        message
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_quan_web::ExecutionTask;
    use crypto_exc_all::ExchangeId;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn task(payload: serde_json::Value) -> ExecutionTask {
        ExecutionTask {
            id: 42,
            news_signal_id: Some(7),
            strategy_signal_id: None,
            combo_id: 9,
            buyer_email: "buyer@example.com".to_string(),
            strategy_slug: "news_momentum".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            task_type: "execute_signal".to_string(),
            task_status: "pending".to_string(),
            priority: 3,
            lease_owner: None,
            lease_until: None,
            scheduled_at: "2026-04-23T12:00:00".to_string(),
            request_payload_json: payload,
            created_at: "2026-04-23T12:00:00".to_string(),
            updated_at: "2026-04-23T12:00:00".to_string(),
        }
    }

    #[test]
    fn redacts_sensitive_values_from_audit_payload() {
        let payload = json!({
            "api_key": "plain-api-key",
            "api_secret": "plain-api-secret",
            "passphrase": "plain-passphrase",
            "nested": {
                "access_token": "plain-token",
                "symbol": "BTC-USDT-SWAP"
            }
        });

        let redacted = redact_audit_payload(payload);
        let serialized = redacted.to_string();

        assert_eq!(redacted["api_key"], "***REDACTED***");
        assert_eq!(redacted["api_secret"], "***REDACTED***");
        assert_eq!(redacted["passphrase"], "***REDACTED***");
        assert_eq!(redacted["nested"]["access_token"], "***REDACTED***");
        assert_eq!(redacted["nested"]["symbol"], "BTC-USDT-SWAP");
        assert!(!serialized.contains("plain-api-key"));
        assert!(!serialized.contains("plain-api-secret"));
        assert!(!serialized.contains("plain-passphrase"));
        assert!(!serialized.contains("plain-token"));
    }

    #[test]
    fn redacts_sensitive_values_from_nested_headers_and_arrays() {
        let payload = json!({
            "headers": {
                "Authorization": "Bearer plain-bearer-token",
                "X-Api-Key": "plain-header-api-key",
                "Content-Type": "application/json"
            },
            "accounts": [
                {
                    "secretKey": "plain-secret-key",
                    "accessToken": "plain-access-token",
                    "label": "primary"
                }
            ]
        });

        let redacted = redact_audit_payload(payload);
        let serialized = redacted.to_string();

        assert_eq!(redacted["headers"]["Authorization"], "***REDACTED***");
        assert_eq!(redacted["headers"]["X-Api-Key"], "***REDACTED***");
        assert_eq!(redacted["headers"]["Content-Type"], "application/json");
        assert_eq!(redacted["accounts"][0]["secretKey"], "***REDACTED***");
        assert_eq!(redacted["accounts"][0]["accessToken"], "***REDACTED***");
        assert_eq!(redacted["accounts"][0]["label"], "primary");
        assert!(!serialized.contains("plain-bearer-token"));
        assert!(!serialized.contains("plain-header-api-key"));
        assert!(!serialized.contains("plain-secret-key"));
        assert!(!serialized.contains("plain-access-token"));
    }

    #[test]
    fn builds_dry_run_audit_payload_without_credentials() {
        let task = task(json!({
            "exchange": "okx",
            "symbol": "BTC-USDT-SWAP",
            "side": "buy",
            "size": "0.01",
            "api_key": "plain-api-key"
        }));
        let order_task = crate::rust_quan_web::ExecutionOrderTask::from_task_with_default(
            &task,
            ExchangeId::Okx,
        )
        .unwrap();
        let request = order_task.to_order_request().unwrap();

        let audit = ExchangeRequestAuditLog::success(
            &task,
            &request,
            true,
            Some(12),
            json!({
                "dry_run": true,
                "api_secret": "plain-api-secret"
            }),
        );

        assert_eq!(audit.request_id, "task-42-rq-task-42");
        assert_eq!(audit.exchange, "okx");
        assert_eq!(audit.symbol, "BTC-USDT-SWAP");
        assert_eq!(audit.endpoint, "trade.place_order");
        assert_eq!(audit.request_status, "completed");
        assert_eq!(audit.latency_ms, Some(12));
        assert_eq!(audit.request_payload["dry_run"], true);
        assert_eq!(audit.request_payload["order"]["size"], "0.01");
        assert_eq!(audit.request_payload["task"]["id"], 42);
        assert_eq!(
            audit.request_payload["task"]["request_payload_json"]["api_key"],
            "***REDACTED***"
        );
        assert_eq!(audit.response_payload["api_secret"], "***REDACTED***");
        assert!(!audit.request_payload.to_string().contains("plain-api-key"));
        assert!(!audit
            .response_payload
            .to_string()
            .contains("plain-api-secret"));
    }

    #[test]
    fn builds_worker_checkpoint_payload() {
        let checkpoint = ExecutionWorkerCheckpoint::heartbeat(
            "worker-a",
            "leased",
            Some(42),
            json!({
                "leased_count": 1,
                "dry_run": true
            }),
        );

        assert_eq!(checkpoint.worker_id, "worker-a");
        assert_eq!(checkpoint.worker_kind, "execution");
        assert_eq!(checkpoint.worker_status, "leased");
        assert_eq!(checkpoint.lease_owner, "worker-a");
        assert_eq!(checkpoint.checkpoint_key, "execution_worker");
        assert_eq!(checkpoint.last_task_id.as_deref(), Some("42"));
        assert_eq!(checkpoint.checkpoint_value["leased_count"], 1);
    }

    #[test]
    fn repository_checkpoint_columns_match_quant_core_ddl() {
        assert_insert_columns_exist_in_ddl(
            UPSERT_WORKER_CHECKPOINT_SQL,
            "execution_worker_checkpoints",
            &[
                "worker_id",
                "worker_kind",
                "worker_status",
                "lease_owner",
                "checkpoint_key",
                "checkpoint_value",
                "last_task_id",
                "last_heartbeat_at",
                "updated_at",
            ],
        );
    }

    #[test]
    fn repository_exchange_audit_columns_match_quant_core_ddl() {
        assert_insert_columns_exist_in_ddl(
            INSERT_EXCHANGE_REQUEST_AUDIT_SQL,
            "exchange_request_audit_logs",
            &[
                "request_id",
                "exchange",
                "symbol",
                "endpoint",
                "request_status",
                "latency_ms",
                "request_payload",
                "response_payload",
                "error_message",
            ],
        );
    }

    fn assert_insert_columns_exist_in_ddl(sql: &str, table: &str, expected_columns: &[&str]) {
        let ddl_columns = create_table_columns(table);
        let insert_columns = insert_columns(sql, table);
        assert_eq!(insert_columns, expected_columns);

        let missing_columns = insert_columns
            .iter()
            .filter(|column| !ddl_columns.contains(**column))
            .copied()
            .collect::<Vec<_>>();
        assert!(
            missing_columns.is_empty(),
            "{table} repository SQL uses columns missing from DDL: {missing_columns:?}"
        );
    }

    fn create_table_columns(table: &str) -> BTreeSet<&'static str> {
        let ddl = include_str!("../../../../sql/postgres_quant_core.sql");
        let marker = format!("CREATE TABLE IF NOT EXISTS {table} (");
        let start = ddl
            .find(&marker)
            .unwrap_or_else(|| panic!("{table} table DDL missing"))
            + marker.len();
        let body = &ddl[start..];
        let end = body
            .find("\n);")
            .unwrap_or_else(|| panic!("{table} table DDL terminator missing"));

        body[..end]
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| line.trim_end_matches(',').split_whitespace().next())
            .collect()
    }

    fn insert_columns<'a>(sql: &'a str, table: &str) -> Vec<&'a str> {
        let marker = format!("INSERT INTO {table} (");
        let start = sql
            .find(&marker)
            .unwrap_or_else(|| panic!("{table} insert SQL missing"))
            + marker.len();
        let body = &sql[start..];
        let end = body
            .find(')')
            .unwrap_or_else(|| panic!("{table} insert SQL column terminator missing"));

        body[..end]
            .split(',')
            .map(str::trim)
            .filter(|column| !column.is_empty())
            .collect()
    }
}
