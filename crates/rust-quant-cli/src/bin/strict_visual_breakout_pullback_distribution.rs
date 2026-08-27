//! 突破后回踩分布诊断（只读统计，不改策略、不下单、不写库）。
//!
//! 目的：为「用回踩挂限价单替代追突破」的可行性提供数据。对 60 币种冻结宇宙，
//! 复用严格视觉横盘突破信号，测量每个确认信号在突破后的回踩深度分布，
//! 以及在若干候选挂单深度成交后走向量度目标 / 止损的结果，量化：
//!   1) 踏空（突破后不回踩，挂单永不成交）比例与其错失利润；
//!   2) 各回踩桶占比（浅 / 中 / 深 / 穿破失败）；
//!   3) 在挂单深度成交后的胜率与平均 R，判断降成本是否被踏空损失抵消。
//!
//! 归一化坐标（以被突破边界为 0、突破棒收盘为 1.0）：
//!   pullback = (extreme - breakout_close) / (boundary - breakout_close)
//! 其中空头 boundary = range.lower、extreme 取回踩窗口内最高 high；
//! 多头 boundary = range.upper、extreme 取最低 low。pullback 越大越贴近边界。

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_frozen_top60_from_quant_core, Candle, Direction, ParityRuleVersion,
    StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal, StrictVisualLongEntryState,
};
use serde::Serialize;

/// 回踩追踪窗口：突破后观察多少根完成棒判断挂单能否成交（用户确认 N=8）。
const PULLBACK_WINDOW_BARS: usize = 8;

/// 候选挂单深度（pullback 归一化档位）：在该回踩比例处假设限价单成交。
// 联合模拟改用浅档：runner 不回踩，maker 单只挂在突破价附近（depth=0 即突破收盘价），
// 略偏边界一点（0.1/0.2）以提高成交率，只赚「免吃 taker 价差」这一层，不等深度回踩。
const CANDIDATE_FILL_DEPTHS: [f64; 3] = [0.0, 0.1, 0.2];

/// Maker 单边费率，2bps。限价挂单入场 + 出场共两边，从实现 R 中按 risk 折算扣除。
const MAKER_COST_RATE_PER_SIDE: f64 = 0.0002;

/// 上海时区相对 UTC 的偏移（+8 小时），用于按上海自然月分桶做多窗验证。
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1000;

/// 把毫秒时间戳映射到上海时区的自然月标签（YYYY-MM），与主回测的月度归因口径一致。
fn shanghai_month(timestamp_ms: i64) -> String {
    let Some(time) = Utc
        .timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
    else {
        return "invalid".to_owned();
    };
    format!("{:04}-{:02}", time.year(), time.month())
}

/// 单个确认信号的回踩测量结果。
#[derive(Debug, Clone)]
struct PullbackSample {
    /// 交易对，便于按币种归因。
    symbol: String,
    /// 信号棒时间戳（毫秒），用于按上海月分桶做多窗验证。
    signal_time_ms: i64,
    /// 冻结方向；当前分析主看空头，多头一并输出。
    direction: Direction,
    /// 回踩窗口内达到的最深回踩（pullback 归一化值，可能 > 1.0 表示穿破失败）。
    max_pullback: f64,
    /// 追高幅度：入场（信号棒收盘）相对被突破边界的方向距离，单位为源 ATR 倍数。
    entry_chase_atr: f64,
    /// 各候选挂单深度的成交后结果；与 CANDIDATE_FILL_DEPTHS 一一对应。
    fills: Vec<LimitFillOutcome>,
}

/// 在某个回踩深度挂限价单后的模拟结果。
#[derive(Debug, Clone, Copy)]
struct LimitFillOutcome {
    /// 挂单深度（pullback 归一化档位）。
    depth: f64,
    /// 回踩窗口内是否触及该挂单价（成交）。
    filled: bool,
    /// 成交后先到量度目标记 true，先触止损记 false；未成交或无目标为 None。
    hit_target: Option<bool>,
    /// 成交入场价到目标 / 止损的实现 R 倍数，已扣两边 maker 成本；未成交为 None。
    realized_r: Option<f64>,
    /// 踏空：挂单未成交，但若按突破棒收盘价入场本会先到目标（错失的盈利机会）。
    missed_profit: bool,
}

/// 把 pullback 归一化档位换算成绝对挂单价：depth=0 贴突破棒收盘，depth=1 贴被突破边界。
fn limit_price_at_depth(signal: &StrictVisualBreakoutSignal, depth: f64) -> f64 {
    let boundary = match signal.direction {
        Direction::Long => signal.range.upper,
        Direction::Short => signal.range.lower,
    };
    signal.breakout_close + (boundary - signal.breakout_close) * depth
}

/// 计算某深度挂单在回踩窗口内是否成交，并从成交棒起追踪到量度目标 / 止损。
/// 只读后续 K 线，不产生任何订单；无目标价或无止损价的信号返回 filled 但结果 None。
fn simulate_limit_fill(
    candles: &[Candle],
    signal: &StrictVisualBreakoutSignal,
    depth: f64,
) -> LimitFillOutcome {
    let limit_price = limit_price_at_depth(signal, depth);
    let signal_index = signal.signal_index;
    let mut fill_index: Option<usize> = None;
    // 回踩窗口：信号棒之后的 N 根内，价格是否回抽触及挂单价。
    for offset in 1..=PULLBACK_WINDOW_BARS {
        let idx = signal_index + offset;
        let Some(candle) = candles.get(idx) else {
            break;
        };
        let touched = match signal.direction {
            // 空头挂单在突破价上方，价格反弹 high 触及即成交。
            Direction::Short => candle.high >= limit_price,
            // 多头挂单在突破价下方，价格回落 low 触及即成交。
            Direction::Long => candle.low <= limit_price,
        };
        if touched {
            fill_index = Some(idx);
            break;
        }
    }

    let Some(fill_index) = fill_index else {
        // 未成交：判断若按突破棒收盘价追入是否本会先到目标，量化踏空损失。
        let missed_profit = breakout_entry_hits_target_first(candles, signal);
        return LimitFillOutcome {
            depth,
            filled: false,
            hit_target: None,
            realized_r: None,
            missed_profit,
        };
    };

    let (Some(target), Some(stop)) = (
        signal.measured_move_target_price,
        signal.breakout_candle_extreme_stop_price,
    ) else {
        return LimitFillOutcome {
            depth,
            filled: true,
            hit_target: None,
            realized_r: None,
            missed_profit: false,
        };
    };

    let risk = (limit_price - stop).abs();
    if risk <= 0.0 {
        return LimitFillOutcome {
            depth,
            filled: true,
            hit_target: None,
            realized_r: None,
            missed_profit: false,
        };
    }

    // 两边 maker 成本折算成 R：(入场价 + 出场价) × 费率 / risk，近似用 limit_price 两次。
    let cost_r = (limit_price * MAKER_COST_RATE_PER_SIDE * 2.0) / risk;

    // 从成交棒起逐棒判定先触目标还是先触止损；同棒都触保守记为止损。
    for candle in candles.iter().skip(fill_index) {
        let (hit_stop, hit_target) = match signal.direction {
            Direction::Short => (candle.high >= stop, candle.low <= target),
            Direction::Long => (candle.low <= stop, candle.high >= target),
        };
        if hit_stop {
            return LimitFillOutcome {
                depth,
                filled: true,
                hit_target: Some(false),
                realized_r: Some(-1.0 - cost_r),
                missed_profit: false,
            };
        }
        if hit_target {
            let reward = (target - limit_price).abs();
            return LimitFillOutcome {
                depth,
                filled: true,
                hit_target: Some(true),
                realized_r: Some(reward / risk - cost_r),
                missed_profit: false,
            };
        }
    }

    // 窗口末仍未触发：按未决处理，不计入胜负，realized_r 记 None。
    LimitFillOutcome {
        depth,
        filled: true,
        hit_target: None,
        realized_r: None,
        missed_profit: false,
    }
}

/// 踏空判定：假设在突破棒收盘价直接追入，从信号棒下一根起先到目标还是先触止损。
/// 只用于量化「挂单没成交、但追入本会盈利」的错失利润，不参与实际挂单结果。
fn breakout_entry_hits_target_first(
    candles: &[Candle],
    signal: &StrictVisualBreakoutSignal,
) -> bool {
    let (Some(target), Some(stop)) = (
        signal.measured_move_target_price,
        signal.breakout_candle_extreme_stop_price,
    ) else {
        return false;
    };
    for candle in candles.iter().skip(signal.signal_index + 1) {
        let (hit_stop, hit_target) = match signal.direction {
            Direction::Short => (candle.high >= stop, candle.low <= target),
            Direction::Long => (candle.low <= stop, candle.high >= target),
        };
        if hit_stop {
            return false;
        }
        if hit_target {
            return true;
        }
    }
    false
}

/// 计算回踩窗口内最深回踩幅度，用源 ATR 归一化（固定尺度，不受突破棒收盘离边界远近影响）。
/// 返回「回踩方向反弹了多少个源 ATR」：空头看反弹高点，多头看回落低点。
fn max_pullback_atr(candles: &[Candle], signal: &StrictVisualBreakoutSignal) -> f64 {
    if signal.source_atr <= 0.0 {
        return 0.0;
    }
    let mut max_pullback = 0.0_f64;
    for offset in 1..=PULLBACK_WINDOW_BARS {
        let Some(candle) = candles.get(signal.signal_index + offset) else {
            break;
        };
        // 空头回踩看反弹高点，多头回踩看回落低点；回踩幅度以突破棒收盘为基准。
        let pullback = match signal.direction {
            Direction::Short => candle.high - signal.breakout_close,
            Direction::Long => signal.breakout_close - candle.low,
        } / signal.source_atr;
        if pullback > max_pullback {
            max_pullback = pullback;
        }
    }
    max_pullback
}

/// 对单个交易对逐棒复放严格视觉横盘突破信号，收集每个确认信号的回踩样本。
fn collect_symbol_samples(
    symbol: &str,
    candles: &[Candle],
    tick_size: f64,
    rule_version: ParityRuleVersion,
    variant: StrictVisualBreakoutResearchVariant,
    window_start_ms: i64,
) -> Vec<PullbackSample> {
    let indicators = compute_indicators(candles, rule_version);
    let mut state = StrictVisualLongEntryState::default();
    let mut samples = Vec::new();

    for index in 0..candles.len() {
        let Some(point) = indicators.get(index) else {
            continue;
        };
        let take_profit_atr = point
            .filtered_volume_ratio
            .and_then(rust_quant_cli::app::tradingview_velocity_parity::volume_take_profit_atr);
        let event = state.update(
            candles,
            index,
            tick_size,
            point.atr14,
            point.volume_event,
            point.filtered_volume_ratio,
            take_profit_atr,
            variant,
        );
        let Some(signal) = event.and_then(|event| event.entry_signal()) else {
            continue;
        };
        // 只统计评价窗口内确认的信号，与主回测口径一致。
        if candles[signal.signal_index].timestamp_ms < window_start_ms {
            continue;
        }
        // 本 bin 直接调 update，绕过了 signals.rs 的空头越界 filter，这里手动补上，
        // 确保「只对通过 V17 温和突破门槛的信号挂 maker 单」的联合模拟口径一致。
        if signal.direction == Direction::Short {
            if let Some(max_excess_atr) = variant.soft_breakout_short_max_excess_atr() {
                if signal.source_atr <= 0.0
                    || signal.breakout_excess / signal.source_atr > max_excess_atr
                {
                    continue;
                }
            }
        }
        let boundary = match signal.direction {
            Direction::Long => signal.range.upper,
            Direction::Short => signal.range.lower,
        };
        let entry_chase_atr = if signal.source_atr > 0.0 {
            (signal.breakout_close - boundary).abs() / signal.source_atr
        } else {
            0.0
        };
        let fills = CANDIDATE_FILL_DEPTHS
            .iter()
            .map(|&depth| simulate_limit_fill(candles, &signal, depth))
            .collect();
        samples.push(PullbackSample {
            symbol: symbol.to_owned(),
            signal_time_ms: candles[signal.signal_index].timestamp_ms,
            direction: signal.direction,
            max_pullback: max_pullback_atr(candles, &signal),
            entry_chase_atr,
            fills,
        });
    }
    samples
}

/// 回踩深度分桶计数。
#[derive(Debug, Default, Serialize)]
struct PullbackBuckets {
    /// 几乎不回踩（反弹 < 0.1 ATR），挂单基本无法成交，踏空风险最高。
    no_pullback: u64,
    /// 浅回踩 [0.1, 0.5) ATR。
    shallow: u64,
    /// 中回踩 [0.5, 1.0) ATR，理想挂单区。
    medium: u64,
    /// 深回踩 [1.0, 2.0) ATR，回抽较深但仍可能延续。
    deep: u64,
    /// 大幅反弹 >= 2.0 ATR，突破多半失败。
    pierced: u64,
}

/// 单个挂单深度的成交与结果聚合。
#[derive(Debug, Serialize)]
struct FillDepthSummary {
    /// 挂单深度档位。
    depth: f64,
    /// 触及挂单价的信号数（成交数）。
    filled: u64,
    /// 成交率 = filled / 信号总数。
    fill_rate_percent: f64,
    /// 成交且在窗口内产生胜负判定的样本数。
    decided: u64,
    /// 成交并先到目标的样本数。
    target_hits: u64,
    /// 成交后胜率 = target_hits / decided。
    win_rate_percent: f64,
    /// 成交样本的平均实现 R（仅计入已决样本，已扣 maker 成本）。
    average_r: f64,
    /// 已决样本的累计实现 R（已扣 maker 成本）。
    total_r: f64,
    /// 踏空数：挂单未成交、但追突破本会盈利的样本数。
    missed_profit: u64,
    /// 踏空率 = missed_profit / 未成交数。
    missed_profit_rate_percent: f64,
}

/// 单方向（多 / 空）的完整回踩诊断汇总。
#[derive(Debug, Serialize)]
struct DirectionSummary {
    /// 方向标签。
    direction: String,
    /// 确认信号总数。
    signals: u64,
    /// 回踩深度分桶。
    buckets: PullbackBuckets,
    /// 追高幅度均值（源 ATR 倍数）。
    average_chase_atr: f64,
    /// 各候选挂单深度的成交与结果。
    fill_depths: Vec<FillDepthSummary>,
}

/// 把某方向的样本聚合成 DirectionSummary。
fn summarize(direction: Direction, samples: &[PullbackSample]) -> DirectionSummary {
    let mut buckets = PullbackBuckets::default();
    let mut chase_sum = 0.0_f64;
    for sample in samples {
        chase_sum += sample.entry_chase_atr;
        // 分桶按源 ATR 倍数：回踩反弹了多少个 ATR。
        let pullback = sample.max_pullback;
        if pullback < 0.1 {
            buckets.no_pullback += 1;
        } else if pullback < 0.5 {
            buckets.shallow += 1;
        } else if pullback < 1.0 {
            buckets.medium += 1;
        } else if pullback < 2.0 {
            buckets.deep += 1;
        } else {
            buckets.pierced += 1;
        }
    }

    let signals = samples.len() as u64;
    let fill_depths = CANDIDATE_FILL_DEPTHS
        .iter()
        .enumerate()
        .map(|(slot, &depth)| {
            let mut filled = 0_u64;
            let mut decided = 0_u64;
            let mut target_hits = 0_u64;
            let mut total_r = 0.0_f64;
            let mut not_filled = 0_u64;
            let mut missed_profit = 0_u64;
            for sample in samples {
                let outcome = sample.fills[slot];
                if outcome.filled {
                    filled += 1;
                } else {
                    not_filled += 1;
                    if outcome.missed_profit {
                        missed_profit += 1;
                    }
                }
                if let (Some(hit), Some(r)) = (outcome.hit_target, outcome.realized_r) {
                    decided += 1;
                    if hit {
                        target_hits += 1;
                    }
                    total_r += r;
                }
            }
            FillDepthSummary {
                depth,
                filled,
                fill_rate_percent: pct(filled, signals),
                decided,
                target_hits,
                win_rate_percent: pct(target_hits, decided),
                average_r: if decided > 0 {
                    total_r / decided as f64
                } else {
                    0.0
                },
                total_r,
                missed_profit,
                missed_profit_rate_percent: pct(missed_profit, not_filled),
            }
        })
        .collect();

    DirectionSummary {
        direction: format!("{direction:?}"),
        signals,
        buckets,
        average_chase_atr: if signals > 0 {
            chase_sum / signals as f64
        } else {
            0.0
        },
        fill_depths,
    }
}

/// 百分比小工具，分母为 0 时返回 0。
fn pct(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

/// 顶层报告：变体标识 + 多空两方向汇总。
#[derive(Debug, Serialize)]
struct Report {
    /// 使用的突破棒门槛变体 slug。
    variant: String,
    /// 回踩追踪窗口棒数。
    pullback_window_bars: usize,
    /// 候选挂单深度档位。
    candidate_fill_depths: Vec<f64>,
    /// 空头汇总（当前分析重点）。
    short: DirectionSummary,
    /// 多头汇总（镜像对照）。
    long: DirectionSummary,
    /// 空头按上海自然月分桶的多窗验证：每月独立重算各挂单深度指标，
    /// 判断 maker 薄边际是否跨月稳定，还是集中在个别月份。
    short_by_month: Vec<MonthlySummary>,
}

/// 单个上海自然月的空头 maker 汇总，用于多窗稳定性判断。
#[derive(Debug, Serialize)]
struct MonthlySummary {
    /// 上海自然月标签（YYYY-MM）。
    month: String,
    /// 该月空头信号数。
    signals: u64,
    /// 该月各挂单深度的成交后指标（与 CANDIDATE_FILL_DEPTHS 一一对应）。
    fill_depths: Vec<FillDepthSummary>,
}

/// 按上海自然月对空头样本分桶，逐月复用方向汇总逻辑重算 maker 指标。
fn summarize_by_month(short_samples: &[PullbackSample]) -> Vec<MonthlySummary> {
    let mut by_month: BTreeMap<String, Vec<PullbackSample>> = BTreeMap::new();
    for sample in short_samples {
        by_month
            .entry(shanghai_month(sample.signal_time_ms))
            .or_default()
            .push(sample.clone());
    }
    by_month
        .into_iter()
        .map(|(month, samples)| {
            let summary = summarize(Direction::Short, &samples);
            MonthlySummary {
                month,
                signals: summary.signals,
                fill_depths: summary.fill_depths,
            }
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    // 联合模拟：变体设 V17，信号已被温和突破越界门槛筛过，只对这批挂 maker 单。
    let variant = StrictVisualBreakoutResearchVariant::V17SoftBreakoutShort;
    let rule_version = ParityRuleVersion::CandidateV20;

    let dataset = load_frozen_top60_from_quant_core()
        .await
        .context("加载 60 币种冻结宇宙失败")?;

    let mut all_samples: Vec<PullbackSample> = Vec::new();
    for symbol in &dataset.symbols {
        let samples = collect_symbol_samples(
            &symbol.symbol,
            &symbol.candles,
            symbol.tick_size,
            rule_version,
            variant,
            dataset.window_start_ms,
        );
        all_samples.extend(samples);
    }

    // 按币种的信号计数，便于快速看分布是否被单一币种主导。
    let mut per_symbol: BTreeMap<String, u64> = BTreeMap::new();
    for sample in &all_samples {
        *per_symbol.entry(sample.symbol.clone()).or_default() += 1;
    }

    let short_samples: Vec<PullbackSample> = all_samples
        .iter()
        .filter(|s| s.direction == Direction::Short)
        .cloned()
        .collect();
    let long_samples: Vec<PullbackSample> = all_samples
        .iter()
        .filter(|s| s.direction == Direction::Long)
        .cloned()
        .collect();

    let report = Report {
        variant: variant.slug().to_owned(),
        pullback_window_bars: PULLBACK_WINDOW_BARS,
        candidate_fill_depths: CANDIDATE_FILL_DEPTHS.to_vec(),
        short: summarize(Direction::Short, &short_samples),
        long: summarize(Direction::Long, &long_samples),
        short_by_month: summarize_by_month(&short_samples),
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
