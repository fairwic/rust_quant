use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn candidate_probe_script_reports_quant_core_backed_recommendations_without_force_signal() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("suggest_binance_natural_probe_candidates.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("strategy_configs"));
    assert!(script.contains("pg_tables"));
    assert!(script.contains("recommended_candidates"));
    assert!(script.contains("Suggested natural probe command"));
    assert!(script.contains("sc.version NOT LIKE 'smoke-binance-websocket-natural-%'"));
    assert!(script.contains("SMOKE_SYMBOL="));
    assert!(script.contains("SMOKE_PERIOD="));
    assert!(script.contains("SMOKE_STRATEGY_VERSION="));
    assert!(script.contains("SMOKE_SOURCE_STRATEGY_VERSION="));
    assert!(script.contains("SMOKE_LIVE_TIMEOUT_SECS="));
    assert!(!script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL="));
}
