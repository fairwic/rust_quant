use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn connectivity_script_exposes_proxy_endpoint_retry_and_tls_diagnostics() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("check_binance_connectivity.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("BINANCE_REST_ENDPOINTS"));
    assert!(script.contains("BINANCE_WS_ENDPOINTS"));
    assert!(script.contains("BINANCE_CONNECTIVITY_RETRIES"));
    assert!(script.contains("BINANCE_CONNECTIVITY_RETRY_DELAY_SECS"));
    assert!(script.contains("proxy_source:"));
    assert!(script.contains("Diagnostic hints"));
    assert!(script.contains("SSL_ERROR_SYSCALL"));
    assert!(script.contains("socks5h://127.0.0.1:7897"));
    assert!(script.contains("Attempt "));
    assert!(script.contains("Connectivity summary"));
}

#[test]
fn natural_probe_script_runs_connectivity_preflight_before_live_probe() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_binance_websocket_natural_probe.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("BINANCE_CONNECTIVITY_PREFLIGHT"));
    assert!(script.contains("BINANCE_CONNECTIVITY_ALLOW_FAILURE"));
    assert!(script.contains("check_binance_connectivity.sh"));
    assert!(script.contains("Skipping Binance connectivity preflight"));
    assert!(script.contains("Binance connectivity preflight failed"));
}
