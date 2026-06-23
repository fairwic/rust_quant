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
            target_task_ids: vec![42],
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
            target_task_ids: vec![42],
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
    assert_eq!(
        repository.report_replay_queries.lock().unwrap().as_slice(),
        &[(1, 300, vec![42])]
    );
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
async fn report_replay_mode_scopes_candidates_to_target_task_ids_before_limit() {
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
    repository.report_replay_candidates.lock().unwrap().extend([
        ReportResultReplayCandidate {
            request_id: "report-task-41-older".to_string(),
            report: ExecutionTaskReportRequest {
                task_id: 41,
                execution_status: "pending_confirmation".to_string(),
                exchange: "binance".to_string(),
                external_order_id: "older".to_string(),
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
            request_id: "report-task-42-target".to_string(),
            report: ExecutionTaskReportRequest {
                task_id: 42,
                execution_status: "pending_confirmation".to_string(),
                exchange: "binance".to_string(),
                external_order_id: "target".to_string(),
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
            worker_id: "worker-report-replay-targeted".to_string(),
            lease_limit: 1,
            dry_run: false,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string()],
            target_task_ids: vec![42],
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
    assert!(request.contains(r#""task_id":42"#));
    assert!(!request.contains(r#""task_id":41"#));
    assert_eq!(handled, 1);
    assert_eq!(
        repository.report_replay_queries.lock().unwrap().as_slice(),
        &[(1, 300, vec![42])]
    );
    assert!(repository
        .checkpoints
        .lock()
        .unwrap()
        .iter()
        .all(|checkpoint| checkpoint.worker_status != "skipped_target_task_mismatch"));
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
            target_task_ids: vec![42, 43],
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
        &[(2, 900, vec![42, 43])]
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
#[tokio::test]
async fn live_order_audit_write_failure_blocks_order_result() {
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1".to_string(),
            internal_secret: String::new(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-audit-fail-closed".to_string(),
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
    .with_audit_repository(Arc::new(FailingAuditRepository));
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
    let error = worker
        .place_order_with_audit(&task, &worker.gateway, request)
        .await
        .expect_err("live order must fail closed when audit write is unavailable");
    assert!(error
        .to_string()
        .contains("live execution audit write failed"));
}
