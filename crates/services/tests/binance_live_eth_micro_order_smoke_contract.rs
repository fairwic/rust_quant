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
fn smoke_script_is_eth_only_and_refuses_linkusdt() {
    let script = read_smoke_script();

    assert!(
        script.contains("ETHUSDT") && script.contains("ETH-USDT-SWAP"),
        "ETH native and Web symbols must both be hardcoded"
    );
    assert!(
        script.contains("case \"${BINANCE_ETH_MICRO_SYMBOL}\" in")
            && script.contains("ETHUSDT|ETH-USDT-SWAP"),
        "script must validate that the configured symbol is ETHUSDT/ETH-USDT-SWAP only"
    );
    assert!(
        script.contains("Refusing non-ETH symbol"),
        "script must explicitly refuse any non-ETH symbol"
    );
    assert!(
        !script.contains("LINKUSDT") && !script.contains("LINK-USDT-SWAP"),
        "script must not reference LINK symbols"
    );
}

#[test]
fn smoke_script_defaults_to_tiny_eth_quantity_and_requires_micro_confirmation() {
    let script = read_smoke_script();

    assert!(
        script.contains("BINANCE_ETH_MICRO_QTY:=\"0.010\""),
        "script must default to a tiny ETH quantity that clears Binance USD-M minNotional in normal ETH price ranges"
    );
    assert!(
        script.contains("I_UNDERSTAND_TINY_ETH_LIVE_ORDER"),
        "script must require the tiny ETH live-order confirmation token"
    );
    assert!(
        script.contains("BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM"),
        "script must use a script-level confirmation env var"
    );
    assert!(
        script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS"),
        "script must still pass the worker's live-order confirmation token internally"
    );
}

#[test]
fn smoke_script_preflights_account_and_exchange_filters_before_web_state_changes() {
    let script = read_smoke_script();

    assert!(
        script.contains("preflight_binance_signed_account()"),
        "script must define a signed account preflight"
    );
    assert!(
        script.contains("/fapi/v2/account"),
        "script must call Binance signed Futures account endpoint"
    );
    assert!(
        script.contains("X-MBX-APIKEY"),
        "script must send API key through Binance request header"
    );
    assert!(
        script.contains("openssl dgst -sha256 -hmac"),
        "script must sign the account preflight request"
    );
    assert!(
        script.contains("preflight_binance_exchange_info_filters()"),
        "script must define exchangeInfo filter preflight"
    );
    assert!(
        script.contains("/fapi/v1/exchangeInfo?symbol=ETHUSDT"),
        "script must fetch ETHUSDT exchangeInfo filters"
    );
    assert!(
        script.contains("/fapi/v1/premiumIndex?symbol=ETHUSDT"),
        "script must fetch ETH mark price for notional validation"
    );

    let account_pos = script
        .find("preflight_binance_signed_account")
        .expect("signed account preflight must exist");
    let filters_pos = script
        .find("preflight_binance_exchange_info_filters")
        .expect("exchangeInfo preflight must exist");
    let credential_pos = script
        .find("verify_existing_binance_credential_ready")
        .expect("existing v3 credential gate must exist");
    let task_pos = script
        .find("INSERT INTO execution_tasks")
        .expect("task insert must exist");

    assert!(
        account_pos < credential_pos && filters_pos < credential_pos && filters_pos < task_pos,
        "Binance preflights must happen before Web credential gate and task state changes"
    );
}

#[test]
fn smoke_script_preflights_available_margin_before_web_state_changes() {
    let script = read_smoke_script();

    assert!(
        script.contains("BINANCE_ETH_MICRO_NOTIONAL"),
        "script must persist the ETH micro notional computed from exchange filters"
    );
    assert!(
        script.contains("preflight_binance_margin_available()"),
        "script must define a signed available-margin preflight"
    );
    assert!(
        script.contains("availableBalance") && script.contains("USDT"),
        "script must inspect Binance Futures available USDT balance"
    );
    assert!(
        script.contains("preflight_margin_available=ok"),
        "script must print a non-sensitive margin preflight success summary"
    );

    let margin_pos = script
        .find("preflight_binance_margin_available")
        .expect("margin preflight must exist");
    let credential_pos = script
        .find("verify_existing_binance_credential_ready")
        .expect("existing v3 credential gate must exist");
    let task_pos = script
        .find("INSERT INTO execution_tasks")
        .expect("task insert must exist");

    assert!(
        margin_pos < credential_pos && margin_pos < task_pos,
        "available-margin preflight must happen before Web credential gate and task state changes"
    );
}

#[test]
fn smoke_script_loads_local_env_files_without_executing_them() {
    let script = read_smoke_script();

    assert!(
        script.contains("load_env_file_safe()"),
        "script must load local .env files through a parser, not shell source"
    );
    assert!(
        script.contains("safe_assign_env")
            && script.contains("rust_quan_web/backend/.env")
            && script.contains("WEB_DATABASE_URL"),
        "script must safely map Web backend DATABASE_URL to WEB_DATABASE_URL"
    );
    assert!(
        !script.contains(". \"${REPO_ROOT}/.env\"")
            && !script.contains("source \"${REPO_ROOT}/.env\""),
        "script must not source .env because malformed/non-kv lines can execute"
    );
}

#[test]
fn smoke_script_requires_eth_position_flat_before_and_after_live_smoke() {
    let script = read_smoke_script();

    assert!(
        script.contains("positionAmt") && script.contains("ETHUSDT"),
        "script must inspect the signed account ETHUSDT position amount"
    );
    assert!(
        script.contains("preflight_eth_position_flat"),
        "script must refuse to open a micro ETH order when ETHUSDT already has a live position"
    );
    assert!(
        script.contains("verify_final_eth_position_flat"),
        "script must verify ETHUSDT is flat after the reduce-only close"
    );
    assert!(
        script.contains("final_eth_position=flat"),
        "script must print a non-sensitive final flat-position summary"
    );
}

#[test]
fn smoke_script_requires_no_eth_open_orders_before_and_after_live_smoke() {
    let script = read_smoke_script();

    assert!(
        script.contains("/fapi/v1/openOrders") && script.contains("symbol=ETHUSDT"),
        "script must query ETHUSDT open orders through Binance signed read-only API"
    );
    assert!(
        script.contains("preflight_eth_open_orders_clear"),
        "script must refuse to start when ETHUSDT has existing open orders"
    );
    assert!(
        script.contains("final_eth_open_orders_clear"),
        "script must verify no ETHUSDT open orders remain after the smoke"
    );
}

#[test]
fn smoke_script_supports_one_way_and_hedge_position_modes_for_close() {
    let script = read_smoke_script();

    assert!(
        script.contains("/fapi/v1/positionSide/dual"),
        "script must inspect Binance Futures position mode before a live close smoke"
    );
    assert!(
        script.contains("dualSidePosition"),
        "script must parse the Binance position mode response"
    );
    assert!(
        script.contains("binance_position_mode=one_way"),
        "script must print a non-sensitive one-way mode summary before live order placement"
    );
    assert!(
        script.contains("binance_position_mode=hedge")
            && script.contains("BINANCE_ETH_MICRO_POSITION_MODE=hedge")
            && script.contains("BINANCE_ETH_MICRO_POSITION_SIDE=long"),
        "script must support Binance Hedge Mode by routing orders to positionSide=LONG"
    );

    let mode_pos = script
        .find("preflight_binance_position_mode")
        .expect("position mode preflight must exist");
    let credential_pos = script
        .find("verify_existing_binance_credential_ready")
        .expect("existing v3 credential gate must exist");
    let task_pos = script
        .find("INSERT INTO execution_tasks")
        .expect("task insert must exist");

    assert!(
        mode_pos < credential_pos && mode_pos < task_pos,
        "Binance position mode preflight must happen before Web credential gate and task state changes"
    );
}

#[test]
fn smoke_script_proves_open_then_immediate_reduce_only_close_intent() {
    let script = read_smoke_script();

    assert!(
        script.contains("task_type")
            && script.contains("task_status")
            && script.contains("'execute_signal', 'pending'")
            && script.contains("'trade_side', 'open'"),
        "script must create an open execute_signal task"
    );
    assert!(
        script.contains("'risk_control_close_candidate', 'pending_close'")
            && script.contains("'close_order'")
            && script.contains("'position_side', NULLIF(:'position_side', '')")
            && script.contains("'reduce_only', NULLIF(:'close_reduce_only', '')::boolean")
            && script.contains("'trade_side', 'close'"),
        "script must create an immediate pending_close task with mode-aware close payload"
    );
    assert!(
        script.contains("run_execution_worker_once \"open\"")
            && script.contains("run_execution_worker_once \"close\""),
        "script must run the worker once for open and once for close"
    );

    let open_insert_pos = script
        .find("create_open_execution_task")
        .expect("open task function/call must exist");
    let close_insert_pos = script
        .find("create_close_execution_task")
        .expect("close task function/call must exist");
    let close_payload_pos = script
        .find("'reduce_only', NULLIF(:'close_reduce_only', '')::boolean")
        .expect("mode-aware close reduce_only intent must exist");

    assert!(
        open_insert_pos < close_insert_pos && open_insert_pos < close_payload_pos,
        "open task must be created before close task and close payload must be mode-aware"
    );
}

#[test]
fn smoke_script_open_task_carries_protective_stop_loss_contract() {
    let script = read_smoke_script();

    assert!(
        script.contains("BINANCE_ETH_MICRO_STOP_LOSS_PRICE"),
        "script must derive or accept a protective stop-loss price before live open"
    );
    assert!(
        script.contains("'risk_plan'")
            && script.contains("'selected_stop_loss_price'")
            && script.contains("'protective_stop_loss_required', true")
            && script.contains("'direction', 'long'"),
        "open task payload must request a protective stop-loss contract"
    );
}

#[test]
fn smoke_script_requires_protection_sync_success_before_claiming_open_success() {
    let script = read_smoke_script();

    assert!(
        script.contains("verify_open_protection_sync()"),
        "script must define an explicit protection_sync verifier for the live open result"
    );
    assert!(
        script.contains("contract_version")
            && script.contains("protective_order_mode")
            && script.contains("protective_order_confirmed")
            && script.contains("protective_order_external_id"),
        "protection verifier must inspect the v2 protection_sync evidence fields"
    );
    assert!(
        script.contains("open_protection_sync=confirmed"),
        "script must print a non-sensitive protection confirmation summary"
    );

    let open_result_pos = script
        .find("verify_order_result \"${OPEN_TASK_ID}\" \"buy\" \"open\"")
        .expect("open order result verification must exist");
    let protection_pos = script
        .find("verify_open_protection_sync \"${OPEN_TASK_ID}\"")
        .expect("open protection verifier call must exist");
    let close_task_pos = script
        .find("CLOSE_TASK_ID=\"$(create_close_execution_task")
        .expect("close task creation must exist");

    assert!(
        open_result_pos < protection_pos && protection_pos < close_task_pos,
        "open protection_sync must be verified after the open fill and before close creation"
    );
}

#[test]
fn smoke_script_attempts_web_close_if_open_stage_fails_with_eth_position() {
    let script = read_smoke_script();

    assert!(
        script.contains("handle_open_stage_failure()"),
        "script must have an explicit open-stage failure handler"
    );
    assert!(
        script.contains("emergency_close_eth_position_via_web"),
        "open-stage failure handling must attempt close through the Web task/worker path"
    );
    assert!(
        script.contains("open stage failed after possible live order placement"),
        "failure handler must make the production risk obvious in logs"
    );
}

#[test]
fn smoke_script_uses_production_ready_credential_code_and_rejects_failed_results() {
    let script = read_smoke_script();

    assert!(
        script.contains("verify_existing_binance_credential_ready()"),
        "script must verify an existing Web credential instead of writing one locally"
    );
    assert!(
        script.contains("'signed_exchange_preflight_passed'")
            && script.contains("'signed_exchange_check_passed'"),
        "credential gate must require production-ready signed preflight codes"
    );
    assert!(
        script.contains("api_key_cipher LIKE 'v3:aes256gcm:%'")
            && script.contains("api_secret_cipher LIKE 'v3:aes256gcm:%'"),
        "credential gate must require v3 AEAD envelopes"
    );
    assert!(
        script.contains(r#"${order_status}" == "failed"#)
            && script.contains(r#"${task_status}" == "failed"#)
            && script.contains("refusing to claim live smoke success"),
        "script must fail closed when Web reports a failed order/task result"
    );
    assert!(
        script.contains(r#"${order_status_upper}" != "FILLED"#),
        "script must require live ETH market smoke orders to be FILLED, not just non-failed"
    );
}

#[test]
fn smoke_script_does_not_generate_or_upsert_legacy_credential_material() {
    let script = read_smoke_script();

    assert!(
        !script.contains("API_CREDENTIAL_SECRET"),
        "script must not accept the removed legacy API_CREDENTIAL_SECRET alias"
    );
    assert!(
        !script.contains("seal_credential()") && !script.contains("seal_credential \""),
        "script must not generate local legacy credential ciphertext"
    );
    assert!(
        !script.contains("INSERT INTO user_api_credentials")
            && !script.contains("ON CONFLICT (buyer_email, exchange) DO UPDATE"),
        "script must not upsert API credentials; credentials must be saved through Web v3 envelope flow"
    );
}

#[test]
fn smoke_script_uses_web_worker_exchange_web_path_not_direct_order_endpoint() {
    let script = read_smoke_script();

    assert!(
        script.contains("RUST_QUAN_WEB_BASE_URL")
            && script.contains("EXECUTION_WORKER_DEFAULT_EXCHANGE=binance")
            && script.contains("EXECUTION_WORKER_DRY_RUN=false")
            && script.contains("exchange_order_results"),
        "script must use Web task leasing, worker execution, and Web order-result verification"
    );
    assert!(
        !script.contains("/fapi/v1/order"),
        "script must not place direct Binance order REST calls; worker owns live order placement"
    );
}

#[test]
fn smoke_script_isolates_worker_lease_scope_before_each_live_worker_run() {
    let script = read_smoke_script();

    assert!(
        script.contains("assert_target_is_next_leaseable_task()"),
        "script must guard worker lease isolation"
    );
    assert!(
        script.contains("task_status = :'task_status'")
            && script.contains("(other.task_status = 'leased' AND other.lease_until < NOW())"),
        "script must mirror Web leasing semantics, including expired leased tasks"
    );
    assert!(
        script.contains("other.priority > target.priority")
            && script.contains("other.priority = target.priority")
            && script.contains("other.scheduled_at < target.scheduled_at")
            && script.contains("other.scheduled_at = target.scheduled_at")
            && script.contains("other.id < target.id"),
        "script must prove no other leaseable task sorts before the pinned target"
    );
    assert!(
        script.contains("refusing live worker run because another task would be leased first"),
        "script must fail closed when another task would sort ahead of the target"
    );
    assert!(
        script.contains("EXECUTION_WORKER_LEASE_LIMIT=1"),
        "script must lease exactly one task per worker run"
    );
}

#[test]
fn smoke_script_pins_each_worker_run_to_the_current_task_id() {
    let script = read_smoke_script();

    assert!(
        script.contains("EXECUTION_WORKER_TARGET_TASK_IDS=\"${task_id}\""),
        "each live worker run must export exactly the current stage task id"
    );
    assert!(
        script.contains("unset EXECUTION_WORKER_TARGET_TASK_IDS"),
        "script must clear the target task id allowlist after the worker process exits"
    );
}

#[test]
fn smoke_script_does_not_print_secret_or_cipher_material() {
    let script = read_smoke_script();
    let printed_lines = script
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("echo ") || trimmed.starts_with("printf ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !printed_lines.contains("${BINANCE_LIVE_API_KEY}")
            && !printed_lines.contains("${BINANCE_LIVE_API_SECRET}")
            && !printed_lines.contains("${API_KEY_CIPHER}")
            && !printed_lines.contains("${API_SECRET_CIPHER}")
            && !printed_lines.contains("${WEB_DATABASE_URL}"),
        "script must not print keys, ciphers, or raw database URLs"
    );
    assert!(
        script.contains("BINANCE_LIVE_API_KEY=<set>")
            && script.contains("BINANCE_LIVE_API_SECRET=<set>")
            && script.contains("WEB_DATABASE_URL=<set>"),
        "script should print only set/unset safety summary values"
    );
}
