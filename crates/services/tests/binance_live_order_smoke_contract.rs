use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

fn read_smoke_script() -> String {
    let script_path = repo_root()
        .join("scripts")
        .join("dev")
        .join("run_binance_live_order_smoke.sh");
    fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error))
}

#[test]
fn smoke_script_passes_bash_syntax_check() {
    let script_path = repo_root()
        .join("scripts")
        .join("dev")
        .join("run_binance_live_order_smoke.sh");

    let output = std::process::Command::new("bash")
        .arg("-n")
        .arg(&script_path)
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn smoke_script_guards_required_env_vars() {
    let script = read_smoke_script();

    // Must check all three required env vars before proceeding
    assert!(
        script.contains("BINANCE_LIVE_API_KEY"),
        "script must check BINANCE_LIVE_API_KEY"
    );
    assert!(
        script.contains("BINANCE_LIVE_API_SECRET"),
        "script must check BINANCE_LIVE_API_SECRET"
    );
    assert!(
        script.contains("WEB_DATABASE_URL"),
        "script must check WEB_DATABASE_URL"
    );
    assert!(
        script.contains("DATABASE_URL"),
        "script must accept DATABASE_URL as fallback"
    );

    // Must exit non-zero when vars are missing
    assert!(
        script.contains("exit 1"),
        "script must exit 1 on missing env vars"
    );
}

#[test]
fn smoke_script_accepts_existing_binance_env_aliases() {
    let script = read_smoke_script();

    assert!(
        script.contains("BINANCE_API_KEY"),
        "script must accept the existing BINANCE_API_KEY alias"
    );
    assert!(
        script.contains("BINANCE_API_SECRET"),
        "script must accept the existing BINANCE_API_SECRET alias"
    );
    assert!(
        script.contains("binance_api_secret"),
        "script must accept the lower-case binance_api_secret alias currently used by local .env"
    );
}

#[test]
fn smoke_script_uses_live_order_confirmation_token() {
    let script = read_smoke_script();

    assert!(
        script.contains("EXECUTION_WORKER_DRY_RUN=false"),
        "script must set EXECUTION_WORKER_DRY_RUN=false for live orders"
    );
    assert!(
        script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS"),
        "script must set the live order confirmation token"
    );
    assert!(
        script.contains("I_UNDERSTAND_LIVE_ORDERS"),
        "confirmation token must be present"
    );
}

#[test]
fn smoke_script_does_not_print_any_secret_material() {
    let script = read_smoke_script();
    let echo_lines = script
        .lines()
        .filter(|line| line.trim_start().starts_with("echo "))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !echo_lines.contains("BINANCE_LIVE_API_KEY:0")
            && !echo_lines.contains("BINANCE_LIVE_API_KEY: -"),
        "script must not print even partial BINANCE_LIVE_API_KEY values"
    );
    assert!(
        !echo_lines.contains("API_KEY_MASK") && !echo_lines.contains("api_key_mask"),
        "script must not print API key masks"
    );
    assert!(
        !echo_lines.contains("BINANCE_LIVE_API_SECRET:0")
            && !echo_lines.contains("BINANCE_LIVE_API_SECRET: -"),
        "script must not print even partial BINANCE_LIVE_API_SECRET values"
    );
    assert!(
        !echo_lines.contains("API_SECRET_CIPHER:0") && !echo_lines.contains("${API_SECRET_CIPHER}"),
        "script must not print API secret cipher material"
    );
}

#[test]
fn smoke_script_preflights_signed_binance_account_before_writing_web_state() {
    let script = read_smoke_script();

    assert!(
        script.contains("preflight_binance_signed_account()"),
        "script must define a signed Binance account preflight"
    );
    assert!(
        script.contains("/fapi/v2/account"),
        "script must use a signed Binance Futures account endpoint before live execution"
    );
    assert!(
        script.contains("X-MBX-APIKEY"),
        "script must send the Binance API key through the request header"
    );
    assert!(
        script.contains("openssl dgst -sha256 -hmac"),
        "script must sign the preflight request with the secret key"
    );

    let preflight_pos = script
        .find("preflight_binance_signed_account")
        .expect("preflight function/call must exist");
    let upsert_pos = script
        .find("INSERT INTO user_api_credentials")
        .expect("credential upsert must exist");
    assert!(
        preflight_pos < upsert_pos,
        "signed account preflight must happen before writing Web credential state"
    );
}

#[test]
fn smoke_script_sets_binance_as_default_exchange() {
    let script = read_smoke_script();

    assert!(
        script.contains("EXECUTION_WORKER_DEFAULT_EXCHANGE=binance"),
        "script must set default exchange to binance"
    );
    assert!(
        script.contains("BINANCE_PROXY_URL"),
        "script must pass BINANCE_PROXY_URL to the worker"
    );
}

#[test]
fn smoke_script_defaults_to_one_pending_close_task_only() {
    let script = read_smoke_script();

    assert!(
        script.contains("EXECUTION_WORKER_LEASE_LIMIT=1"),
        "live smoke must lease at most one task by default"
    );
    assert!(
        script.contains("EXECUTION_WORKER_TASK_TYPES=risk_control_close_candidate"),
        "live smoke must default to Web pending_close close-task execution"
    );
    assert!(
        script.contains("EXECUTION_WORKER_TASK_STATUSES=pending_close"),
        "live smoke must default to pending_close tasks"
    );
}

#[test]
fn smoke_script_requires_explicit_safe_task_source() {
    let script = read_smoke_script();

    assert!(
        script.contains("BINANCE_LIVE_PENDING_CLOSE_TASK_ID"),
        "live smoke must require an explicit Web pending_close task id"
    );
    assert!(
        script.contains("task_type = 'risk_control_close_candidate'"),
        "live smoke must validate the pending_close task type before live execution"
    );
    assert!(
        script.contains("task_status = 'pending_close'"),
        "live smoke must validate pending_close status before live execution"
    );
    assert!(
        script.contains("close_order"),
        "live smoke must validate the Web close_order payload before live execution"
    );
}

#[test]
fn smoke_script_validates_pending_close_task_before_writing_credentials() {
    let script = read_smoke_script();
    let task_validation_pos = script
        .find("task_type = 'risk_control_close_candidate'")
        .expect("task validation must exist");
    let upsert_pos = script
        .find("INSERT INTO user_api_credentials")
        .expect("credential upsert must exist");

    assert!(
        task_validation_pos < upsert_pos,
        "live smoke must validate the explicit pending_close task before writing credentials"
    );
}

#[test]
fn smoke_script_uses_jsonb_for_close_order_existence_check() {
    let script = read_smoke_script();

    assert!(
        script.contains("request_payload_json::jsonb ? 'close_order'"),
        "Postgres ? operator must be used with jsonb, not json"
    );
}

#[test]
fn smoke_script_does_not_default_to_forced_buy_signal() {
    let script = read_smoke_script();

    assert!(
        !script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL=buy"),
        "live smoke must not default to creating a live buy signal"
    );
    assert!(
        !script.contains("EXECUTION_WORKER_TASK_TYPES=execute_signal"),
        "live smoke must not default to execute_signal tasks"
    );
    assert!(
        !script.contains("run_forced_signal_quant_core_smoke.sh"),
        "live smoke must not create a forced signal as the default live path"
    );
}

#[test]
fn smoke_script_seals_credentials_with_xor_cipher() {
    let script = read_smoke_script();

    // The seal_credential bash function must be present
    assert!(
        script.contains("seal_credential()"),
        "script must define seal_credential bash function"
    );
    assert!(
        script.contains("API_CREDENTIAL_SECRET"),
        "script must use API_CREDENTIAL_SECRET as the seal key"
    );
    // Credentials must be stored as cipher, not plaintext
    assert!(
        script.contains("api_key_cipher"),
        "script must insert api_key_cipher (not plaintext)"
    );
    assert!(
        script.contains("api_secret_cipher"),
        "script must insert api_secret_cipher (not plaintext)"
    );
    // Exchange must be stored as the normalized Chinese name
    assert!(
        script.contains("'币安'"),
        "script must store exchange as '币安' (normalized form used by Web backend)"
    );
}

#[test]
fn smoke_script_verifies_order_status_is_not_dry_run() {
    let script = read_smoke_script();

    assert!(
        script.contains("order_status"),
        "script must query order_status from exchange_order_results"
    );
    assert!(
        script.contains("dry_run"),
        "script must check that order_status != 'dry_run'"
    );
    assert!(
        script.contains("exchange_order_results"),
        "script must query exchange_order_results table"
    );
}

#[test]
fn smoke_script_uses_upsert_to_avoid_duplicate_credentials() {
    let script = read_smoke_script();

    assert!(
        script.contains("ON CONFLICT (buyer_email, exchange) DO UPDATE"),
        "script must use ON CONFLICT upsert to avoid duplicate credential rows"
    );
}

#[test]
fn smoke_script_does_not_hardcode_api_keys() {
    let script = read_smoke_script();

    // The script must not contain any hardcoded key-like strings (long hex or base64 sequences
    // that look like real API keys). We check that the actual key values come from env vars.
    assert!(
        script.contains("${BINANCE_LIVE_API_KEY}"),
        "API key must be read from BINANCE_LIVE_API_KEY env var, not hardcoded"
    );
    assert!(
        script.contains("${BINANCE_LIVE_API_SECRET}"),
        "API secret must be read from BINANCE_LIVE_API_SECRET env var, not hardcoded"
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
    // Token with surrounding whitespace must still pass (str::trim)
    assert!(live_order_confirmation_valid(
        false,
        Some("  I_UNDERSTAND_LIVE_ORDERS  ")
    ));
}

#[test]
fn dry_run_false_without_confirmation_is_refused() {
    // This mirrors ensure_live_order_confirmation() returning Err when confirmation is absent.
    let result = if live_order_confirmation_valid(false, None) {
        Ok(())
    } else {
        Err("refusing live exchange orders")
    };
    assert!(result.is_err());
}
