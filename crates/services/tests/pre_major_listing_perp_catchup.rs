use rust_quant_services::strategy::pre_major_listing_perp_catchup::{
    choose_secondary_perp_venue, ListingCatchupCandidate, ListingCatchupDecision,
    ListingCatchupInput,
};

fn candidate(exchange: &str, spread_pct: f64, top5_depth_usdt: f64) -> ListingCatchupCandidate {
    ListingCatchupCandidate {
        exchange: exchange.to_string(),
        symbol: "TEST-USDT-SWAP".to_string(),
        spread_pct,
        top5_depth_usdt,
        response_latency_ms: 80,
    }
}

fn valid_input() -> ListingCatchupInput {
    ListingCatchupInput {
        announcement_exchange: "binance".to_string(),
        base_asset: "TEST".to_string(),
        quote_asset: "USDT".to_string(),
        detection_latency_secs: 18,
        pre_announcement_return_15m_pct: 6.5,
        btc_5m_return_pct: -0.2,
        eth_5m_return_pct: 0.1,
        opening_upper_wick_rejection: false,
        candidates: vec![
            candidate("gate", 0.20, 70_000.0),
            candidate("bybit", 0.14, 110_000.0),
            candidate("bitget", 0.18, 120_000.0),
        ],
    }
}

#[test]
fn chooses_bitget_before_bybit_and_gate_when_tradeable() {
    let decision = choose_secondary_perp_venue(&valid_input());

    assert_eq!(
        decision,
        ListingCatchupDecision::Trade {
            exchange: "bitget".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            size_fraction_r: 0.3,
            stop_loss_pct: 2.0,
            take_profit_first_pct: 3.0,
            take_profit_second_pct: 5.0,
            max_hold_minutes: 120,
        }
    );
}

#[test]
fn falls_back_to_bybit_when_bitget_depth_is_unready() {
    let mut input = valid_input();
    input.candidates[2].top5_depth_usdt = 20_000.0;

    let decision = choose_secondary_perp_venue(&input);

    assert!(matches!(
        decision,
        ListingCatchupDecision::Trade { exchange, .. } if exchange == "bybit"
    ));
}

#[test]
fn rejects_stale_or_prepumped_announcements() {
    let mut stale = valid_input();
    stale.detection_latency_secs = 121;
    assert_eq!(
        choose_secondary_perp_venue(&stale),
        ListingCatchupDecision::Reject {
            reason: "listing_latency_too_high".to_string()
        }
    );

    let mut prepumped = valid_input();
    prepumped.pre_announcement_return_15m_pct = 21.0;
    assert_eq!(
        choose_secondary_perp_venue(&prepumped),
        ListingCatchupDecision::Reject {
            reason: "pre_pump_too_large".to_string()
        }
    );
}

#[test]
fn rejects_thin_orderbooks_and_macro_dumping() {
    let mut thin = valid_input();
    for candidate in &mut thin.candidates {
        candidate.top5_depth_usdt = 10_000.0;
    }
    assert_eq!(
        choose_secondary_perp_venue(&thin),
        ListingCatchupDecision::Reject {
            reason: "secondary_perp_depth_unready".to_string()
        }
    );

    let mut dumping = valid_input();
    dumping.btc_5m_return_pct = -1.3;
    assert_eq!(
        choose_secondary_perp_venue(&dumping),
        ListingCatchupDecision::Reject {
            reason: "macro_market_dumping".to_string()
        }
    );
}
