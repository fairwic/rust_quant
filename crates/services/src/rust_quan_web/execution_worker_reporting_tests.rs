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
use crate::rust_quan_web::execution_take_profit::existing_take_profit_order_request_error;
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
fn protection_sync_cancel_failed_requires_manual_cleanup_without_repeat_open() {
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
    let mut report = ExecutionTaskReportRequest::failed(
        127,
        "binance",
        "buy",
        "prearmed protective stop-loss was not confirmed; refusing main order",
        json!({"execution_status":"failed"}),
    );
    protection.apply_outcome_to_report(
        &mut report,
        ProtectionSyncOutcome::cancel_failed(
            "cancel_unconfirmed_prearmed_protective_order",
            "protective cancel failed after unconfirmed prearm",
        ),
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "protective_cancel_failed");
    assert_eq!(
        report.error_message.as_deref(),
        Some("protective cancel failed after unconfirmed prearm")
    );
    assert_eq!(
        raw_payload["protection_sync"]["status"],
        "protective_cancel_failed"
    );
    assert_eq!(
        raw_payload["protection_sync"]["manual_cleanup_required"],
        true
    );
    assert_eq!(raw_payload["protection_sync"]["place_order_allowed"], false);
    assert_eq!(
        raw_payload["protection_sync"]["repeat_open_order_allowed"],
        false
    );
}
#[test]
fn take_profit_sync_marks_stop_reset_monitor_required_without_changing_completed_report() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
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
    let mut report = ExecutionTaskReportRequest::success(
        42,
        "binance",
        "open-42",
        "buy",
        "FILLED",
        json!({"execution_status":"completed"}),
    );
    report.execution_status = "completed".to_string();
    apply_take_profit_sync_outcome_to_report(
        &mut report,
        &order_task,
        TakeProfitSyncOutcome::Confirmed {
            orders: vec![json!({
                "client_order_id": "rq-tp-42-1",
                "order_status": "NEW"
            })],
        },
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "completed");
    assert_eq!(raw_payload["take_profit_sync"]["status"], "completed");
    assert_eq!(
        raw_payload["take_profit_stop_reset"]["status"],
        "pending_take_profit_monitor"
    );
    assert_eq!(
        raw_payload["take_profit_stop_reset"]["monitor_required"],
        true
    );
    assert_eq!(
        raw_payload["take_profit_stop_reset"]["legs"][0]["take_profit_client_order_id"],
        "rq-tp-42-1"
    );
    assert_eq!(
        raw_payload["take_profit_stop_reset"]["legs"][0]["stop_after_fill_price"],
        100.0
    );
}
#[test]
fn take_profit_stop_reset_cancel_failure_does_not_mark_report_completed() {
    let mut report = ExecutionTaskReportRequest::success(
        42,
        "binance",
        "open-42",
        "buy",
        "FILLED",
        json!({"execution_status":"completed"}),
    );
    report.execution_status = "completed".to_string();
    apply_take_profit_stop_reset_outcome_to_report(
        &mut report,
        TakeProfitStopResetOutcome::Confirmed {
            reset: json!({
                "status": "completed",
                "protective_order_confirmed": true,
                "old_protective_order_cancelled": false,
                "old_protective_cancel_failed": true,
                "manual_cleanup_required": true,
                "old_cancel_error_message": "cancel rejected"
            }),
        },
        true,
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "take_profit_stop_reset_failed");
    assert_eq!(
        report.error_message.as_deref(),
        Some("old protective stop cancel failed after new stop was confirmed; manual cleanup required")
    );
    assert_eq!(
        raw_payload["take_profit_stop_reset"]["stage"],
        "cancel_old_protective_order"
    );
    assert_eq!(
        raw_payload["take_profit_stop_reset"]["reset_attempt"]["manual_cleanup_required"],
        true
    );
}
#[test]
fn take_profit_sync_failure_marks_retry_required_without_changing_completed_report() {
    let task = task(json!({
        "exchange": "binance",
        "symbol": "ETHUSDT",
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
    let mut report = ExecutionTaskReportRequest::success(
        42,
        "binance",
        "open-42",
        "buy",
        "FILLED",
        json!({"execution_status":"completed"}),
    );
    report.execution_status = "completed".to_string();
    apply_take_profit_sync_outcome_to_report(
        &mut report,
        &order_task,
        TakeProfitSyncOutcome::Failed {
            stage: "place_take_profit_order".to_string(),
            message: "network timeout".to_string(),
            submitted_orders: vec![json!({
                "client_order_id": "rq-tp-42-1",
                "order_status": "NEW"
            })],
        },
    );
    let raw_payload =
        serde_json::from_str::<Value>(report.raw_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(report.execution_status, "completed");
    assert_eq!(
        raw_payload["take_profit_sync"]["status"],
        "take_profit_order_retry_required"
    );
    assert_eq!(raw_payload["take_profit_sync"]["retry_required"], true);
    assert_eq!(
        raw_payload["take_profit_sync"]["submitted_orders"][0]["client_order_id"],
        "rq-tp-42-1"
    );
}
#[test]
fn take_profit_order_ack_status_rejects_terminal_failure_before_marking_sync_completed() {
    let rejected = OrderAck {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("tp-rejected".to_string()),
        client_order_id: Some("rq-tp-42-1".to_string()),
        status: Some("REJECTED".to_string()),
        raw: json!({"status": "REJECTED"}),
    };
    let error = take_profit_order_ack_status_error(&rejected)
        .expect("terminal rejected take-profit ack must be retryable");
    assert!(error.contains("REJECTED"));
    for status in ["NEW", "PARTIALLY_FILLED", "FILLED", "0"] {
        let accepted = OrderAck {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("ETH", "USDT"),
            exchange_symbol: "ETHUSDT".to_string(),
            order_id: Some(format!("tp-{status}")),
            client_order_id: Some("rq-tp-42-1".to_string()),
            status: Some(status.to_string()),
            raw: json!({"status": status}),
        };
        assert_eq!(take_profit_order_ack_status_error(&accepted), None);
    }
    let dry_run = OrderAck {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("dry-run-rq-tp-42-1".to_string()),
        client_order_id: Some("rq-tp-42-1".to_string()),
        status: Some("dry_run".to_string()),
        raw: json!({"dry_run": true}),
    };
    let error = take_profit_order_ack_status_error(&dry_run)
        .expect("dry-run take-profit ack must not be marked as live sync completed");
    assert!(error.contains("dry_run"));
}
#[test]
fn existing_take_profit_order_status_rejects_cancelled_order_before_skip_replacement() {
    let cancelled = Order {
        exchange: ExchangeId::Binance,
        instrument: Instrument::perp("ETH", "USDT"),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("tp-cancelled".to_string()),
        client_order_id: Some("rq-tp-42-1".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("LIMIT".to_string()),
        price: Some("106".to_string()),
        size: Some("0.007".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("CANCELED".to_string()),
        created_at: None,
        updated_at: None,
        raw: json!({"status": "CANCELED"}),
    };
    let error = existing_take_profit_order_status_error(&cancelled)
        .expect("terminal cancelled take-profit order must not block replacement");
    assert!(error.contains("CANCELED"));
    assert!(existing_take_profit_order_allows_replacement(&cancelled));
    for status in ["NEW", "PARTIALLY_FILLED", "FILLED", "live", "open"] {
        let accepted = Order {
            status: Some(status.to_string()),
            raw: json!({"status": status}),
            ..cancelled.clone()
        };
        assert_eq!(existing_take_profit_order_status_error(&accepted), None);
        assert!(!existing_take_profit_order_allows_replacement(&accepted));
    }
}
#[test]
fn existing_take_profit_order_rejects_active_order_with_mismatched_request_terms() {
    let instrument = Instrument::perp("ETH", "USDT");
    let existing = Order {
        exchange: ExchangeId::Binance,
        instrument: instrument.clone(),
        exchange_symbol: "ETHUSDT".to_string(),
        order_id: Some("tp-active".to_string()),
        client_order_id: Some("rq-tp-42-1".to_string()),
        side: Some("SELL".to_string()),
        order_type: Some("LIMIT".to_string()),
        price: Some("105".to_string()),
        size: Some("0.008".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("NEW".to_string()),
        created_at: None,
        updated_at: None,
        raw: json!({"status": "NEW"}),
    };
    let request = OrderPlacementRequest {
        exchange: ExchangeId::Binance,
        instrument,
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        size: "0.007".to_string(),
        price: Some("106".to_string()),
        margin_mode: None,
        margin_coin: Some("USDT".to_string()),
        position_side: Some("long".to_string()),
        trade_side: Some("close".to_string()),
        client_order_id: Some("rq-tp-42-1".to_string()),
        reduce_only: Some(true),
        time_in_force: Some(TimeInForce::Gtc),
        attached_stop_loss_price: None,
    };
    let error = existing_take_profit_order_request_error(&existing, &request)
        .expect("active TP order with mismatched price/size must not be treated as synced");
    assert!(error.contains("mismatch"));
    assert!(error.contains("price"));
    assert!(error.contains("size"));
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
    let raw_payload = serde_json::from_str::<serde_json::Value>(&raw).unwrap();
    assert_eq!(raw_payload["place_order_allowed"], false);
    assert_eq!(raw_payload["repeat_open_order_allowed"], false);
}
include!("execution_worker_reporting_client_order_tests.rs");
include!("execution_worker_reporting_audit_tests.rs");
