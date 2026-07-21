use super::{
    simulate_portfolio_with_event_context, Args, CandidateTrade, ComponentPipelineSnapshot,
    EventContext, PortfolioReport,
};
use anyhow::{anyhow, Result};
use chrono::{Datelike, Months, TimeZone, Utc};
use serde::Serialize;

/// 单个时间切片的固定策略组合表现；参数不会在切片内重新优化。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct TemporalPeriodReport {
    /// `in_sample`、`out_of_sample` 或 `walk_forward_test_N`。
    label: String,
    /// 按入场时点划分的起点，UTC 毫秒时间戳。
    start_ts: i64,
    /// 按入场时点划分的终点，UTC 毫秒时间戳，不包含边界。
    end_ts_exclusive: i64,
    /// 进入容量回放的候选交易数。
    candidate_trades: usize,
    /// 被共享账户容量规则接纳的交易数。
    accepted_trades: usize,
    /// 成本后组合收益率。
    total_return_pct: f64,
    /// 成本后 Profit Factor。
    profit_factor: Option<f64>,
    /// 以冻结初始止损为分母的成本后期望。
    net_expectancy_r: Option<f64>,
    /// 保守同棒不利极值最大回撤。
    intrabar_conservative_max_drawdown_pct: f64,
    /// 净利润除以保守最大绝对回撤。
    recovery_factor: Option<f64>,
    /// UTC 连续日收益按 sqrt(365) 年化的 Sharpe。
    daily_sharpe_sqrt_365: Option<f64>,
}

/// 严格时间样本外与固定参数 walk-forward 报告。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct TemporalValidationReport {
    /// 用户冻结的样本外起点。
    oos_start_ts: Option<i64>,
    /// 样本外起点之前的对照结果。
    in_sample: Option<TemporalPeriodReport>,
    /// 样本外起点之后的独立结果。
    out_of_sample: Option<TemporalPeriodReport>,
    /// 固定参数滚动测试窗口；训练窗只用于形成时间隔离，不重新搜索参数。
    walk_forward_tests: Vec<TemporalPeriodReport>,
    /// 防止把固定参数滚动验证误报为窗口内调参。
    walk_forward_mode: &'static str,
}

/// 用入场时点构建时间隔离报告，平仓路径只用于该次已入场交易的结果评估。
pub(super) fn build_temporal_validation_report(
    candidates: &[CandidateTrade],
    args: Args,
    event_context: Option<&EventContext>,
) -> Result<TemporalValidationReport> {
    let first_ts = candidates.iter().map(|trade| trade.open_ts).min();
    let last_ts = candidates.iter().map(|trade| trade.open_ts).max();
    let (in_sample, out_of_sample) = match (first_ts, last_ts, args.oos_start_ts) {
        (Some(first), Some(last), Some(split)) => (
            period_report("in_sample", candidates, first, split, args, event_context)?,
            period_report(
                "out_of_sample",
                candidates,
                split,
                last.saturating_add(1),
                args,
                event_context,
            )?,
        ),
        _ => (None, None),
    };
    let mut walk_forward_tests = Vec::new();
    if let (Some(first), Some(last), Some(train_months), Some(test_months)) = (
        first_ts,
        last_ts,
        args.walk_forward_train_months,
        args.walk_forward_test_months,
    ) {
        for (index, (test_start, test_end)) in
            walk_forward_test_bounds(first, last, train_months, test_months)?
                .into_iter()
                .enumerate()
        {
            if let Some(report) = period_report(
                &format!("walk_forward_test_{}", index + 1),
                candidates,
                test_start,
                test_end,
                args,
                event_context,
            )? {
                walk_forward_tests.push(report);
            }
        }
    }
    Ok(TemporalValidationReport {
        oos_start_ts: args.oos_start_ts,
        in_sample,
        out_of_sample,
        walk_forward_tests,
        walk_forward_mode: "fixed_parameters_rolling_oos_no_refit",
    })
}

fn period_report(
    label: &str,
    candidates: &[CandidateTrade],
    start_ts: i64,
    end_ts_exclusive: i64,
    args: Args,
    event_context: Option<&EventContext>,
) -> Result<Option<TemporalPeriodReport>> {
    let trades: Vec<CandidateTrade> = candidates
        .iter()
        .filter(|trade| trade.open_ts >= start_ts && trade.open_ts < end_ts_exclusive)
        .cloned()
        .collect();
    if trades.is_empty() {
        return Ok(None);
    }
    let component_pipeline = ComponentPipelineSnapshot::without_quality_gate(&trades);
    let report =
        simulate_portfolio_with_event_context(trades, args, event_context, component_pipeline)?;
    Ok(Some(slim_period_report(
        label,
        start_ts,
        end_ts_exclusive,
        report,
    )))
}

fn slim_period_report(
    label: &str,
    start_ts: i64,
    end_ts_exclusive: i64,
    report: PortfolioReport,
) -> TemporalPeriodReport {
    TemporalPeriodReport {
        label: label.to_string(),
        start_ts,
        end_ts_exclusive,
        candidate_trades: report.candidate_trades,
        accepted_trades: report.accepted_trades,
        total_return_pct: report.total_return_pct,
        profit_factor: report.profit_factor,
        net_expectancy_r: report.net_expectancy_r,
        intrabar_conservative_max_drawdown_pct: report.intrabar_conservative_max_drawdown_pct,
        recovery_factor: report.recovery_factor,
        daily_sharpe_sqrt_365: report.daily_sharpe_sqrt_365,
    }
}

fn walk_forward_test_bounds(
    first_ts: i64,
    last_ts: i64,
    train_months: u32,
    test_months: u32,
) -> Result<Vec<(i64, i64)>> {
    let first = Utc
        .timestamp_millis_opt(first_ts)
        .single()
        .ok_or_else(|| anyhow!("invalid first trade timestamp"))?;
    let last = Utc
        .timestamp_millis_opt(last_ts)
        .single()
        .ok_or_else(|| anyhow!("invalid last trade timestamp"))?;
    let anchor = Utc
        .with_ymd_and_hms(first.year(), first.month(), 1, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow!("invalid walk-forward month anchor"))?;
    let mut test_start = anchor
        .checked_add_months(Months::new(train_months))
        .ok_or_else(|| anyhow!("walk-forward train window overflow"))?;
    let mut bounds = Vec::new();
    while test_start.timestamp_millis() <= last.timestamp_millis() {
        let test_end = test_start
            .checked_add_months(Months::new(test_months))
            .ok_or_else(|| anyhow!("walk-forward test window overflow"))?;
        bounds.push((test_start.timestamp_millis(), test_end.timestamp_millis()));
        test_start = test_end;
    }
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_forward_windows_are_non_overlapping_and_follow_training_period() {
        let first = Utc
            .with_ymd_and_hms(2024, 1, 19, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let last = Utc
            .with_ymd_and_hms(2025, 7, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let bounds = walk_forward_test_bounds(first, last, 12, 3).unwrap();
        assert_eq!(
            bounds[0].0,
            Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(bounds[0].1, bounds[1].0);
    }
}
