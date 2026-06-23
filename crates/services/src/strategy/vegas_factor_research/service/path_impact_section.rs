impl VegasFactorResearchService {
    pub fn render_path_impact_report(summaries: &[PathImpactSummary]) -> String {
        render_path_impact_report(summaries)
    }
    /// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
    pub fn summarize_path_impact(
        baseline_id: i64,
        experiment_id: i64,
        baseline: &[ResearchTradeSample],
        experiment: &[ResearchTradeSample],
        top_changed_limit: usize,
    ) -> PathImpactSummary {
        let baseline_map = Self::trade_map(baseline);
        let experiment_map = Self::trade_map(experiment);
        let mut missing_pnls = Vec::new();
        let mut new_pnls = Vec::new();
        let mut common_deltas = Vec::new();
        let mut changes = Vec::new();
        for (key, baseline_trade) in &baseline_map {
            if let Some(experiment_trade) = experiment_map.get(key) {
                let pnl_delta = experiment_trade.pnl - baseline_trade.pnl;
                common_deltas.push(pnl_delta);
                changes.push(PathImpactTradeChange {
                    change_type: "common_changed".to_string(),
                    inst_id: baseline_trade.inst_id.clone(),
                    side: baseline_trade.side.clone(),
                    open_time_ms: baseline_trade.open_time_ms,
                    baseline_pnl: Some(baseline_trade.pnl),
                    experiment_pnl: Some(experiment_trade.pnl),
                    pnl_delta,
                    close_type: experiment_trade.close_type.clone(),
                });
            } else {
                missing_pnls.push(baseline_trade.pnl);
                changes.push(PathImpactTradeChange {
                    change_type: "missing_from_experiment".to_string(),
                    inst_id: baseline_trade.inst_id.clone(),
                    side: baseline_trade.side.clone(),
                    open_time_ms: baseline_trade.open_time_ms,
                    baseline_pnl: Some(baseline_trade.pnl),
                    experiment_pnl: None,
                    pnl_delta: -baseline_trade.pnl,
                    close_type: baseline_trade.close_type.clone(),
                });
            }
        }
        for (key, experiment_trade) in &experiment_map {
            if !baseline_map.contains_key(key) {
                new_pnls.push(experiment_trade.pnl);
                changes.push(PathImpactTradeChange {
                    change_type: "new_in_experiment".to_string(),
                    inst_id: experiment_trade.inst_id.clone(),
                    side: experiment_trade.side.clone(),
                    open_time_ms: experiment_trade.open_time_ms,
                    baseline_pnl: None,
                    experiment_pnl: Some(experiment_trade.pnl),
                    pnl_delta: experiment_trade.pnl,
                    close_type: experiment_trade.close_type.clone(),
                });
            }
        }
        changes.sort_by(|left, right| {
            right
                .pnl_delta
                .abs()
                .total_cmp(&left.pnl_delta.abs())
                .then(left.open_time_ms.cmp(&right.open_time_ms))
        });
        changes.truncate(top_changed_limit);
        let missing_pnl = missing_pnls.iter().sum::<f64>();
        let new_pnl = new_pnls.iter().sum::<f64>();
        let common_pnl_delta = common_deltas.iter().sum::<f64>();
        let total_path_delta = new_pnl - missing_pnl + common_pnl_delta;
        let verdict = if total_path_delta > 1e-6 {
            "path_improved"
        } else if total_path_delta < -1e-6 {
            "path_degraded"
        } else {
            "neutral"
        };
        PathImpactSummary {
            baseline_id,
            experiment_id,
            inst_id: Self::unique_inst_id(baseline, experiment),
            missing_count: missing_pnls.len(),
            missing_pnl,
            missing_wins: missing_pnls.iter().filter(|pnl| **pnl > 0.0).count(),
            missing_avg_pnl: Self::avg(&missing_pnls),
            new_count: new_pnls.len(),
            new_pnl,
            new_wins: new_pnls.iter().filter(|pnl| **pnl > 0.0).count(),
            new_avg_pnl: Self::avg(&new_pnls),
            common_count: common_deltas.len(),
            common_pnl_delta,
            common_improved_count: common_deltas.iter().filter(|delta| **delta > 0.0).count(),
            total_path_delta,
            verdict: verdict.to_string(),
            top_changes: changes,
        }
    }
    /// 提供交易map的集中实现，避免回测策略调用方重复处理相同细节。
    fn trade_map(
        trades: &[ResearchTradeSample],
    ) -> HashMap<(String, String, i64), &ResearchTradeSample> {
        trades
            .iter()
            .map(|trade| {
                (
                    (
                        trade.inst_id.clone(),
                        trade.side.to_ascii_lowercase(),
                        trade.open_time_ms,
                    ),
                    trade,
                )
            })
            .collect()
    }
    /// 提供uniqueinstID的集中实现，避免回测策略调用方重复处理相同细节。
    fn unique_inst_id(
        baseline: &[ResearchTradeSample],
        experiment: &[ResearchTradeSample],
    ) -> Option<String> {
        let ids: Vec<_> = baseline
            .iter()
            .chain(experiment.iter())
            .map(|row| row.inst_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if ids.len() == 1 {
            Some(ids[0].to_string())
        } else {
            None
        }
    }
    /// 封装平均，减少回测策略调用方重复实现相同细节。
    fn avg(values: &[f64]) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }
}
