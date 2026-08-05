use super::equity::profit_observation::{ProfitObservationDecision, ProfitObservationState};
use super::filtered_volume_baseline::causal_filtered_volume_ratio;
use super::{
    uses_trend_managed_volume_trailing_exit, BacktestCandle, ConfirmedEvent,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection, TradeOutcome, TradeResult,
};
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

/// v12 汇总回放使用的冻结 ATR、目标和双边成本保本参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VolumeAtrTrailing {
    /// 锚点 p 的 ATR14，持仓期间不得重算。
    pub(crate) atr14: f64,
    /// 信号时点最终选中的 ATR 止盈倍数。
    pub(crate) target_atr_multiplier: f64,
    /// 单边手续费与等价滑点合计的小数费率。
    pub(crate) per_side_cost_rate: f64,
}

/// 从 V12 入场证据构造冻结的持仓保护参数；旧版本或证据不完整时保持关闭。
pub(crate) fn volume_atr_trailing_for_signal(
    signal: &ConfirmedEvent,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<VolumeAtrTrailing> {
    if !uses_trend_managed_volume_trailing_exit(&args.paper_outcome_entry_rule_version) {
        return None;
    }
    let evidence = signal.entry_signal_evidence.as_ref()?;
    let target_atr_multiplier = evidence.take_profit_atr_multiplier?;
    let per_side_cost_rate = args
        .backtest_fee_bps_per_side
        .map(|fee_bps| (fee_bps + args.backtest_slippage_bps_per_side) / 10_000.0)
        .filter(|value| value.is_finite() && (0.0..1.0).contains(value))?;
    Some(VolumeAtrTrailing {
        atr14: evidence.atr14,
        target_atr_multiplier,
        per_side_cost_rate,
    })
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
    target_completion_profit_observation: bool,
) -> TradeResult {
    simulate_trade_with_volume_atr_trailing(
        candles,
        entry_idx,
        entry_ts,
        entry_price,
        direction,
        stop_loss_pct,
        target_r,
        horizon_ms,
        profit_protection,
        runner_exit,
        early_exit,
        target_completion_profit_observation,
        None,
    )
}

/// 在保持旧 `simulate_trade` 接口不变的前提下，只为 v12 注入持仓放量阶梯保护。
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_trade_with_volume_atr_trailing(
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
    target_completion_profit_observation: bool,
    volume_atr_trailing: Option<VolumeAtrTrailing>,
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
    let mut accepted_volume_trailing_updates = 0usize;
    let mut profit_observation =
        target_completion_profit_observation.then(ProfitObservationState::default);
    for (idx, candle) in candles.iter().enumerate().skip(entry_idx) {
        if candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let active_stop_price = protected_stop.map(|(price, _)| price).unwrap_or(stop_price);
        let hit_stop = hit_stop(candle, active_stop_price, direction);
        // v7/v8 必须与框架动态止盈的严格穿越语义一致；v3 及旧研究版本保持
        // 原有包含等号的模拟口径，避免改写冻结基线。
        let target_hit = if target_completion_profit_observation {
            hit_target_strict(candle, target_price, direction)
        } else {
            hit_target(candle, target_price, direction)
        };
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
        if idx > entry_idx {
            if let Some(state) = profit_observation.as_mut() {
                let favorable_price = match direction {
                    MarketVelocityTradeDirection::Long => candle.high,
                    MarketVelocityTradeDirection::Short => candle.low,
                    MarketVelocityTradeDirection::Both => candle.close,
                };
                let favorable_r =
                    r_for_price(entry_price, stop_loss_pct, favorable_price, direction);
                let close_r = r_for_price(entry_price, stop_loss_pct, candle.close, direction);
                match state.observe_completed_candle(candle.ts, favorable_r, close_r, target_r) {
                    ProfitObservationDecision::ExitAtClose(evidence) => {
                        return base_trade_result(
                            outcome_for_r(close_r),
                            evidence.exit_reason,
                            candle.ts,
                            Some(close_r),
                            true,
                            entry_ts,
                            entry_price,
                        );
                    }
                    ProfitObservationDecision::UpdateLock(evidence) => {
                        if let Some(lock_r) = evidence.active_lock_r {
                            let lock_price =
                                target_price_for(entry_price, stop_loss_pct, lock_r, direction);
                            if stop_already_crossed(candle.close, lock_price, direction) {
                                return base_trade_result(
                                    outcome_for_r(close_r),
                                    "profit_observation_close_crossed_new_lock",
                                    candle.ts,
                                    Some(close_r),
                                    true,
                                    entry_ts,
                                    entry_price,
                                );
                            }
                            protected_stop = Some((lock_price, lock_r));
                        }
                    }
                    ProfitObservationDecision::None => {}
                }
            }
            if let Some(config) = volume_atr_trailing {
                if let Some((new_stop_price, new_stop_r)) = next_volume_atr_trailing_stop(
                    candles,
                    idx,
                    candle.close,
                    entry_price,
                    stop_loss_pct,
                    direction,
                    protected_stop.map(|(price, _)| price).unwrap_or(stop_price),
                    accepted_volume_trailing_updates,
                    config,
                ) {
                    protected_stop = Some((new_stop_price, new_stop_r));
                    accepted_volume_trailing_updates += 1;
                }
            }
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

#[allow(clippy::too_many_arguments)]
fn next_volume_atr_trailing_stop(
    candles: &[BacktestCandle],
    current_idx: usize,
    completed_close: f64,
    entry_price: f64,
    stop_loss_pct: f64,
    direction: MarketVelocityTradeDirection,
    current_stop_price: f64,
    accepted_updates: usize,
    config: VolumeAtrTrailing,
) -> Option<(f64, f64)> {
    const HOLDING_VOLUME_MIN_RATIO: f64 = 2.5;

    if !config.atr14.is_finite()
        || config.atr14 <= 0.0
        || !config.target_atr_multiplier.is_finite()
        || config.target_atr_multiplier <= 0.0
        || !config.per_side_cost_rate.is_finite()
        || !(0.0..1.0).contains(&config.per_side_cost_rate)
    {
        return None;
    }
    let (volume_ratio, _) = causal_filtered_volume_ratio(
        candles.len(),
        current_idx,
        HOLDING_VOLUME_MIN_RATIO,
        |idx| candles.get(idx).map(|candle| candle.volume),
    )
    .ok()?;
    if volume_ratio < HOLDING_VOLUME_MIN_RATIO {
        return None;
    }

    let candidate = if accepted_updates == 0 {
        true_break_even_price(entry_price, direction, config.per_side_cost_rate)?
    } else {
        let atr_step = accepted_updates as f64;
        // 每个 ATR 台阶必须严格位于冻结目标之前；达到上限后不消耗后续放量事件。
        if atr_step >= config.target_atr_multiplier {
            return None;
        }
        match direction {
            MarketVelocityTradeDirection::Long => entry_price + atr_step * config.atr14,
            MarketVelocityTradeDirection::Short => entry_price - atr_step * config.atr14,
            MarketVelocityTradeDirection::Both => return None,
        }
    };
    let target_price = match direction {
        MarketVelocityTradeDirection::Long => {
            entry_price + config.target_atr_multiplier * config.atr14
        }
        MarketVelocityTradeDirection::Short => {
            entry_price - config.target_atr_multiplier * config.atr14
        }
        MarketVelocityTradeDirection::Both => return None,
    };
    let legal = match direction {
        MarketVelocityTradeDirection::Long => {
            candidate > current_stop_price
                && candidate < target_price
                && completed_close > candidate
        }
        MarketVelocityTradeDirection::Short => {
            candidate < current_stop_price
                && candidate > target_price
                && completed_close < candidate
        }
        MarketVelocityTradeDirection::Both => false,
    };
    legal.then_some((
        candidate,
        r_for_price(entry_price, stop_loss_pct, candidate, direction),
    ))
}

/// 让按该价格平仓后的毛利润恰好覆盖开、平两侧相同费率和滑点。
pub(crate) fn true_break_even_price(
    entry_price: f64,
    direction: MarketVelocityTradeDirection,
    per_side_cost_rate: f64,
) -> Option<f64> {
    if !entry_price.is_finite()
        || entry_price <= 0.0
        || !per_side_cost_rate.is_finite()
        || !(0.0..1.0).contains(&per_side_cost_rate)
    {
        return None;
    }
    match direction {
        MarketVelocityTradeDirection::Long => {
            Some(entry_price * (1.0 + per_side_cost_rate) / (1.0 - per_side_cost_rate))
        }
        MarketVelocityTradeDirection::Short => {
            Some(entry_price * (1.0 - per_side_cost_rate) / (1.0 + per_side_cost_rate))
        }
        MarketVelocityTradeDirection::Both => None,
    }
}

fn stop_already_crossed(
    close_price: f64,
    stop_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => close_price <= stop_price,
        MarketVelocityTradeDirection::Short => close_price >= stop_price,
        MarketVelocityTradeDirection::Both => true,
    }
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
/// 判断价格是否严格穿越动态止盈，保持 v7/v8 与框架成交语义一致。
fn hit_target_strict(
    candle: &BacktestCandle,
    target_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.high > target_price,
        MarketVelocityTradeDirection::Short => candle.low < target_price,
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_15M_MS: i64 = 15 * 60 * 1_000;

    fn candles_with_holding_volume_spike() -> Vec<BacktestCandle> {
        let mut candles = (0..23)
            .map(|idx| BacktestCandle {
                ts: idx as i64 * TEST_15M_MS,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            })
            .collect::<Vec<_>>();
        candles[21] = BacktestCandle {
            ts: 21 * TEST_15M_MS,
            open: 100.0,
            high: 103.0,
            low: 99.0,
            close: 102.0,
            volume: 30.0,
        };
        candles[22] = BacktestCandle {
            ts: 22 * TEST_15M_MS,
            open: 102.0,
            high: 102.0,
            low: 100.0,
            close: 100.1,
            volume: 10.0,
        };
        candles
    }

    fn trailing_config() -> VolumeAtrTrailing {
        VolumeAtrTrailing {
            atr14: 2.0,
            target_atr_multiplier: 2.7,
            per_side_cost_rate: 0.0008,
        }
    }

    #[test]
    fn holding_volume_stop_update_only_becomes_active_on_the_next_candle() {
        let candles = candles_with_holding_volume_spike();

        let result = simulate_trade_with_volume_atr_trailing(
            &candles,
            20,
            20 * TEST_15M_MS,
            100.0,
            MarketVelocityTradeDirection::Long,
            0.03,
            1.8,
            10 * TEST_15M_MS,
            None,
            None,
            None,
            false,
            Some(trailing_config()),
        );

        assert_eq!(result.reason, "profit_protect_stop_hit");
        assert_eq!(result.exit_ts, 22 * TEST_15M_MS);
        assert!(result.r.is_some_and(|value| value > 0.0));
    }

    #[test]
    fn old_stop_has_priority_over_a_same_candle_volume_update() {
        let mut candles = candles_with_holding_volume_spike();
        candles[21].low = 96.5;

        let result = simulate_trade_with_volume_atr_trailing(
            &candles,
            20,
            20 * TEST_15M_MS,
            100.0,
            MarketVelocityTradeDirection::Long,
            0.03,
            1.8,
            10 * TEST_15M_MS,
            None,
            None,
            None,
            false,
            Some(trailing_config()),
        );

        assert_eq!(result.reason, "stop_hit");
        assert_eq!(result.exit_ts, 21 * TEST_15M_MS);
        assert_eq!(result.r, Some(-1.0));
    }

    #[test]
    fn true_break_even_covers_equal_entry_and_exit_cost_rates() {
        let long =
            true_break_even_price(100.0, MarketVelocityTradeDirection::Long, 0.0008).unwrap();
        let short =
            true_break_even_price(100.0, MarketVelocityTradeDirection::Short, 0.0008).unwrap();

        assert!((long - 100.16012810248198).abs() < 1e-12);
        assert!((short - 99.84012789768185).abs() < 1e-12);
    }
}
