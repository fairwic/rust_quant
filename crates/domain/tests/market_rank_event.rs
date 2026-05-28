use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};

#[test]
fn market_rank_event_type_uses_product_event_codes() {
    assert_eq!(MarketRankEventType::RankVelocity.as_str(), "rank_velocity");
    assert_eq!(MarketRankEventType::TopEntry.as_str(), "top_entry");
    assert_eq!(MarketRankEventType::TopExit.as_str(), "top_exit");

    assert_eq!(
        MarketRankEventType::try_from("rank_velocity").expect("rank_velocity should parse"),
        MarketRankEventType::RankVelocity
    );
    assert_eq!(
        MarketRankEventType::try_from("top_entry").expect("top_entry should parse"),
        MarketRankEventType::TopEntry
    );
    assert!(MarketRankEventType::try_from("unknown").is_err());
}

#[test]
fn market_rank_event_serializes_product_payload_fields() {
    let event = MarketRankEvent {
        id: None,
        exchange: "okx".to_string(),
        symbol: "ETH-USDT-SWAP".to_string(),
        event_type: MarketRankEventType::RankVelocity,
        timeframe: Some("15m".to_string()),
        old_rank: Some(42),
        new_rank: Some(18),
        delta_rank: Some(24),
        volume_24h_quote: None,
        current_price: Some(Decimal::new(2200, 0)),
        previous_price: Some(Decimal::new(2000, 0)),
        price_change_pct: Some(Decimal::new(100, 1)),
        price_direction: "up".to_string(),
        detected_at: chrono::DateTime::from_timestamp(1_774_814_400, 0)
            .expect("valid test timestamp"),
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    };

    let value = serde_json::to_value(&event).expect("event should serialize");
    assert_eq!(value["exchange"], "okx");
    assert_eq!(value["symbol"], "ETH-USDT-SWAP");
    assert_eq!(value["event_type"], "rank_velocity");
    assert_eq!(value["timeframe"], "15m");
    assert_eq!(value["old_rank"], 42);
    assert_eq!(value["new_rank"], 18);
    assert_eq!(value["delta_rank"], 24);
    assert_eq!(value["current_price"], "2200");
    assert_eq!(value["previous_price"], "2000");
    assert_eq!(value["price_change_pct"], "10.0");
    assert_eq!(value["price_direction"], "up");
    assert_eq!(value["source"], "scanner_service");
    assert_eq!(value["notification_state"], "pending");
}
