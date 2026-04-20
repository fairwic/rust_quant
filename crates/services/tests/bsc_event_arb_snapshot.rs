use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_services::strategy::BscEventArbSnapshotBuilder;
use serde_json::json;

fn snapshot_row(
    metric_type: &str,
    metric_time: i64,
    payload: serde_json::Value,
) -> ExternalMarketSnapshot {
    let mut row = ExternalMarketSnapshot::new(
        "dexscreener".to_string(),
        "RAVE".to_string(),
        metric_type.to_string(),
        metric_time,
    );
    row.raw_payload = Some(payload);
    row
}

#[test]
fn builds_bsc_event_arb_snapshot_from_external_market_rows() {
    let rows = vec![
        snapshot_row(
            "bsc_pair",
            100,
            json!({
                "chain_id": "bsc",
                "price_usd": 1.0,
                "event_tags": ["volume_surge"]
            }),
        ),
        snapshot_row(
            "bsc_pair",
            200,
            json!({
                "chain_id": "bsc",
                "event_tags": ["binance_alpha", "cex_listing"],
                "price_usd": 100.0,
                "volume_24h_usd": 8_000_000.0,
                "volume_1h_vs_24h_avg": 6.0,
                "depth_2pct_usd": 80_000.0,
                "is_dex_only": false
            }),
        ),
        snapshot_row(
            "bsc_security",
            200,
            json!({
                "sell_simulation_passed": true,
                "buy_tax_pct": 1.0,
                "sell_tax_pct": 1.0,
                "has_blacklist_risk": false,
                "has_pause_risk": false,
                "has_mint_risk": false
            }),
        ),
        snapshot_row(
            "cex_market",
            200,
            json!({
                "cex_volume_share": 0.55,
                "price_change_15m_pct": 10.0,
                "price_change_1h_pct": 28.0,
                "price_above_15m_vwap": true,
                "price_below_15m_vwap": false,
                "volume_zscore_5m": 3.6,
                "volume_zscore_15m": 3.2,
                "minutes_since_entry": 42,
                "max_unrealized_profit_pct": 12.0,
                "trailing_drawdown_pct": 4.0,
                "price_change_from_entry_pct": 9.0
            }),
        ),
        snapshot_row(
            "derivatives",
            200,
            json!({
                "oi_growth_1h_pct": 36.0,
                "oi_growth_4h_pct": 88.0,
                "funding_rate": -0.0002,
                "short_crowding_score": 0.72,
                "price_up_with_oi": true
            }),
        ),
        snapshot_row(
            "cex_flow",
            200,
            json!({
                "cex_net_inflow_usd": 0.0,
                "price_resilient_after_inflow": false,
                "cex_outflow_after_inflow": false,
                "spot_absorption": false,
                "cex_withdrawal_or_trading_restriction": true
            }),
        ),
        snapshot_row(
            "holder_concentration",
            200,
            json!({
                "top_holder_or_lp_abnormal_outflow": true
            }),
        ),
    ];

    let snapshot = BscEventArbSnapshotBuilder::build("RAVE", &rows).unwrap();

    assert_eq!(snapshot.chain_id, "bsc");
    assert_eq!(snapshot.event_tags, vec!["binance_alpha", "cex_listing"]);
    assert_eq!(snapshot.price_usd, 100.0);
    assert_eq!(snapshot.volume_24h_usd, 8_000_000.0);
    assert!(snapshot.sell_simulation_passed);
    assert_eq!(snapshot.cex_volume_share, 0.55);
    assert_eq!(snapshot.oi_growth_1h_pct, 36.0);
    assert_eq!(snapshot.funding_rate, -0.0002);
    assert_eq!(snapshot.minutes_since_entry, 42);
    assert_eq!(snapshot.max_unrealized_profit_pct, 12.0);
    assert_eq!(snapshot.price_change_from_entry_pct, 9.0);
    assert!(snapshot.cex_withdrawal_or_trading_restriction);
    assert!(snapshot.top_holder_or_lp_abnormal_outflow);
}
