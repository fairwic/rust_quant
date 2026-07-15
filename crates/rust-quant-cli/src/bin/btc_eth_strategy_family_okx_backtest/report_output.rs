/// Prints per-case and combined backtest summaries in the CLI's plain-text format.
pub(super) fn print_reports(reports: &[super::CaseReport], debug_trades: bool) {
    let total_wins = reports.iter().map(|report| report.wins).sum::<usize>();
    let total_losses = reports.iter().map(|report| report.losses).sum::<usize>();
    let total_pnl = reports.iter().map(|report| report.pnl).sum::<f64>();
    let total_entries = reports.iter().map(|report| report.entries).sum::<usize>();
    let max_drawdown = reports
        .iter()
        .map(|report| report.max_drawdown_pct)
        .fold(0.0, f64::max);
    let combo_days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    let trades_per_day = if combo_days > 0.0 {
        total_entries as f64 / combo_days
    } else {
        0.0
    };

    for report in reports {
        println!(
            "{} source=quant_core_sharded candles={} entries={} closed={} wins={} losses={} win_rate={:.2}% pnl={:.4} final_funds={:.4} max_dd={:.2}% days={:.2} trades_per_day={:.2}",
            report.label,
            report.candles,
            report.entries,
            report.closed,
            report.wins,
            report.losses,
            report.win_rate_pct,
            report.pnl,
            report.final_funds,
            report.max_drawdown_pct,
            report.days,
            report.trades_per_day
        );
        if debug_trades {
            print_report_debug(report);
        }
    }

    println!(
        "combined source=quant_core_sharded entries={total_entries} wins={total_wins} losses={total_losses} win_rate={:.2}% pnl={total_pnl:.4} max_dd={max_drawdown:.2}% days={combo_days:.2} trades_per_day={trades_per_day:.2}",
        super::ratio_pct(total_wins, total_wins + total_losses)
    );
}

fn print_report_debug(report: &super::CaseReport) {
    if report.filtered_signals > 0 {
        println!(
            "  filtered_signals={} top_reasons={}",
            report.filtered_signals,
            format_reason_counts(&report.filtered_reason_counts)
        );
        for filtered in report.filtered_signal_snapshots.iter().take(6) {
            println!(
                "    filtered_signal ts={} reasons={} stop_dist={:.4}% atr={:.4}% oi_growth={:.4}% funding={:.6} long_short={:.4} taker_sell_buy={:.4}",
                filtered.ts,
                filtered.reasons.join(","),
                filtered.snapshot.stop_distance_pct,
                filtered.snapshot.atr_pct,
                filtered.snapshot.oi_growth_pct,
                filtered.snapshot.funding_rate,
                filtered.snapshot.long_short_ratio,
                filtered.snapshot.taker_sell_buy_ratio
            );
        }
    }
    for trade in &report.trades {
        println!(
            "  trade open={} close={:?} open_price={:.4} close_price={:?} pnl={:.4} close_type={}",
            trade.open_time,
            trade.close_time,
            trade.open_price,
            trade.close_price,
            trade.pnl,
            trade.close_type
        );
        if let Some(snapshot) = trade.entry_snapshot {
            println!(
                "    entry_snapshot stop_dist={:.4}% atr={:.4}% oi_growth={:.4}% funding={:.6} long_short={:.4} taker_sell_buy={:.4}",
                snapshot.stop_distance_pct,
                snapshot.atr_pct,
                snapshot.oi_growth_pct,
                snapshot.funding_rate,
                snapshot.long_short_ratio,
                snapshot.taker_sell_buy_ratio
            );
        }
        if !trade.entry_reasons.is_empty() {
            println!("    entry_reasons={}", trade.entry_reasons.join(","));
        }
    }
}

pub(super) fn format_reason_counts(counts: &[(String, usize)]) -> String {
    counts
        .iter()
        .take(6)
        .map(|(reason, count)| format!("{reason}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}
