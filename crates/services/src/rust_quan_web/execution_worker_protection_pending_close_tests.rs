#[tokio::test]
async fn pending_close_live_mode_without_close_order_contract_reports_failed() {
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-close".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["risk_control_close_candidate".to_string()],
            task_statuses: vec!["pending_close".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    );
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "manual_review": {
                "task_type": "risk_control_close_candidate",
                "action": "close_candidate"
            },
            "risk_control": {
                "action": "close_candidate",
                "auto_execution_allowed": false
            }
        }),
    );
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "close");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("requires Web close_order payload"));
    assert_eq!(raw_payload["task_status"], "pending_close");
    assert_eq!(raw_payload["close_order"], Value::Null);
}
#[tokio::test]
async fn pending_close_live_mode_invalid_position_side_reports_failed() {
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-close".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["risk_control_close_candidate".to_string()],
            task_statuses: vec!["pending_close".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    );
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_side": "net",
                "size": 0.42,
                "order_type": "market",
                "reduce_only": true
            }
        }),
    );
    let report = worker.execute_task(&task).await;
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "close");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("unsupported close_order.position_side"));
}
#[tokio::test]
async fn pending_close_live_mode_checks_api_credential_before_resolving_gateway() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let server = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 4096];
        let bytes = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        tx.send(request).unwrap();
        let body = r#"{"success":true,"data":{"id":9901,"exchange":"binance","api_key_mask":"bn_***_tail","permission_scope":"trade","status":"active","credential_envelope_ready":true,"last_check_at":"2026-06-05T08:00:00","last_check_code":"binance_permission_not_trade_enabled","last_check_message":"API key has no trade permission","created_at":"2026-06-05T07:00:00","updated_at":"2026-06-05T08:00:00","execution_readiness":{"can_execute":false,"blocker_code":"binance_permission_not_trade_enabled","blocker_message":"API key has no trade permission","next_action_label":"重新校验 API Key","next_action_href":"/user-center/api-connections"}}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-close-api-credential-check".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["risk_control_close_candidate".to_string()],
            task_statuses: vec!["pending_close".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "api_credential_id": 9901,
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_mode": "hedge",
                "position_side": "long",
                "side": "sell",
                "size": 0.42,
                "order_type": "market"
            }
        }),
    );
    let report = worker.execute_task(&task).await;
    server.await.unwrap();
    let request = rx.recv().unwrap();
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert!(request.starts_with("POST /api/commerce/internal/api-credentials/9901/check HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "sell");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("binance_permission_not_trade_enabled"));
    assert_eq!(raw_payload["stage"], "api_credential_preflight");
    assert_eq!(raw_payload["api_credential_id"], 9901);
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn pending_close_live_mode_missing_api_credential_id_fails_before_resolve() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1:9".to_string(),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-close-missing-api-credential".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["risk_control_close_candidate".to_string()],
            task_statuses: vec!["pending_close".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_mode": "hedge",
                "position_side": "long",
                "side": "sell",
                "size": 0.42,
                "order_type": "market"
            }
        }),
    );
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "sell");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("api_credential_id_missing"));
    assert_eq!(raw_payload["stage"], "api_credential_preflight");
    assert_eq!(raw_payload["blocker_code"], "api_credential_id_missing");
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn leased_risk_close_candidate_still_uses_pending_close_order_path() {
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-close".to_string(),
            lease_limit: 1,
            dry_run: true,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["risk_control_close_candidate".to_string()],
            task_statuses: vec!["pending_close".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    );
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "leased",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_side": "long",
                "side": "sell",
                "size": 0.42,
                "order_type": "market",
                "reduce_only": true
            },
            "signal_type": "hold"
        }),
    );
    let report = worker.execute_task(&task).await;
    assert_eq!(report.execution_status, "completed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "sell");
    assert_ne!(report.order_status, "failed");
}
#[test]
fn pending_close_task_maps_web_close_order_to_reduce_only_order() {
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_side": "long",
                "side": "sell",
                "size": 0.42,
                "order_type": "market",
                "reduce_only": true
            }
        }),
    );
    let close_task = PendingCloseTask::from_task(&task, ExchangeId::Okx).unwrap();
    let order = close_task
        .to_order_request()
        .unwrap()
        .expect("close_order should map to an executable order");
    assert_eq!(order.exchange.as_str(), "binance");
    assert_eq!(order.instrument.symbol_for(order.exchange), "ETHUSDT");
    assert_eq!(order_side_lower(order.side), "sell");
    assert_eq!(order.size, "0.42");
    assert_eq!(order.position_side.as_deref(), Some("long"));
    assert_eq!(order.trade_side.as_deref(), Some("close"));
    assert_eq!(order.reduce_only, Some(true));
    assert_eq!(order.client_order_id.as_deref(), Some("rqclose42"));
}
#[test]
fn pending_close_task_okx_close_order_does_not_set_reduce_only_by_default() {
    // OKX hedge mode uses position_side to specify close direction; reduce_only is only
    // applicable in net mode. Verify the default is None for OKX.
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "okx",
                "symbol": "ETH-USDT-SWAP",
                "position_side": "long",
                "side": "sell",
                "size": "0.1",
                "order_type": "market"
            }
        }),
    );
    let close_task = PendingCloseTask::from_task(&task, ExchangeId::Okx).unwrap();
    let order = close_task
        .to_order_request()
        .unwrap()
        .expect("close_order should map to an executable order");
    assert_eq!(order.exchange.as_str(), "okx");
    assert_eq!(order_side_lower(order.side), "sell");
    assert_eq!(order.position_side.as_deref(), Some("long"));
    // reduce_only must be None for OKX — hedge mode does not support it
    assert_eq!(order.reduce_only, None);
}
#[test]
fn pending_close_task_binance_hedge_close_does_not_default_reduce_only() {
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order_status": "ready",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_mode": "hedge",
                "position_side": "long",
                "side": "sell",
                "size": "0.009",
                "order_type": "market"
            }
        }),
    );
    let close_task = PendingCloseTask::from_task(&task, ExchangeId::Binance).unwrap();
    let order = close_task
        .to_order_request()
        .unwrap()
        .expect("close_order should map to an executable order");
    assert_eq!(order.exchange.as_str(), "binance");
    assert_eq!(order_side_lower(order.side), "sell");
    assert_eq!(order.position_side.as_deref(), Some("long"));
    assert_eq!(order.trade_side.as_deref(), Some("close"));
    assert_eq!(order.reduce_only, None);
}
#[test]
fn pending_close_task_builds_protective_cancel_request_by_client_order_id() {
    let task = task_with_metadata(
        "risk_control_close_candidate",
        "pending_close",
        json!({
            "symbol": "ETH-USDT-SWAP",
            "close_order": {
                "exchange": "binance",
                "symbol": "ETH-USDT-SWAP",
                "position_mode": "hedge",
                "position_side": "long",
                "side": "sell",
                "size": "0.024",
                "order_type": "market",
                "margin_coin": "USDT",
                "cancel_protective_client_order_id": "rq-sl-168"
            }
        }),
    );
    let close_task = PendingCloseTask::from_task(&task, ExchangeId::Binance).unwrap();
    let (exchange, cancel_request) = close_task
        .protective_cancel_request()
        .unwrap()
        .expect("close task should carry a protective cancel request");
    assert_eq!(exchange, ExchangeId::Binance);
    assert_eq!(cancel_request.instrument.symbol_for(exchange), "ETHUSDT");
    assert_eq!(cancel_request.client_order_id.as_deref(), Some("rq-sl-168"));
    assert_eq!(cancel_request.order_id, None);
    assert_eq!(cancel_request.margin_coin.as_deref(), Some("USDT"));
}
#[test]
fn pending_close_reconciliation_requires_matching_nonzero_position() {
    let request = OrderPlacementRequest {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("eth", "usdt").with_settlement("usdt"),
        side: OrderSide::Sell,
        order_type: OrderType::Market,
        size: "0.42".to_string(),
        price: None,
        margin_mode: None,
        margin_coin: None,
        position_side: Some("long".to_string()),
        trade_side: Some("close".to_string()),
        client_order_id: Some("rqclose42".to_string()),
        reduce_only: None,
        time_in_force: None,
        attached_stop_loss_price: None,
    };
    let matching = Position {
        exchange: ExchangeId::Binance,
        instrument: request.instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        side: Some("LONG".to_string()),
        size: "0.42".to_string(),
        entry_price: None,
        mark_price: None,
        unrealized_pnl: None,
        leverage: None,
        margin_mode: None,
        liquidation_price: None,
        raw: json!({}),
    };
    let zero = Position {
        size: "0".to_string(),
        ..matching.clone()
    };
    let short = Position {
        side: Some("SHORT".to_string()),
        size: "-0.42".to_string(),
        ..matching.clone()
    };
    let wrong_exchange = Position {
        exchange: ExchangeId::Okx,
        exchange_symbol: "ETH-USDT-SWAP".to_string(),
        ..matching.clone()
    };
    let undersized_position = Position {
        size: "0.21".to_string(),
        ..matching.clone()
    };
    assert!(pending_close_has_matching_position(&matching, &request));
    assert!(!pending_close_has_matching_position(&zero, &request));
    assert!(!pending_close_has_matching_position(&short, &request));
    assert!(!pending_close_has_matching_position(&wrong_exchange, &request));
    assert!(!pending_close_has_matching_position(
        &undersized_position,
        &request
    ));
}
#[test]
fn pending_close_reconciliation_blocks_extra_active_close_orders_but_allows_planned_protection_cancel(
) {
    let request = OrderPlacementRequest {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("eth", "usdt").with_settlement("usdt"),
        side: OrderSide::Sell,
        order_type: OrderType::Market,
        size: "0.42".to_string(),
        price: None,
        margin_mode: None,
        margin_coin: None,
        position_side: Some("long".to_string()),
        trade_side: Some("close".to_string()),
        client_order_id: Some("rqclose42".to_string()),
        reduce_only: None,
        time_in_force: None,
        attached_stop_loss_price: None,
    };
    let protective_cancel = CancelOrderRequest::by_client_order_id(
        request.instrument.clone(),
        "rq-sl-168".to_string(),
    );
    let protective_stop_order = Order {
        exchange: ExchangeId::Binance,
        instrument: request.instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("sl-external-168".to_string()),
        client_order_id: Some("rq-sl-168".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("STOP_MARKET".to_string()),
        price: None,
        size: Some("0.42".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("NEW".to_string()),
        created_at: None,
        updated_at: None,
        raw: json!({}),
    };
    let take_profit_order = Order {
        client_order_id: Some("rq-tp-42-1".to_string()),
        order_type: Some("LIMIT".to_string()),
        ..protective_stop_order.clone()
    };
    assert!(!pending_close_has_conflicting_open_order(
        &protective_stop_order,
        &request,
        Some(&protective_cancel)
    ));
    assert!(pending_close_has_conflicting_open_order(
        &take_profit_order,
        &request,
        Some(&protective_cancel)
    ));
}
