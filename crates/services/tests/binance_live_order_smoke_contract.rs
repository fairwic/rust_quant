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
        .join("run_binance_live_order_smoke.sh")
}

fn read_smoke_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

#[test]
fn smoke_script_passes_bash_syntax_check() {
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
fn legacy_live_order_smoke_entrypoint_is_fail_fast_disabled() {
    let script = read_smoke_script();

    assert!(
        script.contains("deprecated and disabled"),
        "legacy live order entrypoint must explain that it is disabled"
    );
    assert!(
        script.contains("exit 2"),
        "legacy live order entrypoint must fail closed with a non-zero exit"
    );
    assert!(
        script.contains("run_binance_live_eth_micro_order_smoke.sh"),
        "disabled entrypoint must point operators to the guarded ETH micro validation path"
    );
}

#[test]
fn legacy_live_order_smoke_entrypoint_cannot_write_credentials_or_place_orders() {
    let script = read_smoke_script();

    assert!(
        !script.contains("API_CREDENTIAL_SECRET")
            && !script.contains("seal_credential")
            && !script.contains("INSERT INTO user_api_credentials")
            && !script.contains("ON CONFLICT (buyer_email, exchange) DO UPDATE"),
        "disabled entrypoint must not carry legacy credential sealing/upsert logic"
    );
    assert!(
        !script.contains("EXECUTION_WORKER_DRY_RUN=false")
            && !script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM")
            && !script.contains("/fapi/v1/order"),
        "disabled entrypoint must not contain live mutation wiring"
    );
    assert!(
        !script.contains("LINKUSDT") && !script.contains("LINK-USDT-SWAP"),
        "disabled entrypoint must not mention the protected LINKUSDT position"
    );
}

// ---------------------------------------------------------------------------
// Unit-level contract: live_order_confirmation_valid logic mirrors the Rust
// implementation in execution_worker.rs.
// ---------------------------------------------------------------------------

fn live_order_confirmation_valid(dry_run: bool, confirmation: Option<&str>) -> bool {
    const TOKEN: &str = "I_UNDERSTAND_LIVE_ORDERS";
    dry_run
        || confirmation
            .map(str::trim)
            .is_some_and(|value| value == TOKEN)
}

#[test]
fn dry_run_true_always_passes_regardless_of_confirmation() {
    assert!(live_order_confirmation_valid(true, None));
    assert!(live_order_confirmation_valid(true, Some("")));
    assert!(live_order_confirmation_valid(true, Some("wrong")));
    assert!(live_order_confirmation_valid(
        true,
        Some("I_UNDERSTAND_LIVE_ORDERS")
    ));
}

#[test]
fn dry_run_false_requires_exact_confirmation_token() {
    assert!(!live_order_confirmation_valid(false, None));
    assert!(!live_order_confirmation_valid(false, Some("")));
    assert!(!live_order_confirmation_valid(
        false,
        Some("i_understand_live_orders")
    ));
    assert!(!live_order_confirmation_valid(false, Some("yes")));
    assert!(live_order_confirmation_valid(
        false,
        Some("I_UNDERSTAND_LIVE_ORDERS")
    ));
    assert!(live_order_confirmation_valid(
        false,
        Some("  I_UNDERSTAND_LIVE_ORDERS  ")
    ));
}

#[test]
fn dry_run_false_without_confirmation_is_refused() {
    let result = if live_order_confirmation_valid(false, None) {
        Ok(())
    } else {
        Err("refusing live exchange orders")
    };
    assert!(result.is_err());
}
