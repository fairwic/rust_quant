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
        "market_velocity_worker_history_contract_{}_{}",
        std::process::id(),
        test_name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("scripts").join("dev")).unwrap();
    path
}

#[test]
fn market_velocity_okx_scoped_live_worker_blocks_apply_when_task_has_execution_history() {
    let temp_dir = temp_contract_dir("apply_task_history_blocks_repeat");
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
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tpending\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '1\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:40:32.254771\\tpresent\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\necho 'unexpected_binary_invocation'\nexit 9\n",
    )
    .unwrap();
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
        Some(2),
        "apply must fail closed when target task has execution history\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("pre_apply_task_history=order_results:1 attempts:0"));
    assert!(stderr.contains("blocker=target_task_execution_history_present"));
    assert!(stderr.contains("order_results=1,attempts=0"));
    assert!(
        !stdout.contains("\npre_run_evidence=okx_signed_readonly_reconciliation_snapshot")
            && !stdout.contains("worker=apply")
            && !stdout.contains("unexpected_binary_invocation"),
        "task history blocker must stop before pre-run snapshot or worker\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn market_velocity_okx_scoped_live_worker_blocks_apply_when_task_state_changed_after_manifest() {
    let temp_dir = temp_contract_dir("apply_task_state_changed");
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
if [[ \"${args}\" == *\"pre_apply_task_state\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\texecute_signal\\tcompleted\\n'\n\
elif [[ \"${args}\" == *\"pre_apply_task_execution_history\"* ]]; then\n\
  printf '0\\t0\\n'\n\
elif [[ \"${args}\" == *\"COUNT(*)\"* ]]; then\n\
  printf '0\\n'\n\
elif [[ \"${args}\" == *\"live_apply_manifest_source\"* ]]; then\n\
  printf '85\\t85\\tASTER-USDT-SWAP\\tpending\\tokx\\tmarket_velocity\\t1444950\\t5.0\\t0.605052\\ttrue\\tlong\\t0.6174\\tmarket_velocity_default_stop_loss_pct\\t2026-06-05T17:40:32.254771\\tpresent\\n'\n\
else\n\
  printf '0\\n'\n\
fi\n",
    )
    .unwrap();
    make_executable(&fake_podman);

    let fake_binary = target_dir.join("rust_quant");
    fs::write(
        &fake_binary,
        "#!/usr/bin/env bash\necho 'unexpected_binary_invocation'\nexit 9\n",
    )
    .unwrap();
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
    assert_eq!(
        output.status.code(),
        Some(2),
        "apply rehearsal must fail closed when task state changes after manifest\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("pre_apply_task_state=task_id:85 status:completed"));
    assert!(stderr.contains("blocker=target_task_state_changed"));
    assert!(stderr.contains("task_status=completed"));
    assert!(
        !stdout.contains("\npre_run_evidence=okx_signed_readonly_reconciliation_snapshot")
            && !stdout.contains("worker=apply")
            && !stdout.contains("unexpected_binary_invocation"),
        "task state blocker must stop before pre-run snapshot or worker\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
