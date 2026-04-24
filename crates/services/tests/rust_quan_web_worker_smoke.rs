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
    assert!(script.contains("rustup which --toolchain \"${RUSTUP_TOOLCHAIN}\" rustc"));
    assert!(script.contains("export RUSTC"));
    assert!(script.contains("rustup run \"${RUSTUP_TOOLCHAIN}\" cargo run --bin rust_quant"));
    assert!(script.contains("cargo run --bin rust_quant"));
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
