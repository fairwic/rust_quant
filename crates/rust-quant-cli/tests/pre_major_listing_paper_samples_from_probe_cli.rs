use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn cli_converts_probe_seeds_to_acceptance_samples() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_probe_seed_{}.json",
        std::process::id()
    ));
    fs::write(
        &input_path,
        r#"
        {
          "seeds": [
            {
              "announcement_id": "binance-announcement-1",
              "announcement_exchange": "binance",
              "base_asset": "test",
              "quote_asset": "usdt",
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
              "candidates": [
                {
                  "exchange": "bybit",
                  "symbol": "TESTUSDT",
                  "best_bid": 11.18,
                  "best_ask": 11.22,
                  "bid_depth_top5_usdt": 90000.0,
                  "ask_depth_top5_usdt": 70000.0,
                  "response_latency_ms": 60
                }
              ],
              "price_path": [
                {
                  "minute_after_entry": 30,
                  "high_price": 11.7,
                  "low_price": 11.1,
                  "close_price": 11.6
                }
              ]
            }
          ]
        }
        "#,
    )
    .expect("write probe seed fixture");

    let output = Command::new(env!(
        "CARGO_BIN_EXE_pre_major_listing_paper_samples_from_probe"
    ))
    .arg("--input")
    .arg(&input_path)
    .output()
    .expect("run pre_major_listing_paper_samples_from_probe");

    fs::remove_file(&input_path).ok();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout json");
    assert_eq!(json["samples"].as_array().expect("samples").len(), 1);
    assert_eq!(json["samples"][0]["input"]["base_asset"], "TEST");
    assert_eq!(
        json["samples"][0]["input"]["candidates"][0]["spread_pct"],
        0.3571
    );
    assert_eq!(
        json["production_note"],
        "paper_samples_only_live_trading_disabled"
    );
}
