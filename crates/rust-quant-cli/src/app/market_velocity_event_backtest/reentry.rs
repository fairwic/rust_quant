use super::{
    early_exit, profit_protection_for_target, runner_exit_for_target, simulate_trade,
    BacktestCandle, ConfirmedEvent, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
    StopReentryDetails, StopReentryMode, TradeOutcome, TradeResult,
};

const ORIGINAL_STOP_R: f64 = -1.0;

pub(super) fn maybe_apply_stop_reentry(
    candles: &[BacktestCandle],
    signal: &ConfirmedEvent,
    original: TradeResult,
    target_r: f64,
    horizon_ms: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> TradeResult {
    if args.stop_reentry_mode == StopReentryMode::Off
        || original.outcome != TradeOutcome::Loss
        || original.reason != "stop_hit"
        || signal.trigger != "breakout_previous_high"
    {
        return original;
    }

    match args.stop_reentry_mode {
        StopReentryMode::Off => original,
        StopReentryMode::BreakoutReclaim => {
            apply_breakout_reclaim(candles, signal, original, target_r, horizon_ms, args)
        }
    }
}

fn apply_breakout_reclaim(
    candles: &[BacktestCandle],
    signal: &ConfirmedEvent,
    original: TradeResult,
    target_r: f64,
    horizon_ms: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> TradeResult {
    let Some((signal_idx, reclaim_price)) =
        first_breakout_reclaim_signal(candles, signal, original.exit_ts, horizon_ms)
    else {
        return original;
    };
    let reentry_idx = signal_idx + 1;
    let Some(reentry_entry) = candles.get(reentry_idx) else {
        return original;
    };

    let reentry = simulate_trade(
        candles,
        reentry_idx,
        reentry_entry.ts,
        reentry_entry.open,
        MarketVelocityTradeDirection::Long,
        args.stop_loss_pct,
        target_r,
        horizon_ms,
        profit_protection_for_target(args, target_r),
        runner_exit_for_target(args, target_r),
        early_exit(args),
    );
    combine_reentry_result(original, reentry, reclaim_price, args.stop_reentry_mode)
}

fn first_breakout_reclaim_signal(
    candles: &[BacktestCandle],
    signal: &ConfirmedEvent,
    original_exit_ts: i64,
    horizon_ms: i64,
) -> Option<(usize, f64)> {
    let confirm_idx = signal.entry_idx.checked_sub(1)?;
    let confirm = candles.get(confirm_idx)?;
    let original_entry = candles.get(signal.entry_idx)?;
    let reclaim_price = confirm.high.max(original_entry.high);
    let reclaim_deadline = signal.entry_ts + horizon_ms;

    candles
        .iter()
        .enumerate()
        .skip(signal.entry_idx + 1)
        .find(|(_, candle)| {
            candle.ts > original_exit_ts
                && candle.ts <= reclaim_deadline
                && candle.close > reclaim_price
                && candle.close > candle.open
        })
        .map(|(idx, _)| (idx, reclaim_price))
}

fn combine_reentry_result(
    original: TradeResult,
    reentry: TradeResult,
    reclaim_price: f64,
    mode: StopReentryMode,
) -> TradeResult {
    let net_r = reentry.r.map(|value| ORIGINAL_STOP_R + value);
    let outcome = match reentry.outcome {
        TradeOutcome::Win if net_r.is_some_and(|value| value > 0.0) => TradeOutcome::Win,
        TradeOutcome::Win | TradeOutcome::Loss | TradeOutcome::Flat => TradeOutcome::Loss,
        TradeOutcome::Timeout => TradeOutcome::Timeout,
        TradeOutcome::Incomplete => TradeOutcome::Incomplete,
    };
    let original_reason = original.reason;
    let original_r = original.r;

    TradeResult {
        outcome,
        reason: format!("stop_reentry_{}", reentry.reason),
        exit_ts: reentry.exit_ts,
        r: net_r,
        complete: reentry.complete,
        symbol: None,
        event_id: None,
        detected_at: None,
        entry_ts: reentry.entry_ts,
        entry_price: reentry.entry_price,
        trigger: None,
        reentry: Some(StopReentryDetails {
            mode,
            original_entry_ts: original.entry_ts,
            original_entry_price: original.entry_price,
            original_exit_ts: original.exit_ts,
            original_reason,
            original_r,
            signal_ts: reentry.entry_ts - super::MS_15M,
            reclaim_price,
            reentry_exit_reason: reentry.reason,
            reentry_r: reentry.r,
        }),
    }
}
