use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

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
        .join("inspect_market_velocity_okx_post_trade.sh")
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn temp_contract_dir(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "market_velocity_okx_post_trade_monitor_contract_{}_{}",
        std::process::id(),
        test_name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("scripts").join("dev")).unwrap();
    path
}

#[test]
fn market_velocity_okx_post_trade_monitor_script_passes_bash_syntax_check() {
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
fn market_velocity_okx_post_trade_monitor_is_read_only_and_never_closes_position() {
    let script = fs::read_to_string(script_path()).expect("post-trade monitor script should exist");

    assert!(script.contains("SELECT"));
    assert!(!script.contains("UPDATE "));
    assert!(!script.contains("INSERT "));
    assert!(!script.contains("DELETE "));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
    assert!(!script.contains("MARKET_VELOCITY_LIVE_WORKER_APPLY=true"));
    assert!(!script.contains("run_worker_once"));
    assert!(!script.contains("/api/v5/trade/order"));
}

#[test]
fn market_velocity_okx_post_trade_monitor_reports_filled_order_and_close_auth_boundary() {
    let temp_dir = temp_contract_dir("reports_filled_order");
    let script_dir = temp_dir.join("scripts").join("dev");
    let monitor_script = script_dir.join("inspect_market_velocity_okx_post_trade.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    fs::copy(script_path(), &monitor_script).unwrap();

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"post_trade_task_summary\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tcompleted\\tASTER-USDT-SWAP\\tokx\\tmarket_velocity\\t5.0\\t0.605052\\tlong\\t0.6174\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_order_summary\"* ]]; then\n\
  printf '28\\tokx\\t3631557801300238336\\tbuy\\tfilled\\t1.00000000\\t0.60700000\\t-0.00030350\\ttrue\\t3631557801270792192\\tattached_stop_loss\\t0.605\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_attempt_summary\"* ]]; then\n\
  printf '29\\t1\\tcompleted\\trust_quant\\tnone\\t2026-06-06 07:34:17.000438\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&monitor_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run post-trade monitor script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "post-trade monitor should accept completed filled OKX task\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("mode=read_only"));
    assert!(stdout.contains("post_trade_task=task_id:86 combo_id:85 status:completed symbol:ASTER-USDT-SWAP exchange:okx source_signal_type:market_velocity size_usdt:5.0"));
    assert!(stdout.contains("post_trade_order=order_result_id:28 exchange:okx side:buy status:filled external_order_id:3631557801300238336 filled_qty:1.00000000 filled_quote:0.60700000 fee:-0.00030350"));
    assert!(stdout.contains("post_trade_protection=status:confirmed external_id:3631557801270792192 mode:attached_stop_loss stop_loss:0.605"));
    assert!(stdout.contains("post_trade_attempt=attempt_id:29 attempt_no:1 status:completed executor:rust_quant error:none updated_at:2026-06-06 07:34:17.000438"));
    assert!(stdout.contains("mutation_allowed=false"));
    assert!(stdout.contains("live_close_requires_separate_authorization=true"));
    assert!(stdout.contains("close_authorization_scope=task_id:86 symbol:ASTER-USDT-SWAP filled_qty:1.00000000 max_notional:5"));
    assert!(stdout.contains("signed_snapshot_recheck=available"));
    assert!(stdout.contains("signed_snapshot_recheck_scope=task_id:86 symbol:ASTER-USDT-SWAP exchange:okx report:false include_fills:true mutation_allowed:false"));
    assert!(stdout.contains("signed_snapshot_recheck_requirements=MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE=true bash scripts/dev/inspect_market_velocity_okx_post_trade.sh"));
    assert!(stdout.contains("signed_snapshot_recheck_secret_required=RUST_QUAN_WEB_BASE_URL_and_EXECUTION_EVENT_SECRET_or_RUST_QUAN_WEB_INTERNAL_SECRET"));
    assert!(stdout.contains("close_fill_writeback_apply=available_after_signed_flat_candidate"));
    assert!(stdout.contains("close_fill_writeback_apply_scope=task_id:86 symbol:ASTER-USDT-SWAP exchange:okx web_writeback_only:true exchange_mutation_allowed:false"));
    assert!(stdout.contains("close_fill_writeback_apply_requirements=MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE=true RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY=true RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM=I_UNDERSTAND_THIS_WRITES_EXCHANGE_CLOSE_FILL_TO_WEB RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT=web-close-fill:combo=85:task=86:symbol=ASTER-USDT-SWAP bash scripts/dev/inspect_market_velocity_okx_post_trade.sh"));
    assert!(!stdout.contains("buyer@example.com"));
    assert!(!stdout.contains("local-dev-secret"));
    assert!(!stdout.contains("MARKET_VELOCITY_LIVE_WORKER_APPLY=true"));
    assert!(!stdout.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_post_trade_monitor_runs_signed_read_only_snapshot_only_when_enabled() {
    let temp_dir = temp_contract_dir("runs_signed_read_only_snapshot");
    let script_dir = temp_dir.join("scripts").join("dev");
    let monitor_script = script_dir.join("inspect_market_velocity_okx_post_trade.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    let snapshot_env_file = temp_dir.join("snapshot_env");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    fs::copy(script_path(), &monitor_script).unwrap();

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"post_trade_task_summary\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tcompleted\\tASTER-USDT-SWAP\\tokx\\tmarket_velocity\\t5.0\\t0.605052\\tlong\\t0.6174\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_order_summary\"* ]]; then\n\
  printf '28\\tokx\\t3631557801300238336\\tbuy\\tfilled\\t1.00000000\\t0.60700000\\t-0.00030350\\ttrue\\t3631557801270792192\\tattached_stop_loss\\t0.605\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_attempt_summary\"* ]]; then\n\
  printf '29\\t1\\tcompleted\\trust_quant\\tnone\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"signed_reconciliation_task_context\"* ]]; then\n\
  printf 'buyer@example.com\\tASTER-USDT-SWAP\\t85\\t1444950\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_snapshot_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_snapshot_binary,
        format!(
            "#!/usr/bin/env bash\n\
{{\n\
  printf 'IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=%s\\n' \"${{IS_RUN_RECONCILIATION_SNAPSHOT_CHECK:-}}\"\n\
  printf 'IS_RUN_EXECUTION_WORKER=%s\\n' \"${{IS_RUN_EXECUTION_WORKER:-}}\"\n\
  printf 'IS_RUN_REAL_STRATEGY=%s\\n' \"${{IS_RUN_REAL_STRATEGY:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_CONFIRM=%s\\n' \"${{RECONCILIATION_SNAPSHOT_CONFIRM:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_BUYER_EMAIL=%s\\n' \"${{RECONCILIATION_SNAPSHOT_BUYER_EMAIL:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_EXCHANGE=%s\\n' \"${{RECONCILIATION_SNAPSHOT_EXCHANGE:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_SYMBOL=%s\\n' \"${{RECONCILIATION_SNAPSHOT_SYMBOL:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_COMBO_ID=%s\\n' \"${{RECONCILIATION_SNAPSHOT_COMBO_ID:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_TASK_ID=%s\\n' \"${{RECONCILIATION_SNAPSHOT_TASK_ID:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_REPORT=%s\\n' \"${{RECONCILIATION_SNAPSHOT_REPORT:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_INCLUDE_FILLS=%s\\n' \"${{RECONCILIATION_SNAPSHOT_INCLUDE_FILLS:-}}\"\n\
  printf 'RECONCILIATION_SNAPSHOT_CREDENTIAL_REF=%s\\n' \"${{RECONCILIATION_SNAPSHOT_CREDENTIAL_REF:-}}\"\n\
  printf 'RUST_QUAN_WEB_BASE_URL=%s\\n' \"${{RUST_QUAN_WEB_BASE_URL:-}}\"\n\
}} > '{}'\n\
printf '{{\"mutation_allowed\":false,\"place_order_allowed\":false,\"report_result_allowed\":false,\"non_zero_position_count\":1,\"active_open_order_count\":0}}\\n'\n",
            snapshot_env_file.display()
        ),
    )
    .unwrap();
    make_executable(&fake_snapshot_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&monitor_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8000")
        .env("EXECUTION_EVENT_SECRET", "local-dev-secret")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE", "true")
        .output()
        .expect("bash should run post-trade monitor script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "signed read-only snapshot recheck should succeed when explicitly enabled\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("signed_snapshot_recheck=okx_signed_readonly_reconciliation_snapshot"));
    assert!(stdout.contains("\"mutation_allowed\":false"));
    assert!(stdout.contains("\"place_order_allowed\":false"));
    assert!(!stdout.contains("buyer@example.com"));
    assert!(!stdout.contains("local-dev-secret"));

    let snapshot_env = fs::read_to_string(&snapshot_env_file).unwrap();
    assert!(snapshot_env.contains("IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=true"));
    assert!(snapshot_env.contains("IS_RUN_EXECUTION_WORKER=false"));
    assert!(snapshot_env.contains("IS_RUN_REAL_STRATEGY=false"));
    assert!(snapshot_env
        .contains("RECONCILIATION_SNAPSHOT_CONFIRM=I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_BUYER_EMAIL=buyer@example.com"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_EXCHANGE=okx"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_SYMBOL=ASTER-USDT-SWAP"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_COMBO_ID=85"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_TASK_ID=86"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_REPORT=false"));
    assert!(snapshot_env.contains("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS=true"));
    assert!(snapshot_env
        .contains("RECONCILIATION_SNAPSHOT_CREDENTIAL_REF=web_api_credential_id_1444950"));
    assert!(snapshot_env.contains("RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_post_trade_monitor_apply_fails_fast_when_web_writeback_route_is_missing() {
    let temp_dir = temp_contract_dir("apply_fails_fast_missing_writeback_route");
    let script_dir = temp_dir.join("scripts").join("dev");
    let monitor_script = script_dir.join("inspect_market_velocity_okx_post_trade.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    let snapshot_env_file = temp_dir.join("snapshot_env");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    fs::copy(script_path(), &monitor_script).unwrap();

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"post_trade_task_summary\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tcompleted\\tASTER-USDT-SWAP\\tokx\\tmarket_velocity\\t5.0\\t0.605052\\tlong\\t0.6174\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_order_summary\"* ]]; then\n\
  printf '28\\tokx\\t3631557801300238336\\tbuy\\tfilled\\t1.00000000\\t0.60700000\\t-0.00030350\\ttrue\\t3631557801270792192\\tattached_stop_loss\\t0.605\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_attempt_summary\"* ]]; then\n\
  printf '29\\t1\\tcompleted\\trust_quant\\tnone\\t2026-06-06 07:34:17.000438\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_curl = bin_dir.join("curl");
    fs::write(&fake_curl, "#!/usr/bin/env bash\nprintf '404'\n").unwrap();
    make_executable(&fake_curl);

    let fake_snapshot_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_snapshot_binary,
        format!(
            "#!/usr/bin/env bash\n\
printf 'snapshot should not run\\n' > '{}'\n",
            snapshot_env_file.display()
        ),
    )
    .unwrap();
    make_executable(&fake_snapshot_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&monitor_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8000")
        .env("EXECUTION_EVENT_SECRET", "local-dev-secret")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE", "true")
        .env("RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY", "true")
        .env(
            "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM",
            "I_UNDERSTAND_THIS_WRITES_EXCHANGE_CLOSE_FILL_TO_WEB",
        )
        .env(
            "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT",
            "web-close-fill:combo=85:task=86:symbol=ASTER-USDT-SWAP",
        )
        .output()
        .expect("bash should run post-trade monitor script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "close-fill apply should fail fast when Web writeback route is missing\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stderr.contains("blocker=close_fill_writeback_route_missing"));
    assert!(stderr.contains("http://127.0.0.1:8000"));
    assert!(
        !snapshot_env_file.exists(),
        "signed snapshot binary must not run"
    );
    assert!(!stdout.contains("snapshot should not run"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_post_trade_monitor_fails_without_confirmed_protection() {
    let temp_dir = temp_contract_dir("fails_without_confirmed_protection");
    let script_dir = temp_dir.join("scripts").join("dev");
    let monitor_script = script_dir.join("inspect_market_velocity_okx_post_trade.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    fs::copy(script_path(), &monitor_script).unwrap();

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"post_trade_task_summary\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tcompleted\\tASTER-USDT-SWAP\\tokx\\tmarket_velocity\\t5.0\\t0.605052\\tlong\\t0.6174\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_order_summary\"* ]]; then\n\
  printf '28\\tokx\\t3631557801300238336\\tbuy\\tfilled\\t1.00000000\\t0.60700000\\t-0.00030350\\tfalse\\t\\tattached_stop_loss\\t0.605\\t2026-06-06 07:34:17.000438\\n'\n\
elif [[ \"${args}\" == *\"post_trade_attempt_summary\"* ]]; then\n\
  printf '29\\t1\\tcompleted\\trust_quant\\tnone\\t2026-06-06 07:34:17.000438\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&monitor_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run post-trade monitor script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "post-trade monitor should fail closed when filled order lacks confirmed protection\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("post_trade_order=order_result_id:28"));
    assert!(stderr.contains("blocker=post_trade_protection_not_confirmed"));
    assert!(!stdout.contains("live_close_requires_separate_authorization=true"));

    let _ = fs::remove_dir_all(&temp_dir);
}
