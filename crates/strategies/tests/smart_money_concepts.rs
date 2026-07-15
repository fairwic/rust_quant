use std::str::FromStr;

use rust_quant_domain::{SignalDirection, StrategyType};
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::{
    SmartMoneyConceptsAction, SmartMoneyConceptsBacktestTuning, SmartMoneyConceptsEvent,
    SmartMoneyConceptsSignalSnapshot, SmartMoneyConceptsStrategy, SmartMoneyConceptsThresholds,
};
use rust_quant_strategies::CandleItem;

fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> CandleItem {
    CandleItem {
        o: open,
        h: high,
        l: low,
        c: close,
        v: 2_000.0 + index as f64 * 100.0,
        ts: 1_783_000_000_000 + index as i64 * 300_000,
        confirm: 1,
    }
}

fn bullish_structure_break_candles() -> Vec<CandleItem> {
    let mut candles = (0..501)
        .map(|i| candle(i, 100.0, 100.8, 99.2, 100.0))
        .collect::<Vec<_>>();
    let setup = vec![
        candle(0, 100.0, 101.0, 99.0, 100.0),
        candle(1, 100.0, 100.5, 97.5, 98.0),
        candle(2, 98.0, 98.5, 95.0, 96.0),
        candle(3, 96.0, 98.0, 95.5, 97.5),
        candle(4, 97.5, 100.0, 97.0, 99.0),
        candle(5, 99.0, 103.0, 98.5, 102.0),
        candle(6, 102.0, 102.4, 100.0, 100.8),
        candle(7, 100.8, 101.4, 99.2, 100.0),
        candle(8, 100.0, 101.0, 98.7, 99.5),
        candle(9, 99.5, 102.2, 99.0, 101.8),
        candle(10, 101.8, 104.4, 101.4, 104.0),
    ];
    let base_index = candles.len();
    for (offset, mut item) in setup.into_iter().enumerate() {
        let index = base_index + offset;
        item.ts = 1_783_000_000_000 + index as i64 * 300_000;
        candles.push(item);
    }
    candles
}

fn bullish_break_then_retest_candles() -> Vec<CandleItem> {
    let mut candles = bullish_structure_break_candles();
    let index = candles.len();
    candles.push(candle(index, 104.0, 104.6, 98.8, 101.0));
    candles
}

fn bullish_liquidity_sweep_candles() -> Vec<CandleItem> {
    let mut candles = (0..501)
        .map(|i| candle(i, 100.0, 100.8, 99.2, 100.0))
        .collect::<Vec<_>>();
    let setup = vec![
        candle(0, 100.0, 102.0, 99.0, 101.0),
        candle(1, 101.0, 101.4, 97.0, 98.0),
        candle(2, 98.0, 99.0, 95.0, 96.0),
        candle(3, 96.0, 98.5, 95.8, 98.0),
        candle(4, 98.0, 100.5, 97.5, 100.0),
        candle(5, 100.0, 102.5, 99.5, 102.0),
        candle(6, 102.0, 102.2, 100.8, 101.2),
        candle(7, 101.2, 101.6, 99.8, 100.6),
        candle(8, 100.6, 101.0, 94.6, 100.8),
    ];
    let base_index = candles.len();
    for (offset, mut item) in setup.into_iter().enumerate() {
        let index = base_index + offset;
        item.ts = 1_783_000_000_000 + index as i64 * 300_000;
        candles.push(item);
    }
    candles
}

fn bullish_fair_value_gap_candles() -> Vec<CandleItem> {
    let mut candles = (0..501)
        .map(|i| candle(i, 100.0, 100.8, 99.2, 100.0))
        .collect::<Vec<_>>();
    let setup = vec![
        candle(0, 100.0, 101.0, 99.0, 100.2),
        candle(1, 100.2, 102.4, 100.6, 101.8),
        candle(2, 101.8, 103.0, 101.3, 102.6),
    ];
    let base_index = candles.len();
    for (offset, mut item) in setup.into_iter().enumerate() {
        let index = base_index + offset;
        item.ts = 1_783_000_000_000 + index as i64 * 300_000;
        candles.push(item);
    }
    candles
}

#[test]
fn strategy_type_accepts_smart_money_concepts_research_key() {
    assert_eq!(
        StrategyType::from_str("smart_money_concepts_v1_research"),
        Ok(StrategyType::SmartMoneyConceptsV1Research)
    );
    assert_eq!(
        StrategyType::SmartMoneyConceptsV1Research.as_str(),
        "smart_money_concepts_v1_research"
    );
    assert!(StrategyType::from_str("smart_money_concepts").is_err());
}

#[test]
fn bullish_choch_snapshot_emits_protected_long_signal() {
    let thresholds = SmartMoneyConceptsThresholds {
        stop_atr_buffer: 0.2,
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.0,
        ..Default::default()
    };
    let snapshot = SmartMoneyConceptsSignalSnapshot {
        symbol: "ETH-USDT-SWAP".to_string(),
        price: 104.0,
        atr: 2.0,
        event: SmartMoneyConceptsEvent::BullishChoch,
        break_level: 103.0,
        protected_low: Some(95.0),
        protected_high: Some(103.0),
        order_block_low: Some(98.5),
        order_block_high: Some(103.0),
        entry_extension_atr: 0.5,
        retest_distance_atr: 0.25,
        trend_bias: "long".to_string(),
        trend_strength_pct: 0.2,
        displacement_body_atr: 0.8,
        range_position_pct: Some(35.0),
    };

    let decision = SmartMoneyConceptsStrategy::evaluate(&thresholds, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_000_000);

    assert_eq!(decision.action, SmartMoneyConceptsAction::Long);
    assert!(decision.has_reason("SMART_MONEY_BULLISH_CHOCH"));
    assert!(signal.should_buy);
    assert!(!signal.should_sell);
    assert_eq!(signal.direction, SignalDirection::Long);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(94.6));
    assert_eq!(signal.atr_take_profit_level_1, Some(113.4));
    assert_eq!(signal.atr_take_profit_level_2, Some(122.8));
    assert_eq!(signal.atr_take_profit_level_3, Some(132.2));
    assert_eq!(
        signal.stop_loss_source.as_deref(),
        Some("SmartMoneyStructure")
    );
}

#[test]
fn trend_alignment_can_block_countertrend_structure_breaks() {
    let thresholds = SmartMoneyConceptsThresholds {
        require_trend_alignment: true,
        min_trend_strength_pct: 0.05,
        ..Default::default()
    };
    let snapshot = SmartMoneyConceptsSignalSnapshot {
        symbol: "BTC-USDT-SWAP".to_string(),
        price: 100.0,
        atr: 1.5,
        event: SmartMoneyConceptsEvent::BullishBos,
        break_level: 99.0,
        protected_low: Some(96.0),
        order_block_low: Some(97.0),
        order_block_high: Some(99.0),
        trend_bias: "short".to_string(),
        trend_strength_pct: 0.12,
        entry_extension_atr: 0.3,
        retest_distance_atr: 0.2,
        ..Default::default()
    };

    let decision = SmartMoneyConceptsStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, SmartMoneyConceptsAction::Flat);
    assert!(decision.has_reason("TREND_NOT_ALIGNED"));
}

#[test]
fn volatility_filter_can_block_high_atr_structure_breaks() {
    let thresholds = SmartMoneyConceptsThresholds {
        max_atr_pct: 1.0,
        ..Default::default()
    };
    let snapshot = SmartMoneyConceptsSignalSnapshot {
        symbol: "SOL-USDT-SWAP".to_string(),
        price: 100.0,
        atr: 5.0,
        event: SmartMoneyConceptsEvent::BullishBos,
        break_level: 99.0,
        protected_low: Some(94.0),
        order_block_low: Some(95.0),
        order_block_high: Some(99.0),
        trend_bias: "long".to_string(),
        trend_strength_pct: 0.3,
        entry_extension_atr: 0.2,
        retest_distance_atr: 0.1,
        ..Default::default()
    };

    let decision = SmartMoneyConceptsStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, SmartMoneyConceptsAction::Flat);
    assert!(decision.has_reason("VOLATILITY_TOO_HIGH"));
}

#[test]
fn displacement_filter_can_block_weak_structure_breaks() {
    let thresholds = SmartMoneyConceptsThresholds {
        min_displacement_body_atr: 0.6,
        ..Default::default()
    };
    let snapshot = SmartMoneyConceptsSignalSnapshot {
        symbol: "ETH-USDT-SWAP".to_string(),
        price: 100.0,
        atr: 2.0,
        event: SmartMoneyConceptsEvent::BullishBos,
        break_level: 99.0,
        protected_low: Some(95.0),
        order_block_low: Some(96.0),
        order_block_high: Some(99.0),
        trend_bias: "long".to_string(),
        trend_strength_pct: 0.3,
        entry_extension_atr: 0.2,
        retest_distance_atr: 0.1,
        displacement_body_atr: 0.25,
        ..Default::default()
    };

    let decision = SmartMoneyConceptsStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, SmartMoneyConceptsAction::Flat);
    assert!(decision.has_reason("DISPLACEMENT_BODY_TOO_WEAK"));
}

#[test]
fn premium_discount_filter_blocks_bullish_entry_above_equilibrium() {
    let thresholds = SmartMoneyConceptsThresholds {
        require_premium_discount_zone: true,
        ..Default::default()
    };
    let snapshot = SmartMoneyConceptsSignalSnapshot {
        symbol: "ETH-USDT-SWAP".to_string(),
        price: 104.0,
        atr: 2.0,
        event: SmartMoneyConceptsEvent::BullishBos,
        break_level: 103.0,
        protected_low: Some(96.0),
        order_block_low: Some(98.0),
        order_block_high: Some(103.0),
        trend_bias: "long".to_string(),
        trend_strength_pct: 0.3,
        entry_extension_atr: 0.2,
        retest_distance_atr: 0.1,
        range_position_pct: Some(72.0),
        ..Default::default()
    };

    let decision = SmartMoneyConceptsStrategy::evaluate(&thresholds, &snapshot);

    assert_eq!(decision.action, SmartMoneyConceptsAction::Flat);
    assert!(decision.has_reason("NOT_IN_DISCOUNT_ZONE"));
}

#[test]
fn bullish_liquidity_sweep_snapshot_emits_protected_long_signal() {
    let thresholds = SmartMoneyConceptsThresholds {
        stop_atr_buffer: 0.2,
        target_r_1: 0.6,
        target_r_2: 1.2,
        target_r_3: 1.8,
        ..Default::default()
    };
    let snapshot = SmartMoneyConceptsSignalSnapshot {
        symbol: "BTC-USDT-SWAP".to_string(),
        price: 100.8,
        atr: 1.5,
        event: SmartMoneyConceptsEvent::BullishLiquiditySweep,
        break_level: 100.0,
        protected_low: Some(98.6),
        protected_high: Some(103.0),
        order_block_low: Some(98.6),
        order_block_high: Some(101.2),
        trend_bias: "long".to_string(),
        trend_strength_pct: 0.3,
        entry_extension_atr: 0.53,
        retest_distance_atr: 0.0,
        ..Default::default()
    };

    let decision = SmartMoneyConceptsStrategy::evaluate(&thresholds, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_000_000);

    assert_eq!(decision.action, SmartMoneyConceptsAction::Long);
    assert!(decision.has_reason("SMART_MONEY_BULLISH_LIQUIDITY_SWEEP"));
    assert!(signal.should_buy);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(98.3));
}

#[test]
fn backtest_waits_for_confirmed_pivot_before_breaking_structure() {
    let candles = bullish_structure_break_candles();
    let tuning = SmartMoneyConceptsBacktestTuning {
        pivot_confirmation_bars: 3,
        cooldown_candles: 0,
        ..Default::default()
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let early = SmartMoneyConceptsStrategy.run_test_with_tuning(
        "ETH-USDT-SWAP",
        &candles[..506],
        risk,
        tuning,
    );
    let full =
        SmartMoneyConceptsStrategy.run_test_with_tuning("ETH-USDT-SWAP", &candles, risk, tuning);

    assert_eq!(early.open_trades, 0);
    assert!(full.open_trades > 0);
    assert!(full.audit_trail.signal_snapshots.iter().any(|snapshot| {
        snapshot
            .filter_reasons
            .contains(&"SMART_MONEY_BULLISH_CHOCH".to_string())
    }));
}

#[test]
fn backtest_can_enter_after_delayed_order_block_retest() {
    let candles = bullish_break_then_retest_candles();
    let tuning = SmartMoneyConceptsBacktestTuning {
        pivot_confirmation_bars: 3,
        cooldown_candles: 0,
        retest_max_wait_candles: 4,
        thresholds: SmartMoneyConceptsThresholds {
            require_retest: true,
            max_retest_distance_atr: 0.05,
            ..Default::default()
        },
        ..Default::default()
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let before_retest = SmartMoneyConceptsStrategy.run_test_with_tuning(
        "ETH-USDT-SWAP",
        &candles[..512],
        risk,
        tuning,
    );
    let after_retest =
        SmartMoneyConceptsStrategy.run_test_with_tuning("ETH-USDT-SWAP", &candles, risk, tuning);

    assert_eq!(before_retest.open_trades, 0);
    assert!(after_retest.open_trades > 0);
    assert!(after_retest
        .audit_trail
        .signal_snapshots
        .iter()
        .any(|snapshot| {
            snapshot
                .filter_reasons
                .contains(&"SMART_MONEY_BULLISH_CHOCH".to_string())
        }));
}

#[test]
fn backtest_can_enter_on_breakout_bar_when_close_is_inside_retest_threshold() {
    let candles = bullish_structure_break_candles();
    let tuning = SmartMoneyConceptsBacktestTuning {
        pivot_confirmation_bars: 3,
        cooldown_candles: 0,
        thresholds: SmartMoneyConceptsThresholds {
            require_retest: true,
            max_retest_distance_atr: 3.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result =
        SmartMoneyConceptsStrategy.run_test_with_tuning("ETH-USDT-SWAP", &candles, risk, tuning);

    assert!(result.open_trades > 0);
    assert!(result.audit_trail.signal_snapshots.iter().any(|snapshot| {
        snapshot
            .filter_reasons
            .contains(&"SMART_MONEY_BULLISH_CHOCH".to_string())
    }));
}

#[test]
fn backtest_can_enter_on_confirmed_liquidity_sweep() {
    let candles = bullish_liquidity_sweep_candles();
    let tuning = SmartMoneyConceptsBacktestTuning {
        pivot_confirmation_bars: 3,
        cooldown_candles: 0,
        enable_liquidity_sweep: true,
        thresholds: SmartMoneyConceptsThresholds {
            require_trend_alignment: false,
            max_entry_extension_atr: 6.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result =
        SmartMoneyConceptsStrategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning);

    assert!(result.open_trades > 0);
    assert!(result.audit_trail.signal_snapshots.iter().any(|snapshot| {
        snapshot
            .filter_reasons
            .contains(&"SMART_MONEY_BULLISH_LIQUIDITY_SWEEP".to_string())
    }));
}

#[test]
fn backtest_can_enter_on_confirmed_bullish_fair_value_gap() {
    let candles = bullish_fair_value_gap_candles();
    let tuning = SmartMoneyConceptsBacktestTuning {
        pivot_confirmation_bars: 3,
        cooldown_candles: 0,
        enable_fair_value_gap: true,
        thresholds: SmartMoneyConceptsThresholds {
            require_trend_alignment: false,
            max_entry_extension_atr: 6.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result =
        SmartMoneyConceptsStrategy.run_test_with_tuning("ETH-USDT-SWAP", &candles, risk, tuning);

    assert!(result.open_trades > 0);
    assert!(result.audit_trail.signal_snapshots.iter().any(|snapshot| {
        snapshot
            .filter_reasons
            .contains(&"SMART_MONEY_BULLISH_FVG".to_string())
    }));
}

#[test]
fn backtest_can_fade_bullish_fair_value_gap_into_short_signal() {
    let candles = bullish_fair_value_gap_candles();
    let tuning = SmartMoneyConceptsBacktestTuning {
        pivot_confirmation_bars: 3,
        cooldown_candles: 0,
        allow_short: true,
        enable_fair_value_gap: true,
        fade_signal: true,
        thresholds: SmartMoneyConceptsThresholds {
            require_trend_alignment: false,
            max_entry_extension_atr: 6.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result =
        SmartMoneyConceptsStrategy.run_test_with_tuning("ETH-USDT-SWAP", &candles, risk, tuning);

    assert!(result.open_trades > 0);
    assert!(result.audit_trail.signal_snapshots.iter().any(|snapshot| {
        snapshot
            .filter_reasons
            .contains(&"SMART_MONEY_BEARISH_FVG".to_string())
    }));
}
