use super::{BacktestCandle, MarketVelocityTradeDirection, TradeOutcome, TradeResult};
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfitProtection {
    /// activateafterr，用于行情、K 线或市场扫描。
    pub activate_after_r: f64,
    /// 止损r，用于行情、K 线或市场扫描。
    pub stop_r: f64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunnerExit {
    /// targetr，用于行情、K 线或市场扫描。
    pub target_r: f64,
    /// fraction，用于行情、K 线或市场扫描。
    pub fraction: f64,
    /// 止损r，用于行情、K 线或市场扫描。
    pub stop_r: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EarlyExit {
    /// no收益K 线，用于行情、K 线或市场扫描。
    pub no_profit_candles: usize,
}
/// 执行模拟交易步骤，串起回测策略需要的状态推进和错误处理。
pub fn simulate_trade(
    candles: &[BacktestCandle],
    entry_idx: usize,
    entry_ts: i64,
    entry_price: f64,
    direction: MarketVelocityTradeDirection,
    stop_loss_pct: f64,
    target_r: f64,
    horizon_ms: i64,
    profit_protection: Option<ProfitProtection>,
    runner_exit: Option<RunnerExit>,
    early_exit: Option<EarlyExit>,
) -> TradeResult {
    let stop_price = stop_price_for(entry_price, stop_loss_pct, direction);
    let target_price = target_price_for(entry_price, stop_loss_pct, target_r, direction);
    let protection_trigger_price = profit_protection.map(|protection| {
        target_price_for(
            entry_price,
            stop_loss_pct,
            protection.activate_after_r,
            direction,
        )
    });
    let protection_stop_price = profit_protection.map(|protection| {
        target_price_for(entry_price, stop_loss_pct, protection.stop_r, direction)
    });
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
        let hit_stop = hit_stop(candle, active_stop_price, direction);
        let target_hit = hit_target(candle, target_price, direction);
        if hit_stop && target_hit {
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
        if target_hit {
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
                    direction,
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
            && protection_trigger_price
                .is_some_and(|trigger_price| hit_target(candle, trigger_price, direction))
        {
            if let (Some(stop_price), Some(protection)) = (protection_stop_price, profit_protection)
            {
                protected_stop = Some((stop_price, protection.stop_r));
            }
        }
        if early_exit.is_some_and(|exit| {
            idx > entry_idx
                && idx >= entry_idx + exit.no_profit_candles
                && no_profit_close(candle.close, entry_price, direction)
        }) {
            let r = r_for_price(entry_price, stop_loss_pct, candle.close, direction);
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
        let r = last_seen
            .map(|candle| r_for_price(entry_price, stop_loss_pct, candle.close, direction));
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
    let r =
        last_seen.map(|candle| r_for_price(entry_price, stop_loss_pct, candle.close, direction));
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
/// 执行模拟Runner交易步骤，串起回测策略需要的状态推进和错误处理。
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
    direction: MarketVelocityTradeDirection,
) -> TradeResult {
    let first_profit_r = first_target_r * (1.0 - runner.fraction);
    let runner_target_price =
        target_price_for(entry_price, stop_loss_pct, runner.target_r, direction);
    let runner_stop_price = target_price_for(entry_price, stop_loss_pct, runner.stop_r, direction);
    let horizon_end = entry_ts + horizon_ms;
    let max_ts = candles.last().map(|candle| candle.ts).unwrap_or_default();
    let mut last_seen: Option<&BacktestCandle> = None;
    for candle in candles.iter().skip(target_hit_idx + 1) {
        if candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let hit_stop = hit_stop(candle, runner_stop_price, direction);
        let hit_target = hit_target(candle, runner_target_price, direction);
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
        last_seen.map(|candle| r_for_price(entry_price, stop_loss_pct, candle.close, direction));
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
/// 停止 回测与策略研究 后台流程，确保退出时不留下未释放状态。
fn stop_price_for(
    entry_price: f64,
    stop_loss_pct: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => entry_price * (1.0 - stop_loss_pct),
        MarketVelocityTradeDirection::Short => entry_price * (1.0 + stop_loss_pct),
        MarketVelocityTradeDirection::Both => entry_price,
    }
}
/// 提供目标价格for的集中实现，避免回测策略调用方重复处理相同细节。
fn target_price_for(
    entry_price: f64,
    stop_loss_pct: f64,
    target_r: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => entry_price * (1.0 + stop_loss_pct * target_r),
        MarketVelocityTradeDirection::Short => entry_price * (1.0 - stop_loss_pct * target_r),
        MarketVelocityTradeDirection::Both => entry_price,
    }
}
/// 提供hit止损的集中实现，避免回测策略调用方重复处理相同细节。
fn hit_stop(
    candle: &BacktestCandle,
    stop_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.low <= stop_price,
        MarketVelocityTradeDirection::Short => candle.high >= stop_price,
        MarketVelocityTradeDirection::Both => false,
    }
}
/// 提供hit目标的集中实现，避免回测策略调用方重复处理相同细节。
fn hit_target(
    candle: &BacktestCandle,
    target_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.high >= target_price,
        MarketVelocityTradeDirection::Short => candle.low <= target_price,
        MarketVelocityTradeDirection::Both => false,
    }
}
/// 提供no盈利平仓的集中实现，避免回测策略调用方重复处理相同细节。
fn no_profit_close(
    close_price: f64,
    entry_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => close_price <= entry_price,
        MarketVelocityTradeDirection::Short => close_price >= entry_price,
        MarketVelocityTradeDirection::Both => false,
    }
}
/// 提供rfor价格的集中实现，避免回测策略调用方重复处理相同细节。
fn r_for_price(
    entry_price: f64,
    stop_loss_pct: f64,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => (price - entry_price) / (entry_price * stop_loss_pct),
        MarketVelocityTradeDirection::Short => {
            (entry_price - price) / (entry_price * stop_loss_pct)
        }
        MarketVelocityTradeDirection::Both => 0.0,
    }
}
/// 执行 Runner交易结果步骤，串起回测策略需要的状态推进和错误处理。
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
/// 提供结果forr的集中实现，避免回测策略调用方重复处理相同细节。
fn outcome_for_r(r: f64) -> TradeOutcome {
    if r > 0.0 {
        TradeOutcome::Win
    } else if r < 0.0 {
        TradeOutcome::Loss
    } else {
        TradeOutcome::Flat
    }
}
/// 提供protected止损结果的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 提供base交易结果的集中实现，避免回测策略调用方重复处理相同细节。
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
        target_r: None,
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
