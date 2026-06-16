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
use std::sync::{Arc, Mutex, OnceLock};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
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

#[test]
fn maps_task_payload_to_order_request() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "BTC-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "margin_mode": "cross",
        "position_side": "long",
        "trade_side": "open"
    }));

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request.to_order_request().unwrap();

    assert_eq!(order.exchange.as_str(), "okx");
    assert_eq!(order.instrument.symbol_for(order.exchange), "BTC-USDT-SWAP");
    assert_eq!(order.size, "0.01");
    assert_eq!(order.client_order_id.as_deref(), Some("rqtask42"));
}

#[test]
fn maps_nested_news_signal_payload_to_order_request() {
    let task = task(json!({
        "symbol": "BTC-USDT-SWAP",
        "signal_type": "buy",
        "payload_json": "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.001\",\"order_type\":\"market\",\"client_order_id\":\"smoke-dry-run-42\"}"
    }));

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request.to_order_request().unwrap();

    assert_eq!(order.exchange.as_str(), "okx");
    assert_eq!(order.size, "0.001");
    assert_eq!(order.client_order_id.as_deref(), Some("smoke-dry-run-42"));
}

#[test]
fn maps_web_execution_payload_to_order_request() {
    let task = task(json!({
        "source": "rust_quant",
        "symbol": "ETH-USDT-SWAP",
        "signal_type": "entry",
        "direction": "long",
        "payload_json": "{\"signal\":{\"open_price\":3500.0},\"client_order_id\":\"rq421704067200000\"}",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_settings": {
            "max_position_usdt": 35.0,
            "risk_acknowledged": true,
            "status": "active"
        }
    }));

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request.to_order_request().unwrap();

    assert_eq!(order.exchange.as_str(), "binance");
    assert_eq!(request.symbol, "ETH-USDT-SWAP");
    assert_eq!(order.instrument.symbol_for(order.exchange), "ETHUSDT");
    assert_eq!(order.size, "0.01");
    assert_eq!(order.client_order_id.as_deref(), Some("rq421704067200000"));
}

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

#[test]
fn live_order_confirmation_requires_exact_opt_in_token() {
    assert!(live_order_confirmation_valid(
        false,
        Some("I_UNDERSTAND_LIVE_ORDERS")
    ));
    assert!(live_order_confirmation_valid(true, None));
    assert!(!live_order_confirmation_valid(false, None));
    assert!(!live_order_confirmation_valid(false, Some("true")));
    assert!(!live_order_confirmation_valid(false, Some("I_UNDERSTAND")));
}

#[test]
fn reconciliation_only_mode_is_explicit_opt_in() {
    let _guard = env_lock();
    let previous = std::env::var("EXECUTION_WORKER_RECONCILIATION_ONLY").ok();

    std::env::remove_var("EXECUTION_WORKER_RECONCILIATION_ONLY");
    assert!(!reconciliation_only_mode_from_env());
    std::env::set_var("EXECUTION_WORKER_RECONCILIATION_ONLY", "true");
    assert!(reconciliation_only_mode_from_env());
    std::env::set_var("EXECUTION_WORKER_RECONCILIATION_ONLY", "yes");
    assert!(reconciliation_only_mode_from_env());
    std::env::set_var("EXECUTION_WORKER_RECONCILIATION_ONLY", "false");
    assert!(!reconciliation_only_mode_from_env());

    match previous {
        Some(value) => std::env::set_var("EXECUTION_WORKER_RECONCILIATION_ONLY", value),
        None => std::env::remove_var("EXECUTION_WORKER_RECONCILIATION_ONLY"),
    }
}

#[test]
fn reconciliation_only_symbol_guard_excludes_linkusdt() {
    assert!(is_protected_link_symbol("LINKUSDT"));
    assert!(is_protected_link_symbol("LINK-USDT-SWAP"));
    assert!(is_protected_link_symbol("link-usdt"));
    assert!(!is_protected_link_symbol("ETHUSDT"));
}

#[test]
fn target_task_allowlist_rejects_unlisted_leased_task_ids() {
    let config = ExecutionWorkerConfig {
        worker_id: "worker-targeted".to_string(),
        lease_limit: 1,
        dry_run: false,
        default_exchange: ExchangeId::Binance,
        task_types: vec!["risk_control_close_candidate".to_string()],
        task_statuses: vec!["pending_close".to_string()],
        target_task_ids: vec![1001],
        confirmation_mode: false,
        report_replay_mode: false,
        report_replay_max_per_run: 1,
        report_replay_failure_backoff_seconds: 300,
        report_replay_throttle_ms: 0,
    };

    assert!(config.leased_task_allowed(1001));
    assert!(!config.leased_task_allowed(1002));
}

#[test]
fn live_worker_config_requires_target_task_allowlist() {
    let live_unscoped = ExecutionWorkerConfig {
        worker_id: "worker-live-unscoped".to_string(),
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
    };
    let error = live_unscoped
        .validate_live_worker_scope()
        .expect_err("live worker without target task ids must fail closed");
    assert!(error
        .to_string()
        .contains("EXECUTION_WORKER_TARGET_TASK_IDS"));

    let dry_run_unscoped = ExecutionWorkerConfig {
        dry_run: true,
        ..live_unscoped
    };
    dry_run_unscoped
        .validate_live_worker_scope()
        .expect("dry-run worker may lease broadly");
}

#[test]
fn dry_run_result_is_reportable_without_exchange_credentials() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "BTC-USDT-SWAP",
        "signal_type": "long"
    }));

    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let result = request.dry_run_report().unwrap();

    assert_eq!(result.task_id, 42);
    assert_eq!(result.execution_status, "completed");
    assert_eq!(result.exchange, "okx");
    assert_eq!(result.order_side, "buy");
    assert_eq!(result.order_status, "dry_run");
    assert_eq!(
        result.raw_payload_json.as_deref(),
        Some("{\"dry_run\":true,\"symbol\":\"BTC-USDT-SWAP\"}")
    );
}
