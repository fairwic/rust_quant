use super::*;

fn candle_entity(ts: i64) -> CandlesEntity {
    CandlesEntity {
        id: None,
        ts,
        o: "100".to_string(),
        h: "105".to_string(),
        l: "99".to_string(),
        c: "104".to_string(),
        vol: "10".to_string(),
        vol_ccy: "11".to_string(),
        confirm: "1".to_string(),
        created_at: None,
        updated_at: None,
    }
}

fn cli_scalper_impulse_pullback_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = (0..count)
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
        .collect::<Vec<_>>();
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

fn cli_scalper_short_window_impulse_pullback_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = (0..count)
        .map(|i| {
            let open = start + i as f64 * 2.0;
            CandleItem {
                o: open,
                h: open + 3.0,
                l: open - 3.0,
                c: open + 1.0,
                v: 2_000.0,
                ts: 1_783_000_000_000 + i as i64 * 60_000,
                confirm: 1,
            }
        })
        .collect::<Vec<_>>();
    let high_regime_start = count.saturating_sub(48);
    let short_cycle_start = count.saturating_sub(34);
    for (i, candle) in candles.iter_mut().enumerate() {
        if i >= high_regime_start && i < short_cycle_start {
            candle.o = start + 5_000.0 - (i - high_regime_start) as f64 * 8.0;
            candle.c = candle.o - 3.0;
            candle.h = candle.o + 5.0;
            candle.l = candle.c - 5.0;
        } else if i >= short_cycle_start {
            let open = start + (i - short_cycle_start) as f64 * 5.0;
            candle.o = open;
            candle.c = open + 2.0;
            candle.h = candle.c + 2.0;
            candle.l = candle.o - 2.0;
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

#[test]
fn parses_cli_defaults_and_limit() {
    let args = parse_args(Vec::<String>::new()).unwrap();

    assert_eq!(args.limit, DEFAULT_LIMIT);
    assert_eq!(args.risk_percent, 2.0);
    assert_eq!(args.trade_fee_rate, None);
    assert!(!args.debug_trades);
    assert!(!args.scan_breakdown);
    assert!(!args.scan_exhaustion);
    assert!(!args.scan_micro);
    assert!(!args.scan_scalper);
    assert!(!args.scan_scalper_narrow);
    assert!(!args.diagnose_scalper);
    assert!(!args.use_market_context);
    assert!(!args.backfill_okx_market_context);
    assert_eq!(args.case_label, None);

    let args = parse_args(["--limit".to_string(), "1000".to_string()]).unwrap();
    assert_eq!(args.limit, 1000);

    let args = parse_args(["--trade-fee-rate".to_string(), "0.0005".to_string()]).unwrap();
    assert_eq!(args.trade_fee_rate, Some(0.0005));

    let args = parse_args(["--debug-trades".to_string()]).unwrap();
    assert!(args.debug_trades);

    let args = parse_args(["--scan-exhaustion".to_string()]).unwrap();
    assert!(args.scan_exhaustion);

    let args = parse_args(["--scan-breakdown".to_string()]).unwrap();
    assert!(args.scan_breakdown);

    let args = parse_args(["--scan-scalper".to_string()]).unwrap();
    assert!(args.scan_scalper);

    let args = parse_args(["--scan-scalper-narrow".to_string()]).unwrap();
    assert!(args.scan_scalper_narrow);

    let args = parse_args(["--scan-micro".to_string()]).unwrap();
    assert!(args.scan_micro);

    let args = parse_args(["--diagnose-scalper".to_string()]).unwrap();
    assert!(args.diagnose_scalper);

    let args = parse_args(["--use-market-context".to_string()]).unwrap();
    assert!(args.use_market_context);

    let args = parse_args(["--backfill-okx-market-context".to_string()]).unwrap();
    assert!(args.backfill_okx_market_context);

    let args = parse_args(["--case-label".to_string(), "scalper_btc_1m".to_string()]).unwrap();
    assert_eq!(args.case_label.as_deref(), Some("scalper_btc_1m"));
}

#[test]
fn strategy_cases_include_1m_scalper_for_short_cycle_frequency() {
    let labels = strategy_cases()
        .iter()
        .map(|case| case.label)
        .collect::<Vec<_>>();

    assert!(labels.contains(&"scalper_btc_1m"));
    assert!(labels.contains(&"scalper_eth_1m"));
    assert!(labels.contains(&"micro_scalper_btc_1m"));
    assert!(labels.contains(&"micro_scalper_eth_1m"));
    assert!(labels.contains(&"breakdown_btc_5m"));
    assert!(labels.contains(&"breakdown_eth_5m"));
    assert!(labels.contains(&"exhaustion_btc_5m"));
    assert!(labels.contains(&"exhaustion_eth_5m"));
}

#[test]
fn micro_scalper_1m_runs_existing_backtest_pipeline() {
    let candles = cli_scalper_impulse_pullback_candles(560, 100_000.0);
    let result = run_micro_scalper_1m(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert!(result.open_trades > 0);
    assert!(!result.trade_records.is_empty());
}

#[test]
fn micro_scalper_scan_tunings_are_fee_aware_without_short_cycle_trade_cap() {
    let tunings = micro_scalper_scan_tunings();

    assert!(tunings.len() > 20);
    assert!(tunings.iter().any(|tuning| !tuning.allow_short));
    assert!(tunings.iter().any(|tuning| tuning.target_r_2 >= 2.5));
    assert!(tunings.iter().any(|tuning| tuning.cooldown_candles <= 4));
}

#[test]
fn strategy_family_risk_config_keeps_trade_fee_separate_from_funding() {
    let default_risk = strategy_family_risk_config(2.0, None);
    let explicit_fee_risk = strategy_family_risk_config(2.0, Some(0.0005));

    assert_eq!(default_risk.max_loss_percent, 2.0);
    assert_eq!(default_risk.trade_fee_rate, None);
    assert_eq!(explicit_fee_risk.max_loss_percent, 2.0);
    assert_eq!(explicit_fee_risk.trade_fee_rate, Some(0.0005));
}

#[test]
fn strategy_case_filter_keeps_only_requested_label() {
    let cases = strategy_cases_for_filter(Some("scalper_btc_1m"), false).unwrap();

    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].label, "scalper_btc_1m");
    assert!(strategy_cases_for_filter(Some("missing_case"), false).is_err());
}

#[test]
fn default_case_filter_excludes_failed_research_micro_scalper() {
    let default_labels = strategy_cases_for_filter(None, false)
        .unwrap()
        .into_iter()
        .map(|case| case.label)
        .collect::<Vec<_>>();
    let research_labels = strategy_cases_for_filter(None, true)
        .unwrap()
        .into_iter()
        .map(|case| case.label)
        .collect::<Vec<_>>();
    let explicit_micro = strategy_cases_for_filter(Some("micro_scalper_btc_1m"), false).unwrap();

    assert!(!default_labels.contains(&"micro_scalper_btc_1m"));
    assert!(!default_labels.contains(&"micro_scalper_eth_1m"));
    assert!(research_labels.contains(&"micro_scalper_btc_1m"));
    assert!(research_labels.contains(&"micro_scalper_eth_1m"));
    assert_eq!(explicit_micro[0].label, "micro_scalper_btc_1m");
}

#[test]
fn scalper_scan_tunings_cover_optional_oi_confirmation_filter() {
    let tunings = scalper_scan_tunings();

    assert!(tunings.iter().any(|tuning| tuning.require_oi_confirmation));
    assert!(tunings.iter().any(|tuning| !tuning.require_oi_confirmation));
}

#[test]
fn scalper_scan_tunings_cover_short_cycle_trend_windows() {
    let tunings = scalper_scan_tunings();

    assert!(tunings
        .iter()
        .any(|tuning| tuning.trend_fast_window == 13 && tuning.trend_slow_window == 34));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.trend_fast_window == 20 && tuning.trend_slow_window == 48));
}

#[test]
fn scalper_narrow_scan_tunings_stay_small_and_short_cycle_focused() {
    let tunings = scalper_narrow_scan_tunings();

    assert!(tunings.len() <= 128);
    assert!(tunings
        .iter()
        .all(|tuning| tuning.trend_fast_window == 13 && tuning.trend_slow_window == 34));
    assert!(tunings.iter().any(|tuning| tuning.allow_short));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.min_directional_ratio_48 < 0.25));
}

#[test]
fn scalper_raw_candidate_sort_prefers_frequency_before_pnl() {
    let tuning = BtcEthLiquidityScalperBacktestTuning::default();
    let mut candidates = vec![
        ScalperScanCandidateReport {
            tuning,
            entries: 0,
            wins: 0,
            losses: 0,
            win_rate_pct: 0.0,
            pnl: 0.0,
            max_drawdown_pct: 0.0,
            trades_per_day: 0.0,
            early_win_rate_pct: 0.0,
            early_pnl: 0.0,
            late_win_rate_pct: 0.0,
            late_pnl: 0.0,
            remove_top5_pnl: 0.0,
            filtered_reason_counts: Vec::new(),
        },
        ScalperScanCandidateReport {
            tuning,
            entries: 100,
            wins: 44,
            losses: 32,
            win_rate_pct: 57.89,
            pnl: -1.5,
            max_drawdown_pct: 1.7,
            trades_per_day: 7.14,
            early_win_rate_pct: 64.0,
            early_pnl: -0.7,
            late_win_rate_pct: 51.0,
            late_pnl: -0.8,
            remove_top5_pnl: -2.0,
            filtered_reason_counts: Vec::new(),
        },
    ];

    sort_scalper_raw_candidates(&mut candidates);

    assert_eq!(candidates[0].entries, 100);
}

#[test]
fn merge_filtered_reason_counts_orders_by_frequency() {
    let reports = vec![
        CaseReport {
            label: "a".to_string(),
            candles: 0,
            entries: 0,
            closed: 0,
            wins: 0,
            losses: 0,
            win_rate_pct: 0.0,
            pnl: 0.0,
            final_funds: 100.0,
            max_drawdown_pct: 0.0,
            days: 0.0,
            trades_per_day: 0.0,
            trades: Vec::new(),
            filtered_signals: 0,
            filtered_reason_counts: vec![("LOW".to_string(), 1), ("HIGH".to_string(), 3)],
        },
        CaseReport {
            label: "b".to_string(),
            candles: 0,
            entries: 0,
            closed: 0,
            wins: 0,
            losses: 0,
            win_rate_pct: 0.0,
            pnl: 0.0,
            final_funds: 100.0,
            max_drawdown_pct: 0.0,
            days: 0.0,
            trades_per_day: 0.0,
            trades: Vec::new(),
            filtered_signals: 0,
            filtered_reason_counts: vec![("LOW".to_string(), 2)],
        },
    ];

    assert_eq!(
        merge_filtered_reason_counts(&reports),
        vec![("HIGH".to_string(), 3), ("LOW".to_string(), 3)]
    );
}

#[test]
fn merge_filtered_reason_counts_excludes_confirmed_signal_metadata() {
    let reports = vec![CaseReport {
        label: "a".to_string(),
        candles: 0,
        entries: 0,
        closed: 0,
        wins: 0,
        losses: 0,
        win_rate_pct: 0.0,
        pnl: 0.0,
        final_funds: 100.0,
        max_drawdown_pct: 0.0,
        days: 0.0,
        trades_per_day: 0.0,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: vec![
            ("BTC_ETH_LIQUIDITY_SCALP_CONFIRMED".to_string(), 3),
            ("STOP_PRICE:100.0".to_string(), 3),
            ("OI_NOT_CONFIRMED_REDUCE_SIZE".to_string(), 2),
            ("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 5),
        ],
    }];

    assert_eq!(
        merge_filtered_reason_counts(&reports),
        vec![("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 5)]
    );
}

#[test]
fn scalper_filter_counts_ignore_non_scalper_baseline_reports() {
    let non_scalper = vec![CaseReport {
        label: "exhaustion_btc_5m".to_string(),
        candles: 0,
        entries: 0,
        closed: 0,
        wins: 0,
        losses: 0,
        win_rate_pct: 0.0,
        pnl: 0.0,
        final_funds: 100.0,
        max_drawdown_pct: 0.0,
        days: 0.0,
        trades_per_day: 0.0,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: vec![("EXHAUSTION_FADE_SHORT_V1_CONFIRMED".to_string(), 10)],
    }];
    let scalper = vec![CaseReport {
        label: "scalper_btc_1m".to_string(),
        candles: 0,
        entries: 0,
        closed: 0,
        wins: 0,
        losses: 0,
        win_rate_pct: 0.0,
        pnl: 0.0,
        final_funds: 100.0,
        max_drawdown_pct: 0.0,
        days: 0.0,
        trades_per_day: 0.0,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: vec![("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 3)],
    }];

    assert_eq!(
        scalper_filter_counts(&non_scalper, &scalper),
        vec![("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 3)]
    );
}

#[test]
fn scalper_candidate_summary_ignores_profitable_non_scalper_reports() {
    let non_scalper = vec![scan_case_report("breakdown_eth_5m", 20, 15, 5, 100.0)];
    let scalper = vec![scan_case_report("scalper_btc_1m", 4, 1, 3, -2.0)];

    let summary = summarize_scalper_candidate_reports(&non_scalper, &scalper);

    assert_eq!(summary.entries, 4);
    assert_eq!(summary.wins, 1);
    assert_eq!(summary.losses, 3);
    assert_eq!(summary.win_rate_pct, 25.0);
    assert_eq!(summary.pnl, -2.0);
}

#[test]
fn breakdown_candidate_summary_ignores_profitable_non_breakdown_reports() {
    let non_breakdown = vec![scan_case_report("exhaustion_eth_5m", 30, 20, 10, 80.0)];
    let breakdown = vec![scan_case_report("breakdown_btc_5m", 5, 2, 3, -1.5)];

    let summary = summarize_breakdown_candidate_reports(&non_breakdown, &breakdown);

    assert_eq!(summary.entries, 5);
    assert_eq!(summary.wins, 2);
    assert_eq!(summary.losses, 3);
    assert_eq!(summary.win_rate_pct, 40.0);
    assert_eq!(summary.pnl, -1.5);
}

#[test]
fn exhaustion_candidate_summary_ignores_profitable_non_exhaustion_reports() {
    let non_exhaustion = vec![scan_case_report("breakdown_eth_5m", 30, 20, 10, 80.0)];
    let exhaustion = vec![scan_case_report("exhaustion_btc_5m", 5, 2, 3, -1.5)];

    let summary = summarize_exhaustion_candidate_reports(&non_exhaustion, &exhaustion);

    assert_eq!(summary.entries, 5);
    assert_eq!(summary.wins, 2);
    assert_eq!(summary.losses, 3);
    assert_eq!(summary.win_rate_pct, 40.0);
    assert_eq!(summary.pnl, -1.5);
}

#[test]
fn scalper_setup_diagnostics_count_confirmed_and_failed_windows() {
    let diagnostics = scalper_setup_diagnostics(
        &cli_scalper_impulse_pullback_candles(560, 100_000.0),
        BtcEthLiquidityScalperBacktestTuning::default(),
    );

    assert!(diagnostics.samples > 0);
    assert!(diagnostics.confirmed > 0);
    assert_eq!(diagnostics.classified_windows(), diagnostics.samples);
}

#[test]
fn scalper_setup_diagnostics_respect_short_cycle_trend_windows() {
    let candles = cli_scalper_short_window_impulse_pullback_candles(560, 100_000.0);
    let default_diagnostics =
        scalper_setup_diagnostics(&candles, BtcEthLiquidityScalperBacktestTuning::default());
    let short_window_diagnostics = scalper_setup_diagnostics(
        &candles,
        BtcEthLiquidityScalperBacktestTuning {
            trend_fast_window: 13,
            trend_slow_window: 34,
            ..Default::default()
        },
    );

    assert_eq!(default_diagnostics.confirmed, 0);
    assert!(short_window_diagnostics.confirmed > 0);
}

#[test]
fn scalper_setup_diagnostics_explain_flat_market_rejections() {
    let candles = (0..560)
        .map(|i| CandleItem {
            o: 100.0,
            h: 100.5,
            l: 99.5,
            c: 100.0,
            v: 1_000.0,
            ts: 1_783_000_000_000 + i as i64 * 60_000,
            confirm: 1,
        })
        .collect::<Vec<_>>();
    let diagnostics =
        scalper_setup_diagnostics(&candles, BtcEthLiquidityScalperBacktestTuning::default());

    assert_eq!(diagnostics.confirmed, 0);
    assert!(diagnostics.reason_count("NO_TREND") > 0);
    assert_eq!(diagnostics.classified_windows(), diagnostics.samples);
}

#[test]
fn scalper_diagnostic_reasons_are_sorted_by_frequency() {
    let diagnostics = ScalperSetupDiagnostics {
        samples: 6,
        confirmed: 0,
        reasons: BTreeMap::from([("A_LOW", 1), ("Z_HIGH", 5)]),
    };

    assert_eq!(
        format_scalper_diagnostic_reasons(&diagnostics),
        "Z_HIGH:5,A_LOW:1"
    );
}

#[test]
fn breakdown_scan_tunings_stay_in_context_neighborhood() {
    let tunings = breakdown_scan_tunings();

    assert!(tunings.len() <= 128);
    assert!(tunings.contains(&context_breakdown_tuning()));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.breakdown_initial_move_range_mult < 0.90));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.breakdown_min_volume_mult < 1.20));
}

#[test]
fn candle_entity_conversion_uses_sharded_table_entity_shape() {
    let item =
        candle_entity_to_item(&candle_entity(1_700_000_000_000), "BTC-USDT-SWAP", "5m").unwrap();

    assert_eq!(item.ts, 1_700_000_000_000);
    assert_eq!(item.o, 100.0);
    assert_eq!(item.h, 105.0);
    assert_eq!(item.l, 99.0);
    assert_eq!(item.c, 104.0);
    assert_eq!(item.v, 11.0);
    assert_eq!(item.confirm, 1);
}

#[test]
fn candle_span_days_uses_first_and_last_timestamp() {
    let candles = vec![
        CandleItem {
            ts: 1_700_000_000_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
        CandleItem {
            ts: 1_700_086_400_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
    ];

    assert_eq!(candle_span_days(&candles), 1.0);
}

#[test]
fn market_context_backfill_windows_cover_range_with_fixed_window_size() {
    let windows = market_context_backfill_windows(1_000, 3_500, 1_000);

    assert_eq!(
        windows,
        vec![(1_000, 1_999), (2_000, 2_999), (3_000, 3_500)]
    );
}

#[test]
fn market_context_symbol_base_uses_okx_swap_base_coin() {
    assert_eq!(okx_base_coin("BTC-USDT-SWAP"), "BTC");
    assert_eq!(okx_base_coin("ETH-USDT-SWAP"), "ETH");
}

#[test]
fn run_loaded_case_requires_market_context_instead_of_placeholder_snapshot() {
    let case = StrategyCase {
        label: "scalper_btc_5m",
        symbol: "BTC-USDT-SWAP",
        period: "5m",
        family: StrategyFamily::Scalper,
    };
    let candles = cli_scalper_impulse_pullback_candles(560, 100_000.0);
    let baseline = LoadedCase {
        case: case.clone(),
        candles: candles.clone(),
        context: BacktestMarketContext::default(),
        context_required: false,
    };
    let guarded = LoadedCase {
        case,
        candles,
        context: BacktestMarketContext::default(),
        context_required: true,
    };
    let risk = BasicRiskStrategyConfig::default();

    let baseline_result = run_loaded_case(&baseline, risk, None, None);
    let guarded_result = run_loaded_case(&guarded, risk, None, None);

    assert!(baseline_result.open_trades > 0);
    assert_eq!(guarded_result.open_trades, 0);
}

#[test]
fn context_breakdown_uses_cli_tuning_without_changing_global_defaults() {
    let context_tuning = bear_tuning_for_context_run(StrategyFamily::Breakdown, None);
    let default_tuning = BearShortStackBacktestTuning::default();

    assert_ne!(context_tuning, default_tuning);
    assert_eq!(default_tuning.cooldown_candles, 12);
    assert_eq!(default_tuning.breakdown_initial_move_range_mult, 1.35);
    assert_eq!(context_tuning.cooldown_candles, 6);
    assert_eq!(context_tuning.breakdown_initial_move_range_mult, 0.75);
    assert_eq!(context_tuning.breakdown_initial_volume_mult, 0.70);
    assert_eq!(context_tuning.breakdown_min_body_ratio, 0.30);
    assert_eq!(context_tuning.breakdown_min_volume_mult, 1.00);
    assert_eq!(
        bear_tuning_for_context_run(StrategyFamily::Exhaustion, None),
        BearShortStackBacktestTuning {
            cooldown_candles: 24,
            exhaustion_new_high_range_mult: 1.25,
            exhaustion_min_body_ratio: 0.30,
            ..Default::default()
        }
    );

    let provided = BearShortStackBacktestTuning {
        cooldown_candles: 4,
        ..Default::default()
    };
    assert_eq!(
        bear_tuning_for_context_run(StrategyFamily::Breakdown, Some(provided)),
        provided
    );
}

#[test]
fn report_tunings_keep_breakdown_and_exhaustion_separate() {
    let breakdown = context_breakdown_tuning();
    let exhaustion = BearShortStackBacktestTuning {
        cooldown_candles: 24,
        exhaustion_new_high_range_mult: 1.25,
        ..Default::default()
    };
    let tunings = ReportTuningOverrides {
        breakdown: Some(breakdown),
        exhaustion: Some(exhaustion),
        ..Default::default()
    };

    assert_eq!(
        bear_tuning_for_report_family(StrategyFamily::Breakdown, tunings),
        Some(breakdown)
    );
    assert_eq!(
        bear_tuning_for_report_family(StrategyFamily::Exhaustion, tunings),
        Some(exhaustion)
    );
    assert_eq!(
        bear_tuning_for_report_family(StrategyFamily::Scalper, tunings),
        None
    );
}

#[test]
fn builds_backtest_context_from_market_snapshot_series() {
    let candles = vec![
        CandleItem {
            ts: 1_700_000_300_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
        CandleItem {
            ts: 1_700_000_600_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
    ];
    let series = MarketContextSnapshotSeries {
        funding: vec![metric_snapshot("funding_rate", 1_700_000_000_000, 0.0001)],
        open_interest: vec![
            metric_snapshot("open_interest_volume", 1_700_000_000_000, 100.0),
            metric_snapshot("open_interest_volume", 1_700_000_300_000, 103.0),
        ],
        taker: vec![taker_snapshot(1_700_000_300_000, 12.0, 8.0)],
        long_short: vec![long_short_snapshot(1_700_000_300_000, 1.2)],
    };

    let context = build_backtest_market_context(&candles, &series);

    assert_eq!(context.scalper.len(), 2);
    assert_eq!(context.bear.len(), 2);
    assert!((context.scalper[0].oi_expansion_pct - 3.0).abs() < 1e-9);
    assert_eq!(context.scalper[0].taker_buy_volume, 12.0);
    assert_eq!(context.bear[0].long_short_ratio, 1.2);
}

fn metric_snapshot(metric_type: &str, metric_time: i64, value: f64) -> ExternalMarketSnapshot {
    let mut snapshot = ExternalMarketSnapshot::new(
        "okx".to_string(),
        "BTC-USDT-SWAP".to_string(),
        metric_type.to_string(),
        metric_time,
    );
    if metric_type == "funding_rate" {
        snapshot.funding_rate = Some(value);
    } else {
        snapshot.open_interest = Some(value);
    }
    snapshot
}

fn taker_snapshot(metric_time: i64, buy: f64, sell: f64) -> ExternalMarketSnapshot {
    let mut snapshot = ExternalMarketSnapshot::new(
        "okx".to_string(),
        "BTC-USDT-SWAP".to_string(),
        "taker_volume".to_string(),
        metric_time,
    );
    snapshot.raw_payload = Some(serde_json::json!({
        "buy_volume": buy,
        "sell_volume": sell
    }));
    snapshot
}

fn long_short_snapshot(metric_time: i64, ratio: f64) -> ExternalMarketSnapshot {
    let mut snapshot = ExternalMarketSnapshot::new(
        "okx".to_string(),
        "BTC-USDT-SWAP".to_string(),
        "long_short_ratio".to_string(),
        metric_time,
    );
    snapshot.long_short_ratio = Some(ratio);
    snapshot
}

fn scan_case_report(
    label: &str,
    entries: usize,
    wins: usize,
    losses: usize,
    pnl: f64,
) -> CaseReport {
    CaseReport {
        label: label.to_string(),
        candles: 0,
        entries,
        closed: wins + losses,
        wins,
        losses,
        win_rate_pct: ratio_pct(wins, wins + losses),
        pnl,
        final_funds: 100.0 + pnl,
        max_drawdown_pct: 1.0,
        days: 1.0,
        trades_per_day: entries as f64,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: Vec::new(),
    }
}
