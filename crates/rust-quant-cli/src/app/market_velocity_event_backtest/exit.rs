use super::{BacktestCandle, TradeOutcome, TradeResult};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfitProtection {
    pub activate_after_r: f64,
    pub stop_r: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunnerExit {
    pub target_r: f64,
    pub fraction: f64,
    pub stop_r: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EarlyExit {
    pub no_profit_candles: usize,
}

pub fn simulate_trade(
    candles: &[BacktestCandle],
    entry_idx: usize,
    entry_ts: i64,
    entry_price: f64,
    stop_loss_pct: f64,
    target_r: f64,
    horizon_ms: i64,
    profit_protection: Option<ProfitProtection>,
    runner_exit: Option<RunnerExit>,
    early_exit: Option<EarlyExit>,
) -> TradeResult {
    let stop_price = entry_price * (1.0 - stop_loss_pct);
    let target_price = entry_price * (1.0 + stop_loss_pct * target_r);
    let protection_trigger_price = profit_protection
        .map(|protection| entry_price * (1.0 + stop_loss_pct * protection.activate_after_r));
    let protection_stop_price =
        profit_protection.map(|protection| entry_price * (1.0 + stop_loss_pct * protection.stop_r));
    let horizon_end = entry_ts + horizon_ms;
    let max_ts = candles.last().map(|candle| candle.ts).unwrap_or_default();
    let mut last_seen: Option<&BacktestCandle> = None;
    let mut protected_stop: Option<(f64, f64)> = None;

    for (idx, candle) in candles.iter().enumerate().skip(entry_idx) {
        if candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let active_stop_price = protected_stop.map(|(price, _)| price).unwrap_or(stop_price);
        let hit_stop = candle.low <= active_stop_price;
        let hit_target = candle.high >= target_price;
        if hit_stop && hit_target {
            return protected_stop
                .map(|(_, stop_r)| protected_stop_result(candle.ts, stop_r, entry_ts, entry_price))
                .unwrap_or_else(|| {
                    base_trade_result(
                        TradeOutcome::Loss,
                        "both_hit_stop_first",
                        candle.ts,
                        Some(-1.0),
                        true,
                        entry_ts,
                        entry_price,
                    )
                });
        }
        if hit_target {
            if let Some(runner) = runner_exit {
                return simulate_runner_trade(
                    candles,
                    idx,
                    candle.ts,
                    entry_ts,
                    entry_price,
                    stop_loss_pct,
                    target_r,
                    horizon_ms,
                    runner,
                );
            }
            return base_trade_result(
                TradeOutcome::Win,
                "target_hit",
                candle.ts,
                Some(target_r),
                true,
                entry_ts,
                entry_price,
            );
        }
        if hit_stop {
            return protected_stop
                .map(|(_, stop_r)| protected_stop_result(candle.ts, stop_r, entry_ts, entry_price))
                .unwrap_or_else(|| {
                    base_trade_result(
                        TradeOutcome::Loss,
                        "stop_hit",
                        candle.ts,
                        Some(-1.0),
                        true,
                        entry_ts,
                        entry_price,
                    )
                });
        }
        if protected_stop.is_none()
            && protection_trigger_price.is_some_and(|trigger_price| candle.high >= trigger_price)
        {
            if let (Some(stop_price), Some(protection)) = (protection_stop_price, profit_protection)
            {
                protected_stop = Some((stop_price, protection.stop_r));
            }
        }
        if early_exit.is_some_and(|exit| {
            idx > entry_idx
                && idx >= entry_idx + exit.no_profit_candles
                && candle.close <= entry_price
        }) {
            let r = (candle.close - entry_price) / (entry_price * stop_loss_pct);
            return base_trade_result(
                outcome_for_r(r),
                "early_exit_no_profit",
                candle.ts,
                Some(r),
                true,
                entry_ts,
                entry_price,
            );
        }
    }

    if max_ts >= horizon_end {
        let r =
            last_seen.map(|candle| (candle.close - entry_price) / (entry_price * stop_loss_pct));
        return base_trade_result(
            TradeOutcome::Timeout,
            "horizon_timeout",
            horizon_end,
            r,
            true,
            entry_ts,
            entry_price,
        );
    }

    let r = last_seen.map(|candle| (candle.close - entry_price) / (entry_price * stop_loss_pct));
    base_trade_result(
        TradeOutcome::Incomplete,
        "forward_data_incomplete",
        last_seen.map(|candle| candle.ts).unwrap_or(entry_ts),
        r,
        false,
        entry_ts,
        entry_price,
    )
}

fn simulate_runner_trade(
    candles: &[BacktestCandle],
    target_hit_idx: usize,
    target_hit_ts: i64,
    entry_ts: i64,
    entry_price: f64,
    stop_loss_pct: f64,
    first_target_r: f64,
    horizon_ms: i64,
    runner: RunnerExit,
) -> TradeResult {
    let first_profit_r = first_target_r * (1.0 - runner.fraction);
    let runner_target_price = entry_price * (1.0 + stop_loss_pct * runner.target_r);
    let runner_stop_price = entry_price * (1.0 + stop_loss_pct * runner.stop_r);
    let horizon_end = entry_ts + horizon_ms;
    let max_ts = candles.last().map(|candle| candle.ts).unwrap_or_default();
    let mut last_seen: Option<&BacktestCandle> = None;

    for candle in candles.iter().skip(target_hit_idx + 1) {
        if candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let hit_stop = candle.low <= runner_stop_price;
        let hit_target = candle.high >= runner_target_price;
        if hit_stop && hit_target {
            return runner_trade_result(
                "runner_stop_first",
                candle.ts,
                first_profit_r + runner.fraction * runner.stop_r,
                true,
                entry_ts,
                entry_price,
            );
        }
        if hit_target {
            return runner_trade_result(
                "runner_target_hit",
                candle.ts,
                first_profit_r + runner.fraction * runner.target_r,
                true,
                entry_ts,
                entry_price,
            );
        }
        if hit_stop {
            return runner_trade_result(
                "runner_stop_hit",
                candle.ts,
                first_profit_r + runner.fraction * runner.stop_r,
                true,
                entry_ts,
                entry_price,
            );
        }
    }

    let runner_close_r =
        last_seen.map(|candle| (candle.close - entry_price) / (entry_price * stop_loss_pct));
    if max_ts >= horizon_end {
        let r = runner_close_r
            .map(|close_r| first_profit_r + runner.fraction * close_r)
            .unwrap_or(first_profit_r);
        return base_trade_result(
            TradeOutcome::Timeout,
            "runner_horizon_timeout",
            horizon_end,
            Some(r),
            true,
            entry_ts,
            entry_price,
        );
    }

    let r = runner_close_r
        .map(|close_r| first_profit_r + runner.fraction * close_r)
        .unwrap_or(first_profit_r);
    base_trade_result(
        TradeOutcome::Incomplete,
        "runner_forward_data_incomplete",
        last_seen.map(|candle| candle.ts).unwrap_or(target_hit_ts),
        Some(r),
        false,
        entry_ts,
        entry_price,
    )
}

fn runner_trade_result(
    reason: &str,
    exit_ts: i64,
    r: f64,
    complete: bool,
    entry_ts: i64,
    entry_price: f64,
) -> TradeResult {
    base_trade_result(
        outcome_for_r(r),
        reason,
        exit_ts,
        Some(r),
        complete,
        entry_ts,
        entry_price,
    )
}

fn outcome_for_r(r: f64) -> TradeOutcome {
    if r > 0.0 {
        TradeOutcome::Win
    } else if r < 0.0 {
        TradeOutcome::Loss
    } else {
        TradeOutcome::Flat
    }
}

fn protected_stop_result(
    exit_ts: i64,
    stop_r: f64,
    entry_ts: i64,
    entry_price: f64,
) -> TradeResult {
    base_trade_result(
        outcome_for_r(stop_r),
        "profit_protect_stop_hit",
        exit_ts,
        Some(stop_r),
        true,
        entry_ts,
        entry_price,
    )
}

fn base_trade_result(
    outcome: TradeOutcome,
    reason: &str,
    exit_ts: i64,
    r: Option<f64>,
    complete: bool,
    entry_ts: i64,
    entry_price: f64,
) -> TradeResult {
    TradeResult {
        outcome,
        reason: reason.to_string(),
        exit_ts,
        r,
        complete,
        symbol: None,
        event_id: None,
        detected_at: None,
        entry_ts,
        entry_price,
        trigger: None,
        reentry: None,
    }
}
