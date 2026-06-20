#[test]
fn pending_confirmation_task_builds_query_from_existing_order_result() {
    let task = task_with_metadata(
        "execute_signal",
        "confirming",
        json!({
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "size": "0.009"
        }),
    );
    let pending = PendingConfirmationTask::from_task_and_order_result(
        &task,
        "binance",
        "123456789",
        "buy",
        "NEW",
    )
    .unwrap();
    let query = pending.to_order_query().unwrap();

    assert_eq!(pending.exchange.as_str(), "binance");
    assert_eq!(pending.order_side, "buy");
    assert_eq!(query.order_id.as_deref(), Some("123456789"));
    assert_eq!(query.client_order_id, None);
}

#[test]
fn pending_confirmation_task_uses_stable_client_order_id_when_order_id_is_missing() {
    let task = task_with_metadata(
        "execute_signal",
        "confirming",
        json!({
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "size": "0.009"
        }),
    );
    let pending = PendingConfirmationTask::from_task_and_order_result(
        &task,
        "binance",
        "",
        "buy",
        "submitted",
    )
    .unwrap();
    let query = pending.to_order_query().unwrap();

    assert_eq!(query.order_id, None);
    assert_eq!(query.client_order_id.as_deref(), Some("rqtask42"));
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
fn strategy_size_usdt_payload_waits_for_live_ticker_and_filters() {
    let task = task(json!({
        "source": "rust_quant",
        "symbol": "ETH-USDT-SWAP",
        "payload_json": serde_json::json!({
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 60.0,
            "signal": {
                "open_price": 2300.38
            }
        }).to_string()
    }));

    let request = ExecutionOrderTask::from_task(&task).unwrap();

    assert_eq!(request.size, "0");
    assert_eq!(request.size_usdt, Some(60.0));
}

#[test]
fn live_order_size_is_quantized_to_exchange_step_size() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size_usdt": 60.0,
        "client_order_id": "rqtest"
    }));
    let filters = binance_eth_filters();

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request
        .to_live_order_request(Some(2300.38), Some(&filters))
        .unwrap();

    assert_eq!(order.size, "0.026");
}

#[test]
fn local_live_order_size_is_forced_to_exchange_min_notional() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size_usdt": 1000.0,
        "client_order_id": "rqtest-local-min"
    }));
    let filters = binance_eth_filters();

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request
        .to_live_order_request_with_local_min_size(Some(2300.38), Some(&filters), true)
        .unwrap();

    assert_eq!(order.size, "0.009");
}

#[test]
fn local_okx_swap_min_order_size_uses_contract_units_without_static_min_notional() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "ALLO-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size_usdt": 5.0,
        "client_order_id": "rqtest-okx-contract"
    }));
    let filters = ExchangeOrderFilters {
        min_qty: Some("1".parse().unwrap()),
        max_qty: Some("2700".parse().unwrap()),
        step_size: Some("1".parse().unwrap()),
        min_notional: None,
        quantity_precision: None,
        tick_size: Some("0.00001".parse().unwrap()),
        price_precision: Some(5),
        contract_value: Some("10".parse().unwrap()),
        contract_value_currency: Some("ALLO".to_string()),
    };

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request
        .to_live_order_request_with_local_min_size(Some(0.18702), Some(&filters), true)
        .unwrap();

    assert_eq!(order.size, "1");
}

#[test]
fn okx_swap_contract_value_is_used_for_notional_validation() {
    let filters = ExchangeOrderFilters {
        min_qty: Some("1".parse().unwrap()),
        max_qty: Some("2700".parse().unwrap()),
        step_size: Some("1".parse().unwrap()),
        min_notional: Some("5".parse().unwrap()),
        quantity_precision: None,
        tick_size: Some("0.00001".parse().unwrap()),
        price_precision: Some(5),
        contract_value: Some("10".parse().unwrap()),
        contract_value_currency: Some("ALLO".to_string()),
    };

    let error = quantize_order_size(
        "2".parse().unwrap(),
        "0.18702".parse().unwrap(),
        &filters,
        true,
    )
    .unwrap_err();

    assert!(error.to_string().contains("min_notional"));

    let ok = quantize_order_size(
        "3".parse().unwrap(),
        "0.18702".parse().unwrap(),
        &filters,
        true,
    )
    .unwrap();
    assert_eq!(ok.to_string(), "3");
}

#[test]
fn non_local_live_order_size_keeps_strategy_sizing() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size_usdt": 1000.0,
        "client_order_id": "rqtest-strategy-size"
    }));
    let filters = binance_eth_filters();

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request
        .to_live_order_request_with_local_min_size(Some(2300.38), Some(&filters), false)
        .unwrap();

    assert_eq!(order.size, "0.434");
}

#[test]
fn local_min_live_order_size_does_not_expand_reduce_only_close() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "sell",
        "order_type": "market",
        "size": "0.004",
        "trade_side": "close",
        "reduce_only": true,
        "client_order_id": "rqtest-local-close"
    }));
    let filters = binance_eth_filters();

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request
        .to_live_order_request_with_local_min_size(Some(2300.38), Some(&filters), true)
        .unwrap();

    assert_eq!(order.size, "0.004");
}

#[test]
fn order_request_attaches_selected_stop_loss_price() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 2200.5,
            "direction": "long"
        }
    }));

    let request = ExecutionOrderTask::from_task(&task)
        .unwrap()
        .to_order_request()
        .unwrap();

    assert_eq!(request.attached_stop_loss_price.as_deref(), Some("2200.5"));
}

#[test]
fn attached_stop_loss_exchanges_require_ack_or_order_detail_evidence() {
    for (exchange, raw) in [
        (
            "okx",
            json!({
                "ordId":"10001",
                "attachAlgoOrds":[{
                    "attachAlgoId":"rq-sl-10001",
                    "slTriggerPx":"2200.5",
                    "slOrdPx":"-1",
                    "slTriggerPxType":"last"
                }]
            }),
        ),
        (
            "bitget",
            json!({"orderId":"10002","presetStopLossPrice":"2200.5"}),
        ),
    ] {
        let task = task(json!({
            "exchange": exchange,
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size": "0.01",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.5,
                "direction": "long"
            }
        }));
        let order_task = ExecutionOrderTask::from_task(&task).unwrap();
        let instrument = Instrument::perp("ETH", "USDT");
        let ack = OrderAck {
            exchange: order_task.exchange,
            exchange_symbol: instrument.symbol_for(order_task.exchange),
            instrument,
            order_id: Some(format!("{exchange}-10001")),
            client_order_id: Some(format!("rqtask-{exchange}-10001")),
            status: Some("FILLED".to_string()),
            raw,
        };

        assert!(matches!(
            attached_stop_loss_order_ack_outcome(&order_task, &ack, None),
            Some(ProtectionSyncOutcome::Confirmed { .. })
        ));
    }

    let binance_task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 2200.5,
            "direction": "long"
        }
    }));
    let binance_order_task = ExecutionOrderTask::from_task(&binance_task).unwrap();
    let instrument = Instrument::perp("ETH", "USDT");
    let binance_ack = OrderAck {
        exchange: ExchangeId::Binance,
        exchange_symbol: "ETHUSDT".to_string(),
        instrument,
        order_id: Some("10003".to_string()),
        client_order_id: Some("rqtask-binance-10003".to_string()),
        status: Some("FILLED".to_string()),
        raw: json!({"orderId":"10003","status":"FILLED"}),
    };

    assert_eq!(
        attached_stop_loss_order_ack_outcome(&binance_order_task, &binance_ack, None),
        None
    );
}

#[test]
fn live_order_size_rejects_notional_below_exchange_minimum() {
    let filters = binance_eth_filters();

    let error = quantize_order_size(
        "0.0086".parse().unwrap(),
        "2300.38".parse().unwrap(),
        &filters,
        true,
    )
    .unwrap_err();

    assert!(error.to_string().contains("min_notional"));
}
