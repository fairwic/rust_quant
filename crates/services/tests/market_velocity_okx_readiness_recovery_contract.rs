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
        .join("recover_market_velocity_okx_live_readiness.sh")
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
        "market_velocity_readiness_recovery_contract_{}_{}",
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
fn market_velocity_okx_readiness_recovery_requires_explicit_confirmations_without_worker_apply() {
    let script = fs::read_to_string(script_path()).expect("read readiness recovery script");

    assert!(script.contains("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM"));
    assert!(script.contains("I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS"));
    assert!(script.contains("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT"));
    assert!(script.contains("/api/commerce/internal/execution-tasks/lease?limit=1"));
    assert!(script.contains("/api/commerce/internal/api-credentials/${credential_id}/check"));
    assert!(script.contains("run_market_velocity_okx_live_preflight.sh"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("worker=apply"));
}

#[test]
fn market_velocity_okx_readiness_recovery_rejects_invalid_request_expiration_handoff() {
    let temp_dir = temp_contract_dir("invalid_request_expiration_handoff");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118'\n\
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
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\t\\t2026-06-06T04:32:20.173118\\n'\n\
else\n\
  printf '123\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("OKX_REQUEST_EXPIRATION_MS", "300000ms")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid explicit request expiration should fail before recovery handoff\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stderr.contains("blocker=okx_request_expiration_ms_invalid detail=300000ms"));
    assert!(!stdout.contains("recovery_apply_requirements="));
    assert!(!stdout.contains("OKX_REQUEST_EXPIRATION_MS=300000ms"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_not_needed_prints_worker_handoff_without_refresh() {
    let temp_dir = temp_contract_dir("not_needed_prints_worker_handoff");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
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

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\t\\t2026-06-06T04:32:20.173118\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8001")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "not-needed readiness recovery should hand off to worker without Web mutation\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("mode=dry_run"));
    assert!(stdout.contains("preflight=ok"));
    assert!(stdout.contains("recovery=not_needed"));
    assert!(stdout.contains("next_worker_dry_run=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 scripts/dev/run_market_velocity_okx_scoped_live_worker.sh"));
    assert!(stdout.contains("next_worker_live_apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_LIVE_WORKER_APPLY=true MARKET_VELOCITY_LIVE_WORKER_CONFIRM=I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER MARKET_VELOCITY_LIVE_WORKER_INTENT=okx:task=86:symbol=ASTER-USDT-SWAP:max_notional=5"));
    assert!(!stdout.contains("readiness_refresh=expired_risk_context"));
    assert!(!stdout.contains("recovery=applied"));
    assert!(
        !curl_marker.exists(),
        "not-needed readiness recovery must not call Web lease or credential check"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_not_needed_blocks_leased_task_handoff() {
    let temp_dir = temp_contract_dir("not_needed_blocks_leased_task_handoff");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
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

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tleased\\tASTER-USDT-SWAP\\tworker-1\\t2026-06-06T04:32:20.173118\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8001")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "leased task should not receive worker handoff even when preflight is ok\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("preflight=ok"));
    assert!(stdout.contains("readiness_recovery_target=task_id:86 combo_id:85 status:leased type:execute_signal symbol:ASTER-USDT-SWAP lease_owner:worker-1"));
    assert!(stderr.contains("blocker=readiness_recovery_target_not_pending_for_worker_handoff detail=task_status=leased,lease_owner=worker-1"));
    assert!(!stdout.contains("recovery=not_needed"));
    assert!(!stdout.contains("next_worker_dry_run="));
    assert!(!stdout.contains("next_worker_live_apply_requirements="));
    assert!(
        !curl_marker.exists(),
        "not-needed leased-task handoff must not call Web lease or credential check"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_blocks_malformed_symbol_handoff() {
    let temp_dir = temp_contract_dir("blocks_malformed_symbol_handoff");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
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

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER USDT;rm\\t\\t2026-06-06T04:32:20.173118\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8001")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "malformed OKX symbol should not receive recovery-to-worker handoff\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("preflight=ok"));
    assert!(stderr.contains("blocker=okx_symbol_invalid detail=ASTER USDT;rm"));
    assert!(!stdout.contains("recovery=not_needed"));
    assert!(!stdout.contains("next_worker_dry_run="));
    assert!(!stdout.contains("next_worker_live_apply_requirements="));
    assert!(
        !curl_marker.exists(),
        "malformed-symbol handoff must not call Web lease or credential check"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_dry_run_does_not_call_web_refresh() {
    let temp_dir = temp_contract_dir("dry_run_no_web_refresh");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
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

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118'\n\
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
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\t\\t2026-06-06T04:32:20.173118\\n'\n\
else\n\
  printf '123\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "dry-run readiness recovery should exit cleanly without Web mutation\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("mode=dry_run"));
    assert!(stdout.contains("recovery=dry_run"));
    assert!(stdout.contains("readiness_recovery_target=task_id:86"));
    assert!(stdout.contains("lease_owner:none"));
    assert!(stdout.contains("risk_context_expires_at:2026-06-06T04:32:20.173118"));
    assert!(stdout.contains("recovery_apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY=true MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM=I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT=okx-readiness:task=86:max_notional=5"));
    assert!(!stdout.contains("readiness_refresh=expired_risk_context"));
    assert!(
        !curl_marker.exists(),
        "dry-run readiness recovery must not call Web lease or credential check"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_shell_escapes_handoff_env_values() {
    let temp_dir = temp_contract_dir("shell_escape_handoff_env_values");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118'\n\
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
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\t\\t2026-06-06T04:32:20.173118\\n'\n\
else\n\
  printf '123\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env(
            "RUST_QUAN_WEB_BASE_URL",
            "http://127.0.0.1:8001/with space;echo owned",
        )
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "dry-run readiness recovery should succeed without Web mutation\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("recovery=dry_run"));
    assert!(stdout.contains("recovery_apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001/with\\ space\\;echo\\ owned MARKET_VELOCITY_LIVE_COMBO_ID=85"));
    assert!(!stdout.contains(
        "recovery_apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001/with space;echo owned "
    ));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_dry_run_accepts_blocked_stale_task() {
    let temp_dir = temp_contract_dir("dry_run_blocked_stale_task");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
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

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
            error
        );
    });
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\n\
echo 'blocker=target_task_not_okx_market_velocity_pending detail=task_id=86'\n\
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
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tblocked\\tASTER-USDT-SWAP\\tlease_time_risk_snapshot_stale\\t2026-06-06T04:32:20.173118\\n'\n\
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "blocked stale task dry-run should still print recovery requirements\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("readiness_recovery_target=task_id:86"));
    assert!(stdout.contains("status:blocked"));
    assert!(stdout.contains("lease_owner:lease_time_risk_snapshot_stale"));
    assert!(stdout.contains("recovery_reason=blocked_stale_risk_context"));
    assert!(stdout.contains("recovery=dry_run"));
    assert!(stdout.contains("okx-readiness:task=86:max_notional=5"));
    assert!(!curl_marker.exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_apply_blocks_leased_task_before_refresh() {
    let temp_dir = temp_contract_dir("apply_blocks_leased_task");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let preflight_count_file = temp_dir.join("preflight_count");
    let curl_log = temp_dir.join("curl_log");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${{count}}\" == \"1\" ]]; then\n\
  echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118'\n\
  echo 'preflight=blocked failures=1'\n\
  exit 2\n\
fi\n\
echo 'preflight=ok'\n\
exit 0\n",
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
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tleased\\tASTER-USDT-SWAP\\tlive-worker-1\\t2026-06-06T04:32:20.173118\\n'\n\
elif [[ \"${args}\" == *\"FROM user_api_credentials u\"* ]]; then\n\
  printf '123\\n'\n\
elif [[ \"${args}\" == *\"readiness_recovery_credential_status\"* ]]; then\n\
  printf 'active\\tsigned_exchange_preflight_passed\\n'\n\
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
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '{{\"code\":0}}\\n'\n",
            curl_log.display()
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM",
            "I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS",
        )
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT",
            "okx-readiness:task=86:max_notional=5",
        )
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "leased task recovery apply must stop before any Web refresh\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("readiness_recovery_target=task_id:86"));
    assert!(stdout.contains("status:leased"));
    assert!(stderr.contains("blocker=readiness_recovery_leased_task_not_safe_for_apply"));
    assert!(
        !curl_log.exists(),
        "leased task recovery apply must not call Web lease or credential check"
    );

    let preflight_count = fs::read_to_string(&preflight_count_file).unwrap_or_default();
    assert_eq!(
        preflight_count, "1",
        "leased task apply should stop before post-recovery preflight"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_apply_refreshes_and_rechecks_without_worker() {
    let temp_dir = temp_contract_dir("apply_refresh_rechecks_without_worker");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let preflight_count_file = temp_dir.join("preflight_count");
    let curl_log = temp_dir.join("curl_log");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${{count}}\" == \"1\" ]]; then\n\
  echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118'\n\
  echo 'preflight=blocked failures=1'\n\
  exit 2\n\
fi\n\
echo 'preflight=ok'\n\
exit 0\n",
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
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\n'\n\
elif [[ \"${args}\" == *\"FROM user_api_credentials u\"* ]]; then\n\
  printf '123\\n'\n\
elif [[ \"${args}\" == *\"readiness_recovery_credential_status\"* ]]; then\n\
  printf 'active\\tsigned_exchange_preflight_passed\\n'\n\
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
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '{{\"code\":0}}\\n'\n",
            curl_log.display()
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("OKX_REQUEST_EXPIRATION_MS", "300000")
        .env("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:8001")
        .env("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM",
            "I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS",
        )
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT",
            "okx-readiness:task=86:max_notional=5",
        )
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "readiness recovery should refresh and re-run preflight\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("mode=apply"));
    assert!(stdout.contains("readiness_refresh=expired_risk_context"));
    assert!(stdout.contains("readiness_recovery_lease=ok"));
    assert!(stdout.contains("readiness_recovery_credential_check=ready"));
    assert!(stdout.contains("post_recovery_preflight=market_velocity_okx_live_preflight"));
    assert!(stdout.contains("post_recovery_preflight_status=0"));
    assert!(stdout.contains("recovery=applied"));
    assert!(stdout.contains("next_worker_dry_run=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 scripts/dev/run_market_velocity_okx_scoped_live_worker.sh"));
    assert!(stdout.contains("next_worker_live_apply_requirements=OKX_REQUEST_EXPIRATION_MS=300000 RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8001 MARKET_VELOCITY_LIVE_COMBO_ID=85 MARKET_VELOCITY_LIVE_TARGET_TASK_ID=86 MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT=5 MARKET_VELOCITY_LIVE_WORKER_APPLY=true MARKET_VELOCITY_LIVE_WORKER_CONFIRM=I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER MARKET_VELOCITY_LIVE_WORKER_INTENT=okx:task=86:symbol=ASTER-USDT-SWAP:max_notional=5"));
    assert!(!stdout.contains("worker=apply"));

    let preflight_count = fs::read_to_string(&preflight_count_file).unwrap_or_default();
    assert_eq!(
        preflight_count, "2",
        "preflight should run before and after refresh"
    );

    let curl_calls = fs::read_to_string(&curl_log).unwrap_or_default();
    assert!(curl_calls.contains("/api/commerce/internal/execution-tasks/lease?limit=1"));
    assert!(curl_calls.contains("/api/commerce/internal/api-credentials/123/check"));
    assert!(!curl_calls.contains("/api/v5/trade/order"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_apply_rechecks_network_error_credential() {
    let temp_dir = temp_contract_dir("apply_rechecks_network_error_credential");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let preflight_count_file = temp_dir.join("preflight_count");
    let curl_log = temp_dir.join("curl_log");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${{count}}\" == \"1\" ]]; then\n\
  echo 'blocker=task_risk_context_expired detail=2026-06-06T04:32:20.173118'\n\
  echo 'preflight=blocked failures=1'\n\
  exit 2\n\
fi\n\
echo 'preflight=ok'\n\
exit 0\n",
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
if [[ \"${args}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\t\\t2026-06-06T04:32:20.173118\\n'\n\
elif [[ \"${args}\" == *\"FROM user_api_credentials u\"* ]]; then\n\
  if [[ \"${args}\" == *\"okx_preflight_network_error\"* ]]; then\n\
    printf '123\\n'\n\
  fi\n\
elif [[ \"${args}\" == *\"readiness_recovery_credential_status\"* ]]; then\n\
  printf 'active\\tsigned_exchange_preflight_passed\\n'\n\
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
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '{{\"code\":0}}\\n'\n",
            curl_log.display()
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM",
            "I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS",
        )
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT",
            "okx-readiness:task=86:max_notional=5",
        )
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "network-error credential should be rechecked during authorized readiness recovery\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("readiness_recovery_credential_check=ready"));
    assert!(stdout.contains("post_recovery_preflight_status=0"));
    assert!(stdout.contains("recovery=applied"));
    assert!(!stdout.contains("worker=apply"));

    let curl_calls = fs::read_to_string(&curl_log).unwrap_or_default();
    assert!(curl_calls.contains("/api/commerce/internal/execution-tasks/lease?limit=1"));
    assert!(curl_calls.contains("/api/commerce/internal/api-credentials/123/check"));
    assert!(!curl_calls.contains("/api/v5/trade/order"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_readiness_recovery_apply_skips_lease_for_already_blocked_stale_task() {
    let temp_dir = temp_contract_dir("apply_blocked_stale_skips_lease");
    let script_dir = temp_dir.join("scripts").join("dev");
    let recovery_script = script_dir.join("recover_market_velocity_okx_live_readiness.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let preflight_count_file = temp_dir.join("preflight_count");
    let curl_log = temp_dir.join("curl_log");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create temp bin dir {}: {}",
            bin_dir.display(),
            error
        );
    });

    fs::copy(script_path(), &recovery_script).unwrap_or_else(|error| {
        panic!(
            "failed to copy readiness recovery script to {}: {}",
            recovery_script.display(),
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
if [[ \"${{count}}\" == \"1\" ]]; then\n\
  echo 'blocker=target_task_not_okx_market_velocity_pending detail=task_id=86'\n\
  echo 'preflight=blocked failures=1'\n\
  exit 2\n\
fi\n\
echo 'preflight=ok'\n\
exit 0\n",
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
        format!(
            "#!/usr/bin/env bash\n\
args=\"$*\"\n\
count_file='{}'\n\
preflight_count=$(cat \"${{count_file}}\" 2>/dev/null || printf '0')\n\
if [[ \"${{args}}\" == *\"readiness_recovery_target_task\"* ]]; then\n\
  if [[ \"${{preflight_count}}\" -ge 2 ]]; then\n\
    printf '86\\t85\\texecute_signal\\tpending\\tASTER-USDT-SWAP\\t\\t2026-06-06T04:57:20.173118\\n'\n\
  else\n\
    printf '86\\t85\\texecute_signal\\tblocked\\tASTER-USDT-SWAP\\tlease_time_risk_snapshot_stale\\t2026-06-06T04:32:20.173118\\n'\n\
  fi\n\
elif [[ \"${{args}}\" == *\"FROM user_api_credentials u\"* ]]; then\n\
  printf '123\\n'\n\
elif [[ \"${{args}}\" == *\"readiness_recovery_credential_status\"* ]]; then\n\
  printf 'active\\tsigned_exchange_preflight_passed\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
            preflight_count_file.display()
        ),
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
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '{{\"code\":0}}\\n'\n",
            curl_log.display()
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
        .arg(&recovery_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM",
            "I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS",
        )
        .env(
            "MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT",
            "okx-readiness:task=86:max_notional=5",
        )
        .output()
        .expect("bash should run readiness recovery script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "blocked stale task recovery should refresh credential and re-run preflight\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("recovery_reason=blocked_stale_risk_context"));
    assert!(stdout.contains("readiness_recovery_lease=skipped_already_blocked_stale_task"));
    assert!(stdout.contains("readiness_recovery_credential_check=ready"));
    assert!(stdout.contains("post_recovery_preflight_status=0"));
    assert!(stdout.contains("recovery=applied"));
    assert!(!stdout.contains("worker=apply"));

    let curl_calls = fs::read_to_string(&curl_log).unwrap_or_default();
    assert!(!curl_calls.contains("/api/commerce/internal/execution-tasks/lease?limit=1"));
    assert!(curl_calls.contains("/api/commerce/internal/api-credentials/123/check"));
    assert!(!curl_calls.contains("/api/v5/trade/order"));

    let _ = fs::remove_dir_all(&temp_dir);
}
