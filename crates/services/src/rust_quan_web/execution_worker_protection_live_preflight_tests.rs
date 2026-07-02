#[tokio::test]
async fn live_execute_signal_checks_api_credential_before_resolving_gateway() {
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
        let body = r#"{"success":true,"data":{"id":7788,"exchange":"binance","api_key_mask":"bn_***_tail","permission_scope":"trade","status":"active","credential_envelope_ready":true,"last_check_at":"2026-06-05T08:00:00","last_check_code":"binance_futures_funds_below_min_open","last_check_message":"futures balance below minimum","created_at":"2026-06-05T07:00:00","updated_at":"2026-06-05T08:00:00","execution_readiness":{"can_execute":false,"blocker_code":"binance_futures_funds_below_min_open","blocker_message":"futures balance below minimum","next_action_label":"重新校验 API Key","next_action_href":"/user-center/api-connections"}}}"#;
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
            worker_id: "worker-live-api-credential-check".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "source": "rust_quan_web",
        "api_credential_id": 7788,
        "symbol": "ETHUSDT",
        "execution": {
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    server.await.unwrap();
    let request = rx.recv().unwrap();
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert!(request.starts_with("POST /api/commerce/internal/api-credentials/7788/check HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "buy");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("binance_futures_funds_below_min_open"));
    assert_eq!(raw_payload["stage"], "api_credential_preflight");
    assert_eq!(raw_payload["api_credential_id"], 7788);
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn live_execute_signal_records_structured_api_credential_membership_blocker() {
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
        let body = r#"{"success":false,"code":"MEMBERSHIP_EXPIRED","message":"内部校验 API Key 失败: 会员已过期"}"#;
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
            worker_id: "worker-live-api-credential-membership-blocker".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "source": "rust_quan_web",
        "api_credential_id": 7789,
        "symbol": "ETHUSDT",
        "execution": {
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    server.await.unwrap();
    let request = rx.recv().unwrap();
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert!(request.starts_with("POST /api/commerce/internal/api-credentials/7789/check HTTP/1.1"));
    assert_eq!(report.execution_status, "failed");
    assert_eq!(raw_payload["stage"], "api_credential_preflight");
    assert_eq!(raw_payload["blocker_code"], "MEMBERSHIP_EXPIRED");
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn live_execute_signal_rejects_ready_api_credential_for_wrong_exchange_before_resolve() {
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
        let body = r#"{"success":true,"data":{"id":7799,"exchange":"okx","api_key_mask":"okx_***_tail","permission_scope":"trade","status":"active","credential_envelope_ready":true,"last_check_at":"2026-06-05T08:00:00","last_check_code":"signed_exchange_preflight_passed","last_check_message":"signed preflight passed","created_at":"2026-06-05T07:00:00","updated_at":"2026-06-05T08:00:00","execution_readiness":{"can_execute":true,"blocker_code":null,"blocker_message":null,"next_action_label":null,"next_action_href":null}}}"#;
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
            worker_id: "worker-live-api-credential-wrong-exchange".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "source": "rust_quan_web",
        "api_credential_id": 7799,
        "symbol": "ETHUSDT",
        "execution": {
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    server.await.unwrap();
    let request = rx.recv().unwrap();
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert!(request.starts_with("POST /api/commerce/internal/api-credentials/7799/check HTTP/1.1"));
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "buy");
    assert_eq!(raw_payload["stage"], "api_credential_preflight");
    assert_eq!(raw_payload["api_credential_id"], 7799);
    assert_eq!(raw_payload["credential_exchange"], "okx");
    assert_eq!(raw_payload["task_exchange"], "binance");
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn live_execute_signal_missing_api_credential_id_fails_before_resolve() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1:9".to_string(),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-missing-api-credential".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "source": "rust_quan_web",
        "symbol": "ETHUSDT",
        "execution": {
            "exchange": "binance",
            "symbol": "ETHUSDT",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "buy");
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
async fn live_execute_signal_rejects_unsupported_exchange_before_api_preflight() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1:9".to_string(),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-unsupported-exchange".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "source": "rust_quan_web",
        "api_credential_id": 8802,
        "symbol": "ETHUSDT",
        "execution": {
            "exchange": "bybit",
            "symbol": "ETHUSDT",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "bybit");
    assert_eq!(report.order_side, "buy");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("worker live execution is unsupported for exchange bybit"));
    assert_eq!(raw_payload["stage"], "live_exchange_capability");
    assert_eq!(raw_payload["exchange"], "bybit");
    assert_eq!(raw_payload["api_credential_preflight_attempted"], false);
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn live_execute_signal_resolves_gateway_after_ready_api_credential_preflight() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let server = tokio::task::spawn_blocking(move || {
        for body in [
            r#"{"success":true,"data":{"id":8801,"exchange":"okx","api_key_mask":"okx_***_tail","permission_scope":"trade","status":"active","credential_envelope_ready":true,"last_check_at":"2026-06-05T08:00:00","last_check_code":"signed_exchange_preflight_passed","last_check_message":"signed preflight passed","created_at":"2026-06-05T07:00:00","updated_at":"2026-06-05T08:00:00","execution_readiness":{"can_execute":true,"blocker_code":null,"blocker_message":null,"next_action_label":null,"next_action_href":null}}}"#,
            r#"{"success":true,"data":{"buyer_email":"buyer@example.com","exchange":"okx","api_key":"plain-api-key","api_secret":"plain-api-secret","passphrase":null,"simulated":false}}"#,
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
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
            worker_id: "worker-live-api-credential-ready-resolve".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Okx,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "source": "rust_quan_web",
        "api_credential_id": 8801,
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "okx",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    server.await.unwrap();
    let preflight_request = rx.recv().unwrap();
    let resolve_request = rx.recv().unwrap();
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert!(preflight_request
        .starts_with("POST /api/commerce/internal/api-credentials/8801/check HTTP/1.1"));
    assert!(resolve_request.starts_with(
        "GET /api/commerce/internal/api-credentials/resolve?buyer_email=buyer%40example.com&exchange=okx&credential_id=8801 HTTP/1.1"
    ));
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "okx");
    assert_eq!(report.order_side, "buy");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("OKX exchange credentials require passphrase"));
    assert_eq!(raw_payload["task_id"], 42);
    assert!(repository.audits.lock().unwrap().is_empty());
}
