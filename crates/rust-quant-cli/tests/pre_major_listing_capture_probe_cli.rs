use serde_json::Value;
use std::fs;
use std::process::Command;
#[test]
fn cli_builds_probe_seed_from_orderbook_fixture_without_live_trading() {
    let dir = std::env::temp_dir();
    let request_path = dir.join(format!(
        "pre_major_listing_capture_request_{}.json",
        std::process::id()
    ));
    let orderbook_path = dir.join(format!(
        "pre_major_listing_capture_orderbooks_{}.json",
        std::process::id()
    ));
    fs::write(
        &request_path,
        r#"
        {
          "announcement_id": "binance-announcement-1",
          "announcement_exchange": "binance",
          "base_asset": "TEST",
          "quote_asset": "USDT",
          "announced_at_ms": 1000,
          "detected_at_ms": 31000,
          "pre_announcement_price": 10.0,
          "announcement_price": 11.0,
          "btc_5m_return_pct": -0.2,
          "eth_5m_return_pct": 0.1,
          "opening_upper_wick_rejection": false,
          "entry_price": 11.2,
          "fee_bps_per_side": 5.0,
          "slippage_bps_per_side": 3.0,
          "price_path": []
        }
        "#,
    )
    .expect("write capture request");
    fs::write(
        &orderbook_path,
        r#"
        {
          "orderbooks": [
            {
              "exchange": "bitget",
              "symbol": "TESTUSDT",
              "bids": [["11.18", "4000"], ["11.17", "3000"]],
              "asks": [["11.22", "3500"], ["11.23", "3000"]],
              "response_latency_ms": 45
            },
            {
              "exchange": "bybit",
              "symbol": "TESTUSDT",
              "bids": [["11.16", "1000"]],
              "asks": [["11.24", "1000"]],
              "response_latency_ms": 65
            }
          ]
        }
        "#,
    )
    .expect("write orderbook fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_capture_probe"))
        .arg("--input")
        .arg(&request_path)
        .arg("--orderbook-fixture")
        .arg(&orderbook_path)
        .output()
        .expect("run pre_major_listing_capture_probe");
    fs::remove_file(&request_path).ok();
    fs::remove_file(&orderbook_path).ok();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout json");
    assert_eq!(
        json["production_note"],
        "probe_capture_only_live_trading_disabled"
    );
    assert_eq!(json["seeds"].as_array().expect("seeds").len(), 1);
    let seed = &json["seeds"][0];
    assert_eq!(seed["announcement_id"], "binance-announcement-1");
    assert_eq!(seed["candidates"].as_array().expect("candidates").len(), 2);
    assert_eq!(seed["candidates"][0]["exchange"], "bitget");
    assert_eq!(seed["candidates"][0]["best_bid"], 11.18);
    assert_eq!(seed["candidates"][0]["best_ask"], 11.22);
    assert_eq!(seed["candidates"][0]["bid_depth_top5_usdt"], 78230.0);
    assert_eq!(seed["candidates"][0]["ask_depth_top5_usdt"], 72960.0);
}
