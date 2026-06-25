#[test]
fn reconciliation_only_symbol_guard_excludes_linkusdt() {
    assert!(is_protected_link_symbol("LINKUSDT"));
    assert!(is_protected_link_symbol("LINK-USDT-SWAP"));
    assert!(is_protected_link_symbol("link-usdt"));
    assert!(!is_protected_link_symbol("ETHUSDT"));
}
#[test]
fn live_worker_config_allows_unscoped_database_enabled_tasks() {
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
    live_unscoped
        .validate_lease_limit()
        .expect("unscoped persistent live worker lets Web/database enabled tasks decide eligibility");
}
#[tokio::test]
async fn verify_live_audit_ready_allows_unscoped_database_enabled_tasks() {
    let repository = Arc::new(CapturingAuditRepository::default());
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap(),
        CryptoExcAllGateway::dry_run(),
        ExecutionWorkerConfig {
            worker_id: "worker-programmatic-live-unscoped".to_string(),
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
    worker
        .verify_live_audit_ready()
        .await
        .expect("unscoped persistent live worker should pass local live readiness");
    assert!(
        repository.checkpoints.lock().unwrap().is_empty(),
        "readiness verification must not lease or mutate execution tasks"
    );
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
