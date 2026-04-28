use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn rust_quan_web_worker_smoke_script_sets_safe_local_defaults() {
    let script_path = repo_root()
        .join("scripts")
        .join("dev")
        .join("run_execution_worker_dry_run.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("RUST_QUAN_WEB_BASE_URL:=\"http://127.0.0.1:8000\""));
    assert!(script.contains("EXECUTION_EVENT_SECRET:=\"local-dev-secret\""));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN:=\"true\""));
    assert!(script.contains("EXECUTION_WORKER_RUN_ONCE:=\"true\""));
    assert!(script.contains("EXECUTION_WORKER_ONLY:=\"true\""));
    assert!(script.contains("EXECUTION_WORKER_DEFAULT_EXCHANGE:=\"binance\""));
    assert!(script.contains(
        "QUANT_CORE_DATABASE_URL:=\"postgres://postgres:postgres123@localhost:5432/quant_core\""
    ));
    assert!(script.contains("QUANT_DATABASE_URL:=\"${QUANT_CORE_DATABASE_URL}\""));
    assert!(script.contains("SQLX_OFFLINE:=\"true\""));
    assert!(script.contains("EXECUTION_WORKER_USE_EXISTING_BINARY:=\"auto\""));
    assert!(script.contains("target/debug/rust_quant"));
    assert!(script.contains("Using existing rust_quant binary"));
    assert!(script.contains("rustup which --toolchain \"${RUSTUP_TOOLCHAIN}\" rustc"));
    assert!(script.contains("export RUSTC"));
    assert!(script.contains("export QUANT_DATABASE_URL"));
    assert!(script.contains("export SQLX_OFFLINE"));
    assert!(script.contains("rustup run \"${RUSTUP_TOOLCHAIN}\" cargo run --bin rust_quant"));
    assert!(script.contains("cargo run --bin rust_quant"));
}

#[test]
fn pending_close_worker_e2e_smoke_hands_off_from_web_review_to_worker() {
    let script_path = repo_root()
        .join("scripts")
        .join("dev")
        .join("run_pending_close_worker_e2e_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("smoke_risk_close_review_loop.sh"));
    assert!(script.contains("RISK_CLOSE_SMOKE_STOP_AFTER_REVIEW=1"));
    assert!(script.contains("run_execution_worker_dry_run.sh"));
    assert!(script.contains("EXECUTION_WORKER_TASK_TYPES=risk_control_close_candidate"));
    assert!(script.contains("EXECUTION_WORKER_TASK_STATUSES=pending_close"));
    assert!(script.contains("pending_close_count"));
    assert!(script.contains("effective_lease_limit"));
    assert!(script.contains("order_side = 'sell'"));
    assert!(script.contains("task_status = 'completed'"));
    assert!(script.contains("pending close worker e2e smoke completed"));
}

#[test]
fn quant_core_audit_smoke_script_runs_ddl_and_real_postgres_test() {
    let script_path = repo_root()
        .join("scripts")
        .join("dev")
        .join("quant_core_audit_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("./scripts/dev/ddl_smoke.sh"));
    assert!(script.contains("QUANT_CORE_AUDIT_SMOKE=1"));
    assert!(script.contains("QUANT_CORE_DATABASE_URL"));
    assert!(
        script.contains("cargo test -p rust-quant-services --test quant_core_audit_postgres_smoke")
    );
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=true"));
}
