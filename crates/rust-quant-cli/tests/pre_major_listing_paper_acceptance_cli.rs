use serde_json::json;
use std::fs;
use std::process::Command;

#[test]
fn cli_outputs_paper_ready_report_without_enabling_live_trading() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_paper_acceptance_{}.json",
        std::process::id()
    ));
    let input = json!({
        "criteria": {
            "min_trade_samples": 1,
            "min_win_rate_pct": 1.0,
            "require_positive_total_net_return": true
        },
        "samples": [{
            "announcement_id": "binance-test-listing",
            "input": {
                "announcement_exchange": "binance",
                "base_asset": "TEST",
                "quote_asset": "USDT",
                "detection_latency_secs": 20,
                "pre_announcement_return_15m_pct": 5.0,
                "btc_5m_return_pct": -0.1,
                "eth_5m_return_pct": 0.2,
                "opening_upper_wick_rejection": false,
                "candidates": [{
                    "exchange": "bitget",
                    "symbol": "TEST-USDT-SWAP",
                    "spread_pct": 0.12,
                    "top5_depth_usdt": 100000.0,
                    "response_latency_ms": 80
                }]
            },
            "entry_price": 10.0,
            "price_path": [{
                "minute_after_entry": 30,
                "high_price": 10.4,
                "low_price": 9.95,
                "close_price": 10.4
            }],
            "fee_bps_per_side": 5.0,
            "slippage_bps_per_side": 3.0
        }]
    });
    fs::write(&input_path, serde_json::to_vec_pretty(&input).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_paper_acceptance"))
        .arg("--input")
        .arg(&input_path)
        .output()
        .expect("run paper acceptance cli");
    fs::remove_file(&input_path).ok();

    assert!(
        output.status.success(),
        "cli should pass, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON report");
    assert_eq!(payload["production_status"], "paper_ready");
    assert_eq!(payload["automatic_live_trading_allowed"], false);
    assert_eq!(payload["trade_samples"], 1);
}
