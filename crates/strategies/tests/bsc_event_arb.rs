use rust_quant_domain::SignalDirection;
use rust_quant_strategies::framework::strategy_registry::get_strategy_registry;
use rust_quant_strategies::framework::strategy_trait::StrategyExecutor;
use rust_quant_strategies::implementations::{
    BscEventArbAction, BscEventArbSignalSnapshot, BscEventArbStrategy, BscEventArbStrategyConfig,
    BscEventArbStrategyExecutor,
};

fn safe_event_snapshot() -> BscEventArbSignalSnapshot {
    BscEventArbSignalSnapshot {
        chain_id: "bsc".to_string(),
        event_tags: vec!["binance_alpha".to_string(), "cex_listing".to_string()],
        price_usd: 100.0,
        volume_24h_usd: 8_000_000.0,
        volume_1h_vs_24h_avg: 6.0,
        depth_2pct_usd: 80_000.0,
        is_dex_only: false,
        sell_simulation_passed: true,
        buy_tax_pct: 1.0,
        sell_tax_pct: 1.0,
        has_blacklist_risk: false,
        has_pause_risk: false,
        has_mint_risk: false,
        cex_volume_share: 0.55,
        price_change_15m_pct: 10.0,
        price_change_1h_pct: 28.0,
        price_above_15m_vwap: true,
        volume_zscore_5m: 3.6,
        volume_zscore_15m: 3.2,
        oi_growth_1h_pct: 36.0,
        oi_growth_4h_pct: 88.0,
        funding_rate: -0.0002,
        short_crowding_score: 0.72,
        price_up_with_oi: true,
        cex_net_inflow_usd: 0.0,
        price_resilient_after_inflow: false,
        cex_outflow_after_inflow: false,
        spot_absorption: false,
        ..Default::default()
    }
}

#[test]
fn rave_like_event_squeeze_triggers_long_signal() {
    let config = BscEventArbStrategyConfig::default();
    let snapshot = safe_event_snapshot();

    let decision = BscEventArbStrategy::evaluate(&config, &snapshot);
    let signal = decision.to_signal(snapshot.price_usd, 1_776_000_000_000);

    assert_eq!(decision.action, BscEventArbAction::Long);
    assert!(signal.should_buy);
    assert!(!signal.should_sell);
    assert_eq!(signal.direction, SignalDirection::Long);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(90.0));
    assert!(signal
        .single_result
        .unwrap()
        .contains("\"action\":\"long\""));
}

#[test]
fn unsafe_contract_blocks_entry_even_when_event_is_strong() {
    let config = BscEventArbStrategyConfig::default();
    let mut snapshot = safe_event_snapshot();
    snapshot.sell_simulation_passed = false;
    snapshot.sell_tax_pct = 8.0;

    let decision = BscEventArbStrategy::evaluate(&config, &snapshot);

    assert_eq!(decision.action, BscEventArbAction::Flat);
    assert!(decision.has_reason("CONTRACT_SECURITY_BLOCK"));
    assert!(decision.has_reason("SELL_TAX_TOO_HIGH"));
}

#[test]
fn dex_only_low_depth_is_alert_only_not_trade() {
    let config = BscEventArbStrategyConfig::default();
    let mut snapshot = safe_event_snapshot();
    snapshot.is_dex_only = true;
    snapshot.depth_2pct_usd = 20_000.0;

    let decision = BscEventArbStrategy::evaluate(&config, &snapshot);
    let signal = decision.to_signal(snapshot.price_usd, 1_776_000_000_000);

    assert_eq!(decision.action, BscEventArbAction::Flat);
    assert!(decision.has_reason("DEX_DEPTH_TOO_THIN"));
    assert!(!signal.should_buy);
    assert!(!signal.should_sell);
}

#[test]
fn force_exit_never_becomes_short_signal() {
    let config = BscEventArbStrategyConfig::default();
    let mut snapshot = safe_event_snapshot();
    snapshot.price_below_15m_vwap = true;
    snapshot.oi_drop_1h_pct = 31.0;
    snapshot.funding_flipped_positive = true;
    snapshot.price_making_new_high = false;

    let decision = BscEventArbStrategy::evaluate(&config, &snapshot);
    let signal = decision.to_signal(snapshot.price_usd, 1_776_000_000_000);

    assert_eq!(decision.action, BscEventArbAction::ForceExit);
    assert!(!signal.should_buy);
    assert!(!signal.should_sell);
    assert_eq!(signal.direction, SignalDirection::Close);
    assert!(signal
        .single_result
        .unwrap()
        .contains("\"action\":\"force_exit\""));
}

#[test]
fn executor_is_registered_and_detects_bsc_event_arb_config() {
    let executor = BscEventArbStrategyExecutor::new();
    assert!(executor.can_handle(r#"{"strategy_name":"bsc_event_arb"}"#));

    let registry = get_strategy_registry();
    assert!(registry.contains("BscEventArb"));
    assert_eq!(
        registry
            .get("bsc_event_arb")
            .unwrap()
            .strategy_type()
            .as_str(),
        "bsc_event_arb"
    );
}
