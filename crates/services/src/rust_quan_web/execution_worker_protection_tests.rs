#![allow(unused_imports)]
use super::execution_worker_test_support::*;
use super::*;
use crate::rust_quan_web::execution_payload::protective_stop_loss_required;
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
async fn live_open_order_without_stop_loss_contract_fails_before_api_preflight() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-open-no-stop".to_string(),
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
            "direction": "long",
            "entry_price": 3500.0
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
        .contains("risk_plan.selected_stop_loss_price"));
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
async fn live_config_without_persistent_audit_repo_fails_closed_before_gateway() {
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1:9".to_string(),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-live-no-audit".to_string(),
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
    );
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
        .contains("QUANT_CORE_DATABASE_URL is required for live execution audit"));
    assert_eq!(raw_payload["stage"], "live_audit_repository");
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
}
#[test]
fn live_max_order_size_preflight_happens_after_account_settings_and_before_order_mutations() {
    let source = include_str!("execution_worker_live_execution_section.rs");
    let live_source = &source[source
        .find("// live_order_request")
        .expect("source should keep the final live request section visible")..];
    let prepare_offset = live_source
        .find(".prepare_order_settings_for_live_order")
        .expect("settings preparation should stay visible in execute_task");
    let max_size_offset = live_source
        .find(".apply_live_max_order_size_gate")
        .expect("live order should query exchange max-size after leverage is prepared");
    let prearm_offset = live_source
        .find("match prearm_protective_order_if_required")
        .expect("execute_task should prearm protective order before main order");
    let place_offset = live_source
        .find(".place_order_with_audit")
        .expect("execute_task should submit the main order through audited live guard");
    assert!(
        prepare_offset < max_size_offset,
        "strategy leverage and margin settings must be applied before querying exchange max-size"
    );
    assert!(
        max_size_offset < prearm_offset,
        "exchange max-size preflight must happen before prearmed protection mutation"
    );
    assert!(
        max_size_offset < place_offset,
        "exchange max-size preflight must happen before the main live order mutation"
    );
}
#[test]
fn live_risk_reservation_happens_before_final_live_order_request() {
    let source = include_str!("execution_worker_live_execution_section.rs");
    let reservation_offset = source
        .find(".reserve_live_execution_risk_budget")
        .expect("execute_task should reserve final risk budget before building the live order");
    let live_request_offset = source
        .find(".live_order_request")
        .expect("execute_task should build final live order request");
    assert!(
        reservation_offset < live_request_offset,
        "final account-level risk reservation must happen before live order sizing"
    );
}
#[test]
fn live_min_notional_lookup_happens_before_risk_reservation() {
    let source = include_str!("execution_worker_live_execution_section.rs");
    let min_notional_offset = source
        .find(".live_order_minimum_notional_usdt")
        .expect("execute_task should derive exchange minimum notional before risk reservation");
    let reservation_offset = source
        .find(".reserve_live_execution_risk_budget")
        .expect("execute_task should reserve final risk budget before building the live order");
    assert!(
        min_notional_offset < reservation_offset,
        "exchange minimum notional must be known before occupying risk reservation budget"
    );
}
#[test]
fn live_prepare_order_settings_is_not_binance_only() {
    let source = include_str!("execution_worker_live_execution_support_section.rs");
    assert!(
        !source.contains("order_task.exchange != ExchangeId::Binance"),
        "OKX and Binance must both prepare isolated leverage before the main order"
    );
}
include!("execution_worker_protection_live_preflight_tests.rs");
include!("execution_worker_protection_risk_contract_tests.rs");
include!("execution_worker_protection_pending_close_tests.rs");
