use rust_quant_services::rust_quan_web::{
    worker_live_capability_for_exchange, worker_live_capability_matrix, LiveWorkerCapabilityStatus,
    ProtectionPlacementMode, WorkerLiveExchange,
};
#[test]
fn matrix_declares_supported_exchange_live_worker_capabilities() {
    let binance = worker_live_capability_for_exchange("binance");
    assert_eq!(binance.exchange, WorkerLiveExchange::Binance);
    assert_eq!(
        binance.protection_placement,
        ProtectionPlacementMode::SeparateStopMarket
    );
    assert_eq!(
        binance.protective_order_cancel,
        LiveWorkerCapabilityStatus::MutatingSupported
    );
    assert_eq!(
        binance.unprotected_order,
        LiveWorkerCapabilityStatus::BlockedByPolicy
    );
    assert_eq!(
        binance.order_lookup,
        LiveWorkerCapabilityStatus::ReadOnlySupported
    );
    assert_eq!(
        binance.position_sync,
        LiveWorkerCapabilityStatus::ReadOnlySupported
    );
    assert_eq!(
        binance.open_order_reconciliation,
        LiveWorkerCapabilityStatus::ReadOnlySupported
    );
    assert_eq!(
        binance.read_only_preflight,
        LiveWorkerCapabilityStatus::ReadOnlySupported
    );
    for exchange in ["okx", "bitget"] {
        let capability = worker_live_capability_for_exchange(exchange);
        assert_eq!(
            capability.protection_placement,
            ProtectionPlacementMode::AttachedStopLoss
        );
        assert_eq!(
            capability.protective_order_cancel,
            LiveWorkerCapabilityStatus::Unsupported
        );
        assert_eq!(
            capability.unprotected_order,
            LiveWorkerCapabilityStatus::BlockedByPolicy
        );
        assert_eq!(
            capability.position_sync,
            LiveWorkerCapabilityStatus::ReadOnlySupported
        );
        assert_eq!(
            capability.open_order_reconciliation,
            LiveWorkerCapabilityStatus::ReadOnlySupported
        );
    }
}
#[test]
fn matrix_declares_unwired_exchanges_as_unsupported_without_guessing() {
    let hyperliquid = worker_live_capability_for_exchange("hyperliquid");
    assert_eq!(hyperliquid.exchange, WorkerLiveExchange::Hyperliquid);
    assert_eq!(
        hyperliquid.protection_placement,
        ProtectionPlacementMode::Unsupported
    );
    assert_eq!(
        hyperliquid.unprotected_order,
        LiveWorkerCapabilityStatus::Unsupported
    );
    assert_eq!(
        hyperliquid.position_sync,
        LiveWorkerCapabilityStatus::Unsupported
    );
    assert_eq!(
        hyperliquid.read_only_preflight,
        LiveWorkerCapabilityStatus::Unsupported
    );
    for (exchange, worker_exchange) in [
        ("bybit", WorkerLiveExchange::Bybit),
        ("gate", WorkerLiveExchange::Gate),
    ] {
        let unknown = worker_live_capability_for_exchange(exchange);
        assert_eq!(unknown.exchange, worker_exchange);
        assert_eq!(unknown.exchange_name, exchange);
        assert_eq!(
            unknown.protection_placement,
            ProtectionPlacementMode::Unsupported
        );
        assert_eq!(
            unknown.order_lookup,
            LiveWorkerCapabilityStatus::Unsupported
        );
    }
}
#[test]
fn matrix_normalizes_exchange_names_for_display_inputs() {
    let capability = worker_live_capability_for_exchange("  BINANCE  ");
    assert_eq!(capability.exchange, WorkerLiveExchange::Binance);
    assert_eq!(capability.exchange_name, "binance");
}
#[test]
fn matrix_lists_each_worker_exchange_once_in_display_order() {
    let matrix = worker_live_capability_matrix();
    let names: Vec<_> = matrix
        .iter()
        .map(|capability| capability.exchange_name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["binance", "okx", "bitget", "bybit", "gate", "hyperliquid"]
    );
}
