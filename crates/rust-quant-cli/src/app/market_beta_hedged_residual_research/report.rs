use super::*;

/// 汇总双腿成本、方向、月份、集中度和预注册早停判定。
pub(super) fn build_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    stages: BetaHedgedResidualStageCounts,
    trades: Vec<BetaHedgedResidualTrade>,
) -> BetaHedgedResidualReport {
    let long_trades = trades
        .iter()
        .filter(|trade| trade.direction == "long_residual")
        .cloned()
        .collect::<Vec<_>>();
    let short_trades = trades
        .iter()
        .filter(|trade| trade.direction == "short_residual")
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
    let long_residual = metrics(&long_trades, CostMode::Standard);
    let short_residual = metrics(&short_trades, CostMode::Standard);
    let direction_total = long_residual.trades + short_residual.trades;
    let dominant_direction = long_residual.trades.max(short_residual.trades);
    let minority_direction = long_residual.trades.min(short_residual.trades);
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
    let average_beta = average(trades.iter().map(|trade| trade.beta));
    let average_gross_notional = average(trades.iter().map(|trade| 1.0 + trade.beta));
    BetaHedgedResidualReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        stages,
        effective_events,
        gross_zero_cost,
        overall,
        double_cost,
        long_residual,
        short_residual,
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        average_beta,
        average_gross_notional,
        discovery_gate_passed,
        trades,
    }
}

/// 选择零、标准或双倍成本口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CostMode {
    /// 不含任何成本的反事实。
    Zero,
    /// 冻结四次成交和不利资金成本。
    Standard,
    /// 冻结成本全部翻倍的压力情景。
    Double,
}

/// 按指定成本口径计算交易级 R 指标。
fn metrics(trades: &[BetaHedgedResidualTrade], cost_mode: CostMode) -> BetaHedgedResidualMetrics {
    if trades.is_empty() {
        return BetaHedgedResidualMetrics::default();
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
    BetaHedgedResidualMetrics {
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
fn effective_event_count(trades: &[BetaHedgedResidualTrade]) -> usize {
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

/// 移除标准成本净贡献最高三个盈利币种并返回剩余净 R。
fn concentration_without_top_three(trades: &[BetaHedgedResidualTrade]) -> (Vec<String>, f64) {
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

/// 计算有限样本的平均值；空样本返回空。
fn average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// 输出可机器读取的漏斗、稳定性和四次成交成本证据。
pub(super) fn print_report(report: &BetaHedgedResidualReport) {
    println!(
        "beta_hedged_residual_research\trule={}\tuniverse={}\tsymbols={}\tdecision_points={}\tcoverage_blocked={}\tfactor_observations={}\treversion_pass={}\tscore_pass={}\tbeta_pass={}\tselected={}\trisk_blocked={}\tcapacity_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}\taverage_beta={}\taverage_gross_notional={}\tdiscovery_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.stages.decision_points,
        report.stages.coverage_blocked,
        report.stages.factor_observations,
        report.stages.reversion_pass,
        report.stages.score_pass,
        report.stages.beta_pass,
        report.stages.selected_candidates,
        report.stages.risk_blocked,
        report.stages.capacity_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
        optional(report.average_beta),
        optional(report.average_gross_notional),
        report.discovery_gate_passed,
    );
    for (label, value) in [
        ("gross_zero_cost", &report.gross_zero_cost),
        ("overall", &report.overall),
        ("double_cost", &report.double_cost),
        ("long_residual", &report.long_residual),
        ("short_residual", &report.short_residual),
    ] {
        print_metrics(label, value);
    }
    for (from_ms, value) in &report.monthly {
        print_metrics(&format!("month_{from_ms}"), value);
    }
    println!(
        "beta_hedged_residual_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
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
fn print_metrics(label: &str, value: &BetaHedgedResidualMetrics) {
    println!(
        "beta_hedged_residual_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
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
