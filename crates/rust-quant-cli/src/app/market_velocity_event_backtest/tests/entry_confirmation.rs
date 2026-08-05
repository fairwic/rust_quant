use super::*;

#[test]
fn precomputes_sma_ema_and_previous_volume_average() {
    let candles = vec![
        candle(0, 1.0, 10.0),
        candle(MS_15M, 2.0, 20.0),
        candle(MS_15M * 2, 3.0, 30.0),
        candle(MS_15M * 3, 4.0, 40.0),
        candle(MS_15M * 4, 5.0, 50.0),
    ];
    let computed = build_computed_candles(candles, 3);
    assert_eq!(computed[2].sma, Some(2.0));
    assert_eq!(computed[2].ema, Some(2.0));
    assert_eq!(computed[3].sma, Some(3.0));
    assert_eq!(computed[3].ema, Some(3.0));
    assert_eq!(computed[3].previous_volume_avg, Some(20.0));
    assert_eq!(computed[4].ema, Some(4.0));
}
#[test]
fn precomputes_rsi14_and_bollinger20_for_fast_filters() {
    let candles = (0..22)
        .map(|idx| candle(MS_15M * idx, 100.0 + idx as f64, 10.0))
        .collect::<Vec<_>>();
    let computed = build_computed_candles(candles, 3);
    assert!(computed[14].rsi14.is_some());
    assert!(computed[14].atr14.is_some());
    assert!(computed[19].bollinger_upper.is_some());
    assert!(computed[19].bollinger_middle.is_some());
    assert!(computed[19].bollinger_lower.is_some());
    assert!(computed[19].bollinger_bandwidth_pct.is_some());
    assert!(computed[21].rsi14.unwrap() > 50.0);
}
#[test]
fn entry_confirmation_accepts_breakout_above_averages_with_volume() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        candle(0, 100.0, 10.0),
        candle(MS_15M, 101.0, 10.0),
        candle(MS_15M * 2, 102.0, 10.0),
        BacktestCandle {
            ts: MS_15M * 3,
            open: 101.5,
            high: 102.4,
            low: 101.0,
            close: 102.0,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 4,
            open: 102.0,
            high: 106.0,
            low: 101.8,
            close: 105.0,
            volume: 20.0,
        },
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 5;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(ok);
    assert_eq!(reason, "breakout_previous_high");
}
#[test]
fn entry_confirmation_blocks_when_rsi_is_above_fast_momentum_max() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_max_rsi: Some(50.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = fast_momentum_breakout_candles();
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 22;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "rsi_above_max");
}
#[test]
fn entry_confirmation_accepts_bollinger_breakout_after_recent_drawdown() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_min_rsi: Some(50.0),
        entry_max_rsi: Some(100.0),
        entry_bollinger_breakout: true,
        entry_min_recent_drawdown_pct: Some(10.0),
        entry_recent_drawdown_lookback_candles: 12,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = fast_momentum_breakout_candles();
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 22;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(ok);
    assert_eq!(reason, "breakout_previous_high");
}
#[test]
fn entry_confirmation_blocks_without_recent_drawdown() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_bollinger_breakout: true,
        entry_min_recent_drawdown_pct: Some(10.0),
        entry_recent_drawdown_lookback_candles: 12,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = fast_momentum_breakout_without_drawdown_candles();
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 22;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "recent_drawdown_not_confirmed");
}
#[test]
fn entry_confirmation_accepts_breakdown_below_averages_with_volume_for_short() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        candle(0, 105.0, 10.0),
        candle(MS_15M, 104.0, 10.0),
        candle(MS_15M * 2, 103.0, 10.0),
        BacktestCandle {
            ts: MS_15M * 3,
            open: 103.5,
            high: 104.0,
            low: 102.6,
            close: 103.0,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 4,
            open: 103.0,
            high: 103.2,
            low: 99.0,
            close: 100.0,
            volume: 20.0,
        },
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 5;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Short,
        &args,
    );
    assert!(ok);
    assert_eq!(reason, "breakdown_previous_low");
}
#[test]
fn entry_confirmation_labels_sideways_range_breakdown_with_volume_for_short() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        ohlc(0, 100.0, 101.0, 99.5, 100.4),
        ohlc(MS_15M, 100.4, 101.1, 99.7, 100.6),
        ohlc(MS_15M * 2, 100.6, 101.2, 99.8, 100.7),
        ohlc(MS_15M * 3, 100.7, 101.3, 99.9, 100.8),
        ohlc(MS_15M * 4, 100.8, 101.2, 99.8, 100.4),
        ohlc(MS_15M * 5, 100.4, 101.0, 99.6, 100.2),
        ohlc(MS_15M * 6, 100.2, 101.1, 99.7, 100.5),
        ohlc(MS_15M * 7, 100.5, 101.0, 99.7, 100.1),
        ohlcv(MS_15M * 8, 99.8, 100.2, 96.8, 97.4, 24.0),
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 9;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Short,
        &args,
    );
    assert!(ok);
    assert_eq!(reason, "breakdown_range_low");
}
#[test]
fn entry_confirmation_does_not_label_top_shoulder_as_breakdown_short() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        ohlc(0, 100.0, 101.0, 99.5, 100.5),
        ohlc(MS_15M, 100.5, 103.0, 100.2, 102.6),
        ohlc(MS_15M * 2, 102.6, 107.0, 102.0, 105.8),
        ohlc(MS_15M * 3, 105.8, 106.0, 101.8, 102.4),
        ohlc(MS_15M * 4, 102.4, 104.8, 101.9, 104.0),
        ohlc(MS_15M * 5, 104.0, 104.4, 100.0, 100.8),
        ohlcv(MS_15M * 6, 100.7, 102.1, 99.4, 100.1, 26.0),
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 7;
    let (_, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Short,
        &args,
    );
    assert_ne!(reason, "top_shoulder_volume_fade");
}
#[test]
fn entry_confirmation_requires_latest_volume_for_reclaim_ema() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        candle(0, 100.0, 10.0),
        candle(MS_15M, 102.0, 10.0),
        candle(MS_15M * 2, 104.0, 20.0),
        BacktestCandle {
            ts: MS_15M * 3,
            open: 104.2,
            high: 104.5,
            low: 100.8,
            close: 101.0,
            volume: 30.0,
        },
        BacktestCandle {
            ts: MS_15M * 4,
            open: 101.2,
            high: 104.6,
            low: 101.0,
            close: 104.0,
            volume: 10.0,
        },
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 5;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "volume_not_confirmed");
}
#[test]
fn entry_confirmation_still_requires_latest_volume_for_breakout() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        candle(0, 100.0, 10.0),
        candle(MS_15M, 101.0, 10.0),
        candle(MS_15M * 2, 102.0, 20.0),
        BacktestCandle {
            ts: MS_15M * 3,
            open: 102.0,
            high: 103.4,
            low: 101.8,
            close: 103.0,
            volume: 30.0,
        },
        BacktestCandle {
            ts: MS_15M * 4,
            open: 103.1,
            high: 106.0,
            low: 103.0,
            close: 105.0,
            volume: 10.0,
        },
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 5;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "volume_not_confirmed");
}
#[test]
fn parses_entry_gap_without_retest_controls() {
    let args = parse_cli_args_from([
        "--entry-max-gap-without-retest-pct",
        "0.8",
        "--entry-retest-tolerance-pct",
        "0.3",
    ])
    .unwrap();
    assert_eq!(args.entry_max_gap_without_retest_pct, Some(0.8));
    assert_eq!(args.entry_retest_tolerance_pct, 0.3);
}
#[test]
fn evaluate_events_blocks_large_entry_gap_without_known_retest() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_max_gap_without_retest_pct: Some(0.8),
        entry_retest_tolerance_pct: 0.3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5 + 1);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        candle(base_ts, 100.0, 10.0),
        candle(base_ts + MS_15M, 101.0, 10.0),
        candle(base_ts + MS_15M * 2, 102.0, 10.0),
        ohlc(base_ts + MS_15M * 3, 101.5, 102.4, 101.0, 102.0),
        ohlc(base_ts + MS_15M * 4, 102.0, 106.0, 101.8, 105.0),
        ohlc(base_ts + MS_15M * 5, 105.0, 106.0, 104.8, 105.5),
        ohlc(base_ts + MS_15M * 6, 106.5, 108.0, 106.0, 107.0),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.confirmed[0].entry_ts, base_ts + MS_15M * 5 + 1);
    assert_eq!(report.confirmed[0].entry_price, 105.0);
}
#[test]
fn evaluate_events_allows_large_entry_gap_after_known_retest() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_max_gap_without_retest_pct: Some(0.8),
        entry_retest_tolerance_pct: 0.3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5 + 1);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        candle(base_ts, 100.0, 10.0),
        candle(base_ts + MS_15M, 101.0, 10.0),
        candle(base_ts + MS_15M * 2, 102.0, 10.0),
        ohlc(base_ts + MS_15M * 3, 101.5, 102.4, 101.0, 102.0),
        ohlc(base_ts + MS_15M * 4, 102.0, 106.0, 101.8, 105.0),
        ohlc(base_ts + MS_15M * 5, 105.0, 106.0, 102.6, 103.0),
        ohlc(base_ts + MS_15M * 6, 106.5, 108.0, 106.0, 107.0),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.confirmed[0].entry_ts, base_ts + MS_15M * 5 + 1);
    assert_eq!(report.confirmed[0].entry_price, 105.0);
}
#[test]
fn evaluate_events_blocks_entry_when_signal_pullback_is_too_deep() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_max_signal_pullback_pct: Some(3.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5 + 1);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        candle(base_ts, 100.0, 10.0),
        candle(base_ts + MS_15M, 101.0, 10.0),
        candle(base_ts + MS_15M * 2, 102.0, 10.0),
        ohlc(base_ts + MS_15M * 3, 101.5, 102.4, 101.0, 102.0),
        ohlc(base_ts + MS_15M * 4, 102.0, 106.0, 101.8, 105.0),
        ohlc(base_ts + MS_15M * 5, 104.8, 105.2, 104.0, 104.6),
        ohlc(base_ts + MS_15M * 6, 100.0, 101.0, 99.0, 100.5),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.confirmed[0].entry_ts, base_ts + MS_15M * 5 + 1);
    assert_eq!(report.confirmed[0].entry_price, 105.0);
}
#[test]
fn evaluate_events_allows_entry_when_signal_pullback_stays_within_limit() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_max_signal_pullback_pct: Some(3.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5 + 1);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        candle(base_ts, 100.0, 10.0),
        candle(base_ts + MS_15M, 101.0, 10.0),
        candle(base_ts + MS_15M * 2, 102.0, 10.0),
        ohlc(base_ts + MS_15M * 3, 101.5, 102.4, 101.0, 102.0),
        ohlc(base_ts + MS_15M * 4, 102.0, 106.0, 101.8, 105.0),
        ohlc(base_ts + MS_15M * 5, 104.8, 105.2, 104.0, 104.6),
        ohlc(base_ts + MS_15M * 6, 103.0, 104.0, 102.0, 103.5),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.confirmed[0].entry_ts, base_ts + MS_15M * 5 + 1);
    assert_eq!(report.confirmed[0].entry_price, 105.0);
}
#[test]
fn evaluate_events_waits_for_breakout_retest_after_signal() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_retest_after_signal: true,
        entry_retest_max_wait_candles: 6,
        entry_retest_tolerance_pct: 0.3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        ohlc(base_ts, 100.0, 101.0, 99.5, 100.5),
        ohlc(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5),
        ohlc(base_ts + MS_15M * 2, 101.5, 103.0, 101.0, 102.5),
        ohlc(base_ts + MS_15M * 3, 102.5, 104.0, 102.0, 103.0),
        ohlc(base_ts + MS_15M * 4, 103.1, 106.0, 103.0, 105.0),
        ohlc(base_ts + MS_15M * 5, 106.2, 107.0, 105.5, 106.3),
        ohlc(base_ts + MS_15M * 6, 104.1, 106.4, 103.8, 106.0),
        ohlc(base_ts + MS_15M * 7, 106.1, 107.0, 105.8, 106.7),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert!(report.confirmed.is_empty());
    assert_eq!(report.stage_counts.get("trend_pass"), Some(&1));
    assert_eq!(report.stage_counts.get("entry_signal_pass"), Some(&1));
    assert_eq!(report.stage_counts.get("entry_execution_blocked"), Some(&1));
    assert_eq!(report.stage_counts.get("entry_blocked"), Some(&1));
}

#[test]
fn evaluate_events_counts_retest_shell_failure_after_signal_confirmation() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_retest_after_signal: true,
        entry_retest_max_wait_candles: 2,
        entry_retest_tolerance_pct: 0.3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        ohlc(base_ts, 100.0, 101.0, 99.5, 100.5),
        ohlc(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5),
        ohlc(base_ts + MS_15M * 2, 101.5, 103.0, 101.0, 102.5),
        ohlc(base_ts + MS_15M * 3, 102.5, 104.0, 102.0, 103.0),
        ohlc(base_ts + MS_15M * 4, 103.1, 106.0, 103.0, 105.0),
        ohlc(base_ts + MS_15M * 5, 106.2, 107.0, 105.5, 106.3),
        ohlc(base_ts + MS_15M * 6, 106.4, 107.2, 106.1, 106.8),
        ohlc(base_ts + MS_15M * 7, 106.7, 107.4, 106.4, 107.0),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert!(report.confirmed.is_empty());
    assert_eq!(report.stage_counts.get("trend_pass"), Some(&1));
    assert_eq!(report.stage_counts.get("entry_signal_pass"), Some(&1));
    assert_eq!(report.stage_counts.get("entry_execution_blocked"), Some(&1));
    assert_eq!(report.stage_counts.get("entry_blocked"), Some(&1));
    assert_eq!(
        report
            .blockers
            .get("ETH-USDT-SWAP")
            .and_then(|reasons| reasons.get("entry_retest_no_pullback_confirmation")),
        Some(&1)
    );
}
#[test]
fn evaluate_events_blocks_retest_entry_when_next_open_fades_confirmation() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_retest_after_signal: true,
        entry_retest_max_wait_candles: 6,
        entry_retest_tolerance_pct: 0.3,
        entry_retest_min_entry_open_gap_pct: Some(0.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        ohlc(base_ts, 100.0, 101.0, 99.5, 100.5),
        ohlc(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5),
        ohlc(base_ts + MS_15M * 2, 101.5, 103.0, 101.0, 102.5),
        ohlc(base_ts + MS_15M * 3, 102.5, 104.0, 102.0, 103.0),
        ohlc(base_ts + MS_15M * 4, 103.1, 106.0, 103.0, 105.0),
        ohlc(base_ts + MS_15M * 5, 106.2, 107.0, 105.5, 106.3),
        ohlc(base_ts + MS_15M * 6, 104.1, 106.4, 103.8, 106.0),
        ohlc(base_ts + MS_15M * 7, 105.9, 107.0, 105.8, 106.7),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert!(report.confirmed.is_empty());
    assert_eq!(report.stage_counts.get("entry_blocked"), Some(&1));
    assert_eq!(
        report
            .blockers
            .get("ETH-USDT-SWAP")
            .and_then(|reasons| reasons.get("entry_retest_no_pullback_confirmation")),
        Some(&1)
    );
}
#[test]
fn evaluate_events_allows_retest_entry_open_fade_with_volume_rescue() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_retest_after_signal: true,
        entry_retest_max_wait_candles: 6,
        entry_retest_tolerance_pct: 0.3,
        entry_retest_min_entry_open_gap_pct: Some(0.0),
        entry_retest_open_fade_min_volume_ratio: Some(2.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        ohlc(base_ts, 100.0, 101.0, 99.5, 100.5),
        ohlc(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5),
        ohlc(base_ts + MS_15M * 2, 101.5, 103.0, 101.0, 102.5),
        ohlc(base_ts + MS_15M * 3, 102.5, 104.0, 102.0, 103.0),
        ohlc(base_ts + MS_15M * 4, 103.1, 106.0, 103.0, 105.0),
        ohlc(base_ts + MS_15M * 5, 106.2, 107.0, 105.5, 106.3),
        BacktestCandle {
            ts: base_ts + MS_15M * 6,
            open: 104.1,
            high: 106.4,
            low: 103.8,
            close: 106.0,
            volume: 30.0,
        },
        ohlc(base_ts + MS_15M * 7, 105.9, 107.0, 105.8, 106.7),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert!(report.confirmed.is_empty());
    assert_eq!(report.stage_counts.get("entry_blocked"), Some(&1));
    assert_eq!(
        report
            .blockers
            .get("ETH-USDT-SWAP")
            .and_then(|reasons| reasons.get("entry_retest_no_pullback_confirmation")),
        Some(&1)
    );
}
#[test]
fn evaluate_events_blocks_retest_entry_open_fade_when_volume_rescue_is_too_small() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_retest_after_signal: true,
        entry_retest_max_wait_candles: 6,
        entry_retest_tolerance_pct: 0.3,
        entry_retest_min_entry_open_gap_pct: Some(0.0),
        entry_retest_open_fade_min_volume_ratio: Some(2.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_ok_4h_candles();
    let raw_15m = vec![
        ohlc(base_ts, 100.0, 101.0, 99.5, 100.5),
        ohlc(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5),
        ohlc(base_ts + MS_15M * 2, 101.5, 103.0, 101.0, 102.5),
        ohlc(base_ts + MS_15M * 3, 102.5, 104.0, 102.0, 103.0),
        ohlc(base_ts + MS_15M * 4, 103.1, 106.0, 103.0, 105.0),
        ohlc(base_ts + MS_15M * 5, 106.2, 107.0, 105.5, 106.3),
        BacktestCandle {
            ts: base_ts + MS_15M * 6,
            open: 104.1,
            high: 106.4,
            low: 103.8,
            close: 106.0,
            volume: 15.0,
        },
        ohlc(base_ts + MS_15M * 7, 105.9, 107.0, 105.8, 106.7),
    ];
    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );
    assert!(report.confirmed.is_empty());
    assert_eq!(report.stage_counts.get("entry_blocked"), Some(&1));
    assert_eq!(
        report
            .blockers
            .get("ETH-USDT-SWAP")
            .and_then(|reasons| reasons.get("entry_retest_no_pullback_confirmation")),
        Some(&1)
    );
}
#[test]
fn trend_confirmation_blocks_weak_4h_average_distance_when_required() {
    let mut candles = Vec::new();
    for index in 0..20 {
        candles.push(candle(MS_4H * index, 100.0, 10.0));
    }
    candles.push(candle(MS_4H * 20, 100.2, 10.0));
    let computed = build_computed_candles(candles, 20);
    let args = MarketVelocityEventBacktestArgs {
        trend_min_average_distance_pct: 0.5,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let (ok, reason) = trend_confirmation(
        &computed,
        MS_4H * 21 + MS_15M,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "weak_4h_average_distance");
}
#[test]
fn trend_confirmation_accepts_short_trend_below_averages() {
    let candles = vec![
        candle(0, 105.0, 10.0),
        candle(MS_4H, 104.0, 10.0),
        candle(MS_4H * 2, 103.0, 10.0),
        candle(MS_4H * 3, 99.0, 10.0),
    ];
    let computed = build_computed_candles(candles, 3);
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let (ok, reason) = trend_confirmation(
        &computed,
        MS_4H * 4 + MS_15M,
        MarketVelocityTradeDirection::Short,
        &args,
    );
    assert!(ok);
    assert_eq!(reason, "4h_below_below");
}
#[test]
fn evaluate_events_can_skip_higher_timeframe_trend_for_fast_15m_momentum() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.0,
        trend_timeframe: MarketVelocityTrendTimeframe::Off,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_blocking_4h_candles();
    let raw_15m = momentum_15m_entry_candles(base_ts);

    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), Vec::new())]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );

    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.stage_counts.get("trend_pass"), Some(&1));
    assert_eq!(report.stage_counts.get("trend_blocked"), None);
}
#[test]
fn evaluate_events_can_use_1h_trend_instead_of_4h_trend() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 20.0,
        entry_min_volume_ratio: 1.0,
        trend_timeframe: MarketVelocityTrendTimeframe::OneHour,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let base_ts = MS_4H * 4;
    let event = radar_event_at(base_ts + MS_15M * 5);
    let raw_4h = trend_blocking_4h_candles();
    let raw_1h = trend_ok_1h_candles(base_ts);
    let raw_15m = momentum_15m_entry_candles(base_ts);

    let report = evaluate_events(
        &[event],
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_4h.clone(), 3),
        )]),
        &HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            build_computed_candles(raw_15m.clone(), 3),
        )]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_4h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_1h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );

    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.stage_counts.get("trend_pass"), Some(&1));
    assert_eq!(report.stage_counts.get("trend_blocked"), None);
}
