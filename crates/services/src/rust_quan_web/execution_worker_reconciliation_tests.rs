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
use std::sync::Arc;
#[test]
fn okx_position_mode_switch_restriction_maps_structured_blocker() {
    let error: anyhow::Error = crypto_exc_all::Error::Api {
        exchange: ExchangeId::Okx,
        status: None,
        code: "59000".to_string(),
        message:
            "Setting failed. Cancel any open orders, close positions, and stop trading bots first."
                .to_string(),
    }
    .into();

    let (code, message) = prepare_order_settings_blocker(&error).expect("blocker");

    assert_eq!(
        code,
        "okx_position_mode_switch_blocked_by_existing_exposure"
    );
    assert!(message.contains("本次已在下单前阻断"));
}

#[test]
fn position_stale_reconciliation_request_from_task_uses_idempotent_source_ref() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "credential_ref": "web-cred-42"
    }));
    let request = build_exchange_reconciliation_report_request(
        &task,
        ExchangeReconciliationIssueType::ExchangePositionStale,
        Some("2026-05-15T09:30:00Z".to_string()),
        "position drift detected",
    );
    let repeated = build_exchange_reconciliation_report_request(
        &task,
        ExchangeReconciliationIssueType::ExchangePositionStale,
        Some("2026-05-15T09:31:00Z".to_string()),
        "position drift detected again",
    );
    assert_eq!(request.combo_id, 9);
    assert_eq!(request.buyer_email, "buyer@example.com");
    assert_eq!(request.symbol, "ETHUSDT");
    assert_eq!(
        request.issue_type,
        ExchangeReconciliationIssueType::ExchangePositionStale
    );
    assert_eq!(
        request.source_ref.as_deref(),
        Some(
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-42:combo=9:task=42:sym=ETHUSDT:issue=exchange_position_stale"
        )
    );
    assert_eq!(request.source_ref, repeated.source_ref);
    assert!(!request
        .source_ref
        .as_deref()
        .unwrap()
        .contains("buyer@example.com"));
}
#[test]
fn open_order_conflict_reconciliation_request_uses_hashed_account_and_safe_credential_ref() {
    let mut task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "api_credential_id": 7788,
        "api_key": "plain-api-key",
        "api_secret": "plain-api-secret",
        "passphrase": "plain-passphrase"
    }));
    task.buyer_email = "  Buyer@Example.COM  ".to_string();
    let request = build_exchange_reconciliation_report_request(
        &task,
        ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
        None,
        "unexpected open order blocks execution",
    );
    assert_eq!(
        request.issue_type,
        ExchangeReconciliationIssueType::ExchangeOpenOrderConflict
    );
    assert_eq!(
        request.source_ref.as_deref(),
        Some(
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=7788:combo=9:task=42:sym=ETHUSDT:issue=exchange_open_order_conflict"
        )
    );
    let source_ref = request.source_ref.as_deref().unwrap();
    assert!(!source_ref.contains("Buyer@Example.COM"));
    assert!(!source_ref.contains("plain-api-key"));
    assert!(!source_ref.contains("plain-api-secret"));
    assert!(!source_ref.contains("plain-passphrase"));
    assert_eq!(
        request.message.as_deref(),
        Some("unexpected open order blocks execution")
    );
}
#[test]
fn reconciliation_request_defaults_unknown_credential_ref_without_rendering_secret_fields() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "api_key": "plain-api-key",
        "api_secret": "plain-api-secret",
        "passphrase": "plain-passphrase"
    }));
    let request = build_exchange_reconciliation_report_request(
        &task,
        ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
        None,
        "unexpected open order blocks execution",
    );
    assert_eq!(
        request.source_ref.as_deref(),
        Some(
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=cred_unknown:combo=9:task=42:sym=ETHUSDT:issue=exchange_open_order_conflict"
        )
    );
    let rendered = serde_json::to_string(&request).unwrap();
    assert!(!rendered.contains("plain-api-key"));
    assert!(!rendered.contains("plain-api-secret"));
    assert!(!rendered.contains("plain-passphrase"));
}
#[test]
fn read_only_exchange_snapshot_builds_reconciliation_requests_without_live_mutation() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "credential_ref": "web-cred-42"
    }));
    let instrument = Instrument::perp("ETH", "USDT");
    let positions = vec![Position {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        side: Some("LONG".to_string()),
        size: "0.02".to_string(),
        entry_price: Some("3136".to_string()),
        mark_price: Some("3140".to_string()),
        unrealized_pnl: None,
        leverage: None,
        margin_mode: None,
        liquidation_price: None,
        raw: json!({"secret":"position-raw-should-not-render"}),
    }];
    let open_orders = vec![Order {
        exchange: ExchangeId::Binance,
        instrument,
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("open-1".to_string()),
        client_order_id: Some("client-open-1".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("STOP_MARKET".to_string()),
        price: None,
        size: Some("0.02".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("NEW".to_string()),
        created_at: Some(1),
        updated_at: Some(2),
        raw: json!({"secret":"open-order-raw-should-not-render"}),
    }];
    let requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
        &task,
        &positions,
        &open_orders,
        Some("2026-05-15T10:00:00Z".to_string()),
    );
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].issue_type,
        ExchangeReconciliationIssueType::ExchangePositionStale
    );
    assert_eq!(
        requests[0].source_ref.as_deref(),
        Some(
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-42:combo=9:task=42:sym=ETHUSDT:issue=exchange_position_stale"
        )
    );
    assert_eq!(
        requests[1].issue_type,
        ExchangeReconciliationIssueType::ExchangeOpenOrderConflict
    );
    assert_eq!(
        requests[1].source_ref.as_deref(),
        Some(
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-42:combo=9:task=42:sym=ETHUSDT:issue=exchange_open_order_conflict"
        )
    );
    let rendered = serde_json::to_string(&requests).unwrap();
    assert!(rendered.contains("read-only exchange snapshot"));
    assert!(rendered.contains("place_order_allowed=false"));
    assert!(!rendered.contains("position-raw-should-not-render"));
    assert!(!rendered.contains("open-order-raw-should-not-render"));
}
#[test]
fn hedge_position_side_entry_does_not_block_on_existing_position_or_open_order() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "credential_ref": "web-cred-42",
        "execution": {
            "position_mode": "hedge",
            "position_side": "long"
        }
    }));
    let instrument = Instrument::perp("ETH", "USDT");
    let positions = vec![Position {
        exchange: ExchangeId::Okx,
        instrument: instrument.clone(),
        exchange_symbol: "ETH-USDT-SWAP".to_string(),
        side: Some("long".to_string()),
        size: "0.01".to_string(),
        entry_price: Some("1750".to_string()),
        mark_price: Some("1755".to_string()),
        unrealized_pnl: None,
        leverage: None,
        margin_mode: None,
        liquidation_price: None,
        raw: json!({"secret":"position-raw-should-not-render"}),
    }];
    let open_orders = vec![Order {
        exchange: ExchangeId::Okx,
        instrument,
        exchange_symbol: "ETH-USDT-SWAP".to_string(),
        order_id: Some("protective-1".to_string()),
        client_order_id: Some("client-protective-1".to_string()),
        side: Some("sell".to_string()),
        order_type: Some("stop_market".to_string()),
        price: None,
        size: Some("0.01".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("live".to_string()),
        created_at: Some(1),
        updated_at: Some(2),
        raw: json!({"secret":"open-order-raw-should-not-render"}),
    }];
    let requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
        &task,
        &positions,
        &open_orders,
        Some("2026-05-15T10:00:00Z".to_string()),
    );
    assert!(requests.is_empty());
}
#[test]
fn read_only_exchange_snapshot_blocker_builder_does_not_emit_flat_sync() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "UNI-USDT-SWAP",
        "side": "buy",
        "size": "1",
        "credential_ref": "web-cred-85"
    }));
    let requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
        &task,
        &[],
        &[],
        Some("2026-06-06T08:30:00Z".to_string()),
    );
    assert!(
        requests.is_empty(),
        "normal live preflight builder must not treat flat position sync as a blocker"
    );
}
#[test]
fn read_only_exchange_snapshot_sync_builder_emits_flat_position_report() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "UNI-USDT-SWAP",
        "side": "buy",
        "size": "1",
        "credential_ref": "web-cred-85"
    }));
    let requests = build_exchange_reconciliation_sync_requests_from_read_only_snapshot(
        &task,
        &[],
        &[],
        Some("2026-06-06T08:30:00Z".to_string()),
    );
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].issue_type,
        ExchangeReconciliationIssueType::ExchangePositionFlat
    );
    assert_eq!(requests[0].symbol, "UNI-USDT-SWAP");
    assert_eq!(
        requests[0].source_ref.as_deref(),
        Some(
            "rq:xrec:v2:ex=okx:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-85:combo=9:task=42:sym=UNI-USDT-SWAP:issue=exchange_position_flat"
        )
    );
    assert!(requests[0]
        .message
        .as_deref()
        .is_some_and(|message| message.contains("zero position")));
}
#[test]
fn live_order_reconciliation_conflict_builds_no_mutation_failed_report() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "credential_ref": "web-cred-42"
    }));
    let order_task =
        ExecutionOrderTask::from_task_with_default(&task, ExchangeId::Binance).unwrap();
    let requests = vec![
        build_exchange_reconciliation_report_request(
            &task,
            ExchangeReconciliationIssueType::ExchangePositionStale,
            Some("2026-05-15T10:00:00Z".to_string()),
            "read-only exchange snapshot detected 1 non-zero position(s); place_order_allowed=false; mutation_allowed=false",
        ),
        build_exchange_reconciliation_report_request(
            &task,
            ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
            Some("2026-05-15T10:00:00Z".to_string()),
            "read-only exchange snapshot detected 1 open order(s); place_order_allowed=false; mutation_allowed=false",
        ),
    ];
    let report =
        build_live_order_blocked_by_exchange_reconciliation_report(&task, &order_task, &requests);
    let raw_payload: Value =
        serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.order_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "buy");
    assert!(report
        .error_message
        .as_deref()
        .unwrap()
        .contains("read-only exchange reconciliation"));
    assert_eq!(raw_payload["stage"], "exchange_reconciliation_read_only");
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert_eq!(raw_payload["issues"].as_array().unwrap().len(), 2);
    assert_eq!(
        raw_payload["source_refs"],
        json!([
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-42:combo=9:task=42:sym=ETHUSDT:issue=exchange_position_stale",
            "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-42:combo=9:task=42:sym=ETHUSDT:issue=exchange_open_order_conflict"
        ])
    );
}
#[test]
fn live_order_reconciliation_gateway_read_failure_builds_no_mutation_failed_report() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
        "side": "buy",
        "size": "0.01",
        "credential_ref": "web-cred-42"
    }));
    let order_task =
        ExecutionOrderTask::from_task_with_default(&task, ExchangeId::Binance).unwrap();
    let report = build_live_order_blocked_by_exchange_reconciliation_read_error_report(
        &task,
        &order_task,
        "read-only exchange position reconciliation failed before live order: fixture timeout",
    );
    let raw_payload: Value =
        serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.order_status, "failed");
    assert_eq!(report.exchange, "binance");
    assert_eq!(report.order_side, "buy");
    assert!(report
        .error_message
        .as_deref()
        .unwrap()
        .contains("read-only exchange reconciliation failed before live order"));
    assert_eq!(raw_payload["stage"], "exchange_reconciliation_read_only");
    assert_eq!(raw_payload["gateway_read_failed"], true);
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert_eq!(raw_payload["place_order_retried"], false);
    assert_eq!(
        raw_payload["source_ref"],
        "rq:xrec:v2:ex=binance:acct=email_sha256_6a6c26195c3682fa:cred=web-cred-42:combo=9:task=42:sym=ETHUSDT:issue=gateway_read_failed"
    );
}
include!("execution_worker_reconciliation_order_request_tests.rs");
include!("execution_worker_reconciliation_protection_tests.rs");
include!("execution_worker_reconciliation_sizing_tests.rs");
include!("execution_worker_reconciliation_config_tests.rs");
