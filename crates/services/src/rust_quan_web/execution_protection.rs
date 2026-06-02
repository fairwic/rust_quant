use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, Instrument, Order, OrderAck, OrderSide, ProtectiveOrderQuery,
    ProtectiveOrderRequest, ProtectiveOrderWorkingType,
};
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use super::execution_order_filters::{
    format_protective_stop_price_decimal, load_exchange_order_filters,
    quantize_protective_stop_price, ExchangeOrderFilters,
};
use super::execution_payload::{
    order_payload, parse_instrument, parse_protective_direction, protection_entry_price,
    protective_stop_loss_required, risk_plan_direction_raw, selected_stop_loss_price,
};
use super::execution_worker::ExecutionOrderTask;
use crate::exchange::CryptoExcAllGateway;
use crate::rust_quan_web::{ExecutionTask, ExecutionTaskReportRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProtectiveDirection {
    Long,
    Short,
}

impl ProtectiveDirection {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            ProtectiveDirection::Long => "long",
            ProtectiveDirection::Short => "short",
        }
    }

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
    Uncertain {
        reason: String,
        error_message: String,
    },
}

#[allow(dead_code)]
impl ProtectionSyncOutcome {
    pub(super) fn confirmed(
        protective_order_external_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Confirmed {
            protective_order_external_id: protective_order_external_id.into(),
            source: source.into(),
        }
    }

    pub(super) fn failed(reason: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            error_message: error_message.into(),
        }
    }

    pub(super) fn uncertain(reason: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self::Uncertain {
            reason: reason.into(),
            error_message: error_message.into(),
        }
    }
}

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
                    ack.order_id
                        .clone()
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
        ExchangeId::Binance => None,
    }
}

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

fn attached_stop_loss_key_matches(exchange: ExchangeId, key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase().replace('_', "");
    match exchange {
        ExchangeId::Okx => {
            normalized.contains("attachalgo")
                || normalized == "linkedalgoord"
                || normalized == "algoid"
        }
        ExchangeId::Bitget => {
            normalized.contains("presetstoploss") || normalized.contains("stoploss")
        }
        ExchangeId::Binance => false,
    }
}

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
    pub(super) selected_stop_loss_price: f64,
    direction: ProtectiveDirection,
    entry_reference_price: Option<f64>,
    original_selected_stop_loss_price: Option<f64>,
}

const DEFAULT_PROTECTIVE_STOP_REBASE_RATIO: f64 = 0.02;
const PROTECTIVE_ORDER_QUERY_ATTEMPTS: usize = 4;
const PROTECTIVE_ORDER_QUERY_BACKOFF_MS: u64 = 250;

#[derive(Debug, Clone)]
pub(super) struct PrearmedProtectiveOrder {
    exchange: ExchangeId,
    protection: ProtectionSyncContract,
    cancel_request: CancelOrderRequest,
    protective_order_external_id: String,
    confirmation_source: String,
}

impl PrearmedProtectiveOrder {
    pub(super) async fn cancel_after_main_order_failure(
        &self,
        gateway: &CryptoExcAllGateway,
    ) -> crypto_exc_all::Result<OrderAck> {
        gateway
            .cancel_protective_order(self.exchange, self.cancel_request.clone())
            .await
    }

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

    pub(super) fn apply_main_order_failure_cancel_result(
        &self,
        report: &mut ExecutionTaskReportRequest,
        main_order_error: &str,
        result: crypto_exc_all::Result<OrderAck>,
    ) {
        let mut raw_payload = report
            .raw_payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| json!({}));
        let mut cleanup = json!({
            "exchange": self.exchange.as_str(),
            "status": "cancelled_after_main_order_failure",
            "main_order_placed": false,
            "main_order_error": main_order_error,
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
                cleanup["status"] = json!("already_absent_after_main_order_failure");
                cleanup["protective_order_cancelled"] = json!(false);
                cleanup["protective_order_absent"] = json!(true);
                cleanup["error_message"] = json!(error.to_string());
            }
            Err(error) => {
                let message = error.to_string();
                cleanup["status"] = json!("protective_cancel_failed_after_main_order_failure");
                cleanup["protective_order_cancelled"] = json!(false);
                cleanup["error_message"] = json!(message);
                report.execution_status = "protective_cancel_failed".to_string();
                report.error_message = Some(format!(
                    "main order failed after prearmed protective order; protective cancel failed: {message}"
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

    pub(super) fn apply_to_report(&self, report: &mut ExecutionTaskReportRequest) {
        self.apply_outcome_to_report(
            report,
            ProtectionSyncOutcome::uncertain(
                "protective_order_sync_not_confirmed",
                "protective stop-loss required but protection order sync is not confirmed",
            ),
        );
    }

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

pub(super) async fn prearm_protective_order_if_required(
    gateway: &CryptoExcAllGateway,
    order_task: &ExecutionOrderTask,
    protection: Option<&ProtectionSyncContract>,
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
    let outcome = place_and_confirm_protective_order(gateway, order_task.exchange, request).await;
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
        other => Err((protection.clone(), other)),
    }
}

pub(super) fn exchange_uses_prearmed_protective_order(exchange: ExchangeId) -> bool {
    match exchange {
        ExchangeId::Binance | ExchangeId::Okx | ExchangeId::Bitget => false,
    }
}

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

pub(super) fn protective_order_result_to_sync_outcome(
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

pub(super) async fn place_and_confirm_protective_order(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_exc_all::{CancelOrderRequest, OrderType};

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

    fn binance_buy_order_task() -> ExecutionOrderTask {
        ExecutionOrderTask {
            task_id: 42,
            exchange: ExchangeId::Binance,
            symbol: "ETHUSDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            size: "0.01".to_string(),
            price: None,
            margin_mode: None,
            leverage: None,
            position_mode: None,
            margin_coin: Some("USDT".to_string()),
            position_side: Some("BOTH".to_string()),
            trade_side: None,
            client_order_id: Some("rqtask42".to_string()),
            reduce_only: None,
            time_in_force: None,
            size_usdt: None,
            attached_stop_loss_price: Some("2100".to_string()),
        }
    }

    fn long_protection() -> ProtectionSyncContract {
        ProtectionSyncContract {
            selected_stop_loss_price: 2100.0,
            direction: ProtectiveDirection::Long,
            entry_reference_price: Some(2400.0),
            original_selected_stop_loss_price: None,
        }
    }

    fn cancel_ack() -> OrderAck {
        OrderAck {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT"),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some("2000000953242572".to_string()),
            client_order_id: Some("rq-sl-42".to_string()),
            status: Some("CANCELED".to_string()),
            raw: json!({"algoId": 2000000953242572_i64, "clientAlgoId": "rq-sl-42"}),
        }
    }

    fn prearmed_protection() -> PrearmedProtectiveOrder {
        PrearmedProtectiveOrder {
            exchange: ExchangeId::Binance,
            protection: long_protection(),
            cancel_request: CancelOrderRequest::by_client_order_id(
                Instrument::perp("ETH", "USDT"),
                "rq-sl-42",
            ),
            protective_order_external_id: "2000000953242572".to_string(),
            confirmation_source: "query_protective_order".to_string(),
        }
    }

    #[test]
    fn prearmed_protection_builds_cancel_request_from_protective_client_order_id() {
        let request = build_protective_stop_market_order_request(
            &binance_buy_order_task(),
            &long_protection(),
            &binance_eth_filters(),
        )
        .unwrap();

        let cancel = prearmed_protection_cancel_request_from_request(&request).unwrap();

        assert_eq!(cancel.client_order_id.as_deref(), Some("rq-sl-42"));
        assert_eq!(cancel.order_id, None);
        assert_eq!(cancel.instrument.symbol_for(ExchangeId::Binance), "ETHUSDT");
    }

    #[test]
    fn binance_protective_order_is_not_prearmed_before_main_fill() {
        assert!(!exchange_uses_prearmed_protective_order(
            ExchangeId::Binance
        ));
    }

    #[test]
    fn prearmed_confirmation_is_applied_only_after_main_order_is_filled() {
        let prearmed = prearmed_protection();
        let mut filled = ExecutionTaskReportRequest::success(
            42,
            "binance",
            "main-42",
            "buy",
            "FILLED",
            json!({"execution_status": "completed"}),
        );
        filled.filled_qty = Some(0.01);
        filled.filled_quote = Some(24.0);

        prearmed.apply_after_main_order_report(&mut filled);

        let raw: Value = serde_json::from_str(filled.raw_payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(filled.execution_status, "completed");
        assert_eq!(raw["protection_sync"]["status"], "completed");
        assert_eq!(
            raw["protection_sync"]["source"],
            "prearmed_protective_order"
        );
        assert_eq!(
            raw["protection_sync"]["confirmation_source"],
            "query_protective_order"
        );

        let mut pending = ExecutionTaskReportRequest::success(
            42,
            "binance",
            "main-42",
            "buy",
            "NEW",
            json!({"execution_status": "pending_confirmation"}),
        );
        pending.execution_status = "pending_confirmation".to_string();

        prearmed.apply_after_main_order_report(&mut pending);

        let raw: Value =
            serde_json::from_str(pending.raw_payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(pending.execution_status, "pending_confirmation");
        assert!(raw.get("protection_sync").is_none());
        assert_eq!(
            raw["prearmed_protection"]["status"],
            "active_waiting_for_main_fill"
        );
        assert_eq!(
            raw["prearmed_protection"]["protective_order_confirmed"],
            true
        );
    }

    #[test]
    fn prearmed_main_order_failure_records_protective_cancel_result() {
        let prearmed = prearmed_protection();
        let mut report = ExecutionTaskReportRequest::failed(
            42,
            "binance",
            "buy",
            "main order rejected",
            json!({"stage": "place_order"}),
        );

        prearmed.apply_main_order_failure_cancel_result(
            &mut report,
            "main order rejected",
            Ok(cancel_ack()),
        );

        let raw: Value = serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(report.execution_status, "failed");
        assert_eq!(
            raw["prearmed_protection"]["status"],
            "cancelled_after_main_order_failure"
        );
        assert_eq!(raw["prearmed_protection"]["main_order_placed"], false);
        assert_eq!(
            raw["prearmed_protection"]["protective_order_cancelled"],
            true
        );
        assert_eq!(
            raw["prearmed_protection"]["cancel_client_order_id"],
            "rq-sl-42"
        );
    }
}
