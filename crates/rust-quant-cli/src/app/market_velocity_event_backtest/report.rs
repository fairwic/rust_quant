use super::{
    summarize_target, BacktestCandle, BacktestDataSet, ComputedCandle, ConfirmedEvent,
    EvaluationReport, MarketVelocityEventBacktestArgs, TradeOutcome, TradeResult,
};
use std::collections::{BTreeMap, HashMap};

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

fn coverage(candles: &[BacktestCandle]) -> String {
    match (candles.first(), candles.last()) {
        (Some(first), Some(last)) => format!("{}:{}-{}", candles.len(), first.ts, last.ts),
        _ => "0".to_string(),
    }
}

fn coverage_computed(candles: &[ComputedCandle]) -> String {
    match (candles.first(), candles.last()) {
        (Some(first), Some(last)) => {
            format!("{}:{}-{}", candles.len(), first.candle.ts, last.candle.ts)
        }
        _ => "0".to_string(),
    }
}

fn format_counter(counter: &BTreeMap<String, usize>) -> String {
    counter
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\t")
}

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

fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NA".to_string())
}
