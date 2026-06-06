use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

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
        .join("run_market_velocity_okx_live_preflight.sh")
}

fn read_preflight_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

fn make_executable(path: &std::path::Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn temp_contract_dir(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "market_velocity_okx_preflight_contract_{}_{}",
        std::process::id(),
        test_name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("scripts").join("dev")).unwrap();
    path
}

#[test]
fn okx_market_velocity_live_preflight_script_passes_bash_syntax_check() {
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
fn okx_market_velocity_live_preflight_is_read_only_and_never_runs_worker() {
    let script = read_preflight_script();

    assert!(script.contains("SELECT"));
    assert!(!script.contains("UPDATE "));
    assert!(!script.contains("INSERT "));
    assert!(!script.contains("DELETE "));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS"));
    assert!(!script.contains("cargo run"));
    assert!(!script.contains("/api/v5/trade/order"));
}

#[test]
fn okx_market_velocity_live_preflight_enforces_authorized_okx_scope() {
    let script = read_preflight_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT"));
    assert!(script.contains("5"));
    assert!(script.contains("lower(exchange) = 'okx'"));
    assert!(script.contains("signed_exchange_preflight_passed"));
    assert!(script.contains("signed_exchange_check_passed"));
    assert!(script.contains("api_key_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("api_secret_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("passphrase_cipher LIKE 'v4:local_aes256gcm:%'"));
    assert!(script.contains("execution_exchange"));
    assert!(script.contains("MARKET-VELOCITY-ALL"));
}

#[test]
fn okx_market_velocity_live_preflight_requires_target_task_and_blocks_link() {
    let script = read_preflight_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_TARGET_TASK_ID"));
    assert!(script.contains("EXECUTION_WORKER_TARGET_TASK_IDS"));
    assert!(script.contains("task_status IN ('pending', 'leased')"));
    assert!(script.contains("source_signal_type"));
    assert!(script.contains("market_velocity"));
    assert!(script.contains("UPPER(REPLACE(et.symbol, '-', '')) NOT LIKE 'LINKUSDT%'"));
    assert!(script.contains("selected_stop_loss_price"));
    assert!(script.contains("size_usdt"));
    assert!(script.contains("max_notional"));
}

#[test]
fn okx_market_velocity_live_preflight_requires_fresh_task_risk_context() {
    let script = read_preflight_script();

    assert!(script.contains("user_execution_risk_context"));
    assert!(script.contains("risk_context_expires_at"));
    assert!(script.contains("task_risk_context_expired"));
}

#[test]
fn okx_market_velocity_live_preflight_requires_minimum_risk_context_ttl_buffer() {
    let script = read_preflight_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS"));
    assert!(script.contains("risk_context_seconds_remaining"));
    assert!(script.contains("task_risk_context_ttl_too_short"));
}

#[test]
fn okx_market_velocity_live_preflight_requires_complete_protection_plan() {
    let script = read_preflight_script();

    assert!(script.contains("protective_stop_loss_required"));
    assert!(script.contains("risk_plan_direction"));
    assert!(script.contains("risk_plan_entry_price"));
    assert!(script.contains("risk_plan_stop_loss_source"));
    assert!(script.contains("validate_stop_loss_side"));
    assert!(script.contains("task_protective_stop_loss_not_required"));
    assert!(script.contains("task_risk_plan_direction_missing"));
    assert!(script.contains("task_risk_plan_direction_invalid"));
    assert!(script.contains("task_risk_plan_entry_price_missing"));
    assert!(script.contains("task_stop_loss_not_below_entry_for_long"));
    assert!(script.contains("task_stop_loss_not_above_entry_for_short"));
}

#[test]
fn okx_market_velocity_live_preflight_blocks_when_core_symbol_filters_missing() {
    let temp_dir = temp_contract_dir("symbol_filters_missing");
    let script_dir = temp_dir.join("scripts").join("dev");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    fs::copy(script_path(), &preflight_script).unwrap();

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"FROM strategy_combo_subscriptions c\"* ]]; then\n\
  printf '85\\tmarket_velocity\\tMARKET-VELOCITY-ALL\\tactive\\tapi_trade_enabled\\tokx\\ttrue\\tactive\\ttrue\\t5\\t5\\t3\\n'\n\
elif [[ \"${args}\" == *\"FROM user_api_credentials\"* ]]; then\n\
  printf '1\\n'\n\
elif [[ \"${args}\" == *\"risk_context_fresh\"* ]]; then\n\
  printf '86\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-06T05:10:00\\t300\\ttrue\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"FROM exchange_symbols\"* ]]; then\n\
  exit 0\n\
elif [[ \"${args}\" == *\"candidate_okx_tasks\"* || \"${args}\" == *\"ORDER BY et.updated_at DESC\"* ]]; then\n\
  printf '86\\tASTER-USDT-SWAP\\tpending\\t5.0\\t2026-06-06T05:10:00\\t2026-06-06 05:05:00\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&preflight_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run live preflight script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "preflight must fail closed when Core symbol filters are missing\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("blocker=okx_symbol_filters_missing"));
    assert!(!stdout.contains("\npreflight=ok"));

    let _ = fs::remove_dir_all(&temp_dir);
}
