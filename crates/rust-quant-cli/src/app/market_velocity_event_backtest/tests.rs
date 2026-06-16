use super::*;

fn candle(ts: i64, close: f64, volume: f64) -> BacktestCandle {
    BacktestCandle {
        ts,
        open: close - 0.5,
        high: close + 0.5,
        low: close - 1.0,
        close,
        volume,
    }
}

fn ohlc(ts: i64, open: f64, high: f64, low: f64, close: f64) -> BacktestCandle {
    BacktestCandle {
        ts,
        open,
        high,
        low,
        close,
        volume: 10.0,
    }
}

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
    let (ok, reason) = entry_confirmation(&computed, event_ts, &args);

    assert!(ok);
    assert_eq!(reason, "breakout_previous_high");
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

    let (ok, reason) = trend_confirmation(&computed, MS_4H * 21 + MS_15M, &args);

    assert!(!ok);
    assert_eq!(reason, "weak_4h_average_distance");
}

#[test]
fn simulate_trade_treats_same_candle_stop_and_target_as_loss() {
    let candles = vec![BacktestCandle {
        ts: MS_15M,
        open: 100.0,
        high: 104.0,
        low: 97.0,
        close: 101.0,
        volume: 10.0,
    }];

    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        0.02,
        1.5,
        MS_15M * 4,
        None,
        None,
    );

    assert_eq!(result.outcome, TradeOutcome::Loss);
    assert_eq!(result.reason, "both_hit_stop_first");
    assert_eq!(result.r, Some(-1.0));
    assert!(result.complete);
}

#[test]
fn simulate_trade_can_protect_profit_after_threshold_is_reached() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 103.0,
            low: 99.5,
            close: 102.5,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 102.5,
            high: 103.0,
            low: 100.9,
            close: 101.0,
            volume: 10.0,
        },
    ];

    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        0.02,
        2.0,
        MS_15M * 4,
        Some(ProfitProtection {
            activate_after_r: 1.0,
            stop_r: 0.5,
        }),
        None,
    );

    assert_eq!(result.outcome, TradeOutcome::Win);
    assert_eq!(result.reason, "profit_protect_stop_hit");
    assert_eq!(result.r, Some(0.5));
    assert!(result.complete);
}

#[test]
fn simulate_trade_reports_flat_when_breakeven_protection_is_hit() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 102.5,
            low: 99.5,
            close: 102.0,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 102.0,
            high: 102.2,
            low: 99.8,
            close: 100.2,
            volume: 10.0,
        },
    ];

    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        0.02,
        2.0,
        MS_15M * 4,
        Some(ProfitProtection {
            activate_after_r: 1.0,
            stop_r: 0.0,
        }),
        None,
    );

    assert_eq!(result.outcome, TradeOutcome::Flat);
    assert_eq!(result.reason, "profit_protect_stop_hit");
    assert_eq!(result.r, Some(0.0));
    assert!(result.complete);
}

#[test]
fn simulate_trade_can_take_partial_profit_and_hit_runner_target() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 104.0,
            low: 99.0,
            close: 103.5,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 103.5,
            high: 108.0,
            low: 103.0,
            close: 107.0,
            volume: 10.0,
        },
    ];

    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        0.02,
        2.0,
        MS_15M * 4,
        None,
        Some(RunnerExit {
            target_r: 4.0,
            fraction: 0.5,
            stop_r: 0.0,
        }),
    );

    assert_eq!(result.outcome, TradeOutcome::Win);
    assert_eq!(result.reason, "runner_target_hit");
    assert_eq!(result.r, Some(3.0));
    assert!(result.complete);
}

#[test]
fn simulate_trade_keeps_partial_profit_when_runner_stop_is_hit() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 104.0,
            low: 99.0,
            close: 103.5,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 103.5,
            high: 104.0,
            low: 99.8,
            close: 100.5,
            volume: 10.0,
        },
    ];

    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        0.02,
        2.0,
        MS_15M * 4,
        None,
        Some(RunnerExit {
            target_r: 4.0,
            fraction: 0.5,
            stop_r: 0.0,
        }),
    );

    assert_eq!(result.outcome, TradeOutcome::Win);
    assert_eq!(result.reason, "runner_stop_hit");
    assert_eq!(result.r, Some(1.0));
    assert!(result.complete);
}

#[test]
fn summarize_target_can_reenter_after_stop_on_breakout_reclaim() {
    let args = MarketVelocityEventBacktestArgs {
        stop_reentry_mode: StopReentryMode::BreakoutReclaim,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let confirmed = vec![ConfirmedEvent {
        event: RadarEvent {
            id: 88,
            exchange: "okx".to_string(),
            symbol: "BSB-USDT-SWAP".to_string(),
            ts: MS_15M / 2,
            detected_at: "2026-06-15 06:15:26+00".to_string(),
            new_rank: 23,
            delta_rank: 13,
            current_price: 102.0,
            price_change_pct: 17.0,
        },
        entry_ts: MS_15M,
        entry_price: 102.0,
        entry_idx: 1,
        trigger: "breakout_previous_high".to_string(),
    }];
    let candles = HashMap::from([(
        "BSB-USDT-SWAP".to_string(),
        vec![
            BacktestCandle {
                ts: 0,
                open: 100.0,
                high: 103.0,
                low: 99.0,
                close: 102.5,
                volume: 10.0,
            },
            BacktestCandle {
                ts: MS_15M,
                open: 102.0,
                high: 102.6,
                low: 100.5,
                close: 101.0,
                volume: 10.0,
            },
            BacktestCandle {
                ts: MS_15M * 2,
                open: 101.0,
                high: 101.2,
                low: 98.0,
                close: 99.0,
                volume: 10.0,
            },
            BacktestCandle {
                ts: MS_15M * 3,
                open: 99.0,
                high: 100.0,
                low: 94.0,
                close: 97.0,
                volume: 10.0,
            },
            BacktestCandle {
                ts: MS_15M * 4,
                open: 99.0,
                high: 104.0,
                low: 98.5,
                close: 103.5,
                volume: 10.0,
            },
            BacktestCandle {
                ts: MS_15M * 5,
                open: 103.6,
                high: 104.2,
                low: 103.0,
                close: 104.0,
                volume: 10.0,
            },
            BacktestCandle {
                ts: MS_15M * 6,
                open: 104.0,
                high: 109.0,
                low: 103.5,
                close: 108.0,
                volume: 10.0,
            },
        ],
    )]);

    let (results, skipped_lock) = summarize_target(&confirmed, &candles, 1.5, MS_15M * 12, &args);

    assert_eq!(skipped_lock, 0);
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result.outcome, TradeOutcome::Win);
    assert_eq!(result.reason, "stop_reentry_target_hit");
    assert_eq!(result.r, Some(0.5));
    assert_eq!(result.entry_ts, MS_15M * 5);
    assert_eq!(result.entry_price, 103.6);
    assert_eq!(
        result.trigger.as_deref(),
        Some("breakout_previous_high+stop_reentry_breakout_reclaim")
    );
    let reentry = result.reentry.as_ref().expect("reentry details");
    assert_eq!(reentry.original_entry_ts, MS_15M);
    assert_eq!(reentry.original_exit_ts, MS_15M * 2);
    assert_eq!(reentry.signal_ts, MS_15M * 4);
    assert_eq!(reentry.reclaim_price, 103.0);
}

#[test]
fn event_backtest_defaults_match_production_market_velocity_policy() {
    let args = MarketVelocityEventBacktestArgs::default();

    assert_eq!(args.stop_loss_pct, 0.03);
    assert_eq!(args.target_rs, vec![1.5, 2.0]);
    assert_eq!(args.min_delta_rank, 10);
    assert_eq!(args.max_new_rank, 30);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_timing_v1"
    );
    assert_eq!(args.stop_reentry_mode, StopReentryMode::Off);
}

#[test]
fn parses_paper_outcome_sink_and_entry_rule_version() {
    let args = parse_cli_args_from([
        "--paper-outcome-sink",
        "jsonl",
        "--paper-outcome-entry-rule-version",
        "rank_radar_4h_15m_v2",
    ])
    .unwrap();

    assert_eq!(
        args.paper_outcome_sink,
        MarketVelocityPaperOutcomeSink::Jsonl
    );
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_15m_v2"
    );
}

#[test]
fn parses_stop_reentry_mode() {
    let args = parse_cli_args_from(["--stop-reentry-mode", "breakout_reclaim"]).unwrap();

    assert_eq!(args.stop_reentry_mode, StopReentryMode::BreakoutReclaim);
}

#[test]
fn parses_profit_protection_controls() {
    let args = parse_cli_args_from([
        "--profit-protect-after-r",
        "1.2",
        "--profit-protect-stop-r",
        "0.3",
    ])
    .unwrap();

    assert_eq!(args.profit_protect_after_r, Some(1.2));
    assert_eq!(args.profit_protect_stop_r, 0.3);
}

#[test]
fn parses_runner_exit_controls() {
    let args = parse_cli_args_from([
        "--runner-target-r",
        "4.0",
        "--runner-fraction",
        "0.5",
        "--runner-stop-r",
        "0.0",
    ])
    .unwrap();

    assert_eq!(args.runner_target_r, Some(4.0));
    assert_eq!(args.runner_fraction, 0.5);
    assert_eq!(args.runner_stop_r, 0.0);
}

#[test]
fn rejects_runner_fraction_outside_position_share() {
    let err =
        parse_cli_args_from(["--runner-target-r", "4.0", "--runner-fraction", "1.0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("--runner-fraction must be greater than 0 and lower than 1"));
}

#[test]
fn rejects_profit_protection_stop_at_or_above_activation() {
    let err = parse_cli_args_from([
        "--profit-protect-after-r",
        "1.0",
        "--profit-protect-stop-r",
        "1.0",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--profit-protect-stop-r must be lower than --profit-protect-after-r"));
}

#[test]
fn parses_fvg_entry_mode_and_wait_controls() {
    let args = parse_cli_args_from([
        "--fvg-entry-mode",
        "15m_to_1h",
        "--fvg-lookback-candles",
        "12",
        "--fvg-max-wait-candles",
        "8",
    ])
    .unwrap();

    assert_eq!(args.fvg_entry_mode, FvgEntryMode::M15To1h);
    assert_eq!(args.fvg_lookback_candles, 12);
    assert_eq!(args.fvg_max_wait_candles, 8);
}

#[test]
fn evaluate_events_uses_15m_pullback_into_1h_bullish_fvg() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        fvg_entry_mode: FvgEntryMode::M15To1h,
        fvg_lookback_candles: 8,
        fvg_max_wait_candles: 8,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let event = radar_event_at(MS_4H * 4 + MS_15M);
    let raw_4h = trend_ok_4h_candles();
    let raw_1h = vec![
        ohlc(MS_1H * 11, 99.0, 100.0, 98.0, 99.5),
        ohlc(MS_1H * 12, 101.0, 102.0, 100.5, 101.5),
        ohlc(MS_1H * 13, 103.5, 105.0, 103.0, 104.0),
        ohlc(MS_1H * 14, 104.5, 106.0, 104.0, 105.0),
        ohlc(MS_1H * 15, 105.0, 106.5, 104.1, 105.5),
    ];
    let raw_15m = vec![
        ohlc(MS_1H * 14, 105.0, 106.0, 104.0, 105.5),
        ohlc(MS_1H * 14 + MS_15M, 105.5, 106.0, 104.2, 105.6),
        ohlc(MS_1H * 14 + MS_15M * 2, 105.6, 106.2, 104.2, 105.8),
        ohlc(MS_1H * 14 + MS_15M * 3, 105.8, 106.4, 104.1, 106.0),
        ohlc(MS_1H * 15, 106.0, 106.6, 104.1, 105.8),
        ohlc(MS_1H * 15 + MS_15M, 105.8, 106.2, 104.2, 105.9),
        ohlc(MS_1H * 15 + MS_15M * 2, 105.9, 106.4, 104.1, 106.1),
        ohlc(MS_1H * 15 + MS_15M * 3, 106.1, 106.5, 104.2, 106.2),
        ohlc(MS_1H * 16, 106.0, 106.4, 104.2, 106.1),
        ohlc(MS_1H * 16 + MS_15M, 101.5, 103.0, 100.5, 102.6),
        ohlc(MS_1H * 16 + MS_15M * 2, 102.7, 104.0, 102.4, 103.5),
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
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_1h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );

    assert_eq!(report.confirmed.len(), 1);
    let confirmed = &report.confirmed[0];
    assert_eq!(confirmed.entry_ts, MS_1H * 16 + MS_15M * 2);
    assert_eq!(confirmed.entry_price, 102.7);
    assert_eq!(confirmed.trigger, "fvg_15m_to_1h");
}

#[test]
fn evaluate_events_uses_1h_pullback_into_4h_bullish_fvg() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        fvg_entry_mode: FvgEntryMode::H1To4h,
        fvg_lookback_candles: 8,
        fvg_max_wait_candles: 8,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let event = radar_event_at(MS_4H * 4 + MS_15M);
    let raw_4h = vec![
        ohlc(0, 98.0, 100.0, 97.0, 99.0),
        ohlc(MS_4H, 101.0, 102.0, 100.5, 101.5),
        ohlc(MS_4H * 2, 104.0, 106.0, 103.0, 105.0),
        ohlc(MS_4H * 3, 105.0, 108.0, 104.0, 107.0),
    ];
    let raw_1h = vec![
        ohlc(MS_4H * 3, 105.0, 106.0, 104.0, 105.5),
        ohlc(MS_4H * 3 + MS_1H, 105.5, 106.0, 104.1, 105.6),
        ohlc(MS_4H * 3 + MS_1H * 2, 105.6, 106.2, 104.2, 105.8),
        ohlc(MS_4H * 3 + MS_1H * 3, 105.8, 106.4, 104.1, 106.0),
        ohlc(MS_4H * 4, 106.0, 106.5, 104.2, 106.2),
        ohlc(MS_4H * 4 + MS_1H, 101.5, 103.0, 100.5, 102.6),
        ohlc(MS_4H * 4 + MS_1H * 2, 102.7, 104.0, 102.4, 103.5),
    ];
    let raw_15m = vec![
        ohlc(MS_4H * 4 + MS_1H * 2, 102.7, 103.5, 102.4, 103.0),
        ohlc(MS_4H * 4 + MS_1H * 2 + MS_15M, 103.0, 104.0, 102.8, 103.8),
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
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_1h)]),
        &HashMap::from([("ETH-USDT-SWAP".to_string(), raw_15m)]),
        &args,
    );

    assert_eq!(report.confirmed.len(), 1);
    let confirmed = &report.confirmed[0];
    assert_eq!(confirmed.entry_ts, MS_4H * 4 + MS_1H * 2);
    assert_eq!(confirmed.entry_price, 102.7);
    assert_eq!(confirmed.trigger, "fvg_1h_to_4h");
}

#[test]
fn web_sink_requires_explicit_rule_version_for_stop_reentry_mode() {
    let err = parse_cli_args_from([
        "--paper-outcome-sink",
        "web",
        "--stop-reentry-mode",
        "breakout_reclaim",
    ])
    .unwrap_err();

    assert!(err.to_string().contains(
        "--stop-reentry-mode with --paper-outcome-sink web requires explicit --paper-outcome-entry-rule-version"
    ));
}

#[test]
fn default_analysis_backtest_keeps_entry_trigger_filter_unset() {
    let args = parse_cli_args_from([] as [&str; 0]).unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Off);
    assert!(args.entry_trigger_allowlist.is_empty());
    assert!(args.entry_trigger_blocklist.is_empty());
}

#[test]
fn web_paper_outcome_sink_defaults_to_production_entry_trigger_allowlist() {
    let args = parse_cli_args_from(["--paper-outcome-sink", "web"]).unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert!(args.entry_trigger_blocklist.is_empty());
}

#[test]
fn explicit_all_entry_trigger_allowlist_keeps_web_paper_outcome_sink_unfiltered() {
    let args = parse_cli_args_from([
        "--paper-outcome-sink",
        "web",
        "--entry-trigger-allowlist",
        "all",
    ])
    .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert!(args.entry_trigger_allowlist.is_empty());
    assert!(args.entry_trigger_blocklist.is_empty());
}

#[test]
fn paper_observation_args_force_web_sink_and_production_entry_trigger_allowlist() {
    let args = parse_paper_observation_args_from([] as [&str; 0]).unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert!(args.entry_trigger_blocklist.is_empty());
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_timing_v1"
    );
}

#[test]
fn paper_observation_args_apply_stop_reentry_profit_preset() {
    let args =
        parse_paper_observation_args_from(["--paper-strategy-preset", "stop_reentry_025sl_24r_v1"])
            .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(args.stop_reentry_mode, StopReentryMode::BreakoutReclaim);
    assert_eq!(args.stop_loss_pct, 0.025);
    assert_eq!(args.target_rs, vec![2.4]);
    assert_eq!(
        args.paper_outcome_entry_rule_version,
        "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1"
    );
    assert_eq!(args.profit_protect_after_r, None);
    assert_eq!(args.runner_target_r, None);
}

#[test]
fn paper_observation_args_reject_unknown_strategy_preset() {
    let err =
        parse_paper_observation_args_from(["--paper-strategy-preset", "unknown"]).unwrap_err();

    assert!(err.to_string().contains("unknown --paper-strategy-preset"));
}

#[test]
fn paper_observation_args_reject_preset_target_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "stop_reentry_025sl_24r_v1",
        "--target-rs",
        "2.0",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --target-rs"));
}

#[test]
fn paper_observation_args_reject_preset_stop_override() {
    let err = parse_paper_observation_args_from([
        "--paper-strategy-preset",
        "stop_reentry_025sl_24r_v1",
        "--stop-loss-pct",
        "0.03",
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("--paper-strategy-preset locks --stop-loss-pct"));
}

#[test]
fn paper_observation_args_reject_entry_trigger_filter_overrides() {
    let err = parse_paper_observation_args_from(["--entry-trigger-allowlist", "all"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --entry-trigger-allowlist"));
}

#[test]
fn paper_observation_args_reject_stop_reentry_mode() {
    let err =
        parse_paper_observation_args_from(["--stop-reentry-mode", "breakout_reclaim"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --stop-reentry-mode"));
}

#[test]
fn paper_observation_args_reject_fvg_entry_mode() {
    let err = parse_paper_observation_args_from(["--fvg-entry-mode", "15m_to_1h"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --fvg-entry-mode"));
}

#[test]
fn paper_observation_args_reject_profit_protection() {
    let err = parse_paper_observation_args_from(["--profit-protect-after-r", "1.0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --profit-protect-after-r"));
}

#[test]
fn paper_observation_args_reject_runner_exit() {
    let err = parse_paper_observation_args_from(["--runner-target-r", "4.0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("market_velocity_paper_observation owns --runner-target-r"));
}

#[test]
fn paper_observation_args_keep_backtest_tunables() {
    let args =
        parse_paper_observation_args_from(["--target-rs", "2.0", "--stop-loss-pct", "0.025"])
            .unwrap();

    assert_eq!(args.paper_outcome_sink, MarketVelocityPaperOutcomeSink::Web);
    assert_eq!(args.entry_trigger_allowlist, vec!["breakout_previous_high"]);
    assert_eq!(args.target_rs, vec![2.0]);
    assert_eq!(args.stop_loss_pct, 0.025);
}

#[test]
fn paper_observation_command_defaults_to_one_shot() {
    let command = parse_paper_observation_command_from([] as [&str; 0]).unwrap();

    assert_eq!(
        command.backtest_args.paper_outcome_sink,
        MarketVelocityPaperOutcomeSink::Web
    );
    assert_eq!(
        command.backtest_args.entry_trigger_allowlist,
        vec!["breakout_previous_high"]
    );
    assert_eq!(command.loop_interval_seconds, None);
}

#[test]
fn paper_observation_command_parses_loop_interval_without_losing_tunables() {
    let command = parse_paper_observation_command_from([
        "--loop-interval-seconds",
        "21600",
        "--target-rs",
        "2.0",
        "--stop-loss-pct",
        "0.025",
    ])
    .unwrap();

    assert_eq!(command.loop_interval_seconds, Some(21_600));
    assert_eq!(command.backtest_args.target_rs, vec![2.0]);
    assert_eq!(command.backtest_args.stop_loss_pct, 0.025);
    assert_eq!(
        command.backtest_args.entry_trigger_allowlist,
        vec!["breakout_previous_high"]
    );
}

#[test]
fn paper_observation_command_rejects_zero_loop_interval() {
    let err = parse_paper_observation_command_from(["--loop-interval-seconds", "0"]).unwrap_err();

    assert!(err
        .to_string()
        .contains("--loop-interval-seconds must be greater than 0"));
}

#[test]
fn parses_entry_trigger_allowlist_and_blocklist() {
    let args = parse_cli_args_from([
        "--entry-trigger-allowlist",
        "breakout_previous_high,reclaim_ema",
        "--entry-trigger-blocklist",
        "pullback_hold_ema",
    ])
    .unwrap();

    assert_eq!(
        args.entry_trigger_allowlist,
        vec!["breakout_previous_high", "reclaim_ema"]
    );
    assert_eq!(args.entry_trigger_blocklist, vec!["pullback_hold_ema"]);
}

#[test]
fn filters_confirmed_events_by_entry_trigger_with_blocklist_precedence() {
    let args = MarketVelocityEventBacktestArgs {
        entry_trigger_allowlist: vec![
            "breakout_previous_high".to_string(),
            "reclaim_ema".to_string(),
        ],
        entry_trigger_blocklist: vec!["reclaim_ema".to_string()],
        ..MarketVelocityEventBacktestArgs::default()
    };
    let confirmed = vec![
        confirmed_event(1, "breakout_previous_high"),
        confirmed_event(2, "reclaim_ema"),
        confirmed_event(3, "pullback_hold_ema"),
    ];

    let filtered = filter_confirmed_events_by_entry_trigger(&confirmed, &args);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].event.id, 1);
    assert_eq!(filtered[0].trigger, "breakout_previous_high");
}

#[test]
fn builds_paper_outcomes_for_each_target_and_horizon_without_execution_task_payload() {
    let args = MarketVelocityEventBacktestArgs {
        stop_loss_pct: 0.02,
        target_rs: vec![1.5, 2.0],
        paper_outcome_entry_rule_version: "rank_radar_4h_15m_v2".to_string(),
        entry_trigger_allowlist: vec![
            "breakout_previous_high".to_string(),
            "reclaim_ema".to_string(),
        ],
        ..MarketVelocityEventBacktestArgs::default()
    };
    let confirmed = vec![ConfirmedEvent {
        event: RadarEvent {
            id: 77,
            exchange: "okx".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            ts: 0,
            detected_at: "2026-06-15 00:00:00+00".to_string(),
            new_rank: 18,
            delta_rank: 12,
            current_price: 100.0,
            price_change_pct: 3.5,
        },
        entry_ts: MS_15M,
        entry_price: 100.0,
        entry_idx: 0,
        trigger: "breakout_previous_high".to_string(),
    }];
    let candles = HashMap::from([(
        "ETH-USDT-SWAP".to_string(),
        vec![BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 104.0,
            low: 99.0,
            close: 103.0,
            volume: 10.0,
        }],
    )]);

    let outcomes = build_market_velocity_paper_outcomes(&confirmed, &candles, &args);

    assert_eq!(outcomes.len(), 4);
    let first = &outcomes[0];
    assert_eq!(first.rank_event_id, 77);
    assert_eq!(first.exchange, "okx");
    assert_eq!(first.symbol, "ETH-USDT-SWAP");
    assert_eq!(first.target_r, 1.5);
    assert_eq!(first.horizon_hours, 24);
    assert_eq!(first.entry_rule_version, "rank_radar_4h_15m_v2");
    assert_eq!(
        first.entry_trigger.as_deref(),
        Some("breakout_previous_high")
    );
    assert_eq!(first.entry_price, 100.0);
    assert_eq!(first.outcome_status, "win");
    assert_eq!(first.exit_reason, "target_hit");
    assert_eq!(first.result_r, Some(1.5));
    assert_eq!(
        first.evaluation_payload["source"],
        "market_velocity_event_backtest"
    );
    assert_eq!(first.evaluation_payload["stop_loss_pct"], 0.02);
    assert_eq!(
        first.evaluation_payload["entry_trigger_filter_version"],
        "entry_trigger_allowlist_v1"
    );
    assert_eq!(
        first.evaluation_payload["entry_filter"]["entry_trigger_filter_version"],
        "entry_trigger_allowlist_v1"
    );
    assert_eq!(
        first.evaluation_payload["entry_filter"]["entry_trigger_allowlist"],
        serde_json::json!(["breakout_previous_high", "reclaim_ema"])
    );

    let serialized = serde_json::to_string(first).unwrap();
    assert!(!serialized.contains("execution_task"));
    assert!(!serialized.contains("buyer_email"));
}

fn confirmed_event(id: i64, trigger: &str) -> ConfirmedEvent {
    ConfirmedEvent {
        event: RadarEvent {
            id,
            exchange: "okx".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            ts: 0,
            detected_at: "2026-06-15 00:00:00+00".to_string(),
            new_rank: 18,
            delta_rank: 12,
            current_price: 100.0,
            price_change_pct: 3.5,
        },
        entry_ts: MS_15M,
        entry_price: 100.0,
        entry_idx: 0,
        trigger: trigger.to_string(),
    }
}

fn radar_event_at(ts: i64) -> RadarEvent {
    RadarEvent {
        id: 99,
        exchange: "okx".to_string(),
        symbol: "ETH-USDT-SWAP".to_string(),
        ts,
        detected_at: "2026-06-15 00:00:00+00".to_string(),
        new_rank: 18,
        delta_rank: 12,
        current_price: 105.0,
        price_change_pct: 3.5,
    }
}

fn trend_ok_4h_candles() -> Vec<BacktestCandle> {
    vec![
        ohlc(0, 98.0, 99.0, 97.0, 98.5),
        ohlc(MS_4H, 99.0, 100.0, 98.0, 99.5),
        ohlc(MS_4H * 2, 100.0, 101.0, 99.0, 100.5),
        ohlc(MS_4H * 3, 104.0, 106.0, 103.0, 105.0),
    ]
}
