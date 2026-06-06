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
        .join("prepare_market_velocity_okx_live_config.sh")
}

fn read_prepare_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

#[test]
fn okx_market_velocity_config_prepare_script_passes_bash_syntax_check() {
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
fn okx_market_velocity_config_prepare_defaults_to_dry_run_and_never_starts_worker() {
    let script = read_prepare_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_CONFIG_APPLY"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_CONFIG_CONFIRM"));
    assert!(script.contains("I_UNDERSTAND_THIS_CHANGES_LIVE_CONFIG"));
    assert!(script.contains("mode=dry_run"));
    assert!(script.contains("mode=apply"));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
    assert!(!script.contains("cargo run"));
    assert!(!script.contains("/api/v5/trade/order"));
}

#[test]
fn okx_market_velocity_config_prepare_uses_ready_okx_v4_buyer_and_redacts_identity() {
    let script = read_prepare_script();

    assert!(script.contains("md5(buyer_email)"));
    assert!(script.contains("buyer_hash"));
    assert!(script.contains("lower(exchange) = 'okx'"));
    assert!(script.contains("signed_exchange_preflight_passed"));
    assert!(script.contains("signed_exchange_check_passed"));
    assert!(script.contains("api_key_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("api_secret_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("passphrase_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(!script.contains("echo \"${target_buyer_email"));
    assert!(!script.contains("api_secret_cipher="));
    assert!(!script.contains("api_key_cipher="));
}

#[test]
fn okx_market_velocity_config_prepare_aligns_only_minimal_live_validation_risk() {
    let script = read_prepare_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID"));
    assert!(script.contains("MARKET-VELOCITY-ALL"));
    assert!(script.contains("execution_exchange = 'okx'"));
    assert!(script.contains("service_mode = 'api_trade_enabled'"));
    assert!(script.contains("max_position_usdt"));
    assert!(script.contains("max_daily_loss_usdt"));
    assert!(script.contains("max_daily_trades"));
    assert!(script.contains("1 AS max_daily_trades"));
    assert!(script.contains("risk_acknowledged"));
    assert!(script.contains("ON CONFLICT (combo_id) DO UPDATE"));
    assert!(script.contains("BEGIN;"));
    assert!(script.contains("COMMIT;"));
}

#[test]
fn okx_market_velocity_config_prepare_requires_real_apply_result_row() {
    let script = read_prepare_script();

    assert!(script.contains("updated_existing_combo AS"));
    assert!(script.contains("aligned_combo AS"));
    assert!(script.contains("FROM aligned_combo"));
    assert!(script.contains("apply_result_row"));
    assert!(script.contains("$1 ~ /^[0-9]+$/"));
    assert!(script.contains("$2 ~ /^[0-9a-f]{32}$/"));
    assert!(script.contains("apply_result_missing"));
    assert!(script.contains("next_preflight=MARKET_VELOCITY_LIVE_COMBO_ID=${applied_combo_id}"));
}
