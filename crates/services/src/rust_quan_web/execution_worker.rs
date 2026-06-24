use crate::exchange::{CryptoExcAllGateway, OrderPlacementRequest};
use crate::rust_quan_web::execution_order_filters::{
    decimal_from_f64, format_order_price_decimal, format_order_size_decimal,
    format_protective_stop_price_decimal, load_exchange_order_filters, minimum_order_notional_usdt,
    minimum_order_size, order_notional_usdt, parse_positive_decimal, quantize_limit_order_price,
    quantize_order_size, quantize_protective_stop_price, ExchangeOrderFilters,
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
    ProtectiveDirection, ProtectiveOrderMutator,
};
use crate::rust_quan_web::execution_rollback::{
    apply_protective_failure_rollback_error, apply_protective_failure_rollback_report,
    build_protective_failure_rollback_order_request,
};
use crate::rust_quan_web::execution_take_profit::{
    apply_take_profit_stop_reset_outcome_to_report, apply_take_profit_sync_outcome_to_report,
    carry_take_profit_tracking_from_previous_report, parse_take_profit_legs,
    sync_take_profit_orders_after_main_fill, sync_take_profit_stop_reset_after_fills,
    take_profit_stop_reset_monitor_required, take_profit_sync_retry_required, TakeProfitLeg,
    TakeProfitOrderPlacer, TakeProfitStopResetOutcome,
};
#[cfg(test)]
use crate::rust_quan_web::execution_take_profit::{
    build_take_profit_order_requests, build_take_profit_stop_reset_plan,
    build_take_profit_stop_reset_plan_with_tracking, existing_take_profit_order_allows_replacement,
    existing_take_profit_order_record, existing_take_profit_order_status_error,
    take_profit_order_ack_record, take_profit_order_ack_request_error,
    take_profit_order_ack_status_error, take_profit_stop_reset_capability_error,
    TakeProfitSyncOutcome,
};
use crate::rust_quan_web::{
    worker_live_capability_for_exchange, ExchangeOrderResult, ExchangeReconciliationIssueType,
    ExchangeReconciliationReportRequest, ExchangeReconciliationReportResponse,
    ExchangeRequestAuditLog, ExchangeRequestControlGuard, ExecutionAuditRepository,
    ExecutionRiskReservationRequest, ExecutionRiskReservationResponse, ExecutionTask,
    ExecutionTaskClient, ExecutionTaskConfig, ExecutionTaskConfirmationLeaseItem,
    ExecutionTaskLeaseExtendRequest, ExecutionTaskLeaseRequest, ExecutionTaskReportRequest,
    ExecutionWorkerCheckpoint, NoopExecutionAuditRepository, PostgresExecutionAuditRepository,
    ProtectionPlacementMode,
};
use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, Error as CryptoExchangeError, ExchangeId, Fill, FillListQuery, MarginMode,
    Order, OrderAck, OrderBook, OrderBookLevel, OrderBookQuery, OrderListQuery, OrderQuery,
    OrderSide, OrderType, Position, PositionMode, PrepareOrderSettingsRequest,
    PrepareOrderSettingsResult, ProtectiveOrderRequest, Ticker, TimeInForce,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Instant};
use tokio::time::{sleep, Duration};
use tracing::{error, warn};
const LIVE_TICKER_MAX_AGE_MS: u64 = 30_000;
const LIVE_LAST_PRICE_FALLBACK_BUFFER_RATIO: f64 = 0.001;
const LIVE_STOP_LOSS_MIN_DISTANCE_RATIO: f64 = 0.0005;
const LIVE_STOP_LOSS_MAX_DISTANCE_RATIO: f64 = 0.50;
const LIVE_ORDERBOOK_DEPTH_LIMIT: u32 = 5;
const LIVE_ORDERBOOK_MAX_SPREAD_RATIO: f64 = 0.005;
const LIVE_ORDERBOOK_MIN_DEPTH_NOTIONAL_MULTIPLIER: f64 = 1.20;
#[derive(Debug, Clone)]
pub struct ExecutionWorkerConfig {
    /// worker ID。
    pub worker_id: String,
    /// 租约limit，用于配置运行参数。
    pub lease_limit: u32,
    /// Dry-runrun，用于配置运行参数。
    pub dry_run: bool,
    /// default交易所，用于配置运行参数。
    pub default_exchange: ExchangeId,
    /// 列表数据。
    pub task_types: Vec<String>,
    /// 列表数据。
    pub task_statuses: Vec<String>,
    /// 列表数据。
    pub target_task_ids: Vec<i64>,
    /// confirmation模式，用于配置运行参数。
    pub confirmation_mode: bool,
    /// 报告replay模式，用于配置运行参数。
    pub report_replay_mode: bool,
    /// 报告replay最大perrun，用于配置运行参数。
    pub report_replay_max_per_run: u32,
    /// 秒级时长。
    pub report_replay_failure_backoff_seconds: u64,
    /// 毫秒级时间戳或时长。
    pub report_replay_throttle_ms: u64,
}
impl ExecutionWorkerConfig {
    /// 提供from环境变量的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn from_env() -> Self {
        let worker_id = std::env::var("EXECUTION_WORKER_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "rust_quant".to_string());
        let lease_limit = match std::env::var("EXECUTION_WORKER_LEASE_LIMIT") {
            Ok(value) => value
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|value| *value > 0)
                .unwrap_or(0),
            Err(_) => 10,
        };
        let dry_run = parse_env_bool("EXECUTION_WORKER_DRY_RUN", true);
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
        let confirmation_mode = parse_env_bool("EXECUTION_WORKER_CONFIRMATION_MODE", false);
        let report_replay_mode = parse_env_bool("EXECUTION_WORKER_REPORT_REPLAY_MODE", false);
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
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    pub(crate) fn validate_target_task_ids(&self) -> Result<()> {
        if self.target_task_ids.iter().all(|task_id| *task_id > 0) {
            return Ok(());
        }
        Err(anyhow!(
            "EXECUTION_WORKER_TARGET_TASK_IDS must contain only positive task ids"
        ))
    }
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    pub(crate) fn validate_lease_limit(&self) -> Result<()> {
        if self.lease_limit > 0 {
            return Ok(());
        }
        Err(anyhow!(
            "EXECUTION_WORKER_LEASE_LIMIT must be greater than zero"
        ))
    }
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    pub(crate) fn validate_live_worker_scope(&self) -> Result<()> {
        if self.dry_run || !self.target_task_ids.is_empty() {
            return Ok(());
        }
        Err(anyhow!(
            "refusing live execution worker without EXECUTION_WORKER_TARGET_TASK_IDS; live workers must be scoped to explicit reviewed task ids"
        ))
    }
    /// 提供报告replaylimit的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn report_replay_limit(&self) -> u32 {
        self.lease_limit
            .min(self.report_replay_max_per_run)
            .clamp(1, 100)
    }
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
pub(super) fn validate_worker_mode_bool_envs() -> Result<()> {
    for key in [
        "EXECUTION_WORKER_RECONCILIATION_ONLY",
        "EXECUTION_WORKER_REPORT_REPLAY_MODE",
        "EXECUTION_WORKER_CONFIRMATION_MODE",
    ] {
        validate_bool_env_value(key)?;
    }
    Ok(())
}
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
fn validate_bool_env_value(key: &str) -> Result<()> {
    match std::env::var(key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" | "0" | "false" | "no" | "n" | "off" => Ok(()),
            _ => Err(anyhow!(
                "{key} must be a boolean value: true/false/1/0/yes/no/on/off"
            )),
        },
        Err(_) => Ok(()),
    }
}
fn reconciliation_only_mode_from_env() -> bool {
    parse_env_bool("EXECUTION_WORKER_RECONCILIATION_ONLY", false)
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
fn parse_env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => true,
            "0" | "false" | "no" | "n" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}
/// 封装必需internalsecret来源环境变量，减少Web 商业链路调用方重复实现相同细节。
fn required_internal_secret_from_env() -> Result<String> {
    std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required")
        })
}
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
pub(crate) fn is_protected_link_symbol(symbol: &str) -> bool {
    let normalized: String = symbol
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    normalized == "LINKUSDT" || normalized.starts_with("LINKUSDT")
}
/// 提供APIcredentialIDfromtask的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn api_credential_id_from_task(task: &ExecutionTask) -> Option<i64> {
    let payload = order_payload(&task.request_payload_json);
    payload_string(&payload, "api_credential_id")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|id| *id > 0)
}
/// 提供APIcredential交易所matchestask的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 外部服务客户端。
    client: ExecutionTaskClient,
    /// gateway，用于记录交易或执行状态。
    gateway: CryptoExcAllGateway,
    /// 运行配置。
    config: ExecutionWorkerConfig,
    /// 审计repository，用于记录交易或执行状态。
    audit_repository: Arc<dyn ExecutionAuditRepository>,
}
impl ExecutionWorker {
    /// 构建 Web 商业、会员和执行准备度 所需实例，并集中初始化依赖和默认状态。
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
    /// 提供withauditrepository的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    fn validate_runtime_scope(&self) -> Result<()> {
        self.config.validate_lease_limit()?;
        self.config.validate_target_task_ids()?;
        if !self.config.dry_run {
            self.config.validate_live_worker_scope()?;
        }
        Ok(())
    }
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
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
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    pub async fn verify_live_audit_ready(&self) -> Result<()> {
        self.validate_runtime_scope()?;
        self.ensure_live_audit_repository()?;
        if self.live_order_mode_requires_audit() {
            self.audit_repository.verify_live_audit_ready().await?;
        }
        Ok(())
    }
    /// 封装实盘auditrepositorymissingreport，减少Web 商业链路调用方重复实现相同细节。
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
    /// 从外部输入转换为内部模型，隔离 Web 商业、会员和执行准备度 的字段适配细节。
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
            .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
            .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
        let internal_secret = required_internal_secret_from_env()?;
        validate_worker_mode_bool_envs()?;
        let config = ExecutionWorkerConfig::from_env();
        config.validate_lease_limit()?;
        config.validate_target_task_ids()?;
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url,
            internal_secret,
        })?;
        let reconciliation_only_mode = reconciliation_only_mode_from_env();
        if !config.dry_run {
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
