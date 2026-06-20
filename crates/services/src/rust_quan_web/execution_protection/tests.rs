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
        contract_value: None,
        contract_value_currency: None,
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

fn attached_stop_loss_order_task(exchange: ExchangeId) -> ExecutionOrderTask {
    ExecutionOrderTask {
        task_id: 43,
        exchange,
        symbol: "ETH-USDT-SWAP".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        size: "0.01".to_string(),
        price: None,
        margin_mode: None,
        leverage: None,
        position_mode: None,
        margin_coin: Some("USDT".to_string()),
        position_side: None,
        trade_side: None,
        client_order_id: Some("rqtask43".to_string()),
        reduce_only: None,
        time_in_force: None,
        size_usdt: None,
        attached_stop_loss_price: Some("2100".to_string()),
    }
}

fn attached_stop_loss_ack(exchange: ExchangeId, raw: Value) -> OrderAck {
    let instrument = Instrument::perp("ETH", "USDT");
    OrderAck {
        exchange,
        exchange_symbol: instrument.symbol_for(exchange),
        instrument,
        order_id: Some("10043".to_string()),
        client_order_id: Some("rqtask43".to_string()),
        status: Some("FILLED".to_string()),
        raw,
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
fn independent_stop_market_exchanges_prearm_protection_before_main_fill() {
    for exchange in [ExchangeId::Binance, ExchangeId::Bybit, ExchangeId::Gate] {
        assert!(
            exchange_uses_prearmed_protective_order(exchange),
            "{exchange:?} must prearm protection before the main order"
        );
    }
    for exchange in [ExchangeId::Okx, ExchangeId::Bitget] {
        assert!(
            !exchange_uses_prearmed_protective_order(exchange),
            "{exchange:?} uses attached stop-loss evidence on the main order"
        );
    }
}

#[test]
fn protection_sync_raw_payload_declares_v2_exchange_and_order_mode() {
    for (exchange, expected_mode) in [
        ("binance", "independent_stop_market"),
        ("okx", "attached_stop_loss"),
        ("bitget", "attached_stop_loss"),
    ] {
        let protection = long_protection();
        let mut report = ExecutionTaskReportRequest::success(
            43,
            exchange,
            "main-43",
            "buy",
            "FILLED",
            json!({"execution_status": "pending_protection_sync"}),
        );
        report.execution_status = "pending_protection_sync".to_string();

        protection.apply_outcome_to_report(
            &mut report,
            ProtectionSyncOutcome::confirmed("protective-43", "query_protective_order"),
        );

        let raw: Value = serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(raw["protection_sync"]["contract_version"], "v2");
        assert_eq!(raw["protection_sync"]["exchange"], exchange);
        assert_eq!(
            raw["protection_sync"]["protective_order_mode"],
            expected_mode
        );
        assert_eq!(raw["protection_sync"]["place_order_allowed"], false);
        assert_eq!(raw["protection_sync"]["repeat_open_order_allowed"], false);
    }
}

#[test]
fn okx_generic_algo_id_without_stop_loss_fields_does_not_confirm_attached_stop_loss() {
    let order_task = attached_stop_loss_order_task(ExchangeId::Okx);
    let ack = attached_stop_loss_ack(ExchangeId::Okx, json!({"ordId": "10043", "algoId": "1"}));

    let outcome = attached_stop_loss_order_ack_outcome(&order_task, &ack, None)
        .expect("OKX attached stop-loss should produce an outcome");

    match outcome {
        ProtectionSyncOutcome::Failed { reason, .. } => {
            assert_eq!(reason, "attached_stop_loss_ack_missing");
        }
        other => panic!("expected missing OKX stop-loss evidence failure, got {other:?}"),
    }
}

#[test]
fn okx_stop_loss_fields_confirm_attached_stop_loss() {
    let order_task = attached_stop_loss_order_task(ExchangeId::Okx);
    let ack = attached_stop_loss_ack(
        ExchangeId::Okx,
        json!({
            "ordId": "10043",
            "attachAlgoOrds": [{
                "attachAlgoId": "sl-10043",
                "slTriggerPx": "2100",
                "slOrdPx": "-1",
                "slTriggerPxType": "last"
            }]
        }),
    );

    let outcome = attached_stop_loss_order_ack_outcome(&order_task, &ack, None)
        .expect("OKX attached stop-loss should produce an outcome");

    match outcome {
        ProtectionSyncOutcome::Confirmed {
            protective_order_external_id,
            source,
        } => {
            assert_eq!(protective_order_external_id, "sl-10043");
            assert_eq!(source, "place_order_attached_stop_loss_ack");
        }
        other => panic!("expected OKX attached stop-loss confirmation, got {other:?}"),
    }
}

#[test]
fn bitget_preset_stop_loss_price_confirms_attached_stop_loss() {
    let order_task = attached_stop_loss_order_task(ExchangeId::Bitget);
    let ack = attached_stop_loss_ack(
        ExchangeId::Bitget,
        json!({"orderId": "10043", "presetStopLossPrice": "2100"}),
    );

    let outcome = attached_stop_loss_order_ack_outcome(&order_task, &ack, None)
        .expect("Bitget attached stop-loss should produce an outcome");

    match outcome {
        ProtectionSyncOutcome::Confirmed { source, .. } => {
            assert_eq!(source, "place_order_attached_stop_loss_ack");
        }
        other => panic!("expected Bitget attached stop-loss confirmation, got {other:?}"),
    }
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

    let raw: Value = serde_json::from_str(pending.raw_payload_json.as_deref().unwrap()).unwrap();
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
