use super::directional_reversal::{
    benchmark_abs_net_move_pct_before_entry, benchmark_directional_net_move_pct_before_entry,
    deferred_long_confirmation_entry_idx, deferred_short_confirmation_entry_idx,
    exhaustion_volume_dominance_filter_reason, is_bearish_continuation_setup,
    is_bullish_continuation_setup, opposite_net_move_filter_reason,
    opposite_reversal_confirmation_filter_reason, reversal_average_reclaim_filter_reason,
    volume_atr_target_r, volume_atr_target_r_with_policy,
};
use super::equity::framework_trade_cost_rate;
use super::*;

fn candle(ts: i64, open: f64, close: f64, volume: f64) -> BacktestCandle {
    BacktestCandle {
        ts,
        open,
        high: open.max(close) + 1.0,
        low: open.min(close) - 1.0,
        close,
        volume,
    }
}

fn net_down_history_with_small_bounces() -> Vec<BacktestCandle> {
    let moves = [
        (100.0, 98.0),
        (98.0, 96.0),
        (96.0, 97.0),
        (97.0, 95.0),
        (95.0, 93.0),
        (93.0, 94.0),
        (94.0, 92.0),
        (92.0, 91.0),
        (91.0, 92.0),
        (92.0, 90.0),
        (90.0, 91.0),
        (91.0, 89.0),
        (89.0, 94.0),
    ];
    moves
        .into_iter()
        .enumerate()
        .map(|(idx, (open, close))| candle(idx as i64 * MS_15M, open, close, 10.0))
        .collect()
}

#[test]
fn btc_regime_move_uses_only_candles_completed_before_entry() {
    let mut candles = (0..98)
        .map(|idx| {
            let close = 100.0 + idx as f64 / 95.0;
            candle(idx * MS_15M, close, close, 10.0)
        })
        .collect::<Vec<_>>();
    candles[96].close = 120.0;

    let before_jump = benchmark_abs_net_move_pct_before_entry(&candles, 96 * MS_15M, 96)
        .expect("96 completed BTC candles");
    let after_jump = benchmark_abs_net_move_pct_before_entry(&candles, 97 * MS_15M, 96)
        .expect("jump candle completed");

    assert!(before_jump < 2.0);
    assert!(after_jump > 10.0);
}

#[test]
fn btc_broad_direction_is_symmetric_and_uses_only_completed_candles() {
    let mut candles = (0..386)
        .map(|idx| {
            let close = 100.0 + idx as f64 * 0.05;
            candle(idx * MS_15M, close, close, 10.0)
        })
        .collect::<Vec<_>>();
    candles[384].close = 70.0;
    let entry_ts = 384 * MS_15M;

    let long = benchmark_directional_net_move_pct_before_entry(
        &candles,
        entry_ts,
        384,
        MarketVelocityTradeDirection::Long,
    )
    .expect("384 completed BTC candles");
    let short = benchmark_directional_net_move_pct_before_entry(
        &candles,
        entry_ts,
        384,
        MarketVelocityTradeDirection::Short,
    )
    .expect("384 completed BTC candles");

    assert!(long > 0.0);
    assert_eq!(short, -long);
    assert!(benchmark_directional_net_move_pct_before_entry(
        &candles,
        entry_ts - MS_15M,
        384,
        MarketVelocityTradeDirection::Long,
    )
    .is_none());
    assert!(benchmark_directional_net_move_pct_before_entry(
        &candles,
        entry_ts,
        384,
        MarketVelocityTradeDirection::Both,
    )
    .is_none());
}

fn long_slow_duration_history_with_trigger() -> Vec<BacktestCandle> {
    let mut candles = (0..192)
        .map(|idx| {
            let open = 100.0 - idx as f64 * 0.02;
            candle(idx * MS_15M, open, open - 0.01, 10.0)
        })
        .collect::<Vec<_>>();
    candles.push(candle(192 * MS_15M, 96.16, 97.2, 20.0));
    candles
}

fn exhaustion_volume_history(
    direction: MarketVelocityTradeDirection,
    current_cluster_volume: f64,
) -> Vec<BacktestCandle> {
    let mut candles = (0..100)
        .map(|idx| candle(idx as i64 * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    match direction {
        MarketVelocityTradeDirection::Long => candles[50].low = 90.0,
        MarketVelocityTradeDirection::Short => candles[50].high = 110.0,
        MarketVelocityTradeDirection::Both => {}
    }
    candles[50].volume = 1_000.0;
    candles[99].volume = current_cluster_volume;
    candles
}

fn reversal_entry_args() -> MarketVelocityEventBacktestArgs {
    MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.5,
        entry_min_body_ratio_pct: Some(60.0),
        entry_min_close_position_pct: Some(75.0),
        entry_min_range_expansion_ratio: Some(1.5),
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        ..MarketVelocityEventBacktestArgs::default()
    }
}

fn deferred_long_candles() -> Vec<BacktestCandle> {
    let mut candles = (0..14)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    let history = [
        (100.0, 98.5),
        (98.5, 97.0),
        (97.0, 97.8),
        (97.8, 95.8),
        (95.8, 94.0),
        (94.0, 94.8),
        (94.8, 92.8),
        (92.8, 91.5),
        (91.5, 92.0),
        (92.0, 90.5),
        (90.5, 91.0),
        (91.0, 89.0),
    ];
    candles.extend(
        history
            .into_iter()
            .enumerate()
            .map(|(offset, (open, close))| {
                candle((14 + offset as i64) * MS_15M, open, close, 10.0)
            }),
    );
    candles.push(BacktestCandle {
        ts: 26 * MS_15M,
        open: 100.0,
        high: 100.8,
        low: 95.5,
        close: 96.0,
        volume: 20.0,
    });
    candles.push(BacktestCandle {
        ts: 27 * MS_15M,
        open: 96.0,
        high: 101.5,
        low: 95.8,
        close: 101.2,
        volume: 12.0,
    });
    candles.push(candle(28 * MS_15M, 101.3, 102.0, 10.0));
    candles
}

#[test]
fn long_accepts_ten_percent_net_drop_even_with_small_bounces() {
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(net_down_history_with_small_bounces(), 3);

    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Long,
            &args,
        ),
        None
    );
}

#[test]
fn entry_confirmation_accepts_reversal_before_price_reclaims_averages() {
    let mut candles = net_down_history_with_small_bounces();
    candles.last_mut().expect("trigger candle").volume = 20.0;
    let args = reversal_entry_args();
    let computed = build_computed_candles(candles, args.entry_period);

    let (ok, trigger) = entry_confirmation(
        &computed,
        13 * MS_15M,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(ok);
    assert_eq!(trigger, "opposite_move_momentum_reversal");
}

#[test]
fn short_entry_confirmation_accepts_reversal_before_price_loses_averages() {
    let mut candles = net_down_history_with_small_bounces();
    for candle in &mut candles {
        candle.open = 200.0 - candle.open;
        candle.close = 200.0 - candle.close;
        candle.high = candle.open.max(candle.close) + 1.0;
        candle.low = candle.open.min(candle.close) - 1.0;
    }
    candles.last_mut().expect("trigger candle").volume = 20.0;
    let args = reversal_entry_args();
    let computed = build_computed_candles(candles, args.entry_period);

    let (ok, trigger) = entry_confirmation(
        &computed,
        13 * MS_15M,
        MarketVelocityTradeDirection::Short,
        &args,
    );
    assert!(ok);
    assert_eq!(trigger, "opposite_move_momentum_reversal");
}

#[test]
fn short_accepts_ten_percent_net_rise_even_with_small_pullbacks() {
    let mut candles = net_down_history_with_small_bounces();
    for candle in &mut candles {
        candle.open = 200.0 - candle.open;
        candle.close = 200.0 - candle.close;
        candle.high = candle.open.max(candle.close) + 1.0;
        candle.low = candle.open.min(candle.close) - 1.0;
    }
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(candles, 3);

    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Short,
            &args,
        ),
        None
    );
}

#[test]
fn net_move_filter_rejects_a_window_below_ten_percent() {
    let mut candles = net_down_history_with_small_bounces();
    candles[11].close = 91.0;
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(candles, 3);

    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Long,
            &args,
        ),
        Some("opposite_net_move_not_confirmed")
    );
}

#[test]
fn duration_branch_accepts_slow_drop_below_ten_percent() {
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 192,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_min_opposite_duration_candles: Some(96),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(long_slow_duration_history_with_trigger(), 20);

    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Long,
            &args,
        ),
        None
    );
}

#[test]
fn duration_branch_accepts_overall_drop_with_long_intermediate_rebounds() {
    let mut candles = (0_i64..192_i64)
        .map(|idx| {
            let step = idx.saturating_sub(96) as f64;
            let close = if idx < 96 {
                100.0
            } else {
                100.0 - step * 0.04
                    + (step * std::f64::consts::PI / 23.0 - std::f64::consts::PI / 2.0).sin() * 0.6
            };
            candle(idx * MS_15M, close + 0.01, close, 10.0)
        })
        .collect::<Vec<_>>();
    candles.push(candle(192 * MS_15M, 95.0, 96.0, 20.0));
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 192,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_min_opposite_duration_candles: Some(96),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(candles, 20);

    assert!(computed[119].candle.close > computed[96].candle.close);
    assert!(computed[167].candle.close > computed[144].candle.close);
    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Long,
            &args,
        ),
        None
    );
}

#[test]
fn duration_branch_accepts_slow_rise_for_short() {
    let mut candles = long_slow_duration_history_with_trigger();
    for candle in &mut candles {
        candle.open = 200.0 - candle.open;
        candle.close = 200.0 - candle.close;
        candle.high = candle.open.max(candle.close) + 1.0;
        candle.low = candle.open.min(candle.close) - 1.0;
    }
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 192,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_min_opposite_duration_candles: Some(96),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(candles, 20);

    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Short,
            &args,
        ),
        None
    );
}

#[test]
fn duration_branch_rejects_long_sideways_window() {
    let mut candles = (0..192)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles.push(candle(192 * MS_15M, 100.0, 101.0, 20.0));
    let args = MarketVelocityEventBacktestArgs {
        entry_opposite_move_lookback_candles: 192,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_min_opposite_duration_candles: Some(96),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(candles, 20);

    assert_eq!(
        opposite_net_move_filter_reason(
            &computed,
            computed.len(),
            MarketVelocityTradeDirection::Long,
            &args,
        ),
        Some("opposite_move_not_confirmed")
    );
}

#[test]
fn entry_confirmation_accepts_volume_reversal_after_slow_duration_drop() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 20,
        entry_max_distance_pct: 14.0,
        entry_min_volume_ratio: 1.5,
        entry_opposite_move_lookback_candles: 192,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_min_opposite_duration_candles: Some(96),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(long_slow_duration_history_with_trigger(), 20);

    let (confirmed, trigger) = entry_confirmation(
        &computed,
        193 * MS_15M,
        MarketVelocityTradeDirection::Long,
        &args,
    );

    assert!(confirmed);
    assert_eq!(trigger, "opposite_move_momentum_reversal");
}

#[test]
fn exhaustion_volume_filter_blocks_weaker_long_and_short_clusters() {
    let args = MarketVelocityEventBacktestArgs {
        entry_min_exhaustion_volume_dominance_ratio: Some(1.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    for direction in [
        MarketVelocityTradeDirection::Long,
        MarketVelocityTradeDirection::Short,
    ] {
        let computed = build_computed_candles(exhaustion_volume_history(direction, 500.0), 20);
        assert_eq!(
            exhaustion_volume_dominance_filter_reason(&computed, computed.len(), direction, &args,),
            Some("weaker_volume_than_previous_exhaustion_extreme")
        );
    }
}

#[test]
fn exhaustion_volume_filter_accepts_equal_long_and_stronger_short_clusters() {
    let args = MarketVelocityEventBacktestArgs {
        entry_min_exhaustion_volume_dominance_ratio: Some(1.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    for (direction, current_volume) in [
        (MarketVelocityTradeDirection::Long, 1_000.0),
        (MarketVelocityTradeDirection::Short, 1_200.0),
    ] {
        let computed =
            build_computed_candles(exhaustion_volume_history(direction, current_volume), 20);
        assert_eq!(
            exhaustion_volume_dominance_filter_reason(&computed, computed.len(), direction, &args,),
            None
        );
    }
}

#[test]
fn exhaustion_volume_filter_ignores_candles_after_signal() {
    let args = MarketVelocityEventBacktestArgs {
        entry_min_exhaustion_volume_dominance_ratio: Some(1.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let signal_count = 100;
    let mut candles = exhaustion_volume_history(MarketVelocityTradeDirection::Long, 500.0);
    let computed_at_signal = build_computed_candles(candles.clone(), 20);
    candles.push(candle(100 * MS_15M, 100.0, 120.0, 100_000.0));
    let computed_with_future = build_computed_candles(candles, 20);

    let at_signal = exhaustion_volume_dominance_filter_reason(
        &computed_at_signal,
        signal_count,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    let after_future_arrives = exhaustion_volume_dominance_filter_reason(
        &computed_with_future,
        signal_count,
        MarketVelocityTradeDirection::Long,
        &args,
    );

    assert_eq!(at_signal, after_future_arrives);
    assert_eq!(
        at_signal,
        Some("weaker_volume_than_previous_exhaustion_extreme")
    );
}

#[test]
fn volume_ratio_selects_one_and_half_two_or_three_atr_targets() {
    for (trigger_volume, expected_r) in [(15.0, 1.5), (20.0, 2.0), (30.0, 3.0)] {
        let mut candles = (0..20)
            .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
            .collect::<Vec<_>>();
        candles.push(candle(20 * MS_15M, 100.0, 100.0, trigger_volume));
        let event_ts = 21 * MS_15M;

        assert_eq!(
            volume_atr_target_r(&candles, event_ts, event_ts, 100.0, 0.02),
            Some(expected_r)
        );
    }
}

#[test]
fn volume_atr_target_ignores_candles_after_the_signal() {
    let mut candles = (0..20)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles.push(candle(20 * MS_15M, 100.0, 100.0, 20.0));
    let event_ts = 21 * MS_15M;
    candles.push(candle(21 * MS_15M, 100.0, 160.0, 1_000.0));

    assert_eq!(
        volume_atr_target_r(&candles, event_ts, event_ts, 100.0, 0.02),
        Some(2.0)
    );
}

#[test]
fn volume_atr_target_policy_applies_frozen_r_band() {
    let mut candles = (0..20)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles.push(candle(20 * MS_15M, 100.0, 100.0, 15.0));
    let event_ts = 21 * MS_15M;
    let min_args = MarketVelocityEventBacktestArgs {
        volume_atr_target_scale: 0.5,
        volume_atr_min_target_r: Some(1.8),
        volume_atr_max_target_r: Some(3.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let max_args = MarketVelocityEventBacktestArgs {
        volume_atr_target_scale: 4.0,
        ..min_args.clone()
    };

    assert_eq!(
        volume_atr_target_r_with_policy(&candles, event_ts, event_ts, 100.0, 0.02, &min_args),
        Some(1.8)
    );
    assert_eq!(
        volume_atr_target_r_with_policy(&candles, event_ts, event_ts, 100.0, 0.02, &max_args),
        Some(3.0)
    );
}

#[test]
fn outcome_replay_and_paper_payload_use_the_per_signal_atr_target() {
    let mut candles = (0..20)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles.push(candle(20 * MS_15M, 100.0, 100.0, 20.0));
    candles.push(BacktestCandle {
        ts: 21 * MS_15M,
        open: 100.0,
        high: 105.0,
        low: 99.0,
        close: 104.0,
        volume: 10.0,
    });
    let event_ts = 21 * MS_15M;
    let confirmed = ConfirmedEvent {
        event: RadarEvent {
            id: 7,
            exchange: "okx".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            ts: event_ts,
            detected_at: "2026-07-19T00:00:00Z".to_string(),
            new_rank: 1,
            delta_rank: 10,
            current_price: 100.0,
            price_change_pct: 1.0,
        },
        direction: MarketVelocityTradeDirection::Long,
        entry_ts: event_ts,
        entry_price: 100.0,
        entry_idx: 21,
        trigger: "reclaim_ema".to_string(),
        structure_stop_loss_price: None,
        structure_stop_loss_source: None,
    };
    let args = MarketVelocityEventBacktestArgs {
        stop_loss_pct: 0.02,
        target_rs: vec![1.0],
        volume_atr_take_profit: true,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles_by_symbol =
        std::collections::HashMap::from([("TEST-USDT-SWAP".to_string(), candles)]);

    let (results, _) = summarize_target(
        std::slice::from_ref(&confirmed),
        &candles_by_symbol,
        1.0,
        24 * 60 * 60 * 1_000,
        &args,
    );
    assert_eq!(results[0].target_r, Some(2.0));
    assert_eq!(results[0].outcome, TradeOutcome::Win);

    let outcomes = build_market_velocity_paper_outcomes(&[confirmed], &candles_by_symbol, &args);
    assert_eq!(outcomes[0].target_r, 2.0);
    assert_eq!(
        outcomes[0].evaluation_payload["take_profit_mode"],
        "volume_atr"
    );
}

#[test]
fn new_research_preset_keeps_both_sides_and_ten_percent_net_move() {
    let preset = "research_market_momentum_opposite_move10_n192_volume_atr_both_15m_v1";
    let args = parse_paper_observation_args_from(["--paper-strategy-preset", preset])
        .expect("parse opposite-move volume ATR preset");

    assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
    assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Both);
    assert_eq!(args.entry_opposite_move_lookback_candles, 192);
    assert_eq!(args.entry_min_opposite_net_move_pct, Some(10.0));
    assert_eq!(args.entry_min_volume_ratio, 1.5);
    assert!(args.volume_atr_take_profit);

    let manifest = market_velocity_paper_strategy_preset_manifest(preset)
        .expect("build opposite-move research manifest");
    assert_eq!(
        manifest.strategy_key,
        "market_momentum_opposite_move_reversal"
    );
    assert_eq!(manifest.channel, "research");
    assert_eq!(manifest.manifest_status, "research");
    assert_eq!(
        manifest.manifest_json["parameters"]["take_profit"]["mode"],
        "volume_atr"
    );
    assert!(market_velocity_paper_observation_usage().contains(preset));
}

#[test]
fn volume_atr_mode_rejects_duplicate_placeholder_target_loops() {
    let error = parse_cli_args_from(["--volume-atr-take-profit", "--target-rs", "1.0,2.0"])
        .expect_err("dynamic target must not emit duplicate outcome loops");

    assert!(error.to_string().contains("exactly one placeholder"));
}

#[test]
fn volume_atr_mode_allows_backtest_detail_persistence() {
    let args = parse_cli_args_from([
        "--volume-atr-take-profit",
        "--target-rs",
        "1.0",
        "--save-backtest-detail",
    ])
    .expect("dynamic ATR target should support persistence");

    assert!(args.volume_atr_take_profit);
    assert!(args.save_backtest_detail);
}

#[test]
fn framework_replay_persists_the_per_trade_volume_atr_target() {
    let mut candles = (0..505)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles[504].volume = 20.0;
    candles.push(candle(505 * MS_15M, 100.0, 100.0, 10.0));
    candles.push(BacktestCandle {
        ts: 506 * MS_15M,
        open: 100.0,
        high: 105.0,
        low: 99.5,
        close: 104.0,
        volume: 10.0,
    });
    let event_ts = 505 * MS_15M;
    let confirmed = ConfirmedEvent {
        event: RadarEvent {
            id: 8,
            exchange: "okx".to_string(),
            symbol: "PERSIST-USDT-SWAP".to_string(),
            ts: event_ts,
            detected_at: "2026-07-19T00:00:00Z".to_string(),
            new_rank: 1,
            delta_rank: 10,
            current_price: 100.0,
            price_change_pct: 1.0,
        },
        direction: MarketVelocityTradeDirection::Long,
        entry_ts: event_ts,
        entry_price: 100.0,
        entry_idx: 505,
        trigger: "opposite_move_momentum_reversal".to_string(),
        structure_stop_loss_price: None,
        structure_stop_loss_source: None,
    };
    let args = MarketVelocityEventBacktestArgs {
        stop_loss_pct: 0.02,
        target_rs: vec![1.0],
        volume_atr_take_profit: true,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles_by_symbol =
        std::collections::HashMap::from([("PERSIST-USDT-SWAP".to_string(), candles)]);

    let reports = build_framework_equity_trade_reports(
        std::slice::from_ref(&confirmed),
        &candles_by_symbol,
        1.0,
        &args,
    );
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].target_r, 2.0);

    let details = build_market_velocity_backtest_details(&reports[0], 456, &args)
        .expect("build dynamic ATR backtest details");
    let signal_value = serde_json::from_str::<serde_json::Value>(&details[1].signal_value)
        .expect("parse detail signal value");
    assert!(!details[0].open_position_time.contains('+'));
    assert!(!details[0]
        .signal_open_position_time
        .as_deref()
        .expect("signal open time")
        .contains('+'));
    assert!(!details[1].close_position_time.contains('+'));
    assert_eq!(signal_value["target_r"], 2.0);
    assert_eq!(signal_value["take_profit_mode"], "volume_atr");

    let risk = market_velocity_risk_config_detail(&args, 1.0);
    assert_eq!(risk["take_profit_mode"], "volume_atr");
    assert!(risk["target_r"].is_null());
    assert_eq!(risk["target_r_placeholder"], 1.0);
}

#[test]
fn bearish_volume_expansion_creates_a_deferred_long_setup() {
    let candles = deferred_long_candles();
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.5,
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_defer_bearish_continuation: true,
        entry_defer_max_wait_candles: 3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = 27 * MS_15M;

    assert!(is_bearish_continuation_setup(
        &computed,
        26,
        args.entry_min_volume_ratio,
    ));
    assert_eq!(
        entry_confirmation(
            &computed[..27],
            event_ts,
            MarketVelocityTradeDirection::Long,
            &args,
        ),
        (true, "opposite_move_bearish_continuation_setup".to_string())
    );
    assert_eq!(
        deferred_long_confirmation_entry_idx(
            &computed,
            26,
            args.entry_defer_max_wait_candles,
            args.entry_min_volume_ratio,
        ),
        Ok(28)
    );
}

#[test]
fn event_evaluation_enters_only_after_deferred_bullish_confirmation() {
    let candles = deferred_long_candles();
    let computed = build_computed_candles(candles.clone(), 3);
    let event_ts = 27 * MS_15M;
    let event = RadarEvent {
        id: 9,
        exchange: "okx".to_string(),
        symbol: "DEFER-USDT-SWAP".to_string(),
        ts: event_ts,
        detected_at: "1970-01-01T06:45:00Z".to_string(),
        new_rank: 0,
        delta_rank: 0,
        current_price: 96.0,
        price_change_pct: -4.0,
    };
    let args = MarketVelocityEventBacktestArgs {
        event_source: MarketVelocityEventSource::Kline15m,
        trade_direction: MarketVelocityTradeDirection::Both,
        trend_timeframe: MarketVelocityTrendTimeframe::Off,
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.5,
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_defer_bearish_continuation: true,
        entry_defer_max_wait_candles: 3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed_by_symbol =
        std::collections::HashMap::from([("DEFER-USDT-SWAP".to_string(), computed)]);
    let raw_by_symbol = std::collections::HashMap::from([("DEFER-USDT-SWAP".to_string(), candles)]);

    let report = evaluate_events(
        &[event],
        &std::collections::HashMap::new(),
        &computed_by_symbol,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &raw_by_symbol,
        &args,
    );

    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(
        report.confirmed[0].direction,
        MarketVelocityTradeDirection::Long
    );
    assert_eq!(report.confirmed[0].entry_ts, 28 * MS_15M);
    assert_eq!(report.confirmed[0].entry_price, 101.3);
    assert_eq!(
        report.confirmed[0].trigger,
        "opposite_move_momentum_reversal+deferred_bearish_continuation"
    );
}

#[test]
fn both_mode_reclassifies_bullish_continuation_as_deferred_short_setup() {
    let mut candles = deferred_long_candles();
    for candle in &mut candles {
        let old_high = candle.high;
        let old_low = candle.low;
        candle.open = 200.0 - candle.open;
        candle.close = 200.0 - candle.close;
        candle.high = 200.0 - old_low;
        candle.low = 200.0 - old_high;
    }
    let computed = build_computed_candles(candles.clone(), 3);
    let event_ts = 27 * MS_15M;
    let event = RadarEvent {
        id: 10,
        exchange: "okx".to_string(),
        symbol: "DEFER-SHORT-USDT-SWAP".to_string(),
        ts: event_ts,
        detected_at: "1970-01-01T06:45:00Z".to_string(),
        new_rank: 0,
        delta_rank: 0,
        current_price: 104.0,
        price_change_pct: 4.0,
    };
    let args = MarketVelocityEventBacktestArgs {
        event_source: MarketVelocityEventSource::Kline15m,
        trade_direction: MarketVelocityTradeDirection::Both,
        trend_timeframe: MarketVelocityTrendTimeframe::Off,
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.5,
        entry_opposite_move_lookback_candles: 12,
        entry_min_opposite_net_move_pct: Some(10.0),
        entry_defer_bullish_continuation: true,
        entry_defer_max_wait_candles: 3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let computed_by_symbol =
        std::collections::HashMap::from([("DEFER-SHORT-USDT-SWAP".to_string(), computed)]);
    let raw_by_symbol =
        std::collections::HashMap::from([("DEFER-SHORT-USDT-SWAP".to_string(), candles)]);

    let report = evaluate_events(
        &[event],
        &std::collections::HashMap::new(),
        &computed_by_symbol,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &raw_by_symbol,
        &args,
    );

    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(
        report.confirmed[0].direction,
        MarketVelocityTradeDirection::Short
    );
    assert_eq!(report.confirmed[0].entry_ts, 28 * MS_15M);
    assert_eq!(report.confirmed[0].entry_price, 98.7);
    assert_eq!(
        report.confirmed[0].trigger,
        "opposite_move_momentum_reversal+deferred_bullish_continuation"
    );
}

#[test]
fn deferred_long_setup_expires_when_price_keeps_falling() {
    let mut candles = deferred_long_candles();
    candles[27] = BacktestCandle {
        ts: 27 * MS_15M,
        open: 96.0,
        high: 96.2,
        low: 91.0,
        close: 91.5,
        volume: 10.0,
    };
    let computed = build_computed_candles(candles, 3);

    assert_eq!(
        deferred_long_confirmation_entry_idx(&computed, 26, 3, 1.5),
        Err("deferred_reversal_invalidated")
    );
}

#[test]
fn volume_tier_uses_setup_candle_while_atr_uses_confirmation_time() {
    let mut candles = (0..20)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles.push(candle(20 * MS_15M, 100.0, 100.0, 20.0));
    candles.push(BacktestCandle {
        ts: 21 * MS_15M,
        open: 100.0,
        high: 105.0,
        low: 95.0,
        close: 101.0,
        volume: 10.0,
    });

    let setup_atr_target = volume_atr_target_r(&candles, 21 * MS_15M, 21 * MS_15M, 100.0, 0.02)
        .expect("setup ATR target");
    let confirmation_atr_target =
        volume_atr_target_r(&candles, 21 * MS_15M, 22 * MS_15M, 100.0, 0.02)
            .expect("confirmation ATR target");

    assert_eq!(setup_atr_target, 2.0);
    assert!(confirmation_atr_target > setup_atr_target);
}

#[test]
fn deferred_long_v2_preset_is_research_only_and_keeps_v1_stable() {
    let v1 = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_market_momentum_opposite_move10_n192_volume_atr_both_15m_v1",
    ])
    .expect("parse v1");
    let preset = "research_market_momentum_opposite_move10_n192_volume_atr_long_defer3_15m_v2";
    let v2 = parse_paper_observation_args_from(["--paper-strategy-preset", preset])
        .expect("parse deferred long v2");

    assert_eq!(v1.trade_direction, MarketVelocityTradeDirection::Both);
    assert!(!v1.entry_defer_bearish_continuation);
    assert_eq!(v2.trade_direction, MarketVelocityTradeDirection::Long);
    assert!(v2.entry_defer_bearish_continuation);
    assert_eq!(v2.entry_defer_max_wait_candles, 3);
    assert_eq!(v2.entry_opposite_move_lookback_candles, 192);
    assert_eq!(v2.entry_min_opposite_net_move_pct, Some(10.0));

    let manifest = market_velocity_paper_strategy_preset_manifest(preset)
        .expect("build deferred long v2 manifest");
    assert_eq!(
        manifest.strategy_key,
        "market_momentum_opposite_move_reversal"
    );
    assert_eq!(manifest.channel, "research");
    assert_eq!(manifest.manifest_status, "research");
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_defer_bearish_continuation"],
        true
    );
    assert!(market_velocity_paper_observation_usage().contains(preset));
}

#[test]
fn duration_both_v3_preset_adds_time_branch_and_keeps_v2_stable() {
    let v2 = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_market_momentum_opposite_move10_n192_volume_atr_long_defer3_15m_v2",
    ])
    .expect("parse v2");
    let preset = "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_15m_v3";
    let v3 = parse_paper_observation_args_from(["--paper-strategy-preset", preset])
        .expect("parse duration both v3");

    assert_eq!(v2.entry_min_opposite_duration_candles, None);
    assert_eq!(v3.trade_direction, MarketVelocityTradeDirection::Both);
    assert_eq!(v3.entry_opposite_move_lookback_candles, 192);
    assert_eq!(v3.entry_min_opposite_net_move_pct, Some(10.0));
    assert_eq!(v3.entry_min_opposite_duration_candles, Some(96));
    assert!(v3.entry_defer_bearish_continuation);

    let manifest = market_velocity_paper_strategy_preset_manifest(preset)
        .expect("build duration both v3 manifest");
    assert_eq!(manifest.channel, "research");
    assert_eq!(manifest.manifest_status, "research");
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_min_opposite_duration_candles"],
        96
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_opposite_duration_min_r_squared"],
        0.7
    );
    assert!(market_velocity_paper_observation_usage().contains(preset));
}

#[test]
fn exhaustion_volume_v4_keeps_existing_strategy_identity_and_v3_stable() {
    let v3 = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_15m_v3",
    ])
    .expect("parse v3");
    let preset = "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_exhaustionvol1_15m_v4";
    let v4 = parse_paper_observation_args_from(["--paper-strategy-preset", preset])
        .expect("parse exhaustion volume v4");

    assert_eq!(v3.entry_min_exhaustion_volume_dominance_ratio, None);
    assert_eq!(v4.entry_min_exhaustion_volume_dominance_ratio, Some(1.0));
    assert_eq!(v4.trade_direction, MarketVelocityTradeDirection::Both);
    assert_eq!(v4.entry_min_opposite_duration_candles, Some(96));
    assert_eq!(
        market_velocity_strategy_type(&v4),
        "market_velocity_kline_15m"
    );
    assert_eq!(
        v4.paper_outcome_entry_rule_version,
        "kline15m_market_momentum_opposite_net10_n192_or_dur96_volatr_both_deferlong3_exvol1_v4"
    );

    let manifest = market_velocity_paper_strategy_preset_manifest(preset)
        .expect("build exhaustion volume v4 manifest");
    assert_eq!(
        manifest.strategy_key,
        "market_momentum_opposite_move_reversal"
    );
    assert_eq!(
        manifest.product_slug,
        "market-momentum-opposite-move-reversal"
    );
    assert_eq!(manifest.channel, "research");
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_min_exhaustion_volume_dominance_ratio"],
        1.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_exhaustion_volume_lookback_candles"],
        96
    );
    assert!(market_velocity_paper_observation_usage().contains(preset));
}

#[test]
fn risk_reward_v5_keeps_strategy_identity_and_explicit_costs() {
    let v4 = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_exhaustionvol1_15m_v4",
    ])
    .expect("parse v4");
    let preset = "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_r18_30_scale4_both_deferlong3_exhaustionvol1_15m_v5";
    let v5 = parse_paper_observation_args_from(["--paper-strategy-preset", preset])
        .expect("parse risk reward v5");

    assert_eq!(v4.volume_atr_target_scale, 1.0);
    assert_eq!(v4.volume_atr_min_target_r, None);
    assert_eq!(v4.volume_atr_max_target_r, None);
    assert_eq!(v4.backtest_fee_bps_per_side, None);
    assert_eq!(v5.volume_atr_target_scale, 4.0);
    assert_eq!(v5.volume_atr_min_target_r, Some(1.8));
    assert_eq!(v5.volume_atr_max_target_r, Some(3.0));
    assert_eq!(v5.backtest_fee_bps_per_side, Some(5.0));
    assert_eq!(v5.backtest_slippage_bps_per_side, 3.0);
    assert_eq!(v5.entry_min_exhaustion_volume_dominance_ratio, Some(1.0));
    assert_eq!(v5.trade_direction, MarketVelocityTradeDirection::Both);

    let manifest = market_velocity_paper_strategy_preset_manifest(preset)
        .expect("build risk reward v5 manifest");
    assert_eq!(
        manifest.strategy_key,
        "market_momentum_opposite_move_reversal"
    );
    assert_eq!(
        manifest.product_slug,
        "market-momentum-opposite-move-reversal"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["take_profit"]["target_scale"],
        4.0
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["cost_model"]["fee_bps_per_side"],
        5.0
    );
}

#[test]
fn volume_atr_v5_allows_equity_diagnostics_and_rejects_ambiguous_costs() {
    let args = parse_cli_args_from([
        "--volume-atr-take-profit",
        "--target-rs",
        "1.0",
        "--equity-report",
        "--backtest-fee-bps-per-side",
        "5",
        "--backtest-slippage-bps-per-side",
        "3",
    ])
    .expect("volume ATR replay now supports equity diagnostics");
    assert!(args.equity_report);

    let error = parse_cli_args_from(["--backtest-slippage-bps-per-side", "3"])
        .expect_err("slippage requires an explicit fee baseline");
    assert!(error.to_string().contains("requires explicit"));
}

#[test]
fn v5_combines_explicit_fee_and_slippage_into_framework_cost() {
    let args = MarketVelocityEventBacktestArgs {
        backtest_fee_bps_per_side: Some(5.0),
        backtest_slippage_bps_per_side: 3.0,
        ..MarketVelocityEventBacktestArgs::default()
    };

    assert_eq!(framework_trade_cost_rate(&args), Some(0.0008));
    assert_eq!(
        framework_trade_cost_rate(&MarketVelocityEventBacktestArgs::default()),
        None
    );
}

#[test]
fn v6_requires_symmetric_price_reversal_confirmation() {
    let previous = candle(0, 100.0, 100.0, 10.0);
    let weak = candle(MS_15M, 100.0, 100.2, 20.0);
    let bullish_break = candle(MS_15M, 100.0, 104.0, 20.0);
    let bearish_break = candle(MS_15M, 100.0, 96.0, 20.0);

    let weak = build_computed_candles(vec![previous.clone(), weak], 1);
    let long = build_computed_candles(vec![previous.clone(), bullish_break], 1);
    let short = build_computed_candles(vec![previous, bearish_break], 1);

    assert_eq!(
        opposite_reversal_confirmation_filter_reason(&weak, 1, MarketVelocityTradeDirection::Long,),
        Some("opposite_reversal_candle_not_confirmed")
    );
    assert_eq!(
        opposite_reversal_confirmation_filter_reason(&long, 1, MarketVelocityTradeDirection::Long,),
        None
    );
    assert_eq!(
        opposite_reversal_confirmation_filter_reason(
            &short,
            1,
            MarketVelocityTradeDirection::Short,
        ),
        None
    );
}

#[test]
fn bullish_continuation_waits_for_bearish_short_confirmation() {
    let mut candles = (0..20)
        .map(|idx| candle(idx * MS_15M, 100.0, 100.0, 10.0))
        .collect::<Vec<_>>();
    candles.push(candle(20 * MS_15M, 100.0, 106.0, 20.0));
    candles.push(candle(21 * MS_15M, 106.0, 98.0, 10.0));
    candles.push(candle(22 * MS_15M, 98.0, 98.0, 10.0));
    let computed = build_computed_candles(candles, 3);

    assert!(is_bullish_continuation_setup(&computed, 20, 1.5));
    assert_eq!(
        deferred_short_confirmation_entry_idx(&computed, 20, 3, 1.5),
        Ok(22)
    );
}

#[test]
fn confirmed_reversal_v6_keeps_same_strategy_identity_and_v5_stable() {
    let v5_preset = "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_r18_30_scale4_both_deferlong3_exhaustionvol1_15m_v5";
    let v6_preset =
        "research_market_momentum_opposite_move_reversal_confirmed_both_defer3_volatr_r18_30_15m_v6";
    let v5 = parse_paper_observation_args_from(["--paper-strategy-preset", v5_preset])
        .expect("parse v5");
    let v6 = parse_paper_observation_args_from(["--paper-strategy-preset", v6_preset])
        .expect("parse v6");

    assert!(!v5.entry_defer_bullish_continuation);
    assert!(!v5.entry_require_opposite_reversal_confirmation);
    assert!(v6.entry_defer_bearish_continuation);
    assert!(v6.entry_defer_bullish_continuation);
    assert!(v6.entry_require_opposite_reversal_confirmation);
    assert_eq!(v6.entry_defer_max_wait_candles, 3);
    assert_eq!(v6.volume_atr_target_scale, 4.0);

    let manifest = market_velocity_paper_strategy_preset_manifest(v6_preset)
        .expect("build confirmed reversal v6 manifest");
    assert_eq!(
        manifest.strategy_key,
        "market_momentum_opposite_move_reversal"
    );
    assert_eq!(
        manifest.product_slug,
        "market-momentum-opposite-move-reversal"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_require_opposite_reversal_confirmation"],
        true
    );
}

#[test]
fn v7_requires_reversal_close_beyond_both_averages() {
    let mut candles = build_computed_candles(
        vec![
            candle(0, 100.0, 100.0, 10.0),
            candle(MS_15M, 100.0, 104.0, 20.0),
        ],
        1,
    );
    candles[1].sma = Some(101.0);
    candles[1].ema = Some(102.0);
    assert_eq!(
        reversal_average_reclaim_filter_reason(&candles, 1, MarketVelocityTradeDirection::Long),
        None
    );

    candles[1].ema = Some(105.0);
    assert_eq!(
        reversal_average_reclaim_filter_reason(&candles, 1, MarketVelocityTradeDirection::Long),
        Some("reversal_average_not_reclaimed")
    );
}

#[test]
fn mean_reclaim_v7_keeps_same_strategy_identity_and_v6_stable() {
    let v6_preset =
        "research_market_momentum_opposite_move_reversal_confirmed_both_defer3_volatr_r18_30_15m_v6";
    let v7_preset =
        "research_market_momentum_opposite_move_reversal_mean_reclaim_both_defer3_volatr_r18_30_15m_v7";
    let v6 = parse_paper_observation_args_from(["--paper-strategy-preset", v6_preset])
        .expect("parse v6");
    let v7 = parse_paper_observation_args_from(["--paper-strategy-preset", v7_preset])
        .expect("parse v7");

    assert!(!v6.entry_require_reversal_average_reclaim);
    assert!(v7.entry_require_reversal_average_reclaim);
    assert!(v7.entry_require_opposite_reversal_confirmation);
    assert_eq!(v7.volume_atr_target_scale, 4.0);

    let manifest = market_velocity_paper_strategy_preset_manifest(v7_preset)
        .expect("build mean reclaim v7 manifest");
    assert_eq!(
        manifest.strategy_key,
        "market_momentum_opposite_move_reversal"
    );
    assert_eq!(
        manifest.product_slug,
        "market-momentum-opposite-move-reversal"
    );
    assert_eq!(
        manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["entry_require_reversal_average_reclaim"],
        true
    );
}
