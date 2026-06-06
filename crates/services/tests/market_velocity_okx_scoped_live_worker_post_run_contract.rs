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

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn temp_contract_dir(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "market_velocity_worker_post_run_contract_{}_{}",
        std::process::id(),
        test_name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("scripts").join("dev")).unwrap();
    path
}

fn assert_successful_post_run_status_is_accepted(task_status: &str) {
    let test_name = format!("apply_worker_reports_{task_status}");
    let temp_dir = temp_contract_dir(&test_name);
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    fs::copy(script_path(), &worker_script).unwrap();
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap();
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        format!(
            "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${{args}}\" == *\"post_run_target_handled_summary\"* ]]; then\n\
  printf '{}\\t1\\t1\\n'\n\
elif [[ \"${{args}}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${{args}}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${{args}}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${{args}}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-06T05:10:00\\tpresent\\n'\n\
elif [[ \"${{args}}\" == *\"et.buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
elif [[ \"${{args}}\" == *\"post_run_evidence=web_task_order_result\"* ]]; then\n\
  printf '86\\tASTER-USDT-SWAP\\t{}\\t\\t\\t1\\tfilled:buy:okx\\t2026-06-06 05:10:01\\n'\n\
elif [[ \"${{args}}\" == *\"execution_task_attempts\"* ]]; then\n\
  printf '42\\t1\\t{}\\tmarket_velocity_okx_scoped_live_worker\\t\\t2026-06-06 05:10:01\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
            task_status, task_status, task_status
        ),
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\n\
if [[ \"${IS_RUN_RECONCILIATION_SNAPSHOT_CHECK:-}\" == \"true\" ]]; then\n\
  echo 'snapshot_binary_invoked'\n\
  exit 0\n\
fi\n\
if [[ \"${IS_RUN_EXECUTION_WORKER:-}\" == \"true\" ]]; then\n\
  echo 'worker_binary_invoked_and_reported_successful_task'\n\
  exit 0\n\
fi\n\
echo 'unexpected_binary_invocation'\n\
exit 8\n",
    )
    .unwrap();
    make_executable(&fake_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_WORKER_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_CONFIRM",
            "I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER",
        )
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_INTENT",
            "okx:task=86:symbol=ASTER-USDT-SWAP:max_notional=5",
        )
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "apply should accept post-run task status {task_status} as a successful worker report\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("worker_binary_invoked_and_reported_successful_task"));
    assert!(stdout.contains(&format!(
        "post_run_target_summary=status:{task_status} order_results:1 attempts:1"
    )));
    assert!(stdout.contains("post_run_evidence=okx_signed_readonly_reconciliation_snapshot"));
    assert!(stdout.contains("snapshot_binary_invoked"));
    assert!(stdout.contains("post_run_evidence_status=0"));
    assert!(stdout.contains("final_exit_status=0 reason=worker_and_post_run_evidence_ok"));
    assert!(
        !stderr.contains("blocker=post_run_target_task_"),
        "successful post-run status must not emit post-run target blocker\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_accepts_completed_post_run_status() {
    assert_successful_post_run_status_is_accepted("completed");
}

#[test]
fn market_velocity_okx_scoped_live_worker_accepts_pending_protection_sync_post_run_status() {
    assert_successful_post_run_status_is_accepted("pending_protection_sync");
}

#[test]
fn market_velocity_okx_scoped_live_worker_fails_apply_when_worker_exits_without_task_evidence() {
    let temp_dir = temp_contract_dir("apply_worker_exits_without_task_evidence");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    fs::copy(script_path(), &worker_script).unwrap();
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap();
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"post_run_target_handled_summary\"* ]]; then\n\
  printf 'pending\\t0\\t0\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-06T05:10:00\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"et.buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
elif [[ \"${args}\" == *\"post_run_evidence=web_task_order_result\"* ]]; then\n\
  printf '86\\tASTER-USDT-SWAP\\tpending\\t\\t\\t0\\tnone\\t2026-06-06 05:10:01\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\n\
if [[ \"${IS_RUN_RECONCILIATION_SNAPSHOT_CHECK:-}\" == \"true\" ]]; then\n\
  echo 'snapshot_binary_invoked'\n\
  exit 0\n\
fi\n\
if [[ \"${IS_RUN_EXECUTION_WORKER:-}\" == \"true\" ]]; then\n\
  echo 'worker_binary_invoked_without_handling'\n\
  exit 0\n\
fi\n\
echo 'unexpected_binary_invocation'\n\
exit 8\n",
    )
    .unwrap();
    make_executable(&fake_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_WORKER_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_CONFIRM",
            "I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER",
        )
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_INTENT",
            "okx:task=86:symbol=ASTER-USDT-SWAP:max_notional=5",
        )
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "apply must fail when worker exits 0 without task attempts or order results\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("worker_binary_invoked_without_handling"));
    assert!(stdout.contains("post_run_target_summary=status:pending order_results:0 attempts:0"));
    assert!(stderr.contains("blocker=post_run_target_task_unhandled"));
    assert!(stdout.contains("post_run_evidence_status=2"));
    assert!(stdout.contains("final_exit_status=2 reason=post_run_evidence_failed"));
    assert!(
        !stdout.contains("final_exit_status=0 reason=worker_and_post_run_evidence_ok"),
        "a worker 0 exit without target task evidence must not be reported as successful\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_fails_apply_when_post_run_task_status_failed() {
    let temp_dir = temp_contract_dir("apply_worker_reports_failed_task_status");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    fs::copy(script_path(), &worker_script).unwrap();
    fs::write(
        &preflight_script,
        "#!/usr/bin/env bash\necho 'preflight=ok'\n",
    )
    .unwrap();
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"post_run_target_handled_summary\"* ]]; then\n\
  printf 'failed\\t1\\t1\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-06T05:10:00\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"et.buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
elif [[ \"${args}\" == *\"post_run_evidence=web_task_order_result\"* ]]; then\n\
  printf '86\\tASTER-USDT-SWAP\\tfailed\\t\\t\\t1\\tfailed:buy:okx\\t2026-06-06 05:10:01\\n'\n\
elif [[ \"${args}\" == *\"execution_task_attempts\"* ]]; then\n\
  printf '42\\t1\\tfailed\\tmarket_velocity_okx_scoped_live_worker\\tOKX rejected order\\t2026-06-06 05:10:01\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\n\
if [[ \"${IS_RUN_RECONCILIATION_SNAPSHOT_CHECK:-}\" == \"true\" ]]; then\n\
  echo 'snapshot_binary_invoked'\n\
  exit 0\n\
fi\n\
if [[ \"${IS_RUN_EXECUTION_WORKER:-}\" == \"true\" ]]; then\n\
  echo 'worker_binary_invoked_and_reported_failed_task'\n\
  exit 0\n\
fi\n\
echo 'unexpected_binary_invocation'\n\
exit 8\n",
    )
    .unwrap();
    make_executable(&fake_binary);

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new("bash")
        .arg(&worker_script)
        .env("PATH", format!("{}:{}", bin_dir.display(), inherited_path))
        .env("MARKET_VELOCITY_LIVE_COMBO_ID", "85")
        .env("MARKET_VELOCITY_LIVE_TARGET_TASK_ID", "86")
        .env("MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT", "5")
        .env("MARKET_VELOCITY_LIVE_WORKER_APPLY", "true")
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_CONFIRM",
            "I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER",
        )
        .env(
            "MARKET_VELOCITY_LIVE_WORKER_INTENT",
            "okx:task=86:symbol=ASTER-USDT-SWAP:max_notional=5",
        )
        .output()
        .expect("bash should run scoped worker script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "apply must fail when post-run Web evidence says the target task failed even if worker exited 0\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("worker_binary_invoked_and_reported_failed_task"));
    assert!(stdout.contains("post_run_target_summary=status:failed order_results:1 attempts:1"));
    assert!(stderr.contains("blocker=post_run_target_task_failed"));
    assert!(stdout.contains("post_run_evidence_status=2"));
    assert!(stdout.contains("final_exit_status=2 reason=post_run_evidence_failed"));
    assert!(
        !stdout.contains("final_exit_status=0 reason=worker_and_post_run_evidence_ok"),
        "failed target task must not be reported as successful live apply\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
