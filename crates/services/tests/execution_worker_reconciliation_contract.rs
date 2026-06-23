const EXECUTION_WORKER: &str = concat!(
    include_str!("../src/rust_quan_web/execution_worker.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_orchestration_section.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_live_guard_section.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_live_execution_section.rs"),
    "\n",
    include_str!("../src/rust_quan_web/execution_worker_live_execution_support_section.rs"),
);
const EXECUTION_WORKER_RECONCILIATION: &str =
    include_str!("../src/rust_quan_web/execution_worker_reconciliation_section.rs");
const EXECUTION_AUDIT: &str = include_str!("../src/rust_quan_web/execution_audit.rs");
const EXECUTION_TAKE_PROFIT: &str = include_str!("../src/rust_quan_web/execution_take_profit.rs");
const EXECUTION_PROTECTION: &str = include_str!("../src/rust_quan_web/execution_protection.rs");
const EXECUTION_PROTECTIVE_OUTCOME_CHECK: &str =
    include_str!("../src/rust_quan_web/execution_protective_outcome_check.rs");
const CRYPTO_EXC_ALL_GATEWAY: &str = include_str!("../src/exchange/crypto_exc_all_gateway.rs");
#[test]
fn execution_worker_reconciliation_contract_live_worker_checks_before_live_mutations() {
    let resolve_gateway = EXECUTION_WORKER
        .find(".resolve_live_gateway_for_task(task, order_task.exchange)")
        .expect("live worker must resolve the signed exchange gateway");
    let reconciliation = EXECUTION_WORKER
        .find(".check_exchange_reconciliation_before_live_order(task, &order_task, &gateway)")
        .expect("live worker must run read-only exchange reconciliation before live order");
    let prepare_settings = EXECUTION_WORKER
        .find(".prepare_order_settings_with_audit(")
        .expect("live worker audited Binance settings mutation marker must exist");
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
        .find(".prepare_order_settings_with_audit(")
        .expect("live worker audited Binance settings mutation marker must exist");
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
fn live_order_request_checks_orderbook_before_main_order_settings_mutation() {
    let live_order_request_start = EXECUTION_WORKER
        .find("async fn live_order_request")
        .expect("live order request builder must exist");
    let live_order_request_end = EXECUTION_WORKER[live_order_request_start..]
        .find("async fn resolve_live_gateway")
        .map(|offset| live_order_request_start + offset)
        .expect("live order request section end must exist");
    let live_order_request_section =
        &EXECUTION_WORKER[live_order_request_start..live_order_request_end];
    let orderbook_read = live_order_request_section
        .find(".orderbook(")
        .expect("live order request builder must read orderbook before mutation");
    let request_build = live_order_request_section
        .find("order_task.to_live_order_request")
        .expect("live order request builder must construct final order request");
    let orderbook_guard = live_order_request_section
        .find("validate_live_orderbook_execution_boundary(")
        .expect("live order request builder must validate orderbook execution boundary");
    let prepare_settings = EXECUTION_WORKER
        .find(".prepare_order_settings_with_audit(")
        .expect("account settings mutation path must exist");
    let live_order_call = EXECUTION_WORKER
        .find("self.live_order_request(&gateway, &order_task).await")
        .expect("live worker must build order request before settings mutation");
    assert!(
        orderbook_read < request_build,
        "orderbook should be read before final order request is returned"
    );
    assert!(
        request_build < orderbook_guard,
        "orderbook guard must check the final quantized order size"
    );
    assert!(
        live_order_call < prepare_settings,
        "read-only orderbook guard must run before account settings mutation"
    );
}
#[test]
fn prepare_order_settings_uses_worker_live_mutation_audit() {
    let prepare_start = EXECUTION_WORKER
        .find("async fn prepare_order_settings_after_protection")
        .expect("settings preparation path must exist");
    let prepare_end = EXECUTION_WORKER[prepare_start..]
        .find("async fn confirmed_live_order_report")
        .map(|offset| prepare_start + offset)
        .expect("settings preparation section end must exist");
    let prepare_section = &EXECUTION_WORKER[prepare_start..prepare_end];
    assert!(
        prepare_section.contains(".prepare_order_settings_with_audit("),
        "account settings mutation must go through the worker live audit wrapper"
    );
    assert!(
        !prepare_section.contains(".prepare_order_settings(order_task.exchange, prepare)"),
        "account settings mutation must not bypass worker live mutation audit"
    );
    assert!(
        EXECUTION_AUDIT.contains("account.prepare_order_settings"),
        "account settings mutation must have a dedicated exchange_request_audit_logs endpoint"
    );
}
#[test]
fn crypto_exc_all_gateway_live_mutations_require_worker_audit_scope() {
    for (function_marker, mutation_marker) in [
        (
            "pub async fn prepare_order_settings",
            "prepare_order_settings(request).await",
        ),
        ("pub async fn place_order", ".place_order("),
        (
            "pub async fn place_protective_order",
            ".place_protective_order(request).await",
        ),
        ("pub async fn cancel_order", ".cancel_order(request).await"),
        (
            "pub async fn cancel_protective_order",
            ".cancel_protective_order(request).await",
        ),
    ] {
        let function_offset = CRYPTO_EXC_ALL_GATEWAY
            .find(function_marker)
            .unwrap_or_else(|| panic!("missing gateway function marker {function_marker}"));
        let function_body = &CRYPTO_EXC_ALL_GATEWAY[function_offset..];
        let guard_offset = function_body
            .find("ensure_live_mutation_audit_scope")
            .unwrap_or_else(|| panic!("{function_marker} must require worker audit scope"));
        let mutation_offset = function_body
            .find(mutation_marker)
            .unwrap_or_else(|| panic!("{function_marker} missing mutation marker"));
        assert!(
            guard_offset < mutation_offset,
            "{function_marker} must require worker audit scope before live SDK mutation"
        );
    }
    assert!(
        EXECUTION_WORKER.contains("with_live_mutation_audit_scope("),
        "worker audited wrappers must enter gateway live mutation audit scope"
    );
}
#[test]
fn crypto_exc_all_gateway_signed_read_only_queries_require_scope() {
    for (function_marker, signed_read_marker) in [
        (
            "pub async fn order",
            "sdk.orders(exchange)?.get(query).await",
        ),
        (
            "pub async fn protective_order",
            "sdk.orders(exchange)?.get_protective_order(query).await",
        ),
        (
            "pub async fn open_orders",
            "sdk.orders(exchange)?.open(query).await",
        ),
        (
            "pub async fn order_history",
            "sdk.orders(exchange)?.history(query).await",
        ),
        (
            "pub async fn position_history",
            "sdk.positions(exchange)?.history(query).await",
        ),
        (
            "pub async fn fills",
            "sdk.fills(exchange)?.list(query).await",
        ),
        (
            "pub async fn balances",
            "sdk.account(exchange)?.balances().await",
        ),
        (
            "pub async fn account_bills",
            "sdk.account(exchange)?.bills(query).await",
        ),
        (
            "pub async fn positions",
            "sdk.positions(exchange)?.list(instrument).await",
        ),
    ] {
        let function_offset = CRYPTO_EXC_ALL_GATEWAY
            .find(function_marker)
            .unwrap_or_else(|| panic!("missing gateway function marker {function_marker}"));
        let function_body = &CRYPTO_EXC_ALL_GATEWAY[function_offset..];
        let guard_offset = function_body
            .find("ensure_signed_read_only_scope")
            .unwrap_or_else(|| panic!("{function_marker} must require signed read-only scope"));
        let signed_read_offset = function_body
            .find(signed_read_marker)
            .unwrap_or_else(|| panic!("{function_marker} missing signed read marker"));
        assert!(
            guard_offset < signed_read_offset,
            "{function_marker} must require signed read-only scope before live SDK account/order read"
        );
    }
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
        .find(".resolve_live_gateway_for_task(task, request.exchange)")
        .expect("pending close must resolve signed gateway");
    let credential_preflight = pending_close
        .find(".live_api_credential_preflight_report_for_order(")
        .expect("pending close must check API credential readiness before resolving gateway");
    let read_only = pending_close
        .find(".check_exchange_read_only_before_pending_close(")
        .expect("pending close must run signed read-only reconciliation");
    let pre_place = pending_close
        .find(".pre_place_client_order_report(")
        .expect("pending close must run pre-place client order checks");
    let place_order = pending_close
        .find(".place_order_with_audit(task, &gateway, request.clone())")
        .expect("pending close live order mutation marker must exist");
    assert!(
        request < credential_preflight && credential_preflight < resolve_gateway,
        "pending close API credential readiness must be checked before resolving signed credentials"
    );
    assert!(
        resolve_gateway < read_only,
        "pending close read-only reconciliation needs the signed gateway first"
    );
    assert!(
        read_only < pre_place && read_only < place_order,
        "pending close signed read-only reconciliation must run before close mutations"
    );
    assert!(
        EXECUTION_WORKER.contains("\"stage\": \"pending_close_exchange_reconciliation_read_only\"")
            && EXECUTION_WORKER.contains("pending_close_gateway_read_failed")
            && EXECUTION_WORKER.contains("\"place_order_retried\": false"),
        "pending close read failure report must explicitly preserve no-order semantics"
    );
    assert!(
        pending_close.contains("let gateway_read_failed =")
            && pending_close.contains("\"blocker_code\": blocker_code")
            && pending_close.contains("\"gateway_read_failed\": gateway_read_failed"),
        "pending close reconciliation reports must distinguish gateway read failures from no-matching-position blockers"
    );
}
#[test]
fn execution_worker_reconciliation_contract_pending_close_requires_matching_position() {
    let check_start = EXECUTION_WORKER
        .find("async fn check_exchange_read_only_before_pending_close")
        .expect("pending close reconciliation check must exist");
    let check_end = EXECUTION_WORKER[check_start..]
        .find("async fn run_confirmation_once")
        .map(|offset| check_start + offset)
        .expect("pending close reconciliation section end must exist");
    let check_section = &EXECUTION_WORKER[check_start..check_end];
    assert!(
        check_section.contains("pending_close_has_matching_position"),
        "pending close read-only reconciliation must validate that the signed account still has the position being closed"
    );
    assert!(
        check_section.contains("pending_close_no_matching_position"),
        "missing matching position must be a fail-closed blocker before close order mutation"
    );
    assert!(
        check_section.contains("pending_close_has_conflicting_open_order")
            && check_section.contains("pending_close_active_close_order_conflict"),
        "pending close read-only reconciliation must block duplicate active close-side open orders before close order mutation"
    );
    assert!(
        !check_section.contains("\"place_order_allowed\": true"),
        "pending close reconciliation checkpoint must not declare place_order_allowed=true before mutation"
    );
}
#[test]
fn execution_worker_contract_live_gateway_resolves_exact_task_credential() {
    let support = EXECUTION_WORKER
        .find("async fn resolve_live_gateway_for_task")
        .expect("live gateway must have a task-bound resolve helper");
    let support_section = &EXECUTION_WORKER[support..];
    assert!(
        support_section.contains("api_credential_id_from_task(task)"),
        "live gateway resolve must read api_credential_id from the execution task"
    );
    assert!(
        EXECUTION_WORKER.contains(".resolve_user_exchange_config_for_credential("),
        "live gateway resolve must ask Web for the exact checked credential"
    );
    assert!(
        EXECUTION_WORKER.contains(".resolve_live_gateway_for_task(&item.task, pending.exchange)"),
        "pending confirmation read-only order lookup must use the same exact task credential"
    );
    assert!(
        !EXECUTION_WORKER.contains(".resolve_live_gateway(&task.buyer_email, order_task.exchange)")
            && !EXECUTION_WORKER
                .contains(".resolve_live_gateway(&task.buyer_email, request.exchange)"),
        "live mutation paths must not fall back to buyer+exchange credential resolution"
    );
}
#[test]
fn execution_worker_confirmation_dry_run_blocks_before_signed_lookup_and_tp_mutations() {
    let confirmation_start = EXECUTION_WORKER
        .find("async fn execute_pending_confirmation_item")
        .expect("confirmation execution path must exist");
    let confirmation = &EXECUTION_WORKER[confirmation_start..];
    let dry_run_guard = confirmation
        .find("if self.config.dry_run")
        .expect("confirmation path must check worker dry-run mode");
    let resolve_gateway = confirmation
        .find(".resolve_live_gateway_for_task(&item.task, pending.exchange)")
        .expect("confirmation path must resolve signed gateway only after dry-run guard");
    let query_order = confirmation
        .find("gateway.order(pending.exchange, query)")
        .expect("confirmation path must query exchange order only after dry-run guard");
    let tp_sync = confirmation
        .find("sync_take_profit_orders_after_main_fill(")
        .expect("confirmation path may retry take-profit sync only after dry-run guard");
    let stop_reset = confirmation
        .find("sync_take_profit_stop_reset_after_fills(")
        .expect("confirmation path may reset stops only after dry-run guard");
    assert!(
        dry_run_guard < resolve_gateway
            && dry_run_guard < query_order
            && dry_run_guard < tp_sync
            && dry_run_guard < stop_reset,
        "confirmation dry-run must return before signed lookup or any take-profit mutation"
    );
    assert!(
        confirmation.contains("\"confirmation_stage\": \"dry_run_blocked\"")
            && confirmation.contains("pending confirmation requires live read-only order lookup"),
        "confirmation dry-run report must be explicit and keep the task pending"
    );
}
#[test]
fn take_profit_child_orders_use_worker_live_mutation_audit() {
    assert!(
        !EXECUTION_TAKE_PROFIT.contains("gateway.place_order(request.clone()).await"),
        "take-profit child order mutation must not bypass worker live mutation audit"
    );
    assert!(
        EXECUTION_WORKER.contains("place_take_profit_order_with_audit("),
        "worker must provide audited placement for take-profit child orders"
    );
}
#[test]
fn protective_order_mutations_use_worker_live_mutation_audit() {
    assert!(
        !EXECUTION_PROTECTION.contains(".place_protective_order("),
        "protective order placement must not bypass worker live mutation audit"
    );
    assert!(
        !EXECUTION_PROTECTION.contains(".cancel_protective_order("),
        "protective order cancellation must not bypass worker live mutation audit"
    );
    assert!(
        !EXECUTION_TAKE_PROFIT.contains(".cancel_protective_order("),
        "take-profit stop reset protective cancellation must not bypass worker live mutation audit"
    );
    assert!(
        EXECUTION_WORKER.contains("place_protective_order_with_audit(")
            && EXECUTION_WORKER.contains("cancel_protective_order_with_audit("),
        "worker must provide audited protective placement and cancellation"
    );
}
#[test]
fn pending_close_cancel_order_uses_worker_live_mutation_audit() {
    assert!(
        !EXECUTION_WORKER.contains("gateway.cancel_order(exchange, cancel_request).await"),
        "pending-close post-fill cancel order must not bypass worker live mutation audit"
    );
    assert!(
        EXECUTION_WORKER.contains("cancel_order_with_audit("),
        "worker must provide audited placement for ordinary cancel-order mutations"
    );
}
#[test]
fn protective_outcome_check_live_mutations_use_persistent_audit() {
    assert!(
        !EXECUTION_PROTECTIVE_OUTCOME_CHECK
            .contains(".place_protective_order(config.exchange, request.clone())"),
        "standalone protective outcome placement must not bypass persistent live audit"
    );
    assert!(
        !EXECUTION_PROTECTIVE_OUTCOME_CHECK
            .contains(".cancel_protective_order(config.exchange, cancel_request)"),
        "standalone protective outcome cancellation must not bypass persistent live audit"
    );
    assert!(
        EXECUTION_PROTECTIVE_OUTCOME_CHECK.contains("protective_outcome_place_order_with_audit(")
            && EXECUTION_PROTECTIVE_OUTCOME_CHECK
                .contains("protective_outcome_cancel_order_with_audit(")
            && EXECUTION_PROTECTIVE_OUTCOME_CHECK
                .contains("PostgresExecutionAuditRepository::from_env()")
            && EXECUTION_PROTECTIVE_OUTCOME_CHECK.contains("with_live_mutation_audit_scope("),
        "standalone protective outcome check must use persistent audit before live mutations"
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
