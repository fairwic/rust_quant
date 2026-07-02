use super::*;
#[tokio::test]
async fn report_exchange_account_snapshot_uses_internal_post_contract() {
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
        let body = r#"{"success":true,"data":{"combo_id":85,"buyer_email":"buyer@example.com","exchange":"OKX","symbol":"BTC-USDT-SWAP","source_ref":"rq:acct:v1:ex=okx:combo=85:sym=BTC-USDT-SWAP","snapshot_at":"2026-06-18T02:30:00","orders_upserted":1,"trades_upserted":1,"positions_upserted":1,"position_history_upserted":1,"balances_upserted":1,"bills_upserted":1}}"#;
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
    let response = client
        .report_exchange_account_snapshot(ExchangeAccountSnapshotReportRequest {
            combo_id: 85,
            buyer_email: "buyer@example.com".to_string(),
            exchange: "okx".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            source_ref: "rq:acct:v1:ex=okx:combo=85:sym=BTC-USDT-SWAP".to_string(),
            snapshot_at: Some("2026-06-18T02:30:00".to_string()),
            orders: vec![ExchangeAccountOrderSnapshotInput {
                external_order_id: "3631557801300238336".to_string(),
                order_side: "buy".to_string(),
                order_status: "filled".to_string(),
                price: Some(66000.0),
                filled_qty: Some(0.01),
                filled_quote: Some(660.0),
                fee_amount: Some(0.33),
                raw_payload_json: Some(r#"{"ordId":"3631557801300238336"}"#.to_string()),
                observed_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            trades: vec![ExchangeAccountTradeSnapshotInput {
                external_trade_id: "211849844".to_string(),
                external_order_id: Some("3631557801300238336".to_string()),
                side: "buy".to_string(),
                quantity: Some(0.01),
                quote_amount: Some(660.0),
                fee_amount: Some(0.33),
                price: Some(66000.0),
                raw_payload_json: Some(r#"{"tradeId":"211849844"}"#.to_string()),
                executed_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            positions: vec![ExchangeAccountPositionSnapshotInput {
                side: "long".to_string(),
                quantity: 0.01,
                quote_amount: Some(660.0),
                leverage: Some(3.0),
                margin_mode: Some("isolated".to_string()),
                liquidation_price: Some(52000.0),
                margin_ratio: None,
                unrealized_pnl: Some(4.2),
                protective_order_status: Some("exchange_manual".to_string()),
                raw_payload_json: Some(r#"{"pos":"0.01"}"#.to_string()),
                snapshot_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            position_history: vec![ExchangeAccountPositionHistorySnapshotInput {
                external_position_id: "okx-position-1".to_string(),
                side: Some("long".to_string()),
                direction: Some("long".to_string()),
                close_type: Some("2".to_string()),
                margin_mode: Some("cross".to_string()),
                leverage: Some(3.0),
                open_avg_price: Some(0.6208),
                close_avg_price: Some(0.6047),
                open_max_position: Some(1.0),
                close_total_position: Some(1.0),
                realized_pnl_usdt: Some(-0.01),
                pnl_usdt: Some(-0.01),
                pnl_ratio: Some(-0.0817),
                fee_usdt: Some(-0.0002),
                funding_fee_usdt: Some(0.0),
                liquidation_penalty_usdt: Some(0.0),
                raw_payload_json: Some(r#"{"posId":"okx-position-1"}"#.to_string()),
                opened_at: Some("2026-06-18T00:30:00".to_string()),
                closed_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            balances: vec![ExchangeAccountBalanceSnapshotInput {
                asset: "USDT".to_string(),
                wallet_balance: Some(8211.49),
                available_balance: Some(6400.25),
                equity_usdt: Some(8211.49),
                raw_payload_json: Some(r#"{"ccy":"USDT","eqUsd":"8211.49"}"#.to_string()),
                snapshot_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            bills: vec![ExchangeAccountBillSnapshotInput {
                external_bill_id: "okx-bill-1".to_string(),
                asset: "USDT".to_string(),
                balance_change: Some(9.7),
                balance_change_usdt: None,
                balance_after: Some(8211.49),
                fee_amount: Some(-0.3),
                fee_usdt: None,
                pnl_amount: Some(10.0),
                pnl_usdt: None,
                bill_type: Some("2".to_string()),
                bill_sub_type: Some("1".to_string()),
                external_order_id: Some("3631557801300238336".to_string()),
                external_trade_id: Some("211849844".to_string()),
                raw_payload_json: Some(r#"{"billId":"okx-bill-1"}"#.to_string()),
                bill_at: Some("2026-06-18T02:30:00".to_string()),
            }],
        })
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/commerce/internal/exchange-account-snapshots HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert!(request.contains(r#""combo_id":85"#));
    assert!(request.contains(r#""buyer_email":"buyer@example.com""#));
    assert!(request.contains(r#""external_order_id":"3631557801300238336""#));
    assert!(request.contains(r#""external_trade_id":"211849844""#));
    assert!(request.contains(r#""external_position_id":"okx-position-1""#));
    assert!(request.contains(r#""balances":[{"asset":"USDT""#));
    assert!(request.contains(r#""bills":[{"external_bill_id":"okx-bill-1""#));
    assert!(!request.contains("plain-api-secret"));
    assert_eq!(response.orders_upserted, 1);
    assert_eq!(response.trades_upserted, 1);
    assert_eq!(response.positions_upserted, 1);
    assert_eq!(response.position_history_upserted, 1);
    assert_eq!(response.balances_upserted, 1);
    assert_eq!(response.bills_upserted, 1);
}
#[tokio::test]
async fn resolve_user_exchange_config_uses_internal_get_contract() {
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
        let body = r#"{"success":true,"data":{"buyer_email":"buyer@example.com","exchange":"OKX","api_key":"plain-api-key","api_secret":"plain-api-secret","passphrase":"plain-passphrase","simulated":false}}"#;
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
    let config = client
        .resolve_user_exchange_config("buyer@example.com", "OKX")
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with(
        "GET /api/commerce/internal/api-credentials/resolve?buyer_email=buyer%40example.com&exchange=OKX HTTP/1.1"
    ));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert_eq!(config.buyer_email, "buyer@example.com");
    assert_eq!(config.api_key, "plain-api-key");
    assert_eq!(config.api_secret, "plain-api-secret");
    assert_eq!(config.passphrase.as_deref(), Some("plain-passphrase"));
    assert!(!config.simulated);
}
#[tokio::test]
async fn resolve_user_exchange_config_for_credential_uses_exact_internal_get_contract() {
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
        let body = r#"{"success":true,"data":{"buyer_email":"buyer@example.com","exchange":"OKX","api_key":"plain-api-key","api_secret":"plain-api-secret","passphrase":"plain-passphrase","simulated":false}}"#;
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
    let config = client
        .resolve_user_exchange_config_for_credential("buyer@example.com", "OKX", 8801)
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with(
        "GET /api/commerce/internal/api-credentials/resolve?buyer_email=buyer%40example.com&exchange=OKX&credential_id=8801 HTTP/1.1"
    ));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert_eq!(config.buyer_email, "buyer@example.com");
    assert_eq!(config.api_key, "plain-api-key");
    assert_eq!(config.api_secret, "plain-api-secret");
    assert_eq!(config.passphrase.as_deref(), Some("plain-passphrase"));
    assert!(!config.simulated);
}
#[tokio::test]
async fn check_internal_api_credential_uses_internal_post_contract() {
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
        let body = r#"{"success":true,"data":{"id":42,"exchange":"OKX","api_key_mask":"okx_***_tail","permission_scope":"trade","status":"active","credential_envelope_ready":true,"last_check_at":"2026-06-05T08:00:00","last_check_code":"signed_exchange_preflight_passed","last_check_message":"ok","created_at":"2026-06-05T07:00:00","updated_at":"2026-06-05T08:00:00","execution_readiness":{"can_execute":true,"blocker_code":null,"blocker_message":null,"next_action_label":null,"next_action_href":null}}}"#;
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
    let summary = client.check_internal_api_credential(42).await.unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/commerce/internal/api-credentials/42/check HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert!(request.contains("content-type: application/json"));
    assert!(!request.contains("plain-api-secret"));
    assert_eq!(summary.id, 42);
    assert_eq!(summary.exchange, "OKX");
    assert_eq!(
        summary.last_check_code.as_deref(),
        Some("signed_exchange_preflight_passed")
    );
    assert!(summary.credential_envelope_ready);
    assert!(summary.execution_readiness.can_execute);
}
#[tokio::test]
async fn check_internal_api_credential_preserves_structured_blocker_code() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).unwrap();
        let body = r#"{"success":false,"code":"MEMBERSHIP_EXPIRED","message":"内部校验 API Key 失败: 会员已过期"}"#;
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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

    let error = client
        .check_internal_api_credential(42)
        .await
        .expect_err("structured Web blocker should remain an error");
    server.await.unwrap();
    let client_error = error
        .downcast_ref::<QuantWebClientError>()
        .expect("structured Web error");

    assert_eq!(client_error.error_code(), Some("MEMBERSHIP_EXPIRED"));
    assert!(error.to_string().contains("code=MEMBERSHIP_EXPIRED"));
    assert!(error.to_string().contains("response_body_omitted=true"));
    assert!(!error.to_string().contains("会员已过期"));
}
#[tokio::test]
async fn preview_market_velocity_task_creation_uses_internal_owner_route() {
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
        let body = r#"{"success":true,"data":{"read_only":true,"dry_run_only":true,"mutation_allowed":false,"would_create_execution_task":false,"generated_execution_task_count":0,"owner_service":"quant_web","status":"ready","exchange":"okx","symbol":"ASTER-USDT-SWAP","rank_event_id":2042663,"buyer_email":"buyer@example.com","combo_id":85,"target_r":2.4,"horizon_hours":48,"entry_rule_version":"rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1","entry_trigger_filter_version":"entry_trigger_allowlist_v1","risk_adjusted_win_rate_edge":null,"required_web_checks":[],"blocker_codes":[]}}"#;
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
    let preview = client
        .preview_market_velocity_execution_task_creation(
            MarketVelocityExecutionTaskCreationPreviewRequest {
                rank_event_id: Some(2042663),
                buyer_email: Some("buyer@example.com".to_string()),
                combo_id: Some(85),
                exchange: "okx".to_string(),
                symbol: "ASTER-USDT-SWAP".to_string(),
                target_r: 2.4,
                horizon_hours: 48,
                entry_rule_version: Some(
                    "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1".to_string(),
                ),
                entry_trigger_filter_version: Some("entry_trigger_allowlist_v1".to_string()),
                risk_adjusted_win_rate_edge: None,
            },
        )
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with(
        "POST /api/commerce/internal/market-velocity/execution-task-creation-preview HTTP/1.1"
    ));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert!(request.contains("\"rank_event_id\":2042663"));
    assert!(preview.read_only);
    assert!(preview.dry_run_only);
    assert!(!preview.mutation_allowed);
    assert_eq!(preview.owner_service, "quant_web");
    assert!(preview.blocker_codes.is_empty());
}
#[tokio::test]
async fn report_exchange_reconciliation_uses_internal_post_contract() {
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
        let body = r#"{"success":true,"data":{"combo_id":9,"buyer_email":"buyer@example.com","symbol":"ETHUSDT","signal_id":"exchange-reconciliation-exchange_open_order_conflict-9-ref","issue_type":"exchange_open_order_conflict","api_execution_status":"blocked_by_reconciliation","log":{}}}"#;
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
    let response = client
        .report_exchange_reconciliation(ExchangeReconciliationReportRequest {
            combo_id: 9,
            buyer_email: "buyer@example.com".to_string(),
            symbol: "ETHUSDT".to_string(),
            issue_type: ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
            detected_at: Some("2026-05-15T09:30:00Z".to_string()),
            source_ref: Some(
                "rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT"
                    .to_string(),
            ),
            message: Some("open order conflict detected".to_string()),
        })
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/commerce/internal/exchange-reconciliation HTTP/1.1"));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert!(request.contains(r#""combo_id":9"#));
    assert!(request.contains(r#""buyer_email":"buyer@example.com""#));
    assert!(request.contains(r#""symbol":"ETHUSDT""#));
    assert!(request.contains(r#""issue_type":"exchange_open_order_conflict""#));
    assert!(request.contains(r#""detected_at":"2026-05-15T09:30:00Z""#));
    assert!(request.contains(
        r#""source_ref":"rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT""#
    ));
    assert_eq!(response.combo_id, 9);
    assert_eq!(response.issue_type, "exchange_open_order_conflict");
    assert_eq!(response.api_execution_status, "blocked_by_reconciliation");
}
#[tokio::test]
async fn apply_exchange_close_fill_writeback_uses_internal_post_contract() {
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
        let body = r#"{"success":true,"data":{"order_result":{"id":30,"execution_task_id":86,"combo_id":85,"buyer_email":"buyer@example.com","exchange":"okx","external_order_id":"3631564680998985728","order_side":"sell","order_status":"filled","filled_qty":1.0,"filled_quote":0.6047,"fee_amount":-0.00030235,"raw_payload_json":"{}","created_at":"2026-06-06T08:00:00","updated_at":"2026-06-06T08:00:00"},"trade_record":{"id":44,"exchange_order_result_id":30,"side":"sell"},"position_snapshot_cleared":true}}"#;
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
    let response = client
        .apply_exchange_close_fill_writeback(ExchangeCloseFillWritebackRequest {
            task_id: 86,
            combo_id: 85,
            exchange: "okx".to_string(),
            symbol: "ASTER-USDT-SWAP".to_string(),
            source_ref: "rq:xrec:v2:ex=okx:combo=85:task=86:sym=ASTER-USDT-SWAP".to_string(),
            open_order_id: Some("3631557801300238336".to_string()),
            open_trade_id: Some("211849844".to_string()),
            close_order_id: "3631564680998985728".to_string(),
            close_trade_id: Some("211850229".to_string()),
            close_side: "sell".to_string(),
            close_size: 1.0,
            close_price: Some(0.6047),
            close_fee: Some(-0.00030235),
            close_timestamp_ms: Some(1_780_731_461_395),
            position_flat_confirmed: true,
            active_open_order_count: 0,
            quantity_match: true,
            writeback_authorized: true,
        })
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(
        request.starts_with("POST /api/commerce/internal/exchange-close-fill-writeback HTTP/1.1")
    );
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert!(request.contains(r#""task_id":86"#));
    assert!(request.contains(r#""combo_id":85"#));
    assert!(request.contains(r#""exchange":"okx""#));
    assert!(request.contains(r#""symbol":"ASTER-USDT-SWAP""#));
    assert!(request.contains(r#""close_order_id":"3631564680998985728""#));
    assert!(request.contains(r#""close_side":"sell""#));
    assert!(request.contains(r#""close_size":1.0"#) || request.contains(r#""close_size":1"#));
    assert!(request.contains(r#""writeback_authorized":true"#));
    assert!(!request.contains("api-secret"));
    assert!(response.position_snapshot_cleared);
    assert_eq!(
        response.order_result.external_order_id,
        "3631564680998985728"
    );
    assert_eq!(response.trade_record["side"], "sell");
}
#[tokio::test]
async fn submit_market_velocity_paper_outcome_uses_internal_post_contract() {
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
        let body = r#"{"success":true,"data":{"outcome":{"id":9,"rank_event_id":77,"exchange":"okx","symbol":"ETH-USDT-SWAP","target_r":1.5,"horizon_hours":24,"entry_rule_version":"rank_radar_4h_15m_v2","outcome_status":"win"},"generated_execution_task_count":0}}"#;
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
    let response = client
        .submit_market_velocity_paper_outcome(MarketVelocityPaperOutcomeRequest {
            rank_event_id: 77,
            exchange: "okx".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            target_r: 1.5,
            horizon_hours: 24,
            entry_rule_version: "rank_radar_4h_15m_v2".to_string(),
            entry_trigger: Some("breakout_previous_high".to_string()),
            entry_price: 100.0,
            entry_at: "2026-06-15T00:15:00Z".to_string(),
            outcome_status: "win".to_string(),
            exit_reason: "target_hit".to_string(),
            result_r: Some(1.5),
            evaluated_at: "2026-06-15T01:00:00Z".to_string(),
            evaluation_payload: serde_json::json!({
                "source": "market_velocity_event_backtest",
                "target_r": 1.5
            }),
        })
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(
        request.starts_with("POST /api/commerce/internal/market-velocity/paper-outcomes HTTP/1.1")
    );
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    assert!(request.contains(r#""rank_event_id":77"#));
    assert!(request.contains(r#""symbol":"ETH-USDT-SWAP""#));
    assert!(request.contains(r#""target_r":1.5"#));
    assert!(!request.contains("buyer@example.com"));
    assert_eq!(response.generated_execution_task_count, 0);
    assert_eq!(response.outcome["rank_event_id"], 77);
}
#[tokio::test]
async fn lease_tasks_uses_task_type_filters_in_internal_contract() {
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
        let body = r#"{"success":true,"data":{"tasks":[]}}"#;
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
    let leased = client
        .lease_tasks(ExecutionTaskLeaseRequest {
            worker_id: "worker-close".to_string(),
            limit: 5,
            task_ids: vec![42, 43],
            task_types: vec![
                "execute_signal".to_string(),
                "risk_control_close_candidate".to_string(),
            ],
            task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
        })
        .await
        .unwrap();
    server.await.unwrap();
    let request = rx.recv().unwrap();
    assert!(leased.tasks.is_empty());
    assert!(request.starts_with(
        "GET /api/commerce/internal/execution-tasks/lease?limit=5&task_id=42&task_id=43&task_type=execute_signal&task_type=risk_control_close_candidate&task_status=pending&task_status=pending_close HTTP/1.1"
    ));
    assert!(request.contains("x-alpha-execution-secret: dev-secret"));
}
