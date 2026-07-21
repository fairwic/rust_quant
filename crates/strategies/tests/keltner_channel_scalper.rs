use std::str::FromStr;
use std::sync::Arc;

use rust_quant_domain::{SignalDirection, StrategyConfig, StrategyType, Timeframe};
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::framework::strategy_registry::{
    get_strategy_registry, register_strategy_on_demand,
};
use rust_quant_strategies::framework::strategy_trait::StrategyExecutor;
use rust_quant_strategies::implementations::{
    KeltnerChannelScalperAction, KeltnerChannelScalperBacktestTuning,
    KeltnerChannelScalperEntryMode, KeltnerChannelScalperSignalSnapshot,
    KeltnerChannelScalperStrategy, KeltnerChannelScalperStrategyExecutor,
    KeltnerChannelScalperThresholds,
};
use rust_quant_strategies::CandleItem;

fn short_snapshot() -> KeltnerChannelScalperSignalSnapshot {
    KeltnerChannelScalperSignalSnapshot {
        symbol: "BTC-USDT-SWAP".to_string(),
        timeframe: "1m".to_string(),
        price: 100.0,
        basis: 100.0,
        inner_upper: 104.0,
        inner_lower: 96.0,
        outer_upper: 106.0,
        outer_lower: 94.0,
        atr: 2.0,
        adx: 35.0,
        basis_slope_atr: -0.06,
        outer_upper_breached: true,
        outer_lower_breached: false,
        returned_inside_inner_upper: true,
        returned_inside_inner_lower: false,
        reentry_body_ratio: 0.62,
        rejection_wick_ratio: 0.48,
        reentry_close_progress_ratio: 0.72,
        breakout_reentry_candles: 0,
        bullish_momentum_break: false,
        bearish_momentum_break: false,
    }
}

fn long_snapshot() -> KeltnerChannelScalperSignalSnapshot {
    KeltnerChannelScalperSignalSnapshot {
        symbol: "ETH-USDT-SWAP".to_string(),
        timeframe: "1m".to_string(),
        price: 100.0,
        basis: 100.0,
        inner_upper: 104.0,
        inner_lower: 96.0,
        outer_upper: 106.0,
        outer_lower: 94.0,
        atr: 2.0,
        adx: 25.0,
        basis_slope_atr: 0.06,
        outer_upper_breached: false,
        outer_lower_breached: true,
        returned_inside_inner_upper: false,
        returned_inside_inner_lower: true,
        reentry_body_ratio: 0.58,
        rejection_wick_ratio: 0.52,
        reentry_close_progress_ratio: 0.74,
        breakout_reentry_candles: 0,
        bullish_momentum_break: false,
        bearish_momentum_break: false,
    }
}

#[test]
fn strategy_type_accepts_keltner_channel_scalper_research_key() {
    assert_eq!(
        StrategyType::from_str("keltner_channel_scalper_1m_v1_research"),
        Ok(StrategyType::KeltnerChannelScalper1mV1Research)
    );
    assert_eq!(
        StrategyType::KeltnerChannelScalper1mV1Research.as_str(),
        "keltner_channel_scalper_1m_v1_research"
    );
    assert!(StrategyType::from_str("keltner_channel_scalper").is_err());
}

#[test]
fn upper_outer_break_then_inner_reentry_with_high_adx_emits_short() {
    let thresholds = KeltnerChannelScalperThresholds {
        stop_atr_mult: 1.0,
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.0,
        ..Default::default()
    };
    let snapshot = short_snapshot();

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_000_000);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Short);
    assert!(decision.has_reason("KELTNER_UPPER_REENTRY_SHORT"));
    assert!(signal.should_sell);
    assert!(!signal.should_buy);
    assert_eq!(signal.direction, SignalDirection::Short);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(102.0));
    assert_eq!(signal.atr_take_profit_level_1, Some(98.0));
    assert_eq!(signal.atr_take_profit_level_2, Some(96.0));
    assert_eq!(signal.atr_take_profit_level_3, Some(94.0));
    assert_eq!(
        signal.stop_loss_source.as_deref(),
        Some("KeltnerChannelScalper")
    );
}

#[test]
fn lower_outer_break_then_inner_reentry_with_low_adx_emits_long() {
    let thresholds = KeltnerChannelScalperThresholds {
        stop_atr_mult: 1.0,
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.0,
        ..Default::default()
    };
    let snapshot = long_snapshot();

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_060_000);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Long);
    assert!(decision.has_reason("KELTNER_LOWER_REENTRY_LONG"));
    assert!(signal.should_buy);
    assert!(!signal.should_sell);
    assert_eq!(signal.direction, SignalDirection::Long);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(98.0));
    assert_eq!(signal.atr_take_profit_level_1, Some(102.0));
    assert_eq!(signal.atr_take_profit_level_2, Some(104.0));
    assert_eq!(signal.atr_take_profit_level_3, Some(106.0));
}

#[test]
fn five_and_fifteen_minute_research_snapshots_use_same_keltner_rules() {
    let thresholds = KeltnerChannelScalperThresholds::default();
    for timeframe in ["5m", "15m"] {
        let mut snapshot = long_snapshot();
        snapshot.timeframe = timeframe.to_string();

        let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

        assert_eq!(
            decision.action,
            KeltnerChannelScalperAction::Long,
            "timeframe {timeframe} should be accepted for research comparison"
        );
        assert!(decision.has_reason("KELTNER_LOWER_REENTRY_LONG"));
    }
}

#[test]
fn adx_thresholds_are_strict_for_both_directions() {
    let thresholds = KeltnerChannelScalperThresholds::default();
    let mut short = short_snapshot();
    short.adx = 30.0;
    let short_decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &short);
    assert_eq!(short_decision.action, KeltnerChannelScalperAction::Flat);
    assert!(short_decision.has_reason("ADX_NOT_ABOVE_SHORT_LEVEL"));

    let mut long = long_snapshot();
    long.adx = 30.0;
    let long_decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &long);
    assert_eq!(long_decision.action, KeltnerChannelScalperAction::Flat);
    assert!(long_decision.has_reason("ADX_NOT_BELOW_LONG_LEVEL"));
}

#[test]
fn tiny_reentry_body_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_reentry_body_ratio: 0.45,
        ..Default::default()
    };
    let mut snapshot = short_snapshot();
    snapshot.reentry_body_ratio = 0.12;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("REENTRY_BODY_TOO_SMALL"));
}

#[test]
fn weak_rejection_wick_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_rejection_wick_ratio: 0.35,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.rejection_wick_ratio = 0.12;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("REJECTION_WICK_TOO_SMALL"));
}

#[test]
fn weak_reentry_close_progress_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_reentry_close_progress_ratio: 0.65,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.reentry_close_progress_ratio = 0.42;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("REENTRY_CLOSE_PROGRESS_TOO_WEAK"));
}

#[test]
fn slow_breakout_reentry_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        max_breakout_reentry_candles: 1,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.breakout_reentry_candles = 2;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("BREAKOUT_REENTRY_TOO_SLOW"));
}

#[test]
fn very_low_long_adx_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_long_adx: 18.0,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.adx = 12.0;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("ADX_BELOW_LONG_MIN_LEVEL"));
}

#[test]
fn weak_inner_reclaim_distance_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_inner_reclaim_atr: 0.25,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.inner_lower = 99.8;
    snapshot.price = 100.0;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("INNER_RECLAIM_DISTANCE_TOO_SMALL"));
}

#[test]
fn overextended_inner_reclaim_distance_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        max_inner_reclaim_atr: 0.75,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.inner_lower = 98.0;
    snapshot.price = 100.0;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("INNER_RECLAIM_DISTANCE_TOO_LARGE"));
}

#[test]
fn basis_cross_confirmation_blocks_reentry_until_price_reaches_basis() {
    let thresholds = KeltnerChannelScalperThresholds {
        require_basis_cross: true,
        ..Default::default()
    };
    let mut long = long_snapshot();
    let mut short = short_snapshot();

    long.price = 98.0;
    short.price = 102.0;
    let long_blocked = KeltnerChannelScalperStrategy::evaluate(&thresholds, &long);
    let short_blocked = KeltnerChannelScalperStrategy::evaluate(&thresholds, &short);

    assert_eq!(long_blocked.action, KeltnerChannelScalperAction::Flat);
    assert!(long_blocked.has_reason("BASIS_NOT_CROSSED_FOR_LONG"));
    assert_eq!(short_blocked.action, KeltnerChannelScalperAction::Flat);
    assert!(short_blocked.has_reason("BASIS_NOT_CROSSED_FOR_SHORT"));

    long.price = 100.2;
    short.price = 99.8;
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate(&thresholds, &long).action,
        KeltnerChannelScalperAction::Long
    );
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate(&thresholds, &short).action,
        KeltnerChannelScalperAction::Short
    );
}

#[test]
fn low_atr_percent_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_atr_pct: 3.0,
        ..Default::default()
    };
    let snapshot = long_snapshot();

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("ATR_PCT_TOO_LOW"));
}

#[test]
fn oversized_reentry_body_is_filtered_before_entry() {
    let thresholds = KeltnerChannelScalperThresholds {
        max_reentry_body_ratio: 0.50,
        ..Default::default()
    };
    let mut snapshot = long_snapshot();
    snapshot.reentry_body_ratio = 0.72;

    let decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, KeltnerChannelScalperAction::Flat);
    assert!(decision.has_reason("REENTRY_BODY_TOO_LARGE"));
}

#[test]
fn basis_slope_filter_requires_trade_direction_to_match_trend() {
    let thresholds = KeltnerChannelScalperThresholds {
        min_basis_slope_atr: 0.05,
        ..Default::default()
    };
    let mut long = long_snapshot();
    long.basis_slope_atr = -0.08;
    let mut short = short_snapshot();
    short.basis_slope_atr = 0.08;

    let long_decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &long);
    let short_decision = KeltnerChannelScalperStrategy::evaluate(&thresholds, &short);

    assert_eq!(long_decision.action, KeltnerChannelScalperAction::Flat);
    assert!(long_decision.has_reason("BASIS_SLOPE_NOT_UP_FOR_LONG"));
    assert_eq!(short_decision.action, KeltnerChannelScalperAction::Flat);
    assert!(short_decision.has_reason("BASIS_SLOPE_NOT_DOWN_FOR_SHORT"));

    long.basis_slope_atr = 0.08;
    short.basis_slope_atr = -0.08;
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate(&thresholds, &long).action,
        KeltnerChannelScalperAction::Long
    );
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate(&thresholds, &short).action,
        KeltnerChannelScalperAction::Short
    );
}

#[test]
fn adverse_basis_slope_filter_only_rejects_extreme_counter_trend() {
    let thresholds = KeltnerChannelScalperThresholds {
        max_adverse_basis_slope_atr: 0.10,
        ..Default::default()
    };
    let mut long = long_snapshot();
    let mut short = short_snapshot();

    long.basis_slope_atr = -0.12;
    short.basis_slope_atr = 0.12;
    let long_blocked = KeltnerChannelScalperStrategy::evaluate(&thresholds, &long);
    let short_blocked = KeltnerChannelScalperStrategy::evaluate(&thresholds, &short);

    assert_eq!(long_blocked.action, KeltnerChannelScalperAction::Flat);
    assert!(long_blocked.has_reason("ADVERSE_BASIS_SLOPE_FOR_LONG"));
    assert_eq!(short_blocked.action, KeltnerChannelScalperAction::Flat);
    assert!(short_blocked.has_reason("ADVERSE_BASIS_SLOPE_FOR_SHORT"));

    long.basis_slope_atr = -0.08;
    short.basis_slope_atr = 0.08;
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate(&thresholds, &long).action,
        KeltnerChannelScalperAction::Long
    );
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate(&thresholds, &short).action,
        KeltnerChannelScalperAction::Short
    );
}

#[test]
fn next_candle_confirmation_tuning_defaults_off_and_can_be_enabled() {
    assert!(!KeltnerChannelScalperBacktestTuning::default().confirm_next_candle);
    assert_eq!(
        KeltnerChannelScalperBacktestTuning::default().entry_mode,
        KeltnerChannelScalperEntryMode::Reversal
    );

    let tuning = KeltnerChannelScalperBacktestTuning {
        confirm_next_candle: true,
        entry_mode: KeltnerChannelScalperEntryMode::Continuation,
        ..Default::default()
    };

    assert!(tuning.confirm_next_candle);
    assert_eq!(
        tuning.entry_mode,
        KeltnerChannelScalperEntryMode::Continuation
    );
}

#[test]
fn continuation_entry_mode_inverts_reentry_trade_direction() {
    let thresholds = KeltnerChannelScalperThresholds::default();
    let upper_reentry = short_snapshot();
    let lower_reentry = long_snapshot();

    let upper_decision = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
        &thresholds,
        &upper_reentry,
        KeltnerChannelScalperEntryMode::Continuation,
    );
    let lower_decision = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
        &thresholds,
        &lower_reentry,
        KeltnerChannelScalperEntryMode::Continuation,
    );

    assert_eq!(upper_decision.action, KeltnerChannelScalperAction::Long);
    assert!(upper_decision.has_reason("KELTNER_UPPER_REENTRY_CONTINUATION_LONG"));
    assert_eq!(lower_decision.action, KeltnerChannelScalperAction::Short);
    assert!(lower_decision.has_reason("KELTNER_LOWER_REENTRY_CONTINUATION_SHORT"));
}

#[test]
fn extreme_momentum_reversal_uses_keltner_as_background_not_reentry_trigger() {
    let thresholds = KeltnerChannelScalperThresholds::default();
    let mut long = long_snapshot();
    long.returned_inside_inner_lower = false;
    long.bullish_momentum_break = true;
    let mut short = short_snapshot();
    short.returned_inside_inner_upper = false;
    short.bearish_momentum_break = true;

    let long_decision = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
        &thresholds,
        &long,
        KeltnerChannelScalperEntryMode::ExtremeMomentumReversal,
    );
    let short_decision = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
        &thresholds,
        &short,
        KeltnerChannelScalperEntryMode::ExtremeMomentumReversal,
    );

    assert_eq!(long_decision.action, KeltnerChannelScalperAction::Long);
    assert!(long_decision.has_reason("KELTNER_LOWER_EXTREME_MOMENTUM_LONG"));
    assert_eq!(short_decision.action, KeltnerChannelScalperAction::Short);
    assert!(short_decision.has_reason("KELTNER_UPPER_EXTREME_MOMENTUM_SHORT"));

    long.bullish_momentum_break = false;
    assert_eq!(
        KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
            &thresholds,
            &long,
            KeltnerChannelScalperEntryMode::ExtremeMomentumReversal,
        )
        .action,
        KeltnerChannelScalperAction::Flat
    );
}

#[test]
fn precomputed_snapshot_backtest_matches_regular_backtest() {
    let candles = deterministic_volatile_candles(720);
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 1.0,
        ..BasicRiskStrategyConfig::default()
    };
    let tuning = KeltnerChannelScalperBacktestTuning {
        cooldown_candles: 6,
        reentry_lookback_candles: 3,
        allow_long: true,
        allow_short: false,
        confirm_next_candle: false,
        entry_mode: KeltnerChannelScalperEntryMode::Reversal,
        thresholds: KeltnerChannelScalperThresholds {
            stop_atr_mult: 2.0,
            min_inner_reclaim_atr: 0.15,
            min_long_adx: 20.0,
            ..KeltnerChannelScalperThresholds::default()
        },
    };

    let snapshots = KeltnerChannelScalperStrategy::precompute_backtest_snapshots(
        "BTC-USDT-SWAP",
        &candles,
        tuning,
    );
    assert!(snapshots.iter().any(Option::is_some));

    let regular =
        KeltnerChannelScalperStrategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning);
    let cached = KeltnerChannelScalperStrategy.run_test_with_precomputed_snapshots(
        "BTC-USDT-SWAP",
        &candles,
        risk,
        tuning,
        Arc::new(snapshots),
    );

    assert_eq!(regular.funds, cached.funds);
    assert_eq!(regular.trade_records.len(), cached.trade_records.len());
    assert_eq!(
        regular.filtered_signals.len(),
        cached.filtered_signals.len()
    );
    if let (Some(regular_trade), Some(cached_trade)) =
        (regular.trade_records.first(), cached.trade_records.first())
    {
        assert_eq!(regular_trade.option_type, cached_trade.option_type);
        assert_eq!(
            regular_trade.open_position_time,
            cached_trade.open_position_time
        );
        assert_eq!(
            regular_trade.close_position_time,
            cached_trade.close_position_time
        );
        assert_eq!(regular_trade.open_price, cached_trade.open_price);
        assert_eq!(regular_trade.close_price, cached_trade.close_price);
        assert_eq!(regular_trade.profit_loss, cached_trade.profit_loss);
    }
}

#[tokio::test]
async fn executor_requires_versioned_key_and_preserves_missing_snapshot_as_flat() {
    let executor = KeltnerChannelScalperStrategyExecutor::new();
    assert!(executor.can_handle(r#"{"strategy_key":"keltner_channel_scalper_1m_v1_research"}"#));
    assert!(!executor.can_handle(r#"{"strategy_key":"keltner_channel_scalper"}"#));

    register_strategy_on_demand(&StrategyType::KeltnerChannelScalper1mV1Research);
    let registry = get_strategy_registry();
    assert!(registry.contains("KeltnerChannelScalper1m"));
    assert_eq!(
        registry
            .get("keltner_channel_scalper_1m_v1_research")
            .unwrap()
            .strategy_type(),
        StrategyType::KeltnerChannelScalper1mV1Research
    );

    let config = StrategyConfig::new(
        42,
        StrategyType::KeltnerChannelScalper1mV1Research,
        "BTC-USDT-SWAP".to_string(),
        Timeframe::M1,
        serde_json::json!({"strategy_key": "keltner_channel_scalper_1m_v1_research"}),
        serde_json::json!({}),
    );
    let signal = executor
        .execute(
            "BTC-USDT-SWAP",
            "1m",
            &config,
            Some(CandleItem {
                o: 100.0,
                h: 101.0,
                l: 99.0,
                c: 100.0,
                v: 1_000.0,
                ts: 1_783_000_000_000,
                confirm: 1,
            }),
        )
        .await
        .expect("missing snapshot should be a flat signal");

    assert!(signal
        .filter_reasons
        .contains(&"MISSING_MARKET_SNAPSHOT".to_string()));
}

fn deterministic_volatile_candles(count: usize) -> Vec<CandleItem> {
    (0..count)
        .map(|index| {
            let base = 100.0 + (index as f64 / 8.0).sin() * 3.0;
            let shock = if index % 97 == 0 {
                -8.0
            } else if index % 89 == 0 {
                7.0
            } else {
                0.0
            };
            let close = base + shock;
            let open = base - shock * 0.25;
            let high = open.max(close) + 1.4 + (index % 5) as f64 * 0.15;
            let low = open.min(close) - 1.4 - (index % 7) as f64 * 0.12;
            CandleItem {
                o: open,
                h: high,
                l: low,
                c: close,
                v: 1_000.0 + index as f64,
                ts: 1_783_000_000_000 + index as i64 * 60_000,
                confirm: 1,
            }
        })
        .collect()
}
