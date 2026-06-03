use std::{env, fs, path::PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("services crate should live under crypto_quant/rust_quant/crates/services")
        .to_path_buf()
}

fn read_workspace_file(path: &str) -> String {
    let path = workspace_root().join(path);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn web_backend_projection_keeps_core_worker_capability_semantics() {
    let source =
        read_workspace_file("rust_quan_web/backend/src/services/exchange_execution_capability.rs");

    for required in [
        "protection_placement",
        "unprotected_order",
        "protective_order_cancel",
        "position_sync",
        "open_order_reconciliation",
        "read_only_preflight",
        "blocked_by_policy",
        "separate_stop_market",
        "attached_stop_loss",
    ] {
        assert!(
            source.contains(required),
            "Web backend capability projection must preserve Core semantic field `{required}`"
        );
    }
}

#[test]
fn admin_capability_matrix_uses_operator_copy_without_internal_live_terms() {
    let rows = read_workspace_file(
        "rust_quant_admin/admin/playground/src/views/quant/user/api-key-live-capability.ts",
    );
    let view = read_workspace_file(
        "rust_quant_admin/admin/playground/src/views/quant/user/ApiKeyLiveCapabilityMatrix.vue",
    );
    let combined = format!("{rows}\n{view}");

    for forbidden in ["MVP", "STOP_MARKET", "Worker live", "worker route"] {
        assert!(
            !combined.contains(forbidden),
            "Admin capability surface must not present internal implementation term `{forbidden}` as business closure"
        );
    }

    for required in [
        "已开放自动执行",
        "账户验证已接入",
        "合约资金可检查",
        "自动止损",
        "保护单撤销",
        "非保护下单已阻断",
    ] {
        assert!(
            combined.contains(required),
            "Admin capability surface must expose productized operator copy `{required}`"
        );
    }
}
