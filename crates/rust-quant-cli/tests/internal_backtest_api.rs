use serde_json::json;
use std::sync::Mutex;

use rust_quant_cli::app::internal_server::{
    backtest_config_from_body, handle_backtest_run_body, handle_exchange_symbol_sync_body,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn dry_run_backtest_request_returns_admin_adapter_fields() {
    let body = json!({
        "strategyKey": "vegas",
        "symbol": "ETH-USDT-SWAP",
        "timeframe": "4H",
        "configOverrides": {"kline_nums": 3600},
        "dryRun": true
    })
    .to_string();

    let response = handle_backtest_run_body(body.as_bytes()).await;

    assert_eq!(response.status_code, 200);
    assert_eq!(response.body["status"], "dry_run");
    assert_eq!(response.body["strategyKey"], "vegas");
    assert_eq!(response.body["symbol"], "ETH-USDT-SWAP");
    assert_eq!(response.body["timeframe"], "4H");
    assert_eq!(response.body["dryRun"], true);
    assert!(response
        .body
        .get("runId")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.starts_with("rq-backtest-")));
}

#[tokio::test]
async fn backtest_request_rejects_missing_required_fields() {
    let body = json!({
        "strategyKey": "vegas",
        "symbol": "",
        "timeframe": "4H",
        "dryRun": true
    })
    .to_string();

    let response = handle_backtest_run_body(body.as_bytes()).await;

    assert_eq!(response.status_code, 400);
    assert_eq!(response.body["error"], "symbol is required");
}

#[test]
fn backtest_config_overrides_bound_manual_vegas_runs() {
    let body = json!({
        "strategyKey": "vegas",
        "symbol": "ETH-USDT-SWAP",
        "timeframe": "4H",
        "configOverrides": {
            "kline_nums": 360,
            "maxConcurrent": 2
        },
        "dryRun": false
    })
    .to_string();

    let config = backtest_config_from_body(body.as_bytes()).expect("config");

    assert_eq!(config.candle_limit, 360);
    assert_eq!(config.max_concurrent, 2);
    assert!(config.enable_specified_test_vegas);
    assert!(!config.enable_random_test_vegas);
    assert!(!config.enable_specified_test_nwe);
    assert!(!config.enable_random_test_nwe);
}

#[test]
fn backtest_config_accepts_admin_config_alias() {
    let body = json!({
        "strategyKey": "vegas",
        "symbol": "ETH-USDT-SWAP",
        "timeframe": "4H",
        "config": {
            "klineNums": 720,
            "maxConcurrent": 3
        },
        "dryRun": false
    })
    .to_string();

    let config = backtest_config_from_body(body.as_bytes()).expect("config");

    assert_eq!(config.candle_limit, 720);
    assert_eq!(config.max_concurrent, 3);
    assert!(config.enable_specified_test_vegas);
}

#[test]
fn backtest_config_carries_strategy_config_id_for_exact_row_runs() {
    let body = json!({
        "strategyConfigId": "6f9619ff-8b86-d011-b42d-00cf4fc964ff",
        "strategyKey": "vegas",
        "symbol": "ETH-USDT-SWAP",
        "timeframe": "4H",
        "config": {
            "klineNums": 720
        },
        "dryRun": false
    })
    .to_string();

    let config = backtest_config_from_body(body.as_bytes()).expect("config");

    assert_eq!(
        config.strategy_config_id.as_deref(),
        Some("6f9619ff-8b86-d011-b42d-00cf4fc964ff")
    );
}

#[tokio::test]
async fn exchange_symbol_sync_request_rejects_invalid_json_before_running_job() {
    let response = handle_exchange_symbol_sync_body(b"{not-json").await;

    assert_eq!(response.status_code, 400);
    assert!(response.body["error"]
        .as_str()
        .expect("error")
        .contains("invalid json body"));
}

#[tokio::test]
async fn non_dry_run_backtest_requires_quant_core_database_url() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_quant_core = std::env::var("QUANT_CORE_DATABASE_URL").ok();
    let original_source = std::env::var("STRATEGY_CONFIG_SOURCE").ok();
    std::env::remove_var("QUANT_CORE_DATABASE_URL");
    std::env::remove_var("STRATEGY_CONFIG_SOURCE");

    let body = json!({
        "strategyKey": "vegas",
        "symbol": "ETH-USDT-SWAP",
        "timeframe": "4H",
        "dryRun": false
    })
    .to_string();

    let response = handle_backtest_run_body(body.as_bytes()).await;

    assert_eq!(response.status_code, 400);
    assert_eq!(
        response.body["error"],
        "QUANT_CORE_DATABASE_URL is required for non-dry-run backtests"
    );

    restore_env("QUANT_CORE_DATABASE_URL", original_quant_core.as_deref());
    restore_env("STRATEGY_CONFIG_SOURCE", original_source.as_deref());
}

#[tokio::test]
async fn non_dry_run_backtest_rejects_unknown_strategy_config_source() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_quant_core = std::env::var("QUANT_CORE_DATABASE_URL").ok();
    let original_source = std::env::var("STRATEGY_CONFIG_SOURCE").ok();
    std::env::set_var("QUANT_CORE_DATABASE_URL", "postgres://quant-core");
    std::env::set_var("STRATEGY_CONFIG_SOURCE", "legacy_engine");

    let body = json!({
        "strategyKey": "vegas",
        "symbol": "ETH-USDT-SWAP",
        "timeframe": "4H",
        "dryRun": false
    })
    .to_string();

    let response = handle_backtest_run_body(body.as_bytes()).await;

    assert_eq!(response.status_code, 400);
    assert_eq!(
        response.body["error"],
        "STRATEGY_CONFIG_SOURCE=legacy_engine is not supported for non-dry-run backtests"
    );

    restore_env("QUANT_CORE_DATABASE_URL", original_quant_core.as_deref());
    restore_env("STRATEGY_CONFIG_SOURCE", original_source.as_deref());
}

fn restore_env(key: &str, value: Option<&str>) {
    if let Some(value) = value {
        std::env::set_var(key, value);
    } else {
        std::env::remove_var(key);
    }
}
