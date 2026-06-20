const EXECUTION_WORKER: &str = concat!(
    include_str!("../src/rust_quan_web/execution_worker.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_orchestration_section.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_live_execution_section.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_live_execution_support_section.rs"),
);
const EXECUTION_WORKER_RECONCILIATION: &str =
    include_str!("../src/rust_quan_web/execution_worker_reconciliation_section.rs");

#[test]
fn execution_worker_reconciliation_contract_live_worker_checks_before_live_mutations() {
    let resolve_gateway = EXECUTION_WORKER
        .find(".resolve_live_gateway(&task.buyer_email, order_task.exchange)")
        .expect("live worker must resolve the signed exchange gateway");
    let reconciliation = EXECUTION_WORKER
        .find(".check_exchange_reconciliation_before_live_order(task, &order_task, &gateway)")
        .expect("live worker must run read-only exchange reconciliation before live order");
    let prepare_settings = EXECUTION_WORKER
        .find(".prepare_order_settings(order_task.exchange, prepare)")
        .expect("live worker Binance settings mutation marker must exist");
    let live_order = EXECUTION_WORKER
        .find("self.live_order_request(&gateway, &order_task).await")
        .expect("live worker order mutation marker must exist");

    assert!(
        resolve_gateway < reconciliation,
        "exchange reconciliation needs the signed gateway resolved first"
    );
    assert!(
        reconciliation < prepare_settings,
        "exchange reconciliation must run before Binance settings mutation"
    );
    assert!(
        reconciliation < live_order,
        "exchange reconciliation must run before any live order mutation"
    );
    assert!(
        live_order < prepare_settings,
        "live order read-only request build, including ticker and exchange filters, must run before Binance settings mutation"
    );
    assert!(
        EXECUTION_WORKER.contains("\"stage\": \"exchange_reconciliation_read_only\"")
            && EXECUTION_WORKER.contains("\"mutation_allowed\": false"),
        "reconciliation blocker reports must be read-only and mutation disallowed"
    );
    assert!(
        EXECUTION_WORKER.contains("\"stage\": \"live_order_read_only_request_build\"")
            && EXECUTION_WORKER.contains("\"place_order_allowed\": false"),
        "read-only live order request build failures must fail closed before mutation"
    );
}

#[test]
fn execution_worker_reconciliation_contract_fail_closes_when_read_only_read_fails() {
    let combined = format!("{EXECUTION_WORKER}\n{EXECUTION_WORKER_RECONCILIATION}");
    let reconciliation_error_report = EXECUTION_WORKER
        .find("build_live_order_blocked_by_exchange_reconciliation_read_error_report(")
        .expect("live worker must build a no-mutation report when read-only reconciliation fails");
    let prepare_settings = EXECUTION_WORKER
        .find(".prepare_order_settings(order_task.exchange, prepare)")
        .expect("live worker Binance settings mutation marker must exist");
    let live_order = EXECUTION_WORKER
        .find("self.live_order_request(&gateway, &order_task).await")
        .expect("live worker order mutation marker must exist");

    assert!(
        reconciliation_error_report < prepare_settings,
        "read-only reconciliation read failures must fail closed before settings mutation"
    );
    assert!(
        reconciliation_error_report < live_order,
        "read-only reconciliation read failures must fail closed before order mutation"
    );
    assert!(
        combined.contains("\"gateway_read_failed\": true")
            && combined.contains("\"place_order_retried\": false"),
        "read-only reconciliation failure report must explicitly preserve no-order semantics"
    );
}

#[test]
fn execution_worker_reconciliation_contract_pending_close_reads_before_live_mutations() {
    let pending_close_start = EXECUTION_WORKER
        .find("async fn execute_pending_close_task")
        .expect("pending close worker path must exist");
    let pending_close = &EXECUTION_WORKER[pending_close_start..];
    let request = pending_close
        .find("let request = match close_task.to_order_request()")
        .expect("pending close must build the close order request");
    let resolve_gateway = pending_close
        .find(".resolve_live_gateway(&task.buyer_email, request.exchange)")
        .expect("pending close must resolve signed gateway");
    let read_only = pending_close
        .find(".check_exchange_read_only_before_pending_close(task, &request, &gateway)")
        .expect("pending close must run signed read-only reconciliation");
    let pre_place = pending_close
        .find(".pre_place_client_order_report(")
        .expect("pending close must run pre-place client order checks");
    let place_order = pending_close
        .find(".place_order_with_audit(task, &gateway, request.clone())")
        .expect("pending close live order mutation marker must exist");

    assert!(
        request < resolve_gateway && resolve_gateway < read_only,
        "pending close read-only reconciliation needs the signed gateway and request first"
    );
    assert!(
        read_only < pre_place && read_only < place_order,
        "pending close signed read-only reconciliation must run before close mutations"
    );
    assert!(
        EXECUTION_WORKER.contains("\"stage\": \"pending_close_exchange_reconciliation_read_only\"")
            && EXECUTION_WORKER.contains("\"gateway_read_failed\": true")
            && EXECUTION_WORKER.contains("\"place_order_retried\": false"),
        "pending close read failure report must explicitly preserve no-order semantics"
    );
}

#[test]
fn execution_worker_reconciliation_contract_source_refs_use_secret_safe_v2_contract() {
    assert!(
        EXECUTION_WORKER_RECONCILIATION.contains("\"rq:xrec:v2:ex={exchange}:acct={account_ref}:cred={credential_ref}:combo={combo_id}:task={task_id}:sym={symbol}:issue={issue_type}\""),
        "reconciliation source_ref must use the canonical v2 contract"
    );
    assert!(
        EXECUTION_WORKER_RECONCILIATION.contains("email_sha256_"),
        "reconciliation source_ref must use a normalized buyer_email SHA-256 account reference"
    );
    assert!(
        EXECUTION_WORKER_RECONCILIATION.contains("cred_unknown"),
        "reconciliation source_ref must fail closed to an unknown credential reference"
    );
    assert!(
        EXECUTION_WORKER_RECONCILIATION.contains("\"source_refs\": source_refs")
            && EXECUTION_WORKER_RECONCILIATION.contains("\"source_ref\": source_ref"),
        "reconciliation blocker raw payloads must carry source_ref evidence"
    );
    assert!(
        !EXECUTION_WORKER_RECONCILIATION.contains("rust_quant/exchange_reconciliation/"),
        "legacy reconciliation source_ref path format must not remain"
    );
}
