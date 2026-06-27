use super::{
    summarize_target, BacktestCandle, BacktestDataSet, ComputedCandle, ConfirmedEvent,
    EvaluationReport, MarketVelocityEventBacktestArgs, TradeOutcome, TradeResult,
};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EffectiveEntrySummary {
    pub(super) raw_events: usize,
    pub(super) trend_pass: usize,
    pub(super) entry_pass_before_filters: usize,
    pub(super) symbol_after: usize,
    pub(super) trigger_after: usize,
    pub(super) raw_open_rate_pct: Option<f64>,
    pub(super) trend_open_rate_pct: Option<f64>,
    pub(super) trigger_keep_pct: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
struct TriggerQualitySummary {
    trigger_label: String,
    trades: usize,
    win: usize,
    loss: usize,
    flat: usize,
    timeout: usize,
    incomplete: usize,
    complete_avg_r: Option<f64>,
}
/// 执行输出stage报告步骤，串起回测策略需要的状态推进和错误处理。
pub(super) fn print_stage_report(data: &BacktestDataSet, evaluation: &EvaluationReport) {
    println!("candle_pairs={}", data.pairs.len());
    for pair in &data.pairs {
        let coverage_15m = data
            .candles_15m
            .get(&pair.symbol)
            .map(|candles| coverage(candles))
            .unwrap_or_else(|| "0".to_string());
        let coverage_4h = data
            .candles_4h_computed
            .get(&pair.symbol)
            .map(|candles| coverage_computed(candles))
            .unwrap_or_else(|| "0".to_string());
        println!(
            "coverage\t{}\t15m={}\t4h={}",
            pair.symbol, coverage_15m, coverage_4h
        );
    }
    println!("raw_candidate_events={}", data.events.len());
    println!("stage_counts\t{}", format_counter(&evaluation.stage_counts));
    println!(
        "pass_by_symbol\t{}",
        format_pass_by_symbol(&evaluation.confirmed)
    );
    for (symbol, counter) in &evaluation.blockers {
        println!("blockers\t{}\t{}", symbol, format_counter_top(counter, 10));
    }
}
/// 执行输出有效开仓报告步骤，串起回测策略需要的状态推进和错误处理。
pub(super) fn print_effective_entry_report(
    raw_events: usize,
    evaluation: &EvaluationReport,
    symbol_filtered: &[ConfirmedEvent],
    trigger_filtered: &[ConfirmedEvent],
) {
    let summary = effective_entry_summary(
        raw_events,
        *evaluation.stage_counts.get("trend_pass").unwrap_or(&0),
        evaluation.confirmed.len(),
        symbol_filtered.len(),
        trigger_filtered.len(),
    );
    println!(
        "effective_entry\traw={}\ttrend_pass={}\tentry_pass_before_filters={}\tsymbol_after={}\ttrigger_after={}\traw_open_rate_pct={}\ttrend_open_rate_pct={}\ttrigger_keep_pct={}",
        summary.raw_events,
        summary.trend_pass,
        summary.entry_pass_before_filters,
        summary.symbol_after,
        summary.trigger_after,
        format_optional_f64(summary.raw_open_rate_pct),
        format_optional_f64(summary.trend_open_rate_pct),
        format_optional_f64(summary.trigger_keep_pct)
    );
}
/// 执行输出触发器质量报告步骤，串起回测策略需要的状态推进和错误处理。
pub(super) fn print_trigger_quality_report(
    scope: &str,
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) {
    if confirmed.is_empty() {
        return;
    }
    for target_r in &args.target_rs {
        for (horizon_name, horizon_ms) in
            [("24h", 24 * 60 * 60 * 1_000), ("48h", 48 * 60 * 60 * 1_000)]
        {
            let (results, _skipped_lock) =
                summarize_target(confirmed, candles_15m, *target_r, horizon_ms, args);
            for summary in summarize_results_by_base_trigger(&results) {
                println!(
                    "trigger_quality\tscope={}\ttarget={}R\thorizon={}\ttrigger={}\ttrades={}\twin={}\tloss={}\tflat={}\ttimeout={}\tincomplete={}\tcomplete_avg_r={}",
                    scope,
                    target_r,
                    horizon_name,
                    summary.trigger_label,
                    summary.trades,
                    summary.win,
                    summary.loss,
                    summary.flat,
                    summary.timeout,
                    summary.incomplete,
                    format_optional_f64(summary.complete_avg_r)
                );
            }
        }
    }
}
/// 执行输出完整触发路径质量报告步骤，串起回测策略需要的状态推进和错误处理。
pub(super) fn print_trigger_variant_quality_report(
    scope: &str,
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) {
    if confirmed.is_empty() {
        return;
    }
    for target_r in &args.target_rs {
        for (horizon_name, horizon_ms) in
            [("24h", 24 * 60 * 60 * 1_000), ("48h", 48 * 60 * 60 * 1_000)]
        {
            let (results, _skipped_lock) =
                summarize_target(confirmed, candles_15m, *target_r, horizon_ms, args);
            for summary in summarize_results_by_exact_trigger(&results) {
                println!(
                    "trigger_variant_quality\tscope={}\ttarget={}R\thorizon={}\ttrigger={}\ttrades={}\twin={}\tloss={}\tflat={}\ttimeout={}\tincomplete={}\tcomplete_avg_r={}",
                    scope,
                    target_r,
                    horizon_name,
                    summary.trigger_label,
                    summary.trades,
                    summary.win,
                    summary.loss,
                    summary.flat,
                    summary.timeout,
                    summary.incomplete,
                    format_optional_f64(summary.complete_avg_r)
                );
            }
        }
    }
}
/// 执行输出结果报告步骤，串起回测策略需要的状态推进和错误处理。
pub(super) fn print_result_report(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) {
    for target_r in &args.target_rs {
        for (horizon_name, horizon_ms) in
            [("24h", 24 * 60 * 60 * 1_000), ("48h", 48 * 60 * 60 * 1_000)]
        {
            let (results, skipped_lock) =
                summarize_target(confirmed, candles_15m, *target_r, horizon_ms, args);
            let counts = count_outcomes(&results);
            let complete = results.iter().filter(|result| result.complete).count();
            let resolved = counts.get(&TradeOutcome::Win).copied().unwrap_or_default()
                + counts.get(&TradeOutcome::Loss).copied().unwrap_or_default();
            let wins = counts.get(&TradeOutcome::Win).copied().unwrap_or_default();
            let flats = counts.get(&TradeOutcome::Flat).copied().unwrap_or_default();
            let complete_win_rate = percent(wins, complete);
            let resolved_win_rate = percent(wins, resolved);
            let avg_r = average_complete_r(&results);
            println!(
                "result\ttarget={}R\thorizon={}\ttrades={}\tskipped_lock={}\twin={}\tloss={}\tflat={}\ttimeout={}\tincomplete={}\tcomplete_win_rate={}\tresolved_win_rate={}\tavg_r_complete={}",
                target_r,
                horizon_name,
                results.len(),
                skipped_lock,
                wins,
                counts.get(&TradeOutcome::Loss).copied().unwrap_or_default(),
                flats,
                counts.get(&TradeOutcome::Timeout).copied().unwrap_or_default(),
                counts.get(&TradeOutcome::Incomplete).copied().unwrap_or_default(),
                format_optional_f64(complete_win_rate),
                format_optional_f64(resolved_win_rate),
                format_optional_f64(avg_r)
            );
            for result in results.iter().take(args.sample_limit) {
                println!(
                    "trade_sample\ttarget={}R\thorizon={}\t{}\t{}\tentry_ts={}\tentry={:.8}\toutcome={}\treason={}\tr={}",
                    target_r,
                    horizon_name,
                    result.symbol.as_deref().unwrap_or("NA"),
                    result.detected_at.as_deref().unwrap_or("NA"),
                    result.entry_ts,
                    result.entry_price,
                    result.outcome.label(),
                    result.reason,
                    format_optional_f64(result.r)
                );
            }
        }
    }
}
/// 提供数量outcomes的集中实现，避免回测策略调用方重复处理相同细节。
fn count_outcomes(results: &[TradeResult]) -> BTreeMap<TradeOutcome, usize> {
    let mut counts = BTreeMap::new();
    for result in results {
        *counts.entry(result.outcome).or_default() += 1;
    }
    counts
}
fn percent(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 / denominator as f64 * 100.0)
}
/// 提供有效开仓summary的集中实现，避免回测策略调用方重复计算过滤后口径。
fn effective_entry_summary(
    raw_events: usize,
    trend_pass: usize,
    entry_pass_before_filters: usize,
    symbol_after: usize,
    trigger_after: usize,
) -> EffectiveEntrySummary {
    EffectiveEntrySummary {
        raw_events,
        trend_pass,
        entry_pass_before_filters,
        symbol_after,
        trigger_after,
        raw_open_rate_pct: percent(trigger_after, raw_events),
        trend_open_rate_pct: percent(trigger_after, trend_pass),
        trigger_keep_pct: percent(trigger_after, entry_pass_before_filters),
    }
}
/// 提供按基础触发器聚合结果的集中实现，避免回测策略调用方重复处理后缀触发变体。
fn summarize_results_by_base_trigger(results: &[TradeResult]) -> Vec<TriggerQualitySummary> {
    summarize_results_by_trigger_key(results, base_trigger_label)
}
/// 提供按完整触发路径聚合结果的集中实现，避免回测策略调用方重复处理完整触发变体。
fn summarize_results_by_exact_trigger(results: &[TradeResult]) -> Vec<TriggerQualitySummary> {
    summarize_results_by_trigger_key(results, exact_trigger_label)
}
fn summarize_results_by_trigger_key<F>(
    results: &[TradeResult],
    trigger_key: F,
) -> Vec<TriggerQualitySummary>
where
    F: Fn(&str) -> String,
{
    let mut grouped: BTreeMap<String, Vec<&TradeResult>> = BTreeMap::new();
    for result in results {
        let label = trigger_key(result.trigger.as_deref().unwrap_or("unknown"));
        grouped.entry(label).or_default().push(result);
    }
    let mut summaries = grouped
        .into_iter()
        .map(|(trigger_label, items)| {
            let trades = items.len();
            let win = items
                .iter()
                .filter(|result| result.outcome == TradeOutcome::Win)
                .count();
            let loss = items
                .iter()
                .filter(|result| result.outcome == TradeOutcome::Loss)
                .count();
            let flat = items
                .iter()
                .filter(|result| result.outcome == TradeOutcome::Flat)
                .count();
            let timeout = items
                .iter()
                .filter(|result| result.outcome == TradeOutcome::Timeout)
                .count();
            let incomplete = items
                .iter()
                .filter(|result| result.outcome == TradeOutcome::Incomplete)
                .count();
            let complete_avg_r = average_complete_r(
                &items
                    .iter()
                    .map(|result| (*result).clone())
                    .collect::<Vec<_>>(),
            );
            TriggerQualitySummary {
                trigger_label,
                trades,
                win,
                loss,
                flat,
                timeout,
                incomplete,
                complete_avg_r,
            }
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .trades
            .cmp(&left.trades)
            .then_with(|| left.trigger_label.cmp(&right.trigger_label))
    });
    summaries
}
fn base_trigger_label(trigger: &str) -> String {
    let normalized = trigger.trim().to_ascii_lowercase();
    normalized
        .split_once('+')
        .map_or(normalized.clone(), |(base, _)| base.to_string())
}
fn exact_trigger_label(trigger: &str) -> String {
    trigger.trim().to_ascii_lowercase()
}
/// 提供averagecompleter的集中实现，避免回测策略调用方重复处理相同细节。
fn average_complete_r(results: &[TradeResult]) -> Option<f64> {
    let mut count = 0;
    let mut sum = 0.0;
    for result in results
        .iter()
        .filter(|result| result.complete)
        .filter_map(|result| result.r)
    {
        count += 1;
        sum += result;
    }
    (count > 0).then_some(sum / count as f64)
}
/// 提供coverage的集中实现，避免回测策略调用方重复处理相同细节。
fn coverage(candles: &[BacktestCandle]) -> String {
    match (candles.first(), candles.last()) {
        (Some(first), Some(last)) => format!("{}:{}-{}", candles.len(), first.ts, last.ts),
        _ => "0".to_string(),
    }
}
/// 提供coveragecomputed的集中实现，避免回测策略调用方重复处理相同细节。
fn coverage_computed(candles: &[ComputedCandle]) -> String {
    match (candles.first(), candles.last()) {
        (Some(first), Some(last)) => {
            format!("{}:{}-{}", candles.len(), first.candle.ts, last.candle.ts)
        }
        _ => "0".to_string(),
    }
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
fn format_counter(counter: &BTreeMap<String, usize>) -> String {
    counter
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\t")
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
fn format_counter_top(counter: &BTreeMap<String, usize>, limit: usize) -> String {
    let mut items = counter.iter().collect::<Vec<_>>();
    items.sort_by(|(left_key, left_value), (right_key, right_value)| {
        right_value
            .cmp(left_value)
            .then_with(|| left_key.cmp(right_key))
    });
    items
        .into_iter()
        .take(limit)
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
fn format_pass_by_symbol(confirmed: &[ConfirmedEvent]) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for signal in confirmed {
        *counts.entry(signal.event.symbol.clone()).or_default() += 1;
    }
    let mut items = counts.iter().collect::<Vec<_>>();
    items.sort_by(|(left_key, left_value), (right_key, right_value)| {
        right_value
            .cmp(left_value)
            .then_with(|| left_key.cmp(right_key))
    });
    items
        .into_iter()
        .map(|(symbol, count)| format!("{symbol}:{count}"))
        .collect::<Vec<_>>()
        .join(";")
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NA".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_entry_summary_uses_trigger_filtered_count_for_real_open_rate() {
        let summary = effective_entry_summary(3026, 1976, 43, 43, 8);
        assert_eq!(summary.raw_events, 3026);
        assert_eq!(summary.trend_pass, 1976);
        assert_eq!(summary.entry_pass_before_filters, 43);
        assert_eq!(summary.symbol_after, 43);
        assert_eq!(summary.trigger_after, 8);
        assert_eq!(summary.raw_open_rate_pct, Some(8.0 / 3026.0 * 100.0));
        assert_eq!(summary.trend_open_rate_pct, Some(8.0 / 1976.0 * 100.0));
        assert_eq!(summary.trigger_keep_pct, Some(8.0 / 43.0 * 100.0));
    }

    #[test]
    fn effective_entry_summary_returns_none_when_denominator_is_zero() {
        let summary = effective_entry_summary(0, 0, 0, 0, 0);
        assert_eq!(summary.raw_open_rate_pct, None);
        assert_eq!(summary.trend_open_rate_pct, None);
        assert_eq!(summary.trigger_keep_pct, None);
    }

    #[test]
    fn summarize_results_by_base_trigger_groups_suffix_variants_together() {
        let results = vec![
            sample_trade_result(
                TradeOutcome::Win,
                Some(1.8),
                "reclaim_ema+fvg_15m_impulse_retrace",
            ),
            sample_trade_result(
                TradeOutcome::Win,
                Some(1.8),
                "reclaim_ema+retest_after_signal+fvg_fallback",
            ),
            sample_trade_result(
                TradeOutcome::Loss,
                Some(-1.0),
                "breakout_previous_high+retest_after_signal+fvg_fallback",
            ),
            sample_trade_result(
                TradeOutcome::Timeout,
                Some(0.6),
                "breakout_previous_high+retest_after_signal+fvg_fallback",
            ),
        ];
        let summaries = summarize_results_by_base_trigger(&results);
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].trigger_label, "breakout_previous_high");
        assert_eq!(summaries[0].trades, 2);
        assert_eq!(summaries[0].loss, 1);
        assert_eq!(summaries[0].timeout, 1);
        assert_eq!(summaries[1].trigger_label, "reclaim_ema");
        assert_eq!(summaries[1].trades, 2);
        assert_eq!(summaries[1].win, 2);
        assert_eq!(summaries[1].complete_avg_r, Some(1.8));
    }

    #[test]
    fn summarize_results_by_exact_trigger_keeps_fvg_and_fallback_separate() {
        let results = vec![
            sample_trade_result(
                TradeOutcome::Win,
                Some(1.8),
                "reclaim_ema+fvg_15m_impulse_retrace",
            ),
            sample_trade_result(
                TradeOutcome::Win,
                Some(1.8),
                "reclaim_ema+retest_after_signal+fvg_fallback",
            ),
        ];
        let summaries = summarize_results_by_exact_trigger(&results);
        assert_eq!(summaries.len(), 2);
        assert_eq!(
            summaries[0].trigger_label,
            "reclaim_ema+fvg_15m_impulse_retrace"
        );
        assert_eq!(
            summaries[1].trigger_label,
            "reclaim_ema+retest_after_signal+fvg_fallback"
        );
    }

    #[test]
    fn base_trigger_label_strips_reentry_suffixes() {
        assert_eq!(
            base_trigger_label("reclaim_ema+retest_after_signal+fvg_fallback"),
            "reclaim_ema"
        );
        assert_eq!(
            base_trigger_label("breakout_previous_high"),
            "breakout_previous_high"
        );
    }

    fn sample_trade_result(outcome: TradeOutcome, r: Option<f64>, trigger: &str) -> TradeResult {
        TradeResult {
            outcome,
            reason: "test".to_string(),
            exit_ts: 0,
            r,
            complete: outcome != TradeOutcome::Incomplete,
            symbol: Some("TEST-USDT-SWAP".to_string()),
            event_id: Some(1),
            detected_at: Some("2026-06-27T00:00:00Z".to_string()),
            entry_ts: 0,
            entry_price: 1.0,
            trigger: Some(trigger.to_string()),
            reentry: None,
        }
    }
}
