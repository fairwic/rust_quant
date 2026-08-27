use super::model::{
    Candle, Direction, ExitPolicy, Position, SignalFamily, StrictVisualBreakoutResearchVariant,
};

/// V32 只查看突破棒之前三根完成棒，禁止把突破棒自身影线算成承接证据。
pub const PRIOR_WICK_LOOKBACK_BARS: usize = 3;
/// V32/V33 的长影线统一以冻结来源 ATR 归一化，不根据币价或 K 线振幅另调阈值。
pub const MIN_WICK_TO_SOURCE_ATR: f64 = 0.50;
/// V33 的扫单收回必须落在整根 K 线方向终点的 25% 区域内。
pub const OPPOSING_SWEEP_CLOSE_LOCATION: f64 = 0.75;
/// V35 要求扣除预计双边成本后仍至少保留半个初始风险单位。
pub const MIN_REMAINING_NET_REWARD_R: f64 = 0.50;

impl StrictVisualBreakoutResearchVariant {
    /// V32 只改变满足前三根承接影线时的入场时序。
    pub const fn uses_prior_three_wick_direct_chase(self) -> bool {
        matches!(self, Self::V32PriorThreeWickDirectChase)
    }

    /// V33 只在首次合法确认棒上拒绝反方向扫单收回。
    pub const fn rejects_opposing_confirmation_sweep(self) -> bool {
        matches!(self, Self::V33OpposingConfirmationWickGuard)
    }

    /// V34 只在持仓后的盈利放量完成棒上启用净保本。
    pub const fn uses_volume_profit_protection(self) -> bool {
        matches!(self, Self::V34VolumeProfitProtection)
    }

    /// V35 只在实际下一根开盘时检查剩余成本后奖励。
    pub const fn requires_minimum_remaining_net_reward(self) -> bool {
        matches!(self, Self::V35RemainingNetReward50)
    }

    /// V36b 保留视觉边界，只新增横盘开始前 96 根外部极值作为交易突破门槛。
    pub const fn uses_split_trade_breakout_boundary(self) -> bool {
        matches!(self, Self::V36PreRangeExtremeTradeBoundary96)
    }
}

/// 判断突破棒前三根中是否已有方向承接影线；所有输入都止于突破棒收盘。
pub fn prior_three_bars_have_supporting_wick(
    candles: &[Candle],
    breakout_index: usize,
    direction: Direction,
    source_atr: f64,
) -> bool {
    let Some(start) = breakout_index.checked_sub(PRIOR_WICK_LOOKBACK_BARS) else {
        return false;
    };
    if source_atr <= 0.0 {
        return false;
    }
    candles[start..breakout_index].iter().any(|candle| {
        let wick = match direction {
            Direction::Long => candle.open.min(candle.close) - candle.low,
            Direction::Short => candle.high - candle.open.max(candle.close),
        };
        wick.max(0.0) / source_atr >= MIN_WICK_TO_SOURCE_ATR
    })
}

/// 首次确认棒若向突破反方向深扫并收在相反端，说明对手盘已完成强力收回。
pub fn is_opposing_sweep_reclaimed(candle: Candle, direction: Direction, source_atr: f64) -> bool {
    if !candle.is_valid() || source_atr <= 0.0 {
        return false;
    }
    let range = candle.range();
    match direction {
        Direction::Short => {
            let lower_wick = candle.open.min(candle.close) - candle.low;
            lower_wick / source_atr >= MIN_WICK_TO_SOURCE_ATR
                && (candle.close - candle.low) / range >= OPPOSING_SWEEP_CLOSE_LOCATION
        }
        Direction::Long => {
            let upper_wick = candle.high - candle.open.max(candle.close);
            upper_wick / source_atr >= MIN_WICK_TO_SOURCE_ATR
                && (candle.high - candle.close) / range >= OPPOSING_SWEEP_CLOSE_LOCATION
        }
    }
}

/// 以实际入场、冻结目标、最终初始止损和非对称成本计算剩余净收益 R。
pub fn remaining_net_reward_r(
    direction: Direction,
    entry: f64,
    target: f64,
    initial_stop: f64,
    entry_cost_bps: f64,
    exit_cost_bps: f64,
) -> Option<f64> {
    let risk = (entry - initial_stop).abs();
    if risk <= 0.0 || entry <= 0.0 || target <= 0.0 {
        return None;
    }
    let gross_remaining = direction.gross_pnl(entry, target);
    let expected_cost = (entry * entry_cost_bps + target * exit_cost_bps) / 10_000.0;
    Some((gross_remaining - expected_cost) / risk)
}

/// V35 在实际开盘只读取已经构成的持仓计划，不访问之后的价格路径。
pub fn has_minimum_remaining_net_reward(
    position: &Position,
    entry_cost_bps: f64,
    exit_cost_bps: f64,
) -> bool {
    position.target.is_some_and(|target| {
        remaining_net_reward_r(
            position.direction,
            position.entry_price,
            target,
            position.initial_stop,
            entry_cost_bps,
            exit_cost_bps,
        )
        .is_some_and(|reward| reward >= MIN_REMAINING_NET_REWARD_R)
    })
}

/// 返回能覆盖 maker 入场与 taker 退出成本的精度对齐保护价。
pub fn cost_covered_break_even_price(
    direction: Direction,
    entry: f64,
    entry_cost_bps: f64,
    exit_cost_bps: f64,
    tick_size: f64,
) -> Option<f64> {
    if entry <= 0.0 || tick_size <= 0.0 {
        return None;
    }
    let entry_cost = entry_cost_bps / 10_000.0;
    let exit_cost = exit_cost_bps / 10_000.0;
    match direction {
        Direction::Long if exit_cost < 1.0 => Some(round_up(
            entry * (1.0 + entry_cost) / (1.0 - exit_cost),
            tick_size,
        )),
        Direction::Short => Some(round_down(
            entry * (1.0 - entry_cost) / (1.0 + exit_cost),
            tick_size,
        )),
        Direction::Long => None,
    }
}

/// V34 只在入场后的盈利放量完成棒上抬保护位，返回值用于研究审计。
pub fn activate_volume_profit_protection(
    position: &mut Position,
    candle: Candle,
    volume_event: bool,
    entry_cost_bps: f64,
    exit_cost_bps: f64,
    tick_size: f64,
) -> bool {
    if !volume_event
        || candle.timestamp_ms <= position.entry_time_ms
        || !is_strict_visual_position(position)
    {
        return false;
    }
    let Some(net_break_even) = cost_covered_break_even_price(
        position.direction,
        position.entry_price,
        entry_cost_bps,
        exit_cost_bps,
        tick_size,
    ) else {
        return false;
    };
    let profitable = match position.direction {
        Direction::Long => candle.close >= net_break_even,
        Direction::Short => candle.close <= net_break_even,
    };
    if !profitable {
        return false;
    }
    position.stop = match position.direction {
        Direction::Long => position.stop.max(net_break_even),
        Direction::Short => position.stop.min(net_break_even),
    };
    position.exit_policy = ExitPolicy::StrictVisualVolumeProtected;
    position.activated = true;
    true
}

/// 严格视觉交易可能与同棒其他家族合并，包含任一严格视觉标签即属于本研究范围。
pub fn is_strict_visual_position(position: &Position) -> bool {
    position.families.iter().any(|family| {
        matches!(
            family,
            SignalFamily::StrictVisualConsolidationBreakLong
                | SignalFamily::StrictVisualConsolidationBreakShort
        )
    })
}

fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: 0,
            open,
            high,
            low,
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn third_prior_lower_wick_can_enable_long_chase() {
        let candles = vec![
            candle(100.0, 100.2, 98.8, 99.9),
            candle(99.9, 100.1, 99.7, 100.0),
            candle(100.0, 100.2, 99.8, 100.1),
            candle(100.1, 101.0, 100.0, 100.9),
        ];
        assert!(prior_three_bars_have_supporting_wick(
            &candles,
            3,
            Direction::Long,
            2.0,
        ));
    }

    #[test]
    fn third_prior_upper_wick_can_enable_short_chase() {
        let candles = vec![
            candle(100.1, 101.2, 99.9, 100.0),
            candle(100.0, 100.2, 99.8, 99.9),
            candle(99.9, 100.1, 99.7, 99.8),
            candle(99.8, 99.9, 99.0, 99.1),
        ];
        assert!(prior_three_bars_have_supporting_wick(
            &candles,
            3,
            Direction::Short,
            2.0,
        ));
    }

    #[test]
    fn reclaimed_lower_sweep_rejects_short_confirmation() {
        let confirmation = candle(100.0, 100.2, 98.0, 99.9);
        assert!(is_opposing_sweep_reclaimed(
            confirmation,
            Direction::Short,
            2.0,
        ));
        assert!(!is_opposing_sweep_reclaimed(
            confirmation,
            Direction::Long,
            2.0,
        ));
    }

    #[test]
    fn reclaimed_upper_sweep_rejects_long_confirmation() {
        let confirmation = candle(100.0, 102.0, 99.8, 100.1);
        assert!(is_opposing_sweep_reclaimed(
            confirmation,
            Direction::Long,
            2.0,
        ));
        assert!(!is_opposing_sweep_reclaimed(
            confirmation,
            Direction::Short,
            2.0,
        ));
    }

    #[test]
    fn asymmetric_costs_are_included_in_reward_and_break_even() {
        let reward = remaining_net_reward_r(Direction::Long, 100.0, 102.0, 98.0, 3.0, 6.0);
        assert!(reward.is_some_and(|value| (value - 0.9544).abs() < 1e-5));
        let long = cost_covered_break_even_price(Direction::Long, 100.0, 3.0, 6.0, 0.01);
        let short = cost_covered_break_even_price(Direction::Short, 100.0, 3.0, 6.0, 0.01);
        assert!(long.is_some_and(|price| (price - 100.10).abs() < 1e-9));
        assert!(short.is_some_and(|price| (price - 99.91).abs() < 1e-9));
    }
}
