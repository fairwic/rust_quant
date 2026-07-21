use super::*;

/// 汇总冻结成本、方向、月份、集中度和 Discovery 早停判定。
pub(super) fn build_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    stages: ResidualMomentumStageCounts,
    trades: Vec<ResidualMomentumTrade>,
    rule_version: &str,
) -> ResidualMomentumReport {
    let long_trades = trades
        .iter()
        .filter(|trade| trade.direction == "long")
        .cloned()
        .collect::<Vec<_>>();
    let short_trades = trades
        .iter()
        .filter(|trade| trade.direction == "short")
        .cloned()
        .collect::<Vec<_>>();
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = trades
                .iter()
                .filter(|trade| trade.entry_ts >= window.from_ms && trade.entry_ts < window.to_ms)
                .cloned()
                .collect::<Vec<_>>();
            (window.from_ms, metrics(&values, CostMode::Standard))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, value)| value.net_sum_r > 0.0)
        .count();
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let gross_zero_cost = metrics(&trades, CostMode::Zero);
    let overall = metrics(&trades, CostMode::Standard);
    let double_cost = metrics(&trades, CostMode::Double);
    let effective_events = effective_event_count(&trades);
    let long = metrics(&long_trades, CostMode::Standard);
    let short = metrics(&short_trades, CostMode::Standard);
    let direction_total = long.trades + short.trades;
    let dominant_direction = long.trades.max(short.trades);
    let minority_direction = long.trades.min(short.trades);
    let discovery_gate_passed = gross_zero_cost
        .net_expectancy_r
        .is_some_and(|value| value > 0.0)
        && gross_zero_cost
            .profit_factor
            .is_some_and(|value| value > 1.0)
        && overall.net_expectancy_r.is_some_and(|value| value > 0.0)
        && overall.profit_factor.is_some_and(|value| value > 1.0)
        && trades.len() >= 300
        && effective_events >= 180
        && positive_months >= 8
        && net_r_without_top_three_symbols > 0.0
        && (direction_total == 0
            || dominant_direction * 100 <= direction_total * 80
            || minority_direction >= 60);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_owned())
            .or_default() += 1;
    }
    ResidualMomentumReport {
        rule_version: rule_version.to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        stages,
        effective_events,
        gross_zero_cost,
        overall,
        double_cost,
        long,
        short,
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        discovery_gate_passed,
        trades,
    }
}

/// 选择零、标准或双倍成本口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CostMode {
    /// 不含任何成本的反事实。
    Zero,
    /// 冻结标准成本。
    Standard,
    /// 冻结双倍成本压力。
    Double,
}

/// 按冻结成本口径计算交易级 R 指标。
fn metrics(trades: &[ResidualMomentumTrade], cost_mode: CostMode) -> ResidualMomentumMetrics {
    if trades.is_empty() {
        return ResidualMomentumMetrics::default();
    }
    let values = trades
        .iter()
        .map(|trade| match cost_mode {
            CostMode::Zero => trade.gross_r,
            CostMode::Standard => trade.net_r,
            CostMode::Double => trade.double_cost_net_r,
        })
        .collect::<Vec<_>>();
    let net_sum_r = values.iter().sum::<f64>();
    let profit = values.iter().filter(|value| **value > 0.0).sum::<f64>();
    let loss = values
        .iter()
        .filter(|value| **value < 0.0)
        .map(|value| value.abs())
        .sum::<f64>();
    let mean = net_sum_r / values.len() as f64;
    let variance = if values.len() > 1 {
        values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64
    } else {
        0.0
    };
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in &values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    ResidualMomentumMetrics {
        trades: values.len(),
        net_sum_r,
        net_expectancy_r: Some(mean),
        profit_factor: (loss > 0.0).then_some(profit / loss),
        win_rate_pct: Some(
            values.iter().filter(|value| **value > 0.0).count() as f64 / values.len() as f64
                * 100.0,
        ),
        trade_sharpe: (variance > 0.0)
            .then_some(mean / variance.sqrt() * (values.len() as f64).sqrt()),
        max_drawdown_r: max_drawdown,
        recovery_factor: (max_drawdown > 0.0).then_some(net_sum_r / max_drawdown),
    }
}

/// 把 30 分钟内的同时触发归并为一个有效市场事件。
fn effective_event_count(trades: &[ResidualMomentumTrade]) -> usize {
    let mut count = 0usize;
    let mut latest = None::<i64>;
    for trade in trades {
        if latest.is_none_or(|point| trade.entry_ts - point > MS_30M) {
            count += 1;
        }
        latest = Some(trade.entry_ts);
    }
    count
}

/// 计算移除标准成本净贡献最高三个盈利币种后的净 R。
fn concentration_without_top_three(trades: &[ResidualMomentumTrade]) -> (Vec<String>, f64) {
    let mut by_symbol = BTreeMap::<String, f64>::new();
    for trade in trades {
        *by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
    }
    let mut positive = by_symbol
        .into_iter()
        .filter(|(_, value)| *value > 0.0)
        .collect::<Vec<_>>();
    positive.sort_by(|left, right| right.1.total_cmp(&left.1));
    let top = positive
        .iter()
        .take(3)
        .map(|(symbol, _)| symbol.clone())
        .collect::<Vec<_>>();
    let removed = positive
        .iter()
        .take(3)
        .map(|(_, value)| *value)
        .sum::<f64>();
    (
        top,
        trades.iter().map(|trade| trade.net_r).sum::<f64>() - removed,
    )
}

/// 输出可机器读取的漏斗、稳定性与成本报告。
pub(super) fn print_report(report: &ResidualMomentumReport) {
    println!(
        "residual_momentum_research\trule={}\tuniverse={}\tsymbols={}\tdecision_points={}\tcoverage_blocked={}\tfactor_observations={}\tpersistence_pass={}\treversion_pass={}\tscore_pass={}\tselected={}\trisk_blocked={}\tcapacity_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}\tdiscovery_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.stages.decision_points,
        report.stages.coverage_blocked,
        report.stages.factor_observations,
        report.stages.persistence_pass,
        report.stages.reversion_pass,
        report.stages.score_pass,
        report.stages.selected_candidates,
        report.stages.risk_blocked,
        report.stages.capacity_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
        report.discovery_gate_passed,
    );
    for (label, value) in [
        ("gross_zero_cost", &report.gross_zero_cost),
        ("overall", &report.overall),
        ("double_cost", &report.double_cost),
        ("long", &report.long),
        ("short", &report.short),
    ] {
        print_metrics(label, value);
    }
    for (from_ms, value) in &report.monthly {
        print_metrics(&format!("month_{from_ms}"), value);
    }
    println!(
        "residual_momentum_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
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

/// 输出单个评估切片的交易级 R 指标。
fn print_metrics(label: &str, value: &ResidualMomentumMetrics) {
    println!(
        "residual_momentum_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
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
