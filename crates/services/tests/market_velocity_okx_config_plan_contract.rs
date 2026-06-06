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
        .join("plan_market_velocity_okx_live_config.sh")
}

fn read_plan_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

#[test]
fn okx_market_velocity_config_plan_script_passes_bash_syntax_check() {
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
fn okx_market_velocity_config_plan_is_read_only() {
    let script = read_plan_script();

    assert!(script.contains("SELECT"));
    assert!(!script.contains("UPDATE "));
    assert!(!script.contains("INSERT "));
    assert!(!script.contains("DELETE "));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
    assert!(!script.contains("cargo run"));
    assert!(!script.contains("/api/v5/trade/order"));
}

#[test]
fn okx_market_velocity_config_plan_redacts_user_identity_and_matches_by_hash() {
    let script = read_plan_script();

    assert!(script.contains("md5(buyer_email)"));
    assert!(script.contains("buyer_hash"));
    assert!(script.contains("ready_okx_credentials"));
    assert!(script.contains("market_velocity_combos"));
    assert!(script.contains("matching_ready_buyers"));
    assert!(!script.contains("SELECT buyer_email"));
    assert!(!script.contains("buyer_email,"));
}

#[test]
fn okx_market_velocity_config_plan_checks_live_validation_boundaries() {
    let script = read_plan_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT"));
    assert!(script.contains("5"));
    assert!(script.contains("lower(exchange) = 'okx'"));
    assert!(script.contains("signed_exchange_preflight_passed"));
    assert!(script.contains("signed_exchange_check_passed"));
    assert!(script.contains("api_key_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("api_secret_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("passphrase_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("execution_exchange"));
    assert!(script.contains("max_position_usdt"));
    assert!(script.contains("max_daily_loss_usdt"));
    assert!(script.contains("MARKET-VELOCITY-ALL"));
}
