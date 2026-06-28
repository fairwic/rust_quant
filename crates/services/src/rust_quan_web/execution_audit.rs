use crate::exchange::OrderPlacementRequest;
use crate::rust_quan_web::{ExecutionTask, ExecutionTaskReportRequest};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, PrepareOrderSettingsRequest, ProtectiveOrderRequest,
};
use serde_json::{json, Map, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
const REDACTED: &str = "***REDACTED***";
const AUDIT_ENDPOINT_PLACE_ORDER: &str = "trade.place_order";
const AUDIT_ENDPOINT_CANCEL_ORDER: &str = "trade.cancel_order";
const AUDIT_ENDPOINT_PLACE_PROTECTIVE_ORDER: &str = "trade.place_protective_order";
const AUDIT_ENDPOINT_CANCEL_PROTECTIVE_ORDER: &str = "trade.cancel_protective_order";
const AUDIT_ENDPOINT_PREPARE_ORDER_SETTINGS: &str = "account.prepare_order_settings";
const AUDIT_ENDPOINT_REPORT_RESULT: &str = "web.report_result";
const DEFAULT_EXCHANGE_REQUEST_WINDOW_SECONDS: i64 = 60;
const DEFAULT_EXCHANGE_REQUEST_MAX_PER_WINDOW: i32 = 60;
const DEFAULT_EXCHANGE_CIRCUIT_FAILURE_THRESHOLD: i32 = 3;
const DEFAULT_EXCHANGE_CIRCUIT_OPEN_SECONDS: i64 = 60;
const DEFAULT_EXCHANGE_REQUEST_AUDIT_RETENTION_DAYS: i64 = 30;
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
const DELETE_EXCHANGE_REQUEST_AUDIT_RETENTION_SQL: &str = r#"
            DELETE FROM exchange_request_audit_logs
            WHERE created_at < NOW() - ($1::bigint * INTERVAL '1 day')
            "#;
const SELECT_EXCHANGE_REQUEST_CIRCUIT_SQL: &str = r#"
            SELECT
                state,
                opened_until,
                COALESCE(opened_until > NOW(), FALSE) AS circuit_open
            FROM exchange_request_circuit_breakers
            WHERE exchange = $1
              AND credential_key = $2
              AND endpoint_family = $3
            FOR UPDATE
            "#;
const ACQUIRE_EXCHANGE_REQUEST_PERMIT_SQL: &str = r#"
            INSERT INTO exchange_request_rate_limits (
                exchange,
                credential_key,
                endpoint_family,
                window_started_at,
                window_seconds,
                request_count,
                max_requests,
                updated_at
            )
            VALUES ($1, $2, $3, NOW(), $4, 1, $5, NOW())
            ON CONFLICT (exchange, credential_key, endpoint_family) DO UPDATE SET
                window_started_at = CASE
                    WHEN exchange_request_rate_limits.window_started_at <= NOW() - ($4::bigint * INTERVAL '1 second')
                    THEN NOW()
                    ELSE exchange_request_rate_limits.window_started_at
                END,
                request_count = CASE
                    WHEN exchange_request_rate_limits.window_started_at <= NOW() - ($4::bigint * INTERVAL '1 second')
                    THEN 1
                    ELSE exchange_request_rate_limits.request_count + 1
                END,
                window_seconds = $4,
                max_requests = $5,
                updated_at = NOW()
            RETURNING request_count
            "#;
const RECORD_EXCHANGE_REQUEST_OUTCOME_SQL: &str = r#"
            INSERT INTO exchange_request_circuit_breakers (
                exchange,
                credential_key,
                endpoint_family,
                state,
                failure_count,
                opened_until,
                last_error,
                updated_at
            )
            VALUES (
                $1,
                $2,
                $3,
                CASE WHEN $4 THEN 'closed' WHEN $5 <= 1 THEN 'open' ELSE 'closed' END,
                CASE WHEN $4 THEN 0 ELSE 1 END,
                CASE WHEN $4 THEN NULL WHEN $5 <= 1 THEN NOW() + ($6::bigint * INTERVAL '1 second') ELSE NULL END,
                $7,
                NOW()
            )
            ON CONFLICT (exchange, credential_key, endpoint_family) DO UPDATE SET
                state = CASE
                    WHEN $4 THEN 'closed'
                    WHEN exchange_request_circuit_breakers.failure_count + 1 >= $5 THEN 'open'
                    ELSE 'closed'
                END,
                failure_count = CASE
                    WHEN $4 THEN 0
                    ELSE exchange_request_circuit_breakers.failure_count + 1
                END,
                opened_until = CASE
                    WHEN $4 THEN NULL
                    WHEN exchange_request_circuit_breakers.failure_count + 1 >= $5
                    THEN NOW() + ($6::bigint * INTERVAL '1 second')
                    ELSE exchange_request_circuit_breakers.opened_until
                END,
                last_error = $7,
                updated_at = NOW()
            RETURNING state
            "#;
const LIST_REPORT_RESULT_REPLAY_CANDIDATES_SQL: &str = r#"
            WITH latest_failed AS (
                SELECT DISTINCT ON (failed.endpoint, failed.request_id)
                    failed.endpoint,
                    failed.request_id,
                    failed.request_payload,
                    failed.created_at
                FROM exchange_request_audit_logs failed
                WHERE failed.endpoint = $1
                  AND failed.request_status = 'failed'
                  AND failed.request_payload #>> '{replay,action}' = 'retry_report_result_only'
                  AND failed.request_payload #>> '{replay,place_order_allowed}' = 'false'
                  AND (
                      cardinality($4::text[]) = 0
                      OR failed.request_payload #>> '{report,task_id}' = ANY($4::text[])
                  )
                  AND NOT EXISTS (
                      SELECT 1
                      FROM exchange_request_audit_logs replayed
                      WHERE replayed.endpoint = failed.endpoint
                        AND replayed.request_id = failed.request_id
                        AND replayed.request_status = 'replayed'
                  )
                ORDER BY failed.endpoint, failed.request_id, failed.created_at DESC, failed.id DESC
            )
            SELECT failed.request_id, failed.request_payload
            FROM latest_failed failed
            WHERE failed.created_at <= NOW() - ($3::bigint * INTERVAL '1 second')
            ORDER BY failed.created_at ASC
            LIMIT $2
            "#;
const LIVE_AUDIT_READINESS_TABLE_SQL: &str = r#"
            SELECT
                to_regclass('public.execution_worker_checkpoints') IS NOT NULL AS has_worker_checkpoints,
                to_regclass('public.exchange_request_audit_logs') IS NOT NULL AS has_exchange_audit_logs,
                to_regclass('public.exchange_request_rate_limits') IS NOT NULL AS has_exchange_rate_limits,
                to_regclass('public.exchange_request_circuit_breakers') IS NOT NULL AS has_exchange_circuit_breakers
            "#;
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionWorkerCheckpoint {
    /// worker ID。
    pub worker_id: String,
    /// 类型标识。
    pub worker_kind: String,
    /// 状态值。
    pub worker_status: String,
    /// 租约owner，用于记录交易或执行状态。
    pub lease_owner: String,
    /// checkpointKey，用于记录交易或执行状态。
    pub checkpoint_key: String,
    /// checkpoint值，用于记录交易或执行状态。
    pub checkpoint_value: Value,
    /// 最近task ID；为空时使用默认值或表示不限制。
    pub last_task_id: Option<String>,
}
impl ExecutionWorkerCheckpoint {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
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
    /// 请求追踪 ID。
    pub request_id: String,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// endpoint。
    pub endpoint: String,
    /// 状态值。
    pub request_status: String,
    /// 毫秒级时间戳或时长。
    pub latency_ms: Option<i32>,
    /// 请求载荷，用于构建接口请求。
    pub request_payload: Value,
    /// 响应载荷，用于返回接口响应。
    pub response_payload: Value,
    /// 错误消息。
    pub error_message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeRequestControlGuard {
    pub exchange: String,
    pub credential_key: String,
    pub endpoint_family: String,
    pub window_seconds: i64,
    pub max_requests: i32,
    pub circuit_failure_threshold: i32,
    pub circuit_open_seconds: i64,
}
impl ExchangeRequestControlGuard {
    pub fn for_task(task: &ExecutionTask, exchange: ExchangeId, endpoint: &str) -> Self {
        Self {
            exchange: exchange.as_str().to_string(),
            credential_key: task
                .request_payload_json
                .get("api_credential_id")
                .and_then(|value| {
                    value
                        .as_i64()
                        .map(|id| id.to_string())
                        .or_else(|| value.as_str().map(str::to_string))
                })
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("credential:{}", value.trim()))
                .unwrap_or_else(|| "credential:unknown".to_string()),
            endpoint_family: endpoint.trim().to_string(),
            window_seconds: DEFAULT_EXCHANGE_REQUEST_WINDOW_SECONDS,
            max_requests: DEFAULT_EXCHANGE_REQUEST_MAX_PER_WINDOW,
            circuit_failure_threshold: DEFAULT_EXCHANGE_CIRCUIT_FAILURE_THRESHOLD,
            circuit_open_seconds: DEFAULT_EXCHANGE_CIRCUIT_OPEN_SECONDS,
        }
    }
}
impl ExchangeRequestAuditLog {
    /// 封装成功，减少Web 商业链路调用方重复实现相同细节。
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
    /// 封装实盘mutationpreflight，减少Web 商业链路调用方重复实现相同细节。
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
    /// 封装失败，减少Web 商业链路调用方重复实现相同细节。
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
    /// 提供报告结果failed的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
    pub fn prepare_order_settings_success(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &PrepareOrderSettingsRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        response_payload: Value,
    ) -> Self {
        Self {
            request_id: prepare_order_settings_request_id(task, exchange, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_PREPARE_ORDER_SETTINGS.to_string(),
            request_status: "completed".to_string(),
            latency_ms,
            request_payload: prepare_order_settings_request_payload(
                task, exchange, request, dry_run,
            ),
            response_payload: redact_audit_payload(response_payload),
            error_message: String::new(),
        }
    }
    /// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
    pub fn prepare_order_settings_live_mutation_preflight(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &PrepareOrderSettingsRequest,
        dry_run: bool,
    ) -> Self {
        Self {
            request_id: prepare_order_settings_request_id(task, exchange, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: format!("{AUDIT_ENDPOINT_PREPARE_ORDER_SETTINGS}.preflight"),
            request_status: "completed".to_string(),
            latency_ms: None,
            request_payload: prepare_order_settings_request_payload(
                task, exchange, request, dry_run,
            ),
            response_payload: redact_audit_payload(json!({
                "stage": "live_prepare_order_settings_audit_preflight",
                "place_order_allowed": false,
                "mutation_allowed": false,
                "audit_write_confirmed": true,
            })),
            error_message: String::new(),
        }
    }
    /// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
    pub fn prepare_order_settings_failed(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &PrepareOrderSettingsRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: prepare_order_settings_request_id(task, exchange, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_PREPARE_ORDER_SETTINGS.to_string(),
            request_status: "failed".to_string(),
            latency_ms,
            request_payload: prepare_order_settings_request_payload(
                task, exchange, request, dry_run,
            ),
            response_payload: json!({}),
            error_message: redact_error_message(error_message.into()),
        }
    }
    /// 提供报告结果replayed的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 提供protective订单success的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn protective_order_success(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &ProtectiveOrderRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        response_payload: Value,
    ) -> Self {
        Self {
            request_id: protective_order_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_PLACE_PROTECTIVE_ORDER.to_string(),
            request_status: "completed".to_string(),
            latency_ms,
            request_payload: protective_order_request_payload(task, exchange, request, dry_run),
            response_payload: redact_audit_payload(response_payload),
            error_message: String::new(),
        }
    }
    /// 提供protective订单livemutationpreflight的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn protective_order_live_mutation_preflight(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &ProtectiveOrderRequest,
        dry_run: bool,
    ) -> Self {
        Self {
            request_id: protective_order_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: format!("{AUDIT_ENDPOINT_PLACE_PROTECTIVE_ORDER}.preflight"),
            request_status: "completed".to_string(),
            latency_ms: None,
            request_payload: protective_order_request_payload(task, exchange, request, dry_run),
            response_payload: redact_audit_payload(json!({
                "stage": "live_protective_order_audit_preflight",
                "place_order_allowed": false,
                "mutation_allowed": false,
                "audit_write_confirmed": true,
            })),
            error_message: String::new(),
        }
    }
    /// 提供protective订单failed的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn protective_order_failed(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &ProtectiveOrderRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: protective_order_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_PLACE_PROTECTIVE_ORDER.to_string(),
            request_status: "failed".to_string(),
            latency_ms,
            request_payload: protective_order_request_payload(task, exchange, request, dry_run),
            response_payload: json!({}),
            error_message: redact_error_message(error_message.into()),
        }
    }
    /// 判断cancel订单success，给Web 商业链路流程提供布尔结果。
    pub fn cancel_order_success(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &CancelOrderRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        response_payload: Value,
    ) -> Self {
        Self {
            request_id: cancel_order_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_CANCEL_ORDER.to_string(),
            request_status: "completed".to_string(),
            latency_ms,
            request_payload: cancel_order_request_payload(task, exchange, request, dry_run),
            response_payload: redact_audit_payload(response_payload),
            error_message: String::new(),
        }
    }
    /// 判断cancel订单livemutationpreflight，给Web 商业链路流程提供布尔结果。
    pub fn cancel_order_live_mutation_preflight(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &CancelOrderRequest,
        dry_run: bool,
    ) -> Self {
        Self {
            request_id: cancel_order_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: format!("{AUDIT_ENDPOINT_CANCEL_ORDER}.preflight"),
            request_status: "completed".to_string(),
            latency_ms: None,
            request_payload: cancel_order_request_payload(task, exchange, request, dry_run),
            response_payload: redact_audit_payload(json!({
                "stage": "live_cancel_order_audit_preflight",
                "place_order_allowed": false,
                "mutation_allowed": false,
                "audit_write_confirmed": true,
            })),
            error_message: String::new(),
        }
    }
    /// 判断cancel订单failed，给Web 商业链路流程提供布尔结果。
    pub fn cancel_order_failed(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &CancelOrderRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: cancel_order_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_CANCEL_ORDER.to_string(),
            request_status: "failed".to_string(),
            latency_ms,
            request_payload: cancel_order_request_payload(task, exchange, request, dry_run),
            response_payload: json!({}),
            error_message: redact_error_message(error_message.into()),
        }
    }
    /// 提供protectivecancelsuccess的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn protective_cancel_success(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &CancelOrderRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        response_payload: Value,
    ) -> Self {
        Self {
            request_id: protective_cancel_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_CANCEL_PROTECTIVE_ORDER.to_string(),
            request_status: "completed".to_string(),
            latency_ms,
            request_payload: protective_cancel_request_payload(task, exchange, request, dry_run),
            response_payload: redact_audit_payload(response_payload),
            error_message: String::new(),
        }
    }
    /// 提供protectivecancellivemutationpreflight的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn protective_cancel_live_mutation_preflight(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &CancelOrderRequest,
        dry_run: bool,
    ) -> Self {
        Self {
            request_id: protective_cancel_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: format!("{AUDIT_ENDPOINT_CANCEL_PROTECTIVE_ORDER}.preflight"),
            request_status: "completed".to_string(),
            latency_ms: None,
            request_payload: protective_cancel_request_payload(task, exchange, request, dry_run),
            response_payload: redact_audit_payload(json!({
                "stage": "live_protective_cancel_audit_preflight",
                "place_order_allowed": false,
                "mutation_allowed": false,
                "audit_write_confirmed": true,
            })),
            error_message: String::new(),
        }
    }
    /// 提供protectivecancelfailed的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn protective_cancel_failed(
        task: &ExecutionTask,
        exchange: ExchangeId,
        request: &CancelOrderRequest,
        dry_run: bool,
        latency_ms: Option<i32>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: protective_cancel_request_id(task, request),
            exchange: exchange.as_str().to_string(),
            symbol: request.instrument.symbol_for(exchange),
            endpoint: AUDIT_ENDPOINT_CANCEL_PROTECTIVE_ORDER.to_string(),
            request_status: "failed".to_string(),
            latency_ms,
            request_payload: protective_cancel_request_payload(task, exchange, request, dry_run),
            response_payload: json!({}),
            error_message: redact_error_message(error_message.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ReportResultReplayCandidate {
    /// 请求追踪 ID。
    pub request_id: String,
    /// 报告。
    pub report: ExecutionTaskReportRequest,
}
#[async_trait]
pub trait ExecutionAuditRepository: Send + Sync {
    fn can_audit_live_mutations(&self) -> bool {
        false
    }
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
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
    async fn acquire_exchange_request_permit(
        &self,
        _guard: &ExchangeRequestControlGuard,
    ) -> Result<()> {
        Ok(())
    }
    async fn record_exchange_request_outcome(
        &self,
        _guard: &ExchangeRequestControlGuard,
        _succeeded: bool,
        _error_message: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }
    async fn list_report_result_replay_candidates(
        &self,
        _limit: u32,
        _failure_backoff_seconds: u64,
        _target_task_ids: &[i64],
    ) -> Result<Vec<ReportResultReplayCandidate>> {
        Ok(Vec::new())
    }
    async fn prune_exchange_request_audit_logs(&self, _retention_days: i64) -> Result<u64> {
        Ok(0)
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
    /// 数据库连接池。
    pool: PgPool,
}
impl PostgresExecutionAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    /// 从外部输入转换为内部模型，隔离 Web 商业、会员和执行准备度 的字段适配细节。
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
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    async fn verify_live_audit_ready(&self) -> Result<()> {
        let row = sqlx::query(LIVE_AUDIT_READINESS_TABLE_SQL)
            .fetch_one(&self.pool)
            .await
            .context("connect quant_core live audit database")?;
        let has_worker_checkpoints: bool = row.try_get("has_worker_checkpoints")?;
        let has_exchange_audit_logs: bool = row.try_get("has_exchange_audit_logs")?;
        let has_exchange_rate_limits: bool = row.try_get("has_exchange_rate_limits")?;
        let has_exchange_circuit_breakers: bool = row.try_get("has_exchange_circuit_breakers")?;
        if !has_worker_checkpoints
            || !has_exchange_audit_logs
            || !has_exchange_rate_limits
            || !has_exchange_circuit_breakers
        {
            return Err(anyhow!(
                "quant_core live audit tables are not ready: execution_worker_checkpoints={}, exchange_request_audit_logs={}, exchange_request_rate_limits={}, exchange_request_circuit_breakers={}",
                has_worker_checkpoints,
                has_exchange_audit_logs,
                has_exchange_rate_limits,
                has_exchange_circuit_breakers
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
        self.prune_exchange_request_audit_logs(exchange_request_audit_retention_days())
            .await
            .context("prune stale exchange_request_audit_logs")?;
        Ok(())
    }
    /// 持久化 Web 商业、会员和执行准备度 结果，保证写入路径和幂等语义集中处理。
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
    /// 持久化 Web 商业、会员和执行准备度 结果，保证写入路径和幂等语义集中处理。
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
    async fn prune_exchange_request_audit_logs(&self, retention_days: i64) -> Result<u64> {
        let retention_days = retention_days.max(1);
        let result = sqlx::query(DELETE_EXCHANGE_REQUEST_AUDIT_RETENTION_SQL)
            .bind(retention_days)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
    async fn acquire_exchange_request_permit(
        &self,
        guard: &ExchangeRequestControlGuard,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(SELECT_EXCHANGE_REQUEST_CIRCUIT_SQL)
            .bind(&guard.exchange)
            .bind(&guard.credential_key)
            .bind(&guard.endpoint_family)
            .fetch_optional(&mut *tx)
            .await?
        {
            let circuit_open: bool = row.try_get("circuit_open")?;
            if circuit_open {
                tx.rollback().await?;
                return Err(anyhow!(
                    "exchange request circuit open for {}/{}/{}",
                    guard.exchange,
                    guard.credential_key,
                    guard.endpoint_family
                ));
            }
        }
        let row = sqlx::query(ACQUIRE_EXCHANGE_REQUEST_PERMIT_SQL)
            .bind(&guard.exchange)
            .bind(&guard.credential_key)
            .bind(&guard.endpoint_family)
            .bind(guard.window_seconds)
            .bind(guard.max_requests)
            .fetch_one(&mut *tx)
            .await?;
        let request_count: i32 = row.try_get("request_count")?;
        if request_count > guard.max_requests {
            tx.rollback().await?;
            return Err(anyhow!(
                "exchange request rate limit exceeded for {}/{}/{}: {}/{} per {}s",
                guard.exchange,
                guard.credential_key,
                guard.endpoint_family,
                request_count,
                guard.max_requests,
                guard.window_seconds
            ));
        }
        tx.commit().await?;
        Ok(())
    }
    async fn record_exchange_request_outcome(
        &self,
        guard: &ExchangeRequestControlGuard,
        succeeded: bool,
        error_message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(RECORD_EXCHANGE_REQUEST_OUTCOME_SQL)
            .bind(&guard.exchange)
            .bind(&guard.credential_key)
            .bind(&guard.endpoint_family)
            .bind(succeeded)
            .bind(guard.circuit_failure_threshold)
            .bind(guard.circuit_open_seconds)
            .bind(error_message.unwrap_or(""))
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
    /// 列出 Web 商业、会员和执行准备度 的候选数据集合，并保持分页、过滤或排序语义集中。
    async fn list_report_result_replay_candidates(
        &self,
        limit: u32,
        failure_backoff_seconds: u64,
        target_task_ids: &[i64],
    ) -> Result<Vec<ReportResultReplayCandidate>> {
        let target_task_ids = target_task_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let rows = sqlx::query(LIST_REPORT_RESULT_REPLAY_CANDIDATES_SQL)
            .bind(AUDIT_ENDPOINT_REPORT_RESULT)
            .bind(i64::from(limit.clamp(1, 100)))
            .bind(i64::try_from(failure_backoff_seconds).unwrap_or(i64::MAX))
            .bind(target_task_ids)
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
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn live_audit_preflight_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("live-audit-preflight-{}-{millis}", std::process::id())
}

fn exchange_request_audit_retention_days() -> i64 {
    std::env::var("QUANT_CORE_EXCHANGE_AUDIT_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_EXCHANGE_REQUEST_AUDIT_RETENTION_DAYS)
}
/// 提供redactaudit载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub fn redact_audit_payload(payload: Value) -> Value {
    match payload {
        Value::Object(map) => Value::Object(redact_object(map)),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_audit_payload).collect())
        }
        other => other,
    }
}
/// 提供redactobject的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
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
/// 提供redacterrormessage的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 提供redactsignedurls的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 封装推进URLstart，减少Web 商业链路调用方重复实现相同细节。
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
/// 提供URLtokenend的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn url_token_end(message: &str, start: usize) -> usize {
    message[start..]
        .char_indices()
        .find_map(|(offset, ch)| {
            matches!(ch, ' ' | '\n' | '\t' | '\r' | ')' | '"' | '\'' | ']' | '}')
                .then_some(start + offset)
        })
        .unwrap_or(message.len())
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
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
/// 提供signature值end的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 封装请求id，减少Web 商业链路调用方重复实现相同细节。
fn request_id(task: &ExecutionTask, request: &OrderPlacementRequest) -> String {
    request
        .client_order_id
        .as_ref()
        .map(|client_order_id| format!("task-{}-{}", task.id, client_order_id))
        .unwrap_or_else(|| format!("task-{}", task.id))
}
/// 提供protective订单requestID的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn protective_order_request_id(task: &ExecutionTask, request: &ProtectiveOrderRequest) -> String {
    request
        .client_order_id
        .as_ref()
        .map(|client_order_id| format!("task-{}-protective-{}", task.id, client_order_id))
        .unwrap_or_else(|| format!("task-{}-protective", task.id))
}
/// 提供protectivecancelrequestID的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn protective_cancel_request_id(task: &ExecutionTask, request: &CancelOrderRequest) -> String {
    request
        .client_order_id
        .as_ref()
        .or(request.order_id.as_ref())
        .map(|order_ref| format!("task-{}-protective-cancel-{}", task.id, order_ref))
        .unwrap_or_else(|| format!("task-{}-protective-cancel", task.id))
}
/// 判断cancel订单requestID，给Web 商业链路流程提供布尔结果。
fn cancel_order_request_id(task: &ExecutionTask, request: &CancelOrderRequest) -> String {
    request
        .client_order_id
        .as_ref()
        .or(request.order_id.as_ref())
        .map(|order_ref| format!("task-{}-cancel-{}", task.id, order_ref))
        .unwrap_or_else(|| format!("task-{}-cancel", task.id))
}
/// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
fn prepare_order_settings_request_id(
    task: &ExecutionTask,
    exchange: ExchangeId,
    request: &PrepareOrderSettingsRequest,
) -> String {
    format!(
        "task-{}-prepare-settings-{}",
        task.id,
        request.instrument.symbol_for(exchange)
    )
}
/// 提供订单request载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 提供protective订单request载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn protective_order_request_payload(
    task: &ExecutionTask,
    exchange: ExchangeId,
    request: &ProtectiveOrderRequest,
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
        "protective_order": {
            "exchange": exchange,
            "symbol": request.instrument.symbol_for(exchange),
            "side": request.side,
            "stop_price": request.stop_price,
            "quantity": request.quantity,
            "position_side": request.position_side,
            "reduce_only": request.reduce_only,
            "close_position": request.close_position,
            "working_type": request.working_type,
            "price_protect": request.price_protect,
            "client_order_id": request.client_order_id,
        }
    }))
}
/// 判断cancel订单request载荷，给Web 商业链路流程提供布尔结果。
fn cancel_order_request_payload(
    task: &ExecutionTask,
    exchange: ExchangeId,
    request: &CancelOrderRequest,
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
        "cancel_order": {
            "exchange": exchange,
            "symbol": request.instrument.symbol_for(exchange),
            "order_id": request.order_id,
            "client_order_id": request.client_order_id,
            "margin_coin": request.margin_coin,
        }
    }))
}
/// 提供protectivecancelrequest载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn protective_cancel_request_payload(
    task: &ExecutionTask,
    exchange: ExchangeId,
    request: &CancelOrderRequest,
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
        "protective_cancel": {
            "exchange": exchange,
            "symbol": request.instrument.symbol_for(exchange),
            "order_id": request.order_id,
            "client_order_id": request.client_order_id,
            "margin_coin": request.margin_coin,
        }
    }))
}
/// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
fn prepare_order_settings_request_payload(
    task: &ExecutionTask,
    exchange: ExchangeId,
    request: &PrepareOrderSettingsRequest,
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
        "account_settings": {
            "exchange": exchange,
            "symbol": request.instrument.symbol_for(exchange),
            "margin_mode": request.margin_mode,
            "leverage": request.leverage,
            "position_mode": request.position_mode,
            "product_type": request.product_type,
            "margin_coin": request.margin_coin,
            "position_side": request.position_side,
        }
    }))
}
/// 提供报告requestID的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn report_request_id(report: &ExecutionTaskReportRequest) -> String {
    let external_order_id = report.external_order_id.trim();
    let candidate = if external_order_id.is_empty() {
        format!("report-task-{}", report.task_id)
    } else {
        format!("report-task-{}-{}", report.task_id, external_order_id)
    };
    candidate.chars().take(128).collect()
}
/// 提供报告结果request载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 提供报告结果replay候选from载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 封装必需i64，减少Web 商业链路调用方重复实现相同细节。
fn required_i64(value: &Value, field: &str) -> Result<i64> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("report replay payload missing numeric field: {field}"))
}
/// 封装必需字符串，减少Web 商业链路调用方重复实现相同细节。
fn required_string(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("report replay payload missing string field: {field}"))
}
/// 提供optionalstring的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn optional_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}
fn optional_f64(value: &Value, field: &str) -> Option<f64> {
    value.get(field).and_then(Value::as_f64)
}
/// 提供redact报告raw载荷JSON的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn redact_report_raw_payload_json(raw: &str) -> Value {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => redact_audit_payload(value),
        Err(_) if contains_sensitive_marker(raw) => Value::String(REDACTED.to_string()),
        Err(_) => Value::String(raw.to_string()),
    }
}
/// 提供containssensitivemarker的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
