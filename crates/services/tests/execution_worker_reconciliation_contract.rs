const EXECUTION_WORKER: &str = include_str!("../src/rust_quan_web/execution_worker.rs");

#[test]
fn live_worker_checks_exchange_reconciliation_before_live_mutations() {
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
        EXECUTION_WORKER.contains("\"stage\": \"exchange_reconciliation_read_only\"")
            && EXECUTION_WORKER.contains("\"mutation_allowed\": false"),
        "reconciliation blocker reports must be read-only and mutation disallowed"
    );
}
