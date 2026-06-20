use super::*;
use crate::rust_quan_web::ExecutionTask;
use crypto_exc_all::ExchangeId;
use serde_json::json;
use std::collections::BTreeSet;

fn task(payload: serde_json::Value) -> ExecutionTask {
    ExecutionTask {
        id: 42,
        news_signal_id: Some(7),
        strategy_signal_id: None,
        combo_id: 9,
        buyer_email: "buyer@example.com".to_string(),
        strategy_slug: "news_momentum".to_string(),
        symbol: "BTC-USDT-SWAP".to_string(),
        task_type: "execute_signal".to_string(),
        task_status: "pending".to_string(),
        priority: 3,
        lease_owner: None,
        lease_until: None,
        scheduled_at: "2026-04-23T12:00:00".to_string(),
        request_payload_json: payload,
        created_at: "2026-04-23T12:00:00".to_string(),
        updated_at: "2026-04-23T12:00:00".to_string(),
    }
}

#[test]
fn redacts_sensitive_values_from_audit_payload() {
    let payload = json!({
        "api_key": "plain-api-key",
        "api_secret": "plain-api-secret",
        "passphrase": "plain-passphrase",
        "nested": {
            "access_token": "plain-token",
            "symbol": "BTC-USDT-SWAP"
        }
    });

    let redacted = redact_audit_payload(payload);
    let serialized = redacted.to_string();

    assert_eq!(redacted["api_key"], "***REDACTED***");
    assert_eq!(redacted["api_secret"], "***REDACTED***");
    assert_eq!(redacted["passphrase"], "***REDACTED***");
    assert_eq!(redacted["nested"]["access_token"], "***REDACTED***");
    assert_eq!(redacted["nested"]["symbol"], "BTC-USDT-SWAP");
    assert!(!serialized.contains("plain-api-key"));
    assert!(!serialized.contains("plain-api-secret"));
    assert!(!serialized.contains("plain-passphrase"));
    assert!(!serialized.contains("plain-token"));
}

#[test]
fn redacts_sensitive_values_from_nested_headers_and_arrays() {
    let payload = json!({
        "headers": {
            "Authorization": "Bearer plain-bearer-token",
            "X-Api-Key": "plain-header-api-key",
            "Content-Type": "application/json"
        },
        "accounts": [
            {
                "secretKey": "plain-secret-key",
                "accessToken": "plain-access-token",
                "label": "primary"
            }
        ]
    });

    let redacted = redact_audit_payload(payload);
    let serialized = redacted.to_string();

    assert_eq!(redacted["headers"]["Authorization"], "***REDACTED***");
    assert_eq!(redacted["headers"]["X-Api-Key"], "***REDACTED***");
    assert_eq!(redacted["headers"]["Content-Type"], "application/json");
    assert_eq!(redacted["accounts"][0]["secretKey"], "***REDACTED***");
    assert_eq!(redacted["accounts"][0]["accessToken"], "***REDACTED***");
    assert_eq!(redacted["accounts"][0]["label"], "primary");
    assert!(!serialized.contains("plain-bearer-token"));
    assert!(!serialized.contains("plain-header-api-key"));
    assert!(!serialized.contains("plain-secret-key"));
    assert!(!serialized.contains("plain-access-token"));
}

#[test]
fn redacts_signed_url_signature_from_error_message() {
    let message = "HTTP错误: error sending request for url (https://fapi.binance.com/fapi/v3/positionRisk?symbol=ETHUSDT&timestamp=1780485895031&signature=d9abb4b3b09c375e3111a500ca91e472fce1a3837575ec3753e8038af20f2778): operation timed out";

    let redacted = redact_error_message(message.to_string());

    assert!(redacted.contains("HTTP错误"));
    assert!(redacted.contains("operation timed out"));
    assert!(redacted.contains("[signed_url_redacted]"));
    assert!(!redacted.contains("signature"));
    assert!(!redacted.contains("d9abb4"));
}

#[test]
fn builds_dry_run_audit_payload_without_credentials() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "BTC-USDT-SWAP",
        "side": "buy",
        "size": "0.01",
        "api_key": "plain-api-key"
    }));
    let order_task =
        crate::rust_quan_web::ExecutionOrderTask::from_task_with_default(&task, ExchangeId::Okx)
            .unwrap();
    let request = order_task.to_order_request().unwrap();

    let audit = ExchangeRequestAuditLog::success(
        &task,
        &request,
        true,
        Some(12),
        json!({
            "dry_run": true,
            "api_secret": "plain-api-secret"
        }),
    );

    assert_eq!(audit.request_id, "task-42-rqtask42");
    assert_eq!(audit.exchange, "okx");
    assert_eq!(audit.symbol, "BTC-USDT-SWAP");
    assert_eq!(audit.endpoint, "trade.place_order");
    assert_eq!(audit.request_status, "completed");
    assert_eq!(audit.latency_ms, Some(12));
    assert_eq!(audit.request_payload["dry_run"], true);
    assert_eq!(audit.request_payload["order"]["size"], "0.01");
    assert_eq!(audit.request_payload["task"]["id"], 42);
    assert_eq!(
        audit.request_payload["task"]["request_payload_json"]["api_key"],
        "***REDACTED***"
    );
    assert_eq!(audit.response_payload["api_secret"], "***REDACTED***");
    assert!(!audit.request_payload.to_string().contains("plain-api-key"));
    assert!(!audit
        .response_payload
        .to_string()
        .contains("plain-api-secret"));
}

#[test]
fn builds_worker_checkpoint_payload() {
    let checkpoint = ExecutionWorkerCheckpoint::heartbeat(
        "worker-a",
        "leased",
        Some(42),
        json!({
            "leased_count": 1,
            "dry_run": true
        }),
    );

    assert_eq!(checkpoint.worker_id, "worker-a");
    assert_eq!(checkpoint.worker_kind, "execution");
    assert_eq!(checkpoint.worker_status, "leased");
    assert_eq!(checkpoint.lease_owner, "worker-a");
    assert_eq!(checkpoint.checkpoint_key, "execution_worker");
    assert_eq!(checkpoint.last_task_id.as_deref(), Some("42"));
    assert_eq!(checkpoint.checkpoint_value["leased_count"], 1);
}

#[test]
fn report_result_replay_candidate_reconstructs_report_without_order_retry() {
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
        error_message: Some("waiting for fill".to_string()),
        raw_payload_json: Some(r#"{"client_order_id":"rqtask42"}"#.to_string()),
    };
    let audit = ExchangeRequestAuditLog::report_result_failed(&report, "web outage");

    let candidate = report_result_replay_candidate_from_payload(
        audit.request_id.clone(),
        &audit.request_payload,
    )
    .unwrap();

    assert_eq!(candidate.request_id, "report-task-42-12345");
    assert_eq!(candidate.report.task_id, 42);
    assert_eq!(candidate.report.exchange, "binance");
    assert_eq!(candidate.report.external_order_id, "12345");
    assert_eq!(candidate.report.order_status, "NEW");
    assert_eq!(
        candidate.report.raw_payload_json.as_deref(),
        Some(r#"{"client_order_id":"rqtask42"}"#)
    );
    assert_eq!(
        audit.request_payload["replay"]["action"],
        "retry_report_result_only"
    );
    assert_eq!(
        audit.request_payload["replay"]["place_order_allowed"],
        false
    );
}

#[test]
fn report_result_replay_candidate_rejects_place_order_allowed_payload() {
    let payload = json!({
        "replay": {
            "action": "retry_report_result_only",
            "place_order_allowed": true
        },
        "report": {
            "task_id": 42,
            "execution_status": "completed",
            "exchange": "binance",
            "external_order_id": "12345",
            "order_side": "buy",
            "order_status": "FILLED"
        }
    });

    let err =
        report_result_replay_candidate_from_payload("report-task-42-12345".to_string(), &payload)
            .unwrap_err();

    assert!(err.to_string().contains("allows place_order"));
}

#[test]
fn repository_checkpoint_columns_match_quant_core_ddl() {
    assert_insert_columns_exist_in_ddl(
        UPSERT_WORKER_CHECKPOINT_SQL,
        "execution_worker_checkpoints",
        &[
            "worker_id",
            "worker_kind",
            "worker_status",
            "lease_owner",
            "checkpoint_key",
            "checkpoint_value",
            "last_task_id",
            "last_heartbeat_at",
            "updated_at",
        ],
    );
}

#[test]
fn repository_exchange_audit_columns_match_quant_core_ddl() {
    assert_insert_columns_exist_in_ddl(
        INSERT_EXCHANGE_REQUEST_AUDIT_SQL,
        "exchange_request_audit_logs",
        &[
            "request_id",
            "exchange",
            "symbol",
            "endpoint",
            "request_status",
            "latency_ms",
            "request_payload",
            "response_payload",
            "error_message",
        ],
    );
}

#[test]
fn live_audit_readiness_documents_required_tables() {
    assert!(
        LIVE_AUDIT_READINESS_TABLE_SQL.contains("execution_worker_checkpoints")
            && LIVE_AUDIT_READINESS_TABLE_SQL.contains("exchange_request_audit_logs"),
        "live audit readiness must verify both checkpoint and exchange audit tables"
    );
}

#[tokio::test]
async fn postgres_audit_readiness_connects_before_live_execution() {
    let repository = PostgresExecutionAuditRepository::new(
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_millis(100))
            .connect_lazy("postgres://postgres:postgres@127.0.0.1:1/quant_core")
            .expect("lazy postgres url should parse"),
    );

    let error = repository
        .verify_live_audit_ready()
        .await
        .expect_err("live audit readiness must connect before allowing live execution");

    assert!(
        error
            .to_string()
            .contains("connect quant_core live audit database"),
        "unexpected error: {error:#}"
    );
}

#[test]
fn report_replay_candidate_sql_applies_failure_backoff_window() {
    assert!(
        LIST_REPORT_RESULT_REPLAY_CANDIDATES_SQL
            .contains("recent_failed.created_at > failed.created_at"),
        "report replay SQL must ignore its original failed row while backing off newer replay failures"
    );
    assert!(
        LIST_REPORT_RESULT_REPLAY_CANDIDATES_SQL
            .contains("recent_failed.created_at >= NOW() - ($3::bigint * INTERVAL '1 second')"),
        "report replay SQL must exclude candidates that failed again inside the configured backoff window"
    );
}

fn assert_insert_columns_exist_in_ddl(sql: &str, table: &str, expected_columns: &[&str]) {
    let ddl_columns = create_table_columns(table);
    let insert_columns = insert_columns(sql, table);
    assert_eq!(insert_columns, expected_columns);

    let missing_columns = insert_columns
        .iter()
        .filter(|column| !ddl_columns.contains(**column))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        missing_columns.is_empty(),
        "{table} repository SQL uses columns missing from DDL: {missing_columns:?}"
    );
}

fn create_table_columns(table: &str) -> BTreeSet<&'static str> {
    let ddl = include_str!("../../../../../sql/postgres_quant_core.sql");
    let marker = format!("CREATE TABLE IF NOT EXISTS {table} (");
    let start = ddl
        .find(&marker)
        .unwrap_or_else(|| panic!("{table} table DDL missing"))
        + marker.len();
    let body = &ddl[start..];
    let end = body
        .find("\n);")
        .unwrap_or_else(|| panic!("{table} table DDL terminator missing"));

    body[..end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.trim_end_matches(',').split_whitespace().next())
        .collect()
}

fn insert_columns<'a>(sql: &'a str, table: &str) -> Vec<&'a str> {
    let marker = format!("INSERT INTO {table} (");
    let start = sql
        .find(&marker)
        .unwrap_or_else(|| panic!("{table} insert SQL missing"))
        + marker.len();
    let body = &sql[start..];
    let end = body
        .find(')')
        .unwrap_or_else(|| panic!("{table} insert SQL column terminator missing"));

    body[..end]
        .split(',')
        .map(str::trim)
        .filter(|column| !column.is_empty())
        .collect()
}
