use serde::Serialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerLiveExchange {
    Binance,
    Okx,
    Bitget,
    Bybit,
    Gate,
    Hyperliquid,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionPlacementMode {
    SeparateStopMarket,
    AttachedStopLoss,
    Unsupported,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveWorkerCapabilityStatus {
    MutatingSupported,
    ReadOnlySupported,
    BlockedByPolicy,
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkerLiveCapability {
    /// 交易所名称。
    pub exchange: WorkerLiveExchange,
    /// 名称。
    pub exchange_name: String,
    /// protectionplacement。
    pub protection_placement: ProtectionPlacementMode,
    /// unprotected订单，用于当前结构体的业务数据。
    pub unprotected_order: LiveWorkerCapabilityStatus,
    /// orderlookup。
    pub order_lookup: LiveWorkerCapabilityStatus,
    /// protectiveordercancel。
    pub protective_order_cancel: LiveWorkerCapabilityStatus,
    /// 仓位sync。
    pub position_sync: LiveWorkerCapabilityStatus,
    /// openorder对账。
    pub open_order_reconciliation: LiveWorkerCapabilityStatus,
    /// readonlypreflight。
    pub read_only_preflight: LiveWorkerCapabilityStatus,
}
/// 提供workerlivecapabilitymatrix的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub fn worker_live_capability_matrix() -> Vec<WorkerLiveCapability> {
    ["binance", "okx", "bitget", "bybit", "gate", "hyperliquid"]
        .into_iter()
        .map(worker_live_capability_for_exchange)
        .collect()
}
/// 提供workerlivecapabilityfor交易所的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub fn worker_live_capability_for_exchange(exchange: &str) -> WorkerLiveCapability {
    let exchange_name = normalize_exchange_name(exchange);
    match exchange_name.as_str() {
        "binance" => supported_exchange_capability(
            WorkerLiveExchange::Binance,
            exchange_name,
            ProtectionPlacementMode::SeparateStopMarket,
            LiveWorkerCapabilityStatus::MutatingSupported,
        ),
        "okx" => supported_exchange_capability(
            WorkerLiveExchange::Okx,
            exchange_name,
            ProtectionPlacementMode::AttachedStopLoss,
            LiveWorkerCapabilityStatus::Unsupported,
        ),
        "bitget" => supported_exchange_capability(
            WorkerLiveExchange::Bitget,
            exchange_name,
            ProtectionPlacementMode::AttachedStopLoss,
            LiveWorkerCapabilityStatus::Unsupported,
        ),
        "bybit" => unsupported_exchange_capability(WorkerLiveExchange::Bybit, exchange_name),
        "gate" => unsupported_exchange_capability(WorkerLiveExchange::Gate, exchange_name),
        "hyperliquid" => {
            unsupported_exchange_capability(WorkerLiveExchange::Hyperliquid, exchange_name)
        }
        _ => unsupported_exchange_capability(WorkerLiveExchange::Unknown, exchange_name),
    }
}
/// 提供supported交易所capability的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn supported_exchange_capability(
    exchange: WorkerLiveExchange,
    exchange_name: String,
    protection_placement: ProtectionPlacementMode,
    protective_order_cancel: LiveWorkerCapabilityStatus,
) -> WorkerLiveCapability {
    WorkerLiveCapability {
        exchange,
        exchange_name,
        protection_placement,
        unprotected_order: LiveWorkerCapabilityStatus::BlockedByPolicy,
        order_lookup: LiveWorkerCapabilityStatus::ReadOnlySupported,
        protective_order_cancel,
        position_sync: LiveWorkerCapabilityStatus::ReadOnlySupported,
        open_order_reconciliation: LiveWorkerCapabilityStatus::ReadOnlySupported,
        read_only_preflight: LiveWorkerCapabilityStatus::ReadOnlySupported,
    }
}
/// 提供unsupported交易所capability的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn unsupported_exchange_capability(
    exchange: WorkerLiveExchange,
    exchange_name: String,
) -> WorkerLiveCapability {
    WorkerLiveCapability {
        exchange,
        exchange_name,
        protection_placement: ProtectionPlacementMode::Unsupported,
        unprotected_order: LiveWorkerCapabilityStatus::Unsupported,
        order_lookup: LiveWorkerCapabilityStatus::Unsupported,
        protective_order_cancel: LiveWorkerCapabilityStatus::Unsupported,
        position_sync: LiveWorkerCapabilityStatus::Unsupported,
        open_order_reconciliation: LiveWorkerCapabilityStatus::Unsupported,
        read_only_preflight: LiveWorkerCapabilityStatus::Unsupported,
    }
}
fn normalize_exchange_name(exchange: &str) -> String {
    exchange.trim().to_ascii_lowercase()
}
