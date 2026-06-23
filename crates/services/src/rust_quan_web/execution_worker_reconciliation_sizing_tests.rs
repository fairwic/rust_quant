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
fn risk_reservation_overrides_payload_derived_live_order_size() {
    let task = task(json!({
        "source": "rust_quan_web",
        "symbol": "TEST-USDT-SWAP",
        "signal_type": "buy",
        "execution": {
            "exchange": "binance",
            "symbol": "TEST-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 500.0,
            "leverage": 3.0
        },
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 90.0,
            "direction": "long"
        }
    }));
    let mut request = ExecutionOrderTask::from_task(&task).unwrap();
    assert_eq!(request.size, "5");
    request
        .apply_risk_reservation(&ExecutionRiskReservationResponse {
            task_id: task.id,
            buyer_email: task.buyer_email.clone(),
            exchange: "binance".to_string(),
            api_credential_id: Some(8801),
            risk_budget_batch_id: Some("batch-1".to_string()),
            allocation_mode: "equal_batch_split".to_string(),
            allowed_notional_usdt: 250.0,
            required_margin_usdt: 50.0,
            stop_risk_usdt: 25.0,
            leverage: 5.0,
            margin_mode: "isolated".to_string(),
            position_mode: "one_way".to_string(),
        })
        .unwrap();
    assert_eq!(request.size, "0");
    assert_eq!(request.size_usdt, Some(250.0));
    assert_eq!(request.leverage.as_deref(), Some("5"));
    let order = request.to_order_request_with_last_price(Some(100.0)).unwrap();
    assert_eq!(order.size, "2.5");
}
#[test]
fn risk_reserved_live_order_below_exchange_minimum_fails_closed_even_with_local_min_size() {
    let task = task(json!({
        "source": "rust_quan_web",
        "symbol": "ETH-USDT-SWAP",
        "signal_type": "buy",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 500.0,
            "leverage": 3.0
        },
        "risk_plan": {
            "entry_price": 2300.38,
            "selected_stop_loss_price": 2200.0,
            "direction": "long"
        }
    }));
    let mut request = ExecutionOrderTask::from_task(&task).unwrap();
    request
        .apply_risk_reservation(&ExecutionRiskReservationResponse {
            task_id: task.id,
            buyer_email: task.buyer_email.clone(),
            exchange: "binance".to_string(),
            api_credential_id: Some(8801),
            risk_budget_batch_id: Some("batch-1".to_string()),
            allocation_mode: "equal_batch_split".to_string(),
            allowed_notional_usdt: 19.0,
            required_margin_usdt: 3.8,
            stop_risk_usdt: 0.83,
            leverage: 5.0,
            margin_mode: "isolated".to_string(),
            position_mode: "one_way".to_string(),
        })
        .unwrap();
    let error = request
        .to_live_order_request_with_local_min_size(Some(2300.38), Some(&binance_eth_filters()), true)
        .expect_err("risk reservation must not be expanded above its allowed notional");
    assert!(error.to_string().contains("min_notional"));
}
#[test]
fn exchange_minimum_notional_is_derived_for_risk_reservation() {
    let minimum = minimum_order_notional_usdt(
        "2300.38".parse().unwrap(),
        &binance_eth_filters(),
        true,
    )
    .unwrap();
    assert_eq!(minimum, Some(20.70342));
}
#[test]
fn live_order_reference_price_uses_side_quote_from_fresh_ticker() {
    let now_ms = 1_772_000_000_000_u64;
    let ticker = live_ticker("100", Some("99"), Some("101"), Some(now_ms - 1_000));
    assert_eq!(
        live_order_reference_price(&ticker, OrderSide::Buy, now_ms).unwrap(),
        101.0
    );
    assert_eq!(
        live_order_reference_price(&ticker, OrderSide::Sell, now_ms).unwrap(),
        99.0
    );
}
#[test]
fn stale_live_ticker_is_rejected_before_live_order_sizing() {
    let now_ms = 1_772_000_000_000_u64;
    let ticker = live_ticker(
        "100",
        Some("99"),
        Some("101"),
        Some(now_ms - LIVE_TICKER_MAX_AGE_MS - 1),
    );
    let error = live_order_reference_price(&ticker, OrderSide::Buy, now_ms).unwrap_err();
    assert!(error.to_string().contains("stale_live_ticker"));
}
#[test]
fn live_order_revalidates_stop_loss_against_current_reference_price() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size_usdt": 100.0,
        "client_order_id": "rqtest-live-stop",
        "risk_plan": {
            "entry_price": 110.0,
            "selected_stop_loss_price": 101.0,
            "direction": "long"
        }
    }));
    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let error = request
        .to_live_order_request(Some(100.0), Some(&binance_eth_filters()))
        .expect_err("long stop-loss above current reference price must fail closed");
    assert!(error.to_string().contains("live_stop_loss_price_invalid"));
}
#[test]
fn live_order_rejects_stop_loss_distance_that_is_too_small() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size_usdt": 100.0,
        "client_order_id": "rqtest-tight-stop",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 99.99,
            "direction": "long"
        }
    }));
    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let error = request
        .to_live_order_request(Some(100.0), Some(&binance_eth_filters()))
        .expect_err("stop-loss too close to current reference price must fail closed");
    assert!(error.to_string().contains("live_stop_loss_distance_out_of_range"));
}
#[test]
fn risk_reserved_live_order_rejects_notional_above_reserved_budget() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size": "2",
        "client_order_id": "rqtest-budget-boundary",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 95.0,
            "direction": "long"
        }
    }));
    let mut request = ExecutionOrderTask::from_task(&task).unwrap();
    request.risk_reserved = true;
    request.size_usdt = Some(100.0);
    let error = request
        .to_live_order_request(Some(100.0), Some(&binance_eth_filters()))
        .expect_err("risk-reserved order notional must not exceed allowed budget");
    assert!(error.to_string().contains("live_order_notional_exceeds_reservation"));
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
fn take_profit_legs_build_reduce_only_limit_close_orders() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "position_side": "long",
        "client_order_id": "rq-open-42",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 97.0,
            "direction": "long",
            "take_profit_legs": [
                {
                    "leg_index": 1,
                    "target_r": 2.0,
                    "fraction": 0.7,
                    "role": "base_take_profit"
                },
                {
                    "leg_index": 2,
                    "target_r": 8.0,
                    "fraction": 0.3,
                    "stop_after_fill_r": 0.0,
                    "role": "runner_take_profit"
                }
            ]
        }
    }));
    let order_task = ExecutionOrderTask::from_task(&task).unwrap();
    let requests =
        build_take_profit_order_requests(&order_task, 0.01, &binance_eth_filters()).unwrap();
    assert_eq!(order_task.take_profit_legs.len(), 2);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].side, OrderSide::Sell);
    assert_eq!(requests[0].order_type, OrderType::Limit);
    assert_eq!(requests[0].size, "0.007");
    assert_eq!(requests[0].price.as_deref(), Some("106"));
    assert_eq!(requests[0].reduce_only, Some(true));
    assert_eq!(requests[0].client_order_id.as_deref(), Some("rq-tp-42-1"));
    assert_eq!(requests[1].size, "0.003");
    assert_eq!(requests[1].price.as_deref(), Some("124"));
    assert_eq!(requests[1].reduce_only, Some(true));
    assert_eq!(requests[1].client_order_id.as_deref(), Some("rq-tp-42-2"));
}
#[test]
fn take_profit_ack_record_keeps_requested_client_order_id_when_ack_omits_it() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let request =
        build_take_profit_order_requests(&order_task, 0.01, &binance_eth_filters()).unwrap()[0]
            .clone();
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        exchange_symbol: "ETHUSDT".to_string(),
        instrument: crypto_exc_all::Instrument::perp("ETH", "USDT"),
        order_id: Some("tp-1".to_string()),
        client_order_id: None,
        status: Some("NEW".to_string()),
        raw: json!({"orderId":"tp-1","status":"NEW"}),
    };
    let record = take_profit_order_ack_record(&request, &ack);
    assert_eq!(record["client_order_id"], "rq-tp-42-1");
    assert_eq!(record["size"], "0.007");
    assert_eq!(record["price"], "106");
}
#[test]
fn take_profit_ack_terms_reject_mismatched_client_order_id() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let request =
        build_take_profit_order_requests(&order_task, 0.01, &binance_eth_filters()).unwrap()[0]
            .clone();
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        exchange_symbol: "ETHUSDT".to_string(),
        instrument: Instrument::perp("ETH", "USDT"),
        order_id: Some("tp-1".to_string()),
        client_order_id: Some("foreign-tp".to_string()),
        status: Some("NEW".to_string()),
        raw: json!({"orderId":"tp-1","clientOrderId":"foreign-tp","status":"NEW"}),
    };
    let error = take_profit_order_ack_request_error(&request, &ack)
        .expect("ack with mismatched client_order_id must fail closed");
    let record = take_profit_order_ack_record(&request, &ack);
    assert!(error.contains("client_order_id"));
    assert!(error.contains("foreign-tp"));
    assert_eq!(record["client_order_id"], "rq-tp-42-1");
    assert_eq!(record["exchange_client_order_id"], "foreign-tp");
}
#[test]
fn take_profit_ack_terms_reject_mismatched_exchange_or_instrument() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let request =
        build_take_profit_order_requests(&order_task, 0.01, &binance_eth_filters()).unwrap()[0]
            .clone();
    let mut ack = OrderAck {
        exchange: ExchangeId::Okx,
        exchange_symbol: "ETHUSDT".to_string(),
        instrument: request.instrument.clone(),
        order_id: Some("tp-1".to_string()),
        client_order_id: request.client_order_id.clone(),
        status: Some("NEW".to_string()),
        raw: json!({"orderId":"tp-1","status":"NEW"}),
    };
    let error = take_profit_order_ack_request_error(&request, &ack)
        .expect("ack with mismatched exchange must fail closed");
    assert!(error.contains("exchange"));
    ack.exchange = request.exchange;
    ack.instrument = Instrument::perp("BTC", "USDT");
    let error = take_profit_order_ack_request_error(&request, &ack)
        .expect("ack with mismatched instrument must fail closed");
    assert!(error.contains("instrument"));
}
#[test]
fn take_profit_existing_order_record_keeps_requested_client_order_id_when_exchange_omits_it() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let request =
        build_take_profit_order_requests(&order_task, 0.01, &binance_eth_filters()).unwrap()[0]
            .clone();
    let mut order = filled_base_take_profit_order();
    order.client_order_id = None;
    order.status = Some("NEW".to_string());
    let record = existing_take_profit_order_record(&request, &order);
    assert_eq!(record["client_order_id"], "rq-tp-42-1");
    assert_eq!(record["size"], "0.007");
    assert_eq!(record["price"], "106");
}
#[test]
fn take_profit_legs_reject_duplicate_leg_index_before_order_requests() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "position_side": "long",
        "client_order_id": "rq-open-42",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 97.0,
            "direction": "long",
            "take_profit_legs": [
                {
                    "leg_index": 1,
                    "target_r": 2.0,
                    "fraction": 0.7,
                    "role": "base_take_profit"
                },
                {
                    "leg_index": 1,
                    "target_r": 8.0,
                    "fraction": 0.3,
                    "role": "runner_take_profit"
                }
            ]
        }
    }));
    let error = ExecutionOrderTask::from_task(&task)
        .expect_err("duplicate take-profit leg indexes would reuse client order ids");
    assert!(error.to_string().contains("duplicate leg_index"));
}
#[test]
fn take_profit_legs_reject_non_positive_or_fractional_explicit_leg_index() {
    for invalid_leg_index in [0.0, 1.5] {
        let task = task(json!({
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "order_type": "market",
            "size": "0.01",
            "position_side": "long",
            "client_order_id": "rq-open-42",
            "risk_plan": {
                "entry_price": 100.0,
                "selected_stop_loss_price": 97.0,
                "direction": "long",
                "take_profit_legs": [
                    {
                        "leg_index": invalid_leg_index,
                        "target_r": 2.0,
                        "fraction": 0.7,
                        "role": "base_take_profit"
                    }
                ]
            }
        }));
        let error = ExecutionOrderTask::from_task(&task)
            .expect_err("explicit take-profit leg indexes must be positive integers");
        assert!(error.to_string().contains("leg_index must be a positive integer"));
    }
}
#[test]
fn take_profit_legs_reject_multiple_stop_after_fill_r_until_multi_stage_reset_is_supported() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "position_side": "long",
        "client_order_id": "rq-open-42",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 97.0,
            "direction": "long",
            "take_profit_legs": [
                {
                    "leg_index": 1,
                    "target_r": 2.0,
                    "fraction": 0.5,
                    "stop_after_fill_r": 0.0,
                    "role": "base_take_profit"
                },
                {
                    "leg_index": 2,
                    "target_r": 4.0,
                    "fraction": 0.3,
                    "stop_after_fill_r": 1.0,
                    "role": "trail_take_profit"
                },
                {
                    "leg_index": 3,
                    "target_r": 8.0,
                    "fraction": 0.2,
                    "role": "runner_take_profit"
                }
            ]
        }
    }));
    let error = ExecutionOrderTask::from_task(&task)
        .expect_err("multi-stage take-profit stop reset is not safe without per-leg reset state");
    assert!(error.to_string().contains("multiple stop_after_fill_r"));
}
#[test]
fn take_profit_legs_reject_invalid_stop_after_fill_r_instead_of_ignoring_it() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "position_side": "long",
        "client_order_id": "rq-open-42",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 97.0,
            "direction": "long",
            "take_profit_legs": [
                {
                    "leg_index": 1,
                    "target_r": 2.0,
                    "fraction": 0.7,
                    "stop_after_fill_r": "breakeven",
                    "role": "base_take_profit"
                },
                {
                    "leg_index": 2,
                    "target_r": 8.0,
                    "fraction": 0.3,
                    "role": "runner_take_profit"
                }
            ]
        }
    }));
    let error = ExecutionOrderTask::from_task(&task)
        .expect_err("invalid stop_after_fill_r must not be treated as absent");
    assert!(error.to_string().contains("stop_after_fill_r"));
    assert!(error.to_string().contains("numeric"));
}
#[test]
fn take_profit_stop_after_fill_builds_stop_reset_plan_for_filled_base_leg() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let take_profit_order = filled_base_take_profit_order();
    let plan = build_take_profit_stop_reset_plan(
        &order_task,
        &order_task.take_profit_legs[0],
        &take_profit_order,
        &binance_eth_filters(),
    )
    .unwrap()
    .expect("filled first take-profit leg should request a stop reset");
    assert_eq!(order_task.take_profit_legs[0].stop_after_fill_price, Some(100.0));
    assert_eq!(plan.leg_index, 1);
    assert_eq!(
        plan.take_profit_client_order_id.as_deref(),
        Some("rq-tp-42-1")
    );
    assert_eq!(
        plan.cancel_request.client_order_id.as_deref(),
        Some("rq-sl-42")
    );
    assert_eq!(plan.protective_order_request.stop_price, "100");
    assert_eq!(
        plan.protective_order_request.client_order_id.as_deref(),
        Some("rq-sl-42-tp1")
    );
    assert_ne!(
        plan.cancel_request.client_order_id,
        plan.protective_order_request.client_order_id
    );
    assert_eq!(plan.protective_order_request.close_position, Some(true));
}
#[test]
fn take_profit_stop_reset_rejects_filled_order_without_positive_fill_evidence() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let mut take_profit_order = filled_base_take_profit_order();
    take_profit_order.filled_size = Some("0".to_string());
    let error = build_take_profit_stop_reset_plan(
        &order_task,
        &order_task.take_profit_legs[0],
        &take_profit_order,
        &binance_eth_filters(),
    )
    .expect_err("FILLED take-profit order without positive filled_size must not reset stop");
    assert!(error.to_string().contains("filled_size"));
}
#[test]
fn take_profit_stop_reset_rejects_filled_order_with_mismatched_terms() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let mut take_profit_order = filled_base_take_profit_order();
    take_profit_order.side = Some("BUY".to_string());
    take_profit_order.price = Some("105".to_string());
    let error = build_take_profit_stop_reset_plan(
        &order_task,
        &order_task.take_profit_legs[0],
        &take_profit_order,
        &binance_eth_filters(),
    )
    .expect_err("mismatched take-profit order terms must not reset stop");
    assert!(error.to_string().contains("mismatch"));
    assert!(error.to_string().contains("side"));
    assert!(error.to_string().contains("price"));
}
#[test]
fn take_profit_stop_reset_rejects_filled_order_with_mismatched_size() {
    let order_task = binance_take_profit_stop_reset_order_task();
    let mut take_profit_order = filled_base_take_profit_order();
    take_profit_order.size = Some("0.001".to_string());
    take_profit_order.filled_size = Some("0.001".to_string());
    let error = build_take_profit_stop_reset_plan(
        &order_task,
        &order_task.take_profit_legs[0],
        &take_profit_order,
        &binance_eth_filters(),
    )
    .expect_err("mismatched take-profit order size must not reset stop");
    assert!(error.to_string().contains("mismatch"));
    assert!(error.to_string().contains("size"));
}
#[test]
fn take_profit_stop_reset_uses_tracked_tp_size_for_dynamic_sizing_tasks() {
    let mut order_task = binance_take_profit_stop_reset_order_task();
    order_task.size = "0".to_string();
    order_task.size_usdt = Some(60.0);
    let mut take_profit_order = filled_base_take_profit_order();
    take_profit_order.size = Some("0.001".to_string());
    take_profit_order.filled_size = Some("0.001".to_string());
    let previous_raw_payload_json = json!({
        "take_profit_sync": {
            "status": "completed",
            "orders": [
                {
                    "client_order_id": "rq-tp-42-1",
                    "size": "0.007",
                    "price": "106"
                }
            ]
        }
    })
    .to_string();
    let error = build_take_profit_stop_reset_plan_with_tracking(
        &order_task,
        &order_task.take_profit_legs[0],
        &take_profit_order,
        &binance_eth_filters(),
        Some(previous_raw_payload_json.as_str()),
    )
    .expect_err("tracked take-profit size mismatch must not reset stop");
    assert!(error.to_string().contains("mismatch"));
    assert!(error.to_string().contains("size"));
}
fn binance_take_profit_stop_reset_order_task() -> ExecutionOrderTask {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "position_side": "long",
        "client_order_id": "rq-open-42",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 97.0,
            "direction": "long",
            "take_profit_legs": [
                {
                    "leg_index": 1,
                    "target_r": 2.0,
                    "fraction": 0.7,
                    "stop_after_fill_r": 0.0,
                    "role": "base_take_profit"
                },
                {
                    "leg_index": 2,
                    "target_r": 8.0,
                    "fraction": 0.3,
                    "role": "runner_take_profit"
                }
            ]
        }
    }));
    ExecutionOrderTask::from_task(&task).unwrap()
}
fn filled_base_take_profit_order() -> Order {
    Order {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("tp-1".to_string()),
        client_order_id: Some("rq-tp-42-1".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("LIMIT".to_string()),
        price: Some("106".to_string()),
        size: Some("0.007".to_string()),
        filled_size: Some("0.007".to_string()),
        average_price: Some("106".to_string()),
        status: Some("FILLED".to_string()),
        created_at: None,
        updated_at: None,
        raw: json!({"status": "FILLED"}),
    }
}
#[test]
fn take_profit_stop_reset_requires_protective_cancel_capability() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "position_side": "long",
        "risk_plan": {
            "entry_price": 100.0,
            "selected_stop_loss_price": 97.0,
            "direction": "long",
            "take_profit_legs": [
                {
                    "leg_index": 1,
                    "target_r": 2.0,
                    "fraction": 0.7,
                    "stop_after_fill_r": 0.0,
                    "role": "base_take_profit"
                },
                {
                    "leg_index": 2,
                    "target_r": 8.0,
                    "fraction": 0.3,
                    "role": "runner_take_profit"
                }
            ]
        }
    }));
    let order_task = ExecutionOrderTask::from_task(&task).unwrap();
    let error = take_profit_stop_reset_capability_error(&order_task)
        .expect("OKX attached protection cannot reset stops through protective cancel");
    assert!(error.contains("okx"));
    assert!(error.contains("Unsupported"));
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
fn live_ticker(
    last_price: &str,
    bid_price: Option<&str>,
    ask_price: Option<&str>,
    timestamp: Option<u64>,
) -> crypto_exc_all::Ticker {
    crypto_exc_all::Ticker {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        instrument_type: Some("swap".to_string()),
        exchange_symbol: "ETHUSDT".to_string(),
        last_price: last_price.to_string(),
        last_size: None,
        bid_price: bid_price.map(str::to_string),
        bid_size: None,
        ask_price: ask_price.map(str::to_string),
        ask_size: None,
        open_24h: None,
        high_24h: None,
        low_24h: None,
        volume_24h: None,
        base_volume_24h: None,
        quote_volume_24h: None,
        sod_utc0: None,
        sod_utc8: None,
        timestamp,
        raw: json!({}),
    }
}
