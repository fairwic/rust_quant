use super::execution_capability::{
    worker_live_capability_for_exchange, LiveWorkerCapabilityStatus,
};
use super::execution_order_filters::{
    decimal_from_f64, format_order_size_decimal, format_protective_stop_price_decimal,
    load_exchange_order_filters, quantize_order_size, quantize_protective_stop_price,
    ExchangeOrderFilters,
};
use super::execution_payload::{
    order_side_lower, parse_instrument, payload_f64, payload_string, protection_entry_price,
    selected_stop_loss_price,
};
use super::execution_protection::{
    build_protective_stop_market_order_request, is_protective_order_already_absent,
    place_and_confirm_protective_order, prearmed_protection_cancel_request_from_request,
    ProtectionSyncContract, ProtectionSyncOutcome, ProtectiveDirection, ProtectiveOrderMutator,
};
use super::execution_worker::ExecutionOrderTask;
use crate::exchange::{CryptoExcAllGateway, OrderPlacementRequest};
use crate::rust_quan_web::{ExecutionTask, ExecutionTaskReportRequest};
use anyhow::{anyhow, Result};
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, Order, OrderAck, OrderQuery, OrderSide, OrderType,
    ProtectiveOrderRequest, TimeInForce,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::{future::Future, pin::Pin};
pub(super) trait TakeProfitOrderPlacer {
    /// 提供placetake盈利订单的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn place_take_profit_order<'a>(
        &'a self,
        task: &'a ExecutionTask,
        gateway: &'a CryptoExcAllGateway,
        request: OrderPlacementRequest,
    ) -> Pin<Box<dyn Future<Output = crypto_exc_all::Result<OrderAck>> + Send + 'a>>;
}
#[derive(Debug, Clone, PartialEq)]
pub struct TakeProfitLeg {
    /// legindex。
    pub leg_index: usize,
    /// targetR 倍数；为空时表示该条件不启用。
    pub target_r: Option<f64>,
    /// fraction。
    pub fraction: f64,
    /// 价格。
    pub price: f64,
    /// 止损之后fillR 倍数；为空时表示该条件不启用。
    pub stop_after_fill_r: Option<f64>,
    /// 价格数值。
    pub stop_after_fill_price: Option<f64>,
    /// role；为空时表示该条件不启用。
    pub role: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TakeProfitStopResetPlan {
    /// legindex。
    pub leg_index: usize,
    /// role；为空时表示该条件不启用。
    pub role: Option<String>,
    /// 止盈clientorder ID；为空时使用默认值或表示不限制。
    pub take_profit_client_order_id: Option<String>,
    /// 价格数值。
    pub stop_after_fill_price: f64,
    /// cancel请求，用于构建接口请求。
    pub cancel_request: CancelOrderRequest,
    /// protective订单请求，用于构建接口请求。
    pub protective_order_request: ProtectiveOrderRequest,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TakeProfitSyncOutcome {
    Confirmed {
        orders: Vec<Value>,
    },
    Failed {
        stage: String,
        message: String,
        submitted_orders: Vec<Value>,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TakeProfitStopResetOutcome {
    NotRequired,
    Pending {
        checked_orders: Vec<Value>,
    },
    Confirmed {
        reset: Value,
    },
    Failed {
        stage: String,
        message: String,
        checked_orders: Vec<Value>,
        reset_attempt: Option<Value>,
    },
}
const STOP_RESET_OLD_CANCEL_STAGE: &str = "cancel_old_protective_order";
const STOP_RESET_OLD_CANCEL_FAILED_MESSAGE: &str =
    "old protective stop cancel failed after new stop was confirmed; manual cleanup required";
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
pub(super) fn parse_take_profit_legs(
    payload: &Value,
    direction: ProtectiveDirection,
) -> Result<Vec<TakeProfitLeg>> {
    let Some(legs_value) = take_profit_legs_value(payload) else {
        return Ok(Vec::new());
    };
    let legs = legs_value
        .as_array()
        .ok_or_else(|| anyhow!("risk_plan.take_profit_legs must be an array"))?;
    let entry_price = protection_entry_price(payload);
    let stop_loss_price = selected_stop_loss_price(payload);
    let mut parsed = Vec::new();
    let mut total_fraction = 0.0;
    let mut stop_after_fill_count = 0;
    for (offset, leg) in legs.iter().enumerate() {
        let leg_index = parse_take_profit_leg_index(leg, offset)?;
        if parsed
            .iter()
            .any(|parsed_leg: &TakeProfitLeg| parsed_leg.leg_index == leg_index)
        {
            return Err(anyhow!(
                "take_profit_legs[{offset}].duplicate leg_index {leg_index}"
            ));
        }
        let fraction = payload_f64(leg, "fraction")
            .or_else(|| payload_f64(leg, "size_fraction"))
            .or_else(|| payload_f64(leg, "position_fraction"))
            .ok_or_else(|| anyhow!("take_profit_legs[{offset}].fraction is required"))?;
        if !fraction.is_finite() || fraction <= 0.0 || fraction > 1.0 {
            return Err(anyhow!(
                "take_profit_legs[{offset}].fraction must be in (0, 1]"
            ));
        }
        total_fraction += fraction;
        let target_r = payload_f64(leg, "target_r").or_else(|| payload_f64(leg, "r"));
        let price = payload_f64(leg, "price")
            .or_else(|| payload_f64(leg, "target_price"))
            .or_else(|| payload_f64(leg, "take_profit_price"))
            .or_else(|| {
                target_r.and_then(|target_r| {
                    take_profit_price_from_target_r(
                        direction,
                        entry_price?,
                        stop_loss_price?,
                        target_r,
                    )
                })
            })
            .ok_or_else(|| {
                anyhow!(
                    "take_profit_legs[{offset}] requires price or target_r with entry/stop prices"
                )
            })?;
        validate_take_profit_price(direction, entry_price, price, offset)?;
        let stop_after_fill_r = match leg.get("stop_after_fill_r") {
            Some(value) if !value.is_null() => {
                let stop_after_fill_r = payload_f64(leg, "stop_after_fill_r").ok_or_else(|| {
                    anyhow!("take_profit_legs[{offset}].stop_after_fill_r must be numeric")
                })?;
                if !stop_after_fill_r.is_finite() {
                    return Err(anyhow!(
                        "take_profit_legs[{offset}].stop_after_fill_r must be a finite numeric value"
                    ));
                }
                Some(stop_after_fill_r)
            }
            _ => None,
        };
        let stop_after_fill_price = match stop_after_fill_r {
            Some(stop_after_fill_r) => {
                stop_after_fill_count += 1;
                if stop_after_fill_count > 1 {
                    return Err(anyhow!(
                        "multiple stop_after_fill_r take-profit legs require multi-stage stop reset state"
                    ));
                }
                Some(stop_after_fill_price_from_r(
                    direction,
                    entry_price.ok_or_else(|| {
                        anyhow!("take_profit_legs[{offset}].stop_after_fill_r requires entry price")
                    })?,
                    stop_loss_price.ok_or_else(|| {
                        anyhow!(
                            "take_profit_legs[{offset}].stop_after_fill_r requires stop-loss price"
                        )
                    })?,
                    stop_after_fill_r,
                    price,
                    offset,
                )?)
            }
            None => None,
        };
        parsed.push(TakeProfitLeg {
            leg_index,
            target_r,
            fraction,
            price,
            stop_after_fill_r,
            stop_after_fill_price,
            role: payload_string(leg, "role"),
        });
    }
    if total_fraction > 1.000001 {
        return Err(anyhow!(
            "take_profit_legs fractions must sum to <= 1.0, got {total_fraction}"
        ));
    }
    Ok(parsed)
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
fn parse_take_profit_leg_index(leg: &Value, offset: usize) -> Result<usize> {
    if leg.get("leg_index").is_none() {
        return Ok(offset + 1);
    }
    let value = payload_f64(leg, "leg_index").ok_or_else(|| {
        anyhow!("take_profit_legs[{offset}].leg_index must be a positive integer")
    })?;
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > usize::MAX as f64 {
        return Err(anyhow!(
            "take_profit_legs[{offset}].leg_index must be a positive integer"
        ));
    }
    Ok(value as usize)
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub(super) fn build_take_profit_order_requests(
    order_task: &ExecutionOrderTask,
    filled_qty: f64,
    filters: &ExchangeOrderFilters,
) -> Result<Vec<OrderPlacementRequest>> {
    if order_task.take_profit_legs.is_empty() {
        return Ok(Vec::new());
    }
    if !filled_qty.is_finite() || filled_qty <= 0.0 {
        return Err(anyhow!(
            "filled_qty must be positive before take-profit orders"
        ));
    }
    order_task
        .take_profit_legs
        .iter()
        .map(|leg| build_take_profit_order_request(order_task, leg, filled_qty, filters))
        .collect()
}
#[cfg(test)]
pub(super) fn build_take_profit_stop_reset_plan(
    order_task: &ExecutionOrderTask,
    leg: &TakeProfitLeg,
    take_profit_order: &Order,
    filters: &ExchangeOrderFilters,
) -> Result<Option<TakeProfitStopResetPlan>> {
    build_take_profit_stop_reset_plan_with_tracking(
        order_task,
        leg,
        take_profit_order,
        filters,
        None,
    )
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub(super) fn build_take_profit_stop_reset_plan_with_tracking(
    order_task: &ExecutionOrderTask,
    leg: &TakeProfitLeg,
    take_profit_order: &Order,
    filters: &ExchangeOrderFilters,
    previous_raw_payload_json: Option<&str>,
) -> Result<Option<TakeProfitStopResetPlan>> {
    let Some(stop_after_fill_price) = leg.stop_after_fill_price else {
        return Ok(None);
    };
    if !take_profit_order_is_filled(take_profit_order) {
        return Ok(None);
    }
    let expected_client_order_id = take_profit_order_client_id(order_task.task_id, leg.leg_index);
    let actual_client_order_id = take_profit_order
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if actual_client_order_id != Some(expected_client_order_id.as_str()) {
        return Err(anyhow!(
            "take-profit order client_order_id does not match leg {} for task {}",
            leg.leg_index,
            order_task.task_id
        ));
    }
    if let Some(message) = take_profit_order_filled_size_error(take_profit_order) {
        return Err(anyhow!(message));
    }
    if let Some(message) = take_profit_stop_reset_order_terms_error(
        order_task,
        leg,
        take_profit_order,
        filters,
        previous_raw_payload_json,
    )? {
        return Err(anyhow!(message));
    }
    let protection =
        ProtectionSyncContract::stop_reset_for_order_task(order_task, stop_after_fill_price)?;
    let original_protective_order_request =
        build_protective_stop_market_order_request(order_task, &protection, filters)?;
    let cancel_request =
        prearmed_protection_cancel_request_from_request(&original_protective_order_request)?;
    let protective_order_request = original_protective_order_request.with_client_order_id(
        take_profit_stop_reset_order_client_id(order_task.task_id, leg.leg_index),
    );
    Ok(Some(TakeProfitStopResetPlan {
        leg_index: leg.leg_index,
        role: leg.role.clone(),
        take_profit_client_order_id: Some(expected_client_order_id),
        stop_after_fill_price,
        cancel_request,
        protective_order_request,
    }))
}
/// 提供placeandconfirmtake盈利订单的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) async fn place_and_confirm_take_profit_orders(
    gateway: &CryptoExcAllGateway,
    requests: Vec<OrderPlacementRequest>,
    task: &ExecutionTask,
    placer: &impl TakeProfitOrderPlacer,
) -> TakeProfitSyncOutcome {
    let mut submitted_orders = Vec::new();
    for request in requests {
        match existing_take_profit_order(gateway, &request).await {
            Ok(Some(order)) => {
                let existing_order_record = existing_take_profit_order_record(&request, &order);
                if let Some(message) = existing_take_profit_order_status_error(&order) {
                    submitted_orders.push(existing_order_record);
                    if !existing_take_profit_order_allows_replacement(&order) {
                        return TakeProfitSyncOutcome::Failed {
                            stage: "query_existing_take_profit_order_status".to_string(),
                            message,
                            submitted_orders,
                        };
                    }
                } else if let Some(message) =
                    existing_take_profit_order_request_error(&order, &request)
                {
                    submitted_orders.push(existing_order_record);
                    return TakeProfitSyncOutcome::Failed {
                        stage: "query_existing_take_profit_order_terms".to_string(),
                        message,
                        submitted_orders,
                    };
                } else {
                    submitted_orders.push(existing_order_record);
                    continue;
                }
            }
            Ok(None) => {}
            Err(error) => {
                return TakeProfitSyncOutcome::Failed {
                    stage: "query_existing_take_profit_order".to_string(),
                    message: error.to_string(),
                    submitted_orders,
                };
            }
        }
        match placer
            .place_take_profit_order(task, gateway, request.clone())
            .await
        {
            Ok(ack) => {
                let ack_record = take_profit_order_ack_record(&request, &ack);
                if let Some(message) = take_profit_order_ack_status_error(&ack) {
                    submitted_orders.push(ack_record);
                    return TakeProfitSyncOutcome::Failed {
                        stage: "place_take_profit_order_ack_status".to_string(),
                        message,
                        submitted_orders,
                    };
                }
                if let Some(message) = take_profit_order_ack_request_error(&request, &ack) {
                    submitted_orders.push(ack_record);
                    return TakeProfitSyncOutcome::Failed {
                        stage: "place_take_profit_order_ack_terms".to_string(),
                        message,
                        submitted_orders,
                    };
                }
                submitted_orders.push(ack_record);
            }
            Err(error) => {
                return TakeProfitSyncOutcome::Failed {
                    stage: "place_take_profit_order".to_string(),
                    message: error.to_string(),
                    submitted_orders,
                };
            }
        }
    }
    TakeProfitSyncOutcome::Confirmed {
        orders: submitted_orders,
    }
}
/// 提供existingtake盈利订单record的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn existing_take_profit_order_record(
    request: &OrderPlacementRequest,
    order: &Order,
) -> Value {
    json!({
        "source": "client_order_id_pre_place_check",
        "external_order_id": order.order_id,
        "client_order_id": request_take_profit_client_order_id(request)
            .or_else(|| response_take_profit_client_order_id(order.client_order_id.as_deref())),
        "exchange_client_order_id": order.client_order_id,
        "order_status": order.status,
        "price": order.price,
        "size": order.size,
    })
}
/// 提供take盈利订单ackrecord的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn take_profit_order_ack_record(
    request: &OrderPlacementRequest,
    ack: &OrderAck,
) -> Value {
    json!({
        "source": "place_order_ack",
        "external_order_id": ack.order_id,
        "client_order_id": request_take_profit_client_order_id(request)
            .or_else(|| response_take_profit_client_order_id(ack.client_order_id.as_deref())),
        "exchange_client_order_id": ack.client_order_id,
        "order_status": ack.status,
        "side": order_side_lower(request.side),
        "size": request.size,
        "price": request.price,
    })
}
/// 提供take盈利订单ackrequesterror的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn take_profit_order_ack_request_error(
    request: &OrderPlacementRequest,
    ack: &OrderAck,
) -> Option<String> {
    let mut mismatches = Vec::new();
    if ack.exchange != request.exchange {
        mismatches.push(format!(
            "exchange existing={} expected={}",
            ack.exchange.as_str(),
            request.exchange.as_str()
        ));
    }
    push_instrument_mismatch(&mut mismatches, &ack.instrument, &request.instrument);
    if let Some(expected) = request_take_profit_client_order_id(request) {
        if let Some(actual) = response_take_profit_client_order_id(ack.client_order_id.as_deref()) {
            if !actual.eq_ignore_ascii_case(expected.as_str()) {
                mismatches.push(format!(
                    "client_order_id existing={actual} expected={expected}"
                ));
            }
        }
    }
    (!mismatches.is_empty()).then(|| {
        format!(
            "take-profit order ack terms mismatch: {}",
            mismatches.join(", ")
        )
    })
}
/// 封装请求takeprofitclientorderid，减少Web 商业链路调用方重复实现相同细节。
fn request_take_profit_client_order_id(request: &OrderPlacementRequest) -> Option<String> {
    request
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
/// 提供responsetake盈利client订单ID的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn response_take_profit_client_order_id(client_order_id: Option<&str>) -> Option<String> {
    client_order_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
/// 同步 Web 商业、会员和执行准备度 数据，保证本地状态与外部事实源保持一致。
pub(super) async fn sync_take_profit_orders_after_main_fill(
    gateway: &CryptoExcAllGateway,
    order_task: &ExecutionOrderTask,
    filled_qty: Option<f64>,
    task: &ExecutionTask,
    placer: &impl TakeProfitOrderPlacer,
) -> TakeProfitSyncOutcome {
    if let Some(message) = take_profit_stop_reset_capability_error(order_task) {
        return TakeProfitSyncOutcome::Failed {
            stage: "take_profit_stop_reset_capability".to_string(),
            message,
            submitted_orders: Vec::new(),
        };
    }
    let Some(filled_qty) = filled_qty.filter(|qty| qty.is_finite() && *qty > 0.0) else {
        return TakeProfitSyncOutcome::Failed {
            stage: "main_order_fill_qty".to_string(),
            message: "main order filled_qty is missing before take-profit orders".to_string(),
            submitted_orders: Vec::new(),
        };
    };
    match load_exchange_order_filters(order_task.exchange, &order_task.symbol).await {
        Ok(Some(filters)) => {
            match build_take_profit_order_requests(order_task, filled_qty, &filters) {
                Ok(requests) => {
                    place_and_confirm_take_profit_orders(gateway, requests, task, placer).await
                }
                Err(error) => TakeProfitSyncOutcome::Failed {
                    stage: "build_take_profit_order_request".to_string(),
                    message: error.to_string(),
                    submitted_orders: Vec::new(),
                },
            }
        }
        Ok(None) => TakeProfitSyncOutcome::Failed {
            stage: "load_take_profit_order_filters".to_string(),
            message: format!(
                "missing exchange symbol filters for {} on {} before take-profit orders",
                order_task.symbol,
                order_task.exchange.as_str()
            ),
            submitted_orders: Vec::new(),
        },
        Err(error) => TakeProfitSyncOutcome::Failed {
            stage: "load_take_profit_order_filters".to_string(),
            message: error.to_string(),
            submitted_orders: Vec::new(),
        },
    }
}
/// 同步 Web 商业、会员和执行准备度 数据，保证本地状态与外部事实源保持一致。
pub(super) async fn sync_take_profit_stop_reset_after_fills(
    gateway: &CryptoExcAllGateway,
    order_task: &ExecutionOrderTask,
    filters: &ExchangeOrderFilters,
    previous_raw_payload_json: Option<&str>,
    task: &ExecutionTask,
    mutator: &impl ProtectiveOrderMutator,
) -> TakeProfitStopResetOutcome {
    let legs = order_task
        .take_profit_legs
        .iter()
        .filter(|leg| leg.stop_after_fill_price.is_some())
        .collect::<Vec<_>>();
    if legs.is_empty() {
        return TakeProfitStopResetOutcome::NotRequired;
    }
    if let Some(message) = take_profit_stop_reset_capability_error(order_task) {
        return TakeProfitStopResetOutcome::Failed {
            stage: "take_profit_stop_reset_capability".to_string(),
            message,
            checked_orders: Vec::new(),
            reset_attempt: None,
        };
    }
    let mut checked_orders = Vec::new();
    for leg in legs {
        let client_order_id = take_profit_order_client_id(order_task.task_id, leg.leg_index);
        let mut query = match parse_instrument(&order_task.symbol) {
            Ok(instrument) => OrderQuery::by_client_order_id(instrument, client_order_id.clone()),
            Err(error) => {
                return TakeProfitStopResetOutcome::Failed {
                    stage: "build_take_profit_order_query".to_string(),
                    message: error.to_string(),
                    checked_orders,
                    reset_attempt: None,
                };
            }
        };
        if let Some(margin_coin) = order_task.margin_coin.as_deref() {
            query = query.with_margin_coin(margin_coin.to_string());
        }
        let take_profit_order = match CryptoExcAllGateway::with_signed_read_only_scope(
            gateway.order(order_task.exchange, query),
        )
        .await
        {
            Ok(order) => order,
            Err(error) if is_order_not_found(&error.to_string()) => {
                checked_orders.push(json!({
                    "leg_index": leg.leg_index,
                    "take_profit_client_order_id": client_order_id,
                    "order_status": "not_found",
                }));
                continue;
            }
            Err(error) => {
                return TakeProfitStopResetOutcome::Failed {
                    stage: "query_take_profit_order".to_string(),
                    message: error.to_string(),
                    checked_orders,
                    reset_attempt: None,
                };
            }
        };
        checked_orders.push(json!({
            "leg_index": leg.leg_index,
            "take_profit_client_order_id": client_order_id,
            "external_order_id": take_profit_order.order_id,
            "order_status": take_profit_order.status,
            "filled_size": take_profit_order.filled_size,
        }));
        let plan = match build_take_profit_stop_reset_plan_with_tracking(
            order_task,
            leg,
            &take_profit_order,
            filters,
            previous_raw_payload_json,
        ) {
            Ok(Some(plan)) => plan,
            Ok(None) => continue,
            Err(error) => {
                return TakeProfitStopResetOutcome::Failed {
                    stage: "build_take_profit_stop_reset_plan".to_string(),
                    message: error.to_string(),
                    checked_orders,
                    reset_attempt: None,
                };
            }
        };
        return execute_take_profit_stop_reset_plan(
            gateway,
            order_task.exchange,
            plan,
            checked_orders,
            task,
            mutator,
        )
        .await;
    }
    TakeProfitStopResetOutcome::Pending { checked_orders }
}
/// 提供take盈利止损resetcapabilityerror的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn take_profit_stop_reset_capability_error(
    order_task: &ExecutionOrderTask,
) -> Option<String> {
    if !order_task
        .take_profit_legs
        .iter()
        .any(|leg| leg.stop_after_fill_price.is_some())
    {
        return None;
    }
    let capability = worker_live_capability_for_exchange(order_task.exchange.as_str());
    if capability.protective_order_cancel == LiveWorkerCapabilityStatus::MutatingSupported {
        return None;
    }
    Some(format!(
        "take-profit stop reset requires protective order cancellation support for {}, current status={:?}",
        order_task.exchange.as_str(),
        capability.protective_order_cancel
    ))
}
/// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub(super) fn apply_take_profit_stop_reset_outcome_to_report(
    report: &mut ExecutionTaskReportRequest,
    outcome: TakeProfitStopResetOutcome,
    allow_pending_execution_status: bool,
) {
    let mut raw_payload = report_raw_payload(report);
    match outcome {
        TakeProfitStopResetOutcome::NotRequired => return,
        TakeProfitStopResetOutcome::Pending { checked_orders } => {
            let mut stop_reset = raw_payload
                .get("take_profit_stop_reset")
                .cloned()
                .unwrap_or_else(|| json!({}));
            stop_reset["status"] = json!("pending_take_profit_monitor");
            stop_reset["monitor_required"] = json!(true);
            stop_reset["checked_orders"] = json!(checked_orders);
            stop_reset["place_order_allowed"] = json!(false);
            stop_reset["repeat_open_order_allowed"] = json!(false);
            raw_payload["take_profit_stop_reset"] = stop_reset;
            if allow_pending_execution_status {
                report.execution_status = "pending_take_profit_monitor".to_string();
                report.error_message =
                    Some("waiting for take-profit fill before stop reset".to_string());
            }
        }
        TakeProfitStopResetOutcome::Confirmed { reset } => {
            if stop_reset_old_cancel_failed(&reset) {
                raw_payload["take_profit_stop_reset"] = json!({
                    "status": "take_profit_stop_reset_failed",
                    "stage": STOP_RESET_OLD_CANCEL_STAGE,
                    "message": STOP_RESET_OLD_CANCEL_FAILED_MESSAGE,
                    "checked_orders": reset
                        .get("checked_orders")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    "reset_attempt": reset,
                    "place_order_allowed": false,
                    "repeat_open_order_allowed": false,
                });
                report.execution_status = "take_profit_stop_reset_failed".to_string();
                report.error_message = Some(STOP_RESET_OLD_CANCEL_FAILED_MESSAGE.to_string());
            } else {
                raw_payload["take_profit_stop_reset"] = reset;
                report.execution_status = "completed".to_string();
                report.error_message = None;
            }
        }
        TakeProfitStopResetOutcome::Failed {
            stage,
            message,
            checked_orders,
            reset_attempt,
        } => {
            raw_payload["take_profit_stop_reset"] = json!({
                "status": "take_profit_stop_reset_failed",
                "stage": stage,
                "message": message,
                "checked_orders": checked_orders,
                "reset_attempt": reset_attempt,
                "place_order_allowed": false,
                "repeat_open_order_allowed": false,
            });
            report.execution_status = "take_profit_stop_reset_failed".to_string();
            report.error_message = raw_payload["take_profit_stop_reset"]["message"]
                .as_str()
                .map(ToOwned::to_owned);
        }
    }
    raw_payload["execution_status"] = json!(report.execution_status);
    report.raw_payload_json = Some(raw_payload.to_string());
}
/// 提供carrytake盈利trackingfromprevious报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn carry_take_profit_tracking_from_previous_report(
    report: &mut ExecutionTaskReportRequest,
    previous_raw_payload_json: &str,
) {
    let Some(previous) = serde_json::from_str::<Value>(previous_raw_payload_json).ok() else {
        return;
    };
    let mut raw_payload = report_raw_payload(report);
    for key in ["take_profit_sync", "take_profit_stop_reset"] {
        if raw_payload.get(key).is_none() {
            if let Some(value) = previous.get(key) {
                raw_payload[key] = value.clone();
            }
        }
    }
    raw_payload["execution_status"] = json!(report.execution_status);
    report.raw_payload_json = Some(raw_payload.to_string());
}
/// 提供take盈利止损resetmonitorrequired的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn take_profit_stop_reset_monitor_required(raw_payload_json: &str) -> bool {
    serde_json::from_str::<Value>(raw_payload_json)
        .ok()
        .and_then(|payload| payload.get("take_profit_stop_reset").cloned())
        .is_some_and(|stop_reset| {
            stop_reset
                .get("monitor_required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && stop_reset
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|status| {
                        status.eq_ignore_ascii_case("pending_take_profit_monitor")
                    })
        })
}
/// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn execute_take_profit_stop_reset_plan(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    plan: TakeProfitStopResetPlan,
    checked_orders: Vec<Value>,
    task: &ExecutionTask,
    mutator: &impl ProtectiveOrderMutator,
) -> TakeProfitStopResetOutcome {
    let mut reset = json!({
        "status": "take_profit_stop_reset_in_progress",
        "leg_index": plan.leg_index,
        "role": plan.role,
        "take_profit_client_order_id": plan.take_profit_client_order_id,
        "stop_after_fill_price": plan.stop_after_fill_price,
        "cancel_protective_client_order_id": plan.cancel_request.client_order_id,
        "new_protective_client_order_id": plan.protective_order_request.client_order_id,
        "new_protective_stop_price": plan.protective_order_request.stop_price,
        "switch_order": "place_new_protective_before_cancel_old",
        "place_order_allowed": false,
        "repeat_open_order_allowed": false,
    });
    match place_and_confirm_protective_order(
        gateway,
        exchange,
        plan.protective_order_request,
        task,
        mutator,
    )
    .await
    {
        ProtectionSyncOutcome::Confirmed {
            protective_order_external_id,
            source,
        } => {
            reset["status"] = json!("completed");
            reset["protective_order_confirmed"] = json!(true);
            reset["protective_order_external_id"] = json!(protective_order_external_id);
            reset["confirmation_source"] = json!(source);
            reset["checked_orders"] = json!(checked_orders);
        }
        ProtectionSyncOutcome::Failed {
            reason,
            error_message,
        }
        | ProtectionSyncOutcome::CancelFailed {
            reason,
            error_message,
            ..
        }
        | ProtectionSyncOutcome::Uncertain {
            reason,
            error_message,
        } => {
            reset["old_protective_order_preserved"] = json!(true);
            return TakeProfitStopResetOutcome::Failed {
                stage: reason,
                message: error_message,
                checked_orders,
                reset_attempt: Some(reset),
            };
        }
    }
    match mutator
        .audit_cancel_protective(task, gateway, exchange, plan.cancel_request.clone())
        .await
    {
        Ok(ack) => {
            reset["old_protective_order_cancelled"] = json!(true);
            reset["old_cancel_external_order_id"] = json!(ack.order_id);
            reset["old_cancel_client_order_id"] = json!(ack.client_order_id);
        }
        Err(error) if is_protective_order_already_absent(&error) => {
            reset["old_protective_order_cancelled"] = json!(false);
            reset["old_protective_order_absent"] = json!(true);
            reset["old_cancel_error_message"] = json!(error.to_string());
        }
        Err(error) => {
            reset["old_protective_order_cancelled"] = json!(false);
            reset["old_protective_cancel_failed"] = json!(true);
            reset["old_cancel_error_message"] = json!(error.to_string());
            reset["manual_cleanup_required"] = json!(true);
            return TakeProfitStopResetOutcome::Failed {
                stage: STOP_RESET_OLD_CANCEL_STAGE.to_string(),
                message: STOP_RESET_OLD_CANCEL_FAILED_MESSAGE.to_string(),
                checked_orders,
                reset_attempt: Some(reset),
            };
        }
    }
    TakeProfitStopResetOutcome::Confirmed { reset }
}
/// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub(super) fn apply_take_profit_sync_outcome_to_report(
    report: &mut ExecutionTaskReportRequest,
    order_task: &ExecutionOrderTask,
    outcome: TakeProfitSyncOutcome,
) {
    let mut raw_payload = report_raw_payload(report);
    match outcome {
        TakeProfitSyncOutcome::Confirmed { orders } => {
            raw_payload["take_profit_sync"] = json!({
                "status": "completed",
                "take_profit_order_confirmed": true,
                "take_profit_order_count": orders.len(),
                "orders": orders,
                "place_order_allowed": false,
                "repeat_open_order_allowed": false,
            });
            let stop_reset_legs = stop_reset_monitor_legs(order_task);
            if !stop_reset_legs.is_empty() {
                raw_payload["take_profit_stop_reset"] = json!({
                    "status": "pending_take_profit_monitor",
                    "monitor_required": true,
                    "legs": stop_reset_legs,
                    "place_order_allowed": false,
                    "repeat_open_order_allowed": false,
                });
            }
        }
        TakeProfitSyncOutcome::Failed {
            stage,
            message,
            submitted_orders,
        } => {
            raw_payload["take_profit_sync"] = json!({
                "status": "take_profit_order_retry_required",
                "retry_required": true,
                "stage": stage,
                "message": message,
                "take_profit_order_confirmed": false,
                "submitted_orders": submitted_orders,
                "place_order_allowed": false,
                "repeat_open_order_allowed": false,
            });
            if report.error_message.is_none() {
                report.error_message = Some(
                    "take-profit limit order sync failed after main order; protective stop remains authoritative"
                        .to_string(),
                );
            }
        }
    }
    raw_payload["execution_status"] = json!(report.execution_status);
    report.raw_payload_json = Some(raw_payload.to_string());
}
/// 提供take盈利同步retryrequired的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn take_profit_sync_retry_required(raw_payload_json: &str) -> bool {
    serde_json::from_str::<Value>(raw_payload_json)
        .ok()
        .and_then(|payload| payload.get("take_profit_sync").cloned())
        .is_some_and(|take_profit_sync| {
            take_profit_sync
                .get("retry_required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && take_profit_sync
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|status| {
                        status.eq_ignore_ascii_case("take_profit_order_retry_required")
                    })
        })
}
pub(super) fn take_profit_order_ack_status_error(ack: &OrderAck) -> Option<String> {
    take_profit_order_status_error(ack.status.as_deref(), "take-profit order ack")
}
pub(super) fn existing_take_profit_order_status_error(order: &Order) -> Option<String> {
    take_profit_order_status_error(order.status.as_deref(), "existing take-profit order")
}
/// 提供existingtake盈利订单allowsreplacement的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn existing_take_profit_order_allows_replacement(order: &Order) -> bool {
    matches!(
        order
            .status
            .as_deref()
            .map(|status| status.trim().to_ascii_uppercase())
            .as_deref(),
        Some("CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" | "FAILED" | "FAILURE")
    )
}
/// 提供existingtake盈利订单requesterror的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn existing_take_profit_order_request_error(
    order: &Order,
    request: &OrderPlacementRequest,
) -> Option<String> {
    let mut mismatches = Vec::new();
    if order.exchange != request.exchange {
        mismatches.push(format!(
            "exchange existing={} expected={}",
            order.exchange.as_str(),
            request.exchange.as_str()
        ));
    }
    push_instrument_mismatch(&mut mismatches, &order.instrument, &request.instrument);
    push_optional_text_mismatch(
        &mut mismatches,
        "client_order_id",
        order.client_order_id.as_deref(),
        request.client_order_id.as_deref(),
        false,
    );
    push_optional_text_mismatch(
        &mut mismatches,
        "side",
        order.side.as_deref(),
        Some(order_side_lower(request.side)),
        true,
    );
    push_optional_text_mismatch(
        &mut mismatches,
        "order_type",
        order.order_type.as_deref(),
        Some(take_profit_order_type_label(request.order_type)),
        true,
    );
    push_decimal_text_mismatch(
        &mut mismatches,
        "size",
        order.size.as_deref(),
        Some(request.size.as_str()),
    );
    push_decimal_text_mismatch(
        &mut mismatches,
        "price",
        order.price.as_deref(),
        request.price.as_deref(),
    );
    (!mismatches.is_empty()).then(|| {
        format!(
            "existing take-profit order terms mismatch: {}",
            mismatches.join(", ")
        )
    })
}
/// 提供take盈利止损reset订单termserror的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_stop_reset_order_terms_error(
    order_task: &ExecutionOrderTask,
    leg: &TakeProfitLeg,
    order: &Order,
    filters: &ExchangeOrderFilters,
    previous_raw_payload_json: Option<&str>,
) -> Result<Option<String>> {
    let mut mismatches = Vec::new();
    let expected_instrument = parse_instrument(&order_task.symbol)?;
    let expected_side = direction_from_order_task(order_task).protective_order_side();
    let expected_price =
        quantize_protective_stop_price(leg.price, direction_from_order_task(order_task), filters)
            .map(|price| format_protective_stop_price_decimal(price, filters))?;
    let expected_client_order_id = take_profit_order_client_id(order_task.task_id, leg.leg_index);
    if order.exchange != order_task.exchange {
        mismatches.push(format!(
            "exchange existing={} expected={}",
            order.exchange.as_str(),
            order_task.exchange.as_str()
        ));
    }
    push_instrument_mismatch(&mut mismatches, &order.instrument, &expected_instrument);
    push_optional_text_mismatch(
        &mut mismatches,
        "side",
        order.side.as_deref(),
        Some(order_side_lower(expected_side)),
        true,
    );
    push_optional_text_mismatch(
        &mut mismatches,
        "order_type",
        order.order_type.as_deref(),
        Some(take_profit_order_type_label(OrderType::Limit)),
        true,
    );
    push_decimal_text_mismatch(
        &mut mismatches,
        "price",
        order.price.as_deref(),
        Some(expected_price.as_str()),
    );
    let expected_size = match tracked_take_profit_order_size(
        previous_raw_payload_json,
        expected_client_order_id.as_str(),
    )? {
        Some(size) => Some(size),
        None => expected_take_profit_order_size_from_task(order_task, leg, filters)?,
    };
    if let Some(expected_size) = expected_size {
        push_decimal_text_mismatch(
            &mut mismatches,
            "size",
            order.size.as_deref(),
            Some(expected_size.as_str()),
        );
        push_decimal_text_mismatch(
            &mut mismatches,
            "filled_size",
            order.filled_size.as_deref(),
            Some(expected_size.as_str()),
        );
    }
    Ok((!mismatches.is_empty()).then(|| {
        format!(
            "take-profit order terms mismatch before stop reset: {}",
            mismatches.join(", ")
        )
    }))
}
/// 提供trackedtake盈利订单size的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn tracked_take_profit_order_size(
    previous_raw_payload_json: Option<&str>,
    expected_client_order_id: &str,
) -> Result<Option<String>> {
    let Some(previous_raw_payload_json) = previous_raw_payload_json else {
        return Ok(None);
    };
    let payload = serde_json::from_str::<Value>(previous_raw_payload_json)
        .map_err(|error| anyhow!("invalid previous take-profit raw payload: {error}"))?;
    let Some(orders) = payload
        .get("take_profit_sync")
        .and_then(|take_profit_sync| take_profit_sync.get("orders"))
        .and_then(Value::as_array)
    else {
        return Ok(None);
    };
    for order in orders {
        let client_order_id = order
            .get("client_order_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if client_order_id != Some(expected_client_order_id) {
            continue;
        }
        return json_decimal_text_field(order, "size", "tracked take-profit order size").and_then(
            |size| {
                size.ok_or_else(|| {
                    anyhow!(
                        "tracked take-profit order size is missing for {expected_client_order_id}"
                    )
                })
                .map(Some)
            },
        );
    }
    Ok(None)
}
/// 提供JSON小数text字段的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn json_decimal_text_field(payload: &Value, key: &str, label: &str) -> Result<Option<String>> {
    let Some(value) = payload.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let raw = match value {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.trim().to_string(),
        _ => return Err(anyhow!("{label} must be numeric")),
    };
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse::<Decimal>()
        .map_err(|error| anyhow!("invalid {label} {raw}: {error}"))?;
    Ok(Some(raw))
}
/// 提供expectedtake盈利订单sizefromtask的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn expected_take_profit_order_size_from_task(
    order_task: &ExecutionOrderTask,
    leg: &TakeProfitLeg,
    filters: &ExchangeOrderFilters,
) -> Result<Option<String>> {
    let raw_size = order_task.size.trim();
    if raw_size.is_empty() {
        return Ok(None);
    }
    let filled_qty = raw_size
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid task size before take-profit stop reset: {error}"))?;
    if !filled_qty.is_finite() || filled_qty <= 0.0 {
        return Ok(None);
    }
    let leg_size = decimal_from_f64(filled_qty * leg.fraction)?;
    let price =
        quantize_protective_stop_price(leg.price, direction_from_order_task(order_task), filters)?;
    let size = quantize_order_size(leg_size, price, filters, false)?;
    Ok(Some(format_order_size_decimal(size, filters)))
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_take_profit_order_request(
    order_task: &ExecutionOrderTask,
    leg: &TakeProfitLeg,
    filled_qty: f64,
    filters: &ExchangeOrderFilters,
) -> Result<OrderPlacementRequest> {
    let leg_size = decimal_from_f64(filled_qty * leg.fraction)?;
    let price =
        quantize_protective_stop_price(leg.price, direction_from_order_task(order_task), filters)?;
    let size = quantize_order_size(leg_size, price, filters, false)?;
    let side = direction_from_order_task(order_task).protective_order_side();
    Ok(OrderPlacementRequest {
        exchange: order_task.exchange,
        instrument: parse_instrument(&order_task.symbol)?,
        side,
        order_type: OrderType::Limit,
        size: format_order_size_decimal(size, filters),
        price: Some(format_protective_stop_price_decimal(price, filters)),
        margin_mode: order_task.margin_mode.clone(),
        margin_coin: order_task.margin_coin.clone(),
        position_side: order_task.position_side.clone(),
        trade_side: Some("close".to_string()),
        client_order_id: Some(take_profit_order_client_id(
            order_task.task_id,
            leg.leg_index,
        )),
        reduce_only: take_profit_reduce_only(
            order_task.exchange,
            order_task.position_side.as_deref(),
        ),
        time_in_force: Some(TimeInForce::Gtc),
        attached_stop_loss_price: None,
    })
}
/// 停止 Web 商业、会员和执行准备度 后台流程，确保退出时不留下未释放状态。
fn stop_reset_monitor_legs(order_task: &ExecutionOrderTask) -> Vec<Value> {
    order_task
        .take_profit_legs
        .iter()
        .filter_map(|leg| {
            let stop_after_fill_price = leg.stop_after_fill_price?;
            Some(json!({
                "leg_index": leg.leg_index,
                "role": leg.role,
                "take_profit_client_order_id": take_profit_order_client_id(
                    order_task.task_id,
                    leg.leg_index,
                ),
                "target_r": leg.target_r,
                "take_profit_price": leg.price,
                "stop_after_fill_r": leg.stop_after_fill_r,
                "stop_after_fill_price": stop_after_fill_price,
            }))
        })
        .collect()
}
/// 提供take盈利legs值的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_legs_value(payload: &Value) -> Option<&Value> {
    payload
        .get("risk_plan")
        .and_then(|risk_plan| {
            risk_plan.get("take_profit_legs").or_else(|| {
                risk_plan
                    .get("take_profit_plan")
                    .and_then(|plan| plan.get("legs"))
            })
        })
        .or_else(|| payload.get("take_profit_legs"))
        .or_else(|| {
            payload
                .get("execution")
                .and_then(|execution| execution.get("take_profit_legs"))
        })
}
/// 提供take盈利价格from目标r的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_price_from_target_r(
    direction: ProtectiveDirection,
    entry_price: f64,
    stop_loss_price: f64,
    target_r: f64,
) -> Option<f64> {
    if !entry_price.is_finite()
        || !stop_loss_price.is_finite()
        || !target_r.is_finite()
        || target_r <= 0.0
    {
        return None;
    }
    match direction {
        ProtectiveDirection::Long if stop_loss_price < entry_price => {
            Some(entry_price + (entry_price - stop_loss_price) * target_r)
        }
        ProtectiveDirection::Short if stop_loss_price > entry_price => {
            Some(entry_price - (stop_loss_price - entry_price) * target_r)
        }
        _ => None,
    }
}
/// 停止 Web 商业、会员和执行准备度 后台流程，确保退出时不留下未释放状态。
fn stop_after_fill_price_from_r(
    direction: ProtectiveDirection,
    entry_price: f64,
    stop_loss_price: f64,
    stop_after_fill_r: f64,
    take_profit_price: f64,
    offset: usize,
) -> Result<f64> {
    if !entry_price.is_finite()
        || entry_price <= 0.0
        || !stop_loss_price.is_finite()
        || stop_loss_price <= 0.0
        || !stop_after_fill_r.is_finite()
        || stop_after_fill_r < 0.0
    {
        return Err(anyhow!(
            "take_profit_legs[{offset}].stop_after_fill_r must be a finite non-negative R value"
        ));
    }
    let price = match direction {
        ProtectiveDirection::Long if stop_loss_price < entry_price => {
            entry_price + (entry_price - stop_loss_price) * stop_after_fill_r
        }
        ProtectiveDirection::Short if stop_loss_price > entry_price => {
            entry_price - (stop_loss_price - entry_price) * stop_after_fill_r
        }
        _ => {
            return Err(anyhow!(
                "take_profit_legs[{offset}].stop_after_fill_r is invalid for entry/stop direction"
            ));
        }
    };
    let valid = price.is_finite()
        && price > 0.0
        && match direction {
            ProtectiveDirection::Long => price < take_profit_price,
            ProtectiveDirection::Short => price > take_profit_price,
        };
    if !valid {
        return Err(anyhow!(
            "take_profit_legs[{offset}].stop_after_fill_r would place stop beyond take-profit price"
        ));
    }
    Ok(price)
}
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
fn validate_take_profit_price(
    direction: ProtectiveDirection,
    entry_price: Option<f64>,
    price: f64,
    offset: usize,
) -> Result<()> {
    if !price.is_finite() || price <= 0.0 {
        return Err(anyhow!(
            "take_profit_legs[{offset}].price must be a positive finite number"
        ));
    }
    let Some(entry_price) = entry_price else {
        return Ok(());
    };
    let invalid = match direction {
        ProtectiveDirection::Long => price <= entry_price,
        ProtectiveDirection::Short => price >= entry_price,
    };
    if invalid {
        return Err(anyhow!(
            "take_profit_legs[{offset}].price is invalid for entry direction"
        ));
    }
    Ok(())
}
/// 提供directionfrom订单task的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn direction_from_order_task(order_task: &ExecutionOrderTask) -> ProtectiveDirection {
    match order_task.side {
        OrderSide::Buy => ProtectiveDirection::Long,
        OrderSide::Sell => ProtectiveDirection::Short,
    }
}
/// 提供take盈利reduceonly的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_reduce_only(exchange: ExchangeId, position_side: Option<&str>) -> Option<bool> {
    if exchange == ExchangeId::Okx && position_side.is_some() {
        None
    } else {
        Some(true)
    }
}
fn take_profit_order_client_id(task_id: i64, leg_index: usize) -> String {
    format!("rq-tp-{task_id}-{leg_index}")
}
fn take_profit_stop_reset_order_client_id(task_id: i64, leg_index: usize) -> String {
    format!("rq-sl-{task_id}-tp{leg_index}")
}
/// 提供take盈利订单type标签的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_order_type_label(order_type: OrderType) -> &'static str {
    match order_type {
        OrderType::Limit => "limit",
        OrderType::Market => "market",
    }
}
/// 把数据加入 Web 商业、会员和执行准备度 聚合结果，保持集合构造逻辑集中。
fn push_optional_text_mismatch(
    mismatches: &mut Vec<String>,
    label: &str,
    actual: Option<&str>,
    expected: Option<&str>,
    case_insensitive: bool,
) {
    let expected = expected.map(str::trim).filter(|value| !value.is_empty());
    let actual = actual.map(str::trim).filter(|value| !value.is_empty());
    let matches = match (actual, expected) {
        (Some(actual), Some(expected)) if case_insensitive => actual.eq_ignore_ascii_case(expected),
        (Some(actual), Some(expected)) => actual == expected,
        (None, None) => true,
        _ => false,
    };
    if !matches {
        mismatches.push(format!(
            "{label} existing={} expected={}",
            actual.unwrap_or("<missing>"),
            expected.unwrap_or("<missing>")
        ));
    }
}
/// 把数据加入 Web 商业、会员和执行准备度 聚合结果，保持集合构造逻辑集中。
fn push_instrument_mismatch(
    mismatches: &mut Vec<String>,
    actual: &crypto_exc_all::Instrument,
    expected: &crypto_exc_all::Instrument,
) {
    let settlement_matches = match (actual.settlement.as_deref(), expected.settlement.as_deref()) {
        (Some(actual), Some(expected)) => actual.eq_ignore_ascii_case(expected),
        _ => true,
    };
    if actual.base.eq_ignore_ascii_case(&expected.base)
        && actual.quote.eq_ignore_ascii_case(&expected.quote)
        && actual.market_type == expected.market_type
        && settlement_matches
    {
        return;
    }
    mismatches.push(format!(
        "instrument existing={actual:?} expected={expected:?}"
    ));
}
/// 把数据加入 Web 商业、会员和执行准备度 聚合结果，保持集合构造逻辑集中。
fn push_decimal_text_mismatch(
    mismatches: &mut Vec<String>,
    label: &str,
    actual: Option<&str>,
    expected: Option<&str>,
) {
    let Some(expected) = expected.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let Some(actual) = actual.map(str::trim).filter(|value| !value.is_empty()) else {
        mismatches.push(format!("{label} existing=<missing> expected={expected}"));
        return;
    };
    let actual_decimal = actual.parse::<Decimal>();
    let expected_decimal = expected.parse::<Decimal>();
    if actual_decimal.is_err()
        || expected_decimal.is_err()
        || actual_decimal.ok() != expected_decimal.ok()
    {
        mismatches.push(format!("{label} existing={actual} expected={expected}"));
    }
}
/// 提供take盈利订单filledsizeerror的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_order_filled_size_error(order: &Order) -> Option<String> {
    let Some(filled_size) = order
        .filled_size
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Some("take-profit order filled_size is missing before stop reset".to_string());
    };
    match filled_size.parse::<Decimal>() {
        Ok(value) if value > Decimal::ZERO => None,
        _ => Some(format!(
            "take-profit order filled_size must be positive before stop reset: filled_size={filled_size}"
        )),
    }
}
/// 提供take盈利订单statuserror的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_order_status_error(status: Option<&str>, source: &str) -> Option<String> {
    let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) else {
        return Some(format!("{source} status is missing"));
    };
    match status.to_ascii_uppercase().as_str() {
        "0" | "NEW" | "OPEN" | "LIVE" | "WORKING" | "ACCEPTED" | "PARTIALLY_FILLED"
        | "PARTIAL_FILLED" | "FILLED" => None,
        "DRY_RUN" => Some(format!(
            "{source} is not a live exchange acknowledgement: status={status}"
        )),
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" | "FAILED" | "FAILURE" => {
            Some(format!("{source} is not active: status={status}"))
        }
        _ => Some(format!(
            "{source} status is not recognized: status={status}"
        )),
    }
}
/// 提供take盈利订单isfilled的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_order_is_filled(order: &Order) -> bool {
    order
        .status
        .as_deref()
        .map(str::trim)
        .is_some_and(|status| status.eq_ignore_ascii_case("FILLED"))
}
/// 提供existingtake盈利订单的集中实现，避免Web 商业链路调用方重复处理相同细节。
async fn existing_take_profit_order(
    gateway: &CryptoExcAllGateway,
    request: &OrderPlacementRequest,
) -> Result<Option<crypto_exc_all::Order>> {
    let Some(client_order_id) = request.client_order_id.as_deref() else {
        return Ok(None);
    };
    let mut query =
        OrderQuery::by_client_order_id(request.instrument.clone(), client_order_id.to_string());
    if let Some(margin_coin) = request.margin_coin.as_deref() {
        query = query.with_margin_coin(margin_coin.to_string());
    }
    match CryptoExcAllGateway::with_signed_read_only_scope(gateway.order(request.exchange, query))
        .await
    {
        Ok(order) => Ok(Some(order)),
        Err(error) if is_order_not_found(&error.to_string()) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
fn is_order_not_found(error_message: &str) -> bool {
    let normalized = error_message.to_ascii_lowercase();
    normalized.contains("-2013")
        || normalized.contains("order does not exist")
        || normalized.contains("order not found")
        || normalized.contains("not found")
        || normalized.contains("not exist")
}
/// 提供报告raw载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn report_raw_payload(report: &ExecutionTaskReportRequest) -> Value {
    report
        .raw_payload_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}))
}
/// 停止 Web 商业、会员和执行准备度 后台流程，确保退出时不留下未释放状态。
fn stop_reset_old_cancel_failed(reset: &Value) -> bool {
    ["old_protective_cancel_failed", "manual_cleanup_required"]
        .iter()
        .any(|key| reset.get(*key).and_then(Value::as_bool).unwrap_or(false))
}
