use anyhow::{Context, Result};
use rust_quant_analytics::pa_quant_tree::{
    bootstrap_shared_market_mean, calculate_metrics, default_portfolio_risk_policy,
    replay_shared_portfolio, BootstrapConfig, BootstrapMeanEstimate, ChallengerTrainingResult,
    HistoricalPaSimulation, ModelFamily, PerformanceMetrics, PortfolioReplay,
    SharedMarketTimeBlock,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// 入选模型家族在 walk-forward 验证折上的折外路径统计。
#[derive(Debug, Serialize)]
pub(super) struct SelectedModelOofDiagnostic {
    /// one-standard-error 选择的模型家族。
    family: ModelFamily,
    /// true 表示每个保留决策来自未见该验证候选的折内训练模型。
    out_of_fold: bool,
    /// 始终为 false；当前没有额外外层窗口验证模型家族选择。
    outer_validated: bool,
    /// 始终为 false；当前结果尚未应用 Holm 或其他家族选择惩罚。
    family_selection_adjusted: bool,
    /// 始终为 false；训练期 OOF 诊断不能触发任何晋级。
    promotion_eligible: bool,
    /// 全部 walk-forward 验证候选数，包含模型拒绝项。
    validation_candidates: usize,
    /// 入选家族在验证折保留的候选数。
    kept_trades: usize,
    /// OOF 保留路径的基础成本指标。
    metrics: PerformanceMetrics,
    /// 同一 OOF 保留路径的两倍成本指标。
    double_cost_metrics: PerformanceMetrics,
    /// OOF 保留路径经过共享风险预算后的组合结果。
    portfolio: PortfolioReplay,
    /// 只对 OOF 保留路径执行的共享市场块 bootstrap。
    bootstrap_mean: BootstrapMeanEstimate,
    /// OOF 保留路径的分市场诊断。
    symbols: Vec<SelectedModelOofSymbolDiagnostic>,
}

/// 单市场的入选模型 OOF 路径统计。
#[derive(Debug, Serialize)]
struct SelectedModelOofSymbolDiagnostic {
    /// Core 统一交易对标识。
    symbol: String,
    /// 该市场在全部验证折中被保留的候选数。
    kept_trades: usize,
    /// 该市场 OOF 保留路径的基础成本指标。
    metrics: PerformanceMetrics,
    /// 该市场同一路径的两倍成本指标。
    double_cost_metrics: PerformanceMetrics,
}

/// 将入选家族的折外决策回连原始结算路径，生成不含训练重拟合结果的统计。
pub(super) fn selected_model_oof_diagnostic(
    simulations: &[HistoricalPaSimulation],
    tournament: &ChallengerTrainingResult,
) -> Result<SelectedModelOofDiagnostic> {
    let selected = tournament
        .entries
        .get(tournament.selected_index)
        .context("selected model index is invalid")?;
    anyhow::ensure!(
        selected.oof_decisions.len() == selected.validation_candidate_count,
        "selected model OOF decisions do not cover every validation candidate"
    );
    let kept = selected
        .oof_decisions
        .iter()
        .filter(|decision| decision.keep)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        kept.len() == selected.kept_trade_count,
        "selected model OOF kept count does not match tournament metrics"
    );

    let mut trade_by_candidate = BTreeMap::new();
    for simulation in simulations {
        for trade in &simulation.trades {
            anyhow::ensure!(
                trade_by_candidate
                    .insert(trade.observation.candidate_id.clone(), trade)
                    .is_none(),
                "duplicate settled candidate id: {}",
                trade.observation.candidate_id
            );
        }
    }

    let mut seen_oof = BTreeSet::new();
    let mut base = Vec::with_capacity(kept.len());
    let mut double = Vec::with_capacity(kept.len());
    let mut portfolio_inputs = Vec::with_capacity(kept.len());
    let mut by_symbol = BTreeMap::<String, (Vec<f64>, Vec<f64>)>::new();
    let mut days = BTreeMap::<i64, Vec<f64>>::new();
    const DAY_MS: i64 = 86_400_000;
    for decision in kept {
        anyhow::ensure!(
            seen_oof.insert(decision.candidate_id.clone()),
            "duplicate selected OOF candidate id: {}",
            decision.candidate_id
        );
        let trade = trade_by_candidate
            .get(&decision.candidate_id)
            .with_context(|| format!("missing settled path for {}", decision.candidate_id))?;
        anyhow::ensure!(
            trade.observation.symbol == decision.symbol
                && trade.observation.net_r.to_bits() == decision.net_r.to_bits(),
            "OOF decision does not match settled candidate: {}",
            decision.candidate_id
        );
        base.push(decision.net_r);
        double.push(trade.double_cost_net_r);
        portfolio_inputs.push(trade.portfolio_trade.clone());
        let symbol_values = by_symbol.entry(decision.symbol.clone()).or_default();
        symbol_values.0.push(decision.net_r);
        symbol_values.1.push(trade.double_cost_net_r);
        let day_start = decision.signal_ts.div_euclid(DAY_MS) * DAY_MS;
        days.entry(day_start).or_default().push(decision.net_r);
    }
    anyhow::ensure!(!base.is_empty(), "selected model has no kept OOF path");

    let blocks = days
        .into_iter()
        .map(|(start_ts, net_r)| SharedMarketTimeBlock { start_ts, net_r })
        .collect::<Vec<_>>();
    let bootstrap_mean = bootstrap_shared_market_mean(
        &blocks,
        &BootstrapConfig {
            block_size: 7,
            resamples: 1_000,
            seed: 20_260_715,
        },
    )
    .map_err(anyhow::Error::msg)?;
    let portfolio = replay_shared_portfolio(&portfolio_inputs, &default_portfolio_risk_policy())
        .map_err(anyhow::Error::msg)?;
    let symbols = simulations
        .iter()
        .map(|simulation| {
            let (symbol_base, symbol_double) = by_symbol
                .remove(&simulation.symbol)
                .unwrap_or_else(|| (Vec::new(), Vec::new()));
            SelectedModelOofSymbolDiagnostic {
                symbol: simulation.symbol.clone(),
                kept_trades: symbol_base.len(),
                metrics: calculate_metrics(&symbol_base),
                double_cost_metrics: calculate_metrics(&symbol_double),
            }
        })
        .collect();

    Ok(SelectedModelOofDiagnostic {
        family: selected.family,
        out_of_fold: true,
        outer_validated: false,
        family_selection_adjusted: false,
        promotion_eligible: false,
        validation_candidates: selected.validation_candidate_count,
        kept_trades: base.len(),
        metrics: calculate_metrics(&base),
        double_cost_metrics: calculate_metrics(&double),
        portfolio,
        bootstrap_mean,
        symbols,
    })
}
