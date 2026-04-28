use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn local_preflight_launcher_prefers_existing_binary_and_surfaces_compile_risks() {
    let script_path = repo_root()
        .join("scripts")
        .join("dev")
        .join("run_execution_worker_local_preflight.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("target/debug/rust_quant"));
    assert!(script.contains("EXECUTION_WORKER_USE_EXISTING_BINARY=true"));
    assert!(script.contains("QUANT_CORE_DATABASE_URL"));
    assert!(script.contains("QUANT_DATABASE_URL"));
    assert!(script.contains("rustup which --toolchain \"${RUSTUP_TOOLCHAIN}\" rustc"));
    assert!(script.contains("export RUSTC"));
    assert!(script.contains(".sqlx"));
    assert!(script.contains("SQLX_OFFLINE=true"));
    assert!(script.contains("Homebrew cargo/rustc"));
    assert!(script.contains("./scripts/dev/run_execution_worker_dry_run.sh"));
}

#[test]
fn local_worker_docs_explain_live_order_confirmation_guard() {
    let docs_path = repo_root()
        .join("docs")
        .join("EXECUTION_WORKER_LOCAL_SMOKE.md");
    let docs = fs::read_to_string(&docs_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", docs_path.display(), error));

    assert!(docs.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
    assert!(docs.contains("I_UNDERSTAND_LIVE_ORDERS"));
    assert!(docs.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(docs.contains("reduce-only"));
}
