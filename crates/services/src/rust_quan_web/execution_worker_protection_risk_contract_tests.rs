#[tokio::test]
async fn live_execute_signal_missing_order_side_fails_before_api_preflight() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1:9".to_string(),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-missing-side".to_string(),
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
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
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
        .contains("execution order side is required"));
    assert_eq!(raw_payload["risk_contract"]["blocker_code"], "missing_order_side");
    assert_eq!(
        raw_payload["risk_contract"]["missing_field"],
        "execution.side"
    );
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn news_event_missing_stop_loss_price_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-news-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
        "source_signal_type": "news_event",
        "source": "rust_quant_news",
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "direction": "long",
            "entry_price": 3500.0
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
        .contains("risk_plan.selected_stop_loss_price"));
    assert_eq!(
        raw_payload["risk_contract"]["source_signal_type"],
        "news_event"
    );
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["risk_contract"]["missing_field"],
        "risk_plan.selected_stop_loss_price"
    );
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn news_signal_id_missing_stop_loss_price_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-news-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
    let mut task = task(json!({
        "source": "rust_quant_news",
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "direction": "long",
            "entry_price": 3500.0
        }
    }));
    task.news_signal_id = Some(77);
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("risk_plan.selected_stop_loss_price"));
    assert_eq!(raw_payload["risk_contract"]["news_signal_id"], 77);
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn required_stop_loss_missing_entry_price_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-missing-entry-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size": "0.01"
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("risk_plan.entry_price"));
    assert_eq!(
        raw_payload["risk_contract"]["missing_field"],
        "risk_plan.entry_price"
    );
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn execute_signal_with_required_live_stop_loss_invalid_direction_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "live_order": true,
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "sideways"
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
        .contains("unsupported protective stop-loss direction"));
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["risk_contract"]["invalid_direction"],
        "sideways"
    );
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn execute_signal_with_long_stop_loss_above_entry_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3600.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("invalid protective stop-loss price"));
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(raw_payload["risk_contract"]["entry_price"], 3500.0);
    assert_eq!(
        raw_payload["risk_contract"]["selected_stop_loss_price"],
        3600.0
    );
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn execute_signal_with_short_stop_loss_below_entry_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
        "symbol": "ETH-USDT-SWAP",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "sell",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "short"
        }
    }));
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("invalid protective stop-loss price"));
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(raw_payload["risk_contract"]["entry_price"], 3500.0);
    assert_eq!(
        raw_payload["risk_contract"]["selected_stop_loss_price"],
        3400.0
    );
    assert!(repository.audits.lock().unwrap().is_empty());
}
#[tokio::test]
async fn execute_signal_with_stop_reset_take_profit_on_unsupported_exchange_fails_before_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-risk-contract".to_string(),
            lease_limit: 1,
            dry_run: true,
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
    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");
    assert_eq!(report.execution_status, "failed");
    assert!(report
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("take-profit stop reset requires protective order cancellation support"));
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["risk_contract"]["reason"],
        "unsupported_take_profit_stop_reset"
    );
    assert_eq!(
        raw_payload["risk_contract"]["unsupported_feature"],
        "take_profit_stop_reset"
    );
    assert!(repository.audits.lock().unwrap().is_empty());
}
