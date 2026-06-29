use std::str::FromStr;

use rust_quant_domain::{SignalDirection, StrategyType};
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::framework::strategy_registry::get_strategy_registry;
use rust_quant_strategies::framework::strategy_trait::StrategyExecutor;
use rust_quant_strategies::implementations::{
    BearShortAction, BearShortPreset, BearShortSignalSnapshot, BearShortStackBacktestMarketContext,
    BearShortStackBacktestTuning, BearShortStackConfig, BearShortStackStrategy,
    BearShortStackStrategyExecutor, BtcEthLiquidityScalperAction,
    BtcEthLiquidityScalperBacktestMarketContext, BtcEthLiquidityScalperBacktestTuning,
    BtcEthLiquidityScalperConfig, BtcEthLiquidityScalperSignalSnapshot,
    BtcEthLiquidityScalperStrategy, BtcEthLiquidityScalperStrategyExecutor,
};
use rust_quant_strategies::CandleItem;

fn scalper_snapshot() -> BtcEthLiquidityScalperSignalSnapshot {
    BtcEthLiquidityScalperSignalSnapshot {
        exchange: "binance".to_string(),
        symbol: "BTC-USDT-SWAP".to_string(),
        price: 100_000.0,
        anchor_price: 99_700.0,
        atr_5m: 500.0,
        trend_4h: "long".to_string(),
        trend_1h: "long".to_string(),
        execution_bias: "long".to_string(),
        volume_impulse_confirmed: true,
        pullback_to_anchor: true,
        taker_aggression: 0.62,
        orderbook_imbalance: 0.58,
        oi_expansion_pct: 1.4,
        funding_rate: 0.00008,
        spread_bps: 1.2,
        depth_usd: 25_000_000.0,
        breakout_candle_atr: 0.9,
        ..Default::default()
    }
}

fn bear_breakdown_snapshot() -> BearShortSignalSnapshot {
    BearShortSignalSnapshot {
        exchange: "okx".to_string(),
        symbol: "ETH-USDT-SWAP".to_string(),
        price: 3_400.0,
        failed_reclaim_high: 3_470.0,
        atr_15m: 42.0,
        trend_4h: "short".to_string(),
        trend_1h: "lower_high".to_string(),
        breakdown_confirmed: true,
        failed_reclaim_confirmed: true,
        price_down_with_oi_up: true,
        oi_growth_pct: 2.1,
        funding_rate: 0.00003,
        long_short_ratio: 1.18,
        downside_extension_atr: 1.2,
        ..Default::default()
    }
}

fn exhaustion_snapshot() -> BearShortSignalSnapshot {
    BearShortSignalSnapshot {
        exchange: "binance".to_string(),
        symbol: "BTC-USDT-SWAP".to_string(),
        price: 102_000.0,
        failed_reclaim_high: 103_200.0,
        atr_15m: 720.0,
        preset: BearShortPreset::ExhaustionFade,
        new_high_failed: true,
        taker_flow_diverged: true,
        orderbook_imbalance_diverged: true,
        oi_growth_pct: 4.6,
        funding_rate: 0.00045,
        pullback_failed_below_vwap: true,
        ..Default::default()
    }
}

fn rising_candles(count: usize, start: f64) -> Vec<CandleItem> {
    (0..count)
        .map(|i| {
            let open = start + i as f64 * 2.0;
            let close = open + 1.2;
            CandleItem {
                o: open,
                h: close + 0.8,
                l: open - 0.8,
                c: close,
                v: 2_000.0 + i as f64,
                ts: 1_783_000_000_000 + i as i64 * 300_000,
                confirm: 1,
            }
        })
        .collect()
}

fn falling_candles(count: usize, start: f64) -> Vec<CandleItem> {
    (0..count)
        .map(|i| {
            let open = start - i as f64 * 2.0;
            let close = open - 1.2;
            CandleItem {
                o: open,
                h: open + 0.8,
                l: close - 0.8,
                c: close,
                v: 2_000.0 + i as f64,
                ts: 1_783_000_000_000 + i as i64 * 300_000,
                confirm: 1,
            }
        })
        .collect()
}

fn scalper_impulse_pullback_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = rising_candles(count, start);
    for (i, candle) in candles.iter_mut().enumerate() {
        if i == 520 {
            candle.o = start + 1_040.0;
            candle.c = candle.o + 120.0;
            candle.h = candle.c + 8.0;
            candle.l = candle.o - 8.0;
            candle.v *= 4.0;
        } else if i == 521 {
            candle.o = start + 1_160.0;
            candle.c = candle.o - 34.0;
            candle.h = candle.o + 10.0;
            candle.l = candle.c - 10.0;
            candle.v *= 1.7;
        } else if i == 522 {
            candle.o = start + 1_126.0;
            candle.c = candle.o + 18.0;
            candle.h = candle.c + 7.0;
            candle.l = candle.o - 11.0;
            candle.v *= 1.5;
        } else if i > 522 {
            let open = start + 1_144.0 + (i - 522) as f64 * 28.0;
            candle.o = open;
            candle.c = open + 18.0;
            candle.h = candle.c + 9.0;
            candle.l = candle.o - 9.0;
            candle.v *= 1.2;
        }
    }
    candles
}

/// Builds a 1m-style regime switch where only the shortened trend window should see the setup.
fn scalper_short_window_impulse_pullback_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = rising_candles(count, start);
    let high_regime_start = count.saturating_sub(48);
    let short_cycle_start = count.saturating_sub(34);
    for (i, candle) in candles.iter_mut().enumerate() {
        if i >= high_regime_start && i < short_cycle_start {
            candle.o = start + 5_000.0 - (i - high_regime_start) as f64 * 8.0;
            candle.c = candle.o - 3.0;
            candle.h = candle.o + 5.0;
            candle.l = candle.c - 5.0;
            candle.v = 2_000.0;
        } else if i >= short_cycle_start {
            let open = start + (i - short_cycle_start) as f64 * 5.0;
            candle.o = open;
            candle.c = open + 2.0;
            candle.h = candle.c + 2.0;
            candle.l = candle.o - 2.0;
            candle.v = 2_000.0;
        }
    }

    let impulse = count.saturating_sub(3);
    candles[impulse].o = start + 140.0;
    candles[impulse].c = start + 200.0;
    candles[impulse].h = start + 204.0;
    candles[impulse].l = start + 136.0;
    candles[impulse].v = 8_000.0;
    candles[impulse + 1].o = start + 200.0;
    candles[impulse + 1].c = start + 178.0;
    candles[impulse + 1].h = start + 204.0;
    candles[impulse + 1].l = start + 172.0;
    candles[impulse + 1].v = 3_200.0;
    candles[impulse + 2].o = start + 178.0;
    candles[impulse + 2].c = start + 210.0;
    candles[impulse + 2].h = start + 214.0;
    candles[impulse + 2].l = start + 172.0;
    candles[impulse + 2].v = 3_000.0;
    candles
}

fn bear_breakdown_failed_reclaim_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = falling_candles(count, start);
    let setup = count.saturating_sub(7);
    for (i, candle) in candles.iter_mut().enumerate() {
        if i == setup {
            candle.o = start - 1_110.0;
            candle.c = start - 1_170.0;
            candle.h = start - 1_100.0;
            candle.l = start - 1_180.0;
            candle.v *= 3.5;
        } else if i == setup + 1 {
            candle.o = start - 1_170.0;
            candle.c = start - 1_135.0;
            candle.h = start - 1_115.0;
            candle.l = start - 1_180.0;
            candle.v *= 1.6;
        } else if i == setup + 2 {
            candle.o = start - 1_135.0;
            candle.c = start - 1_142.0;
            candle.h = start - 1_122.0;
            candle.l = start - 1_154.0;
            candle.v *= 1.5;
        } else if i == setup + 3 {
            candle.o = start - 1_142.0;
            candle.c = start - 1_148.0;
            candle.h = start - 1_128.0;
            candle.l = start - 1_160.0;
            candle.v *= 1.5;
        } else if i == count - 1 {
            candle.o = start - 1_130.0;
            candle.c = start - 1_165.0;
            candle.h = start - 1_120.0;
            candle.l = start - 1_190.0;
            candle.v *= 6.0;
        } else if i >= setup + 4 {
            candle.o = start - 1_125.0;
            candle.c = start - 1_155.0;
            candle.h = start - 1_120.0;
            candle.l = start - 1_170.0;
            candle.v *= 2.4;
        }
    }
    candles
}

fn exhaustion_failed_retest_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = rising_candles(count, start);
    for (i, candle) in candles.iter_mut().enumerate() {
        if i == 520 {
            candle.o = start + 1_040.0;
            candle.c = candle.o + 130.0;
            candle.h = candle.c + 80.0;
            candle.l = candle.o - 8.0;
            candle.v *= 4.0;
        } else if i == 521 {
            candle.o = start + 1_170.0;
            candle.c = candle.o - 72.0;
            candle.h = candle.o + 18.0;
            candle.l = candle.c - 20.0;
            candle.v *= 2.0;
        } else if i == 522 {
            candle.o = start + 1_098.0;
            candle.c = candle.o - 34.0;
            candle.h = start + 1_180.0;
            candle.l = candle.c - 12.0;
            candle.v *= 1.8;
        } else if i > 522 {
            let open = start + 1_064.0 - (i - 522) as f64 * 26.0;
            candle.o = open;
            candle.c = open - 18.0;
            candle.h = candle.o + 8.0;
            candle.l = candle.c - 8.0;
            candle.v *= 1.2;
        }
    }
    candles
}

fn stale_exhaustion_failed_retest_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = exhaustion_failed_retest_candles(count, start);
    for candle in candles.iter_mut().skip(523) {
        candle.o = start + 1_070.0;
        candle.c = start + 1_060.0;
        candle.h = start + 1_090.0;
        candle.l = start + 1_040.0;
        candle.v = 2_000.0;
    }
    candles
}

#[test]
fn strategy_type_accepts_new_strategy_keys() {
    assert_eq!(
        StrategyType::from_str("btc_eth_liquidity_scalper_v1"),
        Ok(StrategyType::BtcEthLiquidityScalper)
    );
    assert_eq!(
        StrategyType::from_str("bear_short_stack_v1"),
        Ok(StrategyType::BearShortStack)
    );
    assert_eq!(
        StrategyType::BtcEthLiquidityScalper.as_str(),
        "btc_eth_liquidity_scalper_v1"
    );
    assert_eq!(StrategyType::BearShortStack.as_str(), "bear_short_stack_v1");
    assert!(StrategyType::from_str("btc_eth_liquidity_scalper").is_err());
    assert!(StrategyType::from_str("bear_short_stack").is_err());
}

#[test]
fn scalper_requires_btc_eth_binance_or_okx_and_emits_long_signal() {
    let config = BtcEthLiquidityScalperConfig::default();
    let snapshot = scalper_snapshot();
    let decision = BtcEthLiquidityScalperStrategy::evaluate(&config, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_000_000);

    assert_eq!(decision.action, BtcEthLiquidityScalperAction::Long);
    assert!(signal.should_buy);
    assert!(!signal.should_sell);
    assert_eq!(signal.direction, SignalDirection::Long);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(99_350.0));
    assert_eq!(signal.atr_take_profit_level_1, Some(100_520.0));
    assert_eq!(signal.atr_take_profit_level_2, Some(101_040.0));
    assert_eq!(signal.atr_take_profit_level_3, Some(101_040.0));
}

#[test]
fn scalper_blocks_hyperliquid_live_venue_for_v1() {
    let config = BtcEthLiquidityScalperConfig::default();
    let mut snapshot = scalper_snapshot();
    snapshot.exchange = "hyperliquid".to_string();

    let decision = BtcEthLiquidityScalperStrategy::evaluate(&config, &snapshot);

    assert_eq!(decision.action, BtcEthLiquidityScalperAction::Flat);
    assert!(decision.has_reason("EXCHANGE_NOT_LIVE_READY_V1"));
}

#[test]
fn bear_breakdown_emits_protected_short_signal() {
    let config = BearShortStackConfig::default();
    let snapshot = bear_breakdown_snapshot();
    let decision = BearShortStackStrategy::evaluate(&config, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_000_000);

    assert_eq!(decision.action, BearShortAction::Short);
    assert!(signal.should_sell);
    assert!(!signal.should_buy);
    assert_eq!(signal.direction, SignalDirection::Short);
    assert_eq!(signal.signal_kline_stop_loss_price, Some(3_484.7));
    assert_eq!(signal.atr_take_profit_level_1, Some(3_332.24));
    assert_eq!(signal.atr_take_profit_level_2, Some(3_264.48));
    assert_eq!(signal.atr_take_profit_level_3, Some(3_332.24));
}

#[test]
fn exhaustion_fade_uses_half_risk_short_signal() {
    let config = BearShortStackConfig::default();
    let snapshot = exhaustion_snapshot();
    let decision = BearShortStackStrategy::evaluate(&config, &snapshot);
    let signal = decision.to_signal(snapshot.price, 1_783_000_000_000);

    assert_eq!(decision.action, BearShortAction::Short);
    assert_eq!(decision.preset, BearShortPreset::ExhaustionFade);
    assert_eq!(signal.direction, SignalDirection::Short);
    assert_eq!(signal.atr_take_profit_level_3, Some(100_752.0));
    assert!(signal
        .dynamic_adjustments
        .contains(&"HALF_RISK".to_string()));
    assert!(signal
        .single_result
        .unwrap()
        .contains("\"preset\":\"exhaustion_fade_short_v1\""));
}

#[test]
fn bear_short_blocks_tail_end_when_funding_is_deeply_negative() {
    let config = BearShortStackConfig::default();
    let mut snapshot = bear_breakdown_snapshot();
    snapshot.funding_rate = -0.0007;

    let decision = BearShortStackStrategy::evaluate(&config, &snapshot);

    assert_eq!(decision.action, BearShortAction::Flat);
    assert!(decision.has_reason("FUNDING_ALREADY_DEEPLY_NEGATIVE"));
}

#[test]
fn bear_short_config_accepts_child_strategy_key_as_preset_alias() {
    let config: BearShortStackConfig =
        serde_json::from_str(r#"{"preset":"exhaustion_fade_short_v1","snapshot":{"price":1.0}}"#)
            .unwrap();

    assert_eq!(config.preset, BearShortPreset::ExhaustionFade);
}

#[test]
fn new_executors_are_registered_and_detect_strategy_keys() {
    let scalper = BtcEthLiquidityScalperStrategyExecutor::new();
    let bear = BearShortStackStrategyExecutor::new();

    assert!(scalper.can_handle(r#"{"strategy_key":"btc_eth_liquidity_scalper_v1"}"#));
    assert!(!scalper.can_handle(r#"{"strategy_key":"btc_eth_liquidity_scalper"}"#));
    assert!(bear.can_handle(r#"{"strategy_key":"bear_short_stack_v1"}"#));
    assert!(!bear.can_handle(r#"{"strategy_key":"bear_short_stack"}"#));
    assert!(bear.can_handle(r#"{"strategy_key":"bear_breakdown_short_v1"}"#));
    assert!(bear.can_handle(r#"{"strategy_key":"exhaustion_fade_short_v1"}"#));

    let registry = get_strategy_registry();
    assert!(registry.contains("BtcEthLiquidityScalper"));
    assert!(registry.contains("BearShortStack"));
    assert_eq!(
        registry
            .get("btc_eth_liquidity_scalper_v1")
            .unwrap()
            .strategy_type(),
        StrategyType::BtcEthLiquidityScalper
    );
    assert_eq!(
        registry.get("bear_short_stack_v1").unwrap().strategy_type(),
        StrategyType::BearShortStack
    );
}

#[test]
fn scalper_runs_existing_backtest_pipeline_for_btc_and_eth() {
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };
    let btc = BtcEthLiquidityScalperStrategy.run_test(
        "BTC-USDT-SWAP",
        &scalper_impulse_pullback_candles(560, 100_000.0),
        risk,
    );
    let eth = BtcEthLiquidityScalperStrategy.run_test(
        "ETH-USDT-SWAP",
        &scalper_impulse_pullback_candles(560, 3_000.0),
        risk,
    );

    assert!(btc.open_trades > 0);
    assert!(eth.open_trades > 0);
    assert!(btc.win_rate > 0.0);
    assert!(eth.win_rate > 0.0);
    assert!(btc.funds > 100.0);
    assert!(eth.funds > 100.0);
    assert!(!btc.trade_records.is_empty());
    assert!(!eth.trade_records.is_empty());
    assert!(!btc.audit_trail.signal_snapshots.is_empty());
    assert!(!eth.audit_trail.signal_snapshots.is_empty());
}

#[test]
fn scalper_backtest_ignores_smooth_trend_without_pullback_setup() {
    let result = BtcEthLiquidityScalperStrategy.run_test(
        "BTC-USDT-SWAP",
        &rising_candles(560, 100_000.0),
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn scalper_backtest_tuning_is_research_only_and_can_relax_impulse_volume() {
    let mut candles = scalper_impulse_pullback_candles(560, 100_000.0);
    candles[520].v = 3_000.0;

    let default_result = BtcEthLiquidityScalperStrategy.run_test(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let tuned_result = BtcEthLiquidityScalperStrategy.run_test_with_tuning(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        BtcEthLiquidityScalperBacktestTuning {
            impulse_min_volume_mult: 0.5,
            ..Default::default()
        },
    );

    assert_eq!(default_result.open_trades, 0);
    assert!(tuned_result.open_trades > 0);
}

#[test]
fn scalper_context_backtest_blocks_hot_funding_instead_of_using_placeholder_snapshot() {
    let candles = scalper_impulse_pullback_candles(560, 100_000.0);
    let baseline = BtcEthLiquidityScalperStrategy.run_test(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let context = candles
        .iter()
        .map(|candle| BtcEthLiquidityScalperBacktestMarketContext {
            ts: candle.ts,
            funding_rate: 0.001,
            oi_expansion_pct: 1.2,
            taker_buy_volume: 10.0,
            taker_sell_volume: 1.0,
            orderbook_imbalance: 0.0,
            spread_bps: 1.0,
            depth_usd: 25_000_000.0,
        })
        .collect();

    let context_result = BtcEthLiquidityScalperStrategy.run_test_with_tuning_and_context(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        BtcEthLiquidityScalperBacktestTuning::default(),
        context,
    );

    assert!(baseline.open_trades > 0);
    assert_eq!(context_result.open_trades, 0);
}

#[test]
fn scalper_backtest_tuning_can_require_oi_confirmation_when_size_scaling_is_unmodeled() {
    let candles = scalper_impulse_pullback_candles(560, 100_000.0);
    let context = candles
        .iter()
        .map(|candle| BtcEthLiquidityScalperBacktestMarketContext {
            ts: candle.ts,
            funding_rate: 0.00008,
            oi_expansion_pct: 0.0,
            taker_buy_volume: 10.0,
            taker_sell_volume: 1.0,
            orderbook_imbalance: 0.0,
            spread_bps: 1.0,
            depth_usd: 25_000_000.0,
        })
        .collect::<Vec<_>>();

    let default_result = BtcEthLiquidityScalperStrategy.run_test_with_tuning_and_context(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        BtcEthLiquidityScalperBacktestTuning::default(),
        context.clone(),
    );
    let strict_result = BtcEthLiquidityScalperStrategy.run_test_with_tuning_and_context(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        BtcEthLiquidityScalperBacktestTuning {
            require_oi_confirmation: true,
            ..Default::default()
        },
        context,
    );

    assert!(default_result.open_trades > 0);
    assert_eq!(strict_result.open_trades, 0);
}

#[test]
fn scalper_backtest_tuning_can_shorten_trend_windows_for_1m_frequency() {
    let candles = scalper_short_window_impulse_pullback_candles(560, 100_000.0);
    let default_result = BtcEthLiquidityScalperStrategy.run_test(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let tuned_result = BtcEthLiquidityScalperStrategy.run_test_with_tuning(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        BtcEthLiquidityScalperBacktestTuning {
            trend_fast_window: 13,
            trend_slow_window: 34,
            ..Default::default()
        },
    );

    assert_eq!(default_result.open_trades, 0);
    assert!(tuned_result.open_trades > 0);
}

#[test]
fn bear_breakdown_runs_existing_backtest_pipeline_as_short_strategy() {
    let result = BearShortStackStrategy::for_preset(BearShortPreset::BearBreakdown).run_test(
        "ETH-USDT-SWAP",
        &bear_breakdown_failed_reclaim_candles(560, 4_000.0),
        BasicRiskStrategyConfig::default(),
    );

    assert!(result.open_trades > 0);
    assert!(result
        .trade_records
        .iter()
        .any(|record| record.option_type == "short"));
    assert!(!result.audit_trail.signal_snapshots.is_empty());
}

#[test]
fn bear_breakdown_backtest_ignores_smooth_selloff_without_failed_reclaim() {
    let result = BearShortStackStrategy::for_preset(BearShortPreset::BearBreakdown).run_test(
        "ETH-USDT-SWAP",
        &falling_candles(560, 4_000.0),
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn bear_breakdown_backtest_tuning_can_relax_initial_breakdown_volume() {
    let mut candles = bear_breakdown_failed_reclaim_candles(560, 4_000.0);
    candles[553].v = 1.0;

    let default_result = BearShortStackStrategy::for_preset(BearShortPreset::BearBreakdown)
        .run_test(
            "ETH-USDT-SWAP",
            &candles,
            BasicRiskStrategyConfig::default(),
        );
    let tuned_result = BearShortStackStrategy::for_preset_with_tuning(
        BearShortPreset::BearBreakdown,
        BearShortStackBacktestTuning {
            breakdown_initial_volume_mult: 0.0,
            ..Default::default()
        },
    )
    .run_test(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(default_result.open_trades, 0);
    assert!(tuned_result.open_trades > 0);
}

#[test]
fn bear_breakdown_context_backtest_blocks_deeply_negative_funding() {
    let candles = bear_breakdown_failed_reclaim_candles(560, 4_000.0);
    let baseline = BearShortStackStrategy::for_preset(BearShortPreset::BearBreakdown).run_test(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let context = candles
        .iter()
        .map(|candle| BearShortStackBacktestMarketContext {
            ts: candle.ts,
            funding_rate: -0.001,
            oi_growth_pct: 2.1,
            long_short_ratio: 1.18,
            taker_buy_volume: 1.0,
            taker_sell_volume: 10.0,
        })
        .collect();

    let context_result =
        BearShortStackStrategy::for_preset_with_context(BearShortPreset::BearBreakdown, context)
            .run_test(
                "ETH-USDT-SWAP",
                &candles,
                BasicRiskStrategyConfig::default(),
            );

    assert!(baseline.open_trades > 0);
    assert_eq!(context_result.open_trades, 0);
}

#[test]
fn exhaustion_fade_runs_existing_backtest_pipeline_as_half_risk_short_strategy() {
    let result = BearShortStackStrategy::for_preset(BearShortPreset::ExhaustionFade).run_test(
        "BTC-USDT-SWAP",
        &exhaustion_failed_retest_candles(560, 100_000.0),
        BasicRiskStrategyConfig::default(),
    );

    assert!(result.open_trades > 0);
    assert!(result.win_rate > 0.0);
    assert!(result.funds > 100.0);
    assert!(result
        .dynamic_config_logs
        .iter()
        .any(|log| log.adjustments.contains(&"HALF_RISK".to_string())));
    assert!(result
        .trade_records
        .iter()
        .any(|record| record.option_type == "short"));
}

#[test]
fn exhaustion_fade_backtest_closes_stale_short_before_sample_end() {
    let result = BearShortStackStrategy::for_preset_with_tuning(
        BearShortPreset::ExhaustionFade,
        BearShortStackBacktestTuning {
            exhaustion_max_holding_candles: 8,
            ..Default::default()
        },
    )
    .run_test(
        "BTC-USDT-SWAP",
        &stale_exhaustion_failed_retest_candles(560, 100_000.0),
        BasicRiskStrategyConfig::default(),
    );

    assert!(result.open_trades > 0);
    assert!(result
        .trade_records
        .iter()
        .any(|record| record.close_type != "结束平仓"));
}

#[test]
fn exhaustion_fade_context_backtest_accepts_okx_warm_funding_and_daily_oi_unwind() {
    let candles = exhaustion_failed_retest_candles(560, 100_000.0);
    let context = candles
        .iter()
        .map(|candle| BearShortStackBacktestMarketContext {
            ts: candle.ts,
            funding_rate: 0.00004,
            oi_growth_pct: -4.0,
            long_short_ratio: 1.2,
            taker_buy_volume: 1.0,
            taker_sell_volume: 10.0,
        })
        .collect();

    let result =
        BearShortStackStrategy::for_preset_with_context(BearShortPreset::ExhaustionFade, context)
            .run_test(
                "BTC-USDT-SWAP",
                &candles,
                BasicRiskStrategyConfig::default(),
            );

    assert!(result.open_trades > 0);
}

#[test]
fn exhaustion_fade_backtest_ignores_rally_without_failed_retest() {
    let result = BearShortStackStrategy::for_preset(BearShortPreset::ExhaustionFade).run_test(
        "BTC-USDT-SWAP",
        &rising_candles(560, 100_000.0),
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn exhaustion_fade_backtest_requires_volume_on_failed_retest() {
    let mut candles = exhaustion_failed_retest_candles(560, 100_000.0);
    for candle in candles.iter_mut().skip(521) {
        candle.v = 1.0;
    }

    let result = BearShortStackStrategy::for_preset(BearShortPreset::ExhaustionFade).run_test(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn bear_short_backtest_tuning_is_research_only_and_can_relax_exhaustion_volume() {
    let mut candles = exhaustion_failed_retest_candles(560, 100_000.0);
    for candle in candles.iter_mut().skip(521) {
        candle.v = 2_400.0;
    }

    let default_result = BearShortStackStrategy::for_preset(BearShortPreset::ExhaustionFade)
        .run_test(
            "BTC-USDT-SWAP",
            &candles,
            BasicRiskStrategyConfig::default(),
        );
    let tuned_result = BearShortStackStrategy::for_preset_with_tuning(
        BearShortPreset::ExhaustionFade,
        BearShortStackBacktestTuning {
            exhaustion_new_high_range_mult: 0.5,
            exhaustion_min_body_ratio: 0.1,
            exhaustion_min_volume_mult: 0.1,
            ..Default::default()
        },
    )
    .run_test(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(default_result.open_trades, 0);
    assert!(tuned_result.open_trades > 0);
}
