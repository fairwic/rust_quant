use super::*;

/// Prints candle-structure rejection counts for scalper cases without running trades.
pub(super) fn print_scalper_diagnostics(loaded_cases: &[LoadedCase]) {
    for loaded in loaded_cases
        .iter()
        .filter(|loaded| matches!(loaded.case.family, StrategyFamily::Scalper))
    {
        let diagnostics = scalper_setup_diagnostics(
            &loaded.candles,
            BtcEthLiquidityScalperBacktestTuning::default(),
        );
        println!(
            "scalper_diagnostics label={} candles={} samples={} classified={} confirmed={} no_trend={} top_reasons={}",
            loaded.case.label,
            loaded.candles.len(),
            diagnostics.samples,
            diagnostics.classified_windows(),
            diagnostics.confirmed,
            diagnostics.reason_count("NO_TREND"),
            format_scalper_diagnostic_reasons(&diagnostics)
        );
    }
}

/// Counts scalper candle-structure setup outcomes without changing strategy output.
pub(super) fn scalper_setup_diagnostics(
    candles: &[CandleItem],
    tuning: BtcEthLiquidityScalperBacktestTuning,
) -> ScalperSetupDiagnostics {
    let mut diagnostics = ScalperSetupDiagnostics::default();
    let window = scalper_diagnostic_window(tuning);
    for index in BACKTEST_SIGNAL_WARMUP_CANDLES..candles.len() {
        let end = index + 1;
        if end < window {
            continue;
        }
        let start = end - window;
        diagnostics.samples += 1;
        match diagnose_scalper_setup_window(&candles[start..end], &tuning) {
            Ok(()) => diagnostics.confirmed += 1,
            Err(reason) => *diagnostics.reasons.entry(reason).or_default() += 1,
        }
    }
    diagnostics
}

fn scalper_diagnostic_window(tuning: BtcEthLiquidityScalperBacktestTuning) -> usize {
    tuning
        .trend_slow_window
        .max(tuning.trend_fast_window)
        .max(12)
}

/// Formats the most frequent rejection reasons first so the next scan is evidence-led.
pub(super) fn format_scalper_diagnostic_reasons(diagnostics: &ScalperSetupDiagnostics) -> String {
    let mut reasons = diagnostics
        .reasons
        .iter()
        .map(|(reason, count)| (*reason, *count))
        .collect::<Vec<_>>();
    reasons.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    reasons
        .iter()
        .take(6)
        .map(|(reason, count)| format!("{reason}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Classifies one rolling scalper setup window using the same stage order as the strategy.
fn diagnose_scalper_setup_window(
    candles: &[CandleItem],
    tuning: &BtcEthLiquidityScalperBacktestTuning,
) -> Result<(), &'static str> {
    if candles.len() < scalper_diagnostic_window(*tuning) {
        return Err("INSUFFICIENT_DATA");
    }
    let Some(last) = candles.last() else {
        return Err("INSUFFICIENT_DATA");
    };
    let trend_fast_window = tuning.trend_fast_window.max(1).min(candles.len());
    let trend_slow_window = tuning.trend_slow_window.max(1).min(candles.len());
    let fast_trend = scalper_sma_close(&candles[candles.len() - trend_fast_window..]);
    let slow_trend = scalper_sma_close(&candles[candles.len() - trend_slow_window..]);
    let bias = if last.c > fast_trend && fast_trend > slow_trend {
        "long"
    } else if last.c < fast_trend && fast_trend < slow_trend {
        "short"
    } else {
        return Err("NO_TREND");
    };
    if bias == "short" && !tuning.allow_short {
        return Err("SHORT_DISABLED");
    }
    let slow_directional_ratio = scalper_directional_move_ratio(candles, trend_slow_window, bias);
    let fast_directional_ratio = scalper_directional_move_ratio(candles, trend_fast_window, bias);
    if slow_directional_ratio < tuning.min_directional_ratio_48
        || fast_directional_ratio < tuning.min_directional_ratio_24
    {
        return Err("WEAK_DIRECTIONAL_MOVE");
    }
    let impulse_index =
        scalper_recent_impulse_index(candles, bias, tuning).ok_or("MISSING_VOLUME_IMPULSE")?;
    if !scalper_has_pullback_and_resume(candles, impulse_index, bias, tuning) {
        return Err("PULLBACK_RESUME_MISSING");
    }
    if tuning.require_previous_extreme_break && !scalper_breaks_previous_candle(candles, bias) {
        return Err("PREVIOUS_EXTREME_MISSING");
    }
    Ok(())
}

/// Finds the recent directional impulse required before a scalper pullback can be traded.
fn scalper_recent_impulse_index(
    candles: &[CandleItem],
    bias: &str,
    tuning: &BtcEthLiquidityScalperBacktestTuning,
) -> Option<usize> {
    let start = candles.len().saturating_sub(12).max(1);
    let end = candles.len().saturating_sub(1);
    let avg_range = scalper_average_range(&candles[start - 1..end]).max(0.0001);
    let avg_volume = scalper_average_volume(&candles[start - 1..end]).max(0.0001);
    (start..end).rev().find(|index| {
        let current = &candles[*index];
        let previous = &candles[*index - 1];
        let move_size = current.c - previous.c;
        let range = (current.h - current.l).abs().max(0.0001);
        let body_ratio = (current.c - current.o).abs() / range;
        let volume_ok = current.v >= avg_volume * tuning.impulse_min_volume_mult;
        match bias {
            "long" => {
                move_size >= avg_range * tuning.impulse_move_range_mult
                    && current.c > current.o
                    && body_ratio >= tuning.impulse_min_body_ratio
                    && volume_ok
            }
            "short" => {
                move_size <= -avg_range * tuning.impulse_move_range_mult
                    && current.c < current.o
                    && body_ratio >= tuning.impulse_min_body_ratio
                    && volume_ok
            }
            _ => false,
        }
    })
}

/// Checks whether price pulled back after the impulse and then resumed without chasing too far.
fn scalper_has_pullback_and_resume(
    candles: &[CandleItem],
    impulse_index: usize,
    bias: &str,
    tuning: &BtcEthLiquidityScalperBacktestTuning,
) -> bool {
    let Some(last) = candles.last() else {
        return false;
    };
    let impulse = &candles[impulse_index];
    let after_impulse = &candles[impulse_index + 1..];
    if after_impulse.len() < 2 {
        return false;
    }
    let body = (impulse.c - impulse.o).abs().max(0.0001);
    match bias {
        "long" => {
            let pullback_low = after_impulse
                .iter()
                .map(|candle| candle.l)
                .fold(f64::INFINITY, f64::min);
            let depth = (impulse.c - pullback_low) / body;
            (tuning.pullback_min_depth..=tuning.pullback_max_depth).contains(&depth)
                && last.c > last.o
                && last.c <= impulse.h + body * tuning.resume_extension_body_mult
        }
        "short" => {
            let pullback_high = after_impulse
                .iter()
                .map(|candle| candle.h)
                .fold(f64::NEG_INFINITY, f64::max);
            let depth = (pullback_high - impulse.c) / body;
            (tuning.pullback_min_depth..=tuning.pullback_max_depth).contains(&depth)
                && last.c < last.o
                && last.c >= impulse.l - body * tuning.resume_extension_body_mult
        }
        _ => false,
    }
}

/// Keeps diagnostics aligned with the strategy's previous-candle break requirement.
fn scalper_breaks_previous_candle(candles: &[CandleItem], bias: &str) -> bool {
    if candles.len() < 2 {
        return false;
    }
    let last = &candles[candles.len() - 1];
    let previous = &candles[candles.len() - 2];
    match bias {
        "long" => last.c > previous.h,
        "short" => last.c < previous.l,
        _ => false,
    }
}

/// Measures how much of the lookback movement is in the execution bias direction.
fn scalper_directional_move_ratio(candles: &[CandleItem], lookback: usize, bias: &str) -> f64 {
    if candles.len() < 2 {
        return 0.0;
    }
    let lookback = lookback.min(candles.len() - 1);
    let start = candles.len() - lookback - 1;
    let window = &candles[start..];
    let Some(first) = window.first() else {
        return 0.0;
    };
    let Some(last) = window.last() else {
        return 0.0;
    };
    let directional_move = match bias {
        "long" => last.c - first.c,
        "short" => first.c - last.c,
        _ => return 0.0,
    };
    if directional_move <= 0.0 {
        return 0.0;
    }
    let total_move = window
        .windows(2)
        .map(|pair| (pair[1].c - pair[0].c).abs())
        .sum::<f64>();
    directional_move / total_move.max(0.0001)
}

/// Calculates the local range baseline used by the diagnostic impulse gate.
fn scalper_average_range(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles
        .iter()
        .map(|candle| (candle.h - candle.l).abs())
        .sum::<f64>()
        / candles.len() as f64
}

/// Calculates the local volume baseline used by the diagnostic impulse gate.
fn scalper_average_volume(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles.iter().map(|candle| candle.v).sum::<f64>() / candles.len() as f64
}

/// Calculates the simple moving average used by the diagnostic trend gate.
fn scalper_sma_close(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles.iter().map(|candle| candle.c).sum::<f64>() / candles.len() as f64
}

pub(super) fn print_scalper_scan(
    loaded_cases: &[LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    print_scalper_scan_with_tunings(
        loaded_cases,
        risk_percent,
        trade_fee_rate,
        scalper_scan_tunings(),
        "no_scalper_candidates",
    );
}

pub(super) fn print_scalper_scan_with_tunings(
    loaded_cases: &[LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
    tunings: Vec<BtcEthLiquidityScalperBacktestTuning>,
    empty_prefix: &str,
) {
    let risk = strategy_family_risk_config(risk_percent, trade_fee_rate);
    let non_scalper_reports = Vec::new();
    let scalper_cases = loaded_cases
        .iter()
        .filter(|loaded| matches!(loaded.case.family, StrategyFamily::Scalper))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    let mut raw_candidates = Vec::new();
    for tuning in tunings {
        let mut scalper_reports = Vec::with_capacity(scalper_cases.len());
        for loaded in &scalper_cases {
            let result = run_loaded_case(loaded, risk, Some(tuning), None);
            let report = build_report(loaded.case.label, &loaded.candles, &result);
            scalper_reports.push(report);
        }
        let summary = summarize_scalper_candidate_reports(&non_scalper_reports, &scalper_reports);
        let filtered_reason_counts = scalper_filter_counts(&non_scalper_reports, &scalper_reports);
        raw_candidates.push(ScalperScanCandidateReport {
            tuning,
            entries: summary.entries,
            wins: summary.wins,
            losses: summary.losses,
            win_rate_pct: summary.win_rate_pct,
            pnl: summary.pnl,
            max_drawdown_pct: summary.max_drawdown_pct,
            trades_per_day: summary.trades_per_day,
            early_win_rate_pct: summary.early_win_rate_pct,
            early_pnl: summary.early_pnl,
            late_win_rate_pct: summary.late_win_rate_pct,
            late_pnl: summary.late_pnl,
            remove_top5_pnl: summary.remove_top5_pnl,
            filtered_reason_counts: filtered_reason_counts.clone(),
        });
        if short_scan_candidate_meets_constraints(&summary) {
            candidates.push(ScalperScanCandidateReport {
                tuning,
                entries: summary.entries,
                wins: summary.wins,
                losses: summary.losses,
                win_rate_pct: summary.win_rate_pct,
                pnl: summary.pnl,
                max_drawdown_pct: summary.max_drawdown_pct,
                trades_per_day: summary.trades_per_day,
                early_win_rate_pct: summary.early_win_rate_pct,
                early_pnl: summary.early_pnl,
                late_win_rate_pct: summary.late_win_rate_pct,
                late_pnl: summary.late_pnl,
                remove_top5_pnl: summary.remove_top5_pnl,
                filtered_reason_counts,
            });
        }
    }
    sort_scalper_raw_candidates(&mut raw_candidates);
    for candidate in raw_candidates.iter().take(5) {
        println!(
            "scalper_raw_top allow_short={} require_oi={} trend_fast={} trend_slow={} cooldown={} dir48={:.2} dir24={:.2} impulse_move={:.2} body={:.2} volume={:.2} resume_ext={:.2} break_prev={} r1={:.2} r2={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4} top_filters={}",
            candidate.tuning.allow_short,
            candidate.tuning.require_oi_confirmation,
            candidate.tuning.trend_fast_window,
            candidate.tuning.trend_slow_window,
            candidate.tuning.cooldown_candles,
            candidate.tuning.min_directional_ratio_48,
            candidate.tuning.min_directional_ratio_24,
            candidate.tuning.impulse_move_range_mult,
            candidate.tuning.impulse_min_body_ratio,
            candidate.tuning.impulse_min_volume_mult,
            candidate.tuning.resume_extension_body_mult,
            candidate.tuning.require_previous_extreme_break,
            candidate.tuning.target_r_1,
            candidate.tuning.target_r_2,
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
            candidate.remove_top5_pnl,
            format_reason_counts(&candidate.filtered_reason_counts)
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
            "{empty_prefix} source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,remove_top5_pnl>0"
        );
        return;
    }
    for candidate in candidates.iter().take(20) {
        println!(
            "scalper_candidate allow_short={} require_oi={} trend_fast={} trend_slow={} cooldown={} dir48={:.2} dir24={:.2} impulse_move={:.2} body={:.2} volume={:.2} resume_ext={:.2} break_prev={} r1={:.2} r2={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4} top_filters={}",
            candidate.tuning.allow_short,
            candidate.tuning.require_oi_confirmation,
            candidate.tuning.trend_fast_window,
            candidate.tuning.trend_slow_window,
            candidate.tuning.cooldown_candles,
            candidate.tuning.min_directional_ratio_48,
            candidate.tuning.min_directional_ratio_24,
            candidate.tuning.impulse_move_range_mult,
            candidate.tuning.impulse_min_body_ratio,
            candidate.tuning.impulse_min_volume_mult,
            candidate.tuning.resume_extension_body_mult,
            candidate.tuning.require_previous_extreme_break,
            candidate.tuning.target_r_1,
            candidate.tuning.target_r_2,
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
            candidate.remove_top5_pnl,
            format_reason_counts(&candidate.filtered_reason_counts)
        );
    }
}

/// Summarizes only scalper reports so short-stack profits cannot validate a weak scalper preset.
pub(super) fn summarize_scalper_candidate_reports(
    _non_scalper_reports: &[CaseReport],
    scalper_reports: &[CaseReport],
) -> ScanCandidateReport {
    summarize_isolated_candidate_reports(scalper_reports)
}

/// Summarizes only breakdown reports so exhaustion or scalper profits cannot validate it.
pub(super) fn summarize_breakdown_candidate_reports(
    _non_breakdown_reports: &[CaseReport],
    breakdown_reports: &[CaseReport],
) -> ScanCandidateReport {
    summarize_isolated_candidate_reports(breakdown_reports)
}

/// Summarizes only exhaustion reports so other short-stack presets cannot validate it.
pub(super) fn summarize_exhaustion_candidate_reports(
    _non_exhaustion_reports: &[CaseReport],
    exhaustion_reports: &[CaseReport],
) -> ScanCandidateReport {
    summarize_isolated_candidate_reports(exhaustion_reports)
}

/// Shared scan helper for strategy-family isolation; default combo reports stay separate.
fn summarize_isolated_candidate_reports(candidate_reports: &[CaseReport]) -> ScanCandidateReport {
    summarize_reports(candidate_reports)
}

pub(super) fn merge_filtered_reason_counts(reports: &[CaseReport]) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::<String, usize>::new();
    for report in reports {
        for (reason, count) in &report.filtered_reason_counts {
            if !is_blocking_filter_reason(reason) {
                continue;
            }
            *counts.entry(reason.clone()).or_default() += *count;
        }
    }
    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    counts
}

pub(super) fn scalper_filter_counts(
    _non_scalper_reports: &[CaseReport],
    scalper_reports: &[CaseReport],
) -> Vec<(String, usize)> {
    merge_filtered_reason_counts(scalper_reports)
}

fn is_blocking_filter_reason(reason: &str) -> bool {
    !matches!(
        reason,
        "BTC_ETH_LIQUIDITY_SCALP_CONFIRMED" | "OI_NOT_CONFIRMED_REDUCE_SIZE"
    ) && !reason.starts_with("STOP_PRICE:")
}

pub(super) fn sort_scalper_raw_candidates(candidates: &mut [ScalperScanCandidateReport]) {
    candidates.sort_by(|left, right| {
        right
            .trades_per_day
            .total_cmp(&left.trades_per_day)
            .then_with(|| right.win_rate_pct.total_cmp(&left.win_rate_pct))
            .then_with(|| right.pnl.total_cmp(&left.pnl))
    });
}

fn summarize_reports(reports: &[CaseReport]) -> ScanCandidateReport {
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
    let (early_win_rate_pct, early_pnl) = summarize_trade_debug(&trades[..mid]);
    let (late_win_rate_pct, late_pnl) = summarize_trade_debug(&trades[mid..]);
    let mut without_top5 = trades.clone();
    without_top5.sort_unstable_by(|left, right| right.pnl.total_cmp(&left.pnl));
    let remove_top5_pnl = without_top5
        .iter()
        .skip(5)
        .map(|trade| trade.pnl)
        .sum::<f64>();
    ScanCandidateReport {
        tuning: BearShortStackBacktestTuning::default(),
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
        early_win_rate_pct,
        early_pnl,
        late_win_rate_pct,
        late_pnl,
        remove_top5_pnl,
    }
}

pub(super) fn short_scan_candidate_meets_constraints(summary: &ScanCandidateReport) -> bool {
    summary.win_rate_pct >= 60.0
        && summary.max_drawdown_pct < 15.0
        && summary.pnl > 0.0
        && summary.remove_top5_pnl > 0.0
}

pub(super) fn short_candidate_reports_meet_constraints(
    summary: &ScanCandidateReport,
    reports: &[CaseReport],
) -> bool {
    short_scan_candidate_meets_constraints(summary)
        && reports.iter().all(|report| {
            report.entries == 0
                || (report.pnl > 0.0 && (report.entries < 10 || report.win_rate_pct >= 60.0))
        })
}

pub(super) fn format_case_reports(reports: &[CaseReport]) -> String {
    let mut reports = reports.iter().collect::<Vec<_>>();
    reports.sort_by(|left, right| {
        right
            .entries
            .cmp(&left.entries)
            .then_with(|| left.label.cmp(&right.label))
    });
    reports
        .into_iter()
        .map(|report| {
            let (avg_win, avg_loss) = average_trade_pnls(&report.trades);
            format!(
                "{}:e{}/wr{:.2}/pnl{:.4}/aw{:.4}/al{:.4}",
                report.label, report.entries, report.win_rate_pct, report.pnl, avg_win, avg_loss
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn average_trade_pnls(trades: &[ClosedTradeDebug]) -> (f64, f64) {
    let wins = trades
        .iter()
        .filter(|trade| trade.pnl > 0.0)
        .map(|trade| trade.pnl)
        .collect::<Vec<_>>();
    let losses = trades
        .iter()
        .filter(|trade| trade.pnl < 0.0)
        .map(|trade| trade.pnl)
        .collect::<Vec<_>>();
    (average_or_zero(&wins), average_or_zero(&losses))
}

fn average_or_zero(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn summarize_trade_debug(trades: &[ClosedTradeDebug]) -> (f64, f64) {
    let wins = trades.iter().filter(|trade| trade.pnl > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.pnl < 0.0).count();
    let pnl = trades.iter().map(|trade| trade.pnl).sum::<f64>();
    (ratio_pct(wins, wins + losses), pnl)
}
