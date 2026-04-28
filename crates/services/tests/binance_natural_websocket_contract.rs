use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn natural_binance_websocket_probe_script_stays_dry_run_and_does_not_force_signal() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_binance_websocket_natural_probe.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("IS_RUN_REAL_STRATEGY=true"));
    assert!(script.contains("IS_OPEN_SOCKET=true"));
    assert!(script.contains("STRATEGY_SIGNAL_DISPATCH_MODE=web"));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=true"));
    assert!(script.contains("smoke-binance-websocket-natural-eth-1m"));
    assert!(script.contains("SMOKE_SOURCE_STRATEGY_VERSION"));
    assert!(script.contains("LIVE_STRATEGY_ONLY_INST_IDS"));
    assert!(script.contains("LIVE_STRATEGY_ONLY_PERIODS"));
    assert!(script.contains("derive_runtime_strategy_version"));
    assert!(script.contains("normalize_table_suffix"));
    assert!(script.contains("_candles_$(normalize_table_suffix \"${SMOKE_PERIOD}\")"));
    assert!(script.contains("Natural websocket probe summary"));
    assert!(script.contains("Binance public websocket启动成功"));
    assert!(script.contains("K线确认触发策略检查"));
    assert!(script.contains("已提交策略信号到 rust_quan_web"));
    assert!(script.contains("CREATED_TEMP_STRATEGY_CONFIG"));
    assert!(script.contains("Using existing runtime strategy config"));
    assert!(script.contains("Preparing temporary runtime strategy config"));
    assert!(script.contains("version NOT LIKE 'smoke-binance-websocket-natural-%'"));
    assert!(!script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL="));
}
