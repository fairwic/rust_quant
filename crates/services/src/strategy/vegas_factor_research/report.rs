use super::types::{
    FactorBucketReport, PathImpactSummary, ResearchFilteredSignalSample, ResearchSampleKind,
    ResearchTradeSample, VolatilityTier,
};

pub fn render_report(
    trades: &[ResearchTradeSample],
    filtered_signals: &[ResearchFilteredSignalSample],
    buckets: &[FactorBucketReport],
) -> String {
    let mut lines = vec![
        "# Vegas 外部因子研究报告".to_string(),
        String::new(),
        "## 因子概览表".to_string(),
        format!("- 交易样本数: {}", trades.len()),
        format!("- 过滤候选样本数: {}", filtered_signals.len()),
        format!(
            "- 覆盖因子: {}",
            buckets
                .iter()
                .map(|row| row.factor_name.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "- 波动性分层: BTC / ETH / 其他币种".to_string(),
        "- 结论说明: 可实验仅代表研究候选，回注 Vegas 前必须通过路径影响评估。".to_string(),
        String::new(),
        "## 下一轮未覆盖低 Sharpe 开仓环境候选".to_string(),
        "| 因子 | 桶 | 分层 | 样本数 | 胜率 | AvgPnL | TotalPnL | SharpeProxy |".to_string(),
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |".to_string(),
    ];

    let candidates = low_sharpe_open_candidates(buckets);
    for row in candidates
        .iter()
        .filter(|row| rejected_coverage(row).is_none())
        .filter(|row| is_actionable_impact(row))
        .take(10)
    {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2}% | {:.2} | {:.2} | {:.2} |",
            row.factor_name,
            row.bucket_name,
            row.scope_label,
            row.sample_count,
            row.win_rate * 100.0,
            row.avg_pnl,
            total_pnl(row),
            row.sharpe_proxy,
        ));
    }

    lines.extend([
        String::new(),
        "## 低影响观察候选".to_string(),
        "| 因子 | 桶 | 分层 | 样本数 | 胜率 | AvgPnL | TotalPnL | SharpeProxy |".to_string(),
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |".to_string(),
    ]);
    for row in candidates
        .iter()
        .filter(|row| rejected_coverage(row).is_none())
        .filter(|row| !is_actionable_impact(row))
        .take(10)
    {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2}% | {:.2} | {:.2} | {:.2} |",
            row.factor_name,
            row.bucket_name,
            row.scope_label,
            row.sample_count,
            row.win_rate * 100.0,
            row.avg_pnl,
            total_pnl(row),
            row.sharpe_proxy,
        ));
    }

    lines.extend([
        String::new(),
        "## 已覆盖拒绝候选".to_string(),
        "| 因子 | 桶 | 分层 | 样本数 | 胜率 | AvgPnL | TotalPnL | SharpeProxy | 覆盖原因 |"
            .to_string(),
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |".to_string(),
    ]);
    for row in candidates
        .iter()
        .filter_map(|row| rejected_coverage(row).map(|reason| (*row, reason)))
        .take(10)
    {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2}% | {:.2} | {:.2} | {:.2} | {} |",
            row.0.factor_name,
            row.0.bucket_name,
            row.0.scope_label,
            row.0.sample_count,
            row.0.win_rate * 100.0,
            row.0.avg_pnl,
            total_pnl(row.0),
            row.0.sharpe_proxy,
            row.1,
        ));
    }

    lines.extend([
        String::new(),
        "## 出场/止损环境候选".to_string(),
        "| 因子 | 桶 | 分层 | 样本数 | 胜率 | AvgPnL | TotalPnL | SharpeProxy |".to_string(),
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |".to_string(),
    ]);
    for row in exit_environment_candidates(buckets).iter().take(10) {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2}% | {:.2} | {:.2} | {:.2} |",
            row.factor_name,
            row.bucket_name,
            row.scope_label,
            row.sample_count,
            row.win_rate * 100.0,
            row.avg_pnl,
            total_pnl(row),
            row.sharpe_proxy,
        ));
    }

    lines.extend([
        String::new(),
        "## 分桶统计表".to_string(),
        "| 因子 | 样本类型 | 桶 | 分层 | 样本数 | 胜率 | AvgPnL | SharpeProxy | 结论 |".to_string(),
        "| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- |".to_string(),
    ]);

    for row in buckets {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {:.2}% | {:.2} | {:.2} | {} |",
            row.factor_name,
            row.sample_kind.label(),
            row.bucket_name,
            row.scope_label,
            row.sample_count,
            row.win_rate * 100.0,
            row.avg_pnl,
            row.sharpe_proxy,
            row.conclusion.label()
        ));
    }

    lines.join("\n")
}

fn low_sharpe_open_candidates(buckets: &[FactorBucketReport]) -> Vec<&FactorBucketReport> {
    let mut candidates: Vec<_> = buckets
        .iter()
        .filter(|row| row.sample_kind == ResearchSampleKind::Traded)
        .filter(|row| row.factor_name != "exit_environment_context")
        .filter(|row| row.sample_count >= min_low_sharpe_samples(row.volatility_tier))
        .filter(|row| row.avg_pnl < 0.0 || row.sharpe_proxy < 0.0)
        .collect();
    candidates.sort_by(|left, right| {
        tier_rank(left.volatility_tier)
            .cmp(&tier_rank(right.volatility_tier))
            .then(total_pnl(left).total_cmp(&total_pnl(right)))
            .then(left.sharpe_proxy.total_cmp(&right.sharpe_proxy))
            .then(left.avg_pnl.total_cmp(&right.avg_pnl))
            .then(right.sample_count.cmp(&left.sample_count))
    });
    candidates
}

fn exit_environment_candidates(buckets: &[FactorBucketReport]) -> Vec<&FactorBucketReport> {
    let mut candidates: Vec<_> = buckets
        .iter()
        .filter(|row| row.factor_name == "exit_environment_context")
        .filter(|row| row.sample_kind == ResearchSampleKind::Traded)
        .filter(|row| row.sample_count >= min_low_sharpe_samples(row.volatility_tier))
        .filter(|row| is_actionable_impact(row))
        .collect();
    candidates.sort_by(|left, right| {
        tier_rank(left.volatility_tier)
            .cmp(&tier_rank(right.volatility_tier))
            .then(total_pnl(left).total_cmp(&total_pnl(right)))
            .then(left.sharpe_proxy.total_cmp(&right.sharpe_proxy))
            .then(left.avg_pnl.total_cmp(&right.avg_pnl))
            .then(right.sample_count.cmp(&left.sample_count))
    });
    candidates
}

fn total_pnl(row: &FactorBucketReport) -> f64 {
    row.avg_pnl * row.sample_count as f64
}

fn is_actionable_impact(row: &FactorBucketReport) -> bool {
    total_pnl(row) <= -10.0
}

fn rejected_coverage(row: &FactorBucketReport) -> Option<&'static str> {
    if row.volatility_tier == VolatilityTier::Eth
        && row.bucket_name.contains("funding_negative_short")
        && (row.factor_name == "funding_direction_context"
            || row.factor_name == "funding_trend_context"
            || row.factor_name == "funding_macd_context")
    {
        Some("covered_by_1450")
    } else {
        None
    }
}

fn min_low_sharpe_samples(tier: VolatilityTier) -> usize {
    match tier {
        VolatilityTier::Eth => 3,
        VolatilityTier::Btc => 5,
        VolatilityTier::Alt => 6,
    }
}

fn tier_rank(tier: VolatilityTier) -> u8 {
    match tier {
        VolatilityTier::Eth => 0,
        VolatilityTier::Btc => 1,
        VolatilityTier::Alt => 2,
    }
}

pub fn render_path_impact_report(summaries: &[PathImpactSummary]) -> String {
    let mut lines = vec![
        "# Vegas 路径影响评估报告".to_string(),
        String::new(),
        "## 路径影响评估表".to_string(),
        "| 基线ID | 实验ID | 标的 | 缺失数 | 缺失PnL | 新增数 | 新增PnL | 共同Delta | 总路径Delta | 结论 |"
            .to_string(),
        "| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |".to_string(),
    ];

    for row in summaries {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2} | {} | {:.2} | {:.2} | {:.2} | {} |",
            row.baseline_id,
            row.experiment_id,
            row.inst_id.as_deref().unwrap_or("ALL"),
            row.missing_count,
            row.missing_pnl,
            row.new_count,
            row.new_pnl,
            row.common_pnl_delta,
            row.total_path_delta,
            row.verdict
        ));
    }

    lines.push(String::new());
    lines.push("## Top Changed Trades".to_string());
    lines.push(
        "| 实验ID | 类型 | 标的 | 方向 | OpenTimeMs | BaselinePnL | ExperimentPnL | Delta | CloseType |"
            .to_string(),
    );
    lines.push("| ---: | --- | --- | --- | ---: | ---: | ---: | ---: | --- |".to_string());

    for summary in summaries {
        for change in &summary.top_changes {
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {:.2} | {} |",
                summary.experiment_id,
                change.change_type,
                change.inst_id,
                change.side,
                change.open_time_ms,
                fmt_optional_f64(change.baseline_pnl),
                fmt_optional_f64(change.experiment_pnl),
                change.pnl_delta,
                change.close_type.as_deref().unwrap_or("-")
            ));
        }
    }

    lines.join("\n")
}

fn fmt_optional_f64(value: Option<f64>) -> String {
    value
        .map(|row| format!("{row:.2}"))
        .unwrap_or_else(|| "-".to_string())
}
