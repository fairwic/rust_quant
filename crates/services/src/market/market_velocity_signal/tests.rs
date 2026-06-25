use super::{
    build_market_velocity_strategy_signal_request,
    build_market_velocity_strategy_signal_request_with_entry_confirmation,
    dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled,
    market_velocity_strategy_signal_log_context,
    should_dispatch_market_velocity_signal_to_quant_web_from_env, MarketVelocityEntryConfirmation,
    MarketVelocityStrategySignalBlocker, MarketVelocityStrategySignalConfig,
    MarketVelocityStrategySignalDecision,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use serde_json::{json, Value};
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
    entry_confirmation_with_trigger("breakout_previous_high")
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
    assert_eq!(request.external_id, "rust_quant:market_velocity:991");
    assert_eq!(request.strategy_slug, "market_velocity");
    assert_eq!(
        request.strategy_key,
        "market_velocity:ETH-USDT-SWAP:15m:991"
    );
    assert_eq!(request.symbol, "ETH-USDT-SWAP");
    assert_eq!(request.signal_type, "entry");
    assert_eq!(request.direction, "long");
    assert_eq!(request.confidence, Some(0.83));
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
    assert_eq!(payload["paper_strategy_preset"], "momentum_03sl_20r_v5");
    assert_eq!(
        payload["entry_rule_version"],
        "rank_radar_4h_trend_15m_momentum_03sl_20r_v5"
    );
    assert_eq!(payload["risk_plan"]["entry_price"], 3400.0);
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3298.0);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3604.0);
    assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.03);
    assert_eq!(payload["risk_plan"]["target_r"], 2.0);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
    assert_eq!(payload["risk_plan"]["reward_to_risk_mode"], "fixed_r");
    assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    assert_eq!(payload["entry_filter"]["status"], "confirmed");
    assert_eq!(
        payload["entry_filter"]["mode"],
        "rank_radar_4h_trend_15m_momentum"
    );
    assert_eq!(
        payload["entry_filter"]["entry_rule_version"],
        "rank_radar_4h_trend_15m_momentum_03sl_20r_v5"
    );
    assert_eq!(
        payload["entry_filter"]["paper_strategy_preset"],
        "momentum_03sl_20r_v5"
    );
    assert_eq!(payload["entry_filter"]["min_delta_rank"], 15);
    assert_eq!(payload["entry_filter"]["max_new_rank"], 30);
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
        json!(["breakout_previous_high", "reclaim_ema"])
    );
    assert_eq!(
        payload["entry_filter"]["entry_trigger_blocklist"],
        json!([])
    );
    assert_eq!(payload["entry_confirmation"]["timeframe"], "15m");
    assert_eq!(
        payload["entry_confirmation"]["trigger"],
        "breakout_previous_high"
    );
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
    assert_eq!(payload["signal"]["signal_kline_stop_loss_price"], 3298.0);
    assert_eq!(payload["signal"]["long_signal_take_profit_price"], 3604.0);
    assert_eq!(
        payload["signal"]["stop_loss_source"],
        "market_velocity_fixed_03sl"
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3298.0);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3604.0);
    assert_eq!(payload["risk_plan"]["target_r"], 2.0);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
}
#[test]
fn default_market_velocity_signal_payload_uses_momentum_profit_preset() {
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
    assert_eq!(config.stop_loss_pct, 0.03);
    assert_eq!(config.take_profit_r, 2.0);
    assert_eq!(config.max_holding_hours, 48);
    assert_eq!(config.automation_mode, "live_execution_authorized");
    assert!(config.live_order_allowed);
    assert!(!config.paper_trade_required);
    assert!(
        config.symbol_blocklist.is_empty(),
        "production default must not depend on historical symbol blocklist"
    );
    assert_eq!(config.entry_max_average_distance_pct, 4.0);
    assert_eq!(payload["paper_strategy_preset"], "momentum_03sl_20r_v5");
    assert_eq!(
        payload["entry_rule_version"],
        "rank_radar_4h_trend_15m_momentum_03sl_20r_v5"
    );
    assert_eq!(
        payload["entry_filter"]["mode"],
        "rank_radar_4h_trend_15m_momentum"
    );
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3298.0);
    assert_eq!(payload["risk_plan"]["selected_take_profit_price"], 3604.0);
    assert_eq!(payload["risk_plan"]["stop_loss_percent"], 0.03);
    assert_eq!(payload["risk_plan"]["target_r"], 2.0);
    assert_eq!(payload["risk_plan"]["max_holding_hours"], 48);
    assert_eq!(
        payload["entry_filter"]["entry_max_average_distance_pct"],
        4.0
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
            "max_new_rank": 35,
            "automation_mode": "signal_only",
            "live_order_allowed": false,
            "paper_trade_required": true,
            "require_technical_confirmation": true,
            "require_entry_confirmation": true,
            "chasing_risk_top_rank": 8,
            "chasing_risk_price_change_pct": 7.5,
            "trend_min_average_distance_pct": 0.2,
            "entry_confirmation_period": 18,
            "entry_confirmation_fetch_limit": 90,
            "entry_max_average_distance_pct": 3.6,
            "entry_min_volume_ratio": 1.15,
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
    let mut confirmation = entry_confirmation();
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
    assert_eq!(config.max_new_rank, 35);
    assert_eq!(config.entry_confirmation_period, 18);
    assert_eq!(config.entry_confirmation_fetch_limit, 90);
    assert_eq!(config.entry_max_average_distance_pct, 3.6);
    assert_eq!(config.entry_min_volume_ratio, 1.15);
    assert_eq!(config.automation_mode, "live_execution_authorized");
    assert!(config.live_order_allowed);
    assert!(!config.paper_trade_required);
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
    confirmation.ema_distance_pct = 4.01;
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
    let confirmation = entry_confirmation_with_trigger("reclaim_ma");
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
fn dry_run_execution_task_mode_marks_payload_stage_as_dry_run() {
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
    assert_eq!(payload["auto_execution_allowed"], true);
    assert_eq!(
        payload["execution_policy"]["mode"],
        "execution_task_dry_run"
    );
    assert_eq!(payload["execution_policy"]["live_order_allowed"], true);
    assert_eq!(payload["execution_policy"]["paper_trade_required"], false);
    assert_eq!(
        payload["execution_policy"]["production_stage"],
        "execution_task_dry_run"
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
    assert_eq!(
        payload["entry_confirmation"]["trigger"],
        "breakout_previous_high"
    );
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
        Some("rank_radar_4h_trend_15m_momentum_03sl_20r_v5")
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
fn market_velocity_blocks_chasing_top_rank_after_large_price_jump() {
    let config = MarketVelocityStrategySignalConfig::default();
    let mut event = rank_event(
        MarketRankEventType::RankVelocity,
        "up",
        Some(Decimal::new(3400, 0)),
    );
    event.new_rank = Some(8);
    event.price_change_pct = Some(Decimal::new(850, 2));
    assert_eq!(
        build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &config,
            Some(&entry_confirmation()),
        )
        .expect("event should be evaluated"),
        MarketVelocityStrategySignalDecision::Blocked(
            MarketVelocityStrategySignalBlocker::ChasingRisk
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
