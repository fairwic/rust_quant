use anyhow::{anyhow, Result};
use rust_quant_strategies::implementations::{
    SmartMoneyConceptsBacktestTuning, SmartMoneyConceptsStrategy, SmartMoneyConceptsThresholds,
};

use super::{
    build_report, format_case_reports, load_sharded_candles, ratio_pct, strategy_cases,
    strategy_family_risk_config, BacktestMarketContext, CaseReport, LoadedCase, StrategyFamily,
};

#[derive(Debug, Clone, Copy)]
struct SmcScanCandidateReport {
    tuning: SmartMoneyConceptsBacktestTuning,
    entries: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    max_drawdown_pct: f64,
    trades_per_day: f64,
}

const SMC_MIN_TRADES_PER_DAY: f64 = 1.0;

pub(super) async fn load_smc_cases(
    limit: usize,
    case_label: Option<&str>,
) -> Result<Vec<LoadedCase>> {
    let cases = strategy_cases()
        .into_iter()
        .filter(|case| matches!(case.family, StrategyFamily::SmartMoneyConcepts))
        .filter(|case| case_label.map_or(true, |label| case.label == label))
        .collect::<Vec<_>>();
    if cases.is_empty() {
        return Err(anyhow!(
            "no Smart Money Concepts case matched --case-label {:?}",
            case_label
        ));
    }
    let mut loaded = Vec::with_capacity(cases.len());
    for case in cases {
        let candles = load_sharded_candles(case.symbol, case.period, limit).await?;
        loaded.push(LoadedCase {
            case,
            candles,
            context: BacktestMarketContext::default(),
            context_required: false,
        });
    }
    Ok(loaded)
}

pub(super) fn print_smc_scan(
    loaded_cases: &[LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let smc_cases = loaded_cases
        .iter()
        .filter(|loaded| matches!(loaded.case.family, StrategyFamily::SmartMoneyConcepts))
        .collect::<Vec<_>>();
    if smc_cases.is_empty() {
        println!("no_smc_cases_loaded");
        return;
    }
    let risk = strategy_family_risk_config(risk_percent, trade_fee_rate);
    let mut candidates = Vec::new();
    for tuning in smc_scan_tunings() {
        let reports = smc_cases
            .iter()
            .map(|loaded| {
                let result = SmartMoneyConceptsStrategy.run_test_with_tuning(
                    loaded.case.symbol,
                    &loaded.candles,
                    risk,
                    tuning,
                );
                build_report(loaded.case.label, &loaded.candles, &result)
            })
            .collect::<Vec<_>>();
        let summary = summarize_smc_candidate(tuning, &reports);
        candidates.push((summary, format_case_reports(&reports)));
    }

    candidates.sort_by(|left, right| {
        right
            .0
            .win_rate_pct
            .total_cmp(&left.0.win_rate_pct)
            .then_with(|| right.0.pnl.total_cmp(&left.0.pnl))
            .then_with(|| right.0.trades_per_day.total_cmp(&left.0.trades_per_day))
    });
    for (report, cases) in candidates.iter().take(10) {
        print_smc_candidate_line("smc_raw_top", report, cases);
    }
    for (report, cases) in raw_candidates_by_frequency(&candidates)
        .into_iter()
        .take(10)
    {
        print_smc_candidate_line("smc_raw_freq_top", report, cases);
    }
    for mode in ["base", "sweep", "fade_sweep", "fvg", "fade_fvg"] {
        if let Some((report, cases)) = raw_candidates_by_frequency(&candidates)
            .into_iter()
            .find(|(report, _)| signal_mode_label(&report.tuning) == mode)
        {
            print_smc_candidate_line("smc_mode_freq_top", report, cases);
        }
        if let Some((report, cases)) = mode_candidates_by_quality(&candidates, mode)
            .into_iter()
            .next()
        {
            print_smc_candidate_line("smc_mode_quality_top", report, cases);
        }
    }

    let qualified = candidates
        .iter()
        .filter(|(report, _)| smc_candidate_meets_target(report))
        .collect::<Vec<_>>();
    if qualified.is_empty() {
        println!(
            "no_smc_candidates source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,trades_per_day>=1"
        );
        return;
    }
    for (report, cases) in qualified.into_iter().take(20) {
        print_smc_candidate_line("smc_candidate", report, cases);
    }
    for (report, cases) in qualified_candidates_by_frequency(&candidates)
        .into_iter()
        .take(20)
    {
        print_smc_candidate_line("smc_candidate_freq_top", report, cases);
    }
}

fn smc_scan_tunings() -> Vec<SmartMoneyConceptsBacktestTuning> {
    let mut tunings = Vec::new();
    // Keep this grid bounded for daily research runs; broad parameter mining belongs in a dedicated offline job.
    for pivot_confirmation_bars in [3_usize, 5] {
        for cooldown_candles in [0_usize] {
            for require_retest in [false, true] {
                for retest_max_wait_candles in
                    retest_wait_candidates(require_retest).iter().copied()
                {
                    for min_trend_strength_pct in [0.50, 1.0] {
                        push_smc_tunings(
                            &mut tunings,
                            pivot_confirmation_bars,
                            cooldown_candles,
                            require_retest,
                            retest_max_wait_candles,
                            min_trend_strength_pct,
                        );
                    }
                }
            }
        }
    }
    tunings
}

fn retest_wait_candidates(require_retest: bool) -> &'static [usize] {
    if require_retest {
        &[0, 4]
    } else {
        &[0]
    }
}

fn push_smc_tunings(
    tunings: &mut Vec<SmartMoneyConceptsBacktestTuning>,
    pivot_confirmation_bars: usize,
    cooldown_candles: usize,
    require_retest: bool,
    retest_max_wait_candles: usize,
    min_trend_strength_pct: f64,
) {
    for allow_short in [false, true] {
        for (enable_liquidity_sweep, enable_fair_value_gap, fade_signal) in [
            (false, false, false),
            (true, false, false),
            (true, false, true),
            (false, true, false),
            (false, true, true),
        ] {
            if fade_signal && !allow_short {
                continue;
            }
            for max_entry_extension_atr in [6.0] {
                for min_displacement_body_atr in [0.0, 0.5] {
                    for require_premium_discount_zone in [false, true] {
                        for max_atr_pct in [0.50] {
                            for (target_r_1, target_r_2, target_r_3) in [(0.25, 0.50, 0.75)] {
                                tunings.push(SmartMoneyConceptsBacktestTuning {
                                    pivot_confirmation_bars,
                                    cooldown_candles,
                                    retest_max_wait_candles,
                                    allow_short,
                                    enable_liquidity_sweep,
                                    enable_fair_value_gap,
                                    fade_signal,
                                    trend_fast_window: 20,
                                    trend_slow_window: 96,
                                    thresholds: SmartMoneyConceptsThresholds {
                                        max_entry_extension_atr,
                                        max_retest_distance_atr: 0.75,
                                        require_retest,
                                        require_trend_alignment: true,
                                        require_premium_discount_zone,
                                        min_trend_strength_pct,
                                        min_displacement_body_atr,
                                        min_atr_pct: 0.0,
                                        max_atr_pct,
                                        stop_atr_buffer: 0.75,
                                        target_r_1,
                                        target_r_2,
                                        target_r_3,
                                    },
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn summarize_smc_candidate(
    tuning: SmartMoneyConceptsBacktestTuning,
    reports: &[CaseReport],
) -> SmcScanCandidateReport {
    let wins = reports.iter().map(|report| report.wins).sum::<usize>();
    let losses = reports.iter().map(|report| report.losses).sum::<usize>();
    let pnl = reports.iter().map(|report| report.pnl).sum::<f64>();
    let entries = reports.iter().map(|report| report.entries).sum::<usize>();
    let max_drawdown_pct = reports
        .iter()
        .map(|report| report.max_drawdown_pct)
        .fold(0.0, f64::max);
    let combo_days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    SmcScanCandidateReport {
        tuning,
        entries,
        wins,
        losses,
        win_rate_pct: ratio_pct(wins, wins + losses),
        pnl,
        max_drawdown_pct,
        trades_per_day: if combo_days > 0.0 {
            entries as f64 / combo_days
        } else {
            0.0
        },
    }
}

fn smc_candidate_meets_target(report: &SmcScanCandidateReport) -> bool {
    report.entries > 0
        && report.pnl > 0.0
        && report.win_rate_pct >= 60.0
        && report.max_drawdown_pct < 15.0
        && report.trades_per_day >= SMC_MIN_TRADES_PER_DAY
}

fn qualified_candidates_by_frequency(
    candidates: &[(SmcScanCandidateReport, String)],
) -> Vec<(&SmcScanCandidateReport, &str)> {
    let mut qualified = candidates
        .iter()
        .filter(|(report, _)| smc_candidate_meets_target(report))
        .map(|(report, cases)| (report, cases.as_str()))
        .collect::<Vec<_>>();
    qualified.sort_by(|left, right| {
        right
            .0
            .trades_per_day
            .total_cmp(&left.0.trades_per_day)
            .then_with(|| right.0.win_rate_pct.total_cmp(&left.0.win_rate_pct))
            .then_with(|| right.0.pnl.total_cmp(&left.0.pnl))
    });
    qualified
}

fn raw_candidates_by_frequency(
    candidates: &[(SmcScanCandidateReport, String)],
) -> Vec<(&SmcScanCandidateReport, &str)> {
    let mut ranked = candidates
        .iter()
        .map(|(report, cases)| (report, cases.as_str()))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .trades_per_day
            .total_cmp(&left.0.trades_per_day)
            .then_with(|| right.0.win_rate_pct.total_cmp(&left.0.win_rate_pct))
            .then_with(|| right.0.pnl.total_cmp(&left.0.pnl))
    });
    ranked
}

fn mode_candidates_by_quality<'a>(
    candidates: &'a [(SmcScanCandidateReport, String)],
    mode: &str,
) -> Vec<(&'a SmcScanCandidateReport, &'a str)> {
    let mut ranked = candidates
        .iter()
        .filter(|(report, _)| signal_mode_label(&report.tuning) == mode)
        .map(|(report, cases)| (report, cases.as_str()))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .win_rate_pct
            .total_cmp(&left.0.win_rate_pct)
            .then_with(|| right.0.pnl.total_cmp(&left.0.pnl))
            .then_with(|| right.0.trades_per_day.total_cmp(&left.0.trades_per_day))
    });
    ranked
}

fn signal_mode_label(tuning: &SmartMoneyConceptsBacktestTuning) -> &'static str {
    if tuning.enable_fair_value_gap && tuning.fade_signal {
        "fade_fvg"
    } else if tuning.enable_fair_value_gap {
        "fvg"
    } else if tuning.enable_liquidity_sweep && tuning.fade_signal {
        "fade_sweep"
    } else if tuning.enable_liquidity_sweep {
        "sweep"
    } else {
        "base"
    }
}

fn print_smc_candidate_line(prefix: &str, report: &SmcScanCandidateReport, cases: &str) {
    println!(
        "{} pivot={} trend={}/{} allow_short={} sweep={} fvg={} fade={} align={} pd_zone={} min_trend={:.2} disp_body_atr={:.2} atr_pct={:.2}-{:.2} cooldown={} retest={} retest_wait={} max_ext={:.2} stop_buffer={:.2} target_r={:.1}/{:.1}/{:.1} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} cases={}",
        prefix,
        report.tuning.pivot_confirmation_bars,
        report.tuning.trend_fast_window,
        report.tuning.trend_slow_window,
        report.tuning.allow_short,
        report.tuning.enable_liquidity_sweep,
        report.tuning.enable_fair_value_gap,
        report.tuning.fade_signal,
        report.tuning.thresholds.require_trend_alignment,
        report.tuning.thresholds.require_premium_discount_zone,
        report.tuning.thresholds.min_trend_strength_pct,
        report.tuning.thresholds.min_displacement_body_atr,
        report.tuning.thresholds.min_atr_pct,
        report.tuning.thresholds.max_atr_pct,
        report.tuning.cooldown_candles,
        report.tuning.thresholds.require_retest,
        report.tuning.retest_max_wait_candles,
        report.tuning.thresholds.max_entry_extension_atr,
        report.tuning.thresholds.stop_atr_buffer,
        report.tuning.thresholds.target_r_1,
        report.tuning.thresholds.target_r_2,
        report.tuning.thresholds.target_r_3,
        report.entries,
        report.wins,
        report.losses,
        report.win_rate_pct,
        report.pnl,
        report.max_drawdown_pct,
        report.trades_per_day,
        cases
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        entries: usize,
        win_rate_pct: f64,
        pnl: f64,
        max_drawdown_pct: f64,
        trades_per_day: f64,
    ) -> SmcScanCandidateReport {
        SmcScanCandidateReport {
            tuning: SmartMoneyConceptsBacktestTuning::default(),
            entries,
            wins: 0,
            losses: 0,
            win_rate_pct,
            pnl,
            max_drawdown_pct,
            trades_per_day,
        }
    }

    #[test]
    fn frequency_view_keeps_only_qualified_candidates_and_sorts_by_trades_per_day() {
        let candidates = vec![
            (candidate(8, 72.0, 1.0, 1.0, 0.25), "low_freq".to_string()),
            (candidate(40, 61.0, 1.0, 2.0, 2.50), "high_freq".to_string()),
            (
                candidate(80, 58.0, 10.0, 1.0, 9.00),
                "unqualified".to_string(),
            ),
        ];

        let ranked = qualified_candidates_by_frequency(&candidates);

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].1, "high_freq");
    }

    #[test]
    fn raw_frequency_view_keeps_unqualified_candidates_for_diagnostics() {
        let candidates = vec![
            (candidate(8, 72.0, 1.0, 1.0, 0.25), "qualified".to_string()),
            (
                candidate(80, 41.0, -3.0, 4.0, 8.00),
                "high_freq_raw".to_string(),
            ),
        ];

        let ranked = raw_candidates_by_frequency(&candidates);

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].1, "high_freq_raw");
        assert_eq!(ranked[1].1, "qualified");
    }

    #[test]
    fn signal_mode_label_identifies_base_sweep_and_fvg_variants() {
        let base = SmartMoneyConceptsBacktestTuning::default();
        let sweep = SmartMoneyConceptsBacktestTuning {
            enable_liquidity_sweep: true,
            ..Default::default()
        };
        let fvg = SmartMoneyConceptsBacktestTuning {
            enable_fair_value_gap: true,
            ..Default::default()
        };
        let fade_fvg = SmartMoneyConceptsBacktestTuning {
            enable_fair_value_gap: true,
            fade_signal: true,
            ..Default::default()
        };

        assert_eq!(signal_mode_label(&base), "base");
        assert_eq!(signal_mode_label(&sweep), "sweep");
        assert_eq!(signal_mode_label(&fvg), "fvg");
        assert_eq!(signal_mode_label(&fade_fvg), "fade_fvg");
    }

    #[test]
    fn mode_quality_view_picks_best_candidate_for_each_signal_mode() {
        let base = candidate(3, 61.0, 0.5, 1.0, 0.2);
        let weak_fvg = candidate(30, 38.0, -2.0, 2.0, 3.0);
        let strong_fvg = candidate(8, 64.0, 1.2, 1.0, 0.7);
        let candidates = vec![
            (base, "base".to_string()),
            (
                SmcScanCandidateReport {
                    tuning: SmartMoneyConceptsBacktestTuning {
                        enable_fair_value_gap: true,
                        ..Default::default()
                    },
                    ..weak_fvg
                },
                "weak_fvg".to_string(),
            ),
            (
                SmcScanCandidateReport {
                    tuning: SmartMoneyConceptsBacktestTuning {
                        enable_fair_value_gap: true,
                        ..Default::default()
                    },
                    ..strong_fvg
                },
                "strong_fvg".to_string(),
            ),
        ];

        let ranked = mode_candidates_by_quality(&candidates, "fvg");

        assert_eq!(ranked[0].1, "strong_fvg");
        assert_eq!(ranked[1].1, "weak_fvg");
    }

    #[test]
    fn tuning_grid_includes_higher_frequency_structure_variants() {
        let tunings = smc_scan_tunings();

        assert!(
            tunings.len() <= 384,
            "SMC daily scan grid should stay small enough for local iteration; got {}",
            tunings.len()
        );
        assert!(tunings.iter().any(|tuning| {
            tuning.pivot_confirmation_bars == 3
                && tuning.cooldown_candles == 0
                && !tuning.thresholds.require_retest
                && tuning.thresholds.target_r_1 <= 0.25
        }));
        assert!(tunings.iter().any(|tuning| {
            tuning.thresholds.require_retest && tuning.retest_max_wait_candles == 4
        }));
        assert!(tunings.iter().any(|tuning| {
            tuning.enable_liquidity_sweep && tuning.thresholds.max_entry_extension_atr >= 6.0
        }));
        assert!(tunings.iter().any(|tuning| {
            tuning.enable_fair_value_gap && tuning.thresholds.max_entry_extension_atr >= 6.0
        }));
        assert!(tunings.iter().any(|tuning| tuning.allow_short));
        assert!(tunings
            .iter()
            .any(|tuning| tuning.enable_fair_value_gap && tuning.fade_signal));
        assert!(tunings
            .iter()
            .any(|tuning| tuning.thresholds.min_displacement_body_atr >= 0.5));
        assert!(tunings
            .iter()
            .any(|tuning| tuning.thresholds.require_premium_discount_zone));
    }
}
