use super::{FlowFlipMetrics, FlowFlipResearchReport};

/// 输出可机器读取的漏斗、窗口指标和集中度报告。
pub(super) fn print_report(report: &FlowFlipResearchReport) {
    println!(
        "flow_flip_research\trule={}\tuniverse={}\tsymbols={}\tmapped_symbols={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tmetric_rows={}\tprice_coverage_blocked={}\tprice_tail={}\trecent_low={}\tbreakout={}\tmetrics_pass={}\tacceptance_pass={}\tacceptance_invalidated={}\tacceptance_expired={}\tfailure_pass={}\tfailure_expired={}\trisk_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.metrics_audit.mapped_symbols,
        report.metrics_audit.mapping_blocked_symbols,
        report.metrics_audit.requested_files,
        report.metrics_audit.available_files,
        report.metrics_audit.missing_files,
        report.metrics_audit.invalid_files,
        report.metrics_audit.rows,
        report.price_coverage_blocked,
        report.stages.price_tail_pass,
        report.stages.recent_low_pass,
        report.stages.breakout_pass,
        report.stages.metrics_pass,
        report.stages.acceptance_pass,
        report.stages.acceptance_invalidated,
        report.stages.acceptance_expired,
        report.stages.failure_pass,
        report.stages.failure_expired,
        report.stages.risk_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
    );
    for (label, value) in [
        ("gross_zero_cost", &report.gross_zero_cost),
        ("overall", &report.overall),
        ("discovery", &report.discovery),
        ("validation", &report.validation),
        ("double_cost", &report.double_cost),
    ] {
        print_metrics(label, value);
    }
    for (from_ms, value) in &report.monthly {
        print_metrics(&format!("month_{from_ms}"), value);
    }
    println!(
        "flow_flip_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
        report.top_three_positive_symbols.join(","),
        report.net_r_without_top_three_symbols,
        report
            .exit_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    );
}

/// 输出单个评估窗口的交易级 R 指标。
fn print_metrics(label: &str, value: &FlowFlipMetrics) {
    println!(
        "flow_flip_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
        label,
        value.trades,
        value.net_sum_r,
        optional(value.net_expectancy_r),
        optional(value.profit_factor),
        optional(value.win_rate_pct),
        optional(value.trade_sharpe),
        value.max_drawdown_r,
        optional(value.recovery_factor),
    );
}

/// 将缺失浮点指标稳定格式化为 `NA`。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}
