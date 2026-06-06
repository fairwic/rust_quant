use rust_quant_services::strategy::pre_major_listing_perp_catchup::{
    build_listing_catchup_paper_sample, choose_secondary_perp_venue,
    evaluate_listing_catchup_paper, ListingCatchupAcceptanceCriteria, ListingCatchupCandidate,
    ListingCatchupDecision, ListingCatchupInput, ListingCatchupPaperProbeSeed,
    ListingCatchupPaperSample, ListingCatchupPriceBar, ListingCatchupVenueProbe,
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

fn paper_sample(id: usize, exit_close_pct: f64) -> ListingCatchupPaperSample {
    ListingCatchupPaperSample {
        announcement_id: format!("announcement-{id}"),
        input: valid_input(),
        entry_price: 10.0,
        price_path: vec![ListingCatchupPriceBar {
            minute_after_entry: 30,
            high_price: 10.0 * (1.0 + exit_close_pct.max(0.0) / 100.0),
            low_price: 9.95,
            close_price: 10.0 * (1.0 + exit_close_pct / 100.0),
        }],
        fee_bps_per_side: 5.0,
        slippage_bps_per_side: 3.0,
    }
}

#[test]
fn paper_acceptance_blocks_until_sample_size_and_profitability_are_met() {
    let samples: Vec<_> = (0..29).map(|idx| paper_sample(idx, 4.0)).collect();

    let report = evaluate_listing_catchup_paper(
        samples,
        ListingCatchupAcceptanceCriteria {
            min_trade_samples: 30,
            min_win_rate_pct: 60.0,
            require_positive_total_net_return: true,
        },
    );

    assert_eq!(report.trade_samples, 29);
    assert_eq!(report.production_status, "blocked");
    assert!(report
        .blockers
        .contains(&"paper_trade_samples_below_minimum".to_string()));
    assert!(!report.automatic_live_trading_allowed);
}

#[test]
fn paper_acceptance_passes_at_sixty_percent_wins_but_keeps_live_disabled() {
    let mut samples = Vec::new();
    samples.extend((0..18).map(|idx| paper_sample(idx, 4.0)));
    samples.extend((18..30).map(|idx| paper_sample(idx, -1.0)));

    let report = evaluate_listing_catchup_paper(
        samples,
        ListingCatchupAcceptanceCriteria {
            min_trade_samples: 30,
            min_win_rate_pct: 60.0,
            require_positive_total_net_return: true,
        },
    );

    assert_eq!(report.trade_samples, 30);
    assert_eq!(report.win_rate_pct, 60.0);
    assert!(report.total_net_return_pct > 0.0);
    assert_eq!(report.production_status, "paper_ready");
    assert!(!report.automatic_live_trading_allowed);
}

#[test]
fn paper_acceptance_deduplicates_same_announcement_asset() {
    let first = paper_sample(1, 4.0);
    let mut duplicate = paper_sample(2, -10.0);
    duplicate.announcement_id = first.announcement_id.clone();
    duplicate.input.announcement_exchange = first.input.announcement_exchange.clone();
    duplicate.input.base_asset = first.input.base_asset.clone();
    duplicate.input.quote_asset = first.input.quote_asset.clone();

    let report = evaluate_listing_catchup_paper(
        vec![first, duplicate],
        ListingCatchupAcceptanceCriteria {
            min_trade_samples: 1,
            min_win_rate_pct: 1.0,
            require_positive_total_net_return: true,
        },
    );

    assert_eq!(report.unique_samples, 1);
    assert_eq!(report.duplicate_samples, 1);
    assert_eq!(report.trade_samples, 1);
    assert_eq!(report.win_rate_pct, 100.0);
}

#[test]
fn paper_acceptance_counts_fees_and_slippage_before_profitability() {
    let samples: Vec<_> = (0..30).map(|idx| paper_sample(idx, 0.05)).collect();

    let report = evaluate_listing_catchup_paper(
        samples,
        ListingCatchupAcceptanceCriteria {
            min_trade_samples: 30,
            min_win_rate_pct: 60.0,
            require_positive_total_net_return: true,
        },
    );

    assert_eq!(report.trade_samples, 30);
    assert_eq!(report.win_rate_pct, 0.0);
    assert!(report.total_net_return_pct < 0.0);
    assert!(report
        .blockers
        .contains(&"paper_win_rate_below_minimum".to_string()));
    assert!(report
        .blockers
        .contains(&"paper_total_net_return_not_positive".to_string()));
}

#[test]
fn paper_probe_seed_builds_acceptance_sample_from_orderbook_and_prices() {
    let sample = build_listing_catchup_paper_sample(ListingCatchupPaperProbeSeed {
        announcement_id: "binance-announcement-1".to_string(),
        announcement_exchange: "binance".to_string(),
        base_asset: "test".to_string(),
        quote_asset: "usdt".to_string(),
        announced_at_ms: 1_000,
        detected_at_ms: 31_000,
        pre_announcement_price: 10.0,
        announcement_price: 11.0,
        btc_5m_return_pct: -0.2,
        eth_5m_return_pct: 0.1,
        opening_upper_wick_rejection: false,
        entry_price: 11.2,
        fee_bps_per_side: 5.0,
        slippage_bps_per_side: 3.0,
        candidates: vec![
            ListingCatchupVenueProbe {
                exchange: "bybit".to_string(),
                symbol: "TESTUSDT".to_string(),
                best_bid: 11.18,
                best_ask: 11.22,
                bid_depth_top5_usdt: 90_000.0,
                ask_depth_top5_usdt: 70_000.0,
                response_latency_ms: 60,
            },
            ListingCatchupVenueProbe {
                exchange: "gate".to_string(),
                symbol: "TEST_USDT".to_string(),
                best_bid: 11.0,
                best_ask: 11.2,
                bid_depth_top5_usdt: 100_000.0,
                ask_depth_top5_usdt: 100_000.0,
                response_latency_ms: 120,
            },
        ],
        price_path: vec![ListingCatchupPriceBar {
            minute_after_entry: 30,
            high_price: 11.7,
            low_price: 11.1,
            close_price: 11.6,
        }],
    })
    .expect("valid probe seed should build paper sample");

    assert_eq!(sample.announcement_id, "binance-announcement-1");
    assert_eq!(sample.input.announcement_exchange, "binance");
    assert_eq!(sample.input.base_asset, "TEST");
    assert_eq!(sample.input.quote_asset, "USDT");
    assert_eq!(sample.input.detection_latency_secs, 30);
    assert_eq!(sample.input.pre_announcement_return_15m_pct, 10.0);
    assert_eq!(sample.input.candidates[0].exchange, "bybit");
    assert_eq!(sample.input.candidates[0].spread_pct, 0.3571);
    assert_eq!(sample.input.candidates[0].top5_depth_usdt, 70_000.0);
    assert_eq!(sample.price_path.len(), 1);
}

#[test]
fn paper_probe_seed_rejects_invalid_price_inputs_before_acceptance() {
    let mut seed = ListingCatchupPaperProbeSeed {
        announcement_id: "bad".to_string(),
        announcement_exchange: "binance".to_string(),
        base_asset: "TEST".to_string(),
        quote_asset: "USDT".to_string(),
        announced_at_ms: 1_000,
        detected_at_ms: 500,
        pre_announcement_price: 0.0,
        announcement_price: 11.0,
        btc_5m_return_pct: 0.0,
        eth_5m_return_pct: 0.0,
        opening_upper_wick_rejection: false,
        entry_price: 11.0,
        fee_bps_per_side: 5.0,
        slippage_bps_per_side: 3.0,
        candidates: vec![],
        price_path: vec![],
    };

    let error = build_listing_catchup_paper_sample(seed.clone()).expect_err("zero pre price");
    assert!(error.contains("pre_announcement_price_must_be_positive"));

    seed.pre_announcement_price = 10.0;
    let error = build_listing_catchup_paper_sample(seed).expect_err("negative latency");
    assert!(error.contains("detected_at_before_announced_at"));
}
