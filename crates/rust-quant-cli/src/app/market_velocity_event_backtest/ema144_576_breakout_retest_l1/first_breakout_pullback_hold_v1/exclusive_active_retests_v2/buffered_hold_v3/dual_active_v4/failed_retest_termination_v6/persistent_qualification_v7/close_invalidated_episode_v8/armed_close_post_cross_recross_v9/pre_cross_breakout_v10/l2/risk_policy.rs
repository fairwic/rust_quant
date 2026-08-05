//! L2 入场时冻结初始止损和目标价的研究政策。

use super::{L2Direction, PER_SIDE_COST_RATE, STOP_LOSS_PCT, TARGET_R};

/// L2 入场时冻结初始风险价的来源。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in super::super) enum InitialRiskPolicy {
    /// V10/V11 冻结行为：按真实入场价固定 4% 止损。
    FixedFourPercent,
    /// 使用信号收盘已知的 EMA144，并在失效方向外留固定 ATR14 缓冲。
    SignalEma144AtrBuffer(f64),
}

/// 入场时冻结的目标收益口径。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in super::super) enum TargetRiskPolicy {
    /// V10～V14 冻结行为：目标为成本前固定 0.52R。
    FixedGrossR,
    /// 反解目标价，使按冻结双边成本结算后的净收益等于指定 R。
    NetAfterCostsR(f64),
}

/// 下一根开盘与结构风险已知后，是否允许该机会进入真实成交回放。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in super::super) enum EntryRiskGatePolicy {
    /// V10～V15 冻结行为：只要风险价格合法，不增加成本占 R 门禁。
    AllowAnyPositiveRisk,
    /// 止损成交的开平双边成本不得超过指定初始风险倍数。
    MaxStopCostR(f64),
}

/// 按 4% 初始风险和 0.52R 固定目标生成多空镜像保护价。
pub(super) fn risk_prices(
    entry_price: f64,
    direction: L2Direction,
) -> Result<(f64, f64), &'static str> {
    if !entry_price.is_finite() || entry_price <= 0.0 {
        return Err("next_entry_open_invalid");
    }
    let (stop, target) = match direction {
        L2Direction::Long => (
            entry_price * (1.0 - STOP_LOSS_PCT),
            entry_price * (1.0 + STOP_LOSS_PCT * TARGET_R),
        ),
        L2Direction::Short => (
            entry_price * (1.0 + STOP_LOSS_PCT),
            entry_price * (1.0 - STOP_LOSS_PCT * TARGET_R),
        ),
    };
    if stop <= 0.0 || target <= 0.0 || !stop.is_finite() || !target.is_finite() {
        return Err("risk_or_target_price_invalid");
    }
    Ok((stop, target))
}

/// 按当前研究版本冻结的唯一风险政策生成止损与同一 0.52R 目标。
pub(super) fn risk_prices_for_candidate(
    entry_price: f64,
    direction: L2Direction,
    signal_ema144: f64,
    signal_atr14: f64,
    initial_risk_policy: InitialRiskPolicy,
    target_risk_policy: TargetRiskPolicy,
) -> Result<(f64, f64), &'static str> {
    if initial_risk_policy == InitialRiskPolicy::FixedFourPercent
        && target_risk_policy == TargetRiskPolicy::FixedGrossR
    {
        return risk_prices(entry_price, direction);
    }
    if !entry_price.is_finite() || entry_price <= 0.0 {
        return Err("next_entry_open_invalid");
    }
    let stop = match initial_risk_policy {
        InitialRiskPolicy::FixedFourPercent => match direction {
            L2Direction::Long => entry_price * (1.0 - STOP_LOSS_PCT),
            L2Direction::Short => entry_price * (1.0 + STOP_LOSS_PCT),
        },
        InitialRiskPolicy::SignalEma144AtrBuffer(buffer_atr) => {
            if !signal_ema144.is_finite() || signal_ema144 <= 0.0 {
                return Err("signal_ema144_invalid_for_structural_stop");
            }
            if !signal_atr14.is_finite()
                || signal_atr14 <= 0.0
                || !buffer_atr.is_finite()
                || buffer_atr < 0.0
            {
                return Err("signal_atr_or_buffer_invalid_for_structural_stop");
            }
            match direction {
                L2Direction::Long => signal_ema144 - buffer_atr * signal_atr14,
                L2Direction::Short => signal_ema144 + buffer_atr * signal_atr14,
            }
        }
    };
    let risk = directional_initial_risk(entry_price, stop, direction)?;
    let target = target_price_for_policy(entry_price, risk, direction, target_risk_policy)?;
    if stop <= 0.0 || target <= 0.0 || !stop.is_finite() || !target.is_finite() {
        return Err("risk_or_target_price_invalid");
    }
    Ok((stop, target))
}

/// 目标命中时按同一成本模型结算；净 R 版本必须反解价格，不能把费用再当作固定 R 扣减。
pub(in super::super) fn target_price_for_policy(
    entry: f64,
    risk: f64,
    direction: L2Direction,
    policy: TargetRiskPolicy,
) -> Result<f64, &'static str> {
    if !entry.is_finite() || entry <= 0.0 || !risk.is_finite() || risk <= 0.0 {
        return Err("entry_or_risk_invalid_for_target");
    }
    let target = match policy {
        TargetRiskPolicy::FixedGrossR => match direction {
            L2Direction::Long => entry + TARGET_R * risk,
            L2Direction::Short => entry - TARGET_R * risk,
        },
        TargetRiskPolicy::NetAfterCostsR(net_r) => {
            if !net_r.is_finite() || net_r <= 0.0 {
                return Err("net_target_r_invalid");
            }
            match direction {
                L2Direction::Long => {
                    (entry * (1.0 + PER_SIDE_COST_RATE) + net_r * risk) / (1.0 - PER_SIDE_COST_RATE)
                }
                L2Direction::Short => {
                    (entry * (1.0 - PER_SIDE_COST_RATE) - net_r * risk) / (1.0 + PER_SIDE_COST_RATE)
                }
            }
        }
    };
    if !target.is_finite() || target <= 0.0 {
        return Err("risk_or_target_price_invalid");
    }
    Ok(target)
}

/// 按冻结成本模型计算“若结构止损成交”的成本 R；该值在下单前已经完全可知。
pub(in super::super) fn stop_cost_r_for_prices(
    entry: f64,
    stop: f64,
    risk: f64,
) -> Result<f64, &'static str> {
    if !entry.is_finite()
        || entry <= 0.0
        || !stop.is_finite()
        || stop <= 0.0
        || !risk.is_finite()
        || risk <= 0.0
    {
        return Err("entry_stop_or_risk_invalid_for_cost_gate");
    }
    let stop_cost_r = (entry + stop) * PER_SIDE_COST_RATE / risk;
    if !stop_cost_r.is_finite() || stop_cost_r < 0.0 {
        return Err("stop_cost_r_invalid");
    }
    Ok(stop_cost_r)
}

/// 成本门禁在真实成交前执行；被拒绝的机会不能消费“首笔真实成交”资格。
pub(super) fn validate_entry_risk_gate(
    entry: f64,
    stop: f64,
    risk: f64,
    policy: EntryRiskGatePolicy,
) -> Result<(), &'static str> {
    let EntryRiskGatePolicy::MaxStopCostR(max_stop_cost_r) = policy else {
        return Ok(());
    };
    if !max_stop_cost_r.is_finite() || max_stop_cost_r <= 0.0 {
        return Err("max_stop_cost_r_invalid");
    }
    let stop_cost_r = stop_cost_r_for_prices(entry, stop, risk)?;
    if stop_cost_r > max_stop_cost_r + 1e-12 {
        return Err("stop_cost_r_above_max");
    }
    Ok(())
}

/// 固定百分比版本保留原乘法公式，结构版本才使用入场到止损的实际距离。
pub(super) fn initial_risk_amount(
    entry: f64,
    stop: f64,
    direction: L2Direction,
    policy: InitialRiskPolicy,
) -> Result<f64, &'static str> {
    match policy {
        InitialRiskPolicy::FixedFourPercent => {
            let risk = entry * STOP_LOSS_PCT;
            if risk.is_finite() && risk > 0.0 {
                Ok(risk)
            } else {
                Err("initial_risk_invalid")
            }
        }
        InitialRiskPolicy::SignalEma144AtrBuffer(_) => {
            directional_initial_risk(entry, stop, direction)
        }
    }
}

/// 把多空镜像止损统一换算为正的初始价格风险，R 在入场后不得改变。
fn directional_initial_risk(
    entry: f64,
    stop: f64,
    direction: L2Direction,
) -> Result<f64, &'static str> {
    let risk = match direction {
        L2Direction::Long => entry - stop,
        L2Direction::Short => stop - entry,
    };
    if !risk.is_finite() || risk <= 0.0 {
        return Err("initial_stop_not_beyond_entry");
    }
    Ok(risk)
}
