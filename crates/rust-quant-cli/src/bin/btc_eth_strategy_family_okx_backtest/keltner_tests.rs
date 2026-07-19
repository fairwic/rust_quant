use super::*;
use rust_quant_strategies::implementations::{
    KeltnerChannelScalperBacktestTuning, KeltnerChannelScalperEntryMode,
    KeltnerChannelScalperSignalSnapshot, KeltnerChannelScalperThresholds,
};

#[test]
fn keltner_research_cases_are_filterable_without_default_report_persistence() {
    let cases = strategy_cases_for_filter(Some("keltner_btc_1m"), true).unwrap();

    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].symbol, "BTC-USDT-SWAP");
    assert_eq!(cases[0].period, "1m");
    assert!(matches!(
        cases[0].family,
        StrategyFamily::KeltnerChannelScalper1m
    ));
    assert!(is_research_case(&cases[0]));
}

#[test]
fn keltner_research_cases_include_5m_and_15m_filters() {
    for (label, period) in [
        ("keltner_btc_5m", "5m"),
        ("keltner_eth_5m", "5m"),
        ("keltner_btc_15m", "15m"),
        ("keltner_eth_15m", "15m"),
    ] {
        let cases = strategy_cases_for_filter(Some(label), true).unwrap();

        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].label, label);
        assert_eq!(cases[0].period, period);
        assert!(matches!(
            cases[0].family,
            StrategyFamily::KeltnerChannelScalper1m
        ));
        assert!(is_research_case(&cases[0]));
    }
}

#[test]
fn keltner_scan_grid_keeps_requested_indicator_defaults() {
    let tunings = keltner_channel_scalper_scan_tunings();

    assert!(!tunings.is_empty());
    assert_eq!(tunings.len(), 6912);
    assert!(tunings.iter().any(|tuning| tuning.confirm_next_candle));
    assert!(tunings.iter().any(|tuning| !tuning.confirm_next_candle));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.entry_mode == KeltnerChannelScalperEntryMode::Reversal));
    assert!(
        tunings
            .iter()
            .any(|tuning| tuning.entry_mode
                == KeltnerChannelScalperEntryMode::ExtremeMomentumReversal)
    );
    assert!(!tunings
        .iter()
        .any(|tuning| tuning.entry_mode == KeltnerChannelScalperEntryMode::Continuation));
    assert!(tunings.iter().any(|tuning| {
        tuning.thresholds.stop_atr_mult == 2.0
            && tuning.thresholds.target_r_1 == 1.0
            && tuning.thresholds.target_r_2 == 2.0
            && tuning.thresholds.target_r_3 == 3.0
    }));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.thresholds.stop_atr_mult == 3.0));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.stop_atr_mult >= 2.0));
    assert!(tunings.iter().any(|tuning| {
        tuning.thresholds.target_r_1 == 0.75
            && tuning.thresholds.target_r_2 == 1.25
            && tuning.thresholds.target_r_3 == 2.0
    }));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.min_basis_slope_atr == 0.0));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.max_adverse_basis_slope_atr == 0.0));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.thresholds.min_atr_pct == 0.0));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.thresholds.min_atr_pct == 0.08));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.max_reentry_body_ratio == 0.0));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.thresholds.require_basis_cross));
    assert!(tunings
        .iter()
        .any(|tuning| !tuning.thresholds.require_basis_cross));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.thresholds.min_rejection_wick_ratio == 0.3));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.max_inner_reclaim_atr == 0.0));
    assert!(tunings
        .iter()
        .all(|tuning| [0.0, 0.15].contains(&tuning.thresholds.min_inner_reclaim_atr)));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.thresholds.min_reentry_close_progress_ratio == 0.65));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.max_breakout_reentry_candles == 0));
    assert!(tunings
        .iter()
        .all(|tuning| tuning.thresholds.min_long_adx == 0.0));
    for tuning in tunings {
        assert_eq!(tuning.thresholds.keltner_length, 50);
        assert_eq!(tuning.thresholds.outer_multiplier, 3.75);
        assert_eq!(tuning.thresholds.inner_multiplier, 2.75);
        assert_eq!(tuning.thresholds.adx_trend_length, 12);
        assert_eq!(tuning.thresholds.adx_smoothing, 12);
        assert_eq!(tuning.thresholds.adx_level, 30.0);
    }
}

#[test]
fn keltner_density_screen_counts_after_warmup_and_respects_cooldown() {
    let candles = one_minute_test_candles(560);
    let mut snapshots = vec![None; candles.len()];
    snapshots[498] = Some(keltner_long_snapshot());
    snapshots[501] = Some(keltner_long_snapshot());
    snapshots[502] = Some(keltner_long_snapshot());
    snapshots[504] = Some(keltner_short_snapshot());
    let tuning = KeltnerChannelScalperBacktestTuning {
        cooldown_candles: 2,
        reentry_lookback_candles: 3,
        allow_long: true,
        allow_short: false,
        confirm_next_candle: false,
        entry_mode: KeltnerChannelScalperEntryMode::Reversal,
        thresholds: KeltnerChannelScalperThresholds::default(),
    };

    let report = keltner_channel_scalper_1m::screen_keltner_signal_density_for_case(
        tuning, &candles, &snapshots,
    );

    assert_eq!(report.signals, 1);
    assert_eq!(report.long_signals, 1);
    assert_eq!(report.short_signals, 0);
    assert!(report.signals_per_day > 0.0);
}

#[test]
fn keltner_snapshot_cache_key_separates_next_candle_confirmation() {
    let normal = KeltnerChannelScalperBacktestTuning::default();
    let confirmed = KeltnerChannelScalperBacktestTuning {
        confirm_next_candle: true,
        ..Default::default()
    };

    assert!(!keltner_channel_scalper_1m::share_keltner_snapshot_inputs(
        normal, confirmed
    ));
}

#[test]
fn keltner_risk_config_enables_three_stage_partial_take_profit() {
    let risk = keltner_channel_scalper_1m::keltner_risk_config(BasicRiskStrategyConfig::default());

    assert_eq!(risk.tiered_take_profit_level_1_close_ratio, Some(0.40));
    assert_eq!(risk.tiered_take_profit_level_2_close_ratio, Some(0.50));
    assert_eq!(risk.atr_take_profit_ratio, None);
    assert_eq!(risk.fixed_signal_kline_take_profit_ratio, None);
}

#[test]
fn keltner_risk_config_supports_explicit_tier_close_ratios() {
    let risk = keltner_channel_scalper_1m::keltner_risk_config_with_tiers(
        BasicRiskStrategyConfig::default(),
        0.70,
        0.50,
    );

    assert_eq!(risk.tiered_take_profit_level_1_close_ratio, Some(0.70));
    assert_eq!(risk.tiered_take_profit_level_2_close_ratio, Some(0.50));
    assert_eq!(risk.atr_take_profit_ratio, None);
    assert_eq!(risk.fixed_signal_kline_take_profit_ratio, None);
}

#[test]
fn keltner_basis_cross_profiles_keep_baseline_and_confirmation_pair() {
    let baseline = keltner_channel_scalper_1m::best_keltner_raw_tuning();
    let profiles = keltner_channel_scalper_1m::keltner_basis_cross_profile_tunings(baseline);

    assert_eq!(profiles.len(), 2);
    assert!(!profiles[0].thresholds.require_basis_cross);
    assert!(profiles[1].thresholds.require_basis_cross);
    assert_eq!(
        profiles[0].thresholds.stop_atr_mult,
        profiles[1].thresholds.stop_atr_mult
    );
    assert_eq!(
        profiles[0].thresholds.target_r_1,
        profiles[1].thresholds.target_r_1
    );
    assert_eq!(
        profiles[0].thresholds.min_atr_pct,
        profiles[1].thresholds.min_atr_pct
    );
}

#[test]
fn keltner_failure_exit_decision_closes_slow_long_after_window() {
    let config = keltner_channel_scalper_1m::KeltnerFailureExitConfig {
        bars: 3,
        min_progress_r: 0.50,
    };
    let candles = vec![
        candle_with_close(1_783_000_060_000, 100.8),
        candle_with_close(1_783_000_120_000, 101.2),
        candle_with_close(1_783_000_180_000, 101.5),
    ];

    let decision = keltner_channel_scalper_1m::keltner_failure_exit_decision(
        keltner_channel_scalper_1m::KeltnerFailureExitSide::Long,
        100.0,
        96.0,
        1.0,
        0.0,
        &candles,
        config,
    )
    .expect("slow long should trigger failure exit");

    assert_eq!(decision.exit_price, 101.5);
    assert!((decision.progress_r - 0.375).abs() < 1e-9);
    assert!((decision.pnl - 1.5).abs() < 1e-9);
}

#[test]
fn keltner_failure_exit_overlay_replaces_late_stop_loss() {
    let candles = vec![
        candle_with_close(1_783_000_000_000, 100.0),
        candle_with_close(1_783_000_060_000, 100.5),
        candle_with_close(1_783_000_120_000, 101.0),
        candle_with_close(1_783_000_180_000, 99.0),
        candle_with_close(1_783_000_240_000, 96.0),
    ];
    let open_time = candle_time(candles[0].ts);
    let stop_time = candle_time(candles[4].ts);
    let result = BackTestResult {
        open_trades: 1,
        trade_records: vec![
            trade_record(
                "long",
                &open_time,
                Some(&open_time),
                100.0,
                None,
                0.0,
                1.0,
                Some(
                    serde_json::json!({
                        "strategy": "keltner_channel_scalper_1m_v1_research",
                        "action": "long",
                        "reasons": ["KELTNER_LOWER_REENTRY_LONG", "STOP_PRICE:96"]
                    })
                    .to_string(),
                ),
            ),
            trade_record(
                "close",
                &open_time,
                Some(&stop_time),
                100.0,
                Some(96.0),
                -4.0,
                1.0,
                None,
            ),
        ],
        ..BackTestResult::default()
    };
    let report = keltner_channel_scalper_1m::keltner_failure_exit_overlay_report(
        "keltner_test",
        &candles,
        &result,
        BasicRiskStrategyConfig {
            trade_fee_rate: Some(0.0),
            ..BasicRiskStrategyConfig::default()
        },
        keltner_channel_scalper_1m::KeltnerFailureExitConfig {
            bars: 2,
            min_progress_r: 0.50,
        },
    );

    let summary =
        keltner_channel_scalper_1m::summarize_keltner_failure_exit_overlay_reports(&[report]);

    assert_eq!(summary.entries, 1);
    assert_eq!(summary.triggered, 1);
    assert_eq!(summary.wins, 1);
    assert_eq!(summary.losses, 0);
    assert!((summary.pnl - 1.0).abs() < 1e-9);
    assert!((summary.delta_pnl - 5.0).abs() < 1e-9);
}

#[test]
fn keltner_diagnostics_group_close_types_and_shape_by_outcome() {
    let reports = vec![CaseReport {
        label: "keltner_btc_1m".to_string(),
        candles: 0,
        entries: 3,
        closed: 3,
        wins: 1,
        losses: 2,
        win_rate_pct: 33.33,
        pnl: -0.7,
        final_funds: 99.3,
        max_drawdown_pct: 1.0,
        days: 1.0,
        trades_per_day: 3.0,
        trades: vec![
            keltner_debug_trade("win-1", 0.7, "take_profit", 22.0, 0.10, 0.60),
            keltner_debug_trade("win-1", 0.5, "take_profit", 22.0, 0.10, 0.60),
            keltner_debug_trade("loss-1", -1.0, "stop_loss", 34.0, -0.20, 0.20),
            keltner_debug_trade("loss-2", -0.9, "stop_loss", 30.0, -0.10, 0.40),
        ],
        filtered_signals: 0,
        filtered_reason_counts: Vec::new(),
        filtered_signal_snapshots: Vec::new(),
    }];

    let close_types = keltner_channel_scalper_1m::keltner_close_type_summaries(&reports);
    assert_eq!(close_types.len(), 2);
    assert_eq!(close_types[0].close_type, "stop_loss");
    assert_eq!(close_types[0].count, 2);
    assert_eq!(close_types[0].losses, 2);
    assert!((close_types[0].pnl + 1.9).abs() < 1e-9);
    assert_eq!(close_types[1].close_type, "take_profit");
    assert_eq!(close_types[1].wins, 2);

    let trades = reports[0].trades.as_slice();
    let wins = keltner_channel_scalper_1m::keltner_shape_summary_for_outcome(trades, true);
    let losses = keltner_channel_scalper_1m::keltner_shape_summary_for_outcome(trades, false);
    assert_eq!(wins.count, 1);
    assert_eq!(wins.avg_adx, 22.0);
    assert_eq!(wins.avg_basis_slope_atr, 0.10);
    assert_eq!(losses.count, 2);
    assert_eq!(losses.avg_adx, 32.0);
    assert!((losses.avg_reclaim_atr - 0.30).abs() < 1e-9);
}

#[test]
fn keltner_best_raw_tuning_tracks_latest_scan_leader() {
    let tuning = keltner_channel_scalper_1m::best_keltner_raw_tuning();

    assert_eq!(tuning.cooldown_candles, 6);
    assert!(tuning.allow_long);
    assert!(!tuning.allow_short);
    assert_eq!(tuning.thresholds.stop_atr_mult, 3.0);
    assert_eq!(tuning.thresholds.min_inner_reclaim_atr, 0.0);
    assert_eq!(tuning.thresholds.max_inner_reclaim_atr, 0.0);
    assert_eq!(tuning.thresholds.min_reentry_close_progress_ratio, 0.65);
    assert_eq!(tuning.thresholds.max_breakout_reentry_candles, 0);
    assert_eq!(tuning.thresholds.min_atr_pct, 0.06);
    assert_eq!(tuning.thresholds.target_r_1, 0.75);
    assert_eq!(tuning.thresholds.target_r_2, 1.25);
    assert_eq!(tuning.thresholds.target_r_3, 2.0);
}

fn one_minute_test_candles(count: usize) -> Vec<CandleItem> {
    (0..count)
        .map(|index| CandleItem {
            o: 100.0,
            h: 102.0,
            l: 98.0,
            c: 101.0,
            v: 1_000.0,
            ts: 1_783_000_000_000 + index as i64 * 60_000,
            confirm: 1,
        })
        .collect()
}

fn candle_with_close(ts: i64, close: f64) -> CandleItem {
    CandleItem {
        o: close,
        h: close + 0.4,
        l: close - 0.4,
        c: close,
        v: 1_000.0,
        ts,
        confirm: 1,
    }
}

fn candle_time(ts: i64) -> String {
    rust_quant_common::utils::time::mill_time_to_datetime(ts).unwrap()
}

fn trade_record(
    option_type: &str,
    open_time: &str,
    close_time: Option<&str>,
    open_price: f64,
    close_price: Option<f64>,
    pnl: f64,
    quantity: f64,
    signal_result: Option<String>,
) -> TradeRecord {
    TradeRecord {
        option_type: option_type.to_string(),
        open_position_time: open_time.to_string(),
        signal_open_position_time: None,
        close_position_time: close_time.map(ToOwned::to_owned),
        open_price,
        signal_status: 0,
        close_price,
        profit_loss: pnl,
        quantity,
        full_close: option_type == "close",
        close_type: if option_type == "close" {
            "Signal_Kline_Stop_Loss".to_string()
        } else {
            String::new()
        },
        win_num: 0,
        loss_num: 0,
        signal_value: None,
        signal_result,
        stop_loss_source: None,
        stop_loss_update_history: None,
        initial_stop_price: None,
        initial_risk_amount: None,
        net_profit_r: None,
    }
}

fn keltner_long_snapshot() -> KeltnerChannelScalperSignalSnapshot {
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
        adx: 25.0,
        basis_slope_atr: 0.06,
        outer_upper_breached: false,
        outer_lower_breached: true,
        returned_inside_inner_upper: false,
        returned_inside_inner_lower: true,
        reentry_body_ratio: 0.5,
        rejection_wick_ratio: 0.5,
        reentry_close_progress_ratio: 0.75,
        breakout_reentry_candles: 0,
        bullish_momentum_break: false,
        bearish_momentum_break: false,
    }
}

fn keltner_debug_trade(
    open_time: &str,
    pnl: f64,
    close_type: &str,
    adx: f64,
    basis_slope_atr: f64,
    reclaim_atr: f64,
) -> ClosedTradeDebug {
    ClosedTradeDebug {
        open_time: open_time.to_string(),
        close_time: None,
        open_price: 100.0,
        close_price: Some(101.0),
        pnl,
        close_type: close_type.to_string(),
        entry_snapshot: None,
        keltner_snapshot: Some(KeltnerEntrySnapshotDebug {
            adx,
            basis_slope_atr,
            reclaim_atr,
            reentry_body_ratio: 0.5,
            rejection_wick_ratio: 0.4,
            reentry_close_progress_ratio: 0.7,
            breakout_reentry_candles: 0.0,
            atr_pct: 0.2,
        }),
        entry_reasons: Vec::new(),
    }
}

fn keltner_short_snapshot() -> KeltnerChannelScalperSignalSnapshot {
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
        reentry_body_ratio: 0.5,
        rejection_wick_ratio: 0.5,
        reentry_close_progress_ratio: 0.75,
        breakout_reentry_candles: 0,
        bullish_momentum_break: false,
        bearish_momentum_break: false,
    }
}
