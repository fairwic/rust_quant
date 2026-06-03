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

#[test]
fn confirmed_live_order_report_uses_order_detail_and_fills() {
    let instrument = Instrument::perp("ETH", "USDT");
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("12345".to_string()),
        client_order_id: Some("rqethopen1".to_string()),
        status: Some("NEW".to_string()),
        raw: json!({"status":"NEW","orderId":12345}),
    };
    let order = Order {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("12345".to_string()),
        client_order_id: Some("rqethopen1".to_string()),
        side: Some("BUY".to_string()),
        order_type: Some("MARKET".to_string()),
        price: Some("0".to_string()),
        size: Some("0.009".to_string()),
        filled_size: Some("0.009".to_string()),
        average_price: Some("2267.60000".to_string()),
        status: Some("FILLED".to_string()),
        created_at: Some(1),
        updated_at: Some(2),
        raw: json!({"status":"FILLED","executedQty":"0.009","avgPrice":"2267.60000"}),
    };
    let fill = Fill {
        exchange: ExchangeId::Binance,
        instrument,
        exchange_symbol: "ETHUSDT".to_string(),
        trade_id: Some("9001".to_string()),
        order_id: Some("12345".to_string()),
        side: Some("BUY".to_string()),
        price: Some("2267.60000".to_string()),
        size: Some("0.009".to_string()),
        fee: Some("0.01020420".to_string()),
        fee_asset: Some("USDT".to_string()),
        role: Some("taker".to_string()),
        timestamp: Some(3),
        raw: json!({"id":9001,"qty":"0.009","price":"2267.60000","commission":"0.01020420"}),
    };

    let report =
        build_confirmed_order_report(121, "buy", &ack, Some(order), vec![fill], None, None);

    assert_eq!(report.execution_status, "completed");
    assert_eq!(report.external_order_id, "12345");
    assert_eq!(report.order_status, "FILLED");
    assert_eq!(report.filled_qty, Some(0.009));
    assert_eq!(report.fee_amount, Some(0.01020420));
    let filled_quote = report.filled_quote.unwrap();
    assert!((filled_quote - 20.4084).abs() < 0.00000001);
    let raw = report.raw_payload_json.unwrap();
    assert!(raw.contains("order_detail"));
    assert!(raw.contains("fills"));
}

#[test]
fn filled_live_open_with_required_stop_loss_stays_pending_protection_sync() {
    let instrument = Instrument::perp("ETH", "USDT");
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("12347".to_string()),
        client_order_id: Some("rqethopen3".to_string()),
        status: Some("FILLED".to_string()),
        raw: json!({"status":"FILLED","orderId":12347}),
    };
    let order = Order {
        exchange: ExchangeId::Binance,
        instrument,
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("12347".to_string()),
        client_order_id: Some("rqethopen3".to_string()),
        side: Some("BUY".to_string()),
        order_type: Some("MARKET".to_string()),
        price: Some("0".to_string()),
        size: Some("0.009".to_string()),
        filled_size: Some("0.009".to_string()),
        average_price: Some("2267.60000".to_string()),
        status: Some("FILLED".to_string()),
        created_at: Some(1),
        updated_at: Some(2),
        raw: json!({"status":"FILLED","executedQty":"0.009","avgPrice":"2267.60000"}),
    };
    let protection = ProtectionSyncContract::required(
        json!({
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.0,
                "direction": "long"
            }
        }),
        "buy",
    )
    .expect("valid protection contract");

    let report = build_confirmed_order_report(
        123,
        "buy",
        &ack,
        Some(order),
        vec![],
        None,
        Some(protection),
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

    assert_eq!(report.execution_status, "pending_protection_sync");
    assert_eq!(report.order_status, "FILLED");
    assert_eq!(
        report.error_message.as_deref(),
        Some("protective stop-loss required but protection order sync is not confirmed")
    );
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
        2200.0
    );
    assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
}

#[test]
fn protection_sync_confirmed_completes_task_without_allowing_repeat_open() {
    let protection = ProtectionSyncContract::required(
        json!({
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.0,
                "direction": "long"
            }
        }),
        "buy",
    )
    .expect("valid protection contract");
    let mut report = ExecutionTaskReportRequest::success(
        124,
        "binance",
        "12348",
        "buy",
        "FILLED",
        json!({"execution_status":"pending_protection_sync"}),
    );
    report.execution_status = "pending_protection_sync".to_string();
    report.error_message =
        Some("protective stop-loss required but protection order sync is not confirmed".into());

    protection.apply_outcome_to_report(
        &mut report,
        ProtectionSyncOutcome::confirmed("sl-rqethopen4", "query_order"),
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

    assert_eq!(report.execution_status, "completed");
    assert_eq!(report.error_message, None);
    assert_eq!(raw_payload["protection_sync"]["status"], "completed");
    assert_eq!(
        raw_payload["protection_sync"]["protective_order_external_id"],
        "sl-rqethopen4"
    );
    assert_eq!(
        raw_payload["protection_sync"]["protective_order_confirmed"],
        true
    );
    assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["protection_sync"]["repeat_open_order_allowed"],
        false
    );
}

#[test]
fn okx_attached_stop_loss_ack_without_algo_evidence_fails_protection() {
    let order_task = ExecutionOrderTask::from_task(&task(json!({
        "exchange": "okx",
        "symbol": "ETH-USDT-SWAP",
        "side": "buy",
        "size": "0.01",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 2200.0,
            "direction": "long"
        }
    })))
    .expect("valid OKX order task");
    let instrument = Instrument::perp("ETH", "USDT");
    let ack = OrderAck {
        exchange: ExchangeId::Okx,
        instrument: instrument.clone(),
        exchange_symbol: "ETH-USDT-SWAP".to_string(),
        order_id: Some("10001".to_string()),
        client_order_id: Some("rqtask10001".to_string()),
        status: Some("0".to_string()),
        raw: json!({"ordId":"10001","clOrdId":"rqtask10001","sCode":"0"}),
    };
    let order = Order {
        exchange: ExchangeId::Okx,
        instrument,
        exchange_symbol: "ETH-USDT-SWAP".to_string(),
        order_id: Some("10001".to_string()),
        client_order_id: Some("rqtask10001".to_string()),
        side: Some("buy".to_string()),
        order_type: Some("market".to_string()),
        price: None,
        size: Some("0.01".to_string()),
        filled_size: Some("0.01".to_string()),
        average_price: Some("2300".to_string()),
        status: Some("filled".to_string()),
        created_at: None,
        updated_at: None,
        raw: json!({"ordId":"10001","state":"filled"}),
    };

    let outcome = attached_stop_loss_order_ack_outcome(&order_task, &ack, Some(&order))
        .expect("OKX attached stop-loss should produce a protection outcome");

    match outcome {
        ProtectionSyncOutcome::Failed { reason, .. } => {
            assert_eq!(reason, "attached_stop_loss_ack_missing");
        }
        other => panic!("expected attached stop-loss evidence failure, got {other:?}"),
    }
}

#[test]
fn okx_and_bitget_attached_stop_loss_ack_evidence_confirms_protection() {
    for (exchange, raw) in [
        (
            "okx",
            json!({
                "ordId":"10002",
                "attachAlgoOrds":[{
                    "attachAlgoId":"rq-sl-10002",
                    "slTriggerPx":"2200",
                    "slOrdPx":"-1",
                    "slTriggerPxType":"last"
                }],
                "sCode":"0"
            }),
        ),
        (
            "bitget",
            json!({"orderId":"10003","clientOid":"rqtask10003","presetStopLossPrice":"2200"}),
        ),
    ] {
        let order_task = ExecutionOrderTask::from_task(&task(json!({
            "exchange": exchange,
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "size": "0.01",
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.0,
                "direction": "long"
            }
        })))
        .expect("valid attached stop-loss order task");
        let instrument = Instrument::perp("ETH", "USDT");
        let ack = OrderAck {
            exchange: order_task.exchange,
            exchange_symbol: instrument.symbol_for(order_task.exchange),
            instrument,
            order_id: Some(format!("{exchange}-10002")),
            client_order_id: Some(format!("rqtask-{exchange}-10002")),
            status: Some("FILLED".to_string()),
            raw,
        };

        let outcome = attached_stop_loss_order_ack_outcome(&order_task, &ack, None)
            .expect("attached stop-loss should produce a protection outcome");

        match outcome {
            ProtectionSyncOutcome::Confirmed { source, .. } => {
                assert_eq!(source, "place_order_attached_stop_loss_ack");
            }
            other => panic!("expected attached stop-loss confirmation, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn completed_news_protection_report_posts_safe_task_context_to_web() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    let mut task = task(json!({
        "exchange": "binance",
        "symbol": "ETH-USDT-SWAP",
        "source_signal_type": "news_event",
        "side": "buy",
        "size": "0.011",
        "risk_plan": {
            "protective_stop_loss_required": true,
            "selected_stop_loss_price": 2156.0,
            "entry_reference_price": 2200.0,
            "direction": "long"
        }
    }));
    task.id = 218;
    task.news_signal_id = Some(601);
    task.strategy_slug = "news_momentum".to_string();
    task.symbol = "ETH-USDT-SWAP".to_string();
    let instrument = Instrument::perp("ETH", "USDT");
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("8389766181876482454".to_string()),
        client_order_id: Some("rqtask218".to_string()),
        status: Some("FILLED".to_string()),
        raw: json!({"status":"FILLED","orderId":"8389766181876482454"}),
    };
    let order = Order {
        exchange: ExchangeId::Binance,
        instrument,
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("8389766181876482454".to_string()),
        client_order_id: Some("rqtask218".to_string()),
        side: Some("BUY".to_string()),
        order_type: Some("MARKET".to_string()),
        price: Some("0".to_string()),
        size: Some("0.011".to_string()),
        filled_size: Some("0.011".to_string()),
        average_price: Some("2200.00".to_string()),
        status: Some("FILLED".to_string()),
        created_at: Some(1),
        updated_at: Some(2),
        raw: json!({"status":"FILLED","executedQty":"0.011","avgPrice":"2200.00"}),
    };
    let protection = ProtectionSyncContract::required(task.request_payload_json.clone(), "buy")
        .expect("news task should carry a valid protection contract");
    let mut report = build_confirmed_order_report_for_task(
        &task,
        "buy",
        &ack,
        Some(order),
        vec![],
        None,
        Some(protection),
    );
    ProtectionSyncContract::required(task.request_payload_json.clone(), "buy")
        .unwrap()
        .apply_outcome_to_report(
            &mut report,
            ProtectionSyncOutcome::confirmed("2000000956163119", "query_order"),
        );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();

    let server = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 8192];
        let bytes = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        tx.send(request).unwrap();

        let body = r#"{"success":true,"data":{"task":{"id":218,"news_signal_id":601,"combo_id":9,"buyer_email":"buyer@example.com","strategy_slug":"news_momentum","symbol":"ETH-USDT-SWAP","task_type":"execute_signal","task_status":"completed","priority":3,"lease_owner":"worker","lease_until":null,"scheduled_at":"2026-04-23T12:00:00","request_payload_json":"{}","created_at":"2026-04-23T12:00:00","updated_at":"2026-04-23T12:00:00"},"attempt":{},"order_result":null,"trade_record":null}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url: format!("http://{}", addr),
        internal_secret: "dev-secret".to_string(),
    })
    .unwrap();
    let response = client.report_result(report).await.unwrap();

    server.await.unwrap();
    assert_eq!(response.task.id, 218);
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/commerce/internal/execution-results HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .expect("mock Web request should include JSON body");
    let posted: Value = serde_json::from_str(body).unwrap();
    assert_eq!(posted["task_id"], 218);
    assert_eq!(posted["execution_status"], "completed");
    let raw_payload: Value =
        serde_json::from_str(posted["raw_payload_json"].as_str().unwrap()).unwrap();
    assert_eq!(raw_payload["execution_task"]["news_signal_id"], 601);
    assert_eq!(
        raw_payload["execution_task"]["source_signal_type"],
        "news_event"
    );
    assert_eq!(
        raw_payload["execution_task"]["strategy_slug"],
        "news_momentum"
    );
    assert_eq!(raw_payload["protection_sync"]["status"], "completed");
    assert_eq!(
        raw_payload["protection_sync"]["protective_order_external_id"],
        "2000000956163119"
    );
    assert_eq!(
        raw_payload["protection_sync"]["protective_order_confirmed"],
        true
    );
    assert!(raw_payload.get("api_secret").is_none());
    assert!(raw_payload.get("api_key").is_none());
}

#[test]
fn protection_sync_failure_marks_protective_order_failed_without_allowing_repeat_open() {
    let protection = ProtectionSyncContract::required(
        json!({
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.0,
                "direction": "long"
            }
        }),
        "buy",
    )
    .expect("valid protection contract");
    let mut report = ExecutionTaskReportRequest::success(
        125,
        "binance",
        "12349",
        "buy",
        "FILLED",
        json!({"execution_status":"pending_protection_sync"}),
    );
    report.execution_status = "pending_protection_sync".to_string();

    protection.apply_outcome_to_report(
        &mut report,
        ProtectionSyncOutcome::failed("place_protective_order", "STOP_MARKET rejected"),
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

    assert_eq!(report.execution_status, "protective_order_failed");
    assert_eq!(
        report.error_message.as_deref(),
        Some("STOP_MARKET rejected")
    );
    assert_eq!(
        raw_payload["protection_sync"]["status"],
        "protective_order_failed"
    );
    assert_eq!(
        raw_payload["protection_sync"]["reason"],
        "place_protective_order"
    );
    assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["protection_sync"]["repeat_open_order_allowed"],
        false
    );
}

#[test]
fn protection_sync_uncertain_stays_pending_without_allowing_repeat_open() {
    let protection = ProtectionSyncContract::required(
        json!({
            "risk_plan": {
                "protective_stop_loss_required": true,
                "selected_stop_loss_price": 2200.0,
                "direction": "long"
            }
        }),
        "buy",
    )
    .expect("valid protection contract");
    let mut report = ExecutionTaskReportRequest::success(
        126,
        "binance",
        "12350",
        "buy",
        "FILLED",
        json!({"execution_status":"pending_protection_sync"}),
    );
    report.execution_status = "pending_protection_sync".to_string();

    protection.apply_outcome_to_report(
        &mut report,
        ProtectionSyncOutcome::uncertain("query_protective_order", "read timeout"),
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();

    assert_eq!(report.execution_status, "pending_protection_sync");
    assert_eq!(report.error_message.as_deref(), Some("read timeout"));
    assert_eq!(
        raw_payload["protection_sync"]["status"],
        "pending_protection_sync"
    );
    assert_eq!(
        raw_payload["protection_sync"]["reason"],
        "query_protective_order"
    );
    assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["protection_sync"]["repeat_open_order_allowed"],
        false
    );
}

#[test]
fn confirmed_live_order_report_keeps_unfilled_order_pending_confirmation() {
    let instrument = Instrument::perp("ETH", "USDT");
    let ack = OrderAck {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("12346".to_string()),
        client_order_id: Some("rqethopen2".to_string()),
        status: Some("NEW".to_string()),
        raw: json!({"status":"NEW","orderId":12346}),
    };
    let order = Order {
        exchange: ExchangeId::Binance,
        instrument,
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("12346".to_string()),
        client_order_id: Some("rqethopen2".to_string()),
        side: Some("BUY".to_string()),
        order_type: Some("MARKET".to_string()),
        price: Some("0".to_string()),
        size: Some("0.009".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("NEW".to_string()),
        created_at: Some(1),
        updated_at: Some(2),
        raw: json!({"status":"NEW","executedQty":"0","avgPrice":"0"}),
    };

    let report = build_confirmed_order_report(122, "buy", &ack, Some(order), vec![], None, None);

    assert_eq!(report.execution_status, "pending_confirmation");
    assert_eq!(report.external_order_id, "12346");
    assert_eq!(report.order_status, "NEW");
    assert_eq!(report.filled_qty, Some(0.0));
    let raw = report.raw_payload_json.unwrap();
    assert!(raw.contains("order_detail"));
    assert!(raw.contains("pending_confirmation"));
}

#[test]
fn duplicate_client_order_id_errors_are_reconciled_by_querying_existing_order() {
    assert!(is_duplicate_client_order_id_error(
        "binance error -4111: Duplicate clientOrderId"
    ));
    assert!(is_duplicate_client_order_id_error(
        "client order id is duplicate"
    ));
    assert!(is_duplicate_client_order_id_error(
        "clientOrderId has already been used"
    ));
    assert!(!is_duplicate_client_order_id_error(
        "insufficient margin balance"
    ));
}

#[test]
fn duplicate_client_order_id_reconciliation_ack_keeps_original_client_order_id() {
    let request = OrderPlacementRequest {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        size: "0.009".to_string(),
        price: None,
        margin_mode: None,
        margin_coin: None,
        position_side: Some("long".to_string()),
        trade_side: Some("open".to_string()),
        client_order_id: Some("rqtask42".to_string()),
        reduce_only: None,
        time_in_force: None,
        attached_stop_loss_price: None,
    };

    let ack = duplicate_client_order_id_reconciliation_ack(&request)
        .expect("stable client order id should be enough to reconcile");

    assert_eq!(ack.exchange, ExchangeId::Binance);
    assert_eq!(ack.order_id, None);
    assert_eq!(ack.client_order_id.as_deref(), Some("rqtask42"));
    assert_eq!(ack.status.as_deref(), Some("duplicate_client_order_id"));
    assert_eq!(
        ack.raw["reconciliation"]["action"],
        "query_existing_order_by_client_order_id"
    );
    assert_eq!(ack.raw["reconciliation"]["place_order_retried"], false);
}

#[test]
fn pre_place_client_order_lookup_uses_stable_client_order_id_before_new_order() {
    let request = OrderPlacementRequest {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        size: "0.011".to_string(),
        price: None,
        margin_mode: None,
        margin_coin: Some("USDT".to_string()),
        position_side: Some("long".to_string()),
        trade_side: Some("open".to_string()),
        client_order_id: Some("rqtask218".to_string()),
        reduce_only: None,
        time_in_force: None,
        attached_stop_loss_price: None,
    };

    let lookup = pre_place_client_order_lookup(&request)
        .expect("stable client order id should be queried before placing a retry order");

    assert_eq!(lookup.query.client_order_id.as_deref(), Some("rqtask218"));
    assert_eq!(lookup.query.margin_coin.as_deref(), Some("USDT"));
    assert_eq!(lookup.ack.client_order_id.as_deref(), Some("rqtask218"));
    assert_eq!(
        lookup.ack.raw["reconciliation"]["action"],
        "query_existing_order_before_place_order"
    );
    assert_eq!(
        lookup.ack.raw["reconciliation"]["place_order_allowed"],
        false
    );
    assert_eq!(
        lookup.ack.raw["reconciliation"]["place_order_retried"],
        false
    );
}

#[test]
fn pre_place_client_order_check_only_allows_place_after_order_not_found() {
    assert!(is_order_not_found_for_client_order_preflight(
        "binance error -2013: Order does not exist."
    ));
    assert!(is_order_not_found_for_client_order_preflight(
        "order not found by clientOrderId"
    ));
    assert!(!is_order_not_found_for_client_order_preflight(
        "request timeout while querying order"
    ));
    assert!(!is_order_not_found_for_client_order_preflight(
        "insufficient permission for order query"
    ));
}

#[test]
fn execute_signal_blocks_foreign_rqtask_client_order_id_before_live_mutation() {
    let request = OrderPlacementRequest {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        size: "0.011".to_string(),
        price: None,
        margin_mode: None,
        margin_coin: Some("USDT".to_string()),
        position_side: Some("long".to_string()),
        trade_side: Some("open".to_string()),
        client_order_id: Some("rqtask218".to_string()),
        reduce_only: None,
        time_in_force: None,
        attached_stop_loss_price: None,
    };

    let report = client_order_id_owner_violation_report(999, "execute_signal", "buy", &request)
        .expect("foreign rqtask client id must fail closed before live mutation");
    let raw_payload: Value =
        serde_json::from_str(report.raw_payload_json.as_deref().unwrap()).unwrap();

    assert_eq!(report.execution_status, "failed");
    assert_eq!(report.external_order_id, "failed-task-999");
    assert_eq!(
        report.error_message.as_deref(),
        Some("client_order_id rqtask218 belongs to task 218, not task 999")
    );
    assert_eq!(raw_payload["stage"], "client_order_id_owner_check");
    assert_eq!(
        raw_payload["reconciliation"]["reason"],
        "client_order_id_owner_mismatch"
    );
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["mutation_allowed"], false);
    assert_eq!(raw_payload["protection_sync_allowed"], false);
}

#[tokio::test]
async fn dry_run_worker_records_audit_and_checkpoint_through_repository() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-a".to_string(),
            lease_limit: 1,
            dry_run: true,
            default_exchange: ExchangeId::Okx,
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
    )
    .with_audit_repository(repository.clone());
    let task = task(json!({
        "exchange": "okx",
        "symbol": "BTC-USDT-SWAP",
        "side": "buy",
        "size": "0.01",
        "api_key": "plain-api-key"
    }));
    let request = ExecutionOrderTask::from_task(&task)
        .unwrap()
        .to_order_request()
        .unwrap();

    worker
        .record_checkpoint(
            "leased",
            Some(task.id),
            json!({"api_secret": "plain-secret"}),
        )
        .await;
    let ack = worker
        .place_order_with_audit(&task, &worker.gateway, request)
        .await
        .unwrap();

    assert_eq!(ack.status.as_deref(), Some("dry_run"));
    let checkpoints = repository.checkpoints.lock().unwrap();
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].worker_status, "leased");
    assert_eq!(
        checkpoints[0].checkpoint_value["api_secret"],
        "***REDACTED***"
    );
    drop(checkpoints);

    let audits = repository.audits.lock().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].request_status, "completed");
    assert_eq!(
        audits[0].request_payload["task"]["request_payload_json"]["api_key"],
        "***REDACTED***"
    );
    assert!(!audits[0]
        .request_payload
        .to_string()
        .contains("plain-api-key"));
}

#[tokio::test]
async fn report_result_failure_records_replay_evidence_without_retrying_order() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-report-replay".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending_confirmation".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: true,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());
    let report = ExecutionTaskReportRequest {
        task_id: 42,
        execution_status: "pending_confirmation".to_string(),
        exchange: "binance".to_string(),
        external_order_id: "12345".to_string(),
        order_side: "buy".to_string(),
        order_status: "NEW".to_string(),
        filled_qty: Some(0.0),
        filled_quote: Some(0.0),
        fee_amount: None,
        profit_usdt: None,
        executed_at: None,
        error_message: Some("waiting for exchange fill".to_string()),
        raw_payload_json: Some(
            r#"{"client_order_id":"rqtask42","api_secret":"plain-report-secret"}"#.to_string(),
        ),
    };

    worker
        .record_report_result_failure(42, &report, "web api_secret outage", "report_result")
        .await;

    let audits = repository.audits.lock().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].endpoint, "web.report_result");
    assert_eq!(audits[0].request_status, "failed");
    assert_eq!(
        audits[0].request_payload["report"]["external_order_id"],
        "12345"
    );
    assert_eq!(
        audits[0].request_payload["report"]["raw_payload_json"]["api_secret"],
        "***REDACTED***"
    );
    assert_eq!(
        audits[0].response_payload["replay_action"],
        "retry_report_result_only"
    );
    assert_eq!(audits[0].response_payload["place_order_allowed"], false);
    assert_eq!(audits[0].error_message, "redacted sensitive error");
    assert!(!audits[0]
        .request_payload
        .to_string()
        .contains("plain-report-secret"));
    drop(audits);

    let checkpoints = repository.checkpoints.lock().unwrap();
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].worker_status, "report_failed");
    assert_eq!(
        checkpoints[0].checkpoint_value["replay"]["action"],
        "retry_report_result_only"
    );
    assert_eq!(
        checkpoints[0].checkpoint_value["replay"]["place_order_allowed"],
        false
    );
}

#[tokio::test]
async fn report_replay_mode_reposts_stored_report_without_order_placement() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let server = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 8192];
        let bytes = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        tx.send(request).unwrap();

        let body = r#"{"success":true,"data":{"task":{"id":42,"news_signal_id":null,"strategy_signal_id":null,"combo_id":9,"buyer_email":"buyer@example.com","strategy_slug":"news_momentum","symbol":"ETH-USDT-SWAP","task_type":"execute_signal","task_status":"pending_confirmation","priority":1,"lease_owner":null,"lease_until":null,"scheduled_at":"2026-04-23T12:00:00","request_payload_json":"{}","created_at":"2026-04-23T12:00:00","updated_at":"2026-04-23T12:00:00"},"attempt":{},"order_result":{},"trade_record":null}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let repository = Arc::new(CapturingAuditRepository::default());
    repository
        .report_replay_candidates
        .lock()
        .unwrap()
        .push(ReportResultReplayCandidate {
            request_id: "report-task-42-12345".to_string(),
            report: ExecutionTaskReportRequest {
                task_id: 42,
                execution_status: "pending_confirmation".to_string(),
                exchange: "binance".to_string(),
                external_order_id: "12345".to_string(),
                order_side: "buy".to_string(),
                order_status: "NEW".to_string(),
                filled_qty: Some(0.0),
                filled_quote: Some(0.0),
                fee_amount: None,
                profit_usdt: None,
                executed_at: None,
                error_message: Some("waiting for fill".to_string()),
                raw_payload_json: Some(r#"{"replay":{"place_order_allowed":false}}"#.to_string()),
            },
        });
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "local-dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-report-replay".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: true,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());

    let handled = worker.run_once().await.unwrap();

    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/commerce/internal/execution-results HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: local-dev-secret"));
    assert!(request.contains(r#""task_id":42"#));
    assert!(request.contains(r#""external_order_id":"12345""#));
    assert!(!request.contains("/api/commerce/internal/execution-tasks/lease"));
    assert_eq!(handled, 1);

    let audits = repository.audits.lock().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].endpoint, "web.report_result");
    assert_eq!(audits[0].request_status, "replayed");
    assert_eq!(audits[0].response_payload["replay_status"], "completed");
    assert_eq!(audits[0].response_payload["place_order_allowed"], false);
    drop(audits);

    let checkpoints = repository.checkpoints.lock().unwrap();
    assert!(checkpoints
        .iter()
        .any(|checkpoint| checkpoint.worker_status == "report_replayed"));
    assert!(checkpoints
        .iter()
        .all(|checkpoint| checkpoint.checkpoint_value["place_order_allowed"] != true));
}

#[tokio::test]
async fn report_replay_mode_writes_batch_summary_and_playbook_handoff() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let server = tokio::task::spawn_blocking(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            if index == 0 {
                let body = r#"{"success":true,"data":{"task":{"id":42,"news_signal_id":null,"strategy_signal_id":null,"combo_id":9,"buyer_email":"buyer@example.com","strategy_slug":"news_momentum","symbol":"ETH-USDT-SWAP","task_type":"execute_signal","task_status":"pending_confirmation","priority":1,"lease_owner":null,"lease_until":null,"scheduled_at":"2026-04-23T12:00:00","request_payload_json":"{}","created_at":"2026-04-23T12:00:00","updated_at":"2026-04-23T12:00:00"},"attempt":{},"order_result":{},"trade_record":null}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            } else {
                let body = r#"{"success":false,"error":"web unavailable"}"#;
                let response = format!(
                    "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        }
    });
    let repository = Arc::new(CapturingAuditRepository::default());
    repository.report_replay_candidates.lock().unwrap().extend([
        ReportResultReplayCandidate {
            request_id: "report-task-42-12345".to_string(),
            report: ExecutionTaskReportRequest {
                task_id: 42,
                execution_status: "pending_confirmation".to_string(),
                exchange: "binance".to_string(),
                external_order_id: "12345".to_string(),
                order_side: "buy".to_string(),
                order_status: "NEW".to_string(),
                filled_qty: Some(0.0),
                filled_quote: Some(0.0),
                fee_amount: None,
                profit_usdt: None,
                executed_at: None,
                error_message: Some("waiting for fill".to_string()),
                raw_payload_json: None,
            },
        },
        ReportResultReplayCandidate {
            request_id: "report-task-43-67890".to_string(),
            report: ExecutionTaskReportRequest {
                task_id: 43,
                execution_status: "pending_confirmation".to_string(),
                exchange: "binance".to_string(),
                external_order_id: "67890".to_string(),
                order_side: "buy".to_string(),
                order_status: "NEW".to_string(),
                filled_qty: Some(0.0),
                filled_quote: Some(0.0),
                fee_amount: None,
                profit_usdt: None,
                executed_at: None,
                error_message: Some("waiting for fill".to_string()),
                raw_payload_json: None,
            },
        },
    ]);
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "local-dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-report-replay".to_string(),
            lease_limit: 10,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: true,
            report_replay_max_per_run: 2,
            report_replay_failure_backoff_seconds: 900,
            report_replay_throttle_ms: 0,
        },
    )
    .with_audit_repository(repository.clone());

    let handled = worker.run_once().await.unwrap();

    server.await.unwrap();
    let requests = [rx.recv().unwrap(), rx.recv().unwrap()];
    assert!(requests
        .iter()
        .all(|request| request.starts_with("POST /api/commerce/internal/execution-results")));
    assert_eq!(handled, 2);
    assert_eq!(
        repository.report_replay_queries.lock().unwrap().as_slice(),
        &[(2, 900)]
    );

    let checkpoints = repository.checkpoints.lock().unwrap();
    let final_checkpoint = checkpoints.last().unwrap();
    assert_eq!(final_checkpoint.worker_status, "idle");
    assert_eq!(
        final_checkpoint.checkpoint_value["report_replay"]["leased_count"],
        2
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["report_replay"]["attempted_count"],
        2
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["report_replay"]["replayed_count"],
        1
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["report_replay"]["failed_count"],
        1
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["report_replay"]["failure_backoff_seconds"],
        900
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["health_handoff"]["section"],
        "quant_worker_checkpoint_audit"
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["health_handoff"]["status"],
        "warn"
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["operator_playbook_summary"]["item_count"],
        1
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["operator_playbook_summary"]["items"][0]["code"],
        "QUANT_REPORT_REPLAY_FAILED"
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["operator_playbook_summary"]["items"][0]
            ["admin_link_target"],
        "admin.full_product_health.quant_worker_checkpoint_audit"
    );
    assert_eq!(
        final_checkpoint.checkpoint_value["place_order_allowed"],
        false
    );
}

#[tokio::test]
async fn default_noop_audit_repository_does_not_block_worker_audit_paths() {
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-noop".to_string(),
            lease_limit: 1,
            dry_run: true,
            default_exchange: ExchangeId::Okx,
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
    let task = task(json!({
        "exchange": "okx",
        "symbol": "BTC-USDT-SWAP",
        "side": "buy",
        "size": "0.01",
        "api_secret": "plain-api-secret"
    }));
    let request = ExecutionOrderTask::from_task(&task)
        .unwrap()
        .to_order_request()
        .unwrap();

    worker
        .record_checkpoint(
            "leased",
            Some(task.id),
            json!({"access_token": "plain-access-token"}),
        )
        .await;
    let ack = worker
        .place_order_with_audit(&task, &worker.gateway, request)
        .await
        .unwrap();

    assert_eq!(ack.exchange.as_str(), "okx");
    assert_eq!(ack.status.as_deref(), Some("dry_run"));
}
