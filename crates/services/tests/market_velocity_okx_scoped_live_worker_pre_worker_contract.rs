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
        "market_velocity_worker_pre_worker_contract_{}_{}",
        std::process::id(),
        test_name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("scripts").join("dev")).unwrap();
    path
}

#[test]
fn market_velocity_okx_scoped_live_worker_revalidates_preflight_after_pre_run_snapshot() {
    let temp_dir = temp_contract_dir("pre_worker_preflight_revalidation");
    let script_dir = temp_dir.join("scripts").join("dev");
    let worker_script = script_dir.join("run_market_velocity_okx_scoped_live_worker.sh");
    let preflight_script = script_dir.join("run_market_velocity_okx_live_preflight.sh");
    let bin_dir = temp_dir.join("bin");
    let target_dir = temp_dir.join("target").join("debug");
    let preflight_count_file = temp_dir.join("preflight_count");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    fs::copy(script_path(), &worker_script).unwrap();
    fs::write(
        &preflight_script,
        format!(
            "#!/usr/bin/env bash\n\
count_file='{}'\n\
count=$(cat \"${{count_file}}\" 2>/dev/null || printf '0')\n\
count=$((count + 1))\n\
printf '%s' \"${{count}}\" > \"${{count_file}}\"\n\
if [[ \"${{count}}\" == \"1\" ]]; then\n\
  echo 'preflight=ok'\n\
  exit 0\n\
fi\n\
echo 'blocker=task_risk_context_ttl_too_short detail=3s<60s' >&2\n\
echo 'preflight=blocked failures=1'\n\
exit 2\n",
            preflight_count_file.display()
        ),
    )
    .unwrap();
    make_executable(&preflight_script);

    let fake_podman = bin_dir.join("podman");
    fs::write(
        &fake_podman,
        "#!/usr/bin/env bash\n\
args=\"$*\"\n\
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '86\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-06T05:10:00\\tpresent\\n'\n\
elif [[ \"${args}\" == *\"et.buyer_email\"* ]]; then\n\
  printf 'buyer@example.test\\tASTER-USDT-SWAP\\t85\\t123\\n'\n\
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
  echo 'worker_binary_invoked'\n\
  exit 9\n\
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
        "apply must re-run preflight after pre-run snapshot and stop before worker when risk TTL is no longer fresh\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("snapshot_binary_invoked"));
    assert!(stdout.contains("pre_worker_preflight=market_velocity_okx_live_preflight"));
    assert!(stdout.contains("pre_worker_preflight_status=2"));
    assert!(stdout.contains("final_exit_status=2 reason=pre_worker_preflight_failed"));
    assert!(stderr.contains("blocker=task_risk_context_ttl_too_short"));
    assert!(
        !stdout.contains("worker=apply") && !stdout.contains("worker_binary_invoked"),
        "worker must not start after final pre-worker preflight fails\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let preflight_count = fs::read_to_string(&preflight_count_file).unwrap_or_default();
    assert_eq!(
        preflight_count, "2",
        "preflight should run before and after snapshot"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
