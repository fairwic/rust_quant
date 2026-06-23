use super::execution_protection::ProtectiveDirection;
use super::execution_worker::ExecutionOrderTask;
use crate::rust_quan_web::{
    worker_live_capability_for_exchange, ExecutionTask, LiveWorkerCapabilityStatus,
};
use anyhow::{anyhow, Result};
use crypto_exc_all::{ExchangeId, Instrument, OrderSide, OrderType, PositionMode, TimeInForce};
use serde_json::{json, Value};
use std::str::FromStr;
use tracing::warn;
const LIVE_ORDER_CONFIRM_ENV: &str = "EXECUTION_WORKER_LIVE_ORDER_CONFIRM";
const LIVE_ORDER_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LIVE_ORDERS";
#[derive(Debug, Clone)]
pub(super) struct RiskContractViolation {
    /// 提示信息。
    pub(super) message: String,
    /// raw载荷，用于风控判断或风险展示。
    pub(super) raw_payload: Value,
}
/// 提供订单载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn order_payload(payload: &Value) -> Value {
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
/// 构造载荷字符串，集中维护Web 商业链路的载荷组装规则。
pub(super) fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        Value::String(raw) => Some(raw.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}
/// 构造载荷f64，集中维护Web 商业链路的载荷组装规则。
pub(super) fn payload_f64(payload: &Value, key: &str) -> Option<f64> {
    payload.get(key).and_then(|value| match value {
        Value::Number(raw) => raw.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    })
}
/// 封装嵌套载荷f64，减少Web 商业链路调用方重复实现相同细节。
pub(super) fn nested_payload_f64(payload: &Value, parent: &str, key: &str) -> Option<f64> {
    payload
        .get(parent)
        .and_then(|parent| payload_f64(parent, key))
}
/// 生成 Web 商业、会员和执行准备度 需要的派生数据，供后续执行、展示或审计使用。
pub(super) fn format_order_size(value: f64) -> String {
    let formatted = format!("{value:.8}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
/// 生成 Web 商业、会员和执行准备度 需要的派生数据，供后续执行、展示或审计使用。
pub(super) fn format_order_price(value: f64) -> String {
    let formatted = format!("{value:.8}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
pub(super) fn is_zero_order_size(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .map(|raw| raw == 0.0)
        .unwrap_or(false)
}
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
pub(super) fn is_pending_close_task(task: &ExecutionTask) -> bool {
    task.task_type == "risk_control_close_candidate"
        && matches!(task.task_status.as_str(), "pending_close" | "leased")
}
/// 构造载荷bool，集中维护Web 商业链路的载荷组装规则。
pub(super) fn payload_bool(payload: &Value, key: &str) -> Option<bool> {
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
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
pub(super) fn validate_execute_signal_risk_contract(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
    live_stop_loss_required: bool,
) -> std::result::Result<(), RiskContractViolation> {
    let payload = order_payload(&task.request_payload_json);
    if !live_stop_loss_required
        && !protective_stop_loss_required(&payload, task.news_signal_id.is_some())
    {
        return Ok(());
    }
    if live_stop_loss_required
        && payload_string(&payload, "side")
            .or_else(|| payload_string(&payload, "signal_type"))
            .is_none()
    {
        return Err(risk_contract_violation(
            task,
            order_task,
            "missing_order_side",
            "execution order side is required before live order; set execution.side or signal_type",
            json!({
                "blocker_code": "missing_order_side",
                "missing_field": "execution.side",
            }),
        ));
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
    let entry_price_raw = protection_entry_price(&payload);
    let Some(entry_price) = entry_price_raw.filter(|price| price.is_finite() && *price > 0.0)
    else {
        return Err(risk_contract_violation(
            task,
            order_task,
            "missing_entry_price",
            "protective stop-loss required but risk_plan.entry_price is missing or invalid",
            json!({
                "missing_field": "risk_plan.entry_price",
                "entry_price": entry_price_raw,
                "selected_stop_loss_price": selected_stop_loss_price,
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
    if take_profit_stop_reset_required(&payload) {
        let capability = worker_live_capability_for_exchange(order_task.exchange.as_str());
        if capability.protective_order_cancel != LiveWorkerCapabilityStatus::MutatingSupported {
            return Err(risk_contract_violation(
                task,
                order_task,
                "unsupported_take_profit_stop_reset",
                format!(
                    "take-profit stop reset requires protective order cancellation support for {}, current status={:?}",
                    order_task.exchange.as_str(),
                    capability.protective_order_cancel
                ),
                json!({
                    "unsupported_feature": "take_profit_stop_reset",
                    "protective_order_cancel": format!("{:?}", capability.protective_order_cancel),
                }),
            ));
        }
    }
    Ok(())
}
/// 提供take盈利止损resetrequired的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn take_profit_stop_reset_required(payload: &Value) -> bool {
    take_profit_legs_value(payload)
        .and_then(Value::as_array)
        .is_some_and(|legs| {
            legs.iter()
                .any(|leg| payload_f64(leg, "stop_after_fill_r").is_some())
        })
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
/// 提供protective止损亏损required的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn protective_stop_loss_required(
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
/// 提供news信号requiresprotective止损亏损的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn news_signal_requires_protective_stop_loss(payload: &Value) -> bool {
    let Some(source_signal_type) = payload_string(payload, "source_signal_type") else {
        return false;
    };
    let normalized = source_signal_type.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "news_event" | "news")
}
/// 提供selected止损亏损价格的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn selected_stop_loss_price(payload: &Value) -> Option<f64> {
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
/// 提供保护入场价格的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn protection_entry_price(payload: &Value) -> Option<f64> {
    payload
        .get("risk_plan")
        .and_then(|value| payload_f64(value, "entry_price"))
        .or_else(|| payload_f64(payload, "entry_price"))
        .or_else(|| nested_payload_f64(payload, "signal", "open_price"))
        .or_else(|| payload_f64(payload, "open_price"))
        .or_else(|| payload_f64(payload, "price"))
}
/// 提供风控计划directionraw的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn risk_plan_direction_raw(payload: &Value) -> Option<String> {
    payload
        .get("risk_plan")
        .and_then(|value| payload_string(value, "direction"))
        .or_else(|| payload_string(payload, "direction"))
        .or_else(|| payload_string(payload, "position_side"))
        .or_else(|| payload_string(payload, "side"))
        .or_else(|| payload_string(payload, "signal_type"))
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_protective_direction(raw: &str) -> Result<ProtectiveDirection> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "buy" | "long" | "open_long" => Ok(ProtectiveDirection::Long),
        "sell" | "short" | "open_short" => Ok(ProtectiveDirection::Short),
        other => Err(anyhow!(
            "unsupported protective stop-loss direction: {}",
            other
        )),
    }
}
/// 提供directionfrom订单side的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn direction_from_order_side(side: OrderSide) -> ProtectiveDirection {
    match side {
        OrderSide::Buy => ProtectiveDirection::Long,
        OrderSide::Sell => ProtectiveDirection::Short,
    }
}
/// 提供风控contractviolation的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 停止 Web 商业、会员和执行准备度 后台流程，确保退出时不留下未释放状态。
pub(super) fn close_order_side(payload: &Value) -> Result<OrderSide> {
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
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_env_list(key: &str, defaults: &[&str]) -> Vec<String> {
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
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_env_i64_list(key: &str) -> Vec<i64> {
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
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}
/// 封装实盘orderconfirmationvalid，减少Web 商业链路调用方重复实现相同细节。
pub(super) fn live_order_confirmation_valid(dry_run: bool, confirmation: Option<&str>) -> bool {
    dry_run
        || confirmation
            .map(str::trim)
            .is_some_and(|value| value == LIVE_ORDER_CONFIRM_TOKEN)
}
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
pub(super) fn ensure_live_order_confirmation() -> Result<()> {
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
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(crate) fn parse_exchange(raw: &str) -> Result<ExchangeId> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "币安" => Ok(ExchangeId::Binance),
        other => ExchangeId::from_str(other).map_err(anyhow::Error::msg),
    }
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_side(raw: &str) -> Result<OrderSide> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "buy" | "long" | "open_long" => Ok(OrderSide::Buy),
        "sell" | "short" | "open_short" => Ok(OrderSide::Sell),
        other => Err(anyhow!("unsupported order side: {}", other)),
    }
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_order_type(raw: &str) -> Result<OrderType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "market" => Ok(OrderType::Market),
        "limit" => Ok(OrderType::Limit),
        other => Err(anyhow!("unsupported order type: {}", other)),
    }
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_time_in_force(raw: &str) -> Result<TimeInForce> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "gtc" => Ok(TimeInForce::Gtc),
        "ioc" => Ok(TimeInForce::Ioc),
        "fok" => Ok(TimeInForce::Fok),
        "post_only" | "postonly" => Ok(TimeInForce::PostOnly),
        other => Err(anyhow!("unsupported time_in_force: {}", other)),
    }
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_position_mode(raw: &str) -> Result<PositionMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "hedge" => Ok(PositionMode::Hedge),
        "one_way" | "oneway" | "net" => Ok(PositionMode::OneWay),
        other => Err(anyhow!("unsupported position_mode: {}", other)),
    }
}
/// 判断 Web 商业、会员和执行准备度 条件是否满足，给上层流程提供布尔决策。
pub(super) fn is_duplicate_client_order_id_error(error_message: &str) -> bool {
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
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(crate) fn parse_instrument(symbol: &str) -> Result<Instrument> {
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
/// 提供订单sidelower的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn order_side_lower(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}
