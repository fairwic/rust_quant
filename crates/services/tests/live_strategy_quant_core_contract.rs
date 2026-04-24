use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn live_strategy_startup_uses_candle_service_for_quant_core_source() {
    let root = repo_root();
    let strategy_data_path = root
        .join("crates")
        .join("services")
        .join("src")
        .join("strategy")
        .join("strategy_data_service.rs");
    let strategy_data = fs::read_to_string(&strategy_data_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", strategy_data_path.display(), error)
    });

    assert!(strategy_data.contains("use crate::market::get_confirmed_candles_for_backtest;"));
    assert!(strategy_data.contains("get_confirmed_candles_for_backtest"));
    assert!(!strategy_data.contains("CandlesModel::new()"));

    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));

    assert!(
        bootstrap.contains("use rust_quant_services::market::get_confirmed_candles_for_backtest;")
    );
    assert!(bootstrap.contains("get_confirmed_candles_for_backtest(&inst_id"));
    assert!(!bootstrap.contains("CandlesModel::new()"));
}

#[test]
fn live_strategy_quant_core_smoke_script_is_safe_and_bounded() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_live_strategy_quant_core_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("CANDLE_SOURCE:=\"quant_core\""));
    assert!(script.contains("STRATEGY_CONFIG_SOURCE:=\"quant_core\""));
    assert!(script.contains("IS_RUN_REAL_STRATEGY=true"));
    assert!(script.contains("IS_OPEN_SOCKET=false"));
    assert!(script.contains("IS_BACK_TEST=false"));
    assert!(script.contains("IS_RUN_SYNC_DATA_JOB=false"));
    assert!(script.contains("STRATEGY_SIGNAL_DISPATCH_MODE:=\"web\""));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=true"));
    assert!(script.contains("EXIT_AFTER_REAL_STRATEGY_ONESHOT=true"));
    assert!(script.contains("RUN_EXECUTION_WORKER_AFTER_STRATEGY:=\"true\""));
    assert!(script.contains("SMOKE_TIMEOUT_SECS"));
    assert!(script.contains("./scripts/dev/ddl_smoke.sh"));
    assert!(script.contains("cargo run --bin rust_quant"));
    assert!(script.contains("./scripts/dev/run_execution_worker_dry_run.sh"));

    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));

    assert!(bootstrap.contains("EXIT_AFTER_REAL_STRATEGY_ONESHOT"));
    assert!(bootstrap.contains("实时策略 one-shot 已完成"));
}

#[test]
fn forced_signal_quant_core_smoke_script_drives_web_execution_loop() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_forced_signal_quant_core_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL:=\"buy\""));
    assert!(script.contains("TRADE_SIGNAL_SMOKE_STRATEGY_SLUG=vegas"));
    assert!(script.contains("EXECUTION_DEMO_STRATEGY_TITLE=\"Vegas Strategy Smoke\""));
    assert!(script.contains("TRADE_SIGNAL_SMOKE_SYMBOL=ETH-USDT-SWAP"));
    assert!(script.contains("WEB_SEED_SCRIPT=\"${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh\""));
    assert!(script.contains("STRATEGY_SIGNAL_DISPATCH_MODE=web"));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=true"));
    assert!(script.contains("RUN_EXECUTION_WORKER_AFTER_STRATEGY=true"));
    assert!(script.contains("RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX"));
    assert!(script.contains("BASE_SIGNAL_ID=\"$(query_web_scalar"));
    assert!(script.contains("id > ${BASE_SIGNAL_ID}"));
    assert!(script.contains("Expected a fresh rust_quant forced strategy signal"));
    assert!(script.contains("./scripts/dev/run_live_strategy_quant_core_smoke.sh"));
}

#[test]
fn binance_websocket_quant_core_smoke_script_prepares_sync_then_live_wait() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_binance_websocket_quant_core_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));

    assert!(script.contains("SYNC_ONLY_PERIODS:=\"1m\""));
    assert!(script.contains("SYNC_LATEST_ONLY=true"));
    assert!(script.contains("IS_RUN_REAL_STRATEGY=true"));
    assert!(script.contains("IS_OPEN_SOCKET=true"));
    assert!(script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL=buy"));
    assert!(script.contains("WEB_SEED_SCRIPT=\"${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh\""));
    assert!(script.contains("seed_execution_demo_combo.sh"));
    assert!(script.contains("strategy_signal_inbox"));
    assert!(script.contains("run_execution_worker_dry_run.sh"));
}

#[test]
fn websocket_runtime_keeps_preheated_configs_for_ws_target_derivation() {
    let root = repo_root();
    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));

    assert!(bootstrap.contains("✅ 策略已预热并进入等待"));
    assert!(bootstrap.contains("started_configs.push(config.clone());\n            continue;"));
}

#[test]
fn live_strategy_runtime_can_be_filtered_for_single_probe_target() {
    let root = repo_root();
    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));

    assert!(bootstrap.contains("LIVE_STRATEGY_ONLY_INST_IDS"));
    assert!(bootstrap.contains("LIVE_STRATEGY_ONLY_PERIODS"));
    assert!(bootstrap.contains("filter_live_strategy_configs"));
    assert!(bootstrap.contains("实时策略过滤后剩余"));
}
