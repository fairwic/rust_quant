use anyhow::{anyhow, Context, Result};
use crypto_exc_all::ExchangeId;
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, ExecutionWorker, ExecutionWorkerConfig,
    PostgresExecutionAuditRepository,
};
use rust_quant_services::CryptoExcAllGateway;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{
    env,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

#[tokio::test]
async fn dry_run_worker_writes_checkpoint_and_exchange_audit_to_quant_core() -> Result<()> {
    if !smoke_enabled() {
        eprintln!(
            "skipping real Postgres smoke; set QUANT_CORE_AUDIT_SMOKE=1 and QUANT_CORE_DATABASE_URL"
        );
        return Ok(());
    }

    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .context("QUANT_CORE_DATABASE_URL is required for quant_core audit smoke")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    assert_required_tables_exist(&pool).await?;

    let run_id = unique_run_id()?;
    let task_id = run_id as i64;
    let worker_id = format!("qc_audit_smoke_{run_id}");
    let client_order_id = format!("qc-audit-smoke-{run_id}");
    let request_id = format!("task-{task_id}-{client_order_id}");

    delete_smoke_rows(&pool, &worker_id, &request_id).await?;

    let (base_url, server) = spawn_quant_web_stub(task_id, client_order_id.clone()).await?;
    let audit_repository = PostgresExecutionAuditRepository::from_env()?
        .ok_or_else(|| anyhow!("Postgres audit repository was not configured from env"))?;
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url,
            internal_secret: "local-dev-secret".to_string(),
        })?,
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: worker_id.clone(),
            lease_limit: 1,
            dry_run: true,
            default_exchange: ExchangeId::Okx,
            task_types: vec![
                "execute_signal".to_string(),
                "risk_control_close_candidate".to_string(),
            ],
            task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
        },
    )
    .with_audit_repository(Arc::new(audit_repository));

    let handled = worker.run_once().await?;
    server.await.context("quant web stub task panicked")??;

    assert_eq!(handled, 1);
    assert_worker_checkpoint(&pool, &worker_id, task_id).await?;
    assert_exchange_audit(&pool, &request_id, task_id, &client_order_id).await?;

    Ok(())
}

fn smoke_enabled() -> bool {
    env::var("QUANT_CORE_AUDIT_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn unique_run_id() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis())
}

async fn assert_required_tables_exist(pool: &PgPool) -> Result<()> {
    for table_name in [
        "execution_worker_checkpoints",
        "exchange_request_audit_logs",
    ] {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'public'
                  AND table_name = $1
            )
            "#,
        )
        .bind(table_name)
        .fetch_one(pool)
        .await
        .with_context(|| format!("check quant_core table {table_name} exists"))?;

        assert!(exists, "missing quant_core table: {table_name}");
    }

    Ok(())
}

async fn delete_smoke_rows(pool: &PgPool, worker_id: &str, request_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM exchange_request_audit_logs WHERE request_id = $1")
        .bind(request_id)
        .execute(pool)
        .await
        .context("delete previous smoke exchange audit row")?;
    sqlx::query("DELETE FROM execution_worker_checkpoints WHERE worker_id = $1")
        .bind(worker_id)
        .execute(pool)
        .await
        .context("delete previous smoke worker checkpoint row")?;
    Ok(())
}

async fn assert_worker_checkpoint(pool: &PgPool, worker_id: &str, task_id: i64) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT worker_status, last_task_id, checkpoint_value
        FROM execution_worker_checkpoints
        WHERE worker_id = $1
        "#,
    )
    .bind(worker_id)
    .fetch_one(pool)
    .await
    .context("fetch smoke worker checkpoint")?;

    let worker_status: String = row.try_get("worker_status")?;
    let last_task_id: Option<String> = row.try_get("last_task_id")?;
    let checkpoint_value: Value = row.try_get("checkpoint_value")?;
    let expected_task_id = task_id.to_string();

    assert_eq!(worker_status, "idle");
    assert_eq!(last_task_id.as_deref(), Some(expected_task_id.as_str()));
    assert_eq!(checkpoint_value["handled"], 1);
    assert_eq!(checkpoint_value["dry_run"], true);

    Ok(())
}

async fn assert_exchange_audit(
    pool: &PgPool,
    request_id: &str,
    task_id: i64,
    client_order_id: &str,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT exchange,
               symbol,
               endpoint,
               request_status,
               request_payload,
               response_payload,
               error_message
        FROM exchange_request_audit_logs
        WHERE request_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(request_id)
    .fetch_one(pool)
    .await
    .context("fetch smoke exchange request audit row")?;

    let exchange: String = row.try_get("exchange")?;
    let symbol: String = row.try_get("symbol")?;
    let endpoint: String = row.try_get("endpoint")?;
    let request_status: String = row.try_get("request_status")?;
    let request_payload: Value = row.try_get("request_payload")?;
    let response_payload: Value = row.try_get("response_payload")?;
    let error_message: String = row.try_get("error_message")?;

    assert_eq!(exchange, "okx");
    assert_eq!(symbol, "BTC-USDT-SWAP");
    assert_eq!(endpoint, "trade.place_order");
    assert_eq!(request_status, "completed");
    assert_eq!(error_message, "");
    assert_eq!(request_payload["dry_run"], true);
    assert_eq!(request_payload["task"]["id"], task_id);
    assert_eq!(
        request_payload["task"]["request_payload_json"]["api_key"],
        "***REDACTED***"
    );
    assert_eq!(request_payload["order"]["client_order_id"], client_order_id);
    assert_eq!(response_payload["dry_run"], true);
    assert!(
        !request_payload.to_string().contains("plain-api-key"),
        "audit payload leaked api key"
    );

    Ok(())
}

async fn spawn_quant_web_stub(
    task_id: i64,
    client_order_id: String,
) -> Result<(String, JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("bind quant web stub")?;
    let addr = listener.local_addr().context("read quant web stub addr")?;
    let task = leased_task(task_id, &client_order_id);
    let handle = tokio::spawn(async move {
        let mut saw_lease = false;
        let mut saw_report = false;

        while !(saw_lease && saw_report) {
            let (mut stream, _) = listener.accept().await.context("accept stub request")?;
            let request = read_http_request(&mut stream).await?;
            let request_line = request.lines().next().unwrap_or_default();

            if request_line.starts_with("GET /api/commerce/internal/execution-tasks/lease") {
                saw_lease = true;
                write_json_response(
                    &mut stream,
                    json!({
                        "success": true,
                        "data": {
                            "tasks": [task.clone()]
                        }
                    }),
                )
                .await?;
            } else if request_line.starts_with("POST /api/commerce/internal/execution-results") {
                saw_report = true;
                let report = request_body_json(&request)?;
                assert_eq!(report["task_id"], task_id);
                assert_eq!(report["execution_status"], "completed");
                assert_eq!(report["order_status"], "dry_run");

                let mut completed_task = task.clone();
                completed_task["task_status"] = json!("completed");
                write_json_response(
                    &mut stream,
                    json!({
                        "success": true,
                        "data": {
                            "task": completed_task,
                            "attempt": {},
                            "order_result": {
                                "dry_run": true
                            },
                            "trade_record": null
                        }
                    }),
                )
                .await?;
            } else {
                write_json_response(
                    &mut stream,
                    json!({
                        "success": false,
                        "error": format!("unexpected request: {request_line}")
                    }),
                )
                .await?;
                return Err(anyhow!("unexpected quant web stub request: {request_line}"));
            }
        }

        Ok(())
    });

    Ok((format!("http://{addr}"), handle))
}

fn leased_task(task_id: i64, client_order_id: &str) -> Value {
    json!({
        "id": task_id,
        "news_signal_id": 7,
        "combo_id": 9,
        "buyer_email": "buyer@example.com",
        "strategy_slug": "news_momentum",
        "symbol": "BTC-USDT-SWAP",
        "task_type": "execute_signal",
        "task_status": "pending",
        "priority": 3,
        "lease_owner": null,
        "lease_until": null,
        "scheduled_at": "2026-04-23T12:00:00",
        "request_payload_json": {
            "exchange": "okx",
            "symbol": "BTC-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size": "0.01",
            "margin_mode": "cross",
            "position_side": "long",
            "trade_side": "open",
            "client_order_id": client_order_id,
            "api_key": "plain-api-key"
        },
        "created_at": "2026-04-23T12:00:00",
        "updated_at": "2026-04-23T12:00:00"
    })
}

async fn read_http_request(stream: &mut TcpStream) -> Result<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut expected_len = None;

    loop {
        let read = stream.read(&mut chunk).await.context("read stub request")?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if expected_len.is_none() {
            if let Some(header_end) = find_header_end(&buffer) {
                let header = std::str::from_utf8(&buffer[..header_end])
                    .context("stub request header is not utf-8")?;
                expected_len = Some(header_end + 4 + content_length(header)?);
            }
        }

        if expected_len
            .map(|length| buffer.len() >= length)
            .unwrap_or(false)
        {
            break;
        }
    }

    String::from_utf8(buffer).context("stub request is not utf-8")
}

fn request_body_json(request: &str) -> Result<Value> {
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| anyhow!("stub request missing body separator"))?;
    serde_json::from_str(body).context("parse stub request body json")
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(header: &str) -> Result<usize> {
    header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>())
        })
        .transpose()
        .context("parse content-length")?
        .map_or(Ok(0), Ok)
}

async fn write_json_response(stream: &mut TcpStream, body: Value) -> Result<()> {
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("write stub response")
}
