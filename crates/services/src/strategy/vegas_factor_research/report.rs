use super::types::{FactorBucketReport, ResearchFilteredSignalSample, ResearchTradeSample};

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
        String::new(),
        "## 分桶统计表".to_string(),
        "| 因子 | 样本类型 | 桶 | 分层 | 样本数 | 胜率 | AvgPnL | SharpeProxy | 结论 |".to_string(),
        "| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- |".to_string(),
    ];

    for row in buckets {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {:.2}% | {:.2} | {:.2} | {} |",
            row.factor_name,
            row.sample_kind.label(),
            row.bucket_name,
            row.volatility_tier.label(),
            row.sample_count,
            row.win_rate * 100.0,
            row.avg_pnl,
            row.sharpe_proxy,
            row.conclusion.label()
        ));
    }

    lines.join("\n")
}
