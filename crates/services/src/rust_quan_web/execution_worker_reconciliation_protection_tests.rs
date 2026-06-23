#[test]
fn filled_open_long_builds_binance_protective_stop_market_sell_request() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "client_order_id": "rq-open-42",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "entry_price": 2400.0,
            "selected_stop_loss_price": 2200.0,
            "direction": "long"
        }
    }));
    let order_task = ExecutionOrderTask::from_task(&task).unwrap();
    let protection = ProtectionSyncContract::from_task(&task, "buy").unwrap();
    let request = build_protective_stop_market_order_request(
        &order_task,
        &protection,
        &binance_eth_filters(),
    )
    .unwrap();
    assert_eq!(
        request.instrument.symbol_for(order_task.exchange),
        "ETHUSDT"
    );
    assert_eq!(request.side, OrderSide::Sell);
    assert_eq!(request.stop_price, "2200");
    assert_eq!(request.quantity, None);
    assert_eq!(request.close_position, Some(true));
    assert_eq!(request.price_protect, Some(true));
    assert_eq!(
        request.working_type,
        Some(ProtectiveOrderWorkingType::MarkPrice)
    );
    assert_eq!(request.client_order_id.as_deref(), Some("rq-sl-42"));
}
#[test]
fn technical_strategy_selected_stop_loss_requires_protection_without_flag() {
    let task = task(json!({
        "source": "rust_quant",
        "source_signal_type": "technical_strategy",
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.024",
        "risk_plan": {
            "selected_stop_loss_price": 2134.82,
            "direction": "long"
        }
    }));
    let payload = order_payload(&task.request_payload_json);
    assert!(protective_stop_loss_required(&payload, false));
}
#[test]
fn filled_open_short_builds_binance_protective_stop_market_buy_request() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "sell",
        "size": "0.01",
        "position_side": "short",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "entry_price": 2400.0,
            "selected_stop_loss_price": 2600.0,
            "direction": "short"
        }
    }));
    let order_task = ExecutionOrderTask::from_task(&task).unwrap();
    let protection = ProtectionSyncContract::from_task(&task, "sell").unwrap();
    let request = build_protective_stop_market_order_request(
        &order_task,
        &protection,
        &binance_eth_filters(),
    )
    .unwrap();
    assert_eq!(request.side, OrderSide::Buy);
    assert_eq!(request.stop_price, "2600");
    assert_eq!(request.position_side.as_deref(), Some("short"));
    assert_eq!(request.quantity, None);
    assert_eq!(request.close_position, Some(true));
    assert_eq!(request.client_order_id.as_deref(), Some("rq-sl-42"));
}
#[test]
fn protective_stop_price_is_quantized_to_exchange_tick_size() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.011",
        "position_side": "long",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "entry_price": 2300.0,
            "selected_stop_loss_price": 2254.3724,
            "direction": "long"
        }
    }));
    let order_task = ExecutionOrderTask::from_task(&task).unwrap();
    let protection = ProtectionSyncContract::from_task(&task, "buy").unwrap();
    let request = build_protective_stop_market_order_request(
        &order_task,
        &protection,
        &binance_eth_filters(),
    )
    .unwrap();
    assert_eq!(request.stop_price, "2254.37");
}
#[test]
fn short_protective_stop_price_rounds_up_to_exchange_tick_size() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "sell",
        "size": "0.011",
        "position_side": "short",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "entry_price": 2200.0,
            "selected_stop_loss_price": 2254.3724,
            "direction": "short"
        }
    }));
    let order_task = ExecutionOrderTask::from_task(&task).unwrap();
    let protection = ProtectionSyncContract::from_task(&task, "sell").unwrap();
    let request = build_protective_stop_market_order_request(
        &order_task,
        &protection,
        &binance_eth_filters(),
    )
    .unwrap();
    assert_eq!(request.stop_price, "2254.38");
}
#[test]
fn filled_long_with_stale_strategy_reference_rebases_protective_stop_below_fill_price() {
    let protection = ProtectionSyncContract::required(
        json!({
            "risk_plan": {
                "protective_stop_loss_required": true,
                "entry_price": 2300.38,
                "selected_stop_loss_price": 2254.3724,
                "direction": "long"
            }
        }),
        "buy",
    )
    .expect("valid protection contract");
    let mut report = ExecutionTaskReportRequest::success(
        181,
        "binance",
        "8389766181415858769",
        "buy",
        "FILLED",
        json!({"execution_status":"pending_protection_sync"}),
    );
    report.filled_qty = Some(0.011);
    report.filled_quote = Some(24.12091);
    let adjusted = ProtectionSyncContract::from_task_result(&report, Some(protection))
        .expect("filled order should require protection");
    let fill_price = report.filled_quote.unwrap() / report.filled_qty.unwrap();
    assert!(
        adjusted.selected_stop_loss_price < fill_price,
        "long protective stop must be below fill price to avoid immediate trigger"
    );
    assert!((adjusted.selected_stop_loss_price - 2148.9538).abs() < 0.0001);
}
#[test]
fn protective_order_ack_requires_active_query_confirmation() {
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        exchange_symbol: "ETHUSDT".to_string(),
        instrument: Instrument::perp("ETH", "USDT").with_settlement("USDT"),
        order_id: Some("sl-123".to_string()),
        client_order_id: Some("rq-sl-42".to_string()),
        status: Some("NEW".to_string()),
        raw: json!({"orderId":"sl-123", "status":"NEW"}),
    };
    let outcome = protective_order_result_to_sync_outcome(Ok(ack));
    assert_eq!(
        outcome,
        ProtectionSyncOutcome::uncertain(
            "query_protective_order",
            "protective order ack requires active query confirmation"
        )
    );
}
#[test]
fn queried_new_protective_order_confirms_sync_outcome() {
    let order = Order {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT").with_settlement("USDT"),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("2000000953242572".to_string()),
        client_order_id: Some("rq-sl-183".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("STOP_MARKET".to_string()),
        price: Some("2145.22".to_string()),
        size: Some("0.000".to_string()),
        filled_size: None,
        average_price: None,
        status: Some("NEW".to_string()),
        created_at: Some(1779023785699),
        updated_at: Some(1779023785699),
        raw: json!({"algoStatus":"NEW"}),
    };
    let outcome = protective_order_query_to_sync_outcome(Ok(order));
    assert_eq!(
        outcome,
        ProtectionSyncOutcome::confirmed("2000000953242572", "query_protective_order")
    );
}
#[test]
fn queried_expired_protective_order_fails_sync_outcome() {
    let order = Order {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT").with_settlement("USDT"),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("2000000953242572".to_string()),
        client_order_id: Some("rq-sl-183".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("STOP_MARKET".to_string()),
        price: Some("2145.22".to_string()),
        size: Some("0.000".to_string()),
        filled_size: None,
        average_price: None,
        status: Some("EXPIRED".to_string()),
        created_at: Some(1779023785699),
        updated_at: Some(1779023895192),
        raw: json!({"algoStatus":"EXPIRED"}),
    };
    let outcome = protective_order_query_to_sync_outcome(Ok(order));
    assert_eq!(
        outcome,
        ProtectionSyncOutcome::failed(
            "query_protective_order",
            "protective order is not active: status=EXPIRED"
        )
    );
}
#[test]
fn protective_order_query_candidates_prefer_client_algo_id_then_algo_id() {
    let instrument = Instrument::perp("ETH", "USDT").with_settlement("USDT");
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        exchange_symbol: "ETHUSDT".to_string(),
        instrument: instrument.clone(),
        order_id: Some("2000000953310341".to_string()),
        client_order_id: Some("rq-sl-185".to_string()),
        status: Some("NEW".to_string()),
        raw: json!({"algoId":2000000953310341_i64, "clientAlgoId":"rq-sl-185", "algoStatus":"NEW"}),
    };
    let candidates =
        protective_order_query_candidates_from_ack(&instrument, &ack, Some("rq-sl-185".into()))
            .expect("protective query candidates");
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].client_order_id.as_deref(), Some("rq-sl-185"));
    assert_eq!(candidates[0].order_id, None);
    assert_eq!(candidates[1].order_id.as_deref(), Some("2000000953310341"));
    assert_eq!(candidates[1].client_order_id, None);
}
#[test]
fn protective_order_rejection_maps_to_failed_sync_outcome() {
    let error = crypto_exc_all::Error::Api {
        exchange: ExchangeId::Binance,
        status: Some(400),
        code: "-2021".to_string(),
        message: "Order would immediately trigger.".to_string(),
    };
    let outcome = protective_order_result_to_sync_outcome(Err(error));
    assert_eq!(
        outcome,
        ProtectionSyncOutcome::failed(
            "place_protective_order",
            "交易所 API 错误: binance status=Some(400) code=-2021: Order would immediately trigger."
        )
    );
}
#[test]
fn post_close_cancel_missing_binance_protective_order_is_idempotent_absent() {
    let mut report = ExecutionTaskReportRequest {
        task_id: 42,
        execution_status: "completed".to_string(),
        exchange: "binance".to_string(),
        external_order_id: "close-42".to_string(),
        order_side: "sell".to_string(),
        order_status: "FILLED".to_string(),
        filled_qty: Some(0.024),
        filled_quote: Some(52.26),
        fee_amount: None,
        profit_usdt: None,
        executed_at: None,
        error_message: None,
        raw_payload_json: Some(json!({"execution_status":"completed"}).to_string()),
    };
    let error = crypto_exc_all::Error::Api {
        exchange: ExchangeId::Binance,
        status: Some(400),
        code: "-2011".to_string(),
        message: "Unknown order sent.".to_string(),
    };
    apply_post_close_protection_cancel_result(&mut report, Err(error));
    let raw_payload: Value =
        serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "completed");
    assert_eq!(report.error_message, None);
    assert_eq!(
        raw_payload["post_close_protection_cancel"]["status"],
        "already_absent"
    );
    assert_eq!(
        raw_payload["post_close_protection_cancel"]["protective_order_absent"],
        true
    );
    assert_eq!(raw_payload["execution_status"], "completed");
}
