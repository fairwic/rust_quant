#![allow(unused_imports)]

use super::execution_worker_test_support::*;
use super::*;
use crate::rust_quan_web::execution_payload::{
    live_order_confirmation_valid, protective_stop_loss_required,
};
use crate::rust_quan_web::execution_protection::{
    protective_order_query_candidates_from_ack, protective_order_query_to_sync_outcome,
    protective_order_result_to_sync_outcome,
};
use crate::rust_quan_web::{
    ExchangeReconciliationIssueType, ExecutionTask, ReportResultReplayCandidate,
};
use async_trait::async_trait;
use crypto_exc_all::{Instrument, ProtectiveOrderWorkingType};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn pending_close_task_dry_run_reports_close_candidate_result() {
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
            task_types: vec![
                "execute_signal".to_string(),
                "risk_control_close_candidate".to_string(),
            ],
            task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
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
            "symbol": "BTC-USDT-SWAP",
            "manual_review": {
                "task_type": "risk_control_close_candidate",
                "action": "close_candidate",
                "category": "exchange_delisting"
            },
            "risk_control": {
                "action": "close_candidate",
                "category": "exchange_delisting",
                "auto_execution_allowed": false
            }
        }),
    );

    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");

    assert_eq!(report.execution_status, "completed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "close");
    assert_eq!(report.order_status, "dry_run");
    assert_eq!(raw_payload["task_type"], "risk_control_close_candidate");
    assert_eq!(raw_payload["task_status"], "pending_close");
    assert_eq!(raw_payload["risk_control_action"], "close_candidate");
    assert_eq!(raw_payload["symbol"], "BTC-USDT-SWAP");
}

#[tokio::test]
async fn dry_run_execute_signal_with_required_stop_loss_stays_pending_protection_sync() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-dry-run-protection".to_string(),
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
            "selected_stop_loss_price": 3400.0,
            "entry_price": 3500.0,
            "direction": "long"
        }
    }));

    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");

    assert_eq!(report.execution_status, "pending_protection_sync");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "buy");
    assert_eq!(report.order_status, "dry_run");
    assert_eq!(
        raw_payload["protection_sync"]["status"],
        "pending_protection_sync"
    );
    assert_eq!(
        raw_payload["protection_sync"]["protective_order_confirmed"],
        false
    );
    assert_eq!(
        raw_payload["protection_sync"]["selected_stop_loss_price"],
        3400.0
    );
    assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
    assert_eq!(repository.audits.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn execute_signal_with_required_live_stop_loss_missing_selected_price_fails_before_order() {
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
            "direction": "long",
            "max_loss_percent": 0.02
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
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["risk_contract"]["missing_field"],
        "risk_plan.selected_stop_loss_price"
    );
    assert!(repository.audits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn live_config_missing_stop_loss_short_circuits_before_gateway_audit() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-config-no-live".to_string(),
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
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_plan": {
            "live_order": true,
            "protective_stop_loss_required": true,
            "direction": "long",
            "max_loss_percent": 0.02
        }
    }));

    let report = worker.execute_task(&task).await;
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().expect("raw payload"))
            .expect("raw payload json");

    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(raw_payload["risk_contract"]["worker_dry_run"], false);
    assert_eq!(raw_payload["risk_contract"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["risk_contract"]["missing_field"],
        "risk_plan.selected_stop_loss_price"
    );
    assert!(repository.audits.lock().unwrap().is_empty());
    assert!(repository.checkpoints.lock().unwrap().is_empty());
}

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
        "GET /api/commerce/internal/api-credentials/resolve?buyer_email=buyer%40example.com&exchange=okx HTTP/1.1"
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
