use anyhow::{Context, Result};
use rust_quant_analytics::pa_quant_tree::{
    bootstrap_shared_market_mean, calculate_metrics, default_portfolio_risk_policy,
    replay_shared_portfolio, BootstrapConfig, BootstrapMeanEstimate, ExperimentLedger,
    ExperimentLedgerEntry, ExperimentStatus, PaAbcRejectCounts, PaAbcSimulation,
    PerformanceMetrics, PortfolioReplay, PortfolioTradeCandidate, SharedMarketTimeBlock,
    SourceIdentity,
};
use serde::Serialize;
use std::collections::BTreeMap;

const ABC_PROTOCOL_VERSION: &str = "pa-diagnostic-v7-abc-counterfactual";
const DAY_MS: i64 = 86_400_000;
const SIGN_FLIP_RESAMPLES: usize = 20_000;

/// A/B/C 固定门禁使用的全部已计算事实。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PaAbcGateFacts {
    /// B/C 均结算且身份一致的配对数量。
    pub strict_pair_count: usize,
    /// 至少包含一笔严格配对的市场数量。
    pub covered_market_count: usize,
    /// C 严格配对路径胜率。
    pub c_win_rate: f64,
    /// C 严格配对路径 Profit Factor。
    pub c_profit_factor: f64,
    /// C 基础成本后平均 R。
    pub c_mean_net_r: f64,
    /// C 两倍成本后平均 R。
    pub c_double_cost_mean_net_r: f64,
    /// C 的 7 日共享市场块 bootstrap 单侧 95% 下界。
    pub bootstrap_lower_bound_95: f64,
    /// C 正期望假设经本批次 Holm 校正后的单侧 p 值。
    pub holm_adjusted_expectancy_p: f64,
    /// 删除最大五笔盈利后 C 的基础成本平均 R。
    pub top_five_removed_mean_net_r: f64,
    /// 平均 R 为正且至少 30 笔的市场数量。
    pub positive_markets_with_30_trades: usize,
    /// 共享组合最大回撤比例。
    pub portfolio_max_drawdown_ratio: f64,
    /// 同一 setup 上 `C - B` 的基础成本平均增量。
    pub mean_delay_delta_net_r: f64,
    /// 所有 B 路径是否都显式不可交易且仅供诊断。
    pub all_b_paths_diagnostic_only: bool,
}

/// A/B/C 的唯一允许决策；两者都不是生产生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PaAbcDecision {
    /// 任一固定门禁失败，冻结 PA 独立策略研究。
    ArchivePaStandalone,
    /// 全部门禁通过，也只保留为未来新数据验证假设。
    RetainForFutureValidation,
}

/// 单个路径集合的基础成本与两倍成本统计。
#[derive(Debug, Serialize)]
struct PathMetricSummary {
    metrics: PerformanceMetrics,
    double_cost_metrics: PerformanceMetrics,
}

/// 单市场 A/B/C 样本和拒绝原因摘要。
#[derive(Debug, Serialize)]
struct SymbolAbcSummary {
    symbol: String,
    candles: usize,
    setups: usize,
    a: PathMetricSummary,
    b: PathMetricSummary,
    c: PathMetricSummary,
    strict_pairs: usize,
    mean_delay_delta_net_r: f64,
    rejects: PaAbcRejectCounts,
}

/// 一次预注册 A/B/C 诊断的完整只读 JSON 证据。
#[derive(Debug, Serialize)]
pub(crate) struct PaAbcDiagnosticReport {
    protocol_version: String,
    experiment_id: String,
    training_only: bool,
    promotion_eligible: bool,
    source_identity: SourceIdentity,
    dataset_fingerprint: String,
    candle_dataset_fingerprint: String,
    funding_dataset_fingerprint: String,
    common_start_ts: i64,
    common_end_ts: i64,
    b_tradable: bool,
    b_diagnostic_only: bool,
    a: PathMetricSummary,
    b: PathMetricSummary,
    c: PathMetricSummary,
    b_vs_a_selection_mean_delta_r: f64,
    strict_pair_mean_delay_delta_net_r: f64,
    strict_pair_mean_delay_delta_double_cost_net_r: f64,
    symbols: Vec<SymbolAbcSummary>,
    bootstrap_mean: BootstrapMeanEstimate,
    top_five_removed_mean_net_r: f64,
    portfolio: PortfolioReplay,
    experiment_ledger: ExperimentLedger,
    gate_facts: PaAbcGateFacts,
    failed_gates: Vec<String>,
    decision: PaAbcDecision,
}

/// 汇总四市场 A/B/C 结果、执行 Holm 校正并按固定门禁输出唯一结论。
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_abc_diagnostic_report(
    source_identity: SourceIdentity,
    dataset_fingerprint: String,
    candle_dataset_fingerprint: String,
    funding_dataset_fingerprint: String,
    common_start_ts: i64,
    common_end_ts: i64,
    simulations: &[PaAbcSimulation],
) -> Result<PaAbcDiagnosticReport> {
    let a_net = collect_path_values(simulations, |simulation| {
        simulation.a_paths.iter().map(|path| path.net_r)
    });
    let a_double = collect_path_values(simulations, |simulation| {
        simulation.a_paths.iter().map(|path| path.double_cost_net_r)
    });
    let b_net = collect_path_values(simulations, |simulation| {
        simulation.b_paths.iter().map(|path| path.net_r)
    });
    let b_double = collect_path_values(simulations, |simulation| {
        simulation.b_paths.iter().map(|path| path.double_cost_net_r)
    });
    let c_net = collect_path_values(simulations, |simulation| {
        simulation.strict_pairs.iter().map(|pair| pair.c.net_r)
    });
    let c_double = collect_path_values(simulations, |simulation| {
        simulation
            .strict_pairs
            .iter()
            .map(|pair| pair.c.double_cost_net_r)
    });
    let delay = collect_path_values(simulations, |simulation| {
        simulation
            .strict_pairs
            .iter()
            .map(|pair| pair.delay_delta_net_r)
    });
    let delay_double = collect_path_values(simulations, |simulation| {
        simulation
            .strict_pairs
            .iter()
            .map(|pair| pair.delay_delta_double_cost_net_r)
    });
    let a_metrics = calculate_metrics(&a_net);
    let b_metrics = calculate_metrics(&b_net);
    let c_metrics = calculate_metrics(&c_net);
    let c_blocks = c_shared_daily_blocks(simulations);
    let bootstrap_mean = if c_blocks.is_empty() {
        BootstrapMeanEstimate {
            observed_mean_r: 0.0,
            lower_bound_95: 0.0,
            resamples: 0,
        }
    } else {
        bootstrap_shared_market_mean(
            &c_blocks,
            &BootstrapConfig {
                block_size: 7,
                resamples: 10_000,
                seed: 20_260_715,
            },
        )
        .map_err(anyhow::Error::msg)?
    };
    let expectancy_p = block_sign_flip_p_value(&c_blocks);
    let delay_blocks = delay_shared_daily_blocks(simulations);
    let delay_p = block_sign_flip_p_value(&delay_blocks);
    let markets = simulations
        .iter()
        .map(|simulation| simulation.symbol.clone())
        .collect::<Vec<_>>();
    let mut ledger = ExperimentLedger {
        entries: vec![
            ledger_entry(
                "pa-v7-abc-c-expectancy",
                "C strict-pair base-cost mean R is greater than zero",
                expectancy_p,
                &markets,
                &dataset_fingerprint,
                &source_identity,
            ),
            ledger_entry(
                "pa-v7-abc-delay-preservation",
                "C minus B paired delay mean R is greater than or equal to zero",
                delay_p,
                &markets,
                &dataset_fingerprint,
                &source_identity,
            ),
        ],
    };
    ledger
        .validate_and_adjust_holm()
        .map_err(anyhow::Error::msg)?;
    let holm_expectancy_p = ledger.entries[0]
        .holm_adjusted_p
        .context("expectancy hypothesis has no Holm adjustment")?;
    let portfolio = replay_shared_portfolio(
        &c_portfolio_inputs(simulations),
        &default_portfolio_risk_policy(),
    )
    .map_err(anyhow::Error::msg)?;
    let symbols = simulations.iter().map(symbol_summary).collect::<Vec<_>>();
    let top_five_removed_mean_net_r = top_five_removed_mean(&c_net);
    let gate_facts = PaAbcGateFacts {
        strict_pair_count: c_net.len(),
        covered_market_count: simulations
            .iter()
            .filter(|simulation| !simulation.strict_pairs.is_empty())
            .count(),
        c_win_rate: c_metrics.win_rate,
        c_profit_factor: c_metrics.profit_factor,
        c_mean_net_r: c_metrics.mean_net_r,
        c_double_cost_mean_net_r: calculate_metrics(&c_double).mean_net_r,
        bootstrap_lower_bound_95: bootstrap_mean.lower_bound_95,
        holm_adjusted_expectancy_p: holm_expectancy_p,
        top_five_removed_mean_net_r,
        positive_markets_with_30_trades: symbols
            .iter()
            .filter(|symbol| {
                symbol.c.metrics.trade_count >= 30 && symbol.c.metrics.mean_net_r > 0.0
            })
            .count(),
        portfolio_max_drawdown_ratio: portfolio.max_drawdown_ratio,
        mean_delay_delta_net_r: calculate_metrics(&delay).mean_net_r,
        all_b_paths_diagnostic_only: simulations.iter().all(|simulation| {
            simulation
                .b_paths
                .iter()
                .all(|path| !path.tradable && path.diagnostic_only)
        }),
    };
    let (decision, failed_gates) = evaluate_abc_gate(&gate_facts);
    let decision_value = match decision {
        PaAbcDecision::ArchivePaStandalone => "archive_pa_standalone",
        PaAbcDecision::RetainForFutureValidation => "retain_for_future_validation",
    };
    let reason = if failed_gates.is_empty() {
        "all preregistered gates passed; future-data validation only".to_owned()
    } else {
        failed_gates.join("; ")
    };
    for entry in &mut ledger.entries {
        entry.decision = Some(decision_value.to_owned());
        entry.reason = Some(reason.clone());
    }
    ledger
        .validate_and_adjust_holm()
        .map_err(anyhow::Error::msg)?;

    Ok(PaAbcDiagnosticReport {
        protocol_version: ABC_PROTOCOL_VERSION.to_owned(),
        experiment_id: "pa-v7-abc-counterfactual-once".to_owned(),
        training_only: true,
        promotion_eligible: false,
        source_identity,
        dataset_fingerprint,
        candle_dataset_fingerprint,
        funding_dataset_fingerprint,
        common_start_ts,
        common_end_ts,
        b_tradable: false,
        b_diagnostic_only: true,
        a: PathMetricSummary {
            metrics: a_metrics.clone(),
            double_cost_metrics: calculate_metrics(&a_double),
        },
        b: PathMetricSummary {
            metrics: b_metrics.clone(),
            double_cost_metrics: calculate_metrics(&b_double),
        },
        c: PathMetricSummary {
            metrics: c_metrics,
            double_cost_metrics: calculate_metrics(&c_double),
        },
        b_vs_a_selection_mean_delta_r: b_metrics.mean_net_r - a_metrics.mean_net_r,
        strict_pair_mean_delay_delta_net_r: calculate_metrics(&delay).mean_net_r,
        strict_pair_mean_delay_delta_double_cost_net_r: calculate_metrics(&delay_double).mean_net_r,
        symbols,
        bootstrap_mean,
        top_five_removed_mean_net_r,
        portfolio,
        experiment_ledger: ledger,
        gate_facts,
        failed_gates,
        decision,
    })
}

/// 逐项执行预注册硬门禁；任一失败都只能归档独立 PA。
pub(crate) fn evaluate_abc_gate(facts: &PaAbcGateFacts) -> (PaAbcDecision, Vec<String>) {
    let mut failures = Vec::new();
    if facts.strict_pair_count < 100 {
        failures.push("strict_pair_count_below_100".to_owned());
    }
    if facts.covered_market_count < 3 {
        failures.push("covered_markets_below_3".to_owned());
    }
    if facts.c_win_rate <= 0.60 {
        failures.push("c_win_rate_not_above_60pct".to_owned());
    }
    if facts.c_profit_factor <= 1.20 {
        failures.push("c_profit_factor_not_above_1_2".to_owned());
    }
    if facts.c_mean_net_r <= 0.0 {
        failures.push("c_base_cost_mean_not_positive".to_owned());
    }
    if facts.c_double_cost_mean_net_r <= 0.0 {
        failures.push("c_double_cost_mean_not_positive".to_owned());
    }
    if facts.bootstrap_lower_bound_95 <= 0.0 {
        failures.push("bootstrap_lower_bound_not_positive".to_owned());
    }
    if facts.holm_adjusted_expectancy_p > 0.05 {
        failures.push("holm_adjusted_expectancy_p_above_0_05".to_owned());
    }
    if facts.top_five_removed_mean_net_r <= 0.0 {
        failures.push("top_five_removed_mean_not_positive".to_owned());
    }
    if facts.positive_markets_with_30_trades < 3 {
        failures.push("positive_markets_with_30_trades_below_3".to_owned());
    }
    if facts.portfolio_max_drawdown_ratio >= 0.15 {
        failures.push("portfolio_max_drawdown_not_below_15pct".to_owned());
    }
    if facts.mean_delay_delta_net_r < 0.0 {
        failures.push("c_minus_b_delay_delta_negative".to_owned());
    }
    if !facts.all_b_paths_diagnostic_only {
        failures.push("b_path_not_strictly_diagnostic".to_owned());
    }
    let decision = if failures.is_empty() {
        PaAbcDecision::RetainForFutureValidation
    } else {
        PaAbcDecision::ArchivePaStandalone
    };
    (decision, failures)
}

/// 创建预注册假设条目；统计结论和 Holm 值由统一账本方法回写。
fn ledger_entry(
    experiment_id: &str,
    hypothesis: &str,
    raw_p: f64,
    markets: &[String],
    dataset_fingerprint: &str,
    source_identity: &SourceIdentity,
) -> ExperimentLedgerEntry {
    ExperimentLedgerEntry {
        experiment_id: experiment_id.to_owned(),
        parent_experiment_id: Some("pa-v7-abc-counterfactual-once".to_owned()),
        hypothesis: hypothesis.to_owned(),
        protocol_version: ABC_PROTOCOL_VERSION.to_owned(),
        strategy_key: "pa_trend_15m".to_owned(),
        timeframe: "15m".to_owned(),
        markets: markets.to_vec(),
        preregistered: true,
        research_only: true,
        dataset_fingerprint: dataset_fingerprint.to_owned(),
        source_identity: source_identity.clone(),
        status: ExperimentStatus::Completed,
        raw_one_sided_p: Some(raw_p),
        holm_adjusted_p: None,
        decision: None,
        reason: None,
    }
}

/// 统一收集不同路径迭代器，保持市场输入顺序不变。
fn collect_path_values<'a, F, I>(simulations: &'a [PaAbcSimulation], values: F) -> Vec<f64>
where
    F: Fn(&'a PaAbcSimulation) -> I,
    I: Iterator<Item = f64>,
{
    simulations.iter().flat_map(values).collect()
}

/// 将 C 路径按自然日聚合为共享市场块。
fn c_shared_daily_blocks(simulations: &[PaAbcSimulation]) -> Vec<SharedMarketTimeBlock> {
    shared_daily_blocks(simulations.iter().flat_map(|simulation| {
        simulation
            .strict_pairs
            .iter()
            .map(|pair| (pair.c.execution.entry_ts, pair.c.net_r))
    }))
}

/// 将 `C - B` 配对增量按 setup 自然日聚合为共享市场块。
fn delay_shared_daily_blocks(simulations: &[PaAbcSimulation]) -> Vec<SharedMarketTimeBlock> {
    shared_daily_blocks(simulations.iter().flat_map(|simulation| {
        simulation
            .strict_pairs
            .iter()
            .map(|pair| (pair.b.setup_ts, pair.delay_delta_net_r))
    }))
}

/// 按自然日保留同期多市场相关性，供 bootstrap 与符号翻转检验共用。
fn shared_daily_blocks(values: impl Iterator<Item = (i64, f64)>) -> Vec<SharedMarketTimeBlock> {
    let mut grouped = BTreeMap::<i64, Vec<f64>>::new();
    for (timestamp, value) in values {
        let day = timestamp.div_euclid(DAY_MS);
        grouped.entry(day).or_default().push(value);
    }
    grouped
        .into_iter()
        .map(|(day, net_r)| SharedMarketTimeBlock {
            start_ts: day * DAY_MS,
            net_r,
        })
        .collect()
}

/// 对连续七日共享市场块做固定种子符号翻转，计算均值大于零的单侧 p 值。
fn block_sign_flip_p_value(daily_blocks: &[SharedMarketTimeBlock]) -> f64 {
    let weekly = weekly_block_sums(daily_blocks);
    let total_count = weekly.iter().map(|(_, count)| *count).sum::<usize>();
    let observed_sum = weekly.iter().map(|(sum, _)| *sum).sum::<f64>();
    if weekly.is_empty() || total_count == 0 || observed_sum <= 0.0 {
        return 1.0;
    }
    let mut state = 20_260_715_u64;
    let mut at_least_observed = 0usize;
    for _ in 0..SIGN_FLIP_RESAMPLES {
        let randomized_sum = weekly.iter().fold(0.0, |sum, (block_sum, _)| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            if state & 1 == 0 {
                sum + block_sum
            } else {
                sum - block_sum
            }
        });
        if randomized_sum >= observed_sum {
            at_least_observed += 1;
        }
    }
    (at_least_observed + 1) as f64 / (SIGN_FLIP_RESAMPLES + 1) as f64
}

/// 把自然日块按首日对齐为连续七日块，保留每块总和与样本权重。
fn weekly_block_sums(daily_blocks: &[SharedMarketTimeBlock]) -> Vec<(f64, usize)> {
    let Some(first_day) = daily_blocks.first().map(|block| block.start_ts / DAY_MS) else {
        return Vec::new();
    };
    let mut weekly = BTreeMap::<i64, (f64, usize)>::new();
    for block in daily_blocks {
        let day = block.start_ts / DAY_MS;
        let bucket = (day - first_day).div_euclid(7);
        let entry = weekly.entry(bucket).or_default();
        entry.0 += block.net_r.iter().sum::<f64>();
        entry.1 += block.net_r.len();
    }
    weekly.into_values().collect()
}

/// 把严格 C 路径转换为共享组合回放输入。
fn c_portfolio_inputs(simulations: &[PaAbcSimulation]) -> Vec<PortfolioTradeCandidate> {
    simulations
        .iter()
        .flat_map(|simulation| {
            simulation
                .strict_pairs
                .iter()
                .map(|pair| PortfolioTradeCandidate {
                    candidate_id: pair.pair_id.clone(),
                    symbol: pair.c.symbol.clone(),
                    entry_ts: pair.c.execution.entry_ts,
                    exit_ts: pair.c.exit_ts,
                    entry_price: pair.c.execution.entry_price,
                    stop_price: pair.c.execution.stop_price,
                    net_r: pair.c.net_r,
                })
        })
        .collect()
}

/// 生成单市场 A/B/C 指标和拒绝原因摘要。
fn symbol_summary(simulation: &PaAbcSimulation) -> SymbolAbcSummary {
    let a_net = simulation
        .a_paths
        .iter()
        .map(|path| path.net_r)
        .collect::<Vec<_>>();
    let a_double = simulation
        .a_paths
        .iter()
        .map(|path| path.double_cost_net_r)
        .collect::<Vec<_>>();
    let b_net = simulation
        .b_paths
        .iter()
        .map(|path| path.net_r)
        .collect::<Vec<_>>();
    let b_double = simulation
        .b_paths
        .iter()
        .map(|path| path.double_cost_net_r)
        .collect::<Vec<_>>();
    let c_net = simulation
        .strict_pairs
        .iter()
        .map(|pair| pair.c.net_r)
        .collect::<Vec<_>>();
    let c_double = simulation
        .strict_pairs
        .iter()
        .map(|pair| pair.c.double_cost_net_r)
        .collect::<Vec<_>>();
    let delay = simulation
        .strict_pairs
        .iter()
        .map(|pair| pair.delay_delta_net_r)
        .collect::<Vec<_>>();
    SymbolAbcSummary {
        symbol: simulation.symbol.clone(),
        candles: simulation.candle_count,
        setups: simulation.setup_count,
        a: PathMetricSummary {
            metrics: calculate_metrics(&a_net),
            double_cost_metrics: calculate_metrics(&a_double),
        },
        b: PathMetricSummary {
            metrics: calculate_metrics(&b_net),
            double_cost_metrics: calculate_metrics(&b_double),
        },
        c: PathMetricSummary {
            metrics: calculate_metrics(&c_net),
            double_cost_metrics: calculate_metrics(&c_double),
        },
        strict_pairs: simulation.strict_pairs.len(),
        mean_delay_delta_net_r: calculate_metrics(&delay).mean_net_r,
        rejects: simulation.rejects.clone(),
    }
}

/// 删除最大五笔盈利后计算平均 R；剩余为空时返回零并由门禁拒绝。
fn top_five_removed_mean(net_r: &[f64]) -> f64 {
    let mut values = net_r.to_vec();
    values.sort_by(|left, right| right.total_cmp(left));
    let remaining = values.get(5..).unwrap_or(&[]);
    calculate_metrics(remaining).mean_net_r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_facts() -> PaAbcGateFacts {
        PaAbcGateFacts {
            strict_pair_count: 120,
            covered_market_count: 4,
            c_win_rate: 0.61,
            c_profit_factor: 1.21,
            c_mean_net_r: 0.01,
            c_double_cost_mean_net_r: 0.001,
            bootstrap_lower_bound_95: 0.001,
            holm_adjusted_expectancy_p: 0.05,
            top_five_removed_mean_net_r: 0.001,
            positive_markets_with_30_trades: 3,
            portfolio_max_drawdown_ratio: 0.149,
            mean_delay_delta_net_r: 0.0,
            all_b_paths_diagnostic_only: true,
        }
    }

    #[test]
    fn every_hard_gate_archives_when_it_fails() {
        let (decision, failures) = evaluate_abc_gate(&passing_facts());
        assert_eq!(decision, PaAbcDecision::RetainForFutureValidation);
        assert!(failures.is_empty());

        let mut cases = Vec::new();
        let mut facts = passing_facts();
        facts.strict_pair_count = 99;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.covered_market_count = 2;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.c_win_rate = 0.60;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.c_profit_factor = 1.20;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.c_mean_net_r = 0.0;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.c_double_cost_mean_net_r = 0.0;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.bootstrap_lower_bound_95 = 0.0;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.holm_adjusted_expectancy_p = 0.051;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.top_five_removed_mean_net_r = 0.0;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.positive_markets_with_30_trades = 2;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.portfolio_max_drawdown_ratio = 0.15;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.mean_delay_delta_net_r = -f64::EPSILON;
        cases.push(facts);
        let mut facts = passing_facts();
        facts.all_b_paths_diagnostic_only = false;
        cases.push(facts);

        for failed in cases {
            let (decision, failures) = evaluate_abc_gate(&failed);
            assert_eq!(decision, PaAbcDecision::ArchivePaStandalone);
            assert!(!failures.is_empty());
        }
    }

    #[test]
    fn empty_strict_pair_report_serializes_as_archive_decision() {
        let report = build_abc_diagnostic_report(
            SourceIdentity {
                git_head: "0123456789abcdef".to_owned(),
                source_fingerprint: "sha256:source".to_owned(),
                dirty: true,
            },
            "sha256:dataset".to_owned(),
            "sha256:candles".to_owned(),
            "sha256:funding".to_owned(),
            1,
            2,
            &[PaAbcSimulation {
                symbol: "BTC-USDT-SWAP".to_owned(),
                candle_count: 1_000,
                setup_count: 0,
                a_paths: vec![],
                b_paths: vec![],
                confirmed_setup_ids: vec![],
                strict_pairs: vec![],
                rejects: PaAbcRejectCounts::default(),
            }],
        )
        .unwrap();

        assert_eq!(report.decision, PaAbcDecision::ArchivePaStandalone);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("archive_pa_standalone"));
        assert!(json.contains("pa-diagnostic-v7-abc-counterfactual"));
    }
}
