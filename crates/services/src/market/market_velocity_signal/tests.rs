use super::{
    build_market_velocity_strategy_signal_request,
    build_market_velocity_strategy_signal_request_with_entry_confirmation,
    build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry,
    dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled,
    market_velocity_signal_direct_dispatch_allowed, market_velocity_strategy_signal_log_context,
    should_dispatch_market_velocity_signal_to_quant_web_from_env, MarketVelocityEntryConfirmation,
    MarketVelocityFvgEntryMode, MarketVelocitySelectedEntry, MarketVelocitySignalTradeDirection,
    MarketVelocityStrategySignalBlocker, MarketVelocityStrategySignalConfig,
    MarketVelocityStrategySignalDecision,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};

const STABLE_PRODUCTION_PRESET: &str =
    "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1";
const STABLE_PRODUCTION_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1";
const STABLE_PRODUCTION_ENTRY_FILTER_MODE: &str = "rank_radar_4h15m_reclaim_ma_pullback";
const ENV_OPTIONAL_LIMIT_KEYS: &[&str] = &[
    "MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK",
    "MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT",
];

fn env_mutex() -> &'static Mutex<()> {
    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_MUTEX.get_or_init(|| Mutex::new(()))
}

fn rank_event(
    event_type: MarketRankEventType,
    price_direction: &str,
    current_price: Option<Decimal>,
) -> MarketRankEvent {
    MarketRankEvent {
        id: Some(991),
        exchange: "okx".to_string(),
        symbol: "ETH-USDT-SWAP".to_string(),
        event_type,
        timeframe: Some("15分钟".to_string()),
        old_rank: Some(54),
        new_rank: Some(30),
        delta_rank: Some(23),
        volume_24h_quote: Some(Decimal::new(120_000_000, 0)),
        current_price,
        previous_price: Some(Decimal::new(3200, 0)),
        price_change_pct: Some(Decimal::new(625, 2)),
        price_direction: price_direction.to_string(),
        technical_snapshot_status: "captured".to_string(),
        technical_snapshot: Some(rust_quant_domain::entities::MarketRankTechnicalSnapshot {
            timeframe: "4h".to_string(),
            period: 20,
            close_price: Decimal::new(3400, 0),
            ma_value: Decimal::new(3200, 0),
            ema_value: Decimal::new(3250, 0),
            ma_distance_pct: Decimal::new(625, 2),
            ema_distance_pct: Decimal::new(462, 2),
            ma_state: "above".to_string(),
            ema_state: "breakout_up".to_string(),
            candle_count: 80,
            snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
        }),
        detected_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}
fn entry_confirmation() -> MarketVelocityEntryConfirmation {
    entry_confirmation_with_trigger("reclaim_ema")
}
fn entry_confirmation_with_trigger(trigger: &str) -> MarketVelocityEntryConfirmation {
    MarketVelocityEntryConfirmation {
        timeframe: "15m".to_string(),
        period: 20,
        trigger: trigger.to_string(),
        latest_close: 3400.0,
        previous_close: Some(3330.0),
        previous_high: Some(3388.0),
        ma_value: 3350.0,
        ema_value: 3348.0,
        ma_distance_pct: 1.49,
        ema_distance_pct: 1.49,
        volume_ratio: Some(1.2),
        candle_count: 80,
        snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
    }
}

fn short_entry_confirmation() -> MarketVelocityEntryConfirmation {
    MarketVelocityEntryConfirmation {
        timeframe: "15m".to_string(),
        period: 20,
        trigger: "breakdown_range_low".to_string(),
        latest_close: 3320.0,
        previous_close: Some(3380.0),
        previous_high: Some(3410.0),
        ma_value: 3350.0,
        ema_value: 3348.0,
        ma_distance_pct: 0.9,
        ema_distance_pct: 0.84,
        volume_ratio: Some(1.4),
        candle_count: 80,
        snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
    }
}

fn selected_entry(trigger: &str, entry_price: f64) -> MarketVelocitySelectedEntry {
    MarketVelocitySelectedEntry {
        entry_price,
        entry_ts: DateTime::from_timestamp(1_774_818_000, 0).expect("valid test timestamp"),
        trigger: trigger.to_string(),
        entry_path: "retest_after_signal".to_string(),
        signal_pullback_pct: Some(2.286),
        structure_stop_loss_price: None,
        structure_stop_loss_source: None,
    }
}
#[test]
fn market_velocity_default_config_promotes_stable_production_preset() {
    let config = MarketVelocityStrategySignalConfig::default();
    assert_eq!(config.strategy_preset, STABLE_PRODUCTION_PRESET);
    assert_eq!(
        config.entry_rule_version,
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(config.min_delta_rank, 18);
    assert_eq!(config.max_delta_rank, Some(42));
    assert_eq!(config.min_price_change_pct, Some(5.0));
    assert_eq!(config.max_price_change_pct, Some(10.0));
    assert_eq!(config.stop_loss_pct, 0.0375);
    assert_eq!(config.take_profit_r, 1.7);
    assert_eq!(config.entry_max_average_distance_pct, 5.5);
    assert_eq!(config.entry_min_volume_ratio, 1.0);
    assert_eq!(config.entry_max_signal_pullback_pct, None);
    assert!(!config.entry_retest_after_signal);
    assert_eq!(config.entry_retest_max_wait_candles, 8);
    assert_eq!(config.fvg_entry_mode, MarketVelocityFvgEntryMode::Off);
    assert_eq!(
        config.entry_trigger_allowlist,
        vec!["reclaim_ema", "reclaim_ma", "pullback_hold_ema"]
    );
    assert!(!config.hybrid_live_entry_enabled());
    assert!(!market_velocity_signal_direct_dispatch_allowed(&config));
}

#[test]
fn market_velocity_env_config_allows_explicitly_unbounded_rank_and_price_limits() {
    let _guard = env_mutex().lock().expect("env guard");
    for key in ENV_OPTIONAL_LIMIT_KEYS {
        std::env::remove_var(key);
    }
    let default_config =
        MarketVelocityStrategySignalConfig::from_env().expect("default env config");
    assert_eq!(default_config.max_delta_rank, Some(42));
    assert_eq!(default_config.max_price_change_pct, Some(10.0));

    std::env::set_var("MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK", "none");
    std::env::set_var("MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT", "none");
    let unbounded_config =
        MarketVelocityStrategySignalConfig::from_env().expect("unbounded env config");
    assert_eq!(unbounded_config.max_delta_rank, None);
    assert_eq!(unbounded_config.max_price_change_pct, None);

    for key in ENV_OPTIONAL_LIMIT_KEYS {
        std::env::remove_var(key);
    }
}
#[test]
fn rank_velocity_up_event_builds_quant_web_strategy_signal() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let confirmation = entry_confirmation();
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&confirmation),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("strong rank velocity event should submit a strategy signal");
    };
    assert_eq!(request.source, "rust_quant");
    assert_eq!(
        request.external_id,
        "rust_quant:market_velocity:991:momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1:rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1"
    );
    assert_eq!(request.strategy_slug, "market_velocity");
    assert_eq!(
        request.strategy_key,
        "market_velocity:ETH-USDT-SWAP:15m:991"
    );
    assert_eq!(request.symbol, "ETH-USDT-SWAP");
    assert_eq!(request.signal_type, "entry");
    assert_eq!(request.direction, "long");
    assert_eq!(request.confidence, Some(0.78));
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["source_signal_type"], "market_velocity");
    assert_eq!(payload["rank_event_id"], 991);
    assert_eq!(payload["event_type"], "rank_velocity");
    assert_eq!(payload["side"], "buy");
    assert_eq!(payload["position_side"], "long");
    assert_eq!(payload["order_type"], "market");
    assert_eq!(payload["auto_execution_allowed"], true);
    assert_eq!(
        payload["execution_policy"]["mode"],
        "live_execution_authorized"
    );
    assert_eq!(payload["execution_policy"]["live_order_allowed"], true);
    assert_eq!(payload["execution_policy"]["paper_trade_required"], false);
    assert_eq!(
        payload["execution_policy"]["production_stage"],
        "live_execution_allowed"
    );
    assert_eq!(payload["paper_strategy_preset"], STABLE_PRODUCTION_PRESET);
    assert_eq!(
        payload["entry_rule_version"],
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(payload["risk_plan"]["entry_price"], 3400.0);
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3272.5);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3616.75);
    assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.0375);
    assert_eq!(payload["risk_plan"]["target_r"], 1.7);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
    assert_eq!(payload["risk_plan"]["reward_to_risk_mode"], "fixed_r");
    assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    assert_eq!(payload["entry_filter"]["status"], "confirmed");
    assert_eq!(
        payload["entry_filter"]["mode"],
        STABLE_PRODUCTION_ENTRY_FILTER_MODE
    );
    assert_eq!(
        payload["entry_filter"]["entry_rule_version"],
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(
        payload["entry_filter"]["paper_strategy_preset"],
        STABLE_PRODUCTION_PRESET
    );
    assert_eq!(payload["entry_filter"]["min_delta_rank"], 18);
    assert!(payload["entry_filter"].get("max_new_rank").is_none());
    assert_eq!(
        payload["entry_filter"]["trend_min_average_distance_pct"],
        0.0
    );
    assert_eq!(
        payload["entry_filter"]["entry_trigger_filter_version"],
        "entry_trigger_allowlist_v1"
    );
    assert_eq!(
        payload["entry_filter"]["entry_trigger_allowlist"],
        json!(["reclaim_ema", "reclaim_ma", "pullback_hold_ema"])
    );
    assert_eq!(
        payload["entry_filter"]["entry_trigger_blocklist"],
        json!([])
    );
    assert_eq!(payload["entry_confirmation"]["timeframe"], "15m");
    assert_eq!(payload["entry_confirmation"]["trigger"], "reclaim_ema");
}

#[test]
fn breakdown_short_config_builds_short_signal_without_live_handoff_contract() {
    let config = MarketVelocityStrategySignalConfig {
        strategy_slug: "market_velocity_breakdown_short".to_string(),
        strategy_preset:
            "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1"
                .to_string(),
        entry_rule_version: "rank_radar_15m_short_r0375_10r_15msup_brkdn_d5_72_p1p5_12_v1"
            .to_string(),
        trade_direction: MarketVelocitySignalTradeDirection::Short,
        min_delta_rank: 5,
        max_delta_rank: Some(72),
        min_price_change_pct: Some(1.5),
        max_price_change_pct: Some(12.0),
        take_profit_r: 1.0,
        entry_trigger_allowlist: vec!["breakdown_range_low".to_string()],
        require_technical_confirmation: true,
        require_entry_confirmation: true,
        ..MarketVelocityStrategySignalConfig::default()
    };
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "down",
        Some(Decimal::new(3400, 0)),
    );
    event.price_change_pct = Some(Decimal::new(-625, 2));
    let snapshot = event
        .technical_snapshot
        .as_mut()
        .expect("test event has technical snapshot");
    snapshot.close_price = Decimal::new(3400, 0);
    snapshot.ma_value = Decimal::new(3500, 0);
    snapshot.ema_value = Decimal::new(3480, 0);
    snapshot.ma_state = "below".to_string();
    snapshot.ema_state = "breakdown_down".to_string();
    snapshot.ma_distance_pct = Decimal::new(286, 2);
    snapshot.ema_distance_pct = Decimal::new(230, 2);

    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&short_entry_confirmation()),
    )
    .expect("valid market velocity breakdown event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("breakdown event should submit a short strategy signal: {decision:?}");
    };
    assert_eq!(request.strategy_slug, "market_velocity_breakdown_short");
    assert_eq!(
        request.strategy_key,
        "market_velocity_breakdown_short:ETH-USDT-SWAP:15m:991"
    );
    assert_eq!(request.direction, "short");
    assert_eq!(request.title, "Market Velocity short signal ETH-USDT-SWAP");
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(
        payload["source_signal_type"],
        "market_velocity_breakdown_short"
    );
    assert_eq!(payload["strategy_slug"], "market_velocity_breakdown_short");
    assert_eq!(payload["side"], "sell");
    assert_eq!(payload["position_side"], "short");
    assert_eq!(payload["auto_execution_allowed"], false);
    assert_eq!(payload["execution_policy"]["mode"], "signal_only");
    assert_eq!(payload["execution_policy"]["live_order_allowed"], false);
    assert_eq!(payload["execution_policy"]["paper_trade_required"], true);
    assert_eq!(
        payload["execution_policy"]["production_stage"],
        "paper_signal_only"
    );
    assert_eq!(payload["risk_plan"]["direction"], "short");
    assert_eq!(payload["risk_plan"]["entry_price"], 3400.0);
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3527.5);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3272.5);
    assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    assert_eq!(payload["signal"]["should_buy"], false);
    assert_eq!(payload["signal"]["should_sell"], true);
    assert_eq!(payload["signal"]["open_price"], 3400.0);
    assert_eq!(payload["signal"]["signal_kline_stop_loss_price"], 3527.5);
    assert_eq!(payload["signal"]["short_signal_take_profit_price"], 3272.5);
    assert_eq!(payload["signal"]["direction"], "Short");
    assert_eq!(payload["entry_filter"]["trade_direction"], "short");
    assert_eq!(
        payload["entry_filter"]["entry_trigger_allowlist"],
        json!(["breakdown_range_low"])
    );
}

#[test]
fn breakdown_short_low_price_signal_keeps_stop_loss_above_entry() {
    let config = MarketVelocityStrategySignalConfig {
        strategy_slug: "market_velocity_breakdown_short".to_string(),
        strategy_preset: "research_momentum_short_04sl_065r_15m_support_breakdown_v5".to_string(),
        entry_rule_version: "rank_radar_15m_short_r04_065r_15msup_brkdn_v5".to_string(),
        trade_direction: MarketVelocitySignalTradeDirection::Short,
        min_delta_rank: 1,
        max_delta_rank: Some(100),
        min_price_change_pct: Some(0.5),
        max_price_change_pct: Some(12.0),
        stop_loss_pct: 0.04,
        take_profit_r: 0.65,
        entry_trigger_allowlist: vec!["breakdown_range_low".to_string()],
        require_technical_confirmation: false,
        require_entry_confirmation: false,
        ..MarketVelocityStrategySignalConfig::default()
    };
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "down",
        Some(Decimal::new(2263, 9)),
    );
    event.price_change_pct = Some(Decimal::new(-122217372, 8));
    let mut selected = selected_entry("breakdown_range_low", 0.000002257);
    selected.structure_stop_loss_price = None;

    let decision =
        build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
            &event,
            &config,
            None,
            Some(&selected),
        )
        .expect("low-price breakdown short event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("low-price breakdown short should submit a signal: {decision:?}");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    let stop = payload["risk_plan"]["selected_stop_loss_price"]
        .as_f64()
        .expect("stop loss should be numeric");

    assert!(stop > selected.entry_price);
    assert_eq!(payload["execution_policy"]["live_order_allowed"], false);
}
#[test]
fn market_velocity_signal_does_not_block_by_new_rank() {
    let config = MarketVelocityStrategySignalConfig::default();
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.new_rank = Some(80);
    event.price_change_pct = Some(Decimal::new(625, 2));
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&entry_confirmation()),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("new_rank is diagnostic only and must not block entry: {decision:?}");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["new_rank"], 80);
    assert!(payload["entry_filter"].get("max_new_rank").is_none());
}
#[test]
fn market_velocity_payload_reuses_strategy_signal_live_entry_contract() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let confirmation = entry_confirmation();
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&confirmation),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("strong rank velocity event should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["source_signal_type"], "market_velocity");
    assert_eq!(payload["strategy_type"], "market_velocity");
    assert_eq!(
        payload["strategy_key"],
        "market_velocity:ETH-USDT-SWAP:15m:991"
    );
    assert_eq!(payload["client_order_id"], "rqmv9911774814400000");
    assert_eq!(payload["signal"]["should_buy"], true);
    assert_eq!(payload["signal"]["should_sell"], false);
    assert_eq!(payload["signal"]["open_price"], 3400.0);
    assert_eq!(payload["signal"]["signal_kline_stop_loss_price"], 3272.5);
    assert_eq!(payload["signal"]["long_signal_take_profit_price"], 3616.75);
    assert_eq!(
        payload["signal"]["stop_loss_source"],
        "market_velocity_fixed_0375sl"
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3272.5);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3616.75);
    assert_eq!(payload["risk_plan"]["target_r"], 1.7);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
}

#[test]
fn market_velocity_live_selected_entry_overrides_signal_open_price_without_losing_event_snapshot() {
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &json!({
            "strategy_slug": "market_velocity",
            "strategy_preset": "research_momentum_structure_stop_runner_v1",
            "entry_rule_version": "rank_radar_4h15m_structure_stop_runner_v1",
            "stop_loss_mode": "structure_or_fixed",
            "require_technical_confirmation": false
        }),
        &json!({
            "stop_loss_pct": 0.04,
            "take_profit_r": 2.4,
            "runner_target_r": 8.0,
            "runner_fraction": 0.3,
            "runner_stop_r": 0.0
        }),
    )
    .expect("structure stop runner config");
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(105, 0)),
    );
    let confirmation = MarketVelocityEntryConfirmation {
        timeframe: "15m".to_string(),
        period: 20,
        trigger: "reclaim_ema".to_string(),
        latest_close: 103.1,
        previous_close: Some(100.9),
        previous_high: Some(103.2),
        ma_value: 102.0,
        ema_value: 101.1,
        ma_distance_pct: 1.078,
        ema_distance_pct: 1.979,
        volume_ratio: Some(1.4),
        candle_count: 80,
        snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
    };
    let decision =
        build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
            &event,
            &config,
            Some(&confirmation),
            Some(&selected_entry(
                "reclaim_ema+retest_after_signal+fvg_fallback",
                102.6,
            )),
        )
        .expect("selected live entry should build a strategy signal");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("selected live entry should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["current_price"], 105.0);
    assert_eq!(payload["selected_entry"]["entry_price"], 102.6);
    assert_eq!(
        payload["selected_entry"]["trigger"],
        "reclaim_ema+retest_after_signal+fvg_fallback"
    );
    assert_eq!(
        payload["selected_entry"]["entry_path"],
        "retest_after_signal"
    );
    assert_eq!(payload["entry_confirmation"]["trigger"], "reclaim_ema");
    assert_eq!(payload["signal"]["open_price"], 102.6);
    assert_eq!(payload["risk_plan"]["entry_price"], 102.6);
    assert_eq!(
        payload["selected_entry"]["structure_stop_loss_price"],
        101.1
    );
    assert_eq!(
        payload["selected_entry"]["structure_stop_loss_source"],
        "entry_confirmation_ema"
    );
    assert_eq!(
        payload["risk_plan"]["stop_loss_selection_mode"],
        "structure_or_fixed"
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 101.1);
    assert_eq!(
        payload["risk_plan"]["selected_stop_loss_source"],
        "entry_confirmation_ema"
    );
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 106.2);
    assert_eq!(
        payload["risk_plan"]["take_profit_legs"],
        json!([
            {
                "leg_index": 1,
                "target_r": 2.4,
                "fraction": 0.7,
                "price": 106.2,
                "stop_after_fill_r": 0.0,
                "role": "base_take_profit"
            },
            {
                "leg_index": 2,
                "target_r": 8.0,
                "fraction": 0.3,
                "price": 114.6,
                "role": "runner_take_profit"
            }
        ])
    );
}

#[test]
fn market_velocity_live_selected_entry_applies_structure_stop_min_pct_floor() {
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &json!({
            "strategy_slug": "market_velocity",
            "strategy_preset": "research_momentum_structure_stop_floor_v1",
            "entry_rule_version": "rank_radar_4h15m_structure_stop_floor_v1",
            "stop_loss_mode": "structure_or_fixed",
            "structure_stop_min_pct": 0.02,
            "require_technical_confirmation": false
        }),
        &json!({
            "stop_loss_pct": 0.04,
            "take_profit_r": 2.0
        }),
    )
    .expect("structure stop floor config");
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(105, 0)),
    );
    let confirmation = MarketVelocityEntryConfirmation {
        timeframe: "15m".to_string(),
        period: 20,
        trigger: "reclaim_ema".to_string(),
        latest_close: 103.1,
        previous_close: Some(100.9),
        previous_high: Some(103.2),
        ma_value: 102.0,
        ema_value: 101.1,
        ma_distance_pct: 1.078,
        ema_distance_pct: 1.979,
        volume_ratio: Some(1.4),
        candle_count: 80,
        snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
    };
    let decision =
        build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
            &event,
            &config,
            Some(&confirmation),
            Some(&selected_entry(
                "reclaim_ema+retest_after_signal+fvg_fallback",
                102.6,
            )),
        )
        .expect("selected live entry should build a strategy signal");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("selected live entry should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 100.548);
    assert_eq!(payload["risk_plan"]["selected_stop_loss_percent"], 0.02);
    assert_eq!(payload["risk_plan"]["structure_stop_min_pct"], 0.02);
    assert_eq!(
        payload["risk_plan"]["selected_stop_loss_source"],
        "entry_confirmation_ema+min_pct_floor"
    );
}

#[test]
fn market_velocity_live_selected_entry_supports_structure_with_cap_inside_bounds() {
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &json!({
            "strategy_slug": "market_velocity",
            "strategy_preset": "research_momentum_structure_cap_v1",
            "entry_rule_version": "rank_radar_4h15m_structure_cap_v1",
            "stop_loss_mode": "structure_with_cap",
            "structure_stop_min_pct": 0.02,
            "require_technical_confirmation": false
        }),
        &json!({
            "stop_loss_pct": 0.05,
            "take_profit_r": 2.0
        }),
    )
    .expect("structure cap config");
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(105, 0)),
    );
    let confirmation = MarketVelocityEntryConfirmation {
        timeframe: "15m".to_string(),
        period: 20,
        trigger: "reclaim_ema".to_string(),
        latest_close: 103.1,
        previous_close: Some(100.9),
        previous_high: Some(103.2),
        ma_value: 102.0,
        ema_value: 101.1,
        ma_distance_pct: 1.078,
        ema_distance_pct: 1.979,
        volume_ratio: Some(1.4),
        candle_count: 80,
        snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
    };
    let mut entry = selected_entry("reclaim_ema+retest_after_signal+fvg_fallback", 102.6);
    entry.structure_stop_loss_price = Some(98.8);
    entry.structure_stop_loss_source = Some("fvg_15m_impulse_lower".to_string());

    let decision =
        build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
            &event,
            &config,
            Some(&confirmation),
            Some(&entry),
        )
        .expect("selected live entry should build a strategy signal");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("selected live entry should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(
        payload["risk_plan"]["stop_loss_selection_mode"],
        "structure_with_cap"
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 98.8);
    assert_eq!(payload["risk_plan"]["selected_stop_loss_percent"], 0.037037);
    assert_eq!(
        payload["risk_plan"]["selected_stop_loss_source"],
        "fvg_15m_impulse_lower"
    );
}

#[test]
fn market_velocity_live_selected_entry_caps_structure_with_cap_at_max_pct() {
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &json!({
            "strategy_slug": "market_velocity",
            "strategy_preset": "research_momentum_structure_cap_v1",
            "entry_rule_version": "rank_radar_4h15m_structure_cap_v1",
            "stop_loss_mode": "structure_with_cap",
            "structure_stop_min_pct": 0.02,
            "require_technical_confirmation": false
        }),
        &json!({
            "stop_loss_pct": 0.05,
            "take_profit_r": 2.0
        }),
    )
    .expect("structure cap config");
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(105, 0)),
    );
    let confirmation = MarketVelocityEntryConfirmation {
        timeframe: "15m".to_string(),
        period: 20,
        trigger: "reclaim_ema".to_string(),
        latest_close: 103.1,
        previous_close: Some(100.9),
        previous_high: Some(103.2),
        ma_value: 102.0,
        ema_value: 101.1,
        ma_distance_pct: 1.078,
        ema_distance_pct: 1.979,
        volume_ratio: Some(1.4),
        candle_count: 80,
        snapshot_at: DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp"),
    };
    let mut entry = selected_entry("reclaim_ema+retest_after_signal+fvg_fallback", 102.6);
    entry.structure_stop_loss_price = Some(96.0);
    entry.structure_stop_loss_source = Some("fvg_15m_impulse_lower".to_string());

    let decision =
        build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
            &event,
            &config,
            Some(&confirmation),
            Some(&entry),
        )
        .expect("selected live entry should build a strategy signal");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("selected live entry should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(
        payload["risk_plan"]["stop_loss_selection_mode"],
        "structure_with_cap"
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 97.47);
    assert_eq!(payload["risk_plan"]["selected_stop_loss_percent"], 0.05);
    assert_eq!(
        payload["risk_plan"]["selected_stop_loss_source"],
        "fvg_15m_impulse_lower+max_pct_cap"
    );
}

#[test]
fn hybrid_live_entry_config_disables_direct_scanner_dispatch() {
    let config = MarketVelocityStrategySignalConfig {
        entry_max_signal_pullback_pct: Some(3.0),
        entry_retest_after_signal: true,
        entry_retest_max_wait_candles: 1,
        fvg_entry_mode: MarketVelocityFvgEntryMode::M15ImpulseRetrace,
        ..MarketVelocityStrategySignalConfig::default()
    };
    assert!(
        !market_velocity_signal_direct_dispatch_allowed(&config),
        "hybrid live entry configs must not let scanner_service bypass the live handoff shell"
    );
}

#[test]
fn legacy_momentum_config_keeps_direct_scanner_dispatch_enabled() {
    let config = MarketVelocityStrategySignalConfig {
        strategy_preset: "momentum_03sl_20r_v5".to_string(),
        entry_rule_version: "rank_radar_4h_trend_15m_momentum_03sl_20r_v5".to_string(),
        min_delta_rank: 15,
        max_delta_rank: None,
        min_price_change_pct: None,
        max_price_change_pct: None,
        stop_loss_pct: 0.03,
        take_profit_r: 2.0,
        entry_max_average_distance_pct: 4.0,
        entry_min_volume_ratio: 1.0,
        entry_max_signal_pullback_pct: None,
        entry_retest_after_signal: false,
        entry_retest_max_wait_candles: 8,
        fvg_entry_mode: MarketVelocityFvgEntryMode::Off,
        require_entry_confirmation: false,
        entry_trigger_allowlist: vec![
            "breakout_previous_high".to_string(),
            "reclaim_ema".to_string(),
        ],
        ..MarketVelocityStrategySignalConfig::default()
    };
    assert!(
        market_velocity_signal_direct_dispatch_allowed(&config),
        "legacy non-hybrid configs may still use direct scanner dispatch"
    );
}
#[test]
fn default_market_velocity_signal_payload_uses_stable_production_preset() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let confirmation = entry_confirmation();
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&confirmation),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("default production event should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(config.stop_loss_pct, 0.0375);
    assert_eq!(config.take_profit_r, 1.7);
    assert_eq!(config.max_holding_hours, 48);
    assert_eq!(config.automation_mode, "live_execution_authorized");
    assert!(config.live_order_allowed);
    assert!(!config.paper_trade_required);
    assert!(
        config.symbol_blocklist.is_empty(),
        "production default must not depend on historical symbol blocklist"
    );
    assert_eq!(config.entry_max_average_distance_pct, 5.5);
    assert_eq!(payload["paper_strategy_preset"], STABLE_PRODUCTION_PRESET);
    assert_eq!(
        payload["entry_rule_version"],
        STABLE_PRODUCTION_ENTRY_RULE_VERSION
    );
    assert_eq!(
        payload["entry_filter"]["mode"],
        STABLE_PRODUCTION_ENTRY_FILTER_MODE
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3272.5);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3616.75);
    assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.0375);
    assert_eq!(payload["risk_plan"]["target_r"], 1.7);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
    assert_eq!(
        payload["entry_filter"]["entry_max_average_distance_pct"],
        5.5
    );
}
#[test]
fn strategy_config_json_overrides_market_velocity_signal_defaults() {
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &json!({
            "strategy_slug": "market_velocity",
            "strategy_preset": "momentum_03sl_20r_v5",
            "entry_rule_version": "rank_radar_4h_trend_15m_momentum_03sl_20r_v5",
            "min_delta_rank": 12,
            "max_delta_rank": 40,
            "min_price_change_pct": 5.0,
            "max_price_change_pct": 10.0,
            "automation_mode": "signal_only",
            "live_order_allowed": false,
            "paper_trade_required": true,
            "require_technical_confirmation": true,
            "require_entry_confirmation": true,
            "trend_min_average_distance_pct": 0.2,
            "entry_confirmation_period": 18,
            "entry_confirmation_fetch_limit": 90,
            "entry_max_average_distance_pct": 3.6,
            "entry_min_volume_ratio": 1.15,
            "entry_max_signal_pullback_pct": 3.0,
            "entry_retest_tolerance_pct": 0.3,
            "entry_retest_after_signal": true,
            "entry_retest_max_wait_candles": 1,
            "fvg_entry_mode": "m15_impulse_retrace",
            "fvg_lookback_candles": 40,
            "fvg_max_wait_candles": 24,
            "fvg_impulse_retrace_fill_pct": 20.0,
            "fvg_impulse_retrace_min_wait_candles": 0,
            "entry_trigger_allowlist": ["breakout_previous_high"],
            "entry_trigger_blocklist": ["pullback_hold_ema"],
            "symbol_blocklist": ["DOGE-USDT-SWAP"]
        }),
        &json!({
            "max_loss_percent": 0.0375,
            "fix_signal_kline_take_profit_ratio": 2.7,
            "is_used_signal_k_line_stop_loss": true,
            "max_hold_time": 129600
        }),
    )
    .expect("strategy config json should parse");
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let mut confirmation = entry_confirmation_with_trigger("breakout_previous_high");
    confirmation.period = 18;
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&confirmation),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!(
            "configured production event should submit a strategy signal: {decision:?}, config={config:?}, confirmation={confirmation:?}"
        );
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(config.stop_loss_pct, 0.0375);
    assert_eq!(config.take_profit_r, 2.7);
    assert_eq!(config.max_holding_hours, 36);
    assert_eq!(config.min_delta_rank, 12);
    assert_eq!(config.max_delta_rank, Some(40));
    assert_eq!(config.min_price_change_pct, Some(5.0));
    assert_eq!(config.max_price_change_pct, Some(10.0));
    assert_eq!(config.entry_confirmation_period, 18);
    assert_eq!(config.entry_confirmation_fetch_limit, 90);
    assert_eq!(config.entry_max_average_distance_pct, 3.6);
    assert_eq!(config.entry_min_volume_ratio, 1.15);
    assert_eq!(config.entry_max_signal_pullback_pct, Some(3.0));
    assert_eq!(config.entry_retest_tolerance_pct, 0.3);
    assert!(config.entry_retest_after_signal);
    assert_eq!(config.entry_retest_max_wait_candles, 1);
    assert_eq!(
        config.fvg_entry_mode,
        MarketVelocityFvgEntryMode::M15ImpulseRetrace
    );
    assert_eq!(config.fvg_lookback_candles, 40);
    assert_eq!(config.fvg_max_wait_candles, 24);
    assert_eq!(config.fvg_impulse_retrace_fill_pct, 20.0);
    assert_eq!(config.fvg_impulse_retrace_min_wait_candles, 0);
    assert_eq!(config.automation_mode, "signal_only");
    assert!(!config.live_order_allowed);
    assert!(config.paper_trade_required);
    assert_eq!(
        config.entry_trigger_allowlist,
        vec!["breakout_previous_high"]
    );
    assert_eq!(config.entry_trigger_blocklist, vec!["pullback_hold_ema"]);
    assert_eq!(config.symbol_blocklist, vec!["DOGE-USDT-SWAP"]);
    assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.0375);
    assert_eq!(payload["risk_plan"]["target_r"], 2.7);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 36);
    assert_eq!(
        payload["entry_filter"]["entry_trigger_allowlist"],
        json!(["breakout_previous_high"])
    );
    assert_eq!(payload["entry_filter"]["max_delta_rank"], 40);
    assert_eq!(payload["entry_filter"]["min_price_change_pct"], 5.0);
    assert_eq!(payload["entry_filter"]["entry_retest_after_signal"], true);
    assert_eq!(
        payload["entry_filter"]["fvg_entry_mode"],
        "m15_impulse_retrace"
    );
    assert_eq!(payload["entry_filter"]["max_price_change_pct"], 10.0);
    assert_eq!(payload["auto_execution_allowed"], false);
    assert_eq!(payload["execution_policy"]["mode"], "signal_only");
    assert_eq!(payload["execution_policy"]["live_order_allowed"], false);
    assert_eq!(payload["execution_policy"]["paper_trade_required"], true);
}
#[test]
fn strategy_config_json_builds_partial_take_profit_legs_for_runner() {
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &json!({
            "strategy_slug": "market_velocity",
            "strategy_preset": "momentum_03sl_24r_runner_8r_30pct_v1",
            "entry_rule_version": "rank_radar_4h_trend_15m_momentum_03sl_24r_v5"
        }),
        &json!({
            "stop_loss_pct": 0.03,
            "take_profit_r": 2.4,
            "runner_target_r": 8.0,
            "runner_fraction": 0.3,
            "runner_stop_r": 0.0
        }),
    )
    .expect("runner strategy config json should parse");
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&entry_confirmation()),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("configured production event should submit a strategy signal: {decision:?}");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(config.take_profit_r, 2.4);
    assert_eq!(config.runner_target_r, Some(8.0));
    assert_eq!(config.runner_fraction, 0.3);
    assert_eq!(config.runner_stop_r, 0.0);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3644.8);
    assert_eq!(
        payload["risk_plan"]["take_profit_legs"],
        json!([
            {
                "leg_index": 1,
                "target_r": 2.4,
                "fraction": 0.7,
                "price": 3644.8,
                "stop_after_fill_r": 0.0,
                "role": "base_take_profit"
            },
            {
                "leg_index": 2,
                "target_r": 8.0,
                "fraction": 0.3,
                "price": 4216.0,
                "role": "runner_take_profit"
            }
        ])
    );
}
#[test]
fn market_velocity_default_entry_filter_blocks_overextended_15m_confirmation() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let mut confirmation = entry_confirmation();
    confirmation.ema_distance_pct = 5.51;
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&confirmation),
        )
        .expect("valid market velocity event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::EntryTimingOverextended
        )
    );
}
#[test]
fn market_velocity_blocks_weak_4h_trend_distance() {
    let config = MarketVelocityStrategySignalConfig {
        trend_min_average_distance_pct: 4.0,
        ..MarketVelocityStrategySignalConfig::default()
    };
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let snapshot = event
        .technical_snapshot
        .as_mut()
        .expect("test event has technical snapshot");
    snapshot.ma_distance_pct = Decimal::new(350, 2);
    snapshot.ema_distance_pct = Decimal::new(350, 2);
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::TechnicalTrendNotConfirmed
        )
    );
}
#[test]
fn market_velocity_default_entry_trigger_filter_blocks_weak_trigger() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let confirmation = entry_confirmation_with_trigger("breakout_previous_high");
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&confirmation),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::EntryTriggerFiltered
        )
    );
}
#[test]
fn market_velocity_default_entry_trigger_filter_allows_reclaim_ema() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let confirmation = entry_confirmation_with_trigger("reclaim_ema");
    assert!(matches!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&confirmation),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Submit(_)
    ));
}
#[test]
fn market_velocity_entry_trigger_blocklist_has_precedence() {
    let config = MarketVelocityStrategySignalConfig {
        entry_trigger_allowlist: vec![
            "breakout_previous_high".to_string(),
            "reclaim_ema".to_string(),
        ],
        entry_trigger_blocklist: vec!["reclaim_ema".to_string()],
        ..MarketVelocityStrategySignalConfig::default()
    };
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation_with_trigger("reclaim_ema")),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::EntryTriggerFiltered
        )
    );
}
#[test]
fn market_velocity_symbol_blocklist_blocks_signal_before_submit() {
    let config = MarketVelocityStrategySignalConfig {
        symbol_blocklist: vec!["ASTER-USDT-SWAP".to_string()],
        ..MarketVelocityStrategySignalConfig::default()
    };
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.symbol = "aster-usdt-swap".to_string();
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation_with_trigger("breakout_previous_high")),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::SymbolFiltered
        )
    );
}
#[test]
fn dry_run_execution_task_mode_does_not_authorize_auto_execution() {
    let config = MarketVelocityStrategySignalConfig {
        automation_mode: "execution_task_dry_run".to_string(),
        live_order_allowed: true,
        paper_trade_required: false,
        ..MarketVelocityStrategySignalConfig::default()
    };
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&entry_confirmation()),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("dry-run execution task mode should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["auto_execution_allowed"], false);
    assert_eq!(
        payload["execution_policy"]["mode"],
        "execution_task_dry_run"
    );
    assert_eq!(payload["execution_policy"]["live_order_allowed"], true);
    assert_eq!(payload["execution_policy"]["paper_trade_required"], false);
    assert_eq!(
        payload["execution_policy"]["production_stage"],
        "signal_only"
    );
}
#[test]
fn live_execution_authorized_mode_marks_payload_as_live_allowed() {
    let config = MarketVelocityStrategySignalConfig {
        automation_mode: "live_execution_authorized".to_string(),
        live_order_allowed: true,
        paper_trade_required: false,
        ..MarketVelocityStrategySignalConfig::default()
    };
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&entry_confirmation()),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("live execution authorized mode should submit a strategy signal");
    };
    let payload: Value =
        serde_json::from_str(&request.payload_json).expect("payload should be valid json");
    assert_eq!(payload["auto_execution_allowed"], true);
    assert_eq!(
        payload["execution_policy"]["mode"],
        "live_execution_authorized"
    );
    assert_eq!(payload["execution_policy"]["live_order_allowed"], true);
    assert_eq!(payload["execution_policy"]["paper_trade_required"], false);
    assert_eq!(
        payload["execution_policy"]["production_stage"],
        "live_execution_allowed"
    );
    assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    assert_eq!(payload["entry_confirmation"]["trigger"], "reclaim_ema");
}
#[test]
fn market_velocity_strategy_signal_log_context_carries_chain_identifiers() {
    let config = MarketVelocityStrategySignalConfig {
        automation_mode: "live_execution_authorized".to_string(),
        live_order_allowed: true,
        paper_trade_required: false,
        ..MarketVelocityStrategySignalConfig::default()
    };
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&entry_confirmation()),
    )
    .expect("valid market velocity event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!("live execution authorized mode should submit a strategy signal");
    };
    let context = market_velocity_strategy_signal_log_context(&request);

    assert_eq!(context.source_signal_type, "market_velocity");
    assert_eq!(context.rank_event_id, event.id);
    assert_eq!(context.external_id, request.external_id);
    assert_eq!(context.exchange, "okx");
    assert_eq!(context.symbol, event.symbol);
    assert_eq!(
        context.entry_rule_version.as_deref(),
        Some(STABLE_PRODUCTION_ENTRY_RULE_VERSION)
    );
    assert_eq!(
        context.production_stage.as_deref(),
        Some("live_execution_allowed")
    );
}
#[test]
fn market_velocity_blocks_missing_technical_confirmation() {
    let config = MarketVelocityStrategySignalConfig::default();
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.technical_snapshot = None;
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::TechnicalConfirmationMissing
        )
    );
}
#[test]
fn market_velocity_signal_does_not_block_by_top_rank_chase_bucket() {
    let config = MarketVelocityStrategySignalConfig::default();
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.new_rank = Some(8);
    event.price_change_pct = Some(Decimal::new(750, 2));
    let decision = build_market_velocity_strategy_signal_request_with_entry_confirmation(
        &event,
        &config,
        Some(&entry_confirmation()),
    )
    .expect("event should be evaluated");
    let MarketVelocityStrategySignalDecision::Submit(request) = decision else {
        panic!(
            "new_rank-based chase bucket is diagnostic only and must not block entry: {decision:?}"
        );
    };
    assert_eq!(
        request.confidence,
        Some(0.79),
        "new_rank-based chase bucket must not add confidence bonus"
    );
}
#[test]
fn market_velocity_signal_blocks_above_max_price_change_pct() {
    let config = MarketVelocityStrategySignalConfig {
        max_price_change_pct: Some(10.0),
        ..MarketVelocityStrategySignalConfig::default()
    };
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.price_change_pct = Some(Decimal::new(1001, 2));
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::PriceChangeTooHigh
        )
    );
}
#[test]
fn market_velocity_signal_blocks_below_min_price_change_pct_with_weak_label() {
    let config = MarketVelocityStrategySignalConfig {
        min_price_change_pct: Some(5.0),
        ..MarketVelocityStrategySignalConfig::default()
    };
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.price_change_pct = Some(Decimal::new(499, 2));
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::PriceChangeTooLow
        )
    );
}
#[test]
fn market_velocity_blocks_missing_15m_entry_confirmation() {
    let config = MarketVelocityStrategySignalConfig::default();
    let event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    assert_eq!(
        build_market_velocity_strategy_signal_request(&event, &config)
            .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::EntryTimingMissing
        )
    );
}
#[test]
fn top_exit_or_down_price_event_does_not_build_strategy_signal() {
    let config = MarketVelocityStrategySignalConfig::default();
    let top_exit = rank_event(
        MarketRankEventType::TopExit,
        "down",
        Some(Decimal::new(3000, 0)),
    );
    let down = rank_event(
        MarketRankEventType::RankVelocity,
        "down",
        Some(Decimal::new(3000, 0)),
    );
    assert_eq!(
        build_market_velocity_strategy_signal_request(&top_exit, &config)
            .expect("top exit should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::UnsupportedEventType
        )
    );
    assert_eq!(
        build_market_velocity_strategy_signal_request(&down, &config)
            .expect("down price should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::PriceDirectionNotUp
        )
    );
}
#[test]
fn dispatch_gate_matches_strategy_signal_web_mode_contract() {
    assert!(
        should_dispatch_market_velocity_signal_to_quant_web_from_env(Some("web"), None, None, None,)
    );
    assert!(
        should_dispatch_market_velocity_signal_to_quant_web_from_env(
            None,
            None,
            Some("http://127.0.0.1:5557"),
            None,
        )
    );
    assert!(
        !should_dispatch_market_velocity_signal_to_quant_web_from_env(
            Some("disabled"),
            None,
            Some("http://127.0.0.1:5557"),
            None,
        )
    );
    assert!(
        should_dispatch_market_velocity_signal_to_quant_web_from_env(
            None,
            Some("execution_tasks"),
            None,
            None,
        )
    );
}
#[tokio::test]
#[ignore = "requires seeded rust_quan_web Market Velocity runtime fixture and a running Web backend"]
async fn market_velocity_synthetic_event_dispatches_to_running_quant_web() {
    std::env::set_var("MARKET_VELOCITY_SIGNAL_DISPATCH_MODE", "web");
    std::env::set_var("MARKET_VELOCITY_STRATEGY_SLUG", "market_velocity");
    std::env::set_var(
        "RUST_QUAN_WEB_BASE_URL",
        std::env::var("RUST_QUAN_WEB_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:8001".to_string()),
    );
    std::env::set_var(
        "EXECUTION_EVENT_SECRET",
        std::env::var("EXECUTION_EVENT_SECRET")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "local-dev-secret".to_string()),
    );
    let event_id = Utc::now().timestamp_micros();
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.id = Some(event_id);
    event.exchange = "binance".to_string();
    event.symbol = "ETHUSDT".to_string();
    event.detected_at = Utc::now();
    let confirmation = entry_confirmation();
    let response = dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled(
        &event,
        Some(&confirmation),
    )
    .await
    .expect("synthetic Market Velocity event should dispatch to running quant_web")
    .expect("dispatch should be enabled by test env");
    assert_eq!(
        response.inbox.external_id,
        format!("rust_quant:market_velocity:{event_id}")
    );
    assert_eq!(response.inbox.strategy_slug, "market_velocity");
    assert_eq!(response.inbox.symbol, "ETHUSDT");
    assert_eq!(response.generated_tasks.len(), 1);
    let task = response
        .generated_tasks
        .first()
        .expect("running Web backend should generate one task for runtime fixture");
    assert_eq!(task.symbol, "ETHUSDT");
    assert_eq!(task.task_type, "execute_signal");
    assert_eq!(task.task_status, "pending");
}
