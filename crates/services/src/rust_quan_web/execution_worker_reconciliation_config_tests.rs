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
