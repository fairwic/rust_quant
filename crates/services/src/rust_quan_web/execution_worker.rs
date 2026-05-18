use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, Fill, FillListQuery, Instrument, MarginMode, Order, OrderAck,
    OrderListQuery, OrderQuery, OrderSide, OrderType, Position, PositionMode,
    PrepareOrderSettingsRequest, ProtectiveOrderQuery, ProtectiveOrderRequest,
    ProtectiveOrderWorkingType, TimeInForce,
};
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::{json, Value};
use std::{str::FromStr, sync::Arc, time::Instant};
use tokio::time::{sleep, Duration};
use tracing::{error, warn};

use crate::exchange::{CryptoExcAllGateway, OrderPlacementRequest};
use crate::rust_quan_web::{
    ExchangeOrderResult, ExchangeReconciliationIssueType, ExchangeReconciliationReportRequest,
    ExchangeReconciliationReportResponse, ExchangeRequestAuditLog, ExecutionAuditRepository,
    ExecutionTask, ExecutionTaskClient, ExecutionTaskConfig, ExecutionTaskConfirmationLeaseItem,
    ExecutionTaskLeaseRequest, ExecutionTaskReportRequest, ExecutionWorkerCheckpoint,
    NoopExecutionAuditRepository, PostgresExecutionAuditRepository,
};

const LIVE_ORDER_CONFIRM_ENV: &str = "EXECUTION_WORKER_LIVE_ORDER_CONFIRM";
const LIVE_ORDER_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LIVE_ORDERS";

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

    fn report_replay_limit(&self) -> u32 {
        self.lease_limit
            .min(self.report_replay_max_per_run)
            .clamp(1, 100)
    }
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
        let gateway = if config.dry_run {
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
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "exchange_reconciliation_read_only",
                        "exchange": order_task.exchange.as_str(),
                        "symbol": order_task.symbol,
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        }

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

        match self.live_order_request(&gateway, &order_task).await {
            Ok(request) => {
                let protection =
                    ProtectionSyncContract::from_task(task, order_side_lower(order_task.side));
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
                match self
                    .place_order_with_audit(task, &gateway, request.clone())
                    .await
                {
                    Ok(ack) => {
                        self.confirmed_live_order_report(
                            task,
                            &gateway,
                            Some(&order_task),
                            order_side_lower(order_task.side),
                            ack,
                            protection,
                        )
                        .await
                    }
                    Err(error) if is_duplicate_client_order_id_error(&error.to_string()) => {
                        self.duplicate_client_order_id_report(
                            task,
                            &gateway,
                            Some(&order_task),
                            order_side_lower(order_task.side),
                            &request,
                            protection,
                        )
                        .await
                    }
                    Err(error) => ExecutionTaskReportRequest::failed(
                        task.id,
                        order_task.exchange.as_str(),
                        order_side_lower(order_task.side),
                        error.to_string(),
                        json!({"task_id": task.id}),
                    ),
                }
            }
            Err(error) => ExecutionTaskReportRequest::failed(
                task.id,
                order_task.exchange.as_str(),
                order_side_lower(order_task.side),
                error.to_string(),
                json!({"task_id": task.id}),
            ),
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
        let mut report = match confirm_live_order(gateway, &ack).await {
            Ok((order, fills)) => build_confirmed_order_report_for_task(
                task,
                order_side,
                &ack,
                Some(order),
                fills,
                None,
                protection.clone(),
            ),
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
            let outcome = match load_exchange_order_filters(order_task.exchange, &order_task.symbol)
                .await
            {
                Ok(Some(filters)) => match build_protective_stop_market_order_request(
                    order_task,
                    &protection,
                    &filters,
                ) {
                    Ok(request) => {
                        place_and_confirm_protective_order(gateway, order_task.exchange, request)
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
            };
            protection.apply_outcome_to_report(&mut report, outcome);
        }

        report
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

fn build_exchange_reconciliation_report_request(
    task: &ExecutionTask,
    issue_type: ExchangeReconciliationIssueType,
    detected_at: Option<String>,
    message: impl Into<String>,
) -> ExchangeReconciliationReportRequest {
    let symbol = reconciliation_symbol(task);
    let source_ref = format!(
        "rust_quant/exchange_reconciliation/{}/combo/{}/task/{}/symbol/{}",
        issue_type.as_str(),
        task.combo_id,
        task.id,
        symbol
    );
    let message = message.into().trim().to_string();
    let message = (!message.is_empty()).then_some(message);

    ExchangeReconciliationReportRequest {
        combo_id: task.combo_id,
        buyer_email: task.buyer_email.clone(),
        symbol,
        issue_type,
        detected_at,
        source_ref: Some(source_ref),
        message,
    }
}

fn build_exchange_reconciliation_requests_from_read_only_snapshot(
    task: &ExecutionTask,
    positions: &[Position],
    open_orders: &[Order],
    detected_at: Option<String>,
) -> Vec<ExchangeReconciliationReportRequest> {
    let mut requests = Vec::new();
    let position_count = positions
        .iter()
        .filter(|position| positive_decimal_text(&position.size))
        .count();
    if position_count > 0 {
        requests.push(build_exchange_reconciliation_report_request(
            task,
            ExchangeReconciliationIssueType::ExchangePositionStale,
            detected_at.clone(),
            format!(
                "read-only exchange snapshot detected {position_count} non-zero position(s); place_order_allowed=false; mutation_allowed=false"
            ),
        ));
    }

    let open_order_count = open_orders
        .iter()
        .filter(|order| active_open_order_status(order.status.as_deref()))
        .count();
    if open_order_count > 0 {
        requests.push(build_exchange_reconciliation_report_request(
            task,
            ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
            detected_at,
            format!(
                "read-only exchange snapshot detected {open_order_count} open order(s); place_order_allowed=false; mutation_allowed=false"
            ),
        ));
    }

    requests
}

fn build_live_order_blocked_by_exchange_reconciliation_report(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
    requests: &[ExchangeReconciliationReportRequest],
) -> ExecutionTaskReportRequest {
    let issues: Vec<Value> = requests
        .iter()
        .map(|request| {
            json!({
                "issue_type": request.issue_type.as_str(),
                "source_ref": request.source_ref,
                "message": request.message,
            })
        })
        .collect();
    let issue_codes: Vec<&str> = requests
        .iter()
        .map(|request| request.issue_type.as_str())
        .collect();
    let message = format!(
        "live order blocked by read-only exchange reconciliation: {}; place_order_allowed=false; mutation_allowed=false",
        issue_codes.join(", ")
    );

    ExecutionTaskReportRequest::failed(
        task.id,
        order_task.exchange.as_str(),
        order_side_lower(order_task.side),
        message,
        json!({
            "task_id": task.id,
            "stage": "exchange_reconciliation_read_only",
            "exchange": order_task.exchange.as_str(),
            "symbol": order_task.symbol,
            "issues": issues,
            "place_order_allowed": false,
            "mutation_allowed": false,
        }),
    )
}

fn positive_decimal_text(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|parsed| parsed.is_finite() && parsed.abs() > 0.0)
}

fn active_open_order_status(status: Option<&str>) -> bool {
    let normalized = status.unwrap_or_default().trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "canceled" | "cancelled" | "filled" | "closed" | "rejected" | "expired"
    )
}

fn reconciliation_symbol(task: &ExecutionTask) -> String {
    let payload = order_payload(&task.request_payload_json);
    payload_string(&payload, "symbol").unwrap_or_else(|| task.symbol.clone())
}

fn report_replay_operator_playbook_summary(
    failed_count: usize,
    failure_backoff_seconds: u64,
) -> Value {
    if failed_count == 0 {
        return json!({
            "item_count": 0,
            "blocking_item_count": 0,
            "manual_review_item_count": 0,
            "observe_only_item_count": 0,
            "items": [],
        });
    }

    json!({
        "item_count": 1,
        "blocking_item_count": 0,
        "manual_review_item_count": 1,
        "observe_only_item_count": 0,
        "items": [
            {
                "source": "execution_worker_checkpoint",
                "severity": "P1",
                "code": "QUANT_REPORT_REPLAY_FAILED",
                "section": "quant_worker_checkpoint_audit",
                "message": "report_result replay batch has failed attempts",
                "operator_action": "manual_review_before_release",
                "owner": "quant_ops",
                "default_next_action": "review_report_replay_batch",
                "admin_link_target": "admin.full_product_health.quant_worker_checkpoint_audit",
                "metadata": {
                    "failed_count": failed_count,
                    "failure_backoff_seconds": failure_backoff_seconds,
                    "place_order_allowed": false,
                },
            }
        ],
    })
}

fn elapsed_ms(started_at: Instant) -> Option<i32> {
    Some(started_at.elapsed().as_millis().min(i32::MAX as u128) as i32)
}

#[derive(Debug, Clone)]
pub struct ExecutionOrderTask {
    pub task_id: i64,
    pub exchange: ExchangeId,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub size: String,
    pub price: Option<String>,
    pub margin_mode: Option<MarginMode>,
    pub leverage: Option<String>,
    pub position_mode: Option<PositionMode>,
    pub margin_coin: Option<String>,
    pub position_side: Option<String>,
    pub trade_side: Option<String>,
    pub client_order_id: Option<String>,
    pub reduce_only: Option<bool>,
    pub time_in_force: Option<TimeInForce>,
    pub size_usdt: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ExchangeOrderFilters {
    min_qty: Option<Decimal>,
    max_qty: Option<Decimal>,
    step_size: Option<Decimal>,
    min_notional: Option<Decimal>,
    quantity_precision: Option<u32>,
    tick_size: Option<Decimal>,
    price_precision: Option<u32>,
}

#[derive(Debug, Clone)]
struct RiskContractViolation {
    message: String,
    raw_payload: Value,
}

#[derive(Debug, Clone)]
struct ProtectionSyncContract {
    selected_stop_loss_price: f64,
    direction: ProtectiveDirection,
    entry_reference_price: Option<f64>,
    original_selected_stop_loss_price: Option<f64>,
}

const DEFAULT_PROTECTIVE_STOP_REBASE_RATIO: f64 = 0.02;
const PROTECTIVE_ORDER_QUERY_ATTEMPTS: usize = 4;
const PROTECTIVE_ORDER_QUERY_BACKOFF_MS: u64 = 250;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProtectionSyncOutcome {
    Confirmed {
        protective_order_external_id: String,
        source: String,
    },
    Failed {
        reason: String,
        error_message: String,
    },
    Uncertain {
        reason: String,
        error_message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtectiveDirection {
    Long,
    Short,
}

#[allow(dead_code)]
impl ProtectionSyncOutcome {
    fn confirmed(
        protective_order_external_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Confirmed {
            protective_order_external_id: protective_order_external_id.into(),
            source: source.into(),
        }
    }

    fn failed(reason: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            error_message: error_message.into(),
        }
    }

    fn uncertain(reason: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self::Uncertain {
            reason: reason.into(),
            error_message: error_message.into(),
        }
    }
}

impl ProtectionSyncContract {
    fn from_task(task: &ExecutionTask, order_side: &str) -> Option<Self> {
        Self::required_for_task(task, order_side).ok()
    }

    fn required(payload: Value, order_side: &str) -> Result<Self> {
        Self::required_from_payload(payload, order_side, false)
    }

    fn required_for_task(task: &ExecutionTask, order_side: &str) -> Result<Self> {
        let payload = order_payload(&task.request_payload_json);
        Self::required_from_payload(payload, order_side, task.news_signal_id.is_some())
    }

    fn required_from_payload(
        payload: Value,
        order_side: &str,
        task_news_signal_requires_stop_loss: bool,
    ) -> Result<Self> {
        if !protective_stop_loss_required(&payload, task_news_signal_requires_stop_loss) {
            return Err(anyhow!("protective stop-loss is not required"));
        }
        let selected_stop_loss_price = selected_stop_loss_price(&payload)
            .filter(|price| price.is_finite() && *price > 0.0)
            .ok_or_else(|| anyhow!("risk_plan.selected_stop_loss_price is required"))?;
        let direction = match risk_plan_direction_raw(&payload) {
            Some(raw) => parse_protective_direction(&raw)?,
            None => parse_protective_direction(order_side)?,
        };

        Ok(Self {
            selected_stop_loss_price,
            direction,
            entry_reference_price: protection_entry_price(&payload)
                .filter(|price| price.is_finite() && *price > 0.0),
            original_selected_stop_loss_price: None,
        })
    }

    fn apply_to_report(&self, report: &mut ExecutionTaskReportRequest) {
        self.apply_outcome_to_report(
            report,
            ProtectionSyncOutcome::uncertain(
                "protective_order_sync_not_confirmed",
                "protective stop-loss required but protection order sync is not confirmed",
            ),
        );
    }

    fn from_task_result(
        report: &ExecutionTaskReportRequest,
        protection: Option<ProtectionSyncContract>,
    ) -> Option<ProtectionSyncContract> {
        let protection = protection?;
        if report.order_status.trim().eq_ignore_ascii_case("FILLED") {
            Some(protection.rebased_after_filled_report(report))
        } else {
            None
        }
    }

    fn rebased_after_filled_report(mut self, report: &ExecutionTaskReportRequest) -> Self {
        let Some(fill_price) = filled_average_price(report) else {
            return self;
        };
        let stop_would_immediately_trigger = match self.direction {
            ProtectiveDirection::Long => self.selected_stop_loss_price >= fill_price,
            ProtectiveDirection::Short => self.selected_stop_loss_price <= fill_price,
        };
        if !stop_would_immediately_trigger {
            return self;
        }

        let risk_ratio = self
            .entry_reference_price
            .and_then(|entry_price| {
                stop_loss_risk_ratio(entry_price, self.selected_stop_loss_price, self.direction)
            })
            .unwrap_or(DEFAULT_PROTECTIVE_STOP_REBASE_RATIO);
        let adjusted_stop_loss_price = match self.direction {
            ProtectiveDirection::Long => fill_price * (1.0 - risk_ratio),
            ProtectiveDirection::Short => fill_price * (1.0 + risk_ratio),
        };
        let adjusted_is_valid = adjusted_stop_loss_price.is_finite()
            && adjusted_stop_loss_price > 0.0
            && match self.direction {
                ProtectiveDirection::Long => adjusted_stop_loss_price < fill_price,
                ProtectiveDirection::Short => adjusted_stop_loss_price > fill_price,
            };
        if adjusted_is_valid {
            self.original_selected_stop_loss_price = Some(
                self.original_selected_stop_loss_price
                    .unwrap_or(self.selected_stop_loss_price),
            );
            self.selected_stop_loss_price = adjusted_stop_loss_price;
        }

        self
    }

    fn apply_outcome_to_report(
        &self,
        report: &mut ExecutionTaskReportRequest,
        outcome: ProtectionSyncOutcome,
    ) {
        let mut raw_payload = report
            .raw_payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| json!({}));

        let mut protection_sync = json!({
            "place_order_allowed": false,
            "repeat_open_order_allowed": false,
            "selected_stop_loss_price": self.selected_stop_loss_price,
            "direction": self.direction.as_str(),
        });
        if let Some(entry_reference_price) = self.entry_reference_price {
            protection_sync["entry_reference_price"] = json!(entry_reference_price);
        }
        if let Some(original_selected_stop_loss_price) = self.original_selected_stop_loss_price {
            protection_sync["original_selected_stop_loss_price"] =
                json!(original_selected_stop_loss_price);
            protection_sync["stop_loss_rebased_after_fill"] = json!(true);
        }

        match outcome {
            ProtectionSyncOutcome::Confirmed {
                protective_order_external_id,
                source,
            } => {
                protection_sync["status"] = json!("completed");
                protection_sync["reason"] = json!("protective_order_confirmed");
                protection_sync["source"] = json!(source);
                protection_sync["protective_order_confirmed"] = json!(true);
                protection_sync["exchange_protective_order_supported"] = json!(true);
                protection_sync["protective_order_external_id"] =
                    json!(protective_order_external_id);
                report.execution_status = "completed".to_string();
                report.error_message = None;
            }
            ProtectionSyncOutcome::Failed {
                reason,
                error_message,
            } => {
                protection_sync["status"] = json!("protective_order_failed");
                protection_sync["reason"] = json!(reason);
                protection_sync["protective_order_confirmed"] = json!(false);
                protection_sync["exchange_protective_order_supported"] = json!(true);
                protection_sync["error_message"] = json!(error_message);
                report.execution_status = "protective_order_failed".to_string();
                report.error_message = protection_sync["error_message"]
                    .as_str()
                    .map(ToOwned::to_owned);
            }
            ProtectionSyncOutcome::Uncertain {
                reason,
                error_message,
            } => {
                protection_sync["status"] = json!("pending_protection_sync");
                protection_sync["reason"] = json!(reason);
                protection_sync["protective_order_confirmed"] = json!(false);
                protection_sync["exchange_protective_order_supported"] = json!(false);
                protection_sync["error_message"] = json!(error_message);
                report.execution_status = "pending_protection_sync".to_string();
                report.error_message = protection_sync["error_message"]
                    .as_str()
                    .map(ToOwned::to_owned);
            }
        }

        raw_payload["protection_sync"] = protection_sync;
        raw_payload["execution_status"] = json!(report.execution_status);
        report.raw_payload_json = Some(raw_payload.to_string());
    }
}

fn filled_average_price(report: &ExecutionTaskReportRequest) -> Option<f64> {
    let qty = report.filled_qty?;
    let quote = report.filled_quote?;
    if qty.is_finite() && quote.is_finite() && qty > 0.0 && quote > 0.0 {
        Some(quote / qty)
    } else {
        None
    }
}

fn stop_loss_risk_ratio(
    entry_price: f64,
    selected_stop_loss_price: f64,
    direction: ProtectiveDirection,
) -> Option<f64> {
    if !entry_price.is_finite() || entry_price <= 0.0 || !selected_stop_loss_price.is_finite() {
        return None;
    }
    let ratio = match direction {
        ProtectiveDirection::Long if selected_stop_loss_price < entry_price => {
            (entry_price - selected_stop_loss_price) / entry_price
        }
        ProtectiveDirection::Short if selected_stop_loss_price > entry_price => {
            (selected_stop_loss_price - entry_price) / entry_price
        }
        _ => return None,
    };
    if ratio.is_finite() && ratio > 0.0 && ratio < 1.0 {
        Some(ratio)
    } else {
        None
    }
}

impl ProtectiveDirection {
    fn as_str(self) -> &'static str {
        match self {
            ProtectiveDirection::Long => "long",
            ProtectiveDirection::Short => "short",
        }
    }

    fn protective_order_side(self) -> OrderSide {
        match self {
            ProtectiveDirection::Long => OrderSide::Sell,
            ProtectiveDirection::Short => OrderSide::Buy,
        }
    }
}

fn build_protective_stop_market_order_request(
    order_task: &ExecutionOrderTask,
    protection: &ProtectionSyncContract,
    filters: &ExchangeOrderFilters,
) -> Result<ProtectiveOrderRequest> {
    let stop_price = quantize_protective_stop_price(
        protection.selected_stop_loss_price,
        protection.direction,
        filters,
    )?;
    let mut request = ProtectiveOrderRequest::stop_market(
        parse_instrument(&order_task.symbol)?,
        protection.direction.protective_order_side(),
        format_protective_stop_price_decimal(stop_price, filters),
    )
    .with_close_position(true)
    .with_working_type(ProtectiveOrderWorkingType::MarkPrice)
    .with_price_protect(true)
    .with_client_order_id(protective_order_client_id(order_task.task_id));

    if let Some(position_side) = order_task.position_side.as_deref() {
        request = request.with_position_side(position_side);
    }

    Ok(request)
}

fn protective_order_result_to_sync_outcome(
    result: crypto_exc_all::Result<OrderAck>,
) -> ProtectionSyncOutcome {
    match result {
        Ok(ack) => ProtectionSyncOutcome::confirmed(
            ack.order_id
                .or(ack.client_order_id)
                .unwrap_or_else(|| "unknown_protective_order".to_string()),
            "place_protective_order",
        ),
        Err(error) => ProtectionSyncOutcome::failed("place_protective_order", error.to_string()),
    }
}

async fn place_and_confirm_protective_order(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    request: ProtectiveOrderRequest,
) -> ProtectionSyncOutcome {
    let instrument = request.instrument.clone();
    let request_client_order_id = request.client_order_id.clone();
    let ack = match gateway.place_protective_order(exchange, request).await {
        Ok(ack) => ack,
        Err(error) => return protective_order_result_to_sync_outcome(Err(error)),
    };
    let queries = match protective_order_query_candidates_from_ack(
        &instrument,
        &ack,
        request_client_order_id,
    ) {
        Ok(query) => query,
        Err(error) => {
            return ProtectionSyncOutcome::uncertain("query_protective_order", error.to_string());
        }
    };

    let mut last_absent_error = None;
    for attempt in 0..PROTECTIVE_ORDER_QUERY_ATTEMPTS {
        for query in queries.iter().cloned() {
            match gateway.protective_order(exchange, query).await {
                Ok(order) => return protective_order_query_to_sync_outcome(Ok(order)),
                Err(error) if is_protective_order_already_absent(&error) => {
                    last_absent_error = Some(error.to_string());
                }
                Err(error) => {
                    return ProtectionSyncOutcome::uncertain(
                        "query_protective_order",
                        error.to_string(),
                    );
                }
            }
        }

        if attempt + 1 < PROTECTIVE_ORDER_QUERY_ATTEMPTS {
            sleep(Duration::from_millis(
                PROTECTIVE_ORDER_QUERY_BACKOFF_MS * (attempt as u64 + 1),
            ))
            .await;
        }
    }

    ProtectionSyncOutcome::uncertain(
        "query_protective_order",
        format!(
            "protective order not visible after {} confirmation attempts: {}",
            PROTECTIVE_ORDER_QUERY_ATTEMPTS,
            last_absent_error.unwrap_or_else(|| "no query candidate matched".to_string())
        ),
    )
}

fn protective_order_query_candidates_from_ack(
    instrument: &Instrument,
    ack: &OrderAck,
    request_client_order_id: Option<String>,
) -> Result<Vec<ProtectiveOrderQuery>> {
    let mut queries = Vec::new();
    if let Some(client_order_id) = ack
        .client_order_id
        .as_deref()
        .or(request_client_order_id.as_deref())
        .filter(|value| !value.trim().is_empty())
    {
        queries.push(ProtectiveOrderQuery::by_client_order_id(
            instrument.clone(),
            client_order_id.to_string(),
        ));
    }
    if let Some(order_id) = ack
        .order_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        queries.push(ProtectiveOrderQuery::by_order_id(
            instrument.clone(),
            order_id.to_string(),
        ));
    }
    if !queries.is_empty() {
        return Ok(queries);
    }

    Err(anyhow!(
        "protective order ack did not include order id or client order id"
    ))
}

fn protective_order_query_to_sync_outcome(
    result: crypto_exc_all::Result<Order>,
) -> ProtectionSyncOutcome {
    match result {
        Ok(order) => {
            let status = order
                .status
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("UNKNOWN");
            if protective_order_status_is_active(status) {
                return ProtectionSyncOutcome::confirmed(
                    order
                        .order_id
                        .or(order.client_order_id)
                        .unwrap_or_else(|| "unknown_protective_order".to_string()),
                    "query_protective_order",
                );
            }
            ProtectionSyncOutcome::failed(
                "query_protective_order",
                format!("protective order is not active: status={status}"),
            )
        }
        Err(error) if is_protective_order_already_absent(&error) => {
            ProtectionSyncOutcome::failed("query_protective_order", error.to_string())
        }
        Err(error) => ProtectionSyncOutcome::uncertain("query_protective_order", error.to_string()),
    }
}

fn protective_order_status_is_active(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_uppercase().as_str(),
        "NEW" | "WORKING" | "ACCEPTED"
    )
}

fn protective_order_client_id(task_id: i64) -> String {
    format!("rq-sl-{task_id}")
}

fn apply_post_close_protection_cancel_result(
    report: &mut ExecutionTaskReportRequest,
    result: crypto_exc_all::Result<OrderAck>,
) {
    let mut raw_payload = report
        .raw_payload_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));

    match result {
        Ok(ack) => {
            raw_payload["post_close_protection_cancel"] = json!({
                "status": "completed",
                "protective_order_cancelled": true,
                "exchange": ack.exchange.as_str(),
                "external_order_id": ack.order_id,
                "client_order_id": ack.client_order_id,
            });
        }
        Err(error) => {
            let message = error.to_string();
            if is_protective_order_already_absent(&error) {
                raw_payload["post_close_protection_cancel"] = json!({
                    "status": "already_absent",
                    "protective_order_cancelled": false,
                    "protective_order_absent": true,
                    "error_message": message,
                });
            } else {
                raw_payload["post_close_protection_cancel"] = json!({
                    "status": "protective_cancel_failed",
                    "protective_order_cancelled": false,
                    "error_message": message,
                });
                report.execution_status = "protective_cancel_failed".to_string();
                report.error_message = Some(message);
            }
        }
    }

    raw_payload["execution_status"] = json!(report.execution_status);
    report.raw_payload_json = Some(raw_payload.to_string());
}

fn is_protective_order_already_absent(error: &crypto_exc_all::Error) -> bool {
    matches!(
        error,
        crypto_exc_all::Error::Api {
            exchange: ExchangeId::Binance,
            code,
            ..
        } if matches!(code.as_str(), "-2011" | "-2013")
    )
}

#[derive(Debug, Clone)]
struct PendingCloseTask {
    task_id: i64,
    exchange: ExchangeId,
    symbol: String,
    task_type: String,
    task_status: String,
    risk_control_action: String,
    manual_review: Value,
    close_order_payload: Option<Value>,
}

impl PendingCloseTask {
    fn from_task(task: &ExecutionTask, default_exchange: ExchangeId) -> Result<Self> {
        let payload = order_payload(&task.request_payload_json);
        let exchange = payload_string(&payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(default_exchange);
        let symbol = payload_string(&payload, "symbol").unwrap_or_else(|| task.symbol.clone());
        let risk_control_action = payload
            .get("manual_review")
            .and_then(|value| value.get("action"))
            .and_then(Value::as_str)
            .or_else(|| {
                payload
                    .get("risk_control")
                    .and_then(|value| value.get("action"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("close_candidate")
            .trim()
            .to_string();
        let close_order_payload = payload.get("close_order").cloned().or_else(|| {
            payload
                .get("execution")
                .and_then(|value| value.get("close_order"))
                .cloned()
        });

        Ok(Self {
            task_id: task.id,
            exchange,
            symbol,
            task_type: task.task_type.clone(),
            task_status: task.task_status.clone(),
            risk_control_action,
            manual_review: payload.get("manual_review").cloned().unwrap_or(Value::Null),
            close_order_payload,
        })
    }

    fn to_order_request(&self) -> Result<Option<OrderPlacementRequest>> {
        let Some(payload) = self.close_order_payload.as_ref() else {
            return Ok(None);
        };
        let side = close_order_side(payload)?;
        let exchange = payload_string(payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(self.exchange);
        let symbol = payload_string(payload, "symbol").unwrap_or_else(|| self.symbol.clone());
        let order_type = payload_string(payload, "order_type")
            .map(|value| parse_order_type(&value))
            .transpose()?
            .unwrap_or(OrderType::Market);

        let position_side = payload_string(payload, "position_side");
        // Hedge-mode closes use position_side to constrain the side being reduced.
        // In that mode Binance rejects reduceOnly, while one-way close tasks should
        // still default to reduce_only=true.
        let default_reduce_only = match (exchange, position_side.as_deref()) {
            (ExchangeId::Okx, _) => None,
            (ExchangeId::Binance, Some(_)) => None,
            _ => Some(true),
        };

        Ok(Some(OrderPlacementRequest {
            exchange,
            instrument: parse_instrument(&symbol)?,
            side,
            order_type,
            size: payload_string(payload, "size")
                .or_else(|| payload_string(payload, "quantity"))
                .or_else(|| payload_string(payload, "qty"))
                .unwrap_or_else(|| "0".to_string()),
            price: payload_string(payload, "price"),
            margin_mode: payload_string(payload, "margin_mode").map(MarginMode::from),
            margin_coin: payload_string(payload, "margin_coin"),
            position_side,
            trade_side: payload_string(payload, "trade_side").or_else(|| Some("close".to_string())),
            client_order_id: payload_string(payload, "client_order_id")
                .or_else(|| Some(format!("rqclose{}", self.task_id))),
            reduce_only: payload_bool(payload, "reduce_only").or(default_reduce_only),
            time_in_force: payload_string(payload, "time_in_force")
                .map(|value| parse_time_in_force(&value))
                .transpose()?,
        }))
    }

    fn protective_cancel_request(&self) -> Result<Option<(ExchangeId, CancelOrderRequest)>> {
        let Some(payload) = self.close_order_payload.as_ref() else {
            return Ok(None);
        };
        let client_order_id = payload_string(payload, "cancel_protective_client_order_id")
            .or_else(|| payload_string(payload, "protective_order_client_id"));
        let order_id = payload_string(payload, "cancel_protective_order_id")
            .or_else(|| payload_string(payload, "protective_order_external_id"));
        if client_order_id.is_none() && order_id.is_none() {
            return Ok(None);
        }

        let exchange = payload_string(payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(self.exchange);
        let symbol = payload_string(payload, "symbol").unwrap_or_else(|| self.symbol.clone());
        let instrument = parse_instrument(&symbol)?;
        let mut request = if let Some(client_order_id) = client_order_id {
            CancelOrderRequest::by_client_order_id(instrument, client_order_id)
        } else {
            CancelOrderRequest::by_order_id(
                instrument,
                order_id.expect("checked above that order id is present"),
            )
        };
        if let Some(margin_coin) = payload_string(payload, "margin_coin") {
            request = request.with_margin_coin(margin_coin);
        }

        Ok(Some((exchange, request)))
    }

    fn dry_run_report(&self) -> ExecutionTaskReportRequest {
        ExecutionTaskReportRequest::success(
            self.task_id,
            self.exchange.as_str(),
            format!("dry-run-close-task-{}", self.task_id),
            "close",
            "dry_run",
            self.report_payload(true),
        )
    }

    fn missing_live_contract_message(&self) -> String {
        "pending_close task requires Web close_order payload before live execution".to_string()
    }

    fn report_payload(&self, dry_run: bool) -> Value {
        json!({
            "dry_run": dry_run,
            "task_type": self.task_type.clone(),
            "task_status": self.task_status.clone(),
            "symbol": self.symbol.clone(),
            "risk_control_action": self.risk_control_action.clone(),
            "manual_review": self.manual_review.clone(),
            "close_order": self.close_order_payload.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct PendingConfirmationTask {
    task_id: i64,
    exchange: ExchangeId,
    symbol: String,
    external_order_id: Option<String>,
    client_order_id: Option<String>,
    order_side: String,
    order_status: String,
}

impl PendingConfirmationTask {
    fn from_task_and_order_result(
        task: &ExecutionTask,
        exchange: &str,
        external_order_id: &str,
        order_side: &str,
        order_status: &str,
    ) -> Result<Self> {
        let exchange = parse_exchange(exchange)?;
        let order_task = ExecutionOrderTask::from_task_with_default(task, exchange).ok();
        let client_order_id = order_task
            .as_ref()
            .and_then(|order| order.client_order_id.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(format!("rqtask{}", task.id)));
        let external_order_id =
            Some(external_order_id.trim().to_string()).filter(|value| !value.is_empty());

        Ok(Self {
            task_id: task.id,
            exchange,
            symbol: order_task
                .map(|order| order.symbol)
                .unwrap_or_else(|| task.symbol.clone()),
            external_order_id,
            client_order_id,
            order_side: order_side.trim().to_string(),
            order_status: order_status.trim().to_string(),
        })
    }

    fn from_confirmation_item(
        task: &ExecutionTask,
        order_result: &ExchangeOrderResult,
    ) -> Result<Self> {
        Self::from_task_and_order_result(
            task,
            &order_result.exchange,
            &order_result.external_order_id,
            &order_result.order_side,
            &order_result.order_status,
        )
    }

    fn to_order_query(&self) -> Result<OrderQuery> {
        let instrument = parse_instrument(&self.symbol)?;
        if let Some(external_order_id) = self
            .external_order_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if external_order_id.chars().all(|ch| ch.is_ascii_digit()) {
                return Ok(OrderQuery::by_order_id(instrument, external_order_id));
            }
            return Ok(OrderQuery::by_client_order_id(
                instrument,
                external_order_id,
            ));
        }
        if let Some(client_order_id) = self
            .client_order_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(OrderQuery::by_client_order_id(instrument, client_order_id));
        }

        Err(anyhow!(
            "pending_confirmation task {} requires exchange order id or client_order_id",
            self.task_id
        ))
    }

    fn external_or_client_order_id(&self) -> String {
        self.external_order_id
            .as_ref()
            .or(self.client_order_id.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("pending-confirmation-task-{}", self.task_id))
    }

    fn to_order_ack(&self, order: Option<&Order>) -> OrderAck {
        let instrument = parse_instrument(&self.symbol)
            .expect("pending confirmation symbol was already parsed for order query");
        let order_id = order.and_then(|order| order.order_id.clone()).or_else(|| {
            self.external_order_id
                .as_ref()
                .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
                .cloned()
        });
        let client_order_id = order
            .and_then(|order| order.client_order_id.clone())
            .or_else(|| {
                self.external_order_id
                    .as_ref()
                    .filter(|value| !value.chars().all(|ch| ch.is_ascii_digit()))
                    .cloned()
            })
            .or_else(|| self.client_order_id.clone());
        OrderAck {
            exchange: self.exchange,
            exchange_symbol: instrument.symbol_for(self.exchange),
            instrument,
            order_id,
            client_order_id,
            status: order
                .and_then(|order| order.status.clone())
                .or_else(|| Some(self.order_status.clone())),
            raw: json!({
                "source": "pending_confirmation_reconciler",
                "external_order_id": self.external_order_id,
                "client_order_id": self.client_order_id,
                "order_status": self.order_status,
            }),
        }
    }

    fn pending_report(
        &self,
        error_message: impl Into<String>,
        mut raw_payload: Value,
    ) -> ExecutionTaskReportRequest {
        if let Some(payload) = raw_payload.as_object_mut() {
            payload.insert(
                "execution_status".to_string(),
                json!("pending_confirmation"),
            );
            payload.insert(
                "external_order_id".to_string(),
                json!(self.external_order_id),
            );
            payload.insert("client_order_id".to_string(), json!(self.client_order_id));
        }
        let mut report = ExecutionTaskReportRequest::success(
            self.task_id,
            self.exchange.as_str(),
            self.external_or_client_order_id(),
            self.order_side.clone(),
            self.order_status.clone(),
            raw_payload,
        );
        report.execution_status = "pending_confirmation".to_string();
        report.error_message = Some(error_message.into());
        report
    }
}

impl ExecutionOrderTask {
    pub fn from_task(task: &ExecutionTask) -> Result<Self> {
        Self::from_task_with_default(task, ExchangeId::Okx)
    }

    pub fn from_task_with_default(
        task: &ExecutionTask,
        default_exchange: ExchangeId,
    ) -> Result<Self> {
        let payload = order_payload(&task.request_payload_json);
        let payload = &payload;
        let exchange = payload_string(payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(default_exchange);
        let symbol = payload_string(payload, "symbol").unwrap_or_else(|| task.symbol.clone());
        let side = payload_string(payload, "side")
            .or_else(|| payload_string(payload, "signal_type"))
            .map(|value| parse_side(&value))
            .transpose()?
            .unwrap_or(OrderSide::Buy);
        let order_type = payload_string(payload, "order_type")
            .map(|value| parse_order_type(&value))
            .transpose()?
            .unwrap_or(OrderType::Market);

        Ok(Self {
            task_id: task.id,
            exchange,
            symbol,
            side,
            order_type,
            size_usdt: payload_f64(payload, "size_usdt"),
            size: payload_string(payload, "size")
                .or_else(|| payload_string(payload, "quantity"))
                .or_else(|| payload_string(payload, "qty"))
                .unwrap_or_else(|| "0".to_string()),
            price: payload_string(payload, "price"),
            margin_mode: payload_string(payload, "margin_mode").map(MarginMode::from),
            leverage: payload_string(payload, "leverage"),
            position_mode: payload_string(payload, "position_mode")
                .map(|value| parse_position_mode(&value))
                .transpose()?,
            margin_coin: payload_string(payload, "margin_coin")
                .or_else(|| Some("USDT".to_string())),
            position_side: payload_string(payload, "position_side"),
            trade_side: payload_string(payload, "trade_side"),
            client_order_id: payload_string(payload, "client_order_id")
                .or_else(|| Some(format!("rqtask{}", task.id))),
            reduce_only: payload_bool(payload, "reduce_only"),
            time_in_force: payload_string(payload, "time_in_force")
                .map(|value| parse_time_in_force(&value))
                .transpose()?,
        })
    }

    pub fn to_order_request(&self) -> Result<OrderPlacementRequest> {
        Ok(OrderPlacementRequest {
            exchange: self.exchange,
            instrument: parse_instrument(&self.symbol)?,
            side: self.side,
            order_type: self.order_type,
            size: self.size.clone(),
            price: self.price.clone(),
            margin_mode: self.margin_mode.clone(),
            margin_coin: self.margin_coin.clone(),
            position_side: self.position_side.clone(),
            trade_side: self.trade_side.clone(),
            client_order_id: self.client_order_id.clone(),
            reduce_only: self.reduce_only,
            time_in_force: self.time_in_force,
        })
    }

    pub fn to_order_request_with_last_price(
        &self,
        last_price: Option<f64>,
    ) -> Result<OrderPlacementRequest> {
        let mut request = self.to_order_request()?;
        if !is_zero_order_size(&request.size) {
            return Ok(request);
        }
        let Some(size_usdt) = self.size_usdt else {
            return Ok(request);
        };
        let Some(last_price) = last_price else {
            return Ok(request);
        };
        if size_usdt.is_finite() && last_price.is_finite() && size_usdt > 0.0 && last_price > 0.0 {
            request.size = format_order_size(size_usdt / last_price);
        }
        Ok(request)
    }

    fn to_live_order_request(
        &self,
        last_price: Option<f64>,
        filters: Option<&ExchangeOrderFilters>,
    ) -> Result<OrderPlacementRequest> {
        let mut request = self.to_order_request_with_last_price(last_price)?;
        let filters = filters.ok_or_else(|| {
            anyhow!(
                "missing exchange symbol filters for {} on {}; run exchange symbol sync before live order",
                self.symbol,
                self.exchange.as_str()
            )
        })?;
        let last_price = decimal_from_f64(last_price.ok_or_else(|| {
            anyhow!(
                "missing ticker last_price for {} on {} before live order size validation",
                self.symbol,
                self.exchange.as_str()
            )
        })?)?;
        let size = parse_positive_decimal(&request.size, "order size")?;
        let enforce_min_notional = !request.reduce_only.unwrap_or(false)
            && !matches!(
                request.trade_side.as_deref().map(|value| value.to_ascii_lowercase()),
                Some(value) if value == "close"
            );
        let normalized_size = quantize_order_size(size, last_price, filters, enforce_min_notional)?;
        request.size = format_order_size_decimal(normalized_size, filters);
        Ok(request)
    }

    pub fn dry_run_report(&self) -> Result<ExecutionTaskReportRequest> {
        Ok(ExecutionTaskReportRequest::success(
            self.task_id,
            self.exchange.as_str(),
            format!("dry-run-rq-task-{}", self.task_id),
            order_side_lower(self.side),
            "dry_run",
            json!({
                "dry_run": true,
                "symbol": self.symbol,
            }),
        ))
    }
}

fn order_payload(payload: &Value) -> Value {
    let nested_payload = payload
        .get("payload_json")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok());

    let mut merged = payload.clone();
    let Some(merged_object) = merged.as_object_mut() else {
        return payload.clone();
    };

    if let Some(nested_object) = nested_payload.as_ref().and_then(Value::as_object) {
        for (key, value) in nested_object {
            merged_object
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }

    if let Some(execution_object) = payload.get("execution").and_then(Value::as_object) {
        for (key, value) in execution_object {
            merged_object
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }

    merged
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        Value::String(raw) => Some(raw.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn payload_f64(payload: &Value, key: &str) -> Option<f64> {
    payload.get(key).and_then(|value| match value {
        Value::Number(raw) => raw.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    })
}

fn nested_payload_f64(payload: &Value, parent: &str, key: &str) -> Option<f64> {
    payload
        .get(parent)
        .and_then(|parent| payload_f64(parent, key))
}

fn format_order_size(value: f64) -> String {
    let formatted = format!("{value:.8}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

async fn load_exchange_order_filters(
    exchange: ExchangeId,
    symbol: &str,
) -> Result<Option<ExchangeOrderFilters>> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| anyhow!("QUANT_CORE_DATABASE_URL is required for live order filter checks"))?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    let row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i32>,
            Option<String>,
            Option<i32>,
        ),
    >(
        r#"
        SELECT min_qty, max_qty, step_size, min_notional, quantity_precision, tick_size, price_precision
        FROM exchange_symbols
        WHERE exchange = $1
          AND normalized_symbol = $2
          AND status = 'TRADING'
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(exchange.as_str())
    .bind(symbol)
    .fetch_optional(&pool)
    .await?;
    pool.close().await;

    row.map(
        |(
            min_qty,
            max_qty,
            step_size,
            min_notional,
            quantity_precision,
            tick_size,
            price_precision,
        )| {
            Ok(ExchangeOrderFilters {
                min_qty: parse_optional_decimal(min_qty.as_deref(), "min_qty")?,
                max_qty: parse_optional_decimal(max_qty.as_deref(), "max_qty")?,
                step_size: parse_optional_decimal(step_size.as_deref(), "step_size")?,
                min_notional: parse_optional_decimal(min_notional.as_deref(), "min_notional")?,
                quantity_precision: quantity_precision.and_then(|value| u32::try_from(value).ok()),
                tick_size: parse_optional_decimal(tick_size.as_deref(), "tick_size")?,
                price_precision: price_precision.and_then(|value| u32::try_from(value).ok()),
            })
        },
    )
    .transpose()
}

fn parse_optional_decimal(raw: Option<&str>, label: &str) -> Result<Option<Decimal>> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<Decimal>()
                .map_err(|error| anyhow!("invalid {label} exchange filter {value}: {error}"))
        })
        .transpose()
}

fn parse_positive_decimal(raw: &str, label: &str) -> Result<Decimal> {
    let value = raw
        .trim()
        .parse::<Decimal>()
        .map_err(|error| anyhow!("invalid {label} {raw}: {error}"))?;
    if value <= Decimal::ZERO {
        return Err(anyhow!("{label} must be positive"));
    }
    Ok(value)
}

fn decimal_from_f64(raw: f64) -> Result<Decimal> {
    if !raw.is_finite() || raw <= 0.0 {
        return Err(anyhow!("price must be a positive finite number"));
    }
    format!("{raw:.12}")
        .parse::<Decimal>()
        .map_err(|error| anyhow!("invalid decimal price {raw}: {error}"))
}

fn floor_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    (value / step).floor() * step
}

fn ceil_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    let floored = floor_to_step(value, step);
    if floored == value {
        floored
    } else {
        floored + step
    }
}

fn quantize_order_size(
    requested_size: Decimal,
    last_price: Decimal,
    filters: &ExchangeOrderFilters,
    enforce_min_notional: bool,
) -> Result<Decimal> {
    if requested_size <= Decimal::ZERO {
        return Err(anyhow!("order size must be positive"));
    }

    let mut size = requested_size;
    if let Some(step) = filters.step_size.filter(|value| *value > Decimal::ZERO) {
        size = floor_to_step(size, step);
    } else if let Some(precision) = filters.quantity_precision {
        size = size.round_dp_with_strategy(precision, RoundingStrategy::ToZero);
    }

    if size <= Decimal::ZERO {
        return Err(anyhow!(
            "order size is below exchange step size after quantization"
        ));
    }
    if let Some(min_qty) = filters.min_qty {
        if size < min_qty {
            return Err(anyhow!(
                "order size {} is below exchange min_qty {}",
                format_order_size_decimal(size, filters),
                min_qty
            ));
        }
    }
    if let Some(max_qty) = filters.max_qty {
        if max_qty > Decimal::ZERO && size > max_qty {
            return Err(anyhow!(
                "order size {} is above exchange max_qty {}",
                format_order_size_decimal(size, filters),
                max_qty
            ));
        }
    }
    if enforce_min_notional {
        if let Some(min_notional) = filters.min_notional {
            let notional = size * last_price;
            if min_notional > Decimal::ZERO && notional < min_notional {
                return Err(anyhow!(
                    "order notional {} is below exchange min_notional {} after size quantization",
                    notional,
                    min_notional
                ));
            }
        }
    }

    Ok(size)
}

fn format_order_size_decimal(size: Decimal, filters: &ExchangeOrderFilters) -> String {
    let precision = filters
        .quantity_precision
        .or_else(|| filters.step_size.map(|step| step.scale()));
    let normalized = match precision {
        Some(precision) => size.round_dp_with_strategy(precision, RoundingStrategy::ToZero),
        None => size,
    }
    .normalize();
    normalized.to_string()
}

fn quantize_protective_stop_price(
    price: f64,
    direction: ProtectiveDirection,
    filters: &ExchangeOrderFilters,
) -> Result<Decimal> {
    let price = decimal_from_f64(price)?;
    let step = filters
        .tick_size
        .filter(|value| *value > Decimal::ZERO)
        .or_else(|| {
            filters
                .price_precision
                .map(|precision| Decimal::new(1, precision))
        });
    let Some(step) = step else {
        return Ok(price);
    };

    let normalized = match direction {
        ProtectiveDirection::Long => floor_to_step(price, step),
        ProtectiveDirection::Short => ceil_to_step(price, step),
    };
    if normalized <= Decimal::ZERO {
        return Err(anyhow!(
            "protective stop price is below exchange tick size after quantization"
        ));
    }
    Ok(normalized)
}

fn format_protective_stop_price_decimal(price: Decimal, filters: &ExchangeOrderFilters) -> String {
    let precision = filters
        .price_precision
        .or_else(|| filters.tick_size.map(|step| step.scale()));
    let normalized = match precision {
        Some(precision) => price.round_dp_with_strategy(precision, RoundingStrategy::ToZero),
        None => price,
    }
    .normalize();
    normalized.to_string()
}

fn is_zero_order_size(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .map(|raw| raw == 0.0)
        .unwrap_or(false)
}

fn is_pending_close_task(task: &ExecutionTask) -> bool {
    task.task_type == "risk_control_close_candidate"
        && matches!(task.task_status.as_str(), "pending_close" | "leased")
}

fn payload_bool(payload: &Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(|value| match value {
        Value::Bool(raw) => Some(*raw),
        Value::String(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

fn validate_execute_signal_risk_contract(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
) -> std::result::Result<(), RiskContractViolation> {
    let payload = order_payload(&task.request_payload_json);
    if !protective_stop_loss_required(&payload, task.news_signal_id.is_some()) {
        return Ok(());
    }

    let selected_stop_loss_price_raw = selected_stop_loss_price(&payload);
    let Some(selected_stop_loss_price) =
        selected_stop_loss_price_raw.filter(|price| price.is_finite() && *price > 0.0)
    else {
        return Err(risk_contract_violation(
            task,
            order_task,
            "missing_selected_stop_loss_price",
            "protective stop-loss required but risk_plan.selected_stop_loss_price is missing or invalid",
            json!({
                "missing_field": "risk_plan.selected_stop_loss_price",
                "selected_stop_loss_price": selected_stop_loss_price_raw,
            }),
        ));
    };

    let direction = match risk_plan_direction_raw(&payload) {
        Some(raw) => match parse_protective_direction(&raw) {
            Ok(direction) => direction,
            Err(error) => {
                return Err(risk_contract_violation(
                    task,
                    order_task,
                    "invalid_direction",
                    error.to_string(),
                    json!({
                        "invalid_direction": raw,
                        "selected_stop_loss_price": selected_stop_loss_price,
                    }),
                ));
            }
        },
        None => direction_from_order_side(order_task.side),
    };

    if let Some(entry_price) = protection_entry_price(&payload) {
        let invalid_stop = match direction {
            ProtectiveDirection::Long => selected_stop_loss_price >= entry_price,
            ProtectiveDirection::Short => selected_stop_loss_price <= entry_price,
        };
        if invalid_stop {
            return Err(risk_contract_violation(
                task,
                order_task,
                "invalid_stop_loss_price",
                "invalid protective stop-loss price for entry direction",
                json!({
                    "entry_price": entry_price,
                    "selected_stop_loss_price": selected_stop_loss_price,
                    "direction": direction.as_str(),
                }),
            ));
        }
    }

    Ok(())
}

fn protective_stop_loss_required(
    payload: &Value,
    task_news_signal_requires_stop_loss: bool,
) -> bool {
    payload_bool(payload, "protective_stop_loss_required")
        .or_else(|| payload_bool(payload, "stop_loss_required"))
        .or_else(|| {
            payload
                .get("execution")
                .and_then(|value| payload_bool(value, "protective_stop_loss_required"))
        })
        .or_else(|| {
            payload
                .get("execution")
                .and_then(|value| payload_bool(value, "stop_loss_required"))
        })
        .or_else(|| {
            payload
                .get("risk_plan")
                .and_then(|value| payload_bool(value, "protective_stop_loss_required"))
        })
        .or_else(|| {
            payload
                .get("risk_plan")
                .and_then(|value| payload_bool(value, "stop_loss_required"))
        })
        .unwrap_or(false)
        || task_news_signal_requires_stop_loss
        || news_signal_requires_protective_stop_loss(payload)
        || selected_stop_loss_price(payload).is_some()
}

fn news_signal_requires_protective_stop_loss(payload: &Value) -> bool {
    let Some(source_signal_type) = payload_string(payload, "source_signal_type") else {
        return false;
    };
    let normalized = source_signal_type.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "news_event" | "news")
}

fn selected_stop_loss_price(payload: &Value) -> Option<f64> {
    payload
        .get("risk_plan")
        .and_then(|value| payload_f64(value, "selected_stop_loss_price"))
        .or_else(|| payload_f64(payload, "selected_stop_loss_price"))
        .or_else(|| {
            payload
                .get("execution")
                .and_then(|value| payload_f64(value, "selected_stop_loss_price"))
        })
}

fn protection_entry_price(payload: &Value) -> Option<f64> {
    payload
        .get("risk_plan")
        .and_then(|value| payload_f64(value, "entry_price"))
        .or_else(|| payload_f64(payload, "entry_price"))
        .or_else(|| nested_payload_f64(payload, "signal", "open_price"))
        .or_else(|| payload_f64(payload, "open_price"))
        .or_else(|| payload_f64(payload, "price"))
}

fn risk_plan_direction_raw(payload: &Value) -> Option<String> {
    payload
        .get("risk_plan")
        .and_then(|value| payload_string(value, "direction"))
        .or_else(|| payload_string(payload, "direction"))
        .or_else(|| payload_string(payload, "position_side"))
        .or_else(|| payload_string(payload, "side"))
        .or_else(|| payload_string(payload, "signal_type"))
}

fn parse_protective_direction(raw: &str) -> Result<ProtectiveDirection> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "buy" | "long" | "open_long" => Ok(ProtectiveDirection::Long),
        "sell" | "short" | "open_short" => Ok(ProtectiveDirection::Short),
        other => Err(anyhow!(
            "unsupported protective stop-loss direction: {}",
            other
        )),
    }
}

fn direction_from_order_side(side: OrderSide) -> ProtectiveDirection {
    match side {
        OrderSide::Buy => ProtectiveDirection::Long,
        OrderSide::Sell => ProtectiveDirection::Short,
    }
}

fn risk_contract_violation(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
    reason: &str,
    message: impl Into<String>,
    details: Value,
) -> RiskContractViolation {
    let payload = order_payload(&task.request_payload_json);
    let mut raw_payload = json!({
        "risk_contract": {
            "task_id": task.id,
            "task_type": task.task_type,
            "exchange": order_task.exchange.as_str(),
            "symbol": order_task.symbol,
            "order_side": order_side_lower(order_task.side),
            "protective_stop_loss_required": true,
            "place_order_allowed": false,
            "reason": reason,
        },
        "risk_plan": payload.get("risk_plan").cloned().unwrap_or(Value::Null),
    });
    if let (Some(contract), Some(details)) = (
        raw_payload
            .get_mut("risk_contract")
            .and_then(Value::as_object_mut),
        details.as_object(),
    ) {
        for (key, value) in details {
            contract.insert(key.clone(), value.clone());
        }
    }
    if let Some(source_signal_type) = payload_string(&payload, "source_signal_type") {
        if let Some(contract) = raw_payload
            .get_mut("risk_contract")
            .and_then(Value::as_object_mut)
        {
            contract.insert("source_signal_type".to_string(), json!(source_signal_type));
        }
    }
    if let Some(news_signal_id) = task.news_signal_id {
        if let Some(contract) = raw_payload
            .get_mut("risk_contract")
            .and_then(Value::as_object_mut)
        {
            contract.insert("news_signal_id".to_string(), json!(news_signal_id));
        }
    }

    RiskContractViolation {
        message: message.into(),
        raw_payload,
    }
}

fn close_order_side(payload: &Value) -> Result<OrderSide> {
    if let Some(side) = payload_string(payload, "side") {
        return parse_side(&side);
    }
    if let Some(position_side) = payload_string(payload, "position_side") {
        return match position_side.trim().to_ascii_lowercase().as_str() {
            "long" => Ok(OrderSide::Sell),
            "short" => Ok(OrderSide::Buy),
            other => Err(anyhow!(
                "unsupported close_order.position_side for pending_close: {}",
                other
            )),
        };
    }
    Err(anyhow!(
        "pending_close close_order requires side or position_side"
    ))
}

fn parse_env_list(key: &str, defaults: &[&str]) -> Vec<String> {
    let values = std::env::var(key)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if values.is_empty() {
        defaults.iter().map(|value| value.to_string()).collect()
    } else {
        values
    }
}

fn parse_env_i64_list(key: &str) -> Vec<i64> {
    let Some(raw) = std::env::var(key).ok() else {
        return Vec::new();
    };

    let mut values = Vec::new();
    let mut invalid_values = Vec::new();
    for value in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        match value.parse::<i64>() {
            Ok(parsed) => values.push(parsed),
            Err(_) => invalid_values.push(value.to_string()),
        }
    }

    if !invalid_values.is_empty() {
        warn!(
            env_key = key,
            invalid_values = ?invalid_values,
            "invalid execution worker target task ids; denying all leased tasks"
        );
        return vec![i64::MIN];
    }

    values
}

fn parse_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn live_order_confirmation_valid(dry_run: bool, confirmation: Option<&str>) -> bool {
    dry_run
        || confirmation
            .map(str::trim)
            .is_some_and(|value| value == LIVE_ORDER_CONFIRM_TOKEN)
}

fn ensure_live_order_confirmation() -> Result<()> {
    let confirmation = std::env::var(LIVE_ORDER_CONFIRM_ENV).ok();
    if live_order_confirmation_valid(false, confirmation.as_deref()) {
        Ok(())
    } else {
        Err(anyhow!(
            "refusing live exchange orders: set {}={} after validating API keys, task filters, and exchange environment",
            LIVE_ORDER_CONFIRM_ENV,
            LIVE_ORDER_CONFIRM_TOKEN
        ))
    }
}

fn parse_exchange(raw: &str) -> Result<ExchangeId> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "币安" => Ok(ExchangeId::Binance),
        other => ExchangeId::from_str(other).map_err(anyhow::Error::msg),
    }
}

fn parse_side(raw: &str) -> Result<OrderSide> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "buy" | "long" | "open_long" => Ok(OrderSide::Buy),
        "sell" | "short" | "open_short" => Ok(OrderSide::Sell),
        other => Err(anyhow!("unsupported order side: {}", other)),
    }
}

fn parse_order_type(raw: &str) -> Result<OrderType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "market" => Ok(OrderType::Market),
        "limit" => Ok(OrderType::Limit),
        other => Err(anyhow!("unsupported order type: {}", other)),
    }
}

fn parse_time_in_force(raw: &str) -> Result<TimeInForce> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "gtc" => Ok(TimeInForce::Gtc),
        "ioc" => Ok(TimeInForce::Ioc),
        "fok" => Ok(TimeInForce::Fok),
        "post_only" | "postonly" => Ok(TimeInForce::PostOnly),
        other => Err(anyhow!("unsupported time_in_force: {}", other)),
    }
}

fn parse_position_mode(raw: &str) -> Result<PositionMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "hedge" => Ok(PositionMode::Hedge),
        "one_way" | "oneway" | "net" => Ok(PositionMode::OneWay),
        other => Err(anyhow!("unsupported position_mode: {}", other)),
    }
}

fn is_duplicate_client_order_id_error(error_message: &str) -> bool {
    let normalized = error_message
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    (normalized.contains("duplicate")
        || normalized.contains("alreadyused")
        || (normalized.contains("already") && normalized.contains("used")))
        && (normalized.contains("clientorderid")
            || normalized.contains("clientoid")
            || normalized.contains("clordid"))
}

#[derive(Debug, Clone)]
struct PrePlaceClientOrderLookup {
    query: OrderQuery,
    ack: OrderAck,
}

fn pre_place_client_order_lookup(
    request: &OrderPlacementRequest,
) -> Option<PrePlaceClientOrderLookup> {
    let client_order_id = request
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let mut query =
        OrderQuery::by_client_order_id(request.instrument.clone(), client_order_id.clone());
    if let Some(margin_coin) = request
        .margin_coin
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query = query.with_margin_coin(margin_coin.to_string());
    }

    Some(PrePlaceClientOrderLookup {
        query,
        ack: OrderAck {
            exchange: request.exchange,
            instrument: request.instrument.clone(),
            exchange_symbol: request.instrument.symbol_for(request.exchange),
            order_id: None,
            client_order_id: Some(client_order_id.clone()),
            status: Some("client_order_id_pre_place_check".to_string()),
            raw: json!({
                "reconciliation": {
                    "reason": "client_order_id_pre_place_check",
                    "action": "query_existing_order_before_place_order",
                    "place_order_allowed": false,
                    "place_order_retried": false,
                },
                "client_order_id": client_order_id,
            }),
        },
    })
}

fn is_order_not_found_for_client_order_preflight(error_message: &str) -> bool {
    let normalized = error_message.to_ascii_lowercase();
    normalized.contains("-2013")
        || normalized.contains("order does not exist")
        || normalized.contains("order not found")
        || normalized.contains("not found")
        || normalized.contains("not exist")
}

fn client_order_id_owner_violation_report(
    task_id: i64,
    task_type: &str,
    order_side: &str,
    request: &OrderPlacementRequest,
) -> Option<ExecutionTaskReportRequest> {
    let client_order_id = request.client_order_id.as_deref()?.trim();
    let owner_task_id = generated_client_order_id_owner_task_id(task_type, client_order_id)?;
    if owner_task_id == task_id {
        return None;
    }

    Some(ExecutionTaskReportRequest::failed(
        task_id,
        request.exchange.as_str(),
        order_side,
        format!(
            "client_order_id {client_order_id} belongs to task {owner_task_id}, not task {task_id}"
        ),
        json!({
            "task_id": task_id,
            "stage": "client_order_id_owner_check",
            "exchange": request.exchange.as_str(),
            "symbol": request.instrument.symbol_for(request.exchange),
            "client_order_id": client_order_id,
            "client_order_id_owner_task_id": owner_task_id,
            "expected_task_id": task_id,
            "place_order_allowed": false,
            "mutation_allowed": false,
            "protection_sync_allowed": false,
            "reconciliation": {
                "reason": "client_order_id_owner_mismatch",
                "action": "blocked_foreign_client_order_id",
                "place_order_retried": false,
            },
        }),
    ))
}

fn generated_client_order_id_owner_task_id(task_type: &str, client_order_id: &str) -> Option<i64> {
    let prefix = match task_type {
        "execute_signal" => "rqtask",
        "risk_control_close_candidate" => "rqclose",
        _ => return None,
    };
    parse_generated_client_order_id_owner(client_order_id, prefix)
}

fn parse_generated_client_order_id_owner(client_order_id: &str, prefix: &str) -> Option<i64> {
    let suffix = client_order_id.trim().strip_prefix(prefix)?;
    if suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn duplicate_client_order_id_reconciliation_ack(
    request: &OrderPlacementRequest,
) -> Option<OrderAck> {
    let client_order_id = request
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    Some(OrderAck {
        exchange: request.exchange,
        instrument: request.instrument.clone(),
        exchange_symbol: request.instrument.symbol_for(request.exchange),
        order_id: None,
        client_order_id: Some(client_order_id.clone()),
        status: Some("duplicate_client_order_id".to_string()),
        raw: json!({
            "reconciliation": {
                "reason": "duplicate_client_order_id",
                "action": "query_existing_order_by_client_order_id",
                "place_order_retried": false,
            },
            "client_order_id": client_order_id,
        }),
    })
}

async fn confirm_live_order(
    gateway: &CryptoExcAllGateway,
    ack: &OrderAck,
) -> Result<(Order, Vec<Fill>)> {
    let query = if let Some(order_id) = ack.order_id.as_deref() {
        OrderQuery::by_order_id(ack.instrument.clone(), order_id)
    } else if let Some(client_order_id) = ack.client_order_id.as_deref() {
        OrderQuery::by_client_order_id(ack.instrument.clone(), client_order_id)
    } else {
        return Err(anyhow!(
            "place_order ack missing order_id and client_order_id for confirmation"
        ));
    };

    let mut confirmed_order = None;
    for attempt in 0..3 {
        let order = gateway.order(ack.exchange, query.clone()).await?;
        let status = order.status.as_deref().unwrap_or_default();
        if status != "NEW" || attempt == 2 {
            confirmed_order = Some(order);
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }
    let order = confirmed_order.expect("confirmation loop must set order");
    let order_id = order.order_id.as_deref().or(ack.order_id.as_deref());
    let fills = if let Some(order_id) = order_id {
        match gateway
            .fills(
                ack.exchange,
                FillListQuery::for_instrument(ack.instrument.clone())
                    .with_order_id(order_id)
                    .with_limit(100),
            )
            .await
        {
            Ok(fills) => fills,
            Err(error) => {
                warn!(
                    exchange = ack.exchange.as_str(),
                    order_id, "live order fills confirmation failed: {}", error
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Ok((order, fills))
}

fn build_confirmed_order_report(
    task_id: i64,
    order_side: &str,
    ack: &OrderAck,
    order: Option<Order>,
    fills: Vec<Fill>,
    confirmation_error: Option<String>,
    protection: Option<ProtectionSyncContract>,
) -> ExecutionTaskReportRequest {
    let external_order_id = order
        .as_ref()
        .and_then(|order| order.order_id.as_deref())
        .or(ack.order_id.as_deref())
        .or_else(|| {
            order
                .as_ref()
                .and_then(|order| order.client_order_id.as_deref())
        })
        .or(ack.client_order_id.as_deref())
        .unwrap_or("unknown");
    let order_status = order
        .as_ref()
        .and_then(|order| order.status.as_deref())
        .or(ack.status.as_deref())
        .unwrap_or("submitted");
    let execution_status = live_order_execution_status(order_status);

    let filled_qty = order
        .as_ref()
        .and_then(|order| parse_optional_f64(order.filled_size.as_deref()))
        .or_else(|| sum_fill_sizes(&fills));
    let filled_quote = sum_fill_quote(&fills).or_else(|| {
        let qty = filled_qty?;
        let avg_price = order
            .as_ref()
            .and_then(|order| parse_optional_f64(order.average_price.as_deref()))?;
        Some(qty * avg_price)
    });
    let fee_amount = sum_fill_fees(&fills);

    let raw_payload = json!({
        "ack": ack.raw,
        "order_detail": order.as_ref().map(|order| order.raw.clone()),
        "fills": fills.iter().map(|fill| fill.raw.clone()).collect::<Vec<_>>(),
        "confirmation_error": confirmation_error,
        "execution_status": execution_status,
    });

    let mut report = ExecutionTaskReportRequest::success(
        task_id,
        ack.exchange.as_str(),
        external_order_id,
        order_side,
        order_status,
        raw_payload,
    );
    report.execution_status = execution_status.to_string();
    if execution_status == "failed" {
        report.error_message = Some(format!("live order terminal status: {order_status}"));
    }
    report.filled_qty = filled_qty;
    report.filled_quote = filled_quote;
    report.fee_amount = fee_amount;
    if let Some(protection) = protection {
        protection.apply_to_report(&mut report);
    }
    report
}

fn build_confirmed_order_report_for_task(
    task: &ExecutionTask,
    order_side: &str,
    ack: &OrderAck,
    order: Option<Order>,
    fills: Vec<Fill>,
    confirmation_error: Option<String>,
    protection: Option<ProtectionSyncContract>,
) -> ExecutionTaskReportRequest {
    let mut report = build_confirmed_order_report(
        task.id,
        order_side,
        ack,
        order,
        fills,
        confirmation_error,
        protection,
    );
    attach_execution_task_context_to_report(&mut report, task);
    report
}

fn attach_execution_task_context_to_report(
    report: &mut ExecutionTaskReportRequest,
    task: &ExecutionTask,
) {
    let mut raw_payload = report
        .raw_payload_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));
    let source_signal_type = task
        .request_payload_json
        .get("source_signal_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if task.news_signal_id.is_some() {
                "news_event".to_string()
            } else {
                "technical_strategy".to_string()
            }
        });

    raw_payload["execution_task"] = json!({
        "task_id": task.id,
        "news_signal_id": task.news_signal_id,
        "strategy_signal_id": task.strategy_signal_id,
        "combo_id": task.combo_id,
        "strategy_slug": task.strategy_slug,
        "symbol": task.symbol,
        "task_type": task.task_type,
        "source_signal_type": source_signal_type,
    });
    report.raw_payload_json = Some(raw_payload.to_string());
}

fn live_order_execution_status(order_status: &str) -> &'static str {
    match order_status.trim().to_ascii_uppercase().as_str() {
        "FILLED" => "completed",
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => "failed",
        _ => "pending_confirmation",
    }
}

fn parse_optional_f64(value: Option<&str>) -> Option<f64> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())
}

fn sum_fill_sizes(fills: &[Fill]) -> Option<f64> {
    let mut total = 0.0;
    let mut seen = false;
    for fill in fills {
        if let Some(size) = parse_optional_f64(fill.size.as_deref()) {
            total += size;
            seen = true;
        }
    }
    seen.then_some(total)
}

fn sum_fill_quote(fills: &[Fill]) -> Option<f64> {
    let mut total = 0.0;
    let mut seen = false;
    for fill in fills {
        let price = parse_optional_f64(fill.price.as_deref());
        let size = parse_optional_f64(fill.size.as_deref());
        if let (Some(price), Some(size)) = (price, size) {
            total += price * size;
            seen = true;
        }
    }
    seen.then_some(total)
}

fn sum_fill_fees(fills: &[Fill]) -> Option<f64> {
    let mut total = 0.0;
    let mut seen = false;
    for fill in fills {
        if let Some(fee) = parse_optional_f64(fill.fee.as_deref()) {
            total += fee;
            seen = true;
        }
    }
    seen.then_some(total)
}

fn parse_instrument(symbol: &str) -> Result<Instrument> {
    let normalized = symbol.trim().to_ascii_uppercase();
    let parts: Vec<&str> = normalized.split('-').collect();
    if parts.len() >= 3 && parts[2] == "SWAP" {
        return Ok(Instrument::perp(parts[0], parts[1]).with_settlement(parts[1]));
    }
    if parts.len() == 2 {
        return Ok(Instrument::spot(parts[0], parts[1]));
    }
    if let Some(base) = normalized.strip_suffix("USDT") {
        return Ok(Instrument::perp(base, "USDT").with_settlement("USDT"));
    }
    Err(anyhow!("unsupported symbol format: {}", symbol))
}

fn order_side_lower(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_quan_web::{
        ExchangeReconciliationIssueType, ExecutionTask, ReportResultReplayCandidate,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn task(payload: serde_json::Value) -> ExecutionTask {
        task_with_metadata("execute_signal", "pending", payload)
    }

    fn binance_eth_filters() -> ExchangeOrderFilters {
        ExchangeOrderFilters {
            min_qty: Some("0.001".parse().unwrap()),
            max_qty: Some("10000".parse().unwrap()),
            step_size: Some("0.001".parse().unwrap()),
            min_notional: Some("20".parse().unwrap()),
            quantity_precision: Some(3),
            tick_size: Some("0.01".parse().unwrap()),
            price_precision: Some(2),
        }
    }

    fn task_with_metadata(
        task_type: &str,
        task_status: &str,
        payload: serde_json::Value,
    ) -> ExecutionTask {
        ExecutionTask {
            id: 42,
            news_signal_id: None,
            strategy_signal_id: None,
            combo_id: 9,
            buyer_email: "buyer@example.com".to_string(),
            strategy_slug: "news_momentum".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            task_type: task_type.to_string(),
            task_status: task_status.to_string(),
            priority: 3,
            lease_owner: None,
            lease_until: None,
            scheduled_at: "2026-04-23T12:00:00".to_string(),
            request_payload_json: payload,
            created_at: "2026-04-23T12:00:00".to_string(),
            updated_at: "2026-04-23T12:00:00".to_string(),
        }
    }

    #[derive(Default)]
    struct CapturingAuditRepository {
        checkpoints: Mutex<Vec<ExecutionWorkerCheckpoint>>,
        audits: Mutex<Vec<ExchangeRequestAuditLog>>,
        report_replay_candidates: Mutex<Vec<ReportResultReplayCandidate>>,
        report_replay_queries: Mutex<Vec<(u32, u64)>>,
    }

    #[async_trait]
    impl ExecutionAuditRepository for CapturingAuditRepository {
        async fn upsert_worker_checkpoint(
            &self,
            checkpoint: &ExecutionWorkerCheckpoint,
        ) -> Result<()> {
            self.checkpoints.lock().unwrap().push(checkpoint.clone());
            Ok(())
        }

        async fn insert_exchange_request_audit(
            &self,
            audit: &ExchangeRequestAuditLog,
        ) -> Result<()> {
            self.audits.lock().unwrap().push(audit.clone());
            Ok(())
        }

        async fn list_report_result_replay_candidates(
            &self,
            limit: u32,
            failure_backoff_seconds: u64,
        ) -> Result<Vec<ReportResultReplayCandidate>> {
            self.report_replay_queries
                .lock()
                .unwrap()
                .push((limit, failure_backoff_seconds));
            let mut candidates = self.report_replay_candidates.lock().unwrap();
            let take = usize::min(candidates.len(), limit as usize);
            Ok(candidates.drain(..take).collect())
        }
    }

    #[test]
    fn position_stale_reconciliation_request_from_task_uses_idempotent_source_ref() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.01"
        }));

        let request = build_exchange_reconciliation_report_request(
            &task,
            ExchangeReconciliationIssueType::ExchangePositionStale,
            Some("2026-05-15T09:30:00Z".to_string()),
            "position drift detected",
        );
        let repeated = build_exchange_reconciliation_report_request(
            &task,
            ExchangeReconciliationIssueType::ExchangePositionStale,
            Some("2026-05-15T09:31:00Z".to_string()),
            "position drift detected again",
        );

        assert_eq!(request.combo_id, 9);
        assert_eq!(request.buyer_email, "buyer@example.com");
        assert_eq!(request.symbol, "ETHUSDT");
        assert_eq!(
            request.issue_type,
            ExchangeReconciliationIssueType::ExchangePositionStale
        );
        assert_eq!(
            request.source_ref.as_deref(),
            Some(
                "rust_quant/exchange_reconciliation/exchange_position_stale/combo/9/task/42/symbol/ETHUSDT"
            )
        );
        assert_eq!(request.source_ref, repeated.source_ref);
    }

    #[test]
    fn open_order_conflict_reconciliation_request_from_task_uses_allowed_issue_type() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.01"
        }));

        let request = build_exchange_reconciliation_report_request(
            &task,
            ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
            None,
            "unexpected open order blocks execution",
        );

        assert_eq!(
            request.issue_type,
            ExchangeReconciliationIssueType::ExchangeOpenOrderConflict
        );
        assert_eq!(
            request.source_ref.as_deref(),
            Some(
                "rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT"
            )
        );
        assert_eq!(
            request.message.as_deref(),
            Some("unexpected open order blocks execution")
        );
    }

    #[test]
    fn read_only_exchange_snapshot_builds_reconciliation_requests_without_live_mutation() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.01"
        }));
        let instrument = Instrument::perp("ETH", "USDT");
        let positions = vec![Position {
            exchange: ExchangeId::Binance,
            instrument: instrument.clone(),
            exchange_symbol: "ETHUSDT".to_string(),
            side: Some("LONG".to_string()),
            size: "0.02".to_string(),
            entry_price: Some("3136".to_string()),
            mark_price: Some("3140".to_string()),
            unrealized_pnl: None,
            leverage: None,
            margin_mode: None,
            liquidation_price: None,
            raw: json!({"secret":"position-raw-should-not-render"}),
        }];
        let open_orders = vec![Order {
            exchange: ExchangeId::Binance,
            instrument,
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("open-1".to_string()),
            client_order_id: Some("client-open-1".to_string()),
            side: Some("SELL".to_string()),
            order_type: Some("STOP_MARKET".to_string()),
            price: None,
            size: Some("0.02".to_string()),
            filled_size: Some("0".to_string()),
            average_price: None,
            status: Some("NEW".to_string()),
            created_at: Some(1),
            updated_at: Some(2),
            raw: json!({"secret":"open-order-raw-should-not-render"}),
        }];

        let requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
            &task,
            &positions,
            &open_orders,
            Some("2026-05-15T10:00:00Z".to_string()),
        );

        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].issue_type,
            ExchangeReconciliationIssueType::ExchangePositionStale
        );
        assert_eq!(
            requests[0].source_ref.as_deref(),
            Some(
                "rust_quant/exchange_reconciliation/exchange_position_stale/combo/9/task/42/symbol/ETHUSDT"
            )
        );
        assert_eq!(
            requests[1].issue_type,
            ExchangeReconciliationIssueType::ExchangeOpenOrderConflict
        );
        assert_eq!(
            requests[1].source_ref.as_deref(),
            Some(
                "rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT"
            )
        );
        let rendered = serde_json::to_string(&requests).unwrap();
        assert!(rendered.contains("read-only exchange snapshot"));
        assert!(rendered.contains("place_order_allowed=false"));
        assert!(!rendered.contains("position-raw-should-not-render"));
        assert!(!rendered.contains("open-order-raw-should-not-render"));
    }

    #[test]
    fn live_order_reconciliation_conflict_builds_no_mutation_failed_report() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.01"
        }));
        let order_task =
            ExecutionOrderTask::from_task_with_default(&task, ExchangeId::Binance).unwrap();
        let requests = vec![
            build_exchange_reconciliation_report_request(
                &task,
                ExchangeReconciliationIssueType::ExchangePositionStale,
                Some("2026-05-15T10:00:00Z".to_string()),
                "read-only exchange snapshot detected 1 non-zero position(s); place_order_allowed=false; mutation_allowed=false",
            ),
            build_exchange_reconciliation_report_request(
                &task,
                ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
                Some("2026-05-15T10:00:00Z".to_string()),
                "read-only exchange snapshot detected 1 open order(s); place_order_allowed=false; mutation_allowed=false",
            ),
        ];

        let report = build_live_order_blocked_by_exchange_reconciliation_report(
            &task,
            &order_task,
            &requests,
        );
        let raw_payload: Value =
            serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.order_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "buy");
        assert!(report
            .error_message
            .as_deref()
            .unwrap()
            .contains("read-only exchange reconciliation"));
        assert_eq!(raw_payload["stage"], "exchange_reconciliation_read_only");
        assert_eq!(raw_payload["place_order_allowed"], false);
        assert_eq!(raw_payload["mutation_allowed"], false);
        assert_eq!(raw_payload["issues"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn maps_task_payload_to_order_request() {
        let task = task(json!({
            "exchange": "okx",
            "symbol": "BTC-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size": "0.01",
            "margin_mode": "cross",
            "position_side": "long",
            "trade_side": "open"
        }));

        let request = ExecutionOrderTask::from_task(&task).unwrap();
        let order = request.to_order_request().unwrap();

        assert_eq!(order.exchange.as_str(), "okx");
        assert_eq!(order.instrument.symbol_for(order.exchange), "BTC-USDT-SWAP");
        assert_eq!(order.size, "0.01");
        assert_eq!(order.client_order_id.as_deref(), Some("rqtask42"));
    }

    #[test]
    fn maps_nested_news_signal_payload_to_order_request() {
        let task = task(json!({
            "symbol": "BTC-USDT-SWAP",
            "signal_type": "buy",
            "payload_json": "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.001\",\"order_type\":\"market\",\"client_order_id\":\"smoke-dry-run-42\"}"
        }));

        let request = ExecutionOrderTask::from_task(&task).unwrap();
        let order = request.to_order_request().unwrap();

        assert_eq!(order.exchange.as_str(), "okx");
        assert_eq!(order.size, "0.001");
        assert_eq!(order.client_order_id.as_deref(), Some("smoke-dry-run-42"));
    }

    #[test]
    fn maps_web_execution_payload_to_order_request() {
        let task = task(json!({
            "source": "rust_quant",
            "symbol": "ETH-USDT-SWAP",
            "signal_type": "entry",
            "direction": "long",
            "payload_json": "{\"signal\":{\"open_price\":3500.0},\"client_order_id\":\"rq421704067200000\"}",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_settings": {
                "max_position_usdt": 35.0,
                "risk_acknowledged": true,
                "status": "active"
            }
        }));

        let request = ExecutionOrderTask::from_task(&task).unwrap();
        let order = request.to_order_request().unwrap();

        assert_eq!(order.exchange.as_str(), "binance");
        assert_eq!(request.symbol, "ETH-USDT-SWAP");
        assert_eq!(order.instrument.symbol_for(order.exchange), "ETHUSDT");
        assert_eq!(order.size, "0.01");
        assert_eq!(order.client_order_id.as_deref(), Some("rq421704067200000"));
    }

    #[test]
    fn filled_open_long_builds_binance_protective_stop_market_sell_request() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.01",
            "client_order_id": "rq-open-42",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.0,
                "direction": "long"
            }
        }));
        let order_task = ExecutionOrderTask::from_task(&task).unwrap();
        let protection = ProtectionSyncContract::from_task(&task, "buy").unwrap();

        let request = build_protective_stop_market_order_request(
            &order_task,
            &protection,
            &binance_eth_filters(),
        )
        .unwrap();

        assert_eq!(
            request.instrument.symbol_for(order_task.exchange),
            "ETHUSDT"
        );
        assert_eq!(request.side, OrderSide::Sell);
        assert_eq!(request.stop_price, "2200");
        assert_eq!(request.close_position, Some(true));
        assert_eq!(request.price_protect, Some(true));
        assert_eq!(
            request.working_type,
            Some(ProtectiveOrderWorkingType::MarkPrice)
        );
        assert_eq!(request.client_order_id.as_deref(), Some("rq-sl-42"));
    }

    #[test]
    fn technical_strategy_selected_stop_loss_requires_protection_without_flag() {
        let task = task(json!({
            "source": "rust_quant",
            "source_signal_type": "technical_strategy",
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.024",
            "risk_plan": {
                "selected_stop_loss_price": 2134.82,
                "direction": "long"
            }
        }));
        let payload = order_payload(&task.request_payload_json);

        assert!(protective_stop_loss_required(&payload, false));
    }

    #[test]
    fn filled_open_short_builds_binance_protective_stop_market_buy_request() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "sell",
            "size": "0.01",
            "position_side": "short",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2600.0,
                "direction": "short"
            }
        }));
        let order_task = ExecutionOrderTask::from_task(&task).unwrap();
        let protection = ProtectionSyncContract::from_task(&task, "sell").unwrap();

        let request = build_protective_stop_market_order_request(
            &order_task,
            &protection,
            &binance_eth_filters(),
        )
        .unwrap();

        assert_eq!(request.side, OrderSide::Buy);
        assert_eq!(request.stop_price, "2600");
        assert_eq!(request.position_side.as_deref(), Some("short"));
        assert_eq!(request.close_position, Some(true));
        assert_eq!(request.client_order_id.as_deref(), Some("rq-sl-42"));
    }

    #[test]
    fn protective_stop_price_is_quantized_to_exchange_tick_size() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "size": "0.011",
            "position_side": "long",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2254.3724,
                "direction": "long"
            }
        }));
        let order_task = ExecutionOrderTask::from_task(&task).unwrap();
        let protection = ProtectionSyncContract::from_task(&task, "buy").unwrap();

        let request = build_protective_stop_market_order_request(
            &order_task,
            &protection,
            &binance_eth_filters(),
        )
        .unwrap();

        assert_eq!(request.stop_price, "2254.37");
    }

    #[test]
    fn short_protective_stop_price_rounds_up_to_exchange_tick_size() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "sell",
            "size": "0.011",
            "position_side": "short",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2254.3724,
                "direction": "short"
            }
        }));
        let order_task = ExecutionOrderTask::from_task(&task).unwrap();
        let protection = ProtectionSyncContract::from_task(&task, "sell").unwrap();

        let request = build_protective_stop_market_order_request(
            &order_task,
            &protection,
            &binance_eth_filters(),
        )
        .unwrap();

        assert_eq!(request.stop_price, "2254.38");
    }

    #[test]
    fn filled_long_with_stale_strategy_reference_rebases_protective_stop_below_fill_price() {
        let protection = ProtectionSyncContract::required(
            json!({
                "risk_plan": {
                    "protective_stop_loss_required": true,
                    "entry_price": 2300.38,
                    "selected_stop_loss_price": 2254.3724,
                    "direction": "long"
                }
            }),
            "buy",
        )
        .expect("valid protection contract");
        let mut report = ExecutionTaskReportRequest::success(
            181,
            "binance",
            "8389766181415858769",
            "buy",
            "FILLED",
            json!({"execution_status":"pending_protection_sync"}),
        );
        report.filled_qty = Some(0.011);
        report.filled_quote = Some(24.12091);

        let adjusted = ProtectionSyncContract::from_task_result(&report, Some(protection))
            .expect("filled order should require protection");

        let fill_price = report.filled_quote.unwrap() / report.filled_qty.unwrap();
        assert!(
            adjusted.selected_stop_loss_price < fill_price,
            "long protective stop must be below fill price to avoid immediate trigger"
        );
        assert!((adjusted.selected_stop_loss_price - 2148.9538).abs() < 0.0001);
    }

    #[test]
    fn protective_order_ack_maps_to_confirmed_sync_outcome() {
        let ack = OrderAck {
            exchange: ExchangeId::Binance,
            exchange_symbol: "ETHUSDT".to_string(),
            instrument: Instrument::perp("ETH", "USDT").with_settlement("USDT"),
            order_id: Some("sl-123".to_string()),
            client_order_id: Some("rq-sl-42".to_string()),
            status: Some("NEW".to_string()),
            raw: json!({"orderId":"sl-123", "status":"NEW"}),
        };

        let outcome = protective_order_result_to_sync_outcome(Ok(ack));

        assert_eq!(
            outcome,
            ProtectionSyncOutcome::confirmed("sl-123", "place_protective_order")
        );
    }

    #[test]
    fn queried_new_protective_order_confirms_sync_outcome() {
        let order = Order {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT").with_settlement("USDT"),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("2000000953242572".to_string()),
            client_order_id: Some("rq-sl-183".to_string()),
            side: Some("SELL".to_string()),
            order_type: Some("STOP_MARKET".to_string()),
            price: Some("2145.22".to_string()),
            size: Some("0.000".to_string()),
            filled_size: None,
            average_price: None,
            status: Some("NEW".to_string()),
            created_at: Some(1779023785699),
            updated_at: Some(1779023785699),
            raw: json!({"algoStatus":"NEW"}),
        };

        let outcome = protective_order_query_to_sync_outcome(Ok(order));

        assert_eq!(
            outcome,
            ProtectionSyncOutcome::confirmed("2000000953242572", "query_protective_order")
        );
    }

    #[test]
    fn queried_expired_protective_order_fails_sync_outcome() {
        let order = Order {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT").with_settlement("USDT"),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("2000000953242572".to_string()),
            client_order_id: Some("rq-sl-183".to_string()),
            side: Some("SELL".to_string()),
            order_type: Some("STOP_MARKET".to_string()),
            price: Some("2145.22".to_string()),
            size: Some("0.000".to_string()),
            filled_size: None,
            average_price: None,
            status: Some("EXPIRED".to_string()),
            created_at: Some(1779023785699),
            updated_at: Some(1779023895192),
            raw: json!({"algoStatus":"EXPIRED"}),
        };

        let outcome = protective_order_query_to_sync_outcome(Ok(order));

        assert_eq!(
            outcome,
            ProtectionSyncOutcome::failed(
                "query_protective_order",
                "protective order is not active: status=EXPIRED"
            )
        );
    }

    #[test]
    fn protective_order_query_candidates_prefer_client_algo_id_then_algo_id() {
        let instrument = Instrument::perp("ETH", "USDT").with_settlement("USDT");
        let ack = OrderAck {
            exchange: ExchangeId::Binance,
            exchange_symbol: "ETHUSDT".to_string(),
            instrument: instrument.clone(),
            order_id: Some("2000000953310341".to_string()),
            client_order_id: Some("rq-sl-185".to_string()),
            status: Some("NEW".to_string()),
            raw: json!({"algoId":2000000953310341_i64, "clientAlgoId":"rq-sl-185", "algoStatus":"NEW"}),
        };

        let candidates =
            protective_order_query_candidates_from_ack(&instrument, &ack, Some("rq-sl-185".into()))
                .expect("protective query candidates");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].client_order_id.as_deref(), Some("rq-sl-185"));
        assert_eq!(candidates[0].order_id, None);
        assert_eq!(candidates[1].order_id.as_deref(), Some("2000000953310341"));
        assert_eq!(candidates[1].client_order_id, None);
    }

    #[test]
    fn protective_order_rejection_maps_to_failed_sync_outcome() {
        let error = crypto_exc_all::Error::Api {
            exchange: ExchangeId::Binance,
            status: Some(400),
            code: "-2021".to_string(),
            message: "Order would immediately trigger.".to_string(),
        };

        let outcome = protective_order_result_to_sync_outcome(Err(error));

        assert_eq!(
            outcome,
            ProtectionSyncOutcome::failed(
                "place_protective_order",
                "交易所 API 错误: binance status=Some(400) code=-2021: Order would immediately trigger."
            )
        );
    }

    #[test]
    fn post_close_cancel_missing_binance_protective_order_is_idempotent_absent() {
        let mut report = ExecutionTaskReportRequest {
            task_id: 42,
            execution_status: "completed".to_string(),
            exchange: "binance".to_string(),
            external_order_id: "close-42".to_string(),
            order_side: "sell".to_string(),
            order_status: "FILLED".to_string(),
            filled_qty: Some(0.024),
            filled_quote: Some(52.26),
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: None,
            raw_payload_json: Some(json!({"execution_status":"completed"}).to_string()),
        };
        let error = crypto_exc_all::Error::Api {
            exchange: ExchangeId::Binance,
            status: Some(400),
            code: "-2011".to_string(),
            message: "Unknown order sent.".to_string(),
        };

        apply_post_close_protection_cancel_result(&mut report, Err(error));
        let raw_payload: Value =
            serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "completed");
        assert_eq!(report.error_message, None);
        assert_eq!(
            raw_payload["post_close_protection_cancel"]["status"],
            "already_absent"
        );
        assert_eq!(
            raw_payload["post_close_protection_cancel"]["protective_order_absent"],
            true
        );
        assert_eq!(raw_payload["execution_status"], "completed");
    }

    #[test]
    fn pending_confirmation_task_builds_query_from_existing_order_result() {
        let task = task_with_metadata(
            "execute_signal",
            "confirming",
            json!({
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "size": "0.009"
            }),
        );
        let pending = PendingConfirmationTask::from_task_and_order_result(
            &task,
            "binance",
            "123456789",
            "buy",
            "NEW",
        )
        .unwrap();
        let query = pending.to_order_query().unwrap();

        assert_eq!(pending.exchange.as_str(), "binance");
        assert_eq!(pending.order_side, "buy");
        assert_eq!(query.order_id.as_deref(), Some("123456789"));
        assert_eq!(query.client_order_id, None);
    }

    #[test]
    fn pending_confirmation_task_uses_stable_client_order_id_when_order_id_is_missing() {
        let task = task_with_metadata(
            "execute_signal",
            "confirming",
            json!({
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "size": "0.009"
            }),
        );
        let pending = PendingConfirmationTask::from_task_and_order_result(
            &task,
            "binance",
            "",
            "buy",
            "submitted",
        )
        .unwrap();
        let query = pending.to_order_query().unwrap();

        assert_eq!(query.order_id, None);
        assert_eq!(query.client_order_id.as_deref(), Some("rqtask42"));
    }

    #[test]
    fn derives_market_order_size_from_size_usdt_and_last_price() {
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "TEST-USDT-SWAP",
            "signal_type": "buy",
            "execution": {
                "exchange": "binance",
                "symbol": "TEST-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 25.0
            }
        }));

        let request = ExecutionOrderTask::from_task(&task).unwrap();
        assert_eq!(request.size, "0");

        let order = request.to_order_request_with_last_price(Some(2.5)).unwrap();

        assert_eq!(order.exchange.as_str(), "binance");
        assert_eq!(order.size, "10");
    }

    #[test]
    fn strategy_size_usdt_payload_waits_for_live_ticker_and_filters() {
        let task = task(json!({
            "source": "rust_quant",
            "symbol": "ETH-USDT-SWAP",
            "payload_json": serde_json::json!({
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 60.0,
                "signal": {
                    "open_price": 2300.38
                }
            }).to_string()
        }));

        let request = ExecutionOrderTask::from_task(&task).unwrap();

        assert_eq!(request.size, "0");
        assert_eq!(request.size_usdt, Some(60.0));
    }

    #[test]
    fn live_order_size_is_quantized_to_exchange_step_size() {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 60.0,
            "client_order_id": "rqtest"
        }));
        let filters = binance_eth_filters();

        let request = ExecutionOrderTask::from_task(&task).unwrap();
        let order = request
            .to_live_order_request(Some(2300.38), Some(&filters))
            .unwrap();

        assert_eq!(order.size, "0.026");
    }

    #[test]
    fn live_order_size_rejects_notional_below_exchange_minimum() {
        let filters = binance_eth_filters();

        let error = quantize_order_size(
            "0.0086".parse().unwrap(),
            "2300.38".parse().unwrap(),
            &filters,
            true,
        )
        .unwrap_err();

        assert!(error.to_string().contains("min_notional"));
    }

    #[test]
    fn live_order_confirmation_requires_exact_opt_in_token() {
        assert!(live_order_confirmation_valid(
            false,
            Some("I_UNDERSTAND_LIVE_ORDERS")
        ));
        assert!(live_order_confirmation_valid(true, None));
        assert!(!live_order_confirmation_valid(false, None));
        assert!(!live_order_confirmation_valid(false, Some("true")));
        assert!(!live_order_confirmation_valid(false, Some("I_UNDERSTAND")));
    }

    #[test]
    fn target_task_allowlist_rejects_unlisted_leased_task_ids() {
        let config = ExecutionWorkerConfig {
            worker_id: "worker-targeted".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["risk_control_close_candidate".to_string()],
            task_statuses: vec!["pending_close".to_string()],
            target_task_ids: vec![1001],
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        };

        assert!(config.leased_task_allowed(1001));
        assert!(!config.leased_task_allowed(1002));
    }

    #[test]
    fn dry_run_result_is_reportable_without_exchange_credentials() {
        let task = task(json!({
            "exchange": "okx",
            "symbol": "BTC-USDT-SWAP",
            "signal_type": "long"
        }));

        let request = ExecutionOrderTask::from_task(&task).unwrap();
        let result = request.dry_run_report().unwrap();

        assert_eq!(result.task_id, 42);
        assert_eq!(result.execution_status, "completed");
        assert_eq!(result.exchange, "okx");
        assert_eq!(result.order_side, "buy");
        assert_eq!(result.order_status, "dry_run");
        assert_eq!(
            result.raw_payload_json.as_deref(),
            Some("{\"dry_run\":true,\"symbol\":\"BTC-USDT-SWAP\"}")
        );
    }

    #[tokio::test]
    async fn pending_close_task_dry_run_reports_close_candidate_result() {
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-close".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec![
                    "execute_signal".to_string(),
                    "risk_control_close_candidate".to_string(),
                ],
                task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        );
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "BTC-USDT-SWAP",
                "manual_review": {
                    "task_type": "risk_control_close_candidate",
                    "action": "close_candidate",
                    "category": "exchange_delisting"
                },
                "risk_control": {
                    "action": "close_candidate",
                    "category": "exchange_delisting",
                    "auto_execution_allowed": false
                }
            }),
        );

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "completed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "close");
        assert_eq!(report.order_status, "dry_run");
        assert_eq!(raw_payload["task_type"], "risk_control_close_candidate");
        assert_eq!(raw_payload["task_status"], "pending_close");
        assert_eq!(raw_payload["risk_control_action"], "close_candidate");
        assert_eq!(raw_payload["symbol"], "BTC-USDT-SWAP");
    }

    #[tokio::test]
    async fn dry_run_execute_signal_with_required_stop_loss_stays_pending_protection_sync() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-dry-run-protection".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 3400.0,
                "entry_price": 3500.0,
                "direction": "long"
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "pending_protection_sync");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "buy");
        assert_eq!(report.order_status, "dry_run");
        assert_eq!(
            raw_payload["protection_sync"]["status"],
            "pending_protection_sync"
        );
        assert_eq!(
            raw_payload["protection_sync"]["protective_order_confirmed"],
            false
        );
        assert_eq!(
            raw_payload["protection_sync"]["selected_stop_loss_price"],
            3400.0
        );
        assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
        assert_eq!(repository.audits.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn execute_signal_with_required_live_stop_loss_missing_selected_price_fails_before_order()
    {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-risk-contract".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "live_order": true,
                "protective_stop_loss_required": true,
                "direction": "long",
                "max_loss_percent": 0.02
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "buy");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("risk_plan.selected_stop_loss_price"));
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["risk_contract"]["missing_field"],
            "risk_plan.selected_stop_loss_price"
        );
        assert!(repository.audits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn live_config_missing_stop_loss_short_circuits_before_gateway_audit() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-live-config-no-live".to_string(),
                lease_limit: 1,
                dry_run: false,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "live_order": true,
                "protective_stop_loss_required": true,
                "direction": "long",
                "max_loss_percent": 0.02
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(raw_payload["risk_contract"]["worker_dry_run"], false);
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["risk_contract"]["missing_field"],
            "risk_plan.selected_stop_loss_price"
        );
        assert!(repository.audits.lock().unwrap().is_empty());
        assert!(repository.checkpoints.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn news_event_missing_stop_loss_price_fails_before_order() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-news-risk-contract".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source_signal_type": "news_event",
            "source": "rust_quant_news",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "direction": "long",
                "entry_price": 3500.0
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "buy");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("risk_plan.selected_stop_loss_price"));
        assert_eq!(
            raw_payload["risk_contract"]["source_signal_type"],
            "news_event"
        );
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["risk_contract"]["missing_field"],
            "risk_plan.selected_stop_loss_price"
        );
        assert!(repository.audits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn news_signal_id_missing_stop_loss_price_fails_before_order() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-news-risk-contract".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let mut task = task(json!({
            "source": "rust_quant_news",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "direction": "long",
                "entry_price": 3500.0
            }
        }));
        task.news_signal_id = Some(77);

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("risk_plan.selected_stop_loss_price"));
        assert_eq!(raw_payload["risk_contract"]["news_signal_id"], 77);
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert!(repository.audits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_signal_with_required_live_stop_loss_invalid_direction_fails_before_order() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-risk-contract".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "live_order": true,
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 3400.0,
                "direction": "sideways"
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "buy");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("unsupported protective stop-loss direction"));
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["risk_contract"]["invalid_direction"],
            "sideways"
        );
        assert!(repository.audits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_signal_with_long_stop_loss_above_entry_fails_before_order() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-risk-contract".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "buy",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 3600.0,
                "entry_price": 3500.0,
                "direction": "long"
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("invalid protective stop-loss price"));
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert_eq!(raw_payload["risk_contract"]["entry_price"], 3500.0);
        assert_eq!(
            raw_payload["risk_contract"]["selected_stop_loss_price"],
            3600.0
        );
        assert!(repository.audits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_signal_with_short_stop_loss_below_entry_fails_before_order() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-risk-contract".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "source": "rust_quan_web",
            "symbol": "ETH-USDT-SWAP",
            "execution": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "side": "sell",
                "order_type": "market",
                "size_usdt": 35.0
            },
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 3400.0,
                "entry_price": 3500.0,
                "direction": "short"
            }
        }));

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("invalid protective stop-loss price"));
        assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
        assert_eq!(raw_payload["risk_contract"]["entry_price"], 3500.0);
        assert_eq!(
            raw_payload["risk_contract"]["selected_stop_loss_price"],
            3400.0
        );
        assert!(repository.audits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn pending_close_live_mode_without_close_order_contract_reports_failed() {
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-live-close".to_string(),
                lease_limit: 1,
                dry_run: false,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["risk_control_close_candidate".to_string()],
                task_statuses: vec!["pending_close".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        );
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "manual_review": {
                    "task_type": "risk_control_close_candidate",
                    "action": "close_candidate"
                },
                "risk_control": {
                    "action": "close_candidate",
                    "auto_execution_allowed": false
                }
            }),
        );

        let report = worker.execute_task(&task).await;
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
                .expect("raw payload json");

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "close");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("requires Web close_order payload"));
        assert_eq!(raw_payload["task_status"], "pending_close");
        assert_eq!(raw_payload["close_order"], Value::Null);
    }

    #[tokio::test]
    async fn pending_close_live_mode_invalid_position_side_reports_failed() {
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-live-close".to_string(),
                lease_limit: 1,
                dry_run: false,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["risk_control_close_candidate".to_string()],
                task_statuses: vec!["pending_close".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        );
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "close_order_status": "ready",
                "close_order": {
                    "exchange": "binance",
                    "symbol": "ETH-USDT-SWAP",
                    "position_side": "net",
                    "size": 0.42,
                    "order_type": "market",
                    "reduce_only": true
                }
            }),
        );

        let report = worker.execute_task(&task).await;

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "close");
        assert!(report
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("unsupported close_order.position_side"));
    }

    #[tokio::test]
    async fn leased_risk_close_candidate_still_uses_pending_close_order_path() {
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-close".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["risk_control_close_candidate".to_string()],
                task_statuses: vec!["pending_close".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        );
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "leased",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "close_order_status": "ready",
                "close_order": {
                    "exchange": "binance",
                    "symbol": "ETH-USDT-SWAP",
                    "position_side": "long",
                    "side": "sell",
                    "size": 0.42,
                    "order_type": "market",
                    "reduce_only": true
                },
                "signal_type": "hold"
            }),
        );

        let report = worker.execute_task(&task).await;

        assert_eq!(report.execution_status, "completed");
        assert_eq!(report.exchange, "binance");
        assert_eq!(report.order_side, "sell");
        assert_ne!(report.order_status, "failed");
    }

    #[test]
    fn pending_close_task_maps_web_close_order_to_reduce_only_order() {
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "close_order_status": "ready",
                "close_order": {
                    "exchange": "binance",
                    "symbol": "ETH-USDT-SWAP",
                    "position_side": "long",
                    "side": "sell",
                    "size": 0.42,
                    "order_type": "market",
                    "reduce_only": true
                }
            }),
        );

        let close_task = PendingCloseTask::from_task(&task, ExchangeId::Okx).unwrap();
        let order = close_task
            .to_order_request()
            .unwrap()
            .expect("close_order should map to an executable order");

        assert_eq!(order.exchange.as_str(), "binance");
        assert_eq!(order.instrument.symbol_for(order.exchange), "ETHUSDT");
        assert_eq!(order_side_lower(order.side), "sell");
        assert_eq!(order.size, "0.42");
        assert_eq!(order.position_side.as_deref(), Some("long"));
        assert_eq!(order.trade_side.as_deref(), Some("close"));
        assert_eq!(order.reduce_only, Some(true));
        assert_eq!(order.client_order_id.as_deref(), Some("rqclose42"));
    }

    #[test]
    fn pending_close_task_okx_close_order_does_not_set_reduce_only_by_default() {
        // OKX hedge mode uses position_side to specify close direction; reduce_only is only
        // applicable in net mode. Verify the default is None for OKX.
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "close_order_status": "ready",
                "close_order": {
                    "exchange": "okx",
                    "symbol": "ETH-USDT-SWAP",
                    "position_side": "long",
                    "side": "sell",
                    "size": "0.1",
                    "order_type": "market"
                }
            }),
        );

        let close_task = PendingCloseTask::from_task(&task, ExchangeId::Okx).unwrap();
        let order = close_task
            .to_order_request()
            .unwrap()
            .expect("close_order should map to an executable order");

        assert_eq!(order.exchange.as_str(), "okx");
        assert_eq!(order_side_lower(order.side), "sell");
        assert_eq!(order.position_side.as_deref(), Some("long"));
        // reduce_only must be None for OKX — hedge mode does not support it
        assert_eq!(order.reduce_only, None);
    }

    #[test]
    fn pending_close_task_binance_hedge_close_does_not_default_reduce_only() {
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "close_order_status": "ready",
                "close_order": {
                    "exchange": "binance",
                    "symbol": "ETH-USDT-SWAP",
                    "position_mode": "hedge",
                    "position_side": "long",
                    "side": "sell",
                    "size": "0.009",
                    "order_type": "market"
                }
            }),
        );

        let close_task = PendingCloseTask::from_task(&task, ExchangeId::Binance).unwrap();
        let order = close_task
            .to_order_request()
            .unwrap()
            .expect("close_order should map to an executable order");

        assert_eq!(order.exchange.as_str(), "binance");
        assert_eq!(order_side_lower(order.side), "sell");
        assert_eq!(order.position_side.as_deref(), Some("long"));
        assert_eq!(order.trade_side.as_deref(), Some("close"));
        assert_eq!(order.reduce_only, None);
    }

    #[test]
    fn pending_close_task_builds_protective_cancel_request_by_client_order_id() {
        let task = task_with_metadata(
            "risk_control_close_candidate",
            "pending_close",
            json!({
                "symbol": "ETH-USDT-SWAP",
                "close_order": {
                    "exchange": "binance",
                    "symbol": "ETH-USDT-SWAP",
                    "position_mode": "hedge",
                    "position_side": "long",
                    "side": "sell",
                    "size": "0.024",
                    "order_type": "market",
                    "margin_coin": "USDT",
                    "cancel_protective_client_order_id": "rq-sl-168"
                }
            }),
        );

        let close_task = PendingCloseTask::from_task(&task, ExchangeId::Binance).unwrap();
        let (exchange, cancel_request) = close_task
            .protective_cancel_request()
            .unwrap()
            .expect("close task should carry a protective cancel request");

        assert_eq!(exchange, ExchangeId::Binance);
        assert_eq!(cancel_request.instrument.symbol_for(exchange), "ETHUSDT");
        assert_eq!(cancel_request.client_order_id.as_deref(), Some("rq-sl-168"));
        assert_eq!(cancel_request.order_id, None);
        assert_eq!(cancel_request.margin_coin.as_deref(), Some("USDT"));
    }

    #[test]
    fn confirmed_live_order_report_uses_order_detail_and_fills() {
        let instrument = Instrument::perp("ETH", "USDT");
        let ack = OrderAck {
            exchange: ExchangeId::Binance,
            instrument: instrument.clone(),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("12345".to_string()),
            client_order_id: Some("rqethopen1".to_string()),
            status: Some("NEW".to_string()),
            raw: json!({"status":"NEW","orderId":12345}),
        };
        let order = Order {
            exchange: ExchangeId::Binance,
            instrument: instrument.clone(),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("12345".to_string()),
            client_order_id: Some("rqethopen1".to_string()),
            side: Some("BUY".to_string()),
            order_type: Some("MARKET".to_string()),
            price: Some("0".to_string()),
            size: Some("0.009".to_string()),
            filled_size: Some("0.009".to_string()),
            average_price: Some("2267.60000".to_string()),
            status: Some("FILLED".to_string()),
            created_at: Some(1),
            updated_at: Some(2),
            raw: json!({"status":"FILLED","executedQty":"0.009","avgPrice":"2267.60000"}),
        };
        let fill = Fill {
            exchange: ExchangeId::Binance,
            instrument,
            exchange_symbol: "ETHUSDT".to_string(),
            trade_id: Some("9001".to_string()),
            order_id: Some("12345".to_string()),
            side: Some("BUY".to_string()),
            price: Some("2267.60000".to_string()),
            size: Some("0.009".to_string()),
            fee: Some("0.01020420".to_string()),
            fee_asset: Some("USDT".to_string()),
            role: Some("taker".to_string()),
            timestamp: Some(3),
            raw: json!({"id":9001,"qty":"0.009","price":"2267.60000","commission":"0.01020420"}),
        };

        let report =
            build_confirmed_order_report(121, "buy", &ack, Some(order), vec![fill], None, None);

        assert_eq!(report.execution_status, "completed");
        assert_eq!(report.external_order_id, "12345");
        assert_eq!(report.order_status, "FILLED");
        assert_eq!(report.filled_qty, Some(0.009));
        assert_eq!(report.fee_amount, Some(0.01020420));
        let filled_quote = report.filled_quote.unwrap();
        assert!((filled_quote - 20.4084).abs() < 0.00000001);
        let raw = report.raw_payload_json.unwrap();
        assert!(raw.contains("order_detail"));
        assert!(raw.contains("fills"));
    }

    #[test]
    fn filled_live_open_with_required_stop_loss_stays_pending_protection_sync() {
        let instrument = Instrument::perp("ETH", "USDT");
        let ack = OrderAck {
            exchange: ExchangeId::Binance,
            instrument: instrument.clone(),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("12347".to_string()),
            client_order_id: Some("rqethopen3".to_string()),
            status: Some("FILLED".to_string()),
            raw: json!({"status":"FILLED","orderId":12347}),
        };
        let order = Order {
            exchange: ExchangeId::Binance,
            instrument,
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("12347".to_string()),
            client_order_id: Some("rqethopen3".to_string()),
            side: Some("BUY".to_string()),
            order_type: Some("MARKET".to_string()),
            price: Some("0".to_string()),
            size: Some("0.009".to_string()),
            filled_size: Some("0.009".to_string()),
            average_price: Some("2267.60000".to_string()),
            status: Some("FILLED".to_string()),
            created_at: Some(1),
            updated_at: Some(2),
            raw: json!({"status":"FILLED","executedQty":"0.009","avgPrice":"2267.60000"}),
        };
        let protection = ProtectionSyncContract::required(
            json!({
                "risk_plan": {
                    "protective_stop_loss_required": true,
                    "selected_stop_loss_price": 2200.0,
                    "direction": "long"
                }
            }),
            "buy",
        )
        .expect("valid protection contract");

        let report = build_confirmed_order_report(
            123,
            "buy",
            &ack,
            Some(order),
            vec![],
            None,
            Some(protection),
        );
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "pending_protection_sync");
        assert_eq!(report.order_status, "FILLED");
        assert_eq!(
            report.error_message.as_deref(),
            Some("protective stop-loss required but protection order sync is not confirmed")
        );
        assert_eq!(
            raw_payload["protection_sync"]["status"],
            "pending_protection_sync"
        );
        assert_eq!(
            raw_payload["protection_sync"]["protective_order_confirmed"],
            false
        );
        assert_eq!(
            raw_payload["protection_sync"]["selected_stop_loss_price"],
            2200.0
        );
        assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
    }

    #[test]
    fn protection_sync_confirmed_completes_task_without_allowing_repeat_open() {
        let protection = ProtectionSyncContract::required(
            json!({
                "risk_plan": {
                    "protective_stop_loss_required": true,
                    "selected_stop_loss_price": 2200.0,
                    "direction": "long"
                }
            }),
            "buy",
        )
        .expect("valid protection contract");
        let mut report = ExecutionTaskReportRequest::success(
            124,
            "binance",
            "12348",
            "buy",
            "FILLED",
            json!({"execution_status":"pending_protection_sync"}),
        );
        report.execution_status = "pending_protection_sync".to_string();
        report.error_message =
            Some("protective stop-loss required but protection order sync is not confirmed".into());

        protection.apply_outcome_to_report(
            &mut report,
            ProtectionSyncOutcome::confirmed("sl-rqethopen4", "query_order"),
        );
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "completed");
        assert_eq!(report.error_message, None);
        assert_eq!(raw_payload["protection_sync"]["status"], "completed");
        assert_eq!(
            raw_payload["protection_sync"]["protective_order_external_id"],
            "sl-rqethopen4"
        );
        assert_eq!(
            raw_payload["protection_sync"]["protective_order_confirmed"],
            true
        );
        assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["protection_sync"]["repeat_open_order_allowed"],
            false
        );
    }

    #[tokio::test]
    async fn completed_news_protection_report_posts_safe_task_context_to_web() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let mut task = task(json!({
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "source_signal_type": "news_event",
            "side": "buy",
            "size": "0.011",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2156.0,
                "entry_reference_price": 2200.0,
                "direction": "long"
            }
        }));
        task.id = 218;
        task.news_signal_id = Some(601);
        task.strategy_slug = "news_momentum".to_string();
        task.symbol = "ETH-USDT-SWAP".to_string();
        let instrument = Instrument::perp("ETH", "USDT");
        let ack = OrderAck {
            exchange: ExchangeId::Binance,
            instrument: instrument.clone(),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("8389766181876482454".to_string()),
            client_order_id: Some("rqtask218".to_string()),
            status: Some("FILLED".to_string()),
            raw: json!({"status":"FILLED","orderId":"8389766181876482454"}),
        };
        let order = Order {
            exchange: ExchangeId::Binance,
            instrument,
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("8389766181876482454".to_string()),
            client_order_id: Some("rqtask218".to_string()),
            side: Some("BUY".to_string()),
            order_type: Some("MARKET".to_string()),
            price: Some("0".to_string()),
            size: Some("0.011".to_string()),
            filled_size: Some("0.011".to_string()),
            average_price: Some("2200.00".to_string()),
            status: Some("FILLED".to_string()),
            created_at: Some(1),
            updated_at: Some(2),
            raw: json!({"status":"FILLED","executedQty":"0.011","avgPrice":"2200.00"}),
        };
        let protection = ProtectionSyncContract::required(task.request_payload_json.clone(), "buy")
            .expect("news task should carry a valid protection contract");
        let mut report = build_confirmed_order_report_for_task(
            &task,
            "buy",
            &ack,
            Some(order),
            vec![],
            None,
            Some(protection),
        );
        ProtectionSyncContract::required(task.request_payload_json.clone(), "buy")
            .unwrap()
            .apply_outcome_to_report(
                &mut report,
                ProtectionSyncOutcome::confirmed("2000000956163119", "query_order"),
            );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"task":{"id":218,"news_signal_id":601,"combo_id":9,"buyer_email":"buyer@example.com","strategy_slug":"news_momentum","symbol":"ETH-USDT-SWAP","task_type":"execute_signal","task_status":"completed","priority":3,"lease_owner":"worker","lease_until":null,"scheduled_at":"2026-04-23T12:00:00","request_payload_json":"{}","created_at":"2026-04-23T12:00:00","updated_at":"2026-04-23T12:00:00"},"attempt":{},"order_result":null,"trade_record":null}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let response = client.report_result(report).await.unwrap();

        server.await.unwrap();
        assert_eq!(response.task.id, 218);
        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /api/commerce/internal/execution-results HTTP/1.1"));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        let body = request
            .split("\r\n\r\n")
            .nth(1)
            .expect("mock Web request should include JSON body");
        let posted: Value = serde_json::from_str(body).unwrap();
        assert_eq!(posted["task_id"], 218);
        assert_eq!(posted["execution_status"], "completed");
        let raw_payload: Value =
            serde_json::from_str(posted["raw_payload_json"].as_str().unwrap()).unwrap();
        assert_eq!(raw_payload["execution_task"]["news_signal_id"], 601);
        assert_eq!(
            raw_payload["execution_task"]["source_signal_type"],
            "news_event"
        );
        assert_eq!(
            raw_payload["execution_task"]["strategy_slug"],
            "news_momentum"
        );
        assert_eq!(raw_payload["protection_sync"]["status"], "completed");
        assert_eq!(
            raw_payload["protection_sync"]["protective_order_external_id"],
            "2000000956163119"
        );
        assert_eq!(
            raw_payload["protection_sync"]["protective_order_confirmed"],
            true
        );
        assert!(raw_payload.get("api_secret").is_none());
        assert!(raw_payload.get("api_key").is_none());
    }

    #[test]
    fn protection_sync_failure_marks_protective_order_failed_without_allowing_repeat_open() {
        let protection = ProtectionSyncContract::required(
            json!({
                "risk_plan": {
                    "protective_stop_loss_required": true,
                    "selected_stop_loss_price": 2200.0,
                    "direction": "long"
                }
            }),
            "buy",
        )
        .expect("valid protection contract");
        let mut report = ExecutionTaskReportRequest::success(
            125,
            "binance",
            "12349",
            "buy",
            "FILLED",
            json!({"execution_status":"pending_protection_sync"}),
        );
        report.execution_status = "pending_protection_sync".to_string();

        protection.apply_outcome_to_report(
            &mut report,
            ProtectionSyncOutcome::failed("place_protective_order", "STOP_MARKET rejected"),
        );
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "protective_order_failed");
        assert_eq!(
            report.error_message.as_deref(),
            Some("STOP_MARKET rejected")
        );
        assert_eq!(
            raw_payload["protection_sync"]["status"],
            "protective_order_failed"
        );
        assert_eq!(
            raw_payload["protection_sync"]["reason"],
            "place_protective_order"
        );
        assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["protection_sync"]["repeat_open_order_allowed"],
            false
        );
    }

    #[test]
    fn protection_sync_uncertain_stays_pending_without_allowing_repeat_open() {
        let protection = ProtectionSyncContract::required(
            json!({
                "risk_plan": {
                    "protective_stop_loss_required": true,
                    "selected_stop_loss_price": 2200.0,
                    "direction": "long"
                }
            }),
            "buy",
        )
        .expect("valid protection contract");
        let mut report = ExecutionTaskReportRequest::success(
            126,
            "binance",
            "12350",
            "buy",
            "FILLED",
            json!({"execution_status":"pending_protection_sync"}),
        );
        report.execution_status = "pending_protection_sync".to_string();

        protection.apply_outcome_to_report(
            &mut report,
            ProtectionSyncOutcome::uncertain("query_protective_order", "read timeout"),
        );
        let raw_payload =
            serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "pending_protection_sync");
        assert_eq!(report.error_message.as_deref(), Some("read timeout"));
        assert_eq!(
            raw_payload["protection_sync"]["status"],
            "pending_protection_sync"
        );
        assert_eq!(
            raw_payload["protection_sync"]["reason"],
            "query_protective_order"
        );
        assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
        assert_eq!(
            raw_payload["protection_sync"]["repeat_open_order_allowed"],
            false
        );
    }

    #[test]
    fn confirmed_live_order_report_keeps_unfilled_order_pending_confirmation() {
        let instrument = Instrument::perp("ETH", "USDT");
        let ack = OrderAck {
            exchange: ExchangeId::Binance,
            instrument: instrument.clone(),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("12346".to_string()),
            client_order_id: Some("rqethopen2".to_string()),
            status: Some("NEW".to_string()),
            raw: json!({"status":"NEW","orderId":12346}),
        };
        let order = Order {
            exchange: ExchangeId::Binance,
            instrument,
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("12346".to_string()),
            client_order_id: Some("rqethopen2".to_string()),
            side: Some("BUY".to_string()),
            order_type: Some("MARKET".to_string()),
            price: Some("0".to_string()),
            size: Some("0.009".to_string()),
            filled_size: Some("0".to_string()),
            average_price: None,
            status: Some("NEW".to_string()),
            created_at: Some(1),
            updated_at: Some(2),
            raw: json!({"status":"NEW","executedQty":"0","avgPrice":"0"}),
        };

        let report =
            build_confirmed_order_report(122, "buy", &ack, Some(order), vec![], None, None);

        assert_eq!(report.execution_status, "pending_confirmation");
        assert_eq!(report.external_order_id, "12346");
        assert_eq!(report.order_status, "NEW");
        assert_eq!(report.filled_qty, Some(0.0));
        let raw = report.raw_payload_json.unwrap();
        assert!(raw.contains("order_detail"));
        assert!(raw.contains("pending_confirmation"));
    }

    #[test]
    fn duplicate_client_order_id_errors_are_reconciled_by_querying_existing_order() {
        assert!(is_duplicate_client_order_id_error(
            "binance error -4111: Duplicate clientOrderId"
        ));
        assert!(is_duplicate_client_order_id_error(
            "client order id is duplicate"
        ));
        assert!(is_duplicate_client_order_id_error(
            "clientOrderId has already been used"
        ));
        assert!(!is_duplicate_client_order_id_error(
            "insufficient margin balance"
        ));
    }

    #[test]
    fn duplicate_client_order_id_reconciliation_ack_keeps_original_client_order_id() {
        let request = OrderPlacementRequest {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT"),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            size: "0.009".to_string(),
            price: None,
            margin_mode: None,
            margin_coin: None,
            position_side: Some("long".to_string()),
            trade_side: Some("open".to_string()),
            client_order_id: Some("rqtask42".to_string()),
            reduce_only: None,
            time_in_force: None,
        };

        let ack = duplicate_client_order_id_reconciliation_ack(&request)
            .expect("stable client order id should be enough to reconcile");

        assert_eq!(ack.exchange, ExchangeId::Binance);
        assert_eq!(ack.order_id, None);
        assert_eq!(ack.client_order_id.as_deref(), Some("rqtask42"));
        assert_eq!(ack.status.as_deref(), Some("duplicate_client_order_id"));
        assert_eq!(
            ack.raw["reconciliation"]["action"],
            "query_existing_order_by_client_order_id"
        );
        assert_eq!(ack.raw["reconciliation"]["place_order_retried"], false);
    }

    #[test]
    fn pre_place_client_order_lookup_uses_stable_client_order_id_before_new_order() {
        let request = OrderPlacementRequest {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT"),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            size: "0.011".to_string(),
            price: None,
            margin_mode: None,
            margin_coin: Some("USDT".to_string()),
            position_side: Some("long".to_string()),
            trade_side: Some("open".to_string()),
            client_order_id: Some("rqtask218".to_string()),
            reduce_only: None,
            time_in_force: None,
        };

        let lookup = pre_place_client_order_lookup(&request)
            .expect("stable client order id should be queried before placing a retry order");

        assert_eq!(lookup.query.client_order_id.as_deref(), Some("rqtask218"));
        assert_eq!(lookup.query.margin_coin.as_deref(), Some("USDT"));
        assert_eq!(lookup.ack.client_order_id.as_deref(), Some("rqtask218"));
        assert_eq!(
            lookup.ack.raw["reconciliation"]["action"],
            "query_existing_order_before_place_order"
        );
        assert_eq!(
            lookup.ack.raw["reconciliation"]["place_order_allowed"],
            false
        );
        assert_eq!(
            lookup.ack.raw["reconciliation"]["place_order_retried"],
            false
        );
    }

    #[test]
    fn pre_place_client_order_check_only_allows_place_after_order_not_found() {
        assert!(is_order_not_found_for_client_order_preflight(
            "binance error -2013: Order does not exist."
        ));
        assert!(is_order_not_found_for_client_order_preflight(
            "order not found by clientOrderId"
        ));
        assert!(!is_order_not_found_for_client_order_preflight(
            "request timeout while querying order"
        ));
        assert!(!is_order_not_found_for_client_order_preflight(
            "insufficient permission for order query"
        ));
    }

    #[test]
    fn execute_signal_blocks_foreign_rqtask_client_order_id_before_live_mutation() {
        let request = OrderPlacementRequest {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT"),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            size: "0.011".to_string(),
            price: None,
            margin_mode: None,
            margin_coin: Some("USDT".to_string()),
            position_side: Some("long".to_string()),
            trade_side: Some("open".to_string()),
            client_order_id: Some("rqtask218".to_string()),
            reduce_only: None,
            time_in_force: None,
        };

        let report = client_order_id_owner_violation_report(999, "execute_signal", "buy", &request)
            .expect("foreign rqtask client id must fail closed before live mutation");
        let raw_payload: Value =
            serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();

        assert_eq!(report.execution_status, "failed");
        assert_eq!(report.external_order_id, "failed-task-999");
        assert_eq!(
            report.error_message.as_deref(),
            Some("client_order_id rqtask218 belongs to task 218, not task 999")
        );
        assert_eq!(raw_payload["stage"], "client_order_id_owner_check");
        assert_eq!(
            raw_payload["reconciliation"]["reason"],
            "client_order_id_owner_mismatch"
        );
        assert_eq!(raw_payload["place_order_allowed"], false);
        assert_eq!(raw_payload["mutation_allowed"], false);
        assert_eq!(raw_payload["protection_sync_allowed"], false);
    }

    #[tokio::test]
    async fn dry_run_worker_records_audit_and_checkpoint_through_repository() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-a".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Okx,
                task_types: vec![
                    "execute_signal".to_string(),
                    "risk_control_close_candidate".to_string(),
                ],
                task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let task = task(json!({
            "exchange": "okx",
            "symbol": "BTC-USDT-SWAP",
            "side": "buy",
            "size": "0.01",
            "api_key": "plain-api-key"
        }));
        let request = ExecutionOrderTask::from_task(&task)
            .unwrap()
            .to_order_request()
            .unwrap();

        worker
            .record_checkpoint(
                "leased",
                Some(task.id),
                json!({"api_secret": "plain-secret"}),
            )
            .await;
        let ack = worker
            .place_order_with_audit(&task, &worker.gateway, request)
            .await
            .unwrap();

        assert_eq!(ack.status.as_deref(), Some("dry_run"));
        let checkpoints = repository.checkpoints.lock().unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].worker_status, "leased");
        assert_eq!(
            checkpoints[0].checkpoint_value["api_secret"],
            "***REDACTED***"
        );
        drop(checkpoints);

        let audits = repository.audits.lock().unwrap();
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].request_status, "completed");
        assert_eq!(
            audits[0].request_payload["task"]["request_payload_json"]["api_key"],
            "***REDACTED***"
        );
        assert!(!audits[0]
            .request_payload
            .to_string()
            .contains("plain-api-key"));
    }

    #[tokio::test]
    async fn report_result_failure_records_replay_evidence_without_retrying_order() {
        let repository = Arc::new(CapturingAuditRepository::default());
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-report-replay".to_string(),
                lease_limit: 1,
                dry_run: false,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending_confirmation".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: true,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());
        let report = ExecutionTaskReportRequest {
            task_id: 42,
            execution_status: "pending_confirmation".to_string(),
            exchange: "binance".to_string(),
            external_order_id: "12345".to_string(),
            order_side: "buy".to_string(),
            order_status: "NEW".to_string(),
            filled_qty: Some(0.0),
            filled_quote: Some(0.0),
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: Some("waiting for exchange fill".to_string()),
            raw_payload_json: Some(
                r#"{"client_order_id":"rqtask42","api_secret":"plain-report-secret"}"#.to_string(),
            ),
        };

        worker
            .record_report_result_failure(42, &report, "web api_secret outage", "report_result")
            .await;

        let audits = repository.audits.lock().unwrap();
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].endpoint, "web.report_result");
        assert_eq!(audits[0].request_status, "failed");
        assert_eq!(
            audits[0].request_payload["report"]["external_order_id"],
            "12345"
        );
        assert_eq!(
            audits[0].request_payload["report"]["raw_payload_json"]["api_secret"],
            "***REDACTED***"
        );
        assert_eq!(
            audits[0].response_payload["replay_action"],
            "retry_report_result_only"
        );
        assert_eq!(audits[0].response_payload["place_order_allowed"], false);
        assert_eq!(audits[0].error_message, "redacted sensitive error");
        assert!(!audits[0]
            .request_payload
            .to_string()
            .contains("plain-report-secret"));
        drop(audits);

        let checkpoints = repository.checkpoints.lock().unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].worker_status, "report_failed");
        assert_eq!(
            checkpoints[0].checkpoint_value["replay"]["action"],
            "retry_report_result_only"
        );
        assert_eq!(
            checkpoints[0].checkpoint_value["replay"]["place_order_allowed"],
            false
        );
    }

    #[tokio::test]
    async fn report_replay_mode_reposts_stored_report_without_order_placement() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"task":{"id":42,"news_signal_id":null,"strategy_signal_id":null,"combo_id":9,"buyer_email":"buyer@example.com","strategy_slug":"news_momentum","symbol":"ETH-USDT-SWAP","task_type":"execute_signal","task_status":"pending_confirmation","priority":1,"lease_owner":null,"lease_until":null,"scheduled_at":"2026-04-23T12:00:00","request_payload_json":"{}","created_at":"2026-04-23T12:00:00","updated_at":"2026-04-23T12:00:00"},"attempt":{},"order_result":{},"trade_record":null}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let repository = Arc::new(CapturingAuditRepository::default());
        repository
            .report_replay_candidates
            .lock()
            .unwrap()
            .push(ReportResultReplayCandidate {
                request_id: "report-task-42-12345".to_string(),
                report: ExecutionTaskReportRequest {
                    task_id: 42,
                    execution_status: "pending_confirmation".to_string(),
                    exchange: "binance".to_string(),
                    external_order_id: "12345".to_string(),
                    order_side: "buy".to_string(),
                    order_status: "NEW".to_string(),
                    filled_qty: Some(0.0),
                    filled_quote: Some(0.0),
                    fee_amount: None,
                    profit_usdt: None,
                    executed_at: None,
                    error_message: Some("waiting for fill".to_string()),
                    raw_payload_json: Some(
                        r#"{"replay":{"place_order_allowed":false}}"#.to_string(),
                    ),
                },
            });
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: format!("http://{}", addr),
                internal_secret: "local-dev-secret".to_string(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-report-replay".to_string(),
                lease_limit: 1,
                dry_run: false,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: true,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());

        let handled = worker.run_once().await.unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /api/commerce/internal/execution-results HTTP/1.1"));
        assert!(request.contains("x-alpha-execution-secret: local-dev-secret"));
        assert!(request.contains(r#""task_id":42"#));
        assert!(request.contains(r#""external_order_id":"12345""#));
        assert!(!request.contains("/api/commerce/internal/execution-tasks/lease"));
        assert_eq!(handled, 1);

        let audits = repository.audits.lock().unwrap();
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].endpoint, "web.report_result");
        assert_eq!(audits[0].request_status, "replayed");
        assert_eq!(audits[0].response_payload["replay_status"], "completed");
        assert_eq!(audits[0].response_payload["place_order_allowed"], false);
        drop(audits);

        let checkpoints = repository.checkpoints.lock().unwrap();
        assert!(checkpoints
            .iter()
            .any(|checkpoint| checkpoint.worker_status == "report_replayed"));
        assert!(checkpoints
            .iter()
            .all(|checkpoint| checkpoint.checkpoint_value["place_order_allowed"] != true));
    }

    #[tokio::test]
    async fn report_replay_mode_writes_batch_summary_and_playbook_handoff() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let server = tokio::task::spawn_blocking(move || {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0_u8; 8192];
                let bytes = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
                tx.send(request).unwrap();

                if index == 0 {
                    let body = r#"{"success":true,"data":{"task":{"id":42,"news_signal_id":null,"strategy_signal_id":null,"combo_id":9,"buyer_email":"buyer@example.com","strategy_slug":"news_momentum","symbol":"ETH-USDT-SWAP","task_type":"execute_signal","task_status":"pending_confirmation","priority":1,"lease_owner":null,"lease_until":null,"scheduled_at":"2026-04-23T12:00:00","request_payload_json":"{}","created_at":"2026-04-23T12:00:00","updated_at":"2026-04-23T12:00:00"},"attempt":{},"order_result":{},"trade_record":null}}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                } else {
                    let body = r#"{"success":false,"error":"web unavailable"}"#;
                    let response = format!(
                        "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                }
            }
        });
        let repository = Arc::new(CapturingAuditRepository::default());
        repository.report_replay_candidates.lock().unwrap().extend([
            ReportResultReplayCandidate {
                request_id: "report-task-42-12345".to_string(),
                report: ExecutionTaskReportRequest {
                    task_id: 42,
                    execution_status: "pending_confirmation".to_string(),
                    exchange: "binance".to_string(),
                    external_order_id: "12345".to_string(),
                    order_side: "buy".to_string(),
                    order_status: "NEW".to_string(),
                    filled_qty: Some(0.0),
                    filled_quote: Some(0.0),
                    fee_amount: None,
                    profit_usdt: None,
                    executed_at: None,
                    error_message: Some("waiting for fill".to_string()),
                    raw_payload_json: None,
                },
            },
            ReportResultReplayCandidate {
                request_id: "report-task-43-67890".to_string(),
                report: ExecutionTaskReportRequest {
                    task_id: 43,
                    execution_status: "pending_confirmation".to_string(),
                    exchange: "binance".to_string(),
                    external_order_id: "67890".to_string(),
                    order_side: "buy".to_string(),
                    order_status: "NEW".to_string(),
                    filled_qty: Some(0.0),
                    filled_quote: Some(0.0),
                    fee_amount: None,
                    profit_usdt: None,
                    executed_at: None,
                    error_message: Some("waiting for fill".to_string()),
                    raw_payload_json: None,
                },
            },
        ]);
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: format!("http://{}", addr),
                internal_secret: "local-dev-secret".to_string(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-report-replay".to_string(),
                lease_limit: 10,
                dry_run: false,
                default_exchange: ExchangeId::Binance,
                task_types: vec!["execute_signal".to_string()],
                task_statuses: vec!["pending".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: true,
                report_replay_max_per_run: 2,
                report_replay_failure_backoff_seconds: 900,
                report_replay_throttle_ms: 0,
            },
        )
        .with_audit_repository(repository.clone());

        let handled = worker.run_once().await.unwrap();

        server.await.unwrap();
        let requests = [rx.recv().unwrap(), rx.recv().unwrap()];
        assert!(requests
            .iter()
            .all(|request| request.starts_with("POST /api/commerce/internal/execution-results")));
        assert_eq!(handled, 2);
        assert_eq!(
            repository.report_replay_queries.lock().unwrap().as_slice(),
            &[(2, 900)]
        );

        let checkpoints = repository.checkpoints.lock().unwrap();
        let final_checkpoint = checkpoints.last().unwrap();
        assert_eq!(final_checkpoint.worker_status, "idle");
        assert_eq!(
            final_checkpoint.checkpoint_value["report_replay"]["leased_count"],
            2
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["report_replay"]["attempted_count"],
            2
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["report_replay"]["replayed_count"],
            1
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["report_replay"]["failed_count"],
            1
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["report_replay"]["failure_backoff_seconds"],
            900
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["health_handoff"]["section"],
            "quant_worker_checkpoint_audit"
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["health_handoff"]["status"],
            "warn"
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["operator_playbook_summary"]["item_count"],
            1
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["operator_playbook_summary"]["items"][0]["code"],
            "QUANT_REPORT_REPLAY_FAILED"
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["operator_playbook_summary"]["items"][0]
                ["admin_link_target"],
            "admin.full_product_health.quant_worker_checkpoint_audit"
        );
        assert_eq!(
            final_checkpoint.checkpoint_value["place_order_allowed"],
            false
        );
    }

    #[tokio::test]
    async fn default_noop_audit_repository_does_not_block_worker_audit_paths() {
        let worker = ExecutionWorker::new(
            ExecutionTaskClient::new(ExecutionTaskConfig {
                base_url: "http://127.0.0.1".to_string(),
                internal_secret: String::new(),
            })
            .unwrap(),
            CryptoExcAllGateway::dry_run(),
            ExecutionWorkerConfig {
                worker_id: "worker-noop".to_string(),
                lease_limit: 1,
                dry_run: true,
                default_exchange: ExchangeId::Okx,
                task_types: vec![
                    "execute_signal".to_string(),
                    "risk_control_close_candidate".to_string(),
                ],
                task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
                target_task_ids: Vec::new(),
                confirmation_mode: false,
                report_replay_mode: false,
                report_replay_max_per_run: 1,
                report_replay_failure_backoff_seconds: 300,
                report_replay_throttle_ms: 0,
            },
        );
        let task = task(json!({
            "exchange": "okx",
            "symbol": "BTC-USDT-SWAP",
            "side": "buy",
            "size": "0.01",
            "api_secret": "plain-api-secret"
        }));
        let request = ExecutionOrderTask::from_task(&task)
            .unwrap()
            .to_order_request()
            .unwrap();

        worker
            .record_checkpoint(
                "leased",
                Some(task.id),
                json!({"access_token": "plain-access-token"}),
            )
            .await;
        let ack = worker
            .place_order_with_audit(&task, &worker.gateway, request)
            .await
            .unwrap();

        assert_eq!(ack.exchange.as_str(), "okx");
        assert_eq!(ack.status.as_deref(), Some("dry_run"));
    }
}
