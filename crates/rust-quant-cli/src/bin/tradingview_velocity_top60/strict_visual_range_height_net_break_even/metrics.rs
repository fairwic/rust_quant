use super::{
    nearly_equal, CounterfactualExitKind, CounterfactualRecord, EVENT_CLUSTER_MS, FLOAT_TOLERANCE,
};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::Direction;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const STRESS_COST_BPS_PER_SIDE: [f64; 4] = [0.0, 8.0, 10.0, 12.0];
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;

/// 一个数值字段的最小分位摘要，避免只报告均值掩盖长尾。
#[derive(Debug, Default, Serialize)]
pub(super) struct DistributionSummary {
    min: f64,
    p25: f64,
    p50: f64,
    p75: f64,
    max: f64,
}

/// 固定成本压力下的同身份基线与退出变体表现。
#[derive(Debug, Serialize)]
pub(super) struct CostComparison {
    /// 单边手续费与滑点合计，单位 bps。
    pub(super) cost_bps_per_side: f64,
    pub(super) baseline: RMetrics,
    pub(super) variant: RMetrics,
    /// 变体净 R 减去基线净 R。
    variant_minus_baseline_net_r: f64,
}

/// 固定初始风险口径下的交易级 R 指标。
#[derive(Debug, Default, Clone, Serialize)]
pub(super) struct RMetrics {
    trades: usize,
    wins: usize,
    losses: usize,
    flat: usize,
    /// 成本后胜率，单位百分比。
    win_rate_percent: f64,
    pub(super) net_r: f64,
    pub(super) average_net_r: f64,
    gross_profit_r: f64,
    gross_loss_r: f64,
    /// 没有亏损时为 `None`，并由下一字段标记无穷。
    pub(super) profit_factor_r: Option<f64>,
    pub(super) profit_factor_r_is_infinite: bool,
    /// 按各自真实退出时间排序的闭仓 R 最大回撤。
    chronological_closed_equity_max_drawdown_r: f64,
}

/// 单币或单月在 8bps 单边成本下的贡献。
#[derive(Debug, Serialize)]
pub(super) struct ContributionComparison {
    /// 币种名或 `YYYY-MM` 上海月份。
    key: String,
    trades: usize,
    activations: usize,
    changed_exits: usize,
    baseline_net_r: f64,
    variant_net_r: f64,
    pub(super) variant_minus_baseline_net_r: f64,
}

/// 60 分钟同方向链式事件，防止把同步市场波动当作独立样本。
#[derive(Debug, Default, Serialize)]
pub(super) struct EventComparison {
    raw_trades: usize,
    events: usize,
    pub(super) activated_events: usize,
    pub(super) improved_events: usize,
    worsened_events: usize,
    unchanged_events: usize,
    largest_event_trade_count: usize,
    largest_event_symbol_count: usize,
    baseline_net_r: f64,
    variant_net_r: f64,
    variant_minus_baseline_net_r: f64,
}

/// L2 机械闸门只决定是否值得进入 L3，不触发版本晋级。
#[derive(Debug, Serialize)]
pub(super) struct MetricGate {
    net_r_improved_at_8bps: bool,
    average_r_improved_at_8bps: bool,
    profit_factor_improved_at_8bps: bool,
    variant_net_r_positive_at_8bps: bool,
    variant_profit_factor_above_one_at_8bps: bool,
    activated_trades_at_least_30: bool,
    activated_events_at_least_20: bool,
    improved_trades_at_least_3: bool,
    improved_symbols_at_least_3: bool,
    improved_months_at_least_3: bool,
    improved_events_at_least_3: bool,
    trade_identity_preserved: bool,
    metric_gate_passed: bool,
}

pub(super) fn cost_comparisons(records: &[CounterfactualRecord]) -> Vec<CostComparison> {
    STRESS_COST_BPS_PER_SIDE
        .iter()
        .map(|&cost_bps_per_side| {
            let baseline = r_metrics(records, cost_bps_per_side, false);
            let variant = r_metrics(records, cost_bps_per_side, true);
            CostComparison {
                cost_bps_per_side,
                variant_minus_baseline_net_r: variant.net_r - baseline.net_r,
                baseline,
                variant,
            }
        })
        .collect()
}

fn r_metrics(records: &[CounterfactualRecord], cost_bps_per_side: f64, variant: bool) -> RMetrics {
    let mut outcomes = records
        .iter()
        .map(|record| {
            (
                if variant {
                    record.variant_exit_time_ms
                } else {
                    record.baseline_exit_time_ms
                },
                record.symbol.as_str(),
                record.signal_time_ms,
                record_net_r(record, variant, cost_bps_per_side),
            )
        })
        .collect::<Vec<_>>();
    outcomes.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let wins = outcomes
        .iter()
        .filter(|outcome| outcome.3 > FLOAT_TOLERANCE)
        .count();
    let losses = outcomes
        .iter()
        .filter(|outcome| outcome.3 < -FLOAT_TOLERANCE)
        .count();
    let gross_profit_r = outcomes.iter().map(|outcome| outcome.3.max(0.0)).sum();
    let gross_loss_r = outcomes.iter().map(|outcome| (-outcome.3).max(0.0)).sum();
    let net_r = outcomes.iter().map(|outcome| outcome.3).sum::<f64>();
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for outcome in &outcomes {
        equity += outcome.3;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    RMetrics {
        trades: outcomes.len(),
        wins,
        losses,
        flat: outcomes.len() - wins - losses,
        win_rate_percent: if outcomes.is_empty() {
            0.0
        } else {
            wins as f64 / outcomes.len() as f64 * 100.0
        },
        net_r,
        average_net_r: if outcomes.is_empty() {
            0.0
        } else {
            net_r / outcomes.len() as f64
        },
        gross_profit_r,
        gross_loss_r,
        profit_factor_r: (gross_loss_r > 0.0).then_some(gross_profit_r / gross_loss_r),
        profit_factor_r_is_infinite: gross_loss_r == 0.0 && gross_profit_r > 0.0,
        chronological_closed_equity_max_drawdown_r: max_drawdown,
    }
}

fn record_net_r(record: &CounterfactualRecord, variant: bool, cost_bps_per_side: f64) -> f64 {
    net_r(
        record.direction,
        record.entry_price,
        if variant {
            record.variant_exit_price
        } else {
            record.baseline_exit_price
        },
        record.initial_risk,
        cost_bps_per_side,
    )
}

pub(super) fn net_r(
    direction: Direction,
    entry_price: f64,
    exit_price: f64,
    initial_risk: f64,
    cost_bps_per_side: f64,
) -> f64 {
    if initial_risk <= 0.0 {
        return 0.0;
    }
    let gross = direction.gross_pnl(entry_price, exit_price);
    let costs = (entry_price + exit_price) * cost_bps_per_side / 10_000.0;
    (gross - costs) / initial_risk
}

pub(super) fn contribution_comparisons(
    records: &[CounterfactualRecord],
    key: impl Fn(&CounterfactualRecord) -> String,
) -> Vec<ContributionComparison> {
    let mut grouped = BTreeMap::<String, Vec<&CounterfactualRecord>>::new();
    for record in records {
        grouped.entry(key(record)).or_default().push(record);
    }
    grouped
        .into_iter()
        .map(|(key, group)| {
            let baseline = group
                .iter()
                .map(|record| record_net_r(record, false, 8.0))
                .sum::<f64>();
            let variant = group
                .iter()
                .map(|record| record_net_r(record, true, 8.0))
                .sum::<f64>();
            ContributionComparison {
                key,
                trades: group.len(),
                activations: group
                    .iter()
                    .filter(|record| record.activation_time_ms.is_some())
                    .count(),
                changed_exits: group
                    .iter()
                    .filter(|record| {
                        record.variant_exit_kind != CounterfactualExitKind::BaselineUnchanged
                    })
                    .count(),
                baseline_net_r: baseline,
                variant_net_r: variant,
                variant_minus_baseline_net_r: variant - baseline,
            }
        })
        .collect()
}

pub(super) fn event_comparison(records: &[CounterfactualRecord]) -> EventComparison {
    let mut events = Vec::<Vec<usize>>::new();
    for direction in [Direction::Long, Direction::Short] {
        let mut ordered = records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| (record.direction == direction).then_some(index))
            .collect::<Vec<_>>();
        ordered.sort_by_key(|&index| records[index].signal_time_ms);
        for index in ordered {
            let starts_new = events.last().is_none_or(|event| {
                let previous = &records[*event.last().expect("event is non-empty")];
                previous.direction != direction
                    || records[index].signal_time_ms - previous.signal_time_ms > EVENT_CLUSTER_MS
            });
            if starts_new {
                events.push(vec![index]);
            } else {
                events.last_mut().expect("event exists").push(index);
            }
        }
    }

    let mut comparison = EventComparison {
        raw_trades: records.len(),
        events: events.len(),
        ..EventComparison::default()
    };
    for event in events {
        let symbols = event
            .iter()
            .map(|&index| records[index].symbol.as_str())
            .collect::<BTreeSet<_>>();
        let baseline = event
            .iter()
            .map(|&index| record_net_r(&records[index], false, 8.0))
            .sum::<f64>();
        let variant = event
            .iter()
            .map(|&index| record_net_r(&records[index], true, 8.0))
            .sum::<f64>();
        if event
            .iter()
            .any(|&index| records[index].activation_time_ms.is_some())
        {
            comparison.activated_events += 1;
        }
        if variant > baseline + FLOAT_TOLERANCE {
            comparison.improved_events += 1;
        } else if variant + FLOAT_TOLERANCE < baseline {
            comparison.worsened_events += 1;
        } else {
            comparison.unchanged_events += 1;
        }
        comparison.largest_event_trade_count =
            comparison.largest_event_trade_count.max(event.len());
        comparison.largest_event_symbol_count =
            comparison.largest_event_symbol_count.max(symbols.len());
        comparison.baseline_net_r += baseline;
        comparison.variant_net_r += variant;
    }
    comparison.variant_minus_baseline_net_r = comparison.variant_net_r - comparison.baseline_net_r;
    comparison
}

pub(super) fn metric_gate(
    records: &[CounterfactualRecord],
    costs: &[CostComparison],
    per_symbol: &[ContributionComparison],
    per_month: &[ContributionComparison],
    events: &EventComparison,
) -> MetricGate {
    let cost_8 = costs
        .iter()
        .find(|cost| nearly_equal(cost.cost_bps_per_side, 8.0))
        .expect("8bps cost exists");
    let improved_trades = records
        .iter()
        .filter(|record| {
            record_net_r(record, true, 8.0) > record_net_r(record, false, 8.0) + FLOAT_TOLERANCE
        })
        .count();
    let improved_symbols = per_symbol
        .iter()
        .filter(|item| item.variant_minus_baseline_net_r > FLOAT_TOLERANCE)
        .count();
    let improved_months = per_month
        .iter()
        .filter(|item| item.variant_minus_baseline_net_r > FLOAT_TOLERANCE)
        .count();
    let activated = records
        .iter()
        .filter(|record| record.activation_time_ms.is_some())
        .count();
    let pf_improved = strictly_higher_pf(&cost_8.variant, &cost_8.baseline);
    let variant_pf_above_one = cost_8.variant.profit_factor_r_is_infinite
        || cost_8
            .variant
            .profit_factor_r
            .is_some_and(|value| value > 1.0);
    let checks = [
        cost_8.variant.net_r > cost_8.baseline.net_r + FLOAT_TOLERANCE,
        cost_8.variant.average_net_r > cost_8.baseline.average_net_r + FLOAT_TOLERANCE,
        pf_improved,
        cost_8.variant.net_r > 0.0,
        variant_pf_above_one,
        activated >= 30,
        events.activated_events >= 20,
        improved_trades >= 3,
        improved_symbols >= 3,
        improved_months >= 3,
        events.improved_events >= 3,
    ];
    MetricGate {
        net_r_improved_at_8bps: checks[0],
        average_r_improved_at_8bps: checks[1],
        profit_factor_improved_at_8bps: checks[2],
        variant_net_r_positive_at_8bps: checks[3],
        variant_profit_factor_above_one_at_8bps: checks[4],
        activated_trades_at_least_30: checks[5],
        activated_events_at_least_20: checks[6],
        improved_trades_at_least_3: checks[7],
        improved_symbols_at_least_3: checks[8],
        improved_months_at_least_3: checks[9],
        improved_events_at_least_3: checks[10],
        trade_identity_preserved: true,
        metric_gate_passed: checks.into_iter().all(|passed| passed),
    }
}

fn strictly_higher_pf(candidate: &RMetrics, baseline: &RMetrics) -> bool {
    if candidate.profit_factor_r_is_infinite {
        return !baseline.profit_factor_r_is_infinite;
    }
    match (candidate.profit_factor_r, baseline.profit_factor_r) {
        (Some(candidate), Some(baseline)) => candidate > baseline + FLOAT_TOLERANCE,
        _ => false,
    }
}

pub(super) fn distribution_summary(mut values: Vec<f64>) -> DistributionSummary {
    if values.is_empty() {
        return DistributionSummary::default();
    }
    values.sort_by(f64::total_cmp);
    let at = |ratio: f64| values[((values.len() - 1) as f64 * ratio).floor() as usize];
    DistributionSummary {
        min: values[0],
        p25: at(0.25),
        p50: at(0.50),
        p75: at(0.75),
        max: values[values.len() - 1],
    }
}

pub(super) fn shanghai_month(timestamp_ms: i64) -> String {
    let Some(timestamp) = Utc
        .timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
    else {
        return "invalid".to_owned();
    };
    format!("{:04}-{:02}", timestamp.year(), timestamp.month())
}
