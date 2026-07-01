use super::execution_audit::redact_error_message;
use super::execution_order_filters::{
    format_order_size_decimal, format_protective_stop_price_decimal, load_exchange_order_filters,
    quantize_order_size, quantize_protective_stop_price, ExchangeOrderFilters,
};
use super::execution_payload::{
    order_payload, parse_instrument, parse_protective_direction, protection_entry_price,
    protective_stop_loss_required, risk_plan_direction_raw, selected_stop_loss_price,
};
use super::execution_worker::ExecutionOrderTask;
use crate::exchange::CryptoExcAllGateway;
use crate::rust_quan_web::{ExecutionTask, ExecutionTaskReportRequest};
use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, Instrument, Order, OrderAck, OrderSide, ProtectiveOrderQuery,
    ProtectiveOrderRequest, ProtectiveOrderWorkingType,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::{future::Future, pin::Pin, str::FromStr};
use tokio::time::{sleep, Duration};
pub(super) trait ProtectiveOrderMutator {
    /// 提供auditplaceprotective的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn audit_place_protective<'a>(
        &'a self,
        task: &'a ExecutionTask,
        gateway: &'a CryptoExcAllGateway,
        exchange: ExchangeId,
        request: ProtectiveOrderRequest,
    ) -> Pin<Box<dyn Future<Output = crypto_exc_all::Result<OrderAck>> + Send + 'a>>;
    /// 提供auditcancelprotective的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn audit_cancel_protective<'a>(
        &'a self,
        task: &'a ExecutionTask,
        gateway: &'a CryptoExcAllGateway,
        exchange: ExchangeId,
        request: CancelOrderRequest,
    ) -> Pin<Box<dyn Future<Output = crypto_exc_all::Result<OrderAck>> + Send + 'a>>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProtectiveDirection {
    Long,
    Short,
}
impl ProtectiveDirection {
    /// 提供转换为字符串的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub(super) fn as_str(self) -> &'static str {
        match self {
            ProtectiveDirection::Long => "long",
            ProtectiveDirection::Short => "short",
        }
    }
    /// 提供protective订单side的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub(super) fn protective_order_side(self) -> OrderSide {
        match self {
            ProtectiveDirection::Long => OrderSide::Sell,
            ProtectiveDirection::Short => OrderSide::Buy,
        }
    }
}
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProtectionSyncOutcome {
    Confirmed {
        protective_order_external_id: String,
        source: String,
    },
    Failed {
        reason: String,
        error_message: String,
    },
    CancelFailed {
        reason: String,
        error_message: String,
        cancel_client_order_id: Option<String>,
    },
    Uncertain {
        reason: String,
        error_message: String,
    },
}
#[allow(dead_code)]
impl ProtectionSyncOutcome {
    /// 提供confirmed的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub(super) fn confirmed(
        protective_order_external_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Confirmed {
            protective_order_external_id: protective_order_external_id.into(),
            source: source.into(),
        }
    }
    /// 封装失败，减少Web 商业链路调用方重复实现相同细节。
    pub(super) fn failed(reason: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            error_message: error_message.into(),
        }
    }
    /// 判断cancelfailed，给Web 商业链路流程提供布尔结果。
    pub(super) fn cancel_failed(
        reason: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        Self::CancelFailed {
            reason: reason.into(),
            error_message: error_message.into(),
            cancel_client_order_id: None,
        }
    }
    /// 判断cancelfailedwithclient订单ID，给Web 商业链路流程提供布尔结果。
    pub(super) fn cancel_failed_with_client_order_id(
        reason: impl Into<String>,
        error_message: impl Into<String>,
        cancel_client_order_id: Option<String>,
    ) -> Self {
        Self::CancelFailed {
            reason: reason.into(),
            error_message: error_message.into(),
            cancel_client_order_id,
        }
    }
    /// 提供uncertain的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub(super) fn uncertain(reason: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self::Uncertain {
            reason: reason.into(),
            error_message: error_message.into(),
        }
    }
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
pub(super) fn attached_stop_loss_order_ack_outcome(
    order_task: &ExecutionOrderTask,
    ack: &OrderAck,
    order: Option<&Order>,
) -> Option<ProtectionSyncOutcome> {
    match order_task.exchange {
        ExchangeId::Okx | ExchangeId::Bitget => {
            if order_task.attached_stop_loss_price.is_none() {
                return Some(ProtectionSyncOutcome::failed(
                    "attached_stop_loss_request_missing",
                    "protective stop-loss was required but live order request did not carry an attached stop-loss price",
                ));
            }
            if attached_stop_loss_evidence_present(order_task.exchange, &ack.raw)
                || order.is_some_and(|order| {
                    attached_stop_loss_evidence_present(order_task.exchange, &order.raw)
                })
            {
                return Some(ProtectionSyncOutcome::confirmed(
                    attached_stop_loss_external_id(order_task.exchange, ack, order)
                        .or_else(|| ack.order_id.clone())
                        .or_else(|| ack.client_order_id.clone())
                        .unwrap_or_else(|| "attached_stop_loss".to_string()),
                    "place_order_attached_stop_loss_ack",
                ));
            }
            Some(ProtectionSyncOutcome::failed(
                "attached_stop_loss_ack_missing",
                format!(
                    "{} attached stop-loss was requested but the exchange order ack/detail did not include protective stop-loss evidence",
                    order_task.exchange.as_str()
                ),
            ))
        }
        ExchangeId::Binance | ExchangeId::Bybit | ExchangeId::Gate | ExchangeId::Hyperliquid => {
            None
        }
    }
}
/// 提供attached止损亏损evidencepresent的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn attached_stop_loss_evidence_present(exchange: ExchangeId, value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            attached_stop_loss_key_matches(exchange, key) && value_has_content(value)
                || attached_stop_loss_evidence_present(exchange, value)
        }),
        Value::Array(items) => items
            .iter()
            .any(|item| attached_stop_loss_evidence_present(exchange, item)),
        _ => false,
    }
}
/// 提供attached止损亏损externalID的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn attached_stop_loss_external_id(
    exchange: ExchangeId,
    ack: &OrderAck,
    order: Option<&Order>,
) -> Option<String> {
    attached_stop_loss_external_id_from_value(exchange, &ack.raw).or_else(|| {
        order.and_then(|order| attached_stop_loss_external_id_from_value(exchange, &order.raw))
    })
}
/// 提供attached止损亏损externalIDfrom值的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn attached_stop_loss_external_id_from_value(
    exchange: ExchangeId,
    value: &Value,
) -> Option<String> {
    match value {
        Value::Object(fields) => {
            if object_has_direct_attached_stop_loss_evidence(exchange, fields) {
                let keys: &[&str] = match exchange {
                    ExchangeId::Okx => &[
                        "attachAlgoId",
                        "attach_algo_id",
                        "attachAlgoClOrdId",
                        "attach_algo_cl_ord_id",
                        "clientAlgoId",
                        "algoId",
                    ],
                    ExchangeId::Bitget => &["orderId", "clientOid", "client_order_id"],
                    ExchangeId::Binance
                    | ExchangeId::Bybit
                    | ExchangeId::Gate
                    | ExchangeId::Hyperliquid => &[],
                };
                if let Some(external_id) = string_field_from_object(fields, keys) {
                    return Some(external_id);
                }
            }
            fields
                .values()
                .find_map(|value| attached_stop_loss_external_id_from_value(exchange, value))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|value| attached_stop_loss_external_id_from_value(exchange, value)),
        _ => None,
    }
}
/// 提供objecthasdirectattached止损亏损evidence的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn object_has_direct_attached_stop_loss_evidence(
    exchange: ExchangeId,
    fields: &serde_json::Map<String, Value>,
) -> bool {
    fields.iter().any(|(key, value)| {
        attached_stop_loss_key_matches(exchange, key) && value_has_content(value)
    })
}
/// 封装字符串field来源object，减少Web 商业链路调用方重复实现相同细节。
fn string_field_from_object(
    fields: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        fields
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}
/// 提供attached止损亏损keymatches的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn attached_stop_loss_key_matches(exchange: ExchangeId, key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase().replace('_', "");
    match exchange {
        ExchangeId::Okx => {
            normalized == "sltriggerpx"
                || normalized == "slordpx"
                || normalized == "sltriggerpxtype"
        }
        ExchangeId::Bitget => normalized == "presetstoplossprice" || normalized == "stoplossprice",
        ExchangeId::Binance | ExchangeId::Bybit | ExchangeId::Gate | ExchangeId::Hyperliquid => {
            false
        }
    }
}
/// 封装价值hascontent，减少Web 商业链路调用方重复实现相同细节。
fn value_has_content(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => true,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(items) => !items.is_empty() && items.iter().any(value_has_content),
        Value::Object(fields) => !fields.is_empty() && fields.values().any(value_has_content),
    }
}
#[derive(Debug, Clone)]
pub(super) struct ProtectionSyncContract {
    /// 止损价格。
    pub(super) selected_stop_loss_price: f64,
    /// 方向。
    direction: ProtectiveDirection,
    /// 入场reference价格；为空时表示该过滤条件不启用。
    entry_reference_price: Option<f64>,
    /// 止损价格。
    original_selected_stop_loss_price: Option<f64>,
}
const DEFAULT_PROTECTIVE_STOP_REBASE_RATIO: f64 = 0.02;
const PROTECTIVE_ORDER_QUERY_ATTEMPTS: usize = 4;
const PROTECTIVE_ORDER_QUERY_BACKOFF_MS: u64 = 250;
#[derive(Debug, Clone)]
pub(super) struct PrearmedProtectiveOrder {
    /// 交易所名称。
    exchange: ExchangeId,
    /// protection，用于记录交易或执行状态。
    protection: ProtectionSyncContract,
    /// cancel请求，用于构建接口请求。
    cancel_request: CancelOrderRequest,
    /// protective订单external ID。
    protective_order_external_id: String,
    /// confirmation来源，用于记录交易或执行状态。
    confirmation_source: String,
}
impl PrearmedProtectiveOrder {
    /// 判断cancel之后入口订单failure，给Web 商业链路流程提供布尔结果。
    pub(super) async fn cancel_after_main_order_failure(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        mutator: &impl ProtectiveOrderMutator,
    ) -> crypto_exc_all::Result<OrderAck> {
        mutator
            .audit_cancel_protective(task, gateway, self.exchange, self.cancel_request.clone())
            .await
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub(super) fn apply_after_main_order_report(&self, report: &mut ExecutionTaskReportRequest) {
        if report.order_status.trim().eq_ignore_ascii_case("FILLED") {
            self.apply_confirmed_to_filled_report(report);
            return;
        }
        if !report
            .execution_status
            .trim()
            .eq_ignore_ascii_case("pending_confirmation")
        {
            return;
        }
        let mut raw_payload = report
            .raw_payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| json!({}));
        raw_payload["prearmed_protection"] = json!({
            "exchange": self.exchange.as_str(),
            "status": "active_waiting_for_main_fill",
            "main_order_placed": true,
            "main_order_status": report.order_status.clone(),
            "protective_order_confirmed": true,
            "protective_order_external_id": self.protective_order_external_id.clone(),
            "confirmation_source": self.confirmation_source.clone(),
            "cancel_client_order_id": self.cancel_request.client_order_id.clone(),
            "repeat_open_order_allowed": false,
        });
        raw_payload["execution_status"] = json!(report.execution_status);
        report.raw_payload_json = Some(raw_payload.to_string());
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub(super) fn apply_confirmed_to_filled_report(&self, report: &mut ExecutionTaskReportRequest) {
        if !report.order_status.trim().eq_ignore_ascii_case("FILLED") {
            return;
        }
        self.protection.apply_outcome_to_report(
            report,
            ProtectionSyncOutcome::confirmed(
                self.protective_order_external_id.clone(),
                "prearmed_protective_order",
            ),
        );
        let mut raw_payload = report
            .raw_payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| json!({}));
        raw_payload["protection_sync"]["prearmed_protective_order"] = json!(true);
        raw_payload["protection_sync"]["confirmation_source"] =
            json!(self.confirmation_source.clone());
        raw_payload["protection_sync"]["cancel_client_order_id"] =
            json!(self.cancel_request.client_order_id.clone());
        raw_payload["execution_status"] = json!(report.execution_status);
        report.raw_payload_json = Some(raw_payload.to_string());
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub(super) fn apply_main_order_failure_cancel_result(
        &self,
        report: &mut ExecutionTaskReportRequest,
        main_order_error: &str,
        result: crypto_exc_all::Result<OrderAck>,
    ) {
        self.apply_order_path_failure_cancel_result(
            report,
            "main_order_failure",
            main_order_error,
            result,
        );
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    fn apply_order_path_failure_cancel_result(
        &self,
        report: &mut ExecutionTaskReportRequest,
        failure_stage: &str,
        failure_message: &str,
        result: crypto_exc_all::Result<OrderAck>,
    ) {
        let status_suffix = if failure_stage == "main_order_failure" {
            "main_order_failure"
        } else {
            "pre_main_order_failure"
        };
        let mut raw_payload = report
            .raw_payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| json!({}));
        let mut cleanup = json!({
            "exchange": self.exchange.as_str(),
            "status": format!("cancelled_after_{status_suffix}"),
            "failure_stage": failure_stage,
            "main_order_placed": false,
            "failure_message": failure_message,
            "protective_order_external_id": self.protective_order_external_id.clone(),
            "confirmation_source": self.confirmation_source.clone(),
            "cancel_client_order_id": self.cancel_request.client_order_id.clone(),
            "place_order_allowed": false,
            "repeat_open_order_allowed": false,
        });
        match result {
            Ok(ack) => {
                cleanup["protective_order_cancelled"] = json!(true);
                cleanup["cancel_external_order_id"] = json!(ack.order_id);
                cleanup["cancel_response_client_order_id"] = json!(ack.client_order_id);
            }
            Err(error) if is_protective_order_already_absent(&error) => {
                cleanup["status"] = json!(format!("already_absent_after_{status_suffix}"));
                cleanup["protective_order_cancelled"] = json!(false);
                cleanup["protective_order_absent"] = json!(true);
                cleanup["error_message"] = json!(error.to_string());
            }
            Err(error) => {
                let message = error.to_string();
                cleanup["status"] =
                    json!(format!("protective_cancel_failed_after_{status_suffix}"));
                cleanup["protective_order_cancelled"] = json!(false);
                cleanup["error_message"] = json!(message);
                report.execution_status = "protective_cancel_failed".to_string();
                report.error_message = Some(format!(
                    "order path failed after prearmed protective order; protective cancel failed: {message}; original failure: {failure_message}"
                ));
            }
        }
        raw_payload["prearmed_protection"] = cleanup;
        raw_payload["execution_status"] = json!(report.execution_status);
        report.raw_payload_json = Some(raw_payload.to_string());
    }
}
impl ProtectionSyncContract {
    pub(super) fn from_task(task: &ExecutionTask, order_side: &str) -> Option<Self> {
        Self::required_for_task(task, order_side).ok()
    }
    #[cfg(test)]
    pub(super) fn required(payload: Value, order_side: &str) -> Result<Self> {
        Self::required_from_payload(payload, order_side, false)
    }
    /// 封装必需fortask，减少Web 商业链路调用方重复实现相同细节。
    fn required_for_task(task: &ExecutionTask, order_side: &str) -> Result<Self> {
        let payload = order_payload(&task.request_payload_json);
        Self::required_from_payload(payload, order_side, task.news_signal_id.is_some())
    }
    /// 封装必需来源载荷，减少Web 商业链路调用方重复实现相同细节。
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
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub(super) fn apply_to_report(&self, report: &mut ExecutionTaskReportRequest) {
        self.apply_outcome_to_report(
            report,
            ProtectionSyncOutcome::uncertain(
                "protective_order_sync_not_confirmed",
                "protective stop-loss required but protection order sync is not confirmed",
            ),
        );
    }
    /// 从外部输入转换为内部模型，隔离 Web 商业、会员和执行准备度 的字段适配细节。
    pub(super) fn from_task_result(
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
    /// 提供rebased之后filled报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 停止 Web 商业、会员和执行准备度 后台流程，确保退出时不留下未释放状态。
    pub(super) fn stop_reset_for_order_task(
        order_task: &ExecutionOrderTask,
        selected_stop_loss_price: f64,
    ) -> Result<Self> {
        if !selected_stop_loss_price.is_finite() || selected_stop_loss_price <= 0.0 {
            return Err(anyhow!("take-profit stop reset price must be positive"));
        }
        let direction = match order_task.side {
            OrderSide::Buy => ProtectiveDirection::Long,
            OrderSide::Sell => ProtectiveDirection::Short,
        };
        Ok(Self {
            selected_stop_loss_price,
            direction,
            entry_reference_price: None,
            original_selected_stop_loss_price: order_task
                .attached_stop_loss_price
                .as_deref()
                .and_then(|value| value.trim().parse::<f64>().ok()),
        })
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub(super) fn apply_outcome_to_report(
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
            "contract_version": "v2",
            "exchange": report.exchange.trim().to_ascii_lowercase(),
            "protective_order_mode": protective_order_mode_for_exchange(&report.exchange),
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
            ProtectionSyncOutcome::CancelFailed {
                reason,
                error_message,
                cancel_client_order_id,
            } => {
                protection_sync["status"] = json!("protective_cancel_failed");
                protection_sync["reason"] = json!(reason);
                protection_sync["protective_order_confirmed"] = json!(false);
                protection_sync["exchange_protective_order_supported"] = json!(true);
                protection_sync["manual_cleanup_required"] = json!(true);
                protection_sync["error_message"] = json!(error_message);
                if let Some(cancel_client_order_id) = cancel_client_order_id {
                    protection_sync["cancel_client_order_id"] = json!(cancel_client_order_id);
                }
                report.execution_status = "protective_cancel_failed".to_string();
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
/// 提供protective订单modefor交易所的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn protective_order_mode_for_exchange(exchange: &str) -> &'static str {
    match exchange.trim().to_ascii_lowercase().as_str() {
        "okx" | "bitget" => "attached_stop_loss",
        _ => "independent_stop_market",
    }
}
/// 提供filledaverage价格的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn filled_average_price(report: &ExecutionTaskReportRequest) -> Option<f64> {
    let qty = report.filled_qty?;
    let quote = report.filled_quote?;
    if qty.is_finite() && quote.is_finite() && qty > 0.0 && quote > 0.0 {
        Some(quote / qty)
    } else {
        None
    }
}
/// 停止 Web 商业、会员和执行准备度 后台流程，确保退出时不留下未释放状态。
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
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub(super) fn build_protective_stop_market_order_request(
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

fn fixed_size_prearmed_protective_order_request(
    order_task: &ExecutionOrderTask,
    protection: &ProtectionSyncContract,
    filters: &ExchangeOrderFilters,
    mut request: ProtectiveOrderRequest,
) -> Result<ProtectiveOrderRequest> {
    if order_task.exchange != ExchangeId::Binance {
        return Ok(request);
    }
    let requested_size = Decimal::from_str(order_task.size.trim())
        .map_err(|_| anyhow!("prearmed protective order size must be decimal"))?;
    let entry_price = protection
        .entry_reference_price
        .ok_or_else(|| anyhow!("prearmed protective order entry reference price is required"))?;
    let last_price = Decimal::from_f64_retain(entry_price)
        .filter(|value| *value > Decimal::ZERO)
        .ok_or_else(|| {
            anyhow!("prearmed protective order entry reference price must be positive")
        })?;
    let quantity = quantize_order_size(requested_size, last_price, filters, true)?;
    request.close_position = None;
    request.quantity = Some(format_order_size_decimal(quantity, filters));
    Ok(request)
}
/// 提供prearmprotective订单ifrequired的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) async fn prearm_protective_order_if_required(
    gateway: &CryptoExcAllGateway,
    order_task: &ExecutionOrderTask,
    protection: Option<&ProtectionSyncContract>,
    task: &ExecutionTask,
    mutator: &impl ProtectiveOrderMutator,
) -> std::result::Result<
    Option<PrearmedProtectiveOrder>,
    (ProtectionSyncContract, ProtectionSyncOutcome),
> {
    let Some(protection) = protection else {
        return Ok(None);
    };
    if !exchange_uses_prearmed_protective_order(order_task.exchange) {
        return Ok(None);
    }
    let filters = match load_exchange_order_filters(order_task.exchange, &order_task.symbol).await {
        Ok(Some(filters)) => filters,
        Ok(None) => {
            return Err((
                protection.clone(),
                ProtectionSyncOutcome::failed(
                    "load_prearmed_protective_order_filters",
                    format!(
                        "missing exchange symbol filters for {} on {} before prearmed protective order",
                        order_task.symbol,
                        order_task.exchange.as_str()
                    ),
                ),
            ));
        }
        Err(error) => {
            return Err((
                protection.clone(),
                ProtectionSyncOutcome::failed(
                    "load_prearmed_protective_order_filters",
                    error.to_string(),
                ),
            ));
        }
    };
    let request = match build_protective_stop_market_order_request(order_task, protection, &filters)
    {
        Ok(request) => request,
        Err(error) => {
            return Err((
                protection.clone(),
                ProtectionSyncOutcome::failed(
                    "build_prearmed_protective_order_request",
                    error.to_string(),
                ),
            ));
        }
    };
    let request = match fixed_size_prearmed_protective_order_request(
        order_task, protection, &filters, request,
    ) {
        Ok(request) => request,
        Err(error) => {
            return Err((
                protection.clone(),
                ProtectionSyncOutcome::failed(
                    "build_prearmed_protective_order_request",
                    error.to_string(),
                ),
            ));
        }
    };
    let cancel_request = match prearmed_protection_cancel_request_from_request(&request) {
        Ok(request) => request,
        Err(error) => {
            return Err((
                protection.clone(),
                ProtectionSyncOutcome::failed(
                    "build_prearmed_protective_cancel_request",
                    error.to_string(),
                ),
            ));
        }
    };
    let outcome =
        place_and_confirm_protective_order(gateway, order_task.exchange, request, task, mutator)
            .await;
    match outcome {
        ProtectionSyncOutcome::Confirmed {
            protective_order_external_id,
            source,
        } => Ok(Some(PrearmedProtectiveOrder {
            exchange: order_task.exchange,
            protection: protection.clone(),
            cancel_request,
            protective_order_external_id,
            confirmation_source: source,
        })),
        ProtectionSyncOutcome::Uncertain {
            reason,
            error_message,
        } => {
            let cancel_result = mutator
                .audit_cancel_protective(task, gateway, order_task.exchange, cancel_request.clone())
                .await;
            let outcome = match cancel_result {
                Ok(_) => ProtectionSyncOutcome::failed(
                    reason,
                    format!(
                        "{error_message}; unconfirmed prearmed protective order was cancelled before main order"
                    ),
                ),
                Err(error) if is_protective_order_already_absent(&error) => {
                    ProtectionSyncOutcome::failed(
                        reason,
                        format!(
                            "{error_message}; unconfirmed prearmed protective order was already absent before main order"
                        ),
                    )
                }
                Err(error) => ProtectionSyncOutcome::cancel_failed_with_client_order_id(
                    "cancel_unconfirmed_prearmed_protective_order",
                    format!(
                        "prearmed protective order was placed but not confirmed active; protective cancel failed before main order: {}; original protection confirmation error: {error_message}",
                        redact_error_message(error.to_string())
                    ),
                    cancel_request.client_order_id.clone(),
                ),
            };
            Err((protection.clone(), outcome))
        }
        other => Err((protection.clone(), other)),
    }
}
/// 提供交易所usesprearmedprotective订单的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn exchange_uses_prearmed_protective_order(exchange: ExchangeId) -> bool {
    match exchange {
        ExchangeId::Binance | ExchangeId::Bybit | ExchangeId::Gate => true,
        ExchangeId::Okx | ExchangeId::Bitget | ExchangeId::Hyperliquid => false,
    }
}
/// 提供prearmed保护cancelrequestfromrequest的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn prearmed_protection_cancel_request_from_request(
    request: &ProtectiveOrderRequest,
) -> Result<CancelOrderRequest> {
    let client_order_id = request
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("prearmed protective order requires a stable client order id"))?;
    Ok(CancelOrderRequest::by_client_order_id(
        request.instrument.clone(),
        client_order_id.to_string(),
    ))
}
/// 提供protective订单结果to同步结果的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn protective_order_result_to_sync_outcome(
    result: crypto_exc_all::Result<OrderAck>,
) -> ProtectionSyncOutcome {
    match result {
        Ok(_ack) => ProtectionSyncOutcome::uncertain(
            "query_protective_order",
            "protective order ack requires active query confirmation",
        ),
        Err(error) => ProtectionSyncOutcome::failed("place_protective_order", error.to_string()),
    }
}
/// 提供placeandconfirmprotective订单的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) async fn place_and_confirm_protective_order(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    request: ProtectiveOrderRequest,
    task: &ExecutionTask,
    mutator: &impl ProtectiveOrderMutator,
) -> ProtectionSyncOutcome {
    let instrument = request.instrument.clone();
    let request_client_order_id = request.client_order_id.clone();
    let ack = match mutator
        .audit_place_protective(task, gateway, exchange, request)
        .await
    {
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
            match CryptoExcAllGateway::with_signed_read_only_scope(
                gateway.protective_order(exchange, query),
            )
            .await
            {
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
/// 提供protective订单查询candidatesfromack的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn protective_order_query_candidates_from_ack(
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
/// 提供protective订单查询to同步结果的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn protective_order_query_to_sync_outcome(
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
        Err(error) if is_protective_order_already_absent(&error) => ProtectionSyncOutcome::failed(
            "query_protective_order",
            redact_error_message(error.to_string()),
        ),
        Err(error) => ProtectionSyncOutcome::uncertain(
            "query_protective_order",
            redact_error_message(error.to_string()),
        ),
    }
}
/// 提供protective订单statusisactive的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn protective_order_status_is_active(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_uppercase().as_str(),
        "NEW" | "WORKING" | "ACCEPTED"
    )
}
fn protective_order_client_id(task_id: i64) -> String {
    format!("rq-sl-{task_id}")
}
/// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub(super) fn apply_post_close_protection_cancel_result(
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
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
pub(super) fn is_protective_order_already_absent(error: &crypto_exc_all::Error) -> bool {
    matches!(
        error,
        crypto_exc_all::Error::Api {
            exchange: ExchangeId::Binance,
            code,
            ..
        } if matches!(code.as_str(), "-2011" | "-2013")
    )
}
#[cfg(test)]
mod tests;
