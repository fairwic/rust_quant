use super::{ActivePosition, Args, CandidateTrade, PricePathAnomaly};
use anyhow::{bail, Result};
use std::collections::BTreeSet;

pub(super) const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1_000;
pub(super) const FUNDING_INTERVAL_MS: i64 = 8 * 60 * 60 * 1_000;

/// 一根已确认 4H K 线，用于回放持仓期间可见的收盘和保守棒内价格。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CandleMark {
    /// K 线开始时间，Unix 毫秒时间戳。
    pub(super) ts: i64,
    /// K 线最高价。
    pub(super) high: f64,
    /// K 线最低价。
    pub(super) low: f64,
    /// K 线收盘价。
    pub(super) close: f64,
}

/// 某个已确认 4H 时点结算后的共享账户收盘权益。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct EquityPoint {
    pub(super) ts: i64,
    pub(super) equity: f64,
}

/// 4H 行情路径审计结果，同时给出收盘盯市值和棒内保守上界。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MarkToMarketAudit {
    pub(super) close_mark_max_drawdown: f64,
    pub(super) intrabar_conservative_max_drawdown: f64,
    pub(super) intrabar_conservative_max_drawdown_amount: f64,
    pub(super) close_equity_curve: Vec<EquityPoint>,
    pub(super) fully_covered_positions: usize,
    pub(super) missing_4h_bars: usize,
    pub(super) entry_price_outside_bar_count: usize,
    pub(super) exit_price_outside_bar_count: usize,
    pub(super) price_path_anomalies: Vec<PricePathAnomaly>,
}

/// 计算持仓跨越的 UTC 8 小时资金费率结算点数量，不在入场瞬间重复扣费。
pub(super) fn funding_interval_count(open_ts: i64, end_ts: i64) -> usize {
    if end_ts <= open_ts {
        return 0;
    }
    (end_ts.div_euclid(FUNDING_INTERVAL_MS) - open_ts.div_euclid(FUNDING_INTERVAL_MS)).max(0)
        as usize
}

/// 使用全部接纳仓位的真实 4H 路径构建共享账户盯市曲线，并审计路径完整性。
pub(super) fn calculate_mark_to_market_audit(
    positions: &[ActivePosition],
    args: Args,
) -> Result<MarkToMarketAudit> {
    if positions.is_empty() {
        return Ok(MarkToMarketAudit {
            close_mark_max_drawdown: 0.0,
            intrabar_conservative_max_drawdown: 0.0,
            intrabar_conservative_max_drawdown_amount: 0.0,
            close_equity_curve: Vec::new(),
            fully_covered_positions: 0,
            missing_4h_bars: 0,
            entry_price_outside_bar_count: 0,
            exit_price_outside_bar_count: 0,
            price_path_anomalies: Vec::new(),
        });
    }

    let mut timestamps = BTreeSet::<i64>::new();
    let mut fully_covered_positions = 0_usize;
    let mut missing_4h_bars = 0_usize;
    let mut entry_price_outside_bar_count = 0_usize;
    let mut exit_price_outside_bar_count = 0_usize;
    let mut price_path_anomalies = Vec::<PricePathAnomaly>::new();
    for position in positions {
        let trade = &position.trade;
        timestamps.insert(trade.open_ts);
        timestamps.insert(trade.close_ts);
        timestamps.extend(trade.marks.iter().map(|mark| mark.ts));

        let duration = trade.close_ts - trade.open_ts;
        let expected = duration.div_euclid(FOUR_HOURS_MS).max(0) as usize + 1;
        let aligned = duration.rem_euclid(FOUR_HOURS_MS) == 0;
        let endpoint_covered = trade
            .marks
            .first()
            .is_some_and(|mark| mark.ts == trade.open_ts)
            && trade
                .marks
                .last()
                .is_some_and(|mark| mark.ts == trade.close_ts);
        missing_4h_bars += expected.saturating_sub(trade.marks.len());
        if aligned && endpoint_covered && trade.marks.len() == expected {
            fully_covered_positions += 1;
        }
        if let Some(mark) = trade
            .marks
            .first()
            .filter(|mark| !price_is_inside_bar(trade.open_price, **mark))
        {
            entry_price_outside_bar_count += 1;
            price_path_anomalies.push(PricePathAnomaly {
                detail_id: trade.detail_id,
                symbol: trade.symbol.clone(),
                phase: "entry",
                ts: trade.open_ts,
                execution_price: trade.open_price,
                bar_low: mark.low,
                bar_high: mark.high,
            });
        } else if trade.marks.first().is_none() {
            entry_price_outside_bar_count += 1;
        }
        if let Some(mark) = trade
            .marks
            .last()
            .filter(|mark| !price_is_inside_bar(trade.close_price, **mark))
        {
            exit_price_outside_bar_count += 1;
            price_path_anomalies.push(PricePathAnomaly {
                detail_id: trade.detail_id,
                symbol: trade.symbol.clone(),
                phase: "exit",
                ts: trade.close_ts,
                execution_price: trade.close_price,
                bar_low: mark.low,
                bar_high: mark.high,
            });
        } else if trade.marks.last().is_none() {
            exit_price_outside_bar_count += 1;
        }
    }

    let mut close_peak = args.initial_equity;
    let mut close_max_drawdown = 0.0_f64;
    let mut close_max_drawdown_amount = 0.0_f64;
    let mut conservative_peak = args.initial_equity;
    let mut conservative_max_drawdown = 0.0_f64;
    let mut conservative_max_drawdown_amount = 0.0_f64;
    let mut close_equity_curve = Vec::with_capacity(timestamps.len());
    for ts in timestamps {
        // 先观察旧仓位在该根退出棒内的路径，再结算旧仓位并接纳同刻新信号。
        for after_settlement in [false, true] {
            let close_equity =
                portfolio_marked_equity(positions, args, ts, after_settlement, false)?;
            update_drawdown(
                close_equity,
                &mut close_peak,
                &mut close_max_drawdown,
                &mut close_max_drawdown_amount,
            );
            // 保守曲线共享真实收盘峰值，再用同一棒不利极值估算谷值，
            // 因而其最大回撤不会反常地小于纯收盘曲线。
            update_drawdown(
                close_equity,
                &mut conservative_peak,
                &mut conservative_max_drawdown,
                &mut conservative_max_drawdown_amount,
            );
            if after_settlement {
                close_equity_curve.push(EquityPoint {
                    ts,
                    equity: close_equity,
                });
            }
            let intrabar_equity =
                portfolio_marked_equity(positions, args, ts, after_settlement, true)?;
            update_drawdown(
                intrabar_equity,
                &mut conservative_peak,
                &mut conservative_max_drawdown,
                &mut conservative_max_drawdown_amount,
            );
        }
    }

    Ok(MarkToMarketAudit {
        close_mark_max_drawdown: close_max_drawdown,
        intrabar_conservative_max_drawdown: conservative_max_drawdown,
        intrabar_conservative_max_drawdown_amount: conservative_max_drawdown_amount,
        close_equity_curve,
        fully_covered_positions,
        missing_4h_bars,
        entry_price_outside_bar_count,
        exit_price_outside_bar_count,
        price_path_anomalies,
    })
}

/// 计算一个事件时点的账户清算价值；`after_settlement` 区分同刻平仓前后。
fn portfolio_marked_equity(
    positions: &[ActivePosition],
    args: Args,
    ts: i64,
    after_settlement: bool,
    intrabar_conservative: bool,
) -> Result<f64> {
    let mut equity = args.initial_equity;
    for position in positions {
        let trade = &position.trade;
        let is_realized = if after_settlement {
            trade.close_ts <= ts
        } else {
            trade.close_ts < ts
        };
        if is_realized {
            equity += position.entry_equity * trade.normalized_return;
            continue;
        }
        let is_active = if after_settlement {
            trade.open_ts <= ts && trade.close_ts > ts
        } else {
            trade.open_ts < ts && trade.close_ts >= ts
        };
        if !is_active {
            continue;
        }
        let mark_price = mark_price_at(trade, ts, intrabar_conservative);
        equity += position.entry_equity * normalized_mark_return(trade, mark_price, ts, args)?;
    }
    Ok(equity)
}

/// 选择 4H 收盘盯市价或该棒对持仓最不利的高低价；缺棒时回退最近已知收盘。
fn mark_price_at(trade: &CandidateTrade, ts: i64, intrabar_conservative: bool) -> f64 {
    if ts <= trade.open_ts {
        return trade.open_price;
    }
    if ts == trade.close_ts && !intrabar_conservative {
        return trade.close_price;
    }
    let mark = trade
        .marks
        .iter()
        .find(|mark| mark.ts == ts)
        .or_else(|| trade.marks.iter().rev().find(|mark| mark.ts < ts));
    let Some(mark) = mark else {
        return trade.open_price;
    };
    if !intrabar_conservative {
        return mark.close;
    }
    match trade.side.as_str() {
        "long" => {
            if ts == trade.close_ts {
                mark.low.min(trade.close_price)
            } else {
                mark.low
            }
        }
        "short" => {
            if ts == trade.close_ts {
                mark.high.max(trade.close_price)
            } else {
                mark.high
            }
        }
        _ => trade.open_price,
    }
}

/// 将指定价格处的毛浮盈亏、往返交易成本和累计资金费率转换为源账户收益率。
fn normalized_mark_return(
    trade: &CandidateTrade,
    mark_price: f64,
    mark_ts: i64,
    args: Args,
) -> Result<f64> {
    if !mark_price.is_finite() || mark_price <= 0.0 {
        bail!("invalid mark price for {} at {mark_ts}", trade.symbol);
    }
    let gross_profit = match trade.side.as_str() {
        "long" => (mark_price - trade.open_price) * trade.quantity,
        "short" => (trade.open_price - mark_price) * trade.quantity,
        _ => bail!("unsupported trade side: {}", trade.side),
    };
    let cost_rate = trade.base_fee_rate + args.extra_slippage_bps / 10_000.0;
    let trading_cost = trade.quantity * (trade.open_price + mark_price) * cost_rate;
    let funding_cost = trade.quantity
        * trade.open_price
        * funding_interval_count(trade.open_ts, mark_ts) as f64
        * args.funding_bps_per_8h
        / 10_000.0;
    Ok((gross_profit - trading_cost - funding_cost) / trade.source_entry_equity)
}

/// 允许极小浮点误差后判断执行价是否可由该根 K 线成交。
fn price_is_inside_bar(price: f64, mark: CandleMark) -> bool {
    let tolerance = price.abs().max(mark.high.abs()).max(mark.low.abs()) * 1e-9;
    price >= mark.low - tolerance && price <= mark.high + tolerance
}

/// 用一个新权益点更新峰值和最大回撤。
fn update_drawdown(
    equity: f64,
    peak: &mut f64,
    max_drawdown: &mut f64,
    max_drawdown_amount: &mut f64,
) {
    *peak = peak.max(equity);
    if *peak > 0.0 {
        let drawdown_amount = *peak - equity;
        *max_drawdown = max_drawdown.max(drawdown_amount / *peak);
        *max_drawdown_amount = max_drawdown_amount.max(drawdown_amount);
    }
}
