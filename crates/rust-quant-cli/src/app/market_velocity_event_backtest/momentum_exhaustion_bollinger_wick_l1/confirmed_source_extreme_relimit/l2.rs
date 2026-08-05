//! L1 重挂成交 cohort 的条件 L2 成本后配对回放。

use super::{
    RelimitCandidate, RelimitConcentration, RelimitL1Report, RelimitL2Decision,
    RelimitL2EntrySummary, RelimitL2Identity, RelimitL2Report, RelimitLegRecord,
    RelimitPerformance, RelimitTradeRecord, RELIMIT_L2_RULE_VERSION,
};
use crate::app::market_velocity_event_backtest::{
    BacktestDataSet, ComputedCandle, MarketVelocityTradeDirection,
};
use anyhow::{bail, Result};
use chrono::{TimeZone, Utc};
use std::collections::{BTreeMap, BTreeSet};

const INITIAL_STOP_ATR_MULTIPLIER: f64 = 1.5;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 48 * 60 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const MS_15M: i64 = 15 * 60 * 1_000;

#[derive(Debug, Clone, Copy)]
struct EntryPlan {
    entry_ts_ms: i64,
    entry_idx: usize,
    entry_price: f64,
    stop_price: f64,
    target_price: f64,
}

#[derive(Debug, Clone)]
struct PairedEntry {
    candidate_id: String,
    symbol: String,
    setup_ts_ms: i64,
    confirmation_signal_ts_ms: i64,
    original_expiry_ts_ms: i64,
    direction: MarketVelocityTradeDirection,
    source_trigger: String,
    source_extreme_price: f64,
    filtered_volume_ratio: f64,
    source_atr14: f64,
    target_atr_multiplier: f64,
    baseline: EntryPlan,
    candidate: EntryPlan,
}

#[derive(Debug, Clone, Copy)]
struct ExitPath {
    complete: bool,
    exit_ts_ms: i64,
    exit_price: f64,
    exit_reason: &'static str,
}

/// 对 L1 已成交 cohort 构建相同风险、成本和共同冲突的唯一配对回放。
pub(super) fn build_l2(data: &BacktestDataSet, l1: &RelimitL1Report) -> Result<RelimitL2Report> {
    if l1.decision.status != "coverage_pass_l2_ready" || l1.decision.outcome_evaluation_performed {
        bail!("L1 relimit ledger is not eligible for L2");
    }
    let filled = l1
        .candidates
        .iter()
        .filter(|candidate| candidate.relimit_entry_ts_ms.is_some())
        .collect::<Vec<_>>();
    if filled.len() != l1.summary.relimit_filled_setups {
        bail!("L1 relimit fill count mismatch");
    }
    let mut blockers = BTreeMap::new();
    let mut pairs = Vec::with_capacity(filled.len());
    for candidate in filled {
        match resolve_paired_entry(data, candidate) {
            Ok(pair) => pairs.push(pair),
            Err(reason) => *blockers.entry(reason.to_owned()).or_default() += 1,
        }
    }
    pairs.sort_by(|left, right| {
        (
            left.baseline.entry_ts_ms,
            left.symbol.as_str(),
            left.setup_ts_ms,
        )
            .cmp(&(
                right.baseline.entry_ts_ms,
                right.symbol.as_str(),
                right.setup_ts_ms,
            ))
    });
    let resolved_pairs = pairs.len();
    let mut trades = simulate_with_shared_conflicts(data, pairs, &mut blockers);
    trades.sort_by(|left, right| {
        (
            left.candidate_relimit.entry_ts_ms,
            left.symbol.as_str(),
            left.setup_ts_ms,
        )
            .cmp(&(
                right.candidate_relimit.entry_ts_ms,
                right.symbol.as_str(),
                right.setup_ts_ms,
            ))
    });
    let completed = trades
        .iter()
        .filter(|trade| trade.baseline_next_open.complete && trade.candidate_relimit.complete)
        .collect::<Vec<_>>();
    let baseline = performance(completed.iter().map(|trade| trade.baseline_next_open.net_r));
    let candidate = performance(completed.iter().map(|trade| trade.candidate_relimit.net_r));
    let baseline_by_direction = performance_by_direction(&completed, false);
    let candidate_by_direction = performance_by_direction(&completed, true);
    let concentration = concentration(&completed);
    let paired_contract_identity_verified = trades.iter().all(contract_is_consistent);
    let entry_summary = entry_summary(
        l1.summary.relimit_filled_setups,
        resolved_pairs,
        &trades,
        &completed,
        blockers,
    );
    let decision = decide_l2(
        &entry_summary,
        &baseline,
        &candidate,
        &concentration,
        paired_contract_identity_verified,
    );
    Ok(RelimitL2Report {
        identity: RelimitL2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            rule_version: RELIMIT_L2_RULE_VERSION,
            baseline_entry_policy: "on the same L1-filled cohort, enter at the open of the first 15m candle after strict source-extreme reclaim confirmation",
            candidate_entry_policy: "enter at the frozen source extreme when touched from the next candle through original setup offset 12",
            initial_stop_policy: "each side reanchors the unchanged 1.5 times source-setup ATR14 risk distance to its actual entry",
            target_policy: "each side reanchors the unchanged 2.7/3.6/4.5 source-setup ATR14 target distance to its actual entry",
            intrabar_conflict_policy: "from each actual entry candle, stop wins when stop and target are both reachable without tick ordering",
            paired_position_conflict_policy: "both sides share one symbol lock ending at the later paired exit; any conflict drops both sides",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            outcome_evaluation_performed: true,
        },
        entry_summary,
        baseline_next_open: baseline,
        candidate_relimit: candidate,
        baseline_by_direction,
        candidate_by_direction,
        concentration,
        paired_contract_identity_verified,
        decision,
        trades,
    })
}

/// 从确认后下一根开盘与 L1 重挂成交构造相同 ATR 风险的 pair。
fn resolve_paired_entry(
    data: &BacktestDataSet,
    candidate: &RelimitCandidate,
) -> Result<PairedEntry, &'static str> {
    let candles = data
        .candles_15m_computed
        .get(&candidate.symbol)
        .ok_or("symbol_candles_missing")?;
    let setup_idx = candles
        .binary_search_by_key(&candidate.setup_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "setup_candle_missing")?;
    let setup = candles.get(setup_idx).ok_or("setup_candle_missing")?;
    let source_atr14 = setup
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("source_atr14_invalid")?;
    if candidate.activation_ts_ms != candidate.confirmation_signal_ts_ms.saturating_add(MS_15M) {
        return Err("baseline_activation_timestamp_mismatch");
    }
    let baseline_idx = candles
        .binary_search_by_key(&candidate.activation_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "next_open_candle_missing")?;
    let baseline_candle = candles
        .get(baseline_idx)
        .ok_or("next_open_candle_missing")?;
    let relimit_ts_ms = candidate
        .relimit_entry_ts_ms
        .ok_or("filled_candidate_missing_relimit_timestamp")?;
    let relimit_idx = candles
        .binary_search_by_key(&relimit_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "relimit_candle_missing")?;
    if relimit_idx < baseline_idx || relimit_ts_ms > candidate.original_expiry_ts_ms {
        return Err("relimit_timestamp_outside_frozen_lifetime");
    }
    let relimit_candle = candles.get(relimit_idx).ok_or("relimit_candle_missing")?;
    let direction = parse_direction(&candidate.direction)?;
    if !directional_extreme_touched(relimit_candle, candidate.source_extreme_price, direction) {
        return Err("relimit_candle_does_not_touch_source_extreme");
    }
    let target_atr_multiplier = target_atr_multiplier(candidate.filtered_volume_ratio)
        .ok_or("source_target_tier_invalid")?;
    let baseline = entry_plan(
        candidate.activation_ts_ms,
        baseline_idx,
        baseline_candle.candle.open,
        source_atr14,
        target_atr_multiplier,
        direction,
    )?;
    let relimit = entry_plan(
        relimit_ts_ms,
        relimit_idx,
        candidate.source_extreme_price,
        source_atr14,
        target_atr_multiplier,
        direction,
    )?;
    Ok(PairedEntry {
        candidate_id: candidate.candidate_id.clone(),
        symbol: candidate.symbol.clone(),
        setup_ts_ms: candidate.setup_ts_ms,
        confirmation_signal_ts_ms: candidate.confirmation_signal_ts_ms,
        original_expiry_ts_ms: candidate.original_expiry_ts_ms,
        direction,
        source_trigger: candidate.source_trigger.clone(),
        source_extreme_price: candidate.source_extreme_price,
        filtered_volume_ratio: candidate.filtered_volume_ratio,
        source_atr14,
        target_atr_multiplier,
        baseline,
        candidate: relimit,
    })
}

/// 以每一侧实际成交价重锚相同来源 ATR 的止损和目标距离。
fn entry_plan(
    entry_ts_ms: i64,
    entry_idx: usize,
    entry_price: f64,
    atr14: f64,
    target_atr: f64,
    direction: MarketVelocityTradeDirection,
) -> Result<EntryPlan, &'static str> {
    if !entry_price.is_finite() || entry_price <= 0.0 {
        return Err("entry_price_invalid");
    }
    let (stop_price, target_price) = match direction {
        MarketVelocityTradeDirection::Long => (
            entry_price - INITIAL_STOP_ATR_MULTIPLIER * atr14,
            entry_price + target_atr * atr14,
        ),
        MarketVelocityTradeDirection::Short => (
            entry_price + INITIAL_STOP_ATR_MULTIPLIER * atr14,
            entry_price - target_atr * atr14,
        ),
        MarketVelocityTradeDirection::Both => return Err("candidate_direction_invalid"),
    };
    if !stop_price.is_finite()
        || stop_price <= 0.0
        || !target_price.is_finite()
        || target_price <= 0.0
    {
        return Err("risk_or_target_price_invalid");
    }
    Ok(EntryPlan {
        entry_ts_ms,
        entry_idx,
        entry_price,
        stop_price,
        target_price,
    })
}

/// 两侧共享一个币种锁，防止不同入场时点形成不同的可执行交易集合。
fn simulate_with_shared_conflicts(
    data: &BacktestDataSet,
    entries: Vec<PairedEntry>,
    blockers: &mut BTreeMap<String, usize>,
) -> Vec<RelimitTradeRecord> {
    let mut by_symbol: BTreeMap<String, Vec<PairedEntry>> = BTreeMap::new();
    for entry in entries {
        by_symbol
            .entry(entry.symbol.clone())
            .or_default()
            .push(entry);
    }
    let mut records = Vec::new();
    for (symbol, mut entries) in by_symbol {
        entries.sort_by_key(|entry| (entry.baseline.entry_ts_ms, entry.setup_ts_ms));
        let Some(candles) = data.candles_15m_computed.get(&symbol) else {
            *blockers
                .entry("symbol_candles_missing_during_replay".to_owned())
                .or_default() += entries.len();
            continue;
        };
        let mut locked_until = i64::MIN;
        for entry in entries {
            if entry.baseline.entry_ts_ms <= locked_until
                || entry.candidate.entry_ts_ms <= locked_until
            {
                *blockers
                    .entry("pair_ignored_while_shared_symbol_lock_open".to_owned())
                    .or_default() += 1;
                continue;
            }
            let baseline_path = simulate_exit(candles, entry.baseline, entry.direction);
            let candidate_path = simulate_exit(candles, entry.candidate, entry.direction);
            let (Some(baseline_path), Some(candidate_path)) = (baseline_path, candidate_path)
            else {
                *blockers
                    .entry("entry_candle_missing_during_replay".to_owned())
                    .or_default() += 1;
                continue;
            };
            locked_until = baseline_path.exit_ts_ms.max(candidate_path.exit_ts_ms);
            records.push(build_trade_record(&entry, baseline_path, candidate_path));
        }
    }
    records
}

/// 从各自实际入场 K 起执行止损优先、冻结目标和 48 小时超时。
fn simulate_exit(
    candles: &[ComputedCandle],
    plan: EntryPlan,
    direction: MarketVelocityTradeDirection,
) -> Option<ExitPath> {
    let horizon_end = plan.entry_ts_ms.saturating_add(MAX_HOLDING_MS);
    let mut last_seen = None;
    for candle in candles.get(plan.entry_idx..)? {
        if candle.candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let stop_hit = loss_level_touched(candle, plan.stop_price, direction);
        let target_hit = profit_level_touched(candle, plan.target_price, direction);
        if stop_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: plan.stop_price,
                exit_reason: if target_hit {
                    "both_hit_stop_first"
                } else {
                    "stop_hit"
                },
            });
        }
        if target_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: plan.target_price,
                exit_reason: "original_target_hit",
            });
        }
        if candle.candle.ts >= horizon_end {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: candle.candle.close,
                exit_reason: "max_holding_timeout",
            });
        }
    }
    let last = last_seen?;
    Some(ExitPath {
        complete: false,
        exit_ts_ms: last.candle.ts,
        exit_price: last.candle.close,
        exit_reason: "forward_data_incomplete",
    })
}

/// 将 pair 的两条路径折算为各自相同来源 ATR 风险分母的成本后 R。
fn build_trade_record(
    entry: &PairedEntry,
    baseline_path: ExitPath,
    candidate_path: ExitPath,
) -> RelimitTradeRecord {
    let baseline_next_open = leg_record(entry.baseline, baseline_path, entry.direction);
    let candidate_relimit = leg_record(entry.candidate, candidate_path, entry.direction);
    RelimitTradeRecord {
        candidate_id: entry.candidate_id.clone(),
        symbol: entry.symbol.clone(),
        setup_ts_ms: entry.setup_ts_ms,
        confirmation_signal_ts_ms: entry.confirmation_signal_ts_ms,
        original_expiry_ts_ms: entry.original_expiry_ts_ms,
        direction: direction_label(entry.direction),
        source_trigger: entry.source_trigger.clone(),
        source_extreme_price: entry.source_extreme_price,
        filtered_volume_ratio: entry.filtered_volume_ratio,
        source_atr14: entry.source_atr14,
        target_atr_multiplier: entry.target_atr_multiplier,
        delta_net_r: candidate_relimit.net_r - baseline_next_open.net_r,
        baseline_next_open,
        candidate_relimit,
    }
}

/// 构造单侧完整入场、风险、退出和双边名义成本证据。
fn leg_record(
    plan: EntryPlan,
    path: ExitPath,
    direction: MarketVelocityTradeDirection,
) -> RelimitLegRecord {
    let risk = (plan.entry_price - plan.stop_price).abs();
    RelimitLegRecord {
        entry_ts_ms: plan.entry_ts_ms,
        entry_price: plan.entry_price,
        initial_stop_price: plan.stop_price,
        initial_risk_price: risk,
        target_price: plan.target_price,
        complete: path.complete,
        exit_ts_ms: path.exit_ts_ms,
        exit_price: path.exit_price,
        exit_reason: path.exit_reason,
        net_r: net_r(plan.entry_price, path.exit_price, risk, direction),
    }
}

/// 汇总共同执行后的完整 pair 方向、币种、月份、事件与 blocker。
fn entry_summary(
    l1_filled_setups: usize,
    resolved_pairs: usize,
    trades: &[RelimitTradeRecord],
    completed: &[&RelimitTradeRecord],
    blockers: BTreeMap<String, usize>,
) -> RelimitL2EntrySummary {
    let completed_long = completed
        .iter()
        .filter(|trade| trade.direction == "long")
        .count();
    let completed_short = completed
        .iter()
        .filter(|trade| trade.direction == "short")
        .count();
    let symbols = completed
        .iter()
        .map(|trade| trade.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let months = completed
        .iter()
        .filter_map(|trade| {
            Utc.timestamp_millis_opt(trade.candidate_relimit.entry_ts_ms)
                .single()
                .map(|value| value.format("%Y-%m").to_string())
        })
        .collect::<BTreeSet<_>>()
        .len();
    RelimitL2EntrySummary {
        l1_filled_setups,
        resolved_pairs,
        executed_pairs: trades.len(),
        completed_pairs: completed.len(),
        incomplete_pairs: trades.len().saturating_sub(completed.len()),
        completed_by_direction: BTreeMap::from([
            ("long", completed_long),
            ("short", completed_short),
        ]),
        completed_symbol_count: symbols,
        completed_month_count: months,
        completed_effective_market_events: effective_market_event_count(completed),
        blockers,
    }
}

/// 计算成本后逐笔 R 的期望、PF、胜率、交易级 Sharpe 与累计 R 回撤。
fn performance(values: impl Iterator<Item = f64>) -> RelimitPerformance {
    let values = values.collect::<Vec<_>>();
    let trades = values.len();
    let positive_net_r = values.iter().copied().filter(|value| *value > 0.0).sum();
    let negative_net_r_abs = -values
        .iter()
        .copied()
        .filter(|value| *value < 0.0)
        .sum::<f64>();
    let net_sum_r = values.iter().sum::<f64>();
    RelimitPerformance {
        trades,
        positive_net_r,
        negative_net_r_abs,
        net_sum_r,
        net_expectancy_r: if trades == 0 {
            0.0
        } else {
            net_sum_r / trades as f64
        },
        net_profit_factor: (negative_net_r_abs > 0.0)
            .then_some(positive_net_r / negative_net_r_abs),
        win_rate_pct: if trades == 0 {
            0.0
        } else {
            values.iter().filter(|value| **value > 0.0).count() as f64 / trades as f64 * 100.0
        },
        trade_sharpe: trade_sharpe(&values),
        max_drawdown_r: max_drawdown_r(&values),
    }
}

/// 分别报告多空成本后指标，防止单一方向掩盖另一方向退化。
fn performance_by_direction(
    trades: &[&RelimitTradeRecord],
    candidate: bool,
) -> BTreeMap<&'static str, RelimitPerformance> {
    ["long", "short"]
        .into_iter()
        .map(|direction| {
            let metrics = performance(
                trades
                    .iter()
                    .filter(|trade| trade.direction == direction)
                    .map(|trade| {
                        if candidate {
                            trade.candidate_relimit.net_r
                        } else {
                            trade.baseline_next_open.net_r
                        }
                    }),
            );
            (direction, metrics)
        })
        .collect()
}

/// 汇总候选减基线的交易、币种和方向增量集中度。
fn concentration(trades: &[&RelimitTradeRecord]) -> RelimitConcentration {
    let mut deltas = trades
        .iter()
        .map(|trade| trade.delta_net_r)
        .collect::<Vec<_>>();
    deltas.sort_by(|left, right| right.total_cmp(left));
    let total_delta_net_r = deltas.iter().sum::<f64>();
    let top_two = deltas.iter().take(2).sum::<f64>();
    let mut delta_net_r_by_symbol = BTreeMap::new();
    let mut delta_net_r_by_direction = BTreeMap::from([("long", 0.0), ("short", 0.0)]);
    let mut positive_by_symbol = BTreeMap::new();
    let mut total_positive_delta = 0.0;
    for trade in trades {
        *delta_net_r_by_symbol
            .entry(trade.symbol.clone())
            .or_default() += trade.delta_net_r;
        *delta_net_r_by_direction.entry(trade.direction).or_default() += trade.delta_net_r;
        if trade.delta_net_r > 0.0 {
            *positive_by_symbol.entry(trade.symbol.clone()).or_default() += trade.delta_net_r;
            total_positive_delta += trade.delta_net_r;
        }
    }
    let max_symbol_positive_delta_share_pct = (total_positive_delta > 0.0).then(|| {
        positive_by_symbol.values().copied().fold(0.0_f64, f64::max) / total_positive_delta * 100.0
    });
    RelimitConcentration {
        total_delta_net_r,
        delta_net_r_after_removing_top_two_trades: total_delta_net_r - top_two,
        max_symbol_positive_delta_share_pct,
        delta_net_r_by_symbol,
        delta_net_r_by_direction,
    }
}

/// 应用预注册成本后边际、方向、分散性与合同联合门禁。
fn decide_l2(
    summary: &RelimitL2EntrySummary,
    baseline: &RelimitPerformance,
    candidate: &RelimitPerformance,
    concentration: &RelimitConcentration,
    paired_contract_identity_verified: bool,
) -> RelimitL2Decision {
    let long_count = summary
        .completed_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .completed_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let long_delta = concentration
        .delta_net_r_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_delta = concentration
        .delta_net_r_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert("l1_coverage_passed", true);
    gates.insert("completed_pairs_at_least_30", summary.completed_pairs >= 30);
    gates.insert(
        "completed_both_directions_at_least_5",
        long_count >= 5 && short_count >= 5,
    );
    gates.insert(
        "completed_symbols_at_least_8",
        summary.completed_symbol_count >= 8,
    );
    gates.insert(
        "completed_months_at_least_6",
        summary.completed_month_count >= 6,
    );
    gates.insert(
        "completed_effective_events_at_least_15",
        summary.completed_effective_market_events >= 15,
    );
    gates.insert(
        "candidate_expectancy_positive_and_above_baseline",
        candidate.net_expectancy_r > 0.0 && candidate.net_expectancy_r > baseline.net_expectancy_r,
    );
    gates.insert(
        "candidate_profit_factor_not_below_baseline",
        profit_factor_not_worse(baseline, candidate),
    );
    gates.insert(
        "both_direction_delta_non_negative",
        long_delta >= 0.0 && short_delta >= 0.0,
    );
    gates.insert(
        "total_delta_net_r_positive",
        concentration.total_delta_net_r > 0.0,
    );
    gates.insert(
        "delta_positive_after_removing_top_two",
        concentration.delta_net_r_after_removing_top_two_trades > 0.0,
    );
    gates.insert(
        "max_symbol_positive_delta_share_at_most_35_pct",
        concentration
            .max_symbol_positive_delta_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert(
        "paired_contract_identity_verified",
        paired_contract_identity_verified,
    );
    let passed = gates.values().all(|value| *value);
    RelimitL2Decision {
        status: if passed {
            "L2_pass_L3_required"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "来源极值重挂在同成交 cohort 中形成分散且稳健的成本后正增量；仍需 L3 point-in-time、OOS、统一资金和压力验证。"
                .to_owned()
        } else {
            "至少一项预注册 L2 门禁失败；淘汰本重挂版本，不叠加其他过滤或退出变量。".to_owned()
        },
    }
}

/// 核对两侧共享 setup ATR、目标公式，并且候选成交未越过原有效期。
fn contract_is_consistent(trade: &RelimitTradeRecord) -> bool {
    let expected_risk = trade.source_atr14 * INITIAL_STOP_ATR_MULTIPLIER;
    let expected_target = trade.source_atr14 * trade.target_atr_multiplier;
    approx_equal(trade.baseline_next_open.initial_risk_price, expected_risk)
        && approx_equal(trade.candidate_relimit.initial_risk_price, expected_risk)
        && approx_equal(
            (trade.baseline_next_open.target_price - trade.baseline_next_open.entry_price).abs(),
            expected_target,
        )
        && approx_equal(
            (trade.candidate_relimit.target_price - trade.candidate_relimit.entry_price).abs(),
            expected_target,
        )
        && approx_equal(
            trade.candidate_relimit.entry_price,
            trade.source_extreme_price,
        )
        && trade.baseline_next_open.entry_ts_ms
            == trade.confirmation_signal_ts_ms.saturating_add(MS_15M)
        && trade.candidate_relimit.entry_ts_ms >= trade.baseline_next_open.entry_ts_ms
        && trade.candidate_relimit.entry_ts_ms <= trade.original_expiry_ts_ms
        && trade.baseline_next_open.exit_reason != ""
        && trade.candidate_relimit.exit_reason != ""
}

/// 按候选成交时间和方向将一小时内的跨币共振归并为一个事件。
fn effective_market_event_count(trades: &[&RelimitTradeRecord]) -> usize {
    let mut ordered = trades.to_vec();
    ordered.sort_by_key(|trade| {
        (
            trade.candidate_relimit.entry_ts_ms,
            trade.direction,
            trade.symbol.as_str(),
        )
    });
    let mut last_by_direction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut count = 0;
    for trade in ordered {
        let starts_new = last_by_direction
            .get(trade.direction)
            .is_none_or(|previous| {
                trade.candidate_relimit.entry_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS
            });
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(trade.direction, trade.candidate_relimit.entry_ts_ms);
    }
    count
}

/// 来源 V2 过滤量比沿用 2.7、3.6、4.5 ATR 三档目标。
fn target_atr_multiplier(filtered_volume_ratio: f64) -> Option<f64> {
    if !filtered_volume_ratio.is_finite() || filtered_volume_ratio < 2.5 {
        return None;
    }
    if filtered_volume_ratio < 4.0 {
        Some(2.7)
    } else if filtered_volume_ratio < 6.0 {
        Some(3.6)
    } else {
        Some(4.5)
    }
}

/// 将冻结字符串方向转换为现有回放方向枚举。
fn parse_direction(value: &str) -> Result<MarketVelocityTradeDirection, &'static str> {
    match value {
        "long" => Ok(MarketVelocityTradeDirection::Long),
        "short" => Ok(MarketVelocityTradeDirection::Short),
        _ => Err("candidate_direction_invalid"),
    }
}

/// 将方向枚举写回稳定机器标签。
fn direction_label(direction: MarketVelocityTradeDirection) -> &'static str {
    match direction {
        MarketVelocityTradeDirection::Long => "long",
        MarketVelocityTradeDirection::Short => "short",
        MarketVelocityTradeDirection::Both => "both",
    }
}

/// 判断重挂价格在成交 K 的方向极值内是否可达。
fn directional_extreme_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.low <= price,
        MarketVelocityTradeDirection::Short => candle.candle.high >= price,
        MarketVelocityTradeDirection::Both => false,
    }
}

/// 判断初始止损在当前 K 内是否可达。
fn loss_level_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.low <= price,
        MarketVelocityTradeDirection::Short => candle.candle.high >= price,
        MarketVelocityTradeDirection::Both => true,
    }
}

/// 判断冻结目标在当前 K 内是否可达。
fn profit_level_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.high >= price,
        MarketVelocityTradeDirection::Short => candle.candle.low <= price,
        MarketVelocityTradeDirection::Both => false,
    }
}

/// 按实际开平名义价格扣除双边成本并折算到初始风险 R。
fn net_r(
    entry_price: f64,
    exit_price: f64,
    risk: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    let gross = match direction {
        MarketVelocityTradeDirection::Long => (exit_price - entry_price) / risk,
        MarketVelocityTradeDirection::Short => (entry_price - exit_price) / risk,
        MarketVelocityTradeDirection::Both => 0.0,
    };
    gross - (entry_price + exit_price) * PER_SIDE_COST_RATE / risk
}

/// 计算逐笔 R 的交易级 Sharpe；不足两笔或零方差时为空。
fn trade_sharpe(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    (variance > 0.0).then_some(mean / variance.sqrt() * (values.len() as f64).sqrt())
}

/// 计算按候选成交顺序累计净 R 的最大回撤。
fn max_drawdown_r(values: &[f64]) -> f64 {
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    max_drawdown
}

/// 比较有限 PF 与没有负交易的无穷 PF 情形。
fn profit_factor_not_worse(baseline: &RelimitPerformance, candidate: &RelimitPerformance) -> bool {
    match (baseline.net_profit_factor, candidate.net_profit_factor) {
        (_, None) if candidate.negative_net_r_abs == 0.0 => true,
        (None, Some(_)) if baseline.negative_net_r_abs == 0.0 => false,
        (Some(base), Some(value)) => value >= base,
        _ => false,
    }
}

/// 浮点风险合同使用相对误差，适配不同价格量级。
fn approx_equal(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 1e-10
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    /// 构造测试用完整 K 线，只覆盖入场后止损和目标路径。
    fn candle(ts: i64, open: f64, high: f64, low: f64, close: f64) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts,
                open,
                high,
                low,
                close,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    /// 两侧不同入场价仍必须共享 1.5 倍来源 ATR 风险距离。
    #[test]
    fn entry_plans_share_source_atr_risk_distance() {
        let baseline = entry_plan(0, 0, 99.0, 2.0, 2.7, MarketVelocityTradeDirection::Short)
            .expect("baseline plan");
        let candidate = entry_plan(1, 1, 100.0, 2.0, 2.7, MarketVelocityTradeDirection::Short)
            .expect("candidate plan");
        assert!(approx_equal(
            (baseline.stop_price - baseline.entry_price).abs(),
            3.0
        ));
        assert!(approx_equal(
            (candidate.stop_price - candidate.entry_price).abs(),
            3.0
        ));
    }

    /// 没有 tick 顺序时同棒同时触发止损和目标必须按止损退出。
    #[test]
    fn same_bar_stop_and_target_uses_stop_first() {
        let candles = vec![candle(0, 100.0, 104.0, 94.0, 100.0)];
        let plan = EntryPlan {
            entry_ts_ms: 0,
            entry_idx: 0,
            entry_price: 100.0,
            stop_price: 103.0,
            target_price: 95.0,
        };
        let path =
            simulate_exit(&candles, plan, MarketVelocityTradeDirection::Short).expect("exit path");
        assert_eq!(path.exit_reason, "both_hit_stop_first");
        assert_eq!(path.exit_price, 103.0);
    }

    /// 量比分档必须沿用来源 V2 的冻结目标。
    #[test]
    fn target_tiers_match_source_v2() {
        assert_eq!(target_atr_multiplier(2.5), Some(2.7));
        assert_eq!(target_atr_multiplier(4.0), Some(3.6));
        assert_eq!(target_atr_multiplier(6.0), Some(4.5));
        assert_eq!(target_atr_multiplier(2.49), None);
    }

    /// 双边名义成本应让毛 1R 的净结果严格低于 1R。
    #[test]
    fn net_r_deducts_entry_and_exit_costs() {
        let value = net_r(100.0, 97.0, 3.0, MarketVelocityTradeDirection::Short);
        assert!(value < 1.0);
        assert!(value > 0.9);
    }
}
