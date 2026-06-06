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
        .join("run_market_velocity_okx_scoped_live_worker.sh")
}

fn read_worker_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path)
        .unwrap_or_else(|error| panic!("failed to stat {}: {}", path.display(), error))
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .unwrap_or_else(|error| panic!("failed to chmod {}: {}", path.display(), error));
}

fn temp_contract_dir(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "market_velocity_worker_contract_{}_{}",
        std::process::id(),
        test_name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("scripts").join("dev")).unwrap_or_else(|error| {
        panic!(
            "failed to create temp contract dir {}: {}",
            path.display(),
            error
        );
    });
    path
}

#[test]
fn market_velocity_okx_scoped_live_worker_script_passes_bash_syntax_check() {
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
fn market_velocity_okx_scoped_live_worker_requires_two_live_confirmations() {
    let script = read_worker_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_WORKER_APPLY"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_WORKER_CONFIRM"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_WORKER_INTENT"));
    assert!(script.contains("I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER"));
    assert!(script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS"));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=false"));
}

#[test]
fn market_velocity_okx_scoped_live_worker_forces_single_target_task_scope() {
    let script = read_worker_script();

    assert!(script.contains("run_market_velocity_okx_live_preflight.sh"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_TARGET_TASK_ID"));
    assert!(script
        .contains("EXECUTION_WORKER_TARGET_TASK_IDS=\"${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}\""));
    assert!(script.contains("EXECUTION_WORKER_LEASE_LIMIT=1"));
    assert!(script.contains("EXECUTION_WORKER_TASK_TYPES=execute_signal"));
    assert!(script.contains("EXECUTION_WORKER_TASK_STATUSES=pending"));
    assert!(script.contains("EXECUTION_WORKER_RUN_ONCE=true"));
    assert!(script.contains("EXECUTION_WORKER_ONLY=true"));
}

#[test]
fn market_velocity_okx_scoped_live_worker_preserves_okx_and_link_safety_gates() {
    let script = read_worker_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT"));
    assert!(script.contains("EXECUTION_WORKER_DEFAULT_EXCHANGE=okx"));
    assert!(script.contains("if [[ -n \"${OKX_REQUEST_EXPIRATION_MS:-}\" ]]; then"));
    assert!(script.contains("export OKX_REQUEST_EXPIRATION_MS"));
    assert!(!script.contains("OKX_REQUEST_EXPIRATION_MS=\"${OKX_REQUEST_EXPIRATION_MS:-300000}\""));
    assert!(script.contains("UPPER(REPLACE(et.symbol, '-', '')) LIKE 'LINKUSDT%'"));
    assert!(script.contains("Refusing to run protected LINK"));
    assert!(!script.contains("EXECUTION_WORKER_DEFAULT_EXCHANGE=binance"));
}

#[test]
fn market_velocity_okx_scoped_live_worker_rejects_invalid_request_expiration_handoff() {
    let temp_dir = temp_contract_dir("invalid_request_expiration_handoff");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:35:17.792702\\tpresent\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!("failed to write fake podman {}: {}", fake_podman.display(), error);
    });
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("OKX_REQUEST_EXPIRATION_MS", "300000ms")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid explicit request expiration should fail before handoff\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stderr.contains("blocker=okx_request_expiration_ms_invalid detail=300000ms"));
    assert!(!stdout.contains("live_apply_manifest=market_velocity_okx_scoped_worker"));
    assert!(!stdout.contains("OKX_REQUEST_EXPIRATION_MS=300000ms"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_refreshes_expired_risk_context_before_live_scope() {
    let script = read_worker_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_REFRESH_READINESS"));
    assert!(script.contains("readiness_refresh_enabled"));
    assert!(script.contains("task_risk_context_expired"));
    assert!(script.contains("readiness_refresh=expired_risk_context"));
    assert!(script.contains("/api/commerce/internal/execution-tasks/lease?limit=1"));
    assert!(script.contains("/api/commerce/internal/api-credentials/${credential_id}/check"));
    assert!(script.contains("run_market_velocity_okx_live_preflight.sh"));
    assert!(!script.contains("/api/v5/trade/order"));
}

#[test]
fn market_velocity_okx_scoped_live_worker_dry_run_does_not_refresh_readiness_by_default() {
    let temp_dir = temp_contract_dir("dry_run_no_default_readiness_refresh");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let curl_marker = temp_dir.join("curl_invoked");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118' >&2\n\
echo 'preflight=blocked failures=1'\n\
exit 2\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(&fake_podman, "#!/usr/bin/env bash\nprintf '123\\n'\n").unwrap_or_else(|error| {
        panic!(
            "failed to write fake podman {}: {}",
            fake_podman.display(),
            error
        );
    });
    make_executable(&fake_podman);

    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        format!(
            "#!/usr/bin/env bash\nprintf 'curl-called' > '{}'\nprintf '{{\"code\":0}}\\n'\n",
            curl_marker.display()
        ),
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake curl {}: {}",
            fake_curl.display(),
            error
        );
    });
    make_executable(&fake_curl);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "dry-run expired risk context should stop without readiness refresh\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("refresh_readiness=auto"));
    assert!(stdout.contains("readiness_recovery_dry_run=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 scripts/dev/recover_market_velocity_okx_live_readiness.sh"));
    assert!(stdout.contains("readiness_recovery_apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY=true MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM=I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT=okx-readiness:task=86:max_notional=5"));
    assert!(stderr.contains("blocker=readiness_refresh_disabled"));
    assert!(!stdout.contains("readiness_refresh=expired_risk_context"));
    assert!(!stdout.contains("worker=dry_run") && !stdout.contains("worker=apply"));
    assert!(
        !curl_marker.exists(),
        "dry-run default must not call Web lease or credential check"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_fails_when_readiness_refresh_leaves_credential_not_ready()
{
    let temp_dir = temp_contract_dir("readiness_refresh_credential_not_ready");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let preflight_count_file = temp_dir.join("preflight_count");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        format!(
            "#!/usr/bin/env bash\n\
count_file='{}'\n\
count=$(cat \"${{count_file}}\" 2>/dev/null || printf '0')\n\
count=$((count + 1))\n\
printf '%s' \"${{count}}\" > \"${{count_file}}\"\n\
echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118' >&2\n\
echo 'preflight=blocked failures=1'\n\
exit 2\n",
            preflight_count_file.display()
        ),
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"FROM user_api_credentials u\"* ]]; then\n\
  printf '123\\n'\n\
elif [[ \"${args}\" == *\"readiness_refresh_credential_status\"* ]]; then\n\
  printf 'error\\tokx_preflight_network_error\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake podman {}: {}",
            fake_podman.display(),
            error
        );
    });
    make_executable(&fake_podman);

    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        "#!/usr/bin/env bash\nprintf '{\"code\":0}\\n'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake curl {}: {}",
            fake_curl.display(),
            error
        )
    });
    make_executable(&fake_curl);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_REFRESH_READINESS", "true")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "credential refresh that leaves DB not-ready must fail closed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("readiness_refresh=expired_risk_context"));
    assert!(stdout.contains("readiness_refresh_lease=ok"));
    assert!(stderr.contains("blocker=readiness_refresh_credential_not_ready"));
    assert!(stderr.contains("last_check_code=okx_preflight_network_error"));
    assert!(
        !stdout.contains("readiness_refresh_credential_check=ok")
            && !stdout.contains("worker=dry_run")
            && !stdout.contains("worker=apply"),
        "script must not claim credential refresh succeeded or proceed to worker modes\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    let preflight_count = fs::read_to_string(&preflight_count_file).unwrap_or_default();
    assert_eq!(
        preflight_count, "1",
        "script should stop after failed refresh validation instead of rerunning preflight"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_propagates_non_refreshable_preflight_failures() {
    let temp_dir = temp_contract_dir("preflight_failure_passthrough");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'blocker=task_risk_context_ttl_too_short detail=1s<999999s' >&2\n\
echo 'preflight=blocked failures=1'\n\
exit 2\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(&fake_podman, "#!/usr/bin/env bash\nprintf '0\\n'\n").unwrap_or_else(|error| {
        panic!(
            "failed to write fake podman {}: {}",
            fake_podman.display(),
            error
        );
    });
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "worker must preserve non-refreshable preflight failure status\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        !stdout.contains("worker=dry_run"),
        "worker reached dry-run after a non-refreshable preflight failure\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        !stdout.contains("readiness_refresh=expired_risk_context"),
        "ttl_too_short must not trigger expired-context readiness refresh\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_collects_post_run_evidence_after_apply() {
    let script = read_worker_script();

    assert!(script.contains("collect_post_run_evidence()"));
    assert!(script.contains("worker_status=0"));
    assert!(script.contains("run_worker_once || worker_status=$?"));
    assert!(script.contains("collect_post_run_evidence"));
    assert!(script.contains("exit \"${worker_status}\""));
    assert!(script.contains("post_run_evidence=web_task_order_result"));
    assert!(script.contains("exchange_order_results"));
    assert!(script.contains("execution_task_attempts"));
    assert!(script.contains("rollback_plan=manual_close_required_if_position_open"));
    assert!(
        !script.contains("exec \"${target_binary}\""),
        "scoped worker script must regain control after live worker exits so it can collect evidence"
    );
}

#[test]
fn market_velocity_okx_scoped_live_worker_runs_post_run_okx_read_only_snapshot_after_apply() {
    let script = read_worker_script();

    assert!(script.contains("MARKET_VELOCITY_LIVE_POST_RUN_RECONCILE"));
    assert!(script.contains("collect_post_run_exchange_readonly_evidence()"));
    assert!(script.contains("post_run_evidence=okx_signed_readonly_reconciliation_snapshot"));
    assert!(script.contains("IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=true"));
    assert!(script
        .contains("RECONCILIATION_SNAPSHOT_CONFIRM=I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION"));
    assert!(script.contains("RECONCILIATION_SNAPSHOT_EXCHANGE=okx"));
    assert!(script.contains("RECONCILIATION_SNAPSHOT_SYMBOL=\"${reconciliation_symbol}\""));
    assert!(script.contains("RECONCILIATION_SNAPSHOT_REPORT=false"));
    assert!(script.contains(
        "RECONCILIATION_SNAPSHOT_INCLUDE_FILLS=\"${MARKET_VELOCITY_LIVE_POST_RUN_INCLUDE_FILLS}\""
    ));
    assert!(script.contains("collect_post_run_exchange_readonly_evidence || post_run_status=$?"));
    assert!(
        !script.contains("echo \"buyer_email="),
        "post-run read-only evidence must not print buyer email"
    );
}

#[test]
fn market_velocity_okx_scoped_live_worker_prints_worker_and_post_run_exit_statuses() {
    let script = read_worker_script();

    assert!(script.contains("worker_exit_status=${worker_status}"));
    assert!(script.contains("post_run_evidence_status=${post_run_status}"));
    assert!(script.contains("final_exit_status=${worker_status} reason=worker_failed"));
    assert!(script.contains("final_exit_status=${post_run_status} reason=post_run_evidence_failed"));
    assert!(script.contains("final_exit_status=0 reason=worker_and_post_run_evidence_ok"));
    assert!(
        script.contains("if [[ \"${worker_status}\" != \"0\" ]]; then")
            && script.contains("exit \"${worker_status}\""),
        "worker failure must keep priority over post-run evidence failure"
    );
}

#[test]
fn market_velocity_okx_scoped_live_worker_prints_pre_apply_readiness_manifest() {
    let temp_dir = temp_contract_dir("pre_apply_readiness_manifest");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:35:17.792702\\tpresent\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!("failed to write fake podman {}: {}", fake_podman.display(), error);
    });
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8001")
        .env("EXECUTION_EVENT_SECRET", "must-not-leak")
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "dry-run manifest rehearsal should not require live apply\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("live_apply_manifest=market_velocity_okx_scoped_worker"));
    assert!(stdout.contains("manifest_target_task_id=85"));
    assert!(stdout.contains("manifest_combo_id=85"));
    assert!(stdout.contains("manifest_exchange=okx"));
    assert!(stdout.contains("manifest_symbol=ASTER-USDT-SWAP"));
    assert!(stdout.contains("manifest_source_signal_type=market_velocity"));
    assert!(stdout.contains("manifest_rank_event_id=1444950"));
    assert!(stdout.contains("manifest_size_usdt=5.0"));
    assert!(stdout.contains("manifest_max_notional_usdt=5"));
    assert!(stdout.contains("manifest_stop_loss=0.605052"));
    assert!(stdout.contains("manifest_protection_required=true"));
    assert!(stdout.contains("manifest_protection_direction=long"));
    assert!(stdout.contains("manifest_protection_entry_price=0.6174"));
    assert!(stdout
        .contains("manifest_protection_stop_loss_source=market_velocity_default_stop_loss_pct"));
    assert!(stdout.contains("manifest_credential_ref=present"));
    assert!(stdout.contains("manifest_web_base_url=http://127.0.0.1:8001"));
    assert!(stdout.contains("manifest_okx_request_expiration_ms=explicit:300000"));
    assert!(stdout.contains("manifest_worker_scope=EXECUTION_WORKER_TARGET_TASK_IDS=85"));
    assert!(stdout.contains("manifest_post_run_evidence=web_task_order_result,execution_task_attempts,okx_signed_readonly_reconciliation_snapshot"));
    assert!(stdout.contains(
        "manifest_live_mutation_intent=okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5"
    ));
    assert!(stdout.contains("manifest_live_mutation_requires=MARKET_VELOCITY_LIVE_WORKER_APPLY=true MARKET_VELOCITY_LIVE_WORKER_CONFIRM=I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER MARKET_VELOCITY_LIVE_WORKER_INTENT=okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5"));
    assert!(stdout.contains("worker=dry_run"));
    assert!(stdout.contains("apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=85 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_LIVE_WORKER_APPLY=true MARKET_VELOCITY_LIVE_WORKER_CONFIRM=I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER MARKET_VELOCITY_LIVE_WORKER_INTENT=okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5"));
    assert!(stdout.contains("apply_rehearsal_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=85 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY=true MARKET_VELOCITY_LIVE_WORKER_CONFIRM=I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER MARKET_VELOCITY_LIVE_WORKER_INTENT=okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5"));
    assert!(
        !stdout.contains("buyer_email") && !stdout.contains("api_credential_id"),
        "manifest must not print buyer email or credential ids\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        !stdout.contains("must-not-leak"),
        "manifest must not print execution event secret\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_shell_escapes_apply_handoff_env_values() {
    let temp_dir = temp_contract_dir("shell_escape_apply_handoff_env_values");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:35:17.792702\\tpresent\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!("failed to write fake podman {}: {}", fake_podman.display(), error);
    });
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env(
            "RUST_QUAN_WEB_BASE_URL",
            "http://127.0.0.1:8001/with space;echo owned",
        )
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "dry-run scoped worker should succeed without live apply\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("manifest_web_base_url=http://127.0.0.1:8001/with space;echo owned"));
    assert!(stdout.contains("apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001/with\\ space\\;echo\\ owned MARKET_VELOCITY_LIVE_COMBO_ID=85"));
    assert!(stdout.contains("apply_rehearsal_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001/with\\ space\\;echo\\ owned MARKET_VELOCITY_LIVE_COMBO_ID=85"));
    assert!(!stdout.contains(
        "apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001/with space;echo owned "
    ));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_blocks_malformed_symbol_manifest() {
    let temp_dir = temp_contract_dir("blocks_malformed_symbol_manifest");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER USDT;rm\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:35:17.792702\\tpresent\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake podman {}: {}",
            fake_podman.display(),
            error
        );
    });
    make_executable(&fake_podman);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8001")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "malformed OKX symbol should not receive worker apply handoff\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("preflight=ok"));
    assert!(stderr.contains("blocker=okx_symbol_invalid detail=ASTER USDT;rm"));
    assert!(!stdout.contains("manifest_live_mutation_intent="));
    assert!(!stdout.contains("worker=dry_run"));
    assert!(!stdout.contains("apply_requirements="));
    assert!(!stdout.contains("apply_rehearsal_requirements="));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_requires_bound_live_intent_before_apply() {
    let temp_dir = temp_contract_dir("apply_bound_live_intent");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });
    fs::create_dir_all(&target_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp target dir {}: {}",
            target_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:40:32.254771\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!("failed to write fake podman {}: {}", fake_podman.display(), error);
    });
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\n\
echo 'unexpected_binary_invocation'\n\
exit 9\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake binary {}: {}",
            fake_binary.display(),
            error
        );
    });
    make_executable(&fake_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_WORKER_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_CONFIRM",
            "I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER",
        )
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "apply must fail closed before signed read-only snapshot when bound intent is missing\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains(
        "manifest_live_mutation_intent=okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5"
    ));
    assert!(stderr.contains("blocker=live_worker_intent_missing"));
    assert!(
        !stdout.contains("\npre_run_evidence=okx_signed_readonly_reconciliation_snapshot")
            && !stdout.contains("worker=apply")
            && !stdout.contains("unexpected_binary_invocation"),
        "apply must stop before pre-run snapshot or worker when intent is missing\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_rehearses_apply_until_pre_run_snapshot_only() {
    let temp_dir = temp_contract_dir("apply_rehearsal_pre_run_only");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });
    fs::create_dir_all(&target_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp target dir {}: {}",
            target_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T18:02:50.319947\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!("failed to write fake podman {}: {}", fake_podman.display(), error);
    });
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\n\
if [[ \"${IS_RUN_RECONCILIATION_SNAPSHOT_CHECK:-}\" == \"true\" ]]; then\n\
  if [[ -z \"${RUST_QUAN_WEB_BASE_URL:-}\" ]]; then\n\
    echo 'snapshot_missing_rust_quan_web_base_url'\n\
    exit 6\n\
  fi\n\
  if [[ -z \"${EXECUTION_EVENT_SECRET:-}\" ]]; then\n\
    echo 'snapshot_missing_execution_event_secret'\n\
    exit 6\n\
  fi\n\
  echo 'snapshot_binary_invoked'\n\
  exit 0\n\
fi\n\
if [[ \"${IS_RUN_EXECUTION_WORKER:-}\" == \"true\" ]]; then\n\
  echo 'worker_binary_invoked'\n\
  exit 9\n\
fi\n\
echo 'unexpected_binary_invocation'\n\
exit 8\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake binary {}: {}",
            fake_binary.display(),
            error
        );
    });
    make_executable(&fake_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_CONFIRM",
            "I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER",
        )
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_INTENT",
            "okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5",
        )
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "apply rehearsal should succeed after pre-run snapshot and before worker\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("mode=apply_rehearsal"));
    assert!(stdout.contains("pre_run_evidence=okx_signed_readonly_reconciliation_snapshot"));
    assert!(stdout.contains("snapshot_binary_invoked"));
    assert!(stdout.contains("pre_run_evidence_status=0"));
    assert!(stdout.contains("worker=rehearsal_stop_before_apply"));
    assert!(stdout.contains("final_exit_status=0 reason=pre_run_evidence_ok_rehearsal_no_worker"));
    assert!(
        !stdout.contains("worker=apply") && !stdout.contains("worker_binary_invoked"),
        "apply rehearsal must not start the live worker\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_blocks_rehearsal_apply_conflict() {
    let temp_dir = temp_contract_dir("apply_rehearsal_apply_conflict");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'preflight_should_not_run'\n\
exit 0\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_WORKER_APPLY", "true")
        .env("MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY", "true")
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "rehearsal and apply must not be enabled together\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stderr.contains("blocker=live_worker_rehearsal_conflicts_with_apply"));
    assert!(
        !stdout.contains("preflight_should_not_run")
            && !stdout.contains("pre_run_evidence=okx_signed_readonly_reconciliation_snapshot")
            && !stdout.contains("worker=apply"),
        "conflict must stop before preflight, pre-run snapshot, or worker\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_blocks_apply_when_pre_run_snapshot_fails() {
    let temp_dir = temp_contract_dir("pre_run_snapshot_blocks_apply");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });
    fs::create_dir_all(&target_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp target dir {}: {}",
            target_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &worker_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy worker script to {}: {}",
            worker_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake preflight script {}: {}",
            preflight_script.display(),
            error
        );
    });
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:40:32.254771\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap_or_else(|error| {
        panic!("failed to write fake podman {}: {}", fake_podman.display(), error);
    });
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\n\
if [[ \"${IS_RUN_RECONCILIATION_SNAPSHOT_CHECK:-}\" == \"true\" ]]; then\n\
  echo 'snapshot_binary_invoked'\n\
  exit 7\n\
fi\n\
if [[ \"${IS_RUN_EXECUTION_WORKER:-}\" == \"true\" ]]; then\n\
  echo 'worker_binary_invoked'\n\
  exit 0\n\
fi\n\
echo 'unexpected_binary_invocation'\n\
exit 9\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write fake binary {}: {}",
            fake_binary.display(),
            error
        );
    });
    make_executable(&fake_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "85")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_WORKER_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_CONFIRM",
            "I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER",
        )
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_INTENT",
            "okx:task=85:symbol=ASTER-USDT-SWAP:max_notional=5",
        )
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(7),
        "pre-run snapshot failure must be surfaced before worker apply\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("pre_run_evidence=okx_signed_readonly_reconciliation_snapshot"));
    assert!(stdout.contains("snapshot_binary_invoked"));
    assert!(stdout.contains("pre_run_evidence_status=7"));
    assert!(stdout.contains("final_exit_status=7 reason=pre_run_evidence_failed"));
    assert!(
        !stdout.contains("worker=apply") && !stdout.contains("worker_binary_invoked"),
        "worker must not start when pre-run signed read-only snapshot fails\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
