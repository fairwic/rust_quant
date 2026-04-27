use anyhow::{anyhow, Result};
use crypto_exc_all::{
    ExchangeId, Instrument, MarginMode, OrderAck, OrderSide, OrderType, TimeInForce,
};
use serde_json::{json, Value};
use std::{str::FromStr, sync::Arc, time::Instant};
use tracing::{error, warn};

use crate::exchange::{CryptoExcAllGateway, OrderPlacementRequest};
use crate::rust_quan_web::{
    ExchangeRequestAuditLog, ExecutionAuditRepository, ExecutionTask, ExecutionTaskClient,
    ExecutionTaskConfig, ExecutionTaskLeaseRequest, ExecutionTaskReportRequest,
    ExecutionWorkerCheckpoint, NoopExecutionAuditRepository, PostgresExecutionAuditRepository,
};

#[derive(Debug, Clone)]
pub struct ExecutionWorkerConfig {
    pub worker_id: String,
    pub lease_limit: u32,
    pub dry_run: bool,
    pub default_exchange: ExchangeId,
    pub task_types: Vec<String>,
    pub task_statuses: Vec<String>,
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

        Self {
            worker_id,
            lease_limit,
            dry_run,
            default_exchange,
            task_types,
            task_statuses,
        }
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
            CryptoExcAllGateway::from_env()?
        };
        let mut worker = Self::new(client, gateway, config);
        if let Some(repository) = PostgresExecutionAuditRepository::from_env()? {
            worker = worker.with_audit_repository(Arc::new(repository));
        }

        Ok(worker)
    }

    pub async fn run_once(&self) -> Result<usize> {
        self.record_checkpoint(
            "leasing",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
                "default_exchange": self.config.default_exchange.as_str(),
                "task_types": self.config.task_types.clone(),
                "task_statuses": self.config.task_statuses.clone(),
            }),
        )
        .await;

        let leased = match self
            .client
            .lease_tasks(ExecutionTaskLeaseRequest {
                worker_id: self.config.worker_id.clone(),
                limit: self.config.lease_limit,
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
            }),
        )
        .await;

        let mut handled = 0;
        let mut last_task_id = None;
        for task in leased.tasks {
            let report = self.execute_task(&task).await;
            let report_status = report.execution_status.clone();
            if let Err(error) = self.client.report_result(report).await {
                error!(task_id = task.id, "回写执行任务结果失败: {}", error);
                self.record_checkpoint(
                    "report_failed",
                    Some(task.id),
                    json!({
                        "stage": "report_result",
                        "error": error.to_string(),
                    }),
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

        if self.config.dry_run {
            return match order_task.to_order_request() {
                Ok(request) => match self
                    .place_order_with_audit(task, &self.gateway, request)
                    .await
                {
                    Ok(ack) => ExecutionTaskReportRequest::success(
                        task.id,
                        ack.exchange.as_str(),
                        ack.order_id
                            .as_deref()
                            .or(ack.client_order_id.as_deref())
                            .unwrap_or("dry_run"),
                        order_side_lower(order_task.side),
                        ack.status.as_deref().unwrap_or("dry_run"),
                        ack.raw,
                    ),
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

        match self.live_order_request(&gateway, &order_task).await {
            Ok(request) => match self.place_order_with_audit(task, &gateway, request).await {
                Ok(ack) => ExecutionTaskReportRequest::success(
                    task.id,
                    ack.exchange.as_str(),
                    ack.order_id
                        .as_deref()
                        .or(ack.client_order_id.as_deref())
                        .unwrap_or("unknown"),
                    order_side_lower(order_task.side),
                    ack.status.as_deref().unwrap_or("submitted"),
                    ack.raw,
                ),
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
            .place_order_with_audit(task, &gateway, request.clone())
            .await
        {
            Ok(ack) => ExecutionTaskReportRequest::success(
                task.id,
                ack.exchange.as_str(),
                ack.order_id
                    .as_deref()
                    .or(ack.client_order_id.as_deref())
                    .unwrap_or("unknown"),
                order_side_lower(request.side),
                ack.status.as_deref().unwrap_or("submitted"),
                ack.raw,
            ),
            Err(error) => ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side_lower(request.side),
                error.to_string(),
                close_task.report_payload(false),
            ),
        }
    }

    async fn live_order_request(
        &self,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<OrderPlacementRequest> {
        if !order_task.needs_market_price_size() {
            return order_task.to_order_request();
        }
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
        order_task.to_order_request_with_last_price(Some(last_price))
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
    pub margin_coin: Option<String>,
    pub position_side: Option<String>,
    pub trade_side: Option<String>,
    pub client_order_id: Option<String>,
    pub reduce_only: Option<bool>,
    pub time_in_force: Option<TimeInForce>,
    pub size_usdt: Option<f64>,
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
            position_side: payload_string(payload, "position_side"),
            trade_side: payload_string(payload, "trade_side").or_else(|| Some("close".to_string())),
            client_order_id: payload_string(payload, "client_order_id")
                .or_else(|| Some(format!("rq-close-task-{}", self.task_id))),
            reduce_only: payload_bool(payload, "reduce_only").or(Some(true)),
            time_in_force: payload_string(payload, "time_in_force")
                .map(|value| parse_time_in_force(&value))
                .transpose()?,
        }))
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
                .or_else(|| derive_size_from_notional(payload))
                .unwrap_or_else(|| "0".to_string()),
            price: payload_string(payload, "price"),
            margin_mode: payload_string(payload, "margin_mode").map(MarginMode::from),
            margin_coin: payload_string(payload, "margin_coin")
                .or_else(|| Some("USDT".to_string())),
            position_side: payload_string(payload, "position_side"),
            trade_side: payload_string(payload, "trade_side"),
            client_order_id: payload_string(payload, "client_order_id")
                .or_else(|| Some(format!("rq-task-{}", task.id))),
            reduce_only: payload_bool(payload, "reduce_only"),
            time_in_force: payload_string(payload, "time_in_force")
                .map(|value| parse_time_in_force(&value))
                .transpose()?,
        })
    }

    fn needs_market_price_size(&self) -> bool {
        is_zero_order_size(&self.size) && self.size_usdt.is_some()
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

fn derive_size_from_notional(payload: &Value) -> Option<String> {
    let size_usdt = payload_f64(payload, "size_usdt")?;
    let open_price = nested_payload_f64(payload, "signal", "open_price")
        .or_else(|| payload_f64(payload, "open_price"))
        .or_else(|| payload_f64(payload, "price"))?;
    if !size_usdt.is_finite() || !open_price.is_finite() || size_usdt <= 0.0 || open_price <= 0.0 {
        return None;
    }

    Some(format_order_size(size_usdt / open_price))
}

fn format_order_size(value: f64) -> String {
    let formatted = format!("{value:.8}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
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
    use crate::rust_quan_web::ExecutionTask;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn task(payload: serde_json::Value) -> ExecutionTask {
        task_with_metadata("execute_signal", "pending", payload)
    }

    fn task_with_metadata(
        task_type: &str,
        task_status: &str,
        payload: serde_json::Value,
    ) -> ExecutionTask {
        ExecutionTask {
            id: 42,
            news_signal_id: Some(7),
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
        assert_eq!(order.client_order_id.as_deref(), Some("rq-task-42"));
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
        assert_eq!(order.client_order_id.as_deref(), Some("rq-close-task-42"));
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
