use super::*;

/// Builds the broad BTC/ETH scalper grid used for research scans.
pub(super) fn scalper_scan_tunings() -> Vec<BtcEthLiquidityScalperBacktestTuning> {
    let mut tunings = Vec::new();
    for allow_short in [false, true] {
        for require_oi_confirmation in [false, true] {
            for (trend_fast_window, trend_slow_window) in [(20_usize, 48_usize), (13, 34)] {
                for cooldown in [4_usize, 8] {
                    for min_directional_ratio_48 in [0.25, 0.50] {
                        for min_directional_ratio_24 in [0.35, 0.50] {
                            for impulse_move_range_mult in [0.8, 1.2] {
                                for impulse_min_body_ratio in [0.35, 0.55] {
                                    for impulse_min_volume_mult in [0.8] {
                                        for resume_extension_body_mult in [0.70] {
                                            for require_previous_extreme_break in [false] {
                                                for (target_r_1, target_r_2) in
                                                    [(0.4, 0.8), (0.6, 1.2), (0.8, 1.6)]
                                                {
                                                    tunings.push(
                                                        BtcEthLiquidityScalperBacktestTuning {
                                                            cooldown_candles: cooldown,
                                                            allow_short,
                                                            trend_fast_window,
                                                            trend_slow_window,
                                                            min_directional_ratio_48,
                                                            min_directional_ratio_24,
                                                            impulse_move_range_mult,
                                                            impulse_min_body_ratio,
                                                            impulse_min_volume_mult,
                                                            resume_extension_body_mult,
                                                            require_previous_extreme_break,
                                                            require_oi_confirmation,
                                                            target_r_1,
                                                            target_r_2,
                                                            ..Default::default()
                                                        },
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    tunings
}

/// Builds a small short-cycle grid for fast BTC/ETH 1m/5m scalper iteration.
pub(super) fn scalper_narrow_scan_tunings() -> Vec<BtcEthLiquidityScalperBacktestTuning> {
    let mut tunings = Vec::new();
    for allow_short in [false, true] {
        for cooldown in [2_usize, 4] {
            for (min_directional_ratio_48, min_directional_ratio_24) in [(0.15, 0.25), (0.25, 0.35)]
            {
                for impulse_move_range_mult in [0.6, 0.8] {
                    for impulse_min_body_ratio in [0.25, 0.35] {
                        for (target_r_1, target_r_2) in [(0.30, 0.60), (0.40, 0.80)] {
                            tunings.push(BtcEthLiquidityScalperBacktestTuning {
                                cooldown_candles: cooldown,
                                allow_short,
                                trend_fast_window: 13,
                                trend_slow_window: 34,
                                min_directional_ratio_48,
                                min_directional_ratio_24,
                                impulse_move_range_mult,
                                impulse_min_body_ratio,
                                impulse_min_volume_mult: 0.8,
                                resume_extension_body_mult: 0.70,
                                require_previous_extreme_break: false,
                                require_oi_confirmation: false,
                                target_r_1,
                                target_r_2,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    }
    tunings
}

/// Prints isolated breakdown-short scan results so weak breakdown presets are not masked by other strategies.
pub(super) fn print_breakdown_scan(
    loaded_cases: &[LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = strategy_family_risk_config(risk_percent, trade_fee_rate);
    let non_breakdown_reports = Vec::new();
    let breakdown_cases = loaded_cases
        .iter()
        .filter(|loaded| matches!(loaded.case.family, StrategyFamily::Breakdown))
        .collect::<Vec<_>>();

    let mut candidates = Vec::new();
    let mut raw_candidates = Vec::new();
    for tuning in breakdown_scan_tunings() {
        let mut breakdown_reports = Vec::with_capacity(breakdown_cases.len());
        for loaded in &breakdown_cases {
            let result = run_loaded_case(loaded, risk, None, Some(tuning));
            breakdown_reports.push(build_report(loaded.case.label, &loaded.candles, &result));
        }
        let summary =
            summarize_breakdown_candidate_reports(&non_breakdown_reports, &breakdown_reports);
        raw_candidates.push(ScanCandidateReport {
            tuning,
            ..summary.clone()
        });
        if summary.win_rate_pct >= 60.0
            && summary.max_drawdown_pct < 15.0
            && summary.pnl > 0.0
            && summary.remove_top5_pnl > 0.0
        {
            candidates.push(ScanCandidateReport { tuning, ..summary });
        }
    }

    raw_candidates.sort_by(|left, right| {
        right
            .trades_per_day
            .total_cmp(&left.trades_per_day)
            .then_with(|| right.pnl.total_cmp(&left.pnl))
    });
    for candidate in raw_candidates.iter().take(5) {
        println!(
            "breakdown_raw_top cooldown={} initial_move={:.2} initial_volume={:.2} min_reclaim={:.2} max_reclaim={:.2} support_break={:.2} body={:.2} volume={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
            candidate.tuning.cooldown_candles,
            candidate.tuning.breakdown_initial_move_range_mult,
            candidate.tuning.breakdown_initial_volume_mult,
            candidate.tuning.breakdown_min_reclaim_distance_atr,
            candidate.tuning.breakdown_max_reclaim_distance_atr,
            candidate.tuning.breakdown_min_support_break_range,
            candidate.tuning.breakdown_min_body_ratio,
            candidate.tuning.breakdown_min_volume_mult,
            candidate.entries,
            candidate.wins,
            candidate.losses,
            candidate.win_rate_pct,
            candidate.pnl,
            candidate.max_drawdown_pct,
            candidate.trades_per_day,
            candidate.early_win_rate_pct,
            candidate.early_pnl,
            candidate.late_win_rate_pct,
            candidate.late_pnl,
            candidate.remove_top5_pnl
        );
    }

    candidates.sort_by(|left, right| {
        right
            .trades_per_day
            .total_cmp(&left.trades_per_day)
            .then_with(|| right.pnl.total_cmp(&left.pnl))
    });
    if candidates.is_empty() {
        println!(
            "no_breakdown_candidates source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,remove_top5_pnl>0"
        );
        return;
    }
    for candidate in candidates.iter().take(20) {
        println!(
            "breakdown_candidate cooldown={} initial_move={:.2} initial_volume={:.2} min_reclaim={:.2} max_reclaim={:.2} support_break={:.2} body={:.2} volume={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
            candidate.tuning.cooldown_candles,
            candidate.tuning.breakdown_initial_move_range_mult,
            candidate.tuning.breakdown_initial_volume_mult,
            candidate.tuning.breakdown_min_reclaim_distance_atr,
            candidate.tuning.breakdown_max_reclaim_distance_atr,
            candidate.tuning.breakdown_min_support_break_range,
            candidate.tuning.breakdown_min_body_ratio,
            candidate.tuning.breakdown_min_volume_mult,
            candidate.entries,
            candidate.wins,
            candidate.losses,
            candidate.win_rate_pct,
            candidate.pnl,
            candidate.max_drawdown_pct,
            candidate.trades_per_day,
            candidate.early_win_rate_pct,
            candidate.early_pnl,
            candidate.late_win_rate_pct,
            candidate.late_pnl,
            candidate.remove_top5_pnl
        );
    }
}

/// Returns a deliberately narrow breakdown grid around the current context preset.
/// Wider grids were too slow for the OKX sharded backtest loop and increased overfit risk.
pub(super) fn breakdown_scan_tunings() -> Vec<BearShortStackBacktestTuning> {
    let mut tunings = Vec::new();
    for cooldown in [6_usize, 8, 10] {
        for initial_move in [0.75, 0.90] {
            for initial_volume in [0.70, 0.80] {
                for body_ratio in [0.30, 0.35] {
                    for volume_mult in [1.00, 1.20] {
                        tunings.push(BearShortStackBacktestTuning {
                            cooldown_candles: cooldown,
                            breakdown_initial_move_range_mult: initial_move,
                            breakdown_initial_volume_mult: initial_volume,
                            breakdown_min_reclaim_distance_atr: 0.15,
                            breakdown_max_reclaim_distance_atr: 1.20,
                            breakdown_min_support_break_range: 0.15,
                            breakdown_min_body_ratio: body_ratio,
                            breakdown_min_volume_mult: volume_mult,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
    tunings
}

/// Prints isolated exhaustion-fade scan results for promoting the short preset without live mutation.
pub(super) fn print_exhaustion_scan(
    loaded_cases: &[LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = strategy_family_risk_config(risk_percent, trade_fee_rate);
    let non_exhaustion_reports = Vec::new();
    let exhaustion_cases = loaded_cases
        .iter()
        .filter(|loaded| matches!(loaded.case.family, StrategyFamily::Exhaustion))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for cooldown in [12_usize, 16, 20, 24] {
        for new_high_mult in [1.25, 1.35, 1.5] {
            for body_ratio in [0.30, 0.35, 0.40] {
                for volume_mult in [1.3, 1.4, 1.5, 1.6] {
                    let tuning = BearShortStackBacktestTuning {
                        cooldown_candles: cooldown,
                        exhaustion_new_high_range_mult: new_high_mult,
                        exhaustion_min_body_ratio: body_ratio,
                        exhaustion_min_volume_mult: volume_mult,
                        ..Default::default()
                    };
                    let mut exhaustion_reports = Vec::with_capacity(exhaustion_cases.len());
                    for loaded in &exhaustion_cases {
                        let result = run_loaded_case(loaded, risk, None, Some(tuning));
                        exhaustion_reports.push(build_report(
                            loaded.case.label,
                            &loaded.candles,
                            &result,
                        ));
                    }
                    let summary = summarize_exhaustion_candidate_reports(
                        &non_exhaustion_reports,
                        &exhaustion_reports,
                    );
                    if summary.win_rate_pct >= 60.0
                        && summary.max_drawdown_pct < 15.0
                        && summary.pnl > 0.0
                    {
                        candidates.push(ScanCandidateReport { tuning, ..summary });
                    }
                }
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .trades_per_day
            .total_cmp(&left.trades_per_day)
            .then_with(|| right.pnl.total_cmp(&left.pnl))
    });
    for candidate in candidates.iter().take(20) {
        println!(
            "candidate cooldown={} new_high_mult={:.2} body_ratio={:.2} volume_mult={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
            candidate.tuning.cooldown_candles,
            candidate.tuning.exhaustion_new_high_range_mult,
            candidate.tuning.exhaustion_min_body_ratio,
            candidate.tuning.exhaustion_min_volume_mult,
            candidate.entries,
            candidate.wins,
            candidate.losses,
            candidate.win_rate_pct,
            candidate.pnl,
            candidate.max_drawdown_pct,
            candidate.trades_per_day,
            candidate.early_win_rate_pct,
            candidate.early_pnl,
            candidate.late_win_rate_pct,
            candidate.late_pnl,
            candidate.remove_top5_pnl
        );
    }
}
