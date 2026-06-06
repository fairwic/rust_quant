use std::{fs, path::PathBuf};

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
        .join("select_market_velocity_signal_candidate.sh")
}

fn read_candidate_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

#[test]
fn market_velocity_signal_candidate_script_passes_bash_syntax_check() {
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
fn market_velocity_signal_candidate_script_is_read_only() {
    let script = read_candidate_script();

    assert!(script.contains("SELECT"));
    assert!(!script.contains("UPDATE "));
    assert!(!script.contains("INSERT "));
    assert!(!script.contains("DELETE "));
    assert!(!script.contains("curl -X POST"));
    assert!(!script.contains("submit_strategy_signal"));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
}

#[test]
fn market_velocity_signal_candidate_script_matches_core_signal_gate() {
    let script = read_candidate_script();

    assert!(script.contains("market_rank_events"));
    assert!(script.contains("event_type IN ('rank_velocity', 'top_entry')"));
    assert!(script.contains("delta_rank >= 3"));
    assert!(script.contains("new_rank > 0"));
    assert!(script.contains("new_rank <= 50"));
    assert!(script.contains("lower(price_direction) = 'up'"));
    assert!(script.contains("current_price IS NOT NULL"));
    assert!(script.contains("UPPER(REPLACE(symbol, '-', '')) NOT LIKE 'LINKUSDT%'"));
}

#[test]
fn market_velocity_signal_candidate_script_checks_web_dispatch_state() {
    let script = read_candidate_script();

    assert!(script.contains("strategy_signal_inbox"));
    assert!(script.contains("rust_quant:market_velocity:"));
    assert!(script.contains("MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS"));
    assert!(script.contains("MARKET_VELOCITY_SIGNAL_LIMIT"));
    assert!(script.contains("quant_core"));
    assert!(script.contains("quant_web"));
}
