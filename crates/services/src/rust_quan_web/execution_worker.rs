use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, Fill, FillListQuery, MarginMode, Order, OrderAck,
    OrderListQuery, OrderQuery, OrderSide, OrderType, Position, PositionMode,
    PrepareOrderSettingsRequest, TimeInForce,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Instant};
use tokio::time::{sleep, Duration};
use tracing::{error, warn};

use crate::exchange::{CryptoExcAllGateway, OrderPlacementRequest};
use crate::rust_quan_web::execution_order_filters::{
    decimal_from_f64, format_order_size_decimal, format_protective_stop_price_decimal,
    load_exchange_order_filters, minimum_order_size, parse_positive_decimal, quantize_order_size,
    quantize_protective_stop_price, ExchangeOrderFilters,
};
use crate::rust_quan_web::execution_payload::{
    close_order_side, direction_from_order_side, ensure_live_order_confirmation,
    format_order_price, format_order_size, is_duplicate_client_order_id_error,
    is_pending_close_task, is_zero_order_size, order_payload, order_side_lower, parse_env_i64_list,
    parse_env_list, parse_env_u32, parse_env_u64, parse_exchange, parse_instrument,
    parse_order_type, parse_position_mode, parse_side, parse_time_in_force, payload_bool,
    payload_f64, payload_string, protection_entry_price, selected_stop_loss_price,
    validate_execute_signal_risk_contract,
};
use crate::rust_quan_web::execution_protection::{
    apply_post_close_protection_cancel_result, attached_stop_loss_order_ack_outcome,
    build_protective_stop_market_order_request, place_and_confirm_protective_order,
    prearm_protective_order_if_required, ProtectionSyncContract, ProtectionSyncOutcome,
};
use crate::rust_quan_web::execution_rollback::{
    apply_protective_failure_rollback_error, apply_protective_failure_rollback_report,
    build_protective_failure_rollback_order_request,
};
use crate::rust_quan_web::{
    ExchangeOrderResult, ExchangeReconciliationIssueType, ExchangeReconciliationReportRequest,
    ExchangeReconciliationReportResponse, ExchangeRequestAuditLog, ExecutionAuditRepository,
    ExecutionTask, ExecutionTaskClient, ExecutionTaskConfig, ExecutionTaskConfirmationLeaseItem,
    ExecutionTaskLeaseRequest, ExecutionTaskReportRequest, ExecutionWorkerCheckpoint,
    NoopExecutionAuditRepository, PostgresExecutionAuditRepository,
};

#[derive(Debug, Clone)]
pub struct ExecutionWorkerConfig {
    pub worker_id: String,
    pub lease_limit: u32,
    pub dry_run: bool,
    pub default_exchange: ExchangeId,
    pub task_types: Vec<String>,
    pub task_statuses: Vec<String>,
    pub target_task_ids: Vec<i64>,
    pub confirmation_mode: bool,
    pub report_replay_mode: bool,
    pub report_replay_max_per_run: u32,
    pub report_replay_failure_backoff_seconds: u64,
    pub report_replay_throttle_ms: u64,
}

impl ExecutionWorkerConfig {
    pub fn from_env() -> Self {
        let worker_id = std::env::var("EXECUTION_WORKER_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "rust_quant".to_string());
        let lease_limit = std::env::var("EXECUTION_WORKER_LEASE_LIMIT")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(10);
        let dry_run = std::env::var("EXECUTION_WORKER_DRY_RUN")
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(true);
        let default_exchange = std::env::var("EXECUTION_WORKER_DEFAULT_EXCHANGE")
            .ok()
            .and_then(|value| parse_exchange(&value).ok())
            .unwrap_or(ExchangeId::Okx);
        let task_types = parse_env_list(
            "EXECUTION_WORKER_TASK_TYPES",
            &["execute_signal", "risk_control_close_candidate"],
        );
        let task_statuses = parse_env_list(
            "EXECUTION_WORKER_TASK_STATUSES",
            &["pending", "pending_close"],
        );
        let target_task_ids = parse_env_i64_list("EXECUTION_WORKER_TARGET_TASK_IDS");
        let confirmation_mode = std::env::var("EXECUTION_WORKER_CONFIRMATION_MODE")
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let report_replay_mode = std::env::var("EXECUTION_WORKER_REPORT_REPLAY_MODE")
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let report_replay_max_per_run =
            parse_env_u32("EXECUTION_WORKER_REPORT_REPLAY_MAX_PER_RUN", lease_limit);
        let report_replay_failure_backoff_seconds = parse_env_u64(
            "EXECUTION_WORKER_REPORT_REPLAY_FAILURE_BACKOFF_SECONDS",
            300,
        );
        let report_replay_throttle_ms =
            parse_env_u64("EXECUTION_WORKER_REPORT_REPLAY_THROTTLE_MS", 0);

        Self {
            worker_id,
            lease_limit,
            dry_run,
            default_exchange,
            task_types,
            task_statuses,
            target_task_ids,
            confirmation_mode,
            report_replay_mode,
            report_replay_max_per_run,
            report_replay_failure_backoff_seconds,
            report_replay_throttle_ms,
        }
    }

    fn leased_task_allowed(&self, task_id: i64) -> bool {
        self.target_task_ids.is_empty() || self.target_task_ids.contains(&task_id)
    }

    pub(crate) fn validate_live_worker_scope(&self) -> Result<()> {
        if self.dry_run || !self.target_task_ids.is_empty() {
            return Ok(());
        }

        Err(anyhow!(
            "refusing live execution worker without EXECUTION_WORKER_TARGET_TASK_IDS; live workers must be scoped to explicit reviewed task ids"
        ))
    }

    fn report_replay_limit(&self) -> u32 {
        self.lease_limit
            .min(self.report_replay_max_per_run)
            .clamp(1, 100)
    }
}

fn reconciliation_only_mode_from_env() -> bool {
    std::env::var("EXECUTION_WORKER_RECONCILIATION_ONLY")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

pub(crate) fn is_protected_link_symbol(symbol: &str) -> bool {
    let normalized: String = symbol
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    normalized == "LINKUSDT" || normalized.starts_with("LINKUSDT")
}

fn api_credential_id_from_task(task: &ExecutionTask) -> Option<i64> {
    let payload = order_payload(&task.request_payload_json);
    payload_string(&payload, "api_credential_id")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

fn api_credential_exchange_matches_task(
    credential_exchange: &str,
    task_exchange: ExchangeId,
) -> bool {
    let normalized = credential_exchange.trim();
    if normalized == "币安" {
        return task_exchange == ExchangeId::Binance;
    }
    normalized
        .parse::<ExchangeId>()
        .is_ok_and(|exchange| exchange == task_exchange)
}

pub struct ExecutionWorker {
    client: ExecutionTaskClient,
    gateway: CryptoExcAllGateway,
    config: ExecutionWorkerConfig,
    audit_repository: Arc<dyn ExecutionAuditRepository>,
}

impl ExecutionWorker {
    pub fn new(
        client: ExecutionTaskClient,
        gateway: CryptoExcAllGateway,
        config: ExecutionWorkerConfig,
    ) -> Self {
        Self {
            client,
            gateway,
            config,
            audit_repository: Arc::new(NoopExecutionAuditRepository),
        }
    }

    pub fn with_audit_repository(
        mut self,
        audit_repository: Arc<dyn ExecutionAuditRepository>,
    ) -> Self {
        self.audit_repository = audit_repository;
        self
    }

    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
            .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
            .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
        let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
            .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
            .unwrap_or_default();
        let config = ExecutionWorkerConfig::from_env();
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url,
            internal_secret,
        })?;
        let reconciliation_only_mode = reconciliation_only_mode_from_env();
        if !reconciliation_only_mode {
            config.validate_live_worker_scope()?;
        }
        let gateway = if config.dry_run || reconciliation_only_mode {
            CryptoExcAllGateway::dry_run()
        } else {
            ensure_live_order_confirmation()?;
            CryptoExcAllGateway::from_env()?
        };
        let mut worker = Self::new(client, gateway, config);
        if let Some(repository) = PostgresExecutionAuditRepository::from_env()? {
            worker = worker.with_audit_repository(Arc::new(repository));
        }

        Ok(worker)
    }

    pub async fn run_once(&self) -> Result<usize> {
        if self.config.report_replay_mode {
            return self.run_report_replay_once().await;
        }
        if self.config.confirmation_mode {
            return self.run_confirmation_once().await;
        }
        if reconciliation_only_mode_from_env() {
            return self.run_reconciliation_only_once().await;
        }

        self.record_checkpoint(
            "leasing",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
                "default_exchange": self.config.default_exchange.as_str(),
                "task_types": self.config.task_types.clone(),
                "task_statuses": self.config.task_statuses.clone(),
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let leased = match self
            .client
            .lease_tasks(ExecutionTaskLeaseRequest {
                worker_id: self.config.worker_id.clone(),
                limit: self.config.lease_limit,
                task_ids: self.config.target_task_ids.clone(),
                task_types: self.config.task_types.clone(),
                task_statuses: self.config.task_statuses.clone(),
            })
            .await
        {
            Ok(leased) => leased,
            Err(error) => {
                self.record_checkpoint(
                    "failed",
                    None,
                    json!({
                        "stage": "lease_tasks",
                        "error": error.to_string(),
                    }),
                )
                .await;
                return Err(error);
            }
        };

        self.record_checkpoint(
            "leased",
            None,
            json!({
                "leased_count": leased.tasks.len(),
                "lease_limit": self.config.lease_limit,
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let mut handled = 0;
        let mut last_task_id = None;
        for task in leased.tasks {
            if !self.config.leased_task_allowed(task.id) {
                warn!(
                    task_id = task.id,
                    target_task_ids = ?self.config.target_task_ids,
                    "leased execution task is outside EXECUTION_WORKER_TARGET_TASK_IDS; skipping order execution"
                );
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(task.id),
                    json!({
                        "stage": "target_task_allowlist",
                        "task_id": task.id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                    }),
                )
                .await;
                continue;
            }

            let report = self.execute_task(&task).await;
            let report_status = report.execution_status.clone();
            if let Err(error) = self.client.report_result(report.clone()).await {
                error!(task_id = task.id, "回写执行任务结果失败: {}", error);
                self.record_report_result_failure(
                    task.id,
                    &report,
                    error.to_string(),
                    "report_result",
                )
                .await;
            } else {
                self.record_checkpoint(
                    &report_status,
                    Some(task.id),
                    json!({
                        "stage": "report_result",
                        "execution_status": report_status,
                    }),
                )
                .await;
            }
            last_task_id = Some(task.id);
            handled += 1;
        }
        self.record_checkpoint(
            "idle",
            last_task_id,
            json!({
                "handled": handled,
                "dry_run": self.config.dry_run,
            }),
        )
        .await;
        Ok(handled)
    }

    pub async fn report_exchange_reconciliation_for_task(
        &self,
        task: &ExecutionTask,
        issue_type: ExchangeReconciliationIssueType,
        detected_at: Option<String>,
        message: impl Into<String>,
    ) -> Result<ExchangeReconciliationReportResponse> {
        let request =
            build_exchange_reconciliation_report_request(task, issue_type, detected_at, message);
        let response = self
            .client
            .report_exchange_reconciliation(request.clone())
            .await?;
        self.record_checkpoint(
            "exchange_reconciliation_reported",
            Some(task.id),
            json!({
                "stage": "exchange_reconciliation",
                "combo_id": request.combo_id,
                "buyer_email": request.buyer_email,
                "symbol": request.symbol,
                "issue_type": request.issue_type.as_str(),
                "source_ref": request.source_ref,
                "place_order_allowed": false,
            }),
        )
        .await;
        Ok(response)
    }

    async fn check_exchange_reconciliation_before_live_order(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
        gateway: &CryptoExcAllGateway,
    ) -> Result<Option<ExecutionTaskReportRequest>> {
        let instrument = parse_instrument(&order_task.symbol)?;
        let positions = gateway
            .positions(order_task.exchange, Some(&instrument))
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange position reconciliation failed before live order: {}",
                    error
                )
            })?;
        let open_orders = gateway
            .open_orders(
                order_task.exchange,
                OrderListQuery::for_instrument(instrument).with_limit(100),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange open-order reconciliation failed before live order: {}",
                    error
                )
            })?;
        let requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
            task,
            &positions,
            &open_orders,
            None,
        );
        if requests.is_empty() {
            return Ok(None);
        }

        for request in &requests {
            self.client
                .report_exchange_reconciliation(request.clone())
                .await?;
            self.record_checkpoint(
                "exchange_reconciliation_read_only_blocker_reported",
                Some(task.id),
                json!({
                    "stage": "exchange_reconciliation_read_only",
                    "combo_id": request.combo_id,
                    "buyer_email": request.buyer_email,
                    "symbol": request.symbol,
                    "issue_type": request.issue_type.as_str(),
                    "source_ref": request.source_ref,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            )
            .await;
        }

        Ok(Some(
            build_live_order_blocked_by_exchange_reconciliation_report(task, order_task, &requests),
        ))
    }

    async fn check_exchange_read_only_before_pending_close(
        &self,
        task: &ExecutionTask,
        request: &OrderPlacementRequest,
        gateway: &CryptoExcAllGateway,
    ) -> Result<()> {
        let instrument = request.instrument.clone();
        let positions = gateway
            .positions(request.exchange, Some(&instrument))
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange position reconciliation failed before pending close: {}",
                    error
                )
            })?;
        let open_orders = gateway
            .open_orders(
                request.exchange,
                OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange open-order reconciliation failed before pending close: {}",
                    error
                )
            })?;
        self.record_checkpoint(
            "pending_close_exchange_reconciliation_read_only_checked",
            Some(task.id),
            json!({
                "stage": "pending_close_exchange_reconciliation_read_only",
                "exchange": request.exchange.as_str(),
                "symbol": instrument.symbol_for(request.exchange),
                "position_count": positions.len(),
                "open_order_count": open_orders.len(),
                "place_order_allowed": true,
                "mutation_allowed": false,
            }),
        )
        .await;

        Ok(())
    }

    async fn run_confirmation_once(&self) -> Result<usize> {
        self.record_checkpoint(
            "leasing_confirmations",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let leased = match self
            .client
            .lease_confirmation_tasks(self.config.lease_limit)
            .await
        {
            Ok(leased) => leased,
            Err(error) => {
                self.record_checkpoint(
                    "failed",
                    None,
                    json!({
                        "stage": "lease_confirmation_tasks",
                        "error": error.to_string(),
                    }),
                )
                .await;
                return Err(error);
            }
        };

        self.record_checkpoint(
            "confirmations_leased",
            None,
            json!({
                "leased_count": leased.items.len(),
                "lease_limit": self.config.lease_limit,
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let mut handled = 0;
        let mut last_task_id = None;
        for item in leased.items {
            if !self.config.leased_task_allowed(item.task.id) {
                warn!(
                    task_id = item.task.id,
                    target_task_ids = ?self.config.target_task_ids,
                    "leased pending confirmation task is outside EXECUTION_WORKER_TARGET_TASK_IDS; skipping confirmation"
                );
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(item.task.id),
                    json!({
                        "stage": "confirmation_target_task_allowlist",
                        "task_id": item.task.id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                    }),
                )
                .await;
                continue;
            }

            let report = self.execute_pending_confirmation_item(&item).await;
            let report_status = report.execution_status.clone();
            if let Err(error) = self.client.report_result(report.clone()).await {
                error!(task_id = item.task.id, "回写执行确认结果失败: {}", error);
                self.record_report_result_failure(
                    item.task.id,
                    &report,
                    error.to_string(),
                    "report_confirmation_result",
                )
                .await;
            } else {
                self.record_checkpoint(
                    &report_status,
                    Some(item.task.id),
                    json!({
                        "stage": "report_confirmation_result",
                        "execution_status": report_status,
                    }),
                )
                .await;
            }
            last_task_id = Some(item.task.id);
            handled += 1;
        }

        self.record_checkpoint(
            "idle",
            last_task_id,
            json!({
                "handled": handled,
                "confirmation_mode": true,
            }),
        )
        .await;
        Ok(handled)
    }

    async fn run_report_replay_once(&self) -> Result<usize> {
        let replay_limit = self.config.report_replay_limit();
        let failure_backoff_seconds = self.config.report_replay_failure_backoff_seconds;
        let throttle_ms = self.config.report_replay_throttle_ms;
        self.record_checkpoint(
            "leasing_report_replays",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "replay_limit": replay_limit,
                "report_replay_max_per_run": self.config.report_replay_max_per_run,
                "failure_backoff_seconds": failure_backoff_seconds,
                "throttle_ms": throttle_ms,
                "target_task_ids": self.config.target_task_ids.clone(),
                "place_order_allowed": false,
            }),
        )
        .await;

        let candidates = self
            .audit_repository
            .list_report_result_replay_candidates(replay_limit, failure_backoff_seconds)
            .await?;
        let leased_count = candidates.len();

        self.record_checkpoint(
            "report_replays_leased",
            None,
            json!({
                "leased_count": leased_count,
                "lease_limit": self.config.lease_limit,
                "replay_limit": replay_limit,
                "failure_backoff_seconds": failure_backoff_seconds,
                "throttle_ms": throttle_ms,
                "target_task_ids": self.config.target_task_ids.clone(),
                "place_order_allowed": false,
            }),
        )
        .await;

        let mut handled = 0;
        let mut replayed = 0;
        let mut failed = 0;
        let mut skipped_target_task_mismatch = 0;
        let mut last_task_id = None;
        let mut request_ids = Vec::new();
        let mut replayed_request_ids = Vec::new();
        let mut failed_request_ids = Vec::new();
        let mut skipped_request_ids = Vec::new();
        for candidate in candidates {
            request_ids.push(candidate.request_id.clone());
            if !self.config.leased_task_allowed(candidate.report.task_id) {
                skipped_target_task_mismatch += 1;
                skipped_request_ids.push(candidate.request_id.clone());
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(candidate.report.task_id),
                    json!({
                        "stage": "report_replay_target_task_allowlist",
                        "request_id": candidate.request_id,
                        "task_id": candidate.report.task_id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                        "place_order_allowed": false,
                    }),
                )
                .await;
                continue;
            }

            if handled > 0 && throttle_ms > 0 {
                sleep(Duration::from_millis(throttle_ms)).await;
            }
            let task_id = candidate.report.task_id;
            let started_at = Instant::now();
            match self.client.report_result(candidate.report.clone()).await {
                Ok(response) => {
                    replayed += 1;
                    replayed_request_ids.push(candidate.request_id.clone());
                    let latency_ms = started_at.elapsed().as_millis().min(i32::MAX as u128) as i32;
                    self.record_exchange_request_audit(
                        ExchangeRequestAuditLog::report_result_replayed(
                            &candidate.report,
                            Some(latency_ms),
                            serde_json::to_value(&response).unwrap_or_else(|_| json!({})),
                        ),
                    )
                    .await;
                    self.record_checkpoint(
                        "report_replayed",
                        Some(task_id),
                        json!({
                            "stage": "report_replay",
                            "request_id": candidate.request_id,
                            "task_id": task_id,
                            "execution_status": candidate.report.execution_status,
                            "order_status": candidate.report.order_status,
                            "place_order_allowed": false,
                        }),
                    )
                    .await;
                }
                Err(error) => {
                    failed += 1;
                    failed_request_ids.push(candidate.request_id.clone());
                    self.record_report_result_failure(
                        task_id,
                        &candidate.report,
                        error.to_string(),
                        "report_replay",
                    )
                    .await;
                }
            }

            last_task_id = Some(task_id);
            handled += 1;
        }

        let health_status = if failed > 0 { "warn" } else { "ok" };
        let health_code = if failed > 0 {
            "QUANT_REPORT_REPLAY_FAILED"
        } else {
            "QUANT_REPORT_REPLAY_READY"
        };
        let batch_payload = json!({
            "handled": handled,
            "report_replay_mode": true,
            "place_order_allowed": false,
            "report_replay": {
                "leased_count": leased_count,
                "attempted_count": handled,
                "replayed_count": replayed,
                "failed_count": failed,
                "skipped_target_task_mismatch_count": skipped_target_task_mismatch,
                "lease_limit": self.config.lease_limit,
                "replay_limit": replay_limit,
                "max_per_run": self.config.report_replay_max_per_run,
                "failure_backoff_seconds": failure_backoff_seconds,
                "throttle_ms": throttle_ms,
                "request_ids": request_ids,
                "replayed_request_ids": replayed_request_ids,
                "failed_request_ids": failed_request_ids,
                "skipped_request_ids": skipped_request_ids,
            },
            "health_handoff": {
                "section": "quant_worker_checkpoint_audit",
                "status": health_status,
                "code": health_code,
                "read_only_input": false,
            },
            "operator_playbook_summary": report_replay_operator_playbook_summary(
                failed,
                failure_backoff_seconds,
            ),
        });
        self.record_checkpoint(
            if failed > 0 {
                "report_replay_batch_degraded"
            } else {
                "report_replay_batch_completed"
            },
            last_task_id,
            batch_payload.clone(),
        )
        .await;
        self.record_checkpoint("idle", last_task_id, batch_payload)
            .await;
        Ok(handled)
    }

    async fn execute_task(&self, task: &ExecutionTask) -> ExecutionTaskReportRequest {
        if is_pending_close_task(task) {
            return self.execute_pending_close_task(task).await;
        }

        let order_task =
            match ExecutionOrderTask::from_task_with_default(task, self.config.default_exchange) {
                Ok(value) => value,
                Err(error) => {
                    return ExecutionTaskReportRequest::failed(
                        task.id,
                        self.config.default_exchange.as_str(),
                        "unknown",
                        error.to_string(),
                        json!({"task_id": task.id}),
                    );
                }
            };
        if let Err(mut violation) = validate_execute_signal_risk_contract(task, &order_task) {
            if let Some(contract) = violation
                .raw_payload
                .get_mut("risk_contract")
                .and_then(Value::as_object_mut)
            {
                contract.insert("worker_dry_run".to_string(), json!(self.config.dry_run));
            }
            return ExecutionTaskReportRequest::failed(
                task.id,
                order_task.exchange.as_str(),
                order_side_lower(order_task.side),
                violation.message,
                violation.raw_payload,
            );
        }

        if let Ok(request) = order_task.to_order_request() {
            if let Some(report) = client_order_id_owner_violation_report(
                task.id,
                task.task_type.as_str(),
                order_side_lower(order_task.side),
                &request,
            ) {
                return report;
            }
        }

        if self.config.dry_run {
            return match order_task.to_order_request() {
                Ok(request) => match self
                    .place_order_with_audit(task, &self.gateway, request)
                    .await
                {
                    Ok(ack) => {
                        let mut report = ExecutionTaskReportRequest::success(
                            task.id,
                            ack.exchange.as_str(),
                            ack.order_id
                                .as_deref()
                                .or(ack.client_order_id.as_deref())
                                .unwrap_or("dry_run"),
                            order_side_lower(order_task.side),
                            ack.status.as_deref().unwrap_or("dry_run"),
                            ack.raw,
                        );
                        if let Some(protection) = ProtectionSyncContract::from_task(
                            task,
                            order_side_lower(order_task.side),
                        ) {
                            protection.apply_outcome_to_report(
                                &mut report,
                                ProtectionSyncOutcome::uncertain(
                                    "dry_run_protection_sync_not_confirmed",
                                    "dry-run order does not create a real protective stop-loss order",
                                ),
                            );
                        }
                        report
                    }
                    Err(error) => ExecutionTaskReportRequest::failed(
                        task.id,
                        order_task.exchange.as_str(),
                        order_side_lower(order_task.side),
                        error.to_string(),
                        json!({"task_id": task.id}),
                    ),
                },
                Err(error) => ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({"task_id": task.id}),
                ),
            };
        }

        if let Some(report) = self
            .live_api_credential_preflight_report(task, &order_task)
            .await
        {
            return report;
        }

        let gateway = match self
            .resolve_live_gateway(&task.buyer_email, order_task.exchange)
            .await
        {
            Ok(gateway) => gateway,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({"task_id": task.id}),
                );
            }
        };

        match self
            .check_exchange_reconciliation_before_live_order(task, &order_task, &gateway)
            .await
        {
            Ok(Some(report)) => return report,
            Ok(None) => {}
            Err(error) => {
                return build_live_order_blocked_by_exchange_reconciliation_read_error_report(
                    task,
                    &order_task,
                    error.to_string(),
                );
            }
        }

        let mut request = match self.live_order_request(&gateway, &order_task).await {
            Ok(request) => request,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "live_order_read_only_request_build",
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        };

        if order_task.exchange == ExchangeId::Binance
            && (order_task.margin_mode.is_some()
                || order_task.leverage.is_some()
                || order_task.position_mode.is_some())
        {
            let instrument = match parse_instrument(&order_task.symbol) {
                Ok(value) => value,
                Err(error) => {
                    return ExecutionTaskReportRequest::failed(
                        task.id,
                        order_task.exchange.as_str(),
                        order_side_lower(order_task.side),
                        error.to_string(),
                        json!({"task_id": task.id}),
                    );
                }
            };
            let prepare = PrepareOrderSettingsRequest {
                instrument,
                margin_mode: order_task.margin_mode.clone(),
                leverage: order_task.leverage.clone(),
                position_mode: order_task.position_mode,
                product_type: None,
                margin_coin: order_task.margin_coin.clone(),
                position_side: order_task.position_side.clone(),
            };
            if let Err(error) = gateway
                .prepare_order_settings(order_task.exchange, prepare)
                .await
            {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({"task_id": task.id}),
                );
            }
        }

        let protection = ProtectionSyncContract::from_task(task, order_side_lower(order_task.side));
        match self
            .pre_place_client_order_report(
                task,
                &gateway,
                Some(&order_task),
                order_side_lower(order_task.side),
                &request,
                protection.clone(),
            )
            .await
        {
            Ok(Some(report)) => return report,
            Ok(None) => {}
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "client_order_id_pre_place_check",
                        "exchange": order_task.exchange.as_str(),
                        "symbol": order_task.symbol,
                        "client_order_id": request.client_order_id.clone(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        }
        let prearmed_protection =
            match prearm_protective_order_if_required(&gateway, &order_task, protection.as_ref())
                .await
            {
                Ok(value) => value,
                Err((protection, outcome)) => {
                    let mut report = ExecutionTaskReportRequest::failed(
                        task.id,
                        order_task.exchange.as_str(),
                        order_side_lower(order_task.side),
                        "prearmed protective stop-loss was not confirmed; refusing main order",
                        json!({
                            "task_id": task.id,
                            "stage": "prearmed_protective_order",
                            "exchange": order_task.exchange.as_str(),
                            "symbol": order_task.symbol,
                            "main_order_placed": false,
                            "place_order_allowed": false,
                            "mutation_allowed": false,
                        }),
                    );
                    protection.apply_outcome_to_report(&mut report, outcome);
                    return report;
                }
            };
        let post_fill_protection = if prearmed_protection.is_some() {
            request.attached_stop_loss_price = None;
            None
        } else {
            protection.clone()
        };
        match self
            .place_order_with_audit(task, &gateway, request.clone())
            .await
        {
            Ok(ack) => {
                let mut report = self
                    .confirmed_live_order_report(
                        task,
                        &gateway,
                        Some(&order_task),
                        order_side_lower(order_task.side),
                        ack,
                        post_fill_protection.clone(),
                    )
                    .await;
                if let Some(prearmed) = &prearmed_protection {
                    prearmed.apply_after_main_order_report(&mut report);
                }
                report
            }
            Err(error) if is_duplicate_client_order_id_error(&error.to_string()) => {
                let mut report = self
                    .duplicate_client_order_id_report(
                        task,
                        &gateway,
                        Some(&order_task),
                        order_side_lower(order_task.side),
                        &request,
                        post_fill_protection,
                    )
                    .await;
                if let Some(prearmed) = &prearmed_protection {
                    prearmed.apply_after_main_order_report(&mut report);
                }
                report
            }
            Err(error) => {
                let mut report = ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "place_order",
                        "prearmed_protective_order": prearmed_protection.is_some(),
                    }),
                );
                if let Some(prearmed) = &prearmed_protection {
                    let cancel_result = prearmed.cancel_after_main_order_failure(&gateway).await;
                    prearmed.apply_main_order_failure_cancel_result(
                        &mut report,
                        &error.to_string(),
                        cancel_result,
                    );
                }
                report
            }
        }
    }

    async fn execute_pending_close_task(&self, task: &ExecutionTask) -> ExecutionTaskReportRequest {
        let close_task = match PendingCloseTask::from_task(task, self.config.default_exchange) {
            Ok(value) => value,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    self.config.default_exchange.as_str(),
                    "close",
                    error.to_string(),
                    json!({"task_id": task.id, "task_type": task.task_type}),
                );
            }
        };

        if self.config.dry_run {
            return match close_task.to_order_request() {
                Ok(Some(request)) => match self
                    .place_order_with_audit(task, &self.gateway, request.clone())
                    .await
                {
                    Ok(ack) => ExecutionTaskReportRequest::success(
                        task.id,
                        ack.exchange.as_str(),
                        ack.order_id
                            .as_deref()
                            .or(ack.client_order_id.as_deref())
                            .unwrap_or("dry_run"),
                        order_side_lower(request.side),
                        ack.status.as_deref().unwrap_or("dry_run"),
                        ack.raw,
                    ),
                    Err(error) => ExecutionTaskReportRequest::failed(
                        task.id,
                        close_task.exchange.as_str(),
                        "close",
                        error.to_string(),
                        close_task.report_payload(true),
                    ),
                },
                Ok(None) => close_task.dry_run_report(),
                Err(error) => ExecutionTaskReportRequest::failed(
                    task.id,
                    close_task.exchange.as_str(),
                    "close",
                    error.to_string(),
                    close_task.report_payload(true),
                ),
            };
        }

        let request = match close_task.to_order_request() {
            Ok(Some(request)) => request,
            Ok(None) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    close_task.exchange.as_str(),
                    "close",
                    close_task.missing_live_contract_message(),
                    close_task.report_payload(false),
                );
            }
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    close_task.exchange.as_str(),
                    "close",
                    error.to_string(),
                    close_task.report_payload(false),
                );
            }
        };
        if let Some(report) = client_order_id_owner_violation_report(
            task.id,
            task.task_type.as_str(),
            order_side_lower(request.side),
            &request,
        ) {
            return report;
        }
        let gateway = match self
            .resolve_live_gateway(&task.buyer_email, request.exchange)
            .await
        {
            Ok(gateway) => gateway,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    request.exchange.as_str(),
                    order_side_lower(request.side),
                    error.to_string(),
                    close_task.report_payload(false),
                );
            }
        };

        if let Err(error) = self
            .check_exchange_read_only_before_pending_close(task, &request, &gateway)
            .await
        {
            let symbol = request.instrument.symbol_for(request.exchange);
            let source_ref = build_exchange_reconciliation_source_ref(
                task,
                request.exchange.as_str(),
                &symbol,
                "pending_close_gateway_read_failed",
            );
            return ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side_lower(request.side),
                format!(
                    "pending close blocked because read-only exchange reconciliation failed before live close: {error}; place_order_allowed=false; mutation_allowed=false"
                ),
                json!({
                    "task_id": task.id,
                    "stage": "pending_close_exchange_reconciliation_read_only",
                    "exchange": request.exchange.as_str(),
                    "symbol": symbol,
                    "source_ref": source_ref,
                    "gateway_read_failed": true,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                    "place_order_retried": false,
                }),
            );
        }

        match self
            .pre_place_client_order_report(
                task,
                &gateway,
                None,
                order_side_lower(request.side),
                &request,
                None,
            )
            .await
        {
            Ok(Some(report)) => return report,
            Ok(None) => {}
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    request.exchange.as_str(),
                    order_side_lower(request.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "client_order_id_pre_place_check",
                        "exchange": request.exchange.as_str(),
                        "symbol": request.instrument.symbol_for(request.exchange),
                        "client_order_id": request.client_order_id.clone(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        }

        match self
            .place_order_with_audit(task, &gateway, request.clone())
            .await
        {
            Ok(ack) => {
                let mut report = self
                    .confirmed_live_order_report(
                        task,
                        &gateway,
                        None,
                        order_side_lower(request.side),
                        ack,
                        None,
                    )
                    .await;
                if report.order_status.trim().eq_ignore_ascii_case("FILLED") {
                    if let Ok(Some((exchange, cancel_request))) =
                        close_task.protective_cancel_request()
                    {
                        let cancel_result = gateway.cancel_order(exchange, cancel_request).await;
                        apply_post_close_protection_cancel_result(&mut report, cancel_result);
                    }
                }
                report
            }
            Err(error) if is_duplicate_client_order_id_error(&error.to_string()) => {
                self.duplicate_client_order_id_report(
                    task,
                    &gateway,
                    None,
                    order_side_lower(request.side),
                    &request,
                    None,
                )
                .await
            }
            Err(error) => ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side_lower(request.side),
                error.to_string(),
                close_task.report_payload(false),
            ),
        }
    }

    async fn execute_pending_confirmation_item(
        &self,
        item: &ExecutionTaskConfirmationLeaseItem,
    ) -> ExecutionTaskReportRequest {
        let pending =
            match PendingConfirmationTask::from_confirmation_item(&item.task, &item.order_result) {
                Ok(value) => value,
                Err(error) => {
                    return ExecutionTaskReportRequest::failed(
                        item.task.id,
                        item.order_result.exchange.as_str(),
                        item.order_result.order_side.as_str(),
                        error.to_string(),
                        json!({
                            "task_id": item.task.id,
                            "order_result_id": item.order_result.id,
                            "confirmation_stage": "parse_pending_confirmation",
                        }),
                    );
                }
            };

        if self.config.dry_run {
            return pending.pending_report(
                "pending confirmation requires live read-only order lookup",
                json!({
                    "task_id": item.task.id,
                    "order_result_id": item.order_result.id,
                    "confirmation_stage": "dry_run_blocked",
                }),
            );
        }

        let gateway = match self
            .resolve_live_gateway(&item.task.buyer_email, pending.exchange)
            .await
        {
            Ok(gateway) => gateway,
            Err(error) => {
                return pending.pending_report(
                    error.to_string(),
                    json!({
                        "task_id": item.task.id,
                        "order_result_id": item.order_result.id,
                        "confirmation_stage": "resolve_live_gateway",
                    }),
                );
            }
        };
        let query = match pending.to_order_query() {
            Ok(query) => query,
            Err(error) => {
                return pending.pending_report(
                    error.to_string(),
                    json!({
                        "task_id": item.task.id,
                        "order_result_id": item.order_result.id,
                        "confirmation_stage": "build_order_query",
                    }),
                );
            }
        };

        let order = match gateway.order(pending.exchange, query).await {
            Ok(order) => order,
            Err(error) => {
                return pending.pending_report(
                    error.to_string(),
                    json!({
                        "task_id": item.task.id,
                        "order_result_id": item.order_result.id,
                        "confirmation_stage": "query_order",
                    }),
                );
            }
        };
        let order_id = order.order_id.as_deref().or_else(|| {
            pending
                .external_order_id
                .as_deref()
                .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        });
        let fills = if let Some(order_id) = order_id {
            match gateway
                .fills(
                    pending.exchange,
                    FillListQuery::for_instrument(order.instrument.clone())
                        .with_order_id(order_id)
                        .with_limit(100),
                )
                .await
            {
                Ok(fills) => fills,
                Err(error) => {
                    warn!(
                        exchange = pending.exchange.as_str(),
                        order_id, "pending confirmation fills query failed: {}", error
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let ack = pending.to_order_ack(Some(&order));
        build_confirmed_order_report(
            item.task.id,
            pending.order_side.as_str(),
            &ack,
            Some(order),
            fills,
            None,
            None,
        )
    }

    async fn live_order_request(
        &self,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<OrderPlacementRequest> {
        let instrument = parse_instrument(&order_task.symbol)?;
        let ticker = gateway.ticker(order_task.exchange, &instrument).await?;
        let last_price = ticker.last_price.trim().parse::<f64>().map_err(|err| {
            anyhow!(
                "invalid ticker last_price for {} on {}: {}",
                order_task.symbol,
                order_task.exchange.as_str(),
                err
            )
        })?;
        let filters = load_exchange_order_filters(order_task.exchange, &order_task.symbol).await?;
        order_task.to_live_order_request(Some(last_price), filters.as_ref())
    }

    async fn resolve_live_gateway(
        &self,
        buyer_email: &str,
        exchange: ExchangeId,
    ) -> Result<CryptoExcAllGateway> {
        let config = self
            .client
            .resolve_user_exchange_config(buyer_email, exchange.as_str())
            .await?;
        CryptoExcAllGateway::from_single_exchange_credentials(
            exchange,
            config.api_key,
            config.api_secret,
            config.passphrase,
            config.simulated,
        )
        .map_err(Into::into)
    }

    async fn live_api_credential_preflight_report(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
    ) -> Option<ExecutionTaskReportRequest> {
        let credential_id = api_credential_id_from_task(task)?;
        let checked = match self
            .client
            .check_internal_api_credential(credential_id)
            .await
        {
            Ok(checked) => checked,
            Err(error) => {
                return Some(ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    format!(
                        "API credential preflight failed before live order: {error}; place_order_allowed=false; mutation_allowed=false"
                    ),
                    json!({
                        "task_id": task.id,
                        "stage": "api_credential_preflight",
                        "api_credential_id": credential_id,
                        "exchange": order_task.exchange.as_str(),
                        "symbol": order_task.symbol,
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                ));
            }
        };

        if !api_credential_exchange_matches_task(&checked.exchange, order_task.exchange) {
            return Some(ExecutionTaskReportRequest::failed(
                task.id,
                order_task.exchange.as_str(),
                order_side_lower(order_task.side),
                format!(
                    "API credential preflight returned exchange {} for task exchange {}; place_order_allowed=false; mutation_allowed=false",
                    checked.exchange,
                    order_task.exchange.as_str()
                ),
                json!({
                    "task_id": task.id,
                    "stage": "api_credential_preflight",
                    "api_credential_id": credential_id,
                    "credential_exchange": checked.exchange,
                    "task_exchange": order_task.exchange.as_str(),
                    "symbol": order_task.symbol,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            ));
        }

        if checked.execution_readiness.can_execute {
            return None;
        }

        let blocker_code = checked
            .execution_readiness
            .blocker_code
            .as_deref()
            .or(checked.last_check_code.as_deref())
            .unwrap_or("api_credential_not_ready");
        let blocker_message = checked
            .execution_readiness
            .blocker_message
            .as_deref()
            .or(checked.last_check_message.as_deref())
            .unwrap_or("API credential is not ready for live execution");

        Some(ExecutionTaskReportRequest::failed(
            task.id,
            order_task.exchange.as_str(),
            order_side_lower(order_task.side),
            format!(
                "API credential preflight blocked live order: {blocker_code}: {blocker_message}; place_order_allowed=false; mutation_allowed=false"
            ),
            json!({
                "task_id": task.id,
                "stage": "api_credential_preflight",
                "api_credential_id": credential_id,
                "exchange": order_task.exchange.as_str(),
                "symbol": order_task.symbol,
                "last_check_code": checked.last_check_code,
                "blocker_code": checked.execution_readiness.blocker_code,
                "blocker_message": checked.execution_readiness.blocker_message,
                "place_order_allowed": false,
                "mutation_allowed": false,
            }),
        ))
    }

    async fn place_order_with_audit(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        request: OrderPlacementRequest,
    ) -> crypto_exc_all::Result<OrderAck> {
        let started_at = Instant::now();
        let result = gateway.place_order(request.clone()).await;
        let latency_ms = elapsed_ms(started_at);

        match &result {
            Ok(ack) => {
                self.record_exchange_request_audit(ExchangeRequestAuditLog::success(
                    task,
                    &request,
                    self.config.dry_run,
                    latency_ms,
                    ack.raw.clone(),
                ))
                .await;
            }
            Err(error) => {
                self.record_exchange_request_audit(ExchangeRequestAuditLog::failed(
                    task,
                    &request,
                    self.config.dry_run,
                    latency_ms,
                    error.to_string(),
                ))
                .await;
            }
        }

        result
    }

    async fn confirmed_live_order_report(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: Option<&ExecutionOrderTask>,
        order_side: &str,
        ack: OrderAck,
        protection: Option<ProtectionSyncContract>,
    ) -> ExecutionTaskReportRequest {
        let task_id = task.id;
        let mut confirmed_order = None;
        let mut report = match confirm_live_order(gateway, &ack).await {
            Ok((order, fills)) => {
                confirmed_order = Some(order.clone());
                build_confirmed_order_report_for_task(
                    task,
                    order_side,
                    &ack,
                    Some(order),
                    fills,
                    None,
                    protection.clone(),
                )
            }
            Err(error) => {
                warn!(
                    task_id,
                    exchange = ack.exchange.as_str(),
                    order_id = ack.order_id.as_deref().unwrap_or(""),
                    client_order_id = ack.client_order_id.as_deref().unwrap_or(""),
                    "live order confirmation failed after place_order ack: {}",
                    error
                );
                build_confirmed_order_report_for_task(
                    task,
                    order_side,
                    &ack,
                    None,
                    Vec::new(),
                    Some(error.to_string()),
                    protection.clone(),
                )
            }
        };

        if let (Some(order_task), Some(protection)) = (
            order_task,
            ProtectionSyncContract::from_task_result(&report, protection),
        ) {
            let outcome = if let Some(outcome) =
                attached_stop_loss_order_ack_outcome(order_task, &ack, confirmed_order.as_ref())
            {
                outcome
            } else {
                match load_exchange_order_filters(order_task.exchange, &order_task.symbol).await {
                    Ok(Some(filters)) => match build_protective_stop_market_order_request(
                        order_task,
                        &protection,
                        &filters,
                    ) {
                        Ok(request) => {
                            place_and_confirm_protective_order(
                                gateway,
                                order_task.exchange,
                                request,
                            )
                            .await
                        }
                        Err(error) => ProtectionSyncOutcome::failed(
                            "build_protective_order_request",
                            error.to_string(),
                        ),
                    },
                    Ok(None) => ProtectionSyncOutcome::failed(
                        "load_protective_order_filters",
                        format!(
                            "missing exchange symbol filters for {} on {} before protective order",
                            order_task.symbol,
                            order_task.exchange.as_str()
                        ),
                    ),
                    Err(error) => ProtectionSyncOutcome::failed(
                        "load_protective_order_filters",
                        error.to_string(),
                    ),
                }
            };
            let should_rollback = protection_outcome_requires_rollback(&outcome);
            protection.apply_outcome_to_report(&mut report, outcome);
            if should_rollback {
                self.rollback_after_protective_failure(task, gateway, order_task, &mut report)
                    .await;
            }
        }

        report
    }

    async fn rollback_after_protective_failure(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
        report: &mut ExecutionTaskReportRequest,
    ) {
        let request = match build_protective_failure_rollback_order_request(order_task, report) {
            Ok(Some(request)) => request,
            Ok(None) => return,
            Err(error) => {
                apply_protective_failure_rollback_error(
                    report,
                    "build_rollback_order_request",
                    error.to_string(),
                );
                return;
            }
        };

        let rollback_side = order_side_lower(request.side);
        let ack = match self
            .place_order_with_audit(task, gateway, request.clone())
            .await
        {
            Ok(ack) => ack,
            Err(error) => {
                apply_protective_failure_rollback_error(
                    report,
                    "place_rollback_order",
                    error.to_string(),
                );
                return;
            }
        };

        let rollback_report = match confirm_live_order(gateway, &ack).await {
            Ok((order, fills)) => build_confirmed_order_report_for_task(
                task,
                rollback_side,
                &ack,
                Some(order),
                fills,
                None,
                None,
            ),
            Err(error) => build_confirmed_order_report_for_task(
                task,
                rollback_side,
                &ack,
                None,
                Vec::new(),
                Some(error.to_string()),
                None,
            ),
        };
        apply_protective_failure_rollback_report(report, &rollback_report);
    }

    async fn duplicate_client_order_id_report(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: Option<&ExecutionOrderTask>,
        order_side: &str,
        request: &OrderPlacementRequest,
        protection: Option<ProtectionSyncContract>,
    ) -> ExecutionTaskReportRequest {
        match duplicate_client_order_id_reconciliation_ack(request) {
            Some(ack) => {
                self.confirmed_live_order_report(
                    task, gateway, order_task, order_side, ack, protection,
                )
                .await
            }
            None => ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side,
                "duplicate client order id error requires a stable client_order_id to reconcile",
                json!({
                    "reconciliation": {
                        "reason": "duplicate_client_order_id",
                        "action": "blocked_missing_client_order_id",
                        "place_order_retried": false,
                    }
                }),
            ),
        }
    }

    async fn pre_place_client_order_report(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: Option<&ExecutionOrderTask>,
        order_side: &str,
        request: &OrderPlacementRequest,
        protection: Option<ProtectionSyncContract>,
    ) -> Result<Option<ExecutionTaskReportRequest>> {
        let Some(lookup) = pre_place_client_order_lookup(request) else {
            return Ok(None);
        };

        match gateway.order(request.exchange, lookup.query.clone()).await {
            Ok(_) => Ok(Some(
                self.confirmed_live_order_report(
                    task, gateway, order_task, order_side, lookup.ack, protection,
                )
                .await,
            )),
            Err(error) if is_order_not_found_for_client_order_preflight(&error.to_string()) => {
                Ok(None)
            }
            Err(error) => Err(anyhow!(
                "client order id pre-place check failed for {} on {}: {}",
                lookup.query.client_order_id.as_deref().unwrap_or("unknown"),
                request.exchange.as_str(),
                error
            )),
        }
    }

    async fn record_checkpoint(
        &self,
        worker_status: &str,
        last_task_id: Option<i64>,
        checkpoint_value: Value,
    ) {
        let checkpoint = ExecutionWorkerCheckpoint::heartbeat(
            self.config.worker_id.clone(),
            worker_status,
            last_task_id,
            checkpoint_value,
        );
        if let Err(error) = self
            .audit_repository
            .upsert_worker_checkpoint(&checkpoint)
            .await
        {
            warn!(
                worker_id = self.config.worker_id,
                "写入 execution worker checkpoint 失败: {}", error
            );
        }
    }

    async fn record_report_result_failure(
        &self,
        task_id: i64,
        report: &ExecutionTaskReportRequest,
        error_message: impl Into<String>,
        stage: &str,
    ) {
        let error_message = error_message.into();
        self.record_exchange_request_audit(ExchangeRequestAuditLog::report_result_failed(
            report,
            error_message.clone(),
        ))
        .await;
        self.record_checkpoint(
            "report_failed",
            Some(task_id),
            json!({
                "stage": stage,
                "error": error_message,
                "replay": {
                    "action": "retry_report_result_only",
                    "place_order_allowed": false,
                    "task_id": report.task_id,
                    "exchange": report.exchange,
                    "external_order_id": report.external_order_id,
                    "execution_status": report.execution_status,
                    "order_status": report.order_status,
                },
            }),
        )
        .await;
    }

    async fn record_exchange_request_audit(&self, audit: ExchangeRequestAuditLog) {
        if let Err(error) = self
            .audit_repository
            .insert_exchange_request_audit(&audit)
            .await
        {
            warn!(
                request_id = audit.request_id,
                "写入 exchange request audit 失败: {}", error
            );
        }
    }
}

include!("execution_worker_reconciliation_only_section.rs");
include!("execution_worker_reconciliation_section.rs");
include!("execution_worker_order_task_section.rs");
include!("execution_worker_confirmation_section.rs");

#[cfg(test)]
#[path = "execution_worker_protection_tests.rs"]
mod execution_worker_protection_tests;
#[cfg(test)]
#[path = "execution_worker_reconciliation_tests.rs"]
mod execution_worker_reconciliation_tests;
#[cfg(test)]
#[path = "execution_worker_reporting_tests.rs"]
mod execution_worker_reporting_tests;
#[cfg(test)]
#[path = "execution_worker_test_support.rs"]
mod execution_worker_test_support;
