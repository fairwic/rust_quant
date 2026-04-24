use serde_json::json;

use rust_quant_cli::app::internal_server::{backtest_config_from_body, handle_backtest_run_body};

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
