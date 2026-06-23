use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

fn script_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("run_binance_live_eth_micro_order_smoke.sh")
}

fn rust_cli_module_path() -> PathBuf {
    repo_root()
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("binance_eth_micro_live_validation.rs")
}

fn rust_cli_bin_path() -> PathBuf {
    repo_root()
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("bin")
        .join("binance_eth_micro_live_validation.rs")
}

fn rust_cli_http_module_path() -> PathBuf {
    repo_root()
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("binance_eth_micro_live_validation")
        .join("binance_futures_http.rs")
}

fn read_file(path: PathBuf) -> String {
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

#[test]
fn deprecated_shell_entrypoint_passes_bash_syntax_check() {
    let output = std::process::Command::new("bash")
        .arg("-n")
        .arg(script_path())
        .output()
        .expect("bash -n should be available");
    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn deprecated_shell_entrypoint_is_fail_fast_and_points_to_rust() {
    let script = read_file(script_path());
    assert!(script.contains("deprecated and disabled"));
    assert!(
        script.contains("cargo run -q -p rust-quant-cli --bin binance_eth_micro_live_validation")
    );
    assert!(
        script.contains("BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM=I_UNDERSTAND_TINY_ETH_LIVE_ORDER")
    );
    assert!(script.contains("exit 2"));
}

#[test]
fn deprecated_shell_entrypoint_carries_no_live_validation_logic() {
    let script = read_file(script_path());
    for forbidden in [
        "psql",
        "curl",
        "openssl dgst",
        "INSERT INTO execution_tasks",
        "strategy_signal_inbox",
        "EXECUTION_WORKER_DRY_RUN=false",
        "EXECUTION_WORKER_TARGET_TASK_IDS",
        "/fapi/v2/account",
        "/fapi/v1/openOrders",
        "/fapi/v1/order",
        "LINKUSDT",
        "LINK-USDT-SWAP",
    ] {
        assert!(
            !script.contains(forbidden),
            "deprecated shell entrypoint must not contain live validation fragment `{forbidden}`"
        );
    }
}

#[test]
fn rust_native_cli_entrypoint_exists() {
    let bin = read_file(rust_cli_bin_path());
    assert!(bin.contains("run_binance_eth_micro_live_validation_from_env"));
    assert!(bin.contains("serde_json::to_string_pretty"));
}

#[test]
fn rust_native_cli_keeps_live_validation_safety_contracts() {
    let source = format!(
        "{}\n{}",
        read_file(rust_cli_module_path()),
        read_file(rust_cli_http_module_path())
    );
    for required in [
        "BINANCE_ETH_MICRO_CONFIRM_TOKEN",
        "I_UNDERSTAND_TINY_ETH_LIVE_ORDER",
        "DEFAULT_EXCHANGE_SYMBOL: &str = \"ETHUSDT\"",
        "normalize_eth_symbol",
        "check_internal_api_credential",
        "resolve_user_exchange_config_for_credential",
        "/fapi/v2/account",
        "/fapi/v1/openOrders",
        "/fapi/v1/positionSide/dual",
        "/fapi/v1/exchangeInfo",
        "/fapi/v1/premiumIndex",
        "ensure_eth_position_flat",
        "ensure_no_open_orders",
        "api_credential_id",
        "protective_stop_loss_required",
        "selected_stop_loss_price",
        "ExecutionWorker::new",
        "PostgresExecutionAuditRepository::from_env",
        "target_task_ids: vec![task_id]",
        "verify_open_protection_sync",
        "risk_control_close_candidate",
        "pending_close",
    ] {
        assert!(
            source.contains(required),
            "Rust-native validation source must contain `{required}`"
        );
    }
}
