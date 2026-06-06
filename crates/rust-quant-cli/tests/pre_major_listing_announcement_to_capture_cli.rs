use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn cli_converts_major_listing_announcement_to_capture_request() {
    let dir = std::env::temp_dir();
    let input_path = dir.join(format!(
        "pre_major_listing_announcement_{}.json",
        std::process::id()
    ));

    fs::write(
        &input_path,
        r#"
        {
          "announcement_id": "binance-announcement-1",
          "source": "binance_announcements",
          "title": "Binance Will List Test Protocol (TEST)",
          "content": "Trading will open for TEST/USDT.",
          "announced_at_ms": 1000,
          "detected_at_ms": 31000,
          "pre_announcement_price": 10.0,
          "announcement_price": 11.0,
          "entry_price": 11.2,
          "btc_5m_return_pct": -0.2,
          "eth_5m_return_pct": 0.1,
          "opening_upper_wick_rejection": false,
          "fee_bps_per_side": 5.0,
          "slippage_bps_per_side": 3.0,
          "price_path": []
        }
        "#,
    )
    .expect("write announcement input");

    let output = Command::new(env!(
        "CARGO_BIN_EXE_pre_major_listing_announcement_to_capture"
    ))
    .arg("--input")
    .arg(&input_path)
    .output()
    .expect("run pre_major_listing_announcement_to_capture");

    fs::remove_file(&input_path).ok();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout json");
    assert_eq!(
        json["production_note"],
        "announcement_capture_request_only_live_trading_disabled"
    );
    assert_eq!(json["request"]["announcement_id"], "binance-announcement-1");
    assert_eq!(json["request"]["announcement_exchange"], "binance");
    assert_eq!(json["request"]["base_asset"], "TEST");
    assert_eq!(json["request"]["quote_asset"], "USDT");
    assert_eq!(json["request"]["detected_at_ms"], 31000);
    assert_eq!(json["request"]["price_path"].as_array().unwrap().len(), 0);
}
