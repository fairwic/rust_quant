use anyhow::{anyhow, Result};
use crypto_exc_all::{ExchangeId, OrderSide, OrderType};
use serde_json::{json, Value};

use super::execution_payload::{format_order_size, parse_instrument};
use super::execution_worker::ExecutionOrderTask;
use crate::exchange::OrderPlacementRequest;
use crate::rust_quan_web::ExecutionTaskReportRequest;

pub(super) fn build_protective_failure_rollback_order_request(
    order_task: &ExecutionOrderTask,
    report: &ExecutionTaskReportRequest,
) -> Result<Option<OrderPlacementRequest>> {
    if !report.order_status.trim().eq_ignore_ascii_case("FILLED") {
        return Ok(None);
    }
    let filled_qty = report
        .filled_qty
        .filter(|qty| qty.is_finite() && *qty > 0.0)
        .ok_or_else(|| anyhow!("filled order rollback requires positive filled_qty"))?;

    let position_side = order_task.position_side.clone();
    let reduce_only = match (order_task.exchange, position_side.as_deref()) {
        (ExchangeId::Okx, _) => None,
        (ExchangeId::Binance, Some(_)) => None,
        _ => Some(true),
    };

    Ok(Some(OrderPlacementRequest {
        exchange: order_task.exchange,
        instrument: parse_instrument(&order_task.symbol)?,
        side: opposite_order_side(order_task.side),
        order_type: OrderType::Market,
        size: format_order_size(filled_qty),
        price: None,
        margin_mode: order_task.margin_mode.clone(),
        margin_coin: order_task.margin_coin.clone(),
        position_side,
        trade_side: Some("close".to_string()),
        client_order_id: Some(format!("rqrollback{}", order_task.task_id)),
        reduce_only,
        time_in_force: None,
        attached_stop_loss_price: None,
    }))
}

pub(super) fn apply_protective_failure_rollback_report(
    report: &mut ExecutionTaskReportRequest,
    rollback_report: &ExecutionTaskReportRequest,
) {
    let mut raw_payload = report_raw_payload(report);
    raw_payload["protective_failure_rollback"] = json!({
        "status": "rollback_completed",
        "exchange": rollback_report.exchange,
        "external_order_id": rollback_report.external_order_id,
        "order_side": rollback_report.order_side,
        "order_status": rollback_report.order_status,
        "execution_status": rollback_report.execution_status,
        "filled_qty": rollback_report.filled_qty,
        "filled_quote": rollback_report.filled_quote,
        "fee_amount": rollback_report.fee_amount,
        "raw_payload_json": rollback_report
            .raw_payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
        "place_order_allowed": false,
        "repeat_open_order_allowed": false,
    });
    raw_payload["execution_status"] = json!(report.execution_status);
    report.raw_payload_json = Some(raw_payload.to_string());
}

pub(super) fn apply_protective_failure_rollback_error(
    report: &mut ExecutionTaskReportRequest,
    reason: impl Into<String>,
    error_message: impl Into<String>,
) {
    let reason = reason.into();
    let error_message = error_message.into();
    let mut raw_payload = report_raw_payload(report);
    raw_payload["protective_failure_rollback"] = json!({
        "status": "rollback_failed",
        "reason": reason,
        "error_message": error_message,
        "place_order_allowed": false,
        "repeat_open_order_allowed": false,
    });
    raw_payload["execution_status"] = json!(report.execution_status);
    report.error_message = Some(format!(
        "{}; protective failure rollback failed: {}",
        report
            .error_message
            .as_deref()
            .unwrap_or("protective order failed"),
        error_message
    ));
    report.raw_payload_json = Some(raw_payload.to_string());
}

fn opposite_order_side(side: OrderSide) -> OrderSide {
    match side {
        OrderSide::Buy => OrderSide::Sell,
        OrderSide::Sell => OrderSide::Buy,
    }
}

fn report_raw_payload(report: &ExecutionTaskReportRequest) -> Value {
    report
        .raw_payload_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}))
}

#[cfg(test)]
mod tests {
    use crypto_exc_all::{ExchangeId, MarginMode, OrderSide, OrderType};

    use super::{
        apply_protective_failure_rollback_report, build_protective_failure_rollback_order_request,
    };
    use crate::rust_quan_web::{ExecutionOrderTask, ExecutionTaskReportRequest};

    #[test]
    fn rollback_request_closes_binance_hedge_long_after_protection_failure() {
        let order_task = ExecutionOrderTask {
            task_id: 42,
            exchange: ExchangeId::Binance,
            symbol: "ETH-USDT-SWAP".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            size: "0.011".to_string(),
            price: None,
            margin_mode: Some(MarginMode::Cross),
            leverage: None,
            position_mode: None,
            margin_coin: Some("USDT".to_string()),
            position_side: Some("long".to_string()),
            trade_side: Some("open".to_string()),
            client_order_id: Some("rqtask42".to_string()),
            reduce_only: None,
            time_in_force: None,
            size_usdt: None,
            attached_stop_loss_price: None,
        };
        let mut report = ExecutionTaskReportRequest::success(
            42,
            "binance",
            "12345",
            "buy",
            "FILLED",
            serde_json::json!({"execution_status":"protective_order_failed"}),
        );
        report.execution_status = "protective_order_failed".to_string();
        report.filled_qty = Some(0.011);
        report.filled_quote = Some(22.0);

        let request = build_protective_failure_rollback_order_request(&order_task, &report)
            .expect("rollback request should build")
            .expect("filled protected failure should require rollback");

        assert_eq!(request.exchange, ExchangeId::Binance);
        assert_eq!(
            request.instrument.symbol_for(ExchangeId::Binance),
            "ETHUSDT"
        );
        assert_eq!(request.side, OrderSide::Sell);
        assert_eq!(request.order_type, OrderType::Market);
        assert_eq!(request.size, "0.011");
        assert_eq!(request.position_side.as_deref(), Some("long"));
        assert_eq!(request.trade_side.as_deref(), Some("close"));
        assert_eq!(request.client_order_id.as_deref(), Some("rqrollback42"));
        assert_eq!(request.reduce_only, None);
        assert_eq!(request.attached_stop_loss_price, None);
    }

    #[test]
    fn rollback_report_preserves_protective_failure_status_and_records_close_evidence() {
        let mut report = ExecutionTaskReportRequest::success(
            42,
            "binance",
            "12345",
            "buy",
            "FILLED",
            serde_json::json!({
                "execution_status":"protective_order_failed",
                "protection_sync":{"status":"protective_order_failed"}
            }),
        );
        report.execution_status = "protective_order_failed".to_string();
        report.error_message = Some("STOP_MARKET rejected".to_string());

        let rollback_report = ExecutionTaskReportRequest::success(
            42,
            "binance",
            "67890",
            "sell",
            "FILLED",
            serde_json::json!({"order_detail":{"reduceOnly":false}}),
        );
        apply_protective_failure_rollback_report(&mut report, &rollback_report);

        let raw = serde_json::from_str::<serde_json::Value>(
            report.raw_payload_json.as_deref().expect("raw payload"),
        )
        .expect("valid raw json");

        assert_eq!(report.execution_status, "protective_order_failed");
        assert_eq!(
            raw["protective_failure_rollback"]["status"],
            "rollback_completed"
        );
        assert_eq!(
            raw["protective_failure_rollback"]["external_order_id"],
            "67890"
        );
        assert_eq!(raw["protective_failure_rollback"]["order_side"], "sell");
        assert_eq!(
            raw["protective_failure_rollback"]["place_order_allowed"],
            false
        );
        assert_eq!(raw["execution_status"], "protective_order_failed");
    }
}
