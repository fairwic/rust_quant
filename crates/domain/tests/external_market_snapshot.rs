use rust_quant_domain::entities::ExternalMarketSnapshot;

#[test]
fn external_market_snapshot_serializes_expected_fields() {
    let snapshot = ExternalMarketSnapshot {
        id: None,
        source: "hyperliquid".to_string(),
        symbol: "ETH".to_string(),
        metric_type: "funding".to_string(),
        metric_time: 1_774_814_400_000,
        funding_rate: Some(0.0000105495),
        premium: Some(-0.0004156042),
        open_interest: Some(12345.67),
        oracle_price: Some(1983.26),
        mark_price: Some(1985.11),
        long_short_ratio: None,
        raw_payload: Some(serde_json::json!({
            "coin": "ETH",
            "fundingRate": "0.0000105495"
        })),
        created_at: None,
        updated_at: None,
    };

    let value = serde_json::to_value(&snapshot).expect("snapshot should serialize");
    assert_eq!(value["source"], "hyperliquid");
    assert_eq!(value["symbol"], "ETH");
    assert_eq!(value["metric_type"], "funding");
    assert_eq!(value["metric_time"], 1_774_814_400_000_i64);
    assert_eq!(value["funding_rate"], 0.0000105495);
    assert_eq!(value["premium"], -0.0004156042);
    assert_eq!(value["open_interest"], 12345.67);
}
