use anyhow::{anyhow, Result};
use rust_quant_strategies::framework::backtest::types::{BackTestResult, BasicRiskStrategyConfig};
use rust_quant_strategies::implementations::{
    KeltnerChannelScalperAction, KeltnerChannelScalperBacktestTuning,
    KeltnerChannelScalperEntryMode, KeltnerChannelScalperSignalSnapshot,
    KeltnerChannelScalperStrategy, KeltnerChannelScalperThresholds,
};
use rust_quant_strategies::CandleItem;
use std::{collections::BTreeMap, sync::Arc};

#[path = "keltner_channel_scalper_1m/failure_exit.rs"]
mod failure_exit;
#[cfg(test)]
pub(super) use failure_exit::{
    keltner_failure_exit_decision, summarize_keltner_failure_exit_overlay_reports,
    KeltnerFailureExitConfig, KeltnerFailureExitSide,
};
pub(super) use failure_exit::{
    keltner_failure_exit_overlay_report, print_keltner_failure_exit_overlay_summaries,
    KELTNER_FAILURE_EXIT_PROFILES,
};

const MIN_KELTNER_TRADES_PER_DAY: f64 = 4.0;
const KELTNER_DEFAULT_TIER_CLOSE_RATIO_1: f64 = 0.40;
const KELTNER_DEFAULT_TIER_CLOSE_RATIO_2: f64 = 0.50;
const KELTNER_TIER_CLOSE_RATIO_PROFILES: [(f64, f64); 4] =
    [(0.40, 0.50), (0.50, 0.40), (0.60, 0.30), (0.70, 0.50)];

/// Runs the research-only Keltner Channel 1m scalp strategy with default tuning.
pub(super) fn run_keltner_channel_scalper_1m(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
) -> BackTestResult {
    KeltnerChannelScalperStrategy.run_test(inst_id, candles, keltner_risk_config(risk))
}

/// Loads BTC/ETH Keltner research cases; default remains 1m unless a period label is requested.
pub(super) async fn load_keltner_channel_scalper_cases(
    limit: usize,
    case_label: Option<&str>,
) -> Result<Vec<super::LoadedCase>> {
    let cases = [
        super::StrategyCase {
            label: "keltner_btc_1m",
            symbol: "BTC-USDT-SWAP",
            period: "1m",
            family: super::StrategyFamily::KeltnerChannelScalper1m,
        },
        super::StrategyCase {
            label: "keltner_eth_1m",
            symbol: "ETH-USDT-SWAP",
            period: "1m",
            family: super::StrategyFamily::KeltnerChannelScalper1m,
        },
        super::StrategyCase {
            label: "keltner_btc_5m",
            symbol: "BTC-USDT-SWAP",
            period: "5m",
            family: super::StrategyFamily::KeltnerChannelScalper1m,
        },
        super::StrategyCase {
            label: "keltner_eth_5m",
            symbol: "ETH-USDT-SWAP",
            period: "5m",
            family: super::StrategyFamily::KeltnerChannelScalper1m,
        },
        super::StrategyCase {
            label: "keltner_btc_15m",
            symbol: "BTC-USDT-SWAP",
            period: "15m",
            family: super::StrategyFamily::KeltnerChannelScalper1m,
        },
        super::StrategyCase {
            label: "keltner_eth_15m",
            symbol: "ETH-USDT-SWAP",
            period: "15m",
            family: super::StrategyFamily::KeltnerChannelScalper1m,
        },
    ];
    let cases = cases
        .into_iter()
        .filter(|case| keltner_case_matches_filter(case, case_label))
        .collect::<Vec<_>>();
    if cases.is_empty() {
        return Err(anyhow!(
            "no Keltner Channel case matched --case-label {:?}",
            case_label
        ));
    }

    let mut loaded = Vec::with_capacity(cases.len());
    for case in cases {
        loaded.push(super::LoadedCase {
            candles: super::load_sharded_candles(case.symbol, case.period, limit).await?,
            case,
            context: super::BacktestMarketContext::default(),
            context_required: false,
        });
    }
    Ok(loaded)
}

fn keltner_case_matches_filter(case: &super::StrategyCase, case_label: Option<&str>) -> bool {
    match case_label {
        Some(label) => case.label == label || label == format!("keltner_{}", case.period),
        None => case.period == "1m",
    }
}

/// Prints a compact bounded grid scan for Keltner Channel 1m risk/exit iteration.
pub(super) fn print_keltner_channel_scalper_scan(
    loaded_cases: &[super::LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = keltner_risk_config(super::strategy_family_risk_config(
        risk_percent,
        trade_fee_rate,
    ));
    let keltner_cases = loaded_cases
        .iter()
        .filter(|loaded| {
            matches!(
                loaded.case.family,
                super::StrategyFamily::KeltnerChannelScalper1m
            )
        })
        .collect::<Vec<_>>();
    if keltner_cases.is_empty() {
        println!("no_keltner_channel_scalper_cases source=quant_core_sharded");
        return;
    }

    let tunings = keltner_channel_scalper_scan_tunings();
    let snapshot_cache_sets = build_keltner_snapshot_cache_sets(&tunings, &keltner_cases);

    let mut density_reports = tunings
        .iter()
        .map(|tuning| {
            let cache_set = keltner_snapshot_cache_set(&snapshot_cache_sets, *tuning);
            screen_keltner_signal_density_for_cases(
                *tuning,
                &keltner_cases,
                &cache_set.snapshot_caches,
            )
        })
        .collect::<Vec<_>>();
    density_reports.sort_by(|left, right| {
        right
            .signals_per_day
            .total_cmp(&left.signals_per_day)
            .then_with(|| right.signals.cmp(&left.signals))
    });
    let tunings_to_backtest = density_reports
        .iter()
        .filter(|report| report.signals_per_day >= MIN_KELTNER_TRADES_PER_DAY)
        .map(|report| report.tuning)
        .collect::<Vec<_>>();
    println!(
        "keltner_density_filter total={} qualified={} min_signals_per_day={:.2}",
        density_reports.len(),
        tunings_to_backtest.len(),
        MIN_KELTNER_TRADES_PER_DAY
    );
    for report in density_reports.iter().take(8) {
        print_keltner_density("keltner_density_top", report);
    }

    let mut candidates = Vec::new();
    let mut raw_candidates = Vec::new();
    for tuning in tunings_to_backtest {
        let cache_set = keltner_snapshot_cache_set(&snapshot_cache_sets, tuning);
        let mut reports = Vec::with_capacity(keltner_cases.len());
        for (case_index, loaded) in keltner_cases.iter().enumerate() {
            let result = KeltnerChannelScalperStrategy
                .run_test_with_precomputed_snapshots_for_timeframe(
                    loaded.case.symbol,
                    loaded.case.period,
                    &loaded.candles,
                    risk,
                    tuning,
                    Arc::clone(&cache_set.snapshot_caches[case_index]),
                );
            reports.push(super::build_report(
                loaded.case.label,
                &loaded.candles,
                &result,
            ));
        }
        let summary = summarize_keltner_reports(&reports);
        raw_candidates.push(KeltnerScanCandidateReport { tuning, ..summary });
        if summary.win_rate_pct >= 60.0
            && summary.max_drawdown_pct < 15.0
            && summary.pnl > 0.0
            && summary.remove_top5_pnl > 0.0
            && summary.trades_per_day >= MIN_KELTNER_TRADES_PER_DAY
        {
            candidates.push(KeltnerScanCandidateReport { tuning, ..summary });
        }
    }

    raw_candidates.sort_by(|left, right| {
        right
            .pnl
            .total_cmp(&left.pnl)
            .then_with(|| right.win_rate_pct.total_cmp(&left.win_rate_pct))
            .then_with(|| right.trades_per_day.total_cmp(&left.trades_per_day))
    });
    for candidate in raw_candidates.iter().take(12) {
        print_keltner_candidate("keltner_raw_top", candidate);
    }

    candidates.sort_by(|left, right| {
        right
            .pnl
            .total_cmp(&left.pnl)
            .then_with(|| right.trades_per_day.total_cmp(&left.trades_per_day))
    });
    if candidates.is_empty() {
        println!(
            "no_keltner_channel_scalper_candidates source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,remove_top5_pnl>0,min_trades_per_day>=4"
        );
        return;
    }
    for candidate in candidates.iter().take(20) {
        print_keltner_candidate("keltner_candidate", candidate);
    }
}

/// Runs the strongest current raw Keltner tuning and prints trade-shape diagnostics.
pub(super) fn print_keltner_channel_scalper_diagnostics(
    loaded_cases: &[super::LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let base_risk = super::strategy_family_risk_config(risk_percent, trade_fee_rate);
    let risk = keltner_risk_config(base_risk);
    let tuning = best_keltner_raw_tuning();
    let mut reports = Vec::new();
    let mut failure_exit_reports = KELTNER_FAILURE_EXIT_PROFILES
        .iter()
        .map(|config| (*config, Vec::new()))
        .collect::<Vec<_>>();
    for loaded in loaded_cases.iter().filter(|loaded| {
        matches!(
            loaded.case.family,
            super::StrategyFamily::KeltnerChannelScalper1m
        )
    }) {
        let result = KeltnerChannelScalperStrategy.run_test_with_tuning_for_timeframe(
            loaded.case.symbol,
            loaded.case.period,
            &loaded.candles,
            risk,
            tuning,
        );
        let report = super::build_report(loaded.case.label, &loaded.candles, &result);
        for (config, overlay_reports) in &mut failure_exit_reports {
            overlay_reports.push(keltner_failure_exit_overlay_report(
                loaded.case.label,
                &loaded.candles,
                &result,
                risk,
                *config,
            ));
        }
        println!(
            "keltner_best_case label={} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2}",
            report.label,
            report.entries,
            report.wins,
            report.losses,
            report.win_rate_pct,
            report.pnl,
            report.max_drawdown_pct,
            report.trades_per_day
        );
        reports.push(report);
    }
    if reports.is_empty() {
        println!("no_keltner_channel_scalper_cases source=quant_core_sharded");
        return;
    }

    let summary = summarize_keltner_reports(&reports);
    print_keltner_candidate(
        "keltner_best_diagnostic",
        &KeltnerScanCandidateReport { tuning, ..summary },
    );
    for close_type in keltner_close_type_summaries(&reports) {
        println!(
            "keltner_best_close_type close_type={} count={} wins={} losses={} pnl={:.4} avg_pnl={:.4}",
            close_type.close_type,
            close_type.count,
            close_type.wins,
            close_type.losses,
            close_type.pnl,
            close_type.avg_pnl
        );
    }
    let trades = reports
        .iter()
        .flat_map(|report| report.trades.iter().cloned())
        .collect::<Vec<_>>();
    print_keltner_shape("win", &keltner_shape_summary_for_outcome(&trades, true));
    print_keltner_shape("loss", &keltner_shape_summary_for_outcome(&trades, false));
    for (level_1_close_ratio, level_2_close_ratio) in KELTNER_TIER_CLOSE_RATIO_PROFILES {
        print_keltner_tier_profile_summary(
            loaded_cases,
            base_risk,
            tuning,
            level_1_close_ratio,
            level_2_close_ratio,
        );
    }
    for profile_tuning in keltner_basis_cross_profile_tunings(tuning) {
        print_keltner_profile_summary(
            "keltner_best_basis_cross_profile",
            loaded_cases,
            base_risk,
            profile_tuning,
        );
    }
    print_keltner_failure_exit_overlay_summaries(&failure_exit_reports);
}

fn print_keltner_tier_profile_summary(
    loaded_cases: &[super::LoadedCase],
    base_risk: BasicRiskStrategyConfig,
    tuning: KeltnerChannelScalperBacktestTuning,
    level_1_close_ratio: f64,
    level_2_close_ratio: f64,
) {
    let risk = keltner_risk_config_with_tiers(base_risk, level_1_close_ratio, level_2_close_ratio);
    let reports = loaded_cases
        .iter()
        .filter(|loaded| {
            matches!(
                loaded.case.family,
                super::StrategyFamily::KeltnerChannelScalper1m
            )
        })
        .map(|loaded| {
            let result = KeltnerChannelScalperStrategy.run_test_with_tuning_for_timeframe(
                loaded.case.symbol,
                loaded.case.period,
                &loaded.candles,
                risk,
                tuning,
            );
            super::build_report(loaded.case.label, &loaded.candles, &result)
        })
        .collect::<Vec<_>>();
    if reports.is_empty() {
        return;
    }

    let summary = summarize_keltner_reports(&reports);
    println!(
        "keltner_best_tier_profile level1_close={:.2} level2_close={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
        level_1_close_ratio,
        level_2_close_ratio,
        summary.entries,
        summary.wins,
        summary.losses,
        summary.win_rate_pct,
        summary.pnl,
        summary.max_drawdown_pct,
        summary.trades_per_day,
        summary.early_win_rate_pct,
        summary.early_pnl,
        summary.late_win_rate_pct,
        summary.late_pnl,
        summary.remove_top5_pnl
    );
}

pub(super) fn best_keltner_raw_tuning() -> KeltnerChannelScalperBacktestTuning {
    KeltnerChannelScalperBacktestTuning {
        cooldown_candles: 6,
        reentry_lookback_candles: 3,
        allow_long: true,
        allow_short: false,
        confirm_next_candle: false,
        entry_mode: KeltnerChannelScalperEntryMode::Reversal,
        thresholds: KeltnerChannelScalperThresholds {
            stop_atr_mult: 3.0,
            target_r_1: 0.75,
            target_r_2: 1.25,
            target_r_3: 2.0,
            min_inner_reclaim_atr: 0.0,
            min_reentry_close_progress_ratio: 0.65,
            min_atr_pct: 0.06,
            ..KeltnerChannelScalperThresholds::default()
        },
    }
}

pub(super) fn keltner_basis_cross_profile_tunings(
    baseline: KeltnerChannelScalperBacktestTuning,
) -> [KeltnerChannelScalperBacktestTuning; 2] {
    let mut basis_cross = baseline;
    basis_cross.thresholds.require_basis_cross = true;
    [baseline, basis_cross]
}

fn print_keltner_profile_summary(
    prefix: &str,
    loaded_cases: &[super::LoadedCase],
    base_risk: BasicRiskStrategyConfig,
    tuning: KeltnerChannelScalperBacktestTuning,
) {
    let risk = keltner_risk_config(base_risk);
    let reports = loaded_cases
        .iter()
        .filter(|loaded| {
            matches!(
                loaded.case.family,
                super::StrategyFamily::KeltnerChannelScalper1m
            )
        })
        .map(|loaded| {
            let result = KeltnerChannelScalperStrategy.run_test_with_tuning_for_timeframe(
                loaded.case.symbol,
                loaded.case.period,
                &loaded.candles,
                risk,
                tuning,
            );
            super::build_report(loaded.case.label, &loaded.candles, &result)
        })
        .collect::<Vec<_>>();
    if reports.is_empty() {
        return;
    }

    let summary = summarize_keltner_reports(&reports);
    print_keltner_candidate(prefix, &KeltnerScanCandidateReport { tuning, ..summary });
}

/// Builds a small, non-coin-specific scan grid around the requested fixed indicator setup.
pub(super) fn keltner_channel_scalper_scan_tunings() -> Vec<KeltnerChannelScalperBacktestTuning> {
    let mut tunings = Vec::new();
    for (allow_long, allow_short) in [(true, true), (true, false), (false, true)] {
        for entry_mode in [
            KeltnerChannelScalperEntryMode::Reversal,
            KeltnerChannelScalperEntryMode::ExtremeMomentumReversal,
        ] {
            let min_atr_pct_values: &[f64] = match entry_mode {
                KeltnerChannelScalperEntryMode::Reversal => &[0.0, 0.06, 0.08],
                KeltnerChannelScalperEntryMode::Continuation
                | KeltnerChannelScalperEntryMode::ExtremeMomentumReversal => &[0.0],
            };
            for confirm_next_candle in [false, true] {
                for cooldown_candles in [3_usize, 6] {
                    for reentry_lookback_candles in [3_usize] {
                        for stop_atr_mult in [2.0, 2.5, 3.0] {
                            for (target_r_1, target_r_2, target_r_3) in
                                [(0.75, 1.25, 2.0), (1.0, 2.0, 3.0)]
                            {
                                for &min_atr_pct in min_atr_pct_values {
                                    for max_inner_reclaim_atr in [0.0] {
                                        for require_basis_cross in [false, true] {
                                            for min_reentry_close_progress_ratio in [0.0, 0.65] {
                                                for min_rejection_wick_ratio in [0.0, 0.2, 0.3] {
                                                    for min_long_adx in [0.0] {
                                                        for min_inner_reclaim_atr in [0.0, 0.15] {
                                                            for max_breakout_reentry_candles in
                                                                [0_usize]
                                                            {
                                                                tunings.push(
                                                                KeltnerChannelScalperBacktestTuning {
                                                            cooldown_candles,
                                                            reentry_lookback_candles,
                                                            allow_long,
                                                            allow_short,
                                                            confirm_next_candle,
                                                            entry_mode,
                                                            thresholds:
                                                                KeltnerChannelScalperThresholds {
                                                                    stop_atr_mult,
                                                                    target_r_1,
                                                                    target_r_2,
                                                                    target_r_3,
                                                                    min_reentry_body_ratio: 0.0,
                                                                    max_reentry_body_ratio: 0.0,
                                                                    min_rejection_wick_ratio,
                                                                    min_long_adx,
                                                                    min_inner_reclaim_atr,
                                                                    max_inner_reclaim_atr,
                                                                    require_basis_cross,
                                                                    min_reentry_close_progress_ratio,
                                                                    max_breakout_reentry_candles,
                                                                    min_atr_pct,
                                                                    min_basis_slope_atr: 0.0,
                                                                    max_adverse_basis_slope_atr:
                                                                        0.0,
                                                                    ..KeltnerChannelScalperThresholds::default()
                                                                },
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
            }
        }
    }
    tunings
}

struct KeltnerSnapshotCacheSet {
    input_tuning: KeltnerChannelScalperBacktestTuning,
    snapshot_caches: Vec<Arc<Vec<Option<KeltnerChannelScalperSignalSnapshot>>>>,
}

fn build_keltner_snapshot_cache_sets(
    tunings: &[KeltnerChannelScalperBacktestTuning],
    keltner_cases: &[&super::LoadedCase],
) -> Vec<KeltnerSnapshotCacheSet> {
    let mut cache_sets = Vec::new();
    for &tuning in tunings {
        if cache_sets.iter().any(|set: &KeltnerSnapshotCacheSet| {
            share_keltner_snapshot_inputs(set.input_tuning, tuning)
        }) {
            continue;
        }
        let snapshot_caches = keltner_cases
            .iter()
            .map(|loaded| {
                Arc::new(
                    KeltnerChannelScalperStrategy::precompute_backtest_snapshots_for_timeframe(
                        loaded.case.symbol,
                        loaded.case.period,
                        &loaded.candles,
                        tuning,
                    ),
                )
            })
            .collect::<Vec<_>>();
        cache_sets.push(KeltnerSnapshotCacheSet {
            input_tuning: tuning,
            snapshot_caches,
        });
    }
    cache_sets
}

fn keltner_snapshot_cache_set(
    cache_sets: &[KeltnerSnapshotCacheSet],
    tuning: KeltnerChannelScalperBacktestTuning,
) -> &KeltnerSnapshotCacheSet {
    cache_sets
        .iter()
        .find(|set| share_keltner_snapshot_inputs(set.input_tuning, tuning))
        .expect("Keltner snapshot cache set should be built for every scan tuning")
}

#[derive(Debug, Clone, Copy)]
pub(super) struct KeltnerSignalDensityReport {
    pub(super) tuning: KeltnerChannelScalperBacktestTuning,
    pub(super) signals: usize,
    pub(super) long_signals: usize,
    pub(super) short_signals: usize,
    pub(super) days: f64,
    pub(super) signals_per_day: f64,
}

pub(super) fn screen_keltner_signal_density_for_case(
    tuning: KeltnerChannelScalperBacktestTuning,
    candles: &[CandleItem],
    snapshots: &[Option<KeltnerChannelScalperSignalSnapshot>],
) -> KeltnerSignalDensityReport {
    let mut cooldown_remaining = 0_usize;
    let mut signals = 0_usize;
    let mut long_signals = 0_usize;
    let mut short_signals = 0_usize;
    for (index, snapshot) in snapshots.iter().enumerate().take(candles.len()) {
        if cooldown_remaining > 0 {
            cooldown_remaining -= 1;
            continue;
        }
        let Some(snapshot) = snapshot else {
            continue;
        };
        let decision = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
            &tuning.thresholds,
            snapshot,
            tuning.entry_mode,
        );
        let action = match decision.action {
            KeltnerChannelScalperAction::Long if tuning.allow_long => {
                KeltnerChannelScalperAction::Long
            }
            KeltnerChannelScalperAction::Short if tuning.allow_short => {
                KeltnerChannelScalperAction::Short
            }
            _ => continue,
        };
        if index >= super::BACKTEST_SIGNAL_WARMUP_CANDLES {
            signals += 1;
            match action {
                KeltnerChannelScalperAction::Long => long_signals += 1,
                KeltnerChannelScalperAction::Short => short_signals += 1,
                KeltnerChannelScalperAction::Flat => {}
            }
        }
        cooldown_remaining = tuning.cooldown_candles;
    }
    let days = super::candle_span_days(candles);
    KeltnerSignalDensityReport {
        tuning,
        signals,
        long_signals,
        short_signals,
        days,
        signals_per_day: if days > 0.0 {
            signals as f64 / days
        } else {
            0.0
        },
    }
}

fn screen_keltner_signal_density_for_cases(
    tuning: KeltnerChannelScalperBacktestTuning,
    keltner_cases: &[&super::LoadedCase],
    snapshot_caches: &[Arc<Vec<Option<KeltnerChannelScalperSignalSnapshot>>>],
) -> KeltnerSignalDensityReport {
    let mut signals = 0_usize;
    let mut long_signals = 0_usize;
    let mut short_signals = 0_usize;
    let mut days = 0.0_f64;
    for (case_index, loaded) in keltner_cases.iter().enumerate() {
        let report = screen_keltner_signal_density_for_case(
            tuning,
            &loaded.candles,
            &snapshot_caches[case_index],
        );
        signals += report.signals;
        long_signals += report.long_signals;
        short_signals += report.short_signals;
        days = days.max(report.days);
    }
    KeltnerSignalDensityReport {
        tuning,
        signals,
        long_signals,
        short_signals,
        days,
        signals_per_day: if days > 0.0 {
            signals as f64 / days
        } else {
            0.0
        },
    }
}

pub(crate) fn share_keltner_snapshot_inputs(
    left: KeltnerChannelScalperBacktestTuning,
    right: KeltnerChannelScalperBacktestTuning,
) -> bool {
    let left_thresholds = left.thresholds;
    let right_thresholds = right.thresholds;
    left.reentry_lookback_candles == right.reentry_lookback_candles
        && left.confirm_next_candle == right.confirm_next_candle
        && left_thresholds.keltner_length == right_thresholds.keltner_length
        && left_thresholds.outer_multiplier == right_thresholds.outer_multiplier
        && left_thresholds.inner_multiplier == right_thresholds.inner_multiplier
        && left_thresholds.adx_trend_length == right_thresholds.adx_trend_length
        && left_thresholds.adx_smoothing == right_thresholds.adx_smoothing
}

pub(crate) fn keltner_risk_config(risk: BasicRiskStrategyConfig) -> BasicRiskStrategyConfig {
    keltner_risk_config_with_tiers(
        risk,
        KELTNER_DEFAULT_TIER_CLOSE_RATIO_1,
        KELTNER_DEFAULT_TIER_CLOSE_RATIO_2,
    )
}

pub(crate) fn keltner_risk_config_with_tiers(
    mut risk: BasicRiskStrategyConfig,
    level_1_close_ratio: f64,
    level_2_close_ratio: f64,
) -> BasicRiskStrategyConfig {
    // Keltner 1m research uses the shared three-stage ATR/R engine:
    // 1R closes part, 2R closes part and 3R fully exits the remaining runner.
    risk.atr_take_profit_ratio = None;
    risk.fixed_signal_kline_take_profit_ratio = None;
    risk.tiered_take_profit_level_1_close_ratio = Some(level_1_close_ratio);
    risk.tiered_take_profit_level_2_close_ratio = Some(level_2_close_ratio);
    risk
}

#[derive(Debug, Clone, Copy)]
struct KeltnerScanCandidateReport {
    tuning: KeltnerChannelScalperBacktestTuning,
    entries: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    max_drawdown_pct: f64,
    trades_per_day: f64,
    early_win_rate_pct: f64,
    early_pnl: f64,
    late_win_rate_pct: f64,
    late_pnl: f64,
    remove_top5_pnl: f64,
}

fn summarize_keltner_reports(reports: &[super::CaseReport]) -> KeltnerScanCandidateReport {
    let wins = reports.iter().map(|report| report.wins).sum::<usize>();
    let losses = reports.iter().map(|report| report.losses).sum::<usize>();
    let pnl = reports.iter().map(|report| report.pnl).sum::<f64>();
    let entries = reports.iter().map(|report| report.entries).sum::<usize>();
    let max_drawdown_pct = reports
        .iter()
        .map(|report| report.max_drawdown_pct)
        .fold(0.0, f64::max);
    let combo_days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    let mut trades = reports
        .iter()
        .flat_map(|report| report.trades.iter().cloned())
        .collect::<Vec<_>>();
    trades.sort_unstable_by(|left, right| left.open_time.cmp(&right.open_time));
    let mid = trades.len() / 2;
    let (early_win_rate_pct, early_pnl) = summarize_keltner_trades(&trades[..mid]);
    let (late_win_rate_pct, late_pnl) = summarize_keltner_trades(&trades[mid..]);
    let mut without_top5 = trades.clone();
    without_top5.sort_unstable_by(|left, right| right.pnl.total_cmp(&left.pnl));
    let remove_top5_pnl = without_top5
        .iter()
        .skip(5)
        .map(|trade| trade.pnl)
        .sum::<f64>();

    KeltnerScanCandidateReport {
        tuning: KeltnerChannelScalperBacktestTuning::default(),
        entries,
        wins,
        losses,
        win_rate_pct: super::ratio_pct(wins, wins + losses),
        pnl,
        max_drawdown_pct,
        trades_per_day: if combo_days > 0.0 {
            entries as f64 / combo_days
        } else {
            0.0
        },
        early_win_rate_pct,
        early_pnl,
        late_win_rate_pct,
        late_pnl,
        remove_top5_pnl,
    }
}

fn summarize_keltner_trades(trades: &[super::ClosedTradeDebug]) -> (f64, f64) {
    let wins = trades.iter().filter(|trade| trade.pnl > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.pnl < 0.0).count();
    let pnl = trades.iter().map(|trade| trade.pnl).sum::<f64>();
    (super::ratio_pct(wins, wins + losses), pnl)
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct KeltnerCloseTypeSummary {
    pub(super) close_type: String,
    pub(super) count: usize,
    pub(super) wins: usize,
    pub(super) losses: usize,
    pub(super) pnl: f64,
    pub(super) avg_pnl: f64,
}

pub(super) fn keltner_close_type_summaries(
    reports: &[super::CaseReport],
) -> Vec<KeltnerCloseTypeSummary> {
    let mut groups = BTreeMap::<String, Vec<&super::ClosedTradeDebug>>::new();
    for trade in reports.iter().flat_map(|report| report.trades.iter()) {
        groups
            .entry(trade.close_type.clone())
            .or_default()
            .push(trade);
    }
    let mut summaries = groups
        .into_iter()
        .map(|(close_type, trades)| {
            let count = trades.len();
            let wins = trades.iter().filter(|trade| trade.pnl > 0.0).count();
            let losses = trades.iter().filter(|trade| trade.pnl < 0.0).count();
            let pnl = trades.iter().map(|trade| trade.pnl).sum::<f64>();
            KeltnerCloseTypeSummary {
                close_type,
                count,
                wins,
                losses,
                pnl,
                avg_pnl: if count > 0 { pnl / count as f64 } else { 0.0 },
            }
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.close_type.cmp(&right.close_type))
    });
    summaries
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct KeltnerShapeSummary {
    pub(super) count: usize,
    pub(super) avg_adx: f64,
    pub(super) avg_basis_slope_atr: f64,
    pub(super) avg_reclaim_atr: f64,
    pub(super) avg_body_ratio: f64,
    pub(super) avg_rejection_wick_ratio: f64,
    pub(super) avg_close_progress_ratio: f64,
    pub(super) avg_breakout_reentry_candles: f64,
    pub(super) avg_atr_pct: f64,
}

pub(super) fn keltner_shape_summary_for_outcome(
    trades: &[super::ClosedTradeDebug],
    winners: bool,
) -> KeltnerShapeSummary {
    let mut outcomes = BTreeMap::<&str, (f64, Option<super::KeltnerEntrySnapshotDebug>)>::new();
    for trade in trades {
        let entry = outcomes
            .entry(trade.open_time.as_str())
            .or_insert((0.0, None));
        entry.0 += trade.pnl;
        if entry.1.is_none() {
            entry.1 = trade.keltner_snapshot;
        }
    }
    let snapshots = outcomes
        .into_values()
        .filter(|(pnl, _)| if winners { *pnl > 0.0 } else { *pnl < 0.0 })
        .filter_map(|(_, snapshot)| snapshot)
        .collect::<Vec<_>>();
    let count = snapshots.len();
    if count == 0 {
        return KeltnerShapeSummary::default();
    }
    let divisor = count as f64;
    KeltnerShapeSummary {
        count,
        avg_adx: snapshots.iter().map(|snapshot| snapshot.adx).sum::<f64>() / divisor,
        avg_basis_slope_atr: snapshots
            .iter()
            .map(|snapshot| snapshot.basis_slope_atr)
            .sum::<f64>()
            / divisor,
        avg_reclaim_atr: snapshots
            .iter()
            .map(|snapshot| snapshot.reclaim_atr)
            .sum::<f64>()
            / divisor,
        avg_body_ratio: snapshots
            .iter()
            .map(|snapshot| snapshot.reentry_body_ratio)
            .sum::<f64>()
            / divisor,
        avg_rejection_wick_ratio: snapshots
            .iter()
            .map(|snapshot| snapshot.rejection_wick_ratio)
            .sum::<f64>()
            / divisor,
        avg_close_progress_ratio: snapshots
            .iter()
            .map(|snapshot| snapshot.reentry_close_progress_ratio)
            .sum::<f64>()
            / divisor,
        avg_breakout_reentry_candles: snapshots
            .iter()
            .map(|snapshot| snapshot.breakout_reentry_candles)
            .sum::<f64>()
            / divisor,
        avg_atr_pct: snapshots
            .iter()
            .map(|snapshot| snapshot.atr_pct)
            .sum::<f64>()
            / divisor,
    }
}

fn print_keltner_shape(outcome: &str, summary: &KeltnerShapeSummary) {
    println!(
        "keltner_best_shape outcome={} count={} avg_adx={:.2} avg_basis_slope_atr={:.4} avg_reclaim_atr={:.4} avg_body={:.4} avg_wick={:.4} avg_close_progress={:.4} avg_breakout_reentry_candles={:.4} avg_atr_pct={:.4}",
        outcome,
        summary.count,
        summary.avg_adx,
        summary.avg_basis_slope_atr,
        summary.avg_reclaim_atr,
        summary.avg_body_ratio,
        summary.avg_rejection_wick_ratio,
        summary.avg_close_progress_ratio,
        summary.avg_breakout_reentry_candles,
        summary.avg_atr_pct
    );
}

fn print_keltner_candidate(prefix: &str, candidate: &KeltnerScanCandidateReport) {
    let thresholds = candidate.tuning.thresholds;
    println!(
        "{prefix} entry_mode={:?} allow_long={} allow_short={} confirm_next={} cooldown={} reentry={} stop_atr={:.2} body={:.2} max_body={:.2} wick={:.2} close_progress={:.2} basis_cross={} max_reentry_candles={} long_adx_min={:.2} reclaim_atr={:.2} max_reclaim_atr={:.2} min_atr_pct={:.4} basis_slope_min={:.2} adverse_slope_max={:.2} r1={:.2} r2={:.2} r3={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
        candidate.tuning.entry_mode,
        candidate.tuning.allow_long,
        candidate.tuning.allow_short,
        candidate.tuning.confirm_next_candle,
        candidate.tuning.cooldown_candles,
        candidate.tuning.reentry_lookback_candles,
        thresholds.stop_atr_mult,
        thresholds.min_reentry_body_ratio,
        thresholds.max_reentry_body_ratio,
        thresholds.min_rejection_wick_ratio,
        thresholds.min_reentry_close_progress_ratio,
        thresholds.require_basis_cross,
        thresholds.max_breakout_reentry_candles,
        thresholds.min_long_adx,
        thresholds.min_inner_reclaim_atr,
        thresholds.max_inner_reclaim_atr,
        thresholds.min_atr_pct,
        thresholds.min_basis_slope_atr,
        thresholds.max_adverse_basis_slope_atr,
        thresholds.target_r_1,
        thresholds.target_r_2,
        thresholds.target_r_3,
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

fn print_keltner_density(prefix: &str, report: &KeltnerSignalDensityReport) {
    let thresholds = report.tuning.thresholds;
    println!(
        "{prefix} entry_mode={:?} allow_long={} allow_short={} confirm_next={} cooldown={} reentry={} body={:.2} max_body={:.2} wick={:.2} close_progress={:.2} basis_cross={} max_reentry_candles={} long_adx_min={:.2} reclaim_atr={:.2} max_reclaim_atr={:.2} min_atr_pct={:.4} basis_slope_min={:.2} adverse_slope_max={:.2} signals={} long_signals={} short_signals={} signals_per_day={:.2}",
        report.tuning.entry_mode,
        report.tuning.allow_long,
        report.tuning.allow_short,
        report.tuning.confirm_next_candle,
        report.tuning.cooldown_candles,
        report.tuning.reentry_lookback_candles,
        thresholds.min_reentry_body_ratio,
        thresholds.max_reentry_body_ratio,
        thresholds.min_rejection_wick_ratio,
        thresholds.min_reentry_close_progress_ratio,
        thresholds.require_basis_cross,
        thresholds.max_breakout_reentry_candles,
        thresholds.min_long_adx,
        thresholds.min_inner_reclaim_atr,
        thresholds.max_inner_reclaim_atr,
        thresholds.min_atr_pct,
        thresholds.min_basis_slope_atr,
        thresholds.max_adverse_basis_slope_atr,
        report.signals,
        report.long_signals,
        report.short_signals,
        report.signals_per_day,
    );
}
