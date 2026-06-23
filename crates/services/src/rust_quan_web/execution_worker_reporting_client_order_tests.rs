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
        attached_stop_loss_price: None,
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
        attached_stop_loss_price: None,
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
        attached_stop_loss_price: None,
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
