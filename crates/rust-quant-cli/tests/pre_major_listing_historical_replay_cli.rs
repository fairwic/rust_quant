use serde_json::json;
use std::fs;
use std::process::Command;
#[test]
fn cli_replays_fixture_announcements_without_enabling_live_trading() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_historical_replay_{}.json",
        std::process::id()
    ));
    let input = json!({
        "announcements": [{
            "announcement_id": "binance-ann-1",
            "source": "binance_announcements",
            "title": "Binance Will List Test Token (TEST)",
            "content": "Binance will list TEST and open TEST/USDT spot trading.",
            "announced_at_ms": 1_700_000_000_000u64,
            "detected_assets": ""
        }],
        "venue_candles": [{
            "exchange": "bitget",
            "base_asset": "TEST",
            "candles": [
                {"open_time": 1_699_999_100_000u64, "open": 10.0, "high": 10.1, "low": 9.9, "close": 10.0, "quote_volume": 80_000.0},
                {"open_time": 1_700_000_000_000u64, "open": 10.0, "high": 10.2, "low": 9.95, "close": 10.1, "quote_volume": 90_000.0},
                {"open_time": 1_700_000_060_000u64, "open": 10.1, "high": 10.2, "low": 10.0, "close": 10.1, "quote_volume": 90_000.0},
                {"open_time": 1_700_001_800_000u64, "open": 10.1, "high": 10.55, "low": 10.0, "close": 10.45, "quote_volume": 100_000.0}
            ]
        }]
    });
    fs::write(&input_path, serde_json::to_vec_pretty(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_historical_replay"))
        .arg("--fixture")
        .arg(&input_path)
        .arg("--min-trade-samples")
        .arg("1")
        .arg("--min-win-rate-pct")
        .arg("1")
        .output()
        .expect("run historical replay cli");
    fs::remove_file(&input_path).ok();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["mode"], "historical_kline_proxy");
    assert_eq!(payload["automatic_live_trading_allowed"], false);
    assert_eq!(payload["report"]["production_status"], "paper_ready");
    assert_eq!(payload["report"]["trade_samples"], 1);
    assert_eq!(
        payload["limitations"][0],
        "historical_orderbook_depth_unavailable"
    );
}
#[test]
fn cli_replays_binance_and_okx_fixture_announcements() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_historical_replay_multi_source_{}.json",
        std::process::id()
    ));
    let input = json!({
        "announcements": [
            {
                "announcement_id": "binance-ann-1",
                "source": "binance_announcements",
                "title": "Binance Will List Test Token (TEST)",
                "content": "Binance will list TEST and open TEST/USDT spot trading.",
                "announced_at_ms": 1_700_000_000_000u64,
                "detected_assets": ""
            },
            {
                "announcement_id": "okx-ann-1",
                "source": "okx_announcements",
                "title": "OKX to list Sample Token (SAMP) for spot trading",
                "content": "OKX will list SAMP and open SAMP/USDT spot trading.",
                "announced_at_ms": 1_700_010_000_000u64,
                "detected_assets": ""
            }
        ],
        "venue_candles": [
            {
                "exchange": "bitget",
                "base_asset": "TEST",
                "candles": profitable_candles(1_700_000_000_000u64)
            },
            {
                "exchange": "bitget",
                "base_asset": "SAMP",
                "candles": profitable_candles(1_700_010_000_000u64)
            }
        ]
    });
    fs::write(&input_path, serde_json::to_vec_pretty(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_historical_replay"))
        .arg("--fixture")
        .arg(&input_path)
        .arg("--min-trade-samples")
        .arg("2")
        .arg("--min-win-rate-pct")
        .arg("1")
        .output()
        .expect("run historical replay cli");
    fs::remove_file(&input_path).ok();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["announcements_read"], 2);
    assert_eq!(payload["report"]["trade_samples"], 2);
    assert_eq!(payload["report"]["production_status"], "paper_ready");
}
#[test]
fn cli_prefers_okx_listing_symbol_over_detected_asset_fallback() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_historical_replay_okx_asset_{}.json",
        std::process::id()
    ));
    let input = json!({
        "announcements": [{
            "announcement_id": "okx-rlusd-1",
            "source": "okx_announcements",
            "title": "OKX to list RLUSD (Ripple USD) for spot trading",
            "content": "OKX will list RLUSD and open spot trading.",
            "announced_at_ms": 1_700_020_000_000u64,
            "detected_assets": "XRP"
        }],
        "venue_candles": [{
            "exchange": "bitget",
            "base_asset": "RLUSD",
            "candles": profitable_candles(1_700_020_000_000u64)
        }]
    });
    fs::write(&input_path, serde_json::to_vec_pretty(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_historical_replay"))
        .arg("--fixture")
        .arg(&input_path)
        .arg("--min-trade-samples")
        .arg("1")
        .arg("--min-win-rate-pct")
        .arg("1")
        .output()
        .expect("run historical replay cli");
    fs::remove_file(&input_path).ok();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["report"]["trade_samples"], 1);
    assert_eq!(payload["report"]["trade_results"][0]["symbol"], "RLUSDUSDT");
}
#[test]
fn cli_rejects_okx_expiry_perps_as_major_spot_listing() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_historical_replay_okx_perps_{}.json",
        std::process::id()
    ));
    let input = json!({
        "announcements": [{
            "announcement_id": "okx-expiry-perps-1",
            "source": "okx_announcements",
            "title": "OKX to list TAOUSD, BNBUSD and LINKUSD Expiry Perps (X-Perp)",
            "content": "OKX will list expiry perps.",
            "announced_at_ms": 1_700_030_000_000u64,
            "detected_assets": "BNB"
        }],
        "venue_candles": [{
            "exchange": "bitget",
            "base_asset": "BNB",
            "candles": profitable_candles(1_700_030_000_000u64)
        }]
    });
    fs::write(&input_path, serde_json::to_vec_pretty(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_historical_replay"))
        .arg("--fixture")
        .arg(&input_path)
        .output()
        .expect("run historical replay cli");
    fs::remove_file(&input_path).ok();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["report"]["trade_samples"], 0);
    assert_eq!(
        payload["skipped"][0]["reason"],
        "not_positive_major_listing"
    );
}
#[test]
fn cli_rejects_binance_trading_pair_additions() {
    let input_path = std::env::temp_dir().join(format!(
        "pre_major_listing_historical_replay_binance_pairs_{}.json",
        std::process::id()
    ));
    let input = json!({
        "announcements": [{
            "announcement_id": "binance-pairs-1",
            "source": "binance_announcements",
            "title": "Binance Adds BNB/JPY, BTC/JPY & ETH/JPY Trading Pairs and Launches Zero-Fee Trading for JPY Spot Trading Pairs",
            "content": "Binance adds spot trading pairs.",
            "announced_at_ms": 1_700_040_000_000u64,
            "detected_assets": "BTC"
        }],
        "venue_candles": [{
            "exchange": "bitget",
            "base_asset": "BTC",
            "candles": profitable_candles(1_700_040_000_000u64)
        }]
    });
    fs::write(&input_path, serde_json::to_vec_pretty(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pre_major_listing_historical_replay"))
        .arg("--fixture")
        .arg(&input_path)
        .output()
        .expect("run historical replay cli");
    fs::remove_file(&input_path).ok();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["report"]["trade_samples"], 0);
    assert_eq!(
        payload["skipped"][0]["reason"],
        "not_positive_major_listing"
    );
}
fn profitable_candles(announced_at_ms: u64) -> serde_json::Value {
    json!([
        {"open_time": announced_at_ms - 900_000u64, "open": 10.0, "high": 10.1, "low": 9.9, "close": 10.0, "quote_volume": 80_000.0},
        {"open_time": announced_at_ms, "open": 10.0, "high": 10.2, "low": 9.95, "close": 10.1, "quote_volume": 90_000.0},
        {"open_time": announced_at_ms + 60_000u64, "open": 10.1, "high": 10.2, "low": 10.0, "close": 10.1, "quote_volume": 90_000.0},
        {"open_time": announced_at_ms + 1_800_000u64, "open": 10.1, "high": 10.55, "low": 10.0, "close": 10.45, "quote_volume": 100_000.0}
    ])
}
