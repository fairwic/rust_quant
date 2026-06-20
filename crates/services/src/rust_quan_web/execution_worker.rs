use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, Error as CryptoExchangeError, ExchangeId, Fill, FillListQuery, MarginMode,
    Order, OrderAck, OrderListQuery, OrderQuery, OrderSide, OrderType, Position, PositionMode,
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
    worker_live_capability_for_exchange, ExchangeOrderResult, ExchangeReconciliationIssueType,
    ExchangeReconciliationReportRequest, ExchangeReconciliationReportResponse,
    ExchangeRequestAuditLog, ExecutionAuditRepository, ExecutionTask, ExecutionTaskClient,
    ExecutionTaskConfig, ExecutionTaskConfirmationLeaseItem, ExecutionTaskLeaseRequest,
    ExecutionTaskReportRequest, ExecutionWorkerCheckpoint, NoopExecutionAuditRepository,
    PostgresExecutionAuditRepository, ProtectionPlacementMode,
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

fn live_audit_write_error(error: anyhow::Error) -> CryptoExchangeError {
    CryptoExchangeError::Config(format!("live execution audit write failed: {error}"))
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

    fn live_order_mode_requires_audit(&self) -> bool {
        !self.config.dry_run && !reconciliation_only_mode_from_env()
    }

    fn ensure_live_audit_repository(&self) -> Result<()> {
        if self.live_order_mode_requires_audit()
            && !self.audit_repository.can_audit_live_mutations()
        {
            return Err(anyhow!(
                "QUANT_CORE_DATABASE_URL is required for live execution audit"
            ));
        }
        Ok(())
    }

    pub async fn verify_live_audit_ready(&self) -> Result<()> {
        self.ensure_live_audit_repository()?;
        if self.live_order_mode_requires_audit() {
            self.audit_repository.verify_live_audit_ready().await?;
        }
        Ok(())
    }

    fn live_audit_repository_missing_report(
        &self,
        task: &ExecutionTask,
        exchange: &str,
        order_side: &str,
        mut payload: Value,
    ) -> Option<ExecutionTaskReportRequest> {
        if self.ensure_live_audit_repository().is_ok() {
            return None;
        }
        if let Some(payload) = payload.as_object_mut() {
            payload.insert("stage".to_string(), json!("live_audit_repository"));
            payload.insert("place_order_allowed".to_string(), json!(false));
            payload.insert("mutation_allowed".to_string(), json!(false));
        } else {
            payload = json!({
                "task_id": task.id,
                "stage": "live_audit_repository",
                "place_order_allowed": false,
                "mutation_allowed": false,
            });
        }
        Some(ExecutionTaskReportRequest::failed(
            task.id,
            exchange,
            order_side,
            "QUANT_CORE_DATABASE_URL is required for live execution audit; place_order_allowed=false; mutation_allowed=false",
            payload,
        ))
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
        let audit_repository = PostgresExecutionAuditRepository::from_env()?;
        let live_order_mode = !config.dry_run && !reconciliation_only_mode;
        if live_order_mode && audit_repository.is_none() {
            return Err(anyhow!(
                "QUANT_CORE_DATABASE_URL is required for live execution audit"
            ));
        }
        let gateway = if config.dry_run || reconciliation_only_mode {
            CryptoExcAllGateway::dry_run()
        } else {
            ensure_live_order_confirmation()?;
            CryptoExcAllGateway::from_env()?
        };
        let mut worker = Self::new(client, gateway, config);
        if let Some(repository) = audit_repository {
            worker = worker.with_audit_repository(Arc::new(repository));
        }

        Ok(worker)
    }
}

include!("execution_worker_orchestration_section.rs");
include!("execution_worker_audit_section.rs");
include!("execution_worker_live_guard_section.rs");
include!("execution_worker_live_execution_section.rs");
include!("execution_worker_live_execution_support_section.rs");
include!("execution_worker_reconciliation_only_section.rs");
include!("execution_worker_reconciliation_section.rs");
include!("execution_worker_order_task_section.rs");
include!("execution_worker_confirmation_section.rs");

#[cfg(test)]
#[path = "execution_worker_env_tests.rs"]
mod execution_worker_env_tests;
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
