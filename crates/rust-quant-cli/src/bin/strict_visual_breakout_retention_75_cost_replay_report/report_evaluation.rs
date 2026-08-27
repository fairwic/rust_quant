//! 冻结成本回放的身份校验、分组指标和预注册门禁。

use super::*;

/// 按谓词复制冻结交易子集后复用统一指标口径。
fn subset_summary(
    trades: &[TradeObservation],
    predicate: impl Fn(&TradeObservation) -> bool,
) -> MetricSummary {
    summarize(
        &trades
            .iter()
            .filter(|trade| predicate(trade))
            .cloned()
            .collect::<Vec<_>>(),
    )
}

/// 构造版本分析，并与回放已发布的长短家族汇总做数量与净 R 对账。
fn analyze_replay(input: &ReplayInput) -> ReplayAnalysis {
    let trades = closed_trades(input);
    let strict_candidates = input
        .candidate_ledger
        .candidates
        .iter()
        .filter(|candidate| is_strict_visual(candidate))
        .collect::<Vec<_>>();
    let mut non_closed = BTreeMap::<String, usize>::new();
    for candidate in &strict_candidates {
        if candidate.outcome.status != "closed_trade" {
            *non_closed
                .entry(candidate.outcome.status.clone())
                .or_default() += 1;
        }
    }
    let published_trades = input.strict_visual_breakout_family.cost_adjusted.trades
        + input
            .strict_visual_breakout_short_family
            .cost_adjusted
            .trades;
    let published_net_r = input.strict_visual_breakout_family.cost_adjusted.net_r
        + input
            .strict_visual_breakout_short_family
            .cost_adjusted
            .net_r;
    let ledger_net_r: f64 = trades.iter().map(|trade| trade.net_r).sum();
    let mut losses = trades
        .iter()
        .filter(|trade| trade.net_r < 0.0)
        .cloned()
        .collect::<Vec<_>>();
    losses.sort_by(|left, right| {
        right
            .identity
            .signal_time_ms
            .cmp(&left.identity.signal_time_ms)
            .then_with(|| left.identity.symbol.cmp(&right.identity.symbol))
    });
    let concurrency = max_concurrency(&trades);
    ReplayAnalysis {
        strategy_version: input.strategy_version.clone(),
        variant: input.strict_visual_breakout_research_variant.clone(),
        execution: ExecutionAudit {
            strict_candidate_intents: strict_candidates.len(),
            closed_trades: trades.len(),
            non_closed_status_counts: non_closed,
            max_concurrent_positions_same_timestamp_entries_first: concurrency,
            max_concurrent_initial_r: concurrency,
            published_family_trade_count: published_trades,
            published_family_net_r: published_net_r,
            ledger_matches_published_family_metrics: published_trades == trades.len()
                && (published_net_r - ledger_net_r).abs() <= 1e-8,
        },
        metrics: MetricBreakdown {
            overall: summarize(&trades),
            discovery: subset_summary(&trades, |trade| {
                trade.identity.signal_time_ms < VALIDATION_START_MS
            }),
            validation: subset_summary(&trades, |trade| {
                trade.identity.signal_time_ms >= VALIDATION_START_MS
            }),
            long: subset_summary(&trades, |trade| trade.identity.direction == "long"),
            short: subset_summary(&trades, |trade| trade.identity.direction == "short"),
        },
        concentration: concentration_audit(&trades),
        latest_ten_losses: losses.iter().take(10).map(trade_summary).collect(),
    }
}

/// 返回严格视觉实际候选的信号身份集合。
fn actual_intent_identities(input: &ReplayInput) -> BTreeSet<SignalIdentity> {
    input
        .candidate_ledger
        .candidates
        .iter()
        .filter(|candidate| is_strict_visual(candidate))
        .map(|candidate| SignalIdentity {
            symbol: candidate.symbol.clone(),
            direction: candidate.direction.clone(),
            signal_time_ms: candidate.signal_time_ms,
        })
        .collect()
}

/// 验证当前候选实际意图全部来自 V23 原确认时点的 retention>=0.75 集合。
fn raw_subset_audit(
    baseline: &ReplayInput,
    candidate: &ReplayInput,
    quality: &QualityOracleInput,
) -> RawSubsetAudit {
    let eligible = baseline
        .per_symbol
        .iter()
        .map(|symbol| symbol.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let rows = quality
        .candidate_ledger
        .iter()
        .filter(|row| eligible.contains(row.symbol.as_str()))
        .collect::<Vec<_>>();
    let selected = rows
        .iter()
        .copied()
        .filter(|row| row.acceptance_retention_ratio + RETENTION_EPSILON >= RETENTION_THRESHOLD)
        .collect::<Vec<_>>();
    let selected_identities = selected
        .iter()
        .map(|row| SignalIdentity {
            symbol: row.symbol.clone(),
            direction: row.direction.clone(),
            signal_time_ms: row.signal_time_ms,
        })
        .collect::<BTreeSet<_>>();
    let candidate_actual = actual_intent_identities(candidate);
    let baseline_actual = actual_intent_identities(baseline);
    let distinct_raw = rows
        .iter()
        .map(|row| {
            (
                row.symbol.as_str(),
                row.direction.as_str(),
                row.breakout_time_ms,
                row.signal_time_ms,
            )
        })
        .collect::<BTreeSet<_>>();
    RawSubsetAudit {
        eligible_symbols: eligible.len(),
        baseline_v23_raw_confirmations: rows.len(),
        retention_75_raw_confirmations: selected.len(),
        retention_75_share_percent: ratio_percent(selected.len() as f64, rows.len() as f64),
        retention_75_long_confirmations: selected
            .iter()
            .filter(|row| row.direction == "long")
            .count(),
        retention_75_short_confirmations: selected
            .iter()
            .filter(|row| row.direction == "short")
            .count(),
        retention_75_discovery_confirmations: selected
            .iter()
            .filter(|row| row.split == "discovery")
            .count(),
        retention_75_validation_confirmations: selected
            .iter()
            .filter(|row| row.split == "validation")
            .count(),
        retention_75_symbols: selected
            .iter()
            .map(|row| row.symbol.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        retention_75_direction_hour_event_clusters: selected
            .iter()
            .map(|row| row.event_cluster.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        retention_75_minimum_observed: selected
            .iter()
            .map(|row| row.acceptance_retention_ratio)
            .min_by(f64::total_cmp),
        raw_identity_duplicates: rows.len().saturating_sub(distinct_raw.len()),
        candidate_actual_intents_absent_from_raw_retention_75_oracle: candidate_actual
            .difference(&selected_identities)
            .count(),
        candidate_actual_intents_not_present_in_baseline_actual_intents: candidate_actual
            .difference(&baseline_actual)
            .count(),
        interpretation: "raw subset is validated against the original V23 confirmation-time oracle; actual entry intents need not be a subset of baseline executions because rejecting an earlier trade can free the single-symbol position for a later oracle-qualified signal",
    }
}

/// 读取有限 PF；无亏损样本时不把无限值用于数值差分。
fn finite_pf(metrics: &MetricSummary) -> Option<f64> {
    metrics.profit_factor_r
}

/// 将候选相对 V23 的方向变化集中到一个稳定结构。
fn replay_delta(baseline: &ReplayAnalysis, candidate: &ReplayAnalysis) -> ReplayDelta {
    ReplayDelta {
        closed_trades: candidate.metrics.overall.trades as i64
            - baseline.metrics.overall.trades as i64,
        net_r: candidate.metrics.overall.net_r - baseline.metrics.overall.net_r,
        average_net_r: candidate.metrics.overall.average_net_r
            - baseline.metrics.overall.average_net_r,
        profit_factor_r: finite_pf(&candidate.metrics.overall)
            .zip(finite_pf(&baseline.metrics.overall))
            .map(|(candidate, baseline)| candidate - baseline),
        max_drawdown_r: candidate
            .metrics
            .overall
            .chronological_closed_equity_max_drawdown_r
            - baseline
                .metrics
                .overall
                .chronological_closed_equity_max_drawdown_r,
    }
}

/// 判断一个分组是否同时具备正净 EV 与 PF>1。
fn has_positive_cost_edge(metrics: &MetricSummary) -> bool {
    metrics.average_net_r > 0.0 && metrics.profit_factor_r.is_some_and(|value| value > 1.0)
}

/// 按预注册合同生成停止门禁；不会新增或调整阈值。
fn gate_checks(
    raw: &RawSubsetAudit,
    baseline: &ReplayAnalysis,
    candidate: &ReplayAnalysis,
) -> Vec<GateCheck> {
    let candidate_pf = finite_pf(&candidate.metrics.overall).unwrap_or(0.0);
    let baseline_pf = finite_pf(&baseline.metrics.overall).unwrap_or(0.0);
    let top5 = &candidate.concentration.after_removing_top_five_winners;
    let best_symbol = &candidate.concentration.after_removing_best_symbol;
    vec![
        GateCheck {
            name: "coverage_and_raw_strict_subset",
            passed: raw.retention_75_raw_confirmations >= 1_000
                && raw.retention_75_symbols >= 30
                && raw.retention_75_direction_hour_event_clusters >= 100
                && raw.retention_75_long_confirmations >= 200
                && raw.retention_75_short_confirmations >= 200
                && raw.candidate_actual_intents_absent_from_raw_retention_75_oracle == 0,
            observed: format!(
                "raw={} symbols={} clusters={} long={} short={} actual_absent={}",
                raw.retention_75_raw_confirmations,
                raw.retention_75_symbols,
                raw.retention_75_direction_hour_event_clusters,
                raw.retention_75_long_confirmations,
                raw.retention_75_short_confirmations,
                raw.candidate_actual_intents_absent_from_raw_retention_75_oracle,
            ),
            requirement: ">=1000 raw, >=30 symbols, >=100 clusters, >=200 each side, zero candidate intents outside the raw >=0.75 oracle",
        },
        GateCheck {
            name: "cost_ev_and_pf_improve_vs_v23",
            passed: candidate.metrics.overall.average_net_r
                > baseline.metrics.overall.average_net_r
                && candidate_pf > baseline_pf,
            observed: format!(
                "EV {:.6}->{:.6}; PF {:.6}->{:.6}",
                baseline.metrics.overall.average_net_r,
                candidate.metrics.overall.average_net_r,
                baseline_pf,
                candidate_pf,
            ),
            requirement: "candidate cost-adjusted average net R and PF both exceed V23",
        },
        GateCheck {
            name: "discovery_and_validation_positive",
            passed: has_positive_cost_edge(&candidate.metrics.discovery)
                && has_positive_cost_edge(&candidate.metrics.validation),
            observed: format!(
                "discovery EV={:.6} PF={:.6}; validation EV={:.6} PF={:.6}",
                candidate.metrics.discovery.average_net_r,
                finite_pf(&candidate.metrics.discovery).unwrap_or(0.0),
                candidate.metrics.validation.average_net_r,
                finite_pf(&candidate.metrics.validation).unwrap_or(0.0),
            ),
            requirement: "cost-adjusted EV>0 and PF>1 in both frozen windows",
        },
        GateCheck {
            name: "long_and_short_positive",
            passed: has_positive_cost_edge(&candidate.metrics.long)
                && has_positive_cost_edge(&candidate.metrics.short),
            observed: format!(
                "long EV={:.6} PF={:.6}; short EV={:.6} PF={:.6}",
                candidate.metrics.long.average_net_r,
                finite_pf(&candidate.metrics.long).unwrap_or(0.0),
                candidate.metrics.short.average_net_r,
                finite_pf(&candidate.metrics.short).unwrap_or(0.0),
            ),
            requirement: "cost-adjusted EV>0 and PF>1 for both directions",
        },
        GateCheck {
            name: "max_drawdown_not_worse",
            passed: candidate
                .metrics
                .overall
                .chronological_closed_equity_max_drawdown_r
                <= baseline
                    .metrics
                    .overall
                    .chronological_closed_equity_max_drawdown_r,
            observed: format!(
                "{:.6}R -> {:.6}R",
                baseline
                    .metrics
                    .overall
                    .chronological_closed_equity_max_drawdown_r,
                candidate
                    .metrics
                    .overall
                    .chronological_closed_equity_max_drawdown_r,
            ),
            requirement: "candidate chronological closed-equity max drawdown R <= V23",
        },
        GateCheck {
            name: "remove_top_five_winners_still_positive",
            passed: has_positive_cost_edge(top5) && top5.net_r > 0.0,
            observed: format!(
                "net={:.6}R EV={:.6} PF={:.6}",
                top5.net_r,
                top5.average_net_r,
                finite_pf(top5).unwrap_or(0.0),
            ),
            requirement: "remaining net R>0 and PF>1",
        },
        GateCheck {
            name: "remove_best_symbol_still_positive",
            passed: best_symbol.net_r > 0.0,
            observed: format!(
                "removed={:?}; remaining net={:.6}R",
                candidate.concentration.best_symbol_removed, best_symbol.net_r,
            ),
            requirement: "remaining net R>0",
        },
        GateCheck {
            name: "largest_positive_event_cluster_share",
            passed: candidate
                .concentration
                .largest_positive_event_cluster_share_percent
                <= 10.0,
            observed: format!(
                "{:.6}%",
                candidate
                    .concentration
                    .largest_positive_event_cluster_share_percent,
            ),
            requirement: "<=10% of total positive event-cluster R",
        },
    ]
}

/// 检查两次回放和 oracle 数据身份、成员、成本完全相同。
fn validate_inputs(
    baseline: &ReplayInput,
    candidate: &ReplayInput,
    quality: &QualityOracleInput,
) -> Result<()> {
    if baseline.universe_version != candidate.universe_version
        || baseline.universe_manifest_sha256 != candidate.universe_manifest_sha256
        || baseline.dataset_fingerprint_sha256 != candidate.dataset_fingerprint_sha256
        || baseline.evaluation_start_ms != candidate.evaluation_start_ms
        || baseline.evaluation_end_ms != candidate.evaluation_end_ms
        || baseline.execution_cost_contract != candidate.execution_cost_contract
    {
        bail!("baseline 与 candidate 的数据或成本身份不一致");
    }
    if baseline.execution_cost_contract
        != (ExecutionCostContract {
            entry_fee_bps: 2.0,
            entry_slippage_bps: 1.0,
            exit_fee_bps: 5.0,
            exit_slippage_bps: 1.0,
        })
    {
        bail!("执行成本不是冻结的 maker2/taker5/双边1bps滑点合同");
    }
    let baseline_symbols = baseline
        .per_symbol
        .iter()
        .map(|symbol| symbol.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let candidate_symbols = candidate
        .per_symbol
        .iter()
        .map(|symbol| symbol.symbol.as_str())
        .collect::<BTreeSet<_>>();
    if baseline_symbols != candidate_symbols {
        bail!("baseline 与 candidate 的实际成员不一致");
    }
    if quality.dataset.universe_version != baseline.universe_version
        || quality.dataset.manifest_sha256 != baseline.universe_manifest_sha256
        || quality.dataset.evaluation_start_ms != baseline.evaluation_start_ms
        || quality.dataset.evaluation_end_ms != baseline.evaluation_end_ms + 1
        || quality.dataset.validation_start_ms != VALIDATION_START_MS
    {
        bail!("retention oracle 与成本回放的数据窗口身份不一致");
    }
    Ok(())
}

/// 组装唯一机器报告；调用方负责写文件。
pub(super) fn build_report(
    baseline_input: &ReplayInput,
    candidate_input: &ReplayInput,
    quality_input: &QualityOracleInput,
    baseline_sha256: String,
    candidate_sha256: String,
    quality_sha256: String,
    preregistration_sha256: String,
) -> Result<MachineReport> {
    validate_inputs(baseline_input, candidate_input, quality_input)?;
    let raw = raw_subset_audit(baseline_input, candidate_input, quality_input);
    let baseline = analyze_replay(baseline_input);
    let candidate = analyze_replay(candidate_input);
    let delta = replay_delta(&baseline, &candidate);
    let checks = gate_checks(&raw, &baseline, &candidate);
    let all_passed = checks.iter().all(|check| check.passed);
    let mut source_sha256 = BTreeMap::new();
    source_sha256.insert("baseline_replay_json", baseline_sha256);
    source_sha256.insert("candidate_replay_json", candidate_sha256);
    source_sha256.insert("quality_oracle_json", quality_sha256);

    Ok(MachineReport {
        schema_version: "strict_visual_v23_acceptance_retention_75_cost_l2_v1",
        generated_at_ms: Utc::now().timestamp_millis(),
        level: "L2_multi_symbol_cost_diagnostic",
        research_status: if all_passed {
            "all_preregistered_l2_gates_passed"
        } else {
            "stopped_l2_cost_edge_negative"
        },
        promotion_status: "research_only_not_promoted",
        single_variable: "first_v23_legal_confirmation_acceptance_retention_ratio",
        frozen_threshold: RETENTION_THRESHOLD,
        forbidden_threshold_search: "no_0_70_0_80_or_other_thresholds_were_evaluated",
        preregistration_sha256,
        source_sha256,
        dataset: DatasetAudit {
            universe_version: baseline_input.universe_version.clone(),
            universe_manifest_sha256: baseline_input.universe_manifest_sha256.clone(),
            dataset_fingerprint_sha256: baseline_input.dataset_fingerprint_sha256.clone(),
            evaluation_start_ms: baseline_input.evaluation_start_ms,
            evaluation_end_ms: baseline_input.evaluation_end_ms,
            manifest_evaluation_end_ms: baseline_input.manifest_evaluation_end_ms,
            evaluation_window_was_clipped: baseline_input.evaluation_window_was_clipped,
            expected_symbols: baseline_input.expected_symbols,
            included_symbols: baseline_input.included_symbols,
            excluded_symbols: baseline_input.excluded_symbols.clone(),
            full_universe_complete: baseline_input.full_universe_complete,
        },
        execution_cost_contract: baseline_input.execution_cost_contract.clone(),
        raw_subset_audit: raw,
        baseline,
        candidate,
        candidate_minus_baseline: delta,
        gate_checks: checks,
        verdict: if all_passed {
            "eligible_for_l3_preparation_but_not_promoted"
        } else {
            "reject_retention_75_as_a_tradeable_cost_filter_and_stop_without_threshold_chasing"
        },
        limitations: vec![
            "only 43 of 60 frozen members satisfy the full evaluation and 60-day warmup contract in the current local Top60 replay, so this is not a formal full-universe conclusion",
            "each symbol is replayed independently; max concurrency is reported, but no capacity ranking, correlation cap, leverage cap, or unified portfolio selection is applied",
            "drawdown uses exit-time aggregated cost-adjusted net R with fixed 1R initial risk and a non-compounding 100R reference equity",
            "the quality oracle is the earlier V23 confirmation ledger on the same manifest; unit tests separately prove that a rejected first confirmation consumes the source and cannot be resurrected",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造只改变成本后 R 与持仓时间的最小统计成交。
    fn trade(index: usize, net_r: f64, entry_time_ms: i64, exit_time_ms: i64) -> TradeObservation {
        TradeObservation {
            ledger_index: index,
            identity: SignalIdentity {
                symbol: format!("S{index}"),
                direction: if index % 2 == 0 { "long" } else { "short" }.to_owned(),
                signal_time_ms: entry_time_ms - 1,
            },
            event_cluster_id: format!("cluster-{index}"),
            entry_time_ms,
            entry_price: 100.0,
            exit_time_ms,
            exit_price: 100.0,
            exit_reason: "test".to_owned(),
            frozen_stop: 99.0,
            frozen_target: 102.0,
            actual_initial_stop: Some(99.0),
            actual_target: Some(102.0),
            exit_policy: "Fixed".to_owned(),
            signal_atr: Some(1.0),
            range_height: Some(2.0),
            range_length_bars: Some(8),
            net_r,
        }
    }

    #[test]
    fn metric_summary_uses_net_r_profit_factor_and_exit_time_drawdown() {
        let trades = vec![
            trade(0, 1.0, 0, 10),
            trade(1, -0.5, 11, 20),
            trade(2, -0.75, 21, 30),
        ];
        let metrics = summarize(&trades);

        assert_eq!(metrics.trades, 3);
        assert!((metrics.net_r + 0.25).abs() < 1e-12);
        assert!((metrics.average_net_r + 1.0 / 12.0).abs() < 1e-12);
        assert!((metrics.profit_factor_r.expect("finite PF") - 0.8).abs() < 1e-12);
        assert!((metrics.chronological_closed_equity_max_drawdown_r - 1.25).abs() < 1e-12);
    }

    #[test]
    fn concurrency_counts_same_timestamp_entries_before_exits_conservatively() {
        let trades = vec![trade(0, 1.0, 0, 10), trade(1, 1.0, 10, 20)];
        assert_eq!(max_concurrency(&trades), 2);
    }
}
