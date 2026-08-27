//! V20 严格横盘突破做空 —— 真实 maker 挂单成交回测（只读事后建模，不改 engine、不下单、不写库）。
//!
//! 背景：V20（三门 excess≤0.39 + close_vs_ema144≥-4.65 + body_ratio≤0.915）在「次开盘市价成交 +
//! 全 maker 费率」假设下 maker net +72R。但那假设每笔都能在入场价成交。真实 maker 做空要在当前价
//! 上方挂卖单，突破后价格立刻下跌（最赚的行情）时挂单永远够不到 → 系统性错过 runner（逆向选择）。
//!
//! 本 bin 复用 pullback bin 的独立复放链路（加载冻结 K 线 + 直接驱动 StrictVisualLongEntryState
//! 状态机产信号 + 挂单触及模拟），回答两问：① 真实挂单成交率；② 成交批 vs 错过批的收益差。
//!
//! 已确认口径（用户拍板）：
//!   - 入场挂单 maker 0.02%/side，出场（止损/目标）按 taker 0.05%/side（真实止损多为市价）。
//!   - 挂单未成交：realized_r 记 0，但单独量化「若市价入场本可赚多少 R」暴露逆向选择丢掉的 runner。
//!   - 只测 depth=0（贴突破棒收盘价，最易成交）。
//!   - 出场扫描从 fill_index+1 起（成交棒本身不判出场，避免虚记同棒胜）。
//!
//! 因 bin 绕过 signals.rs 直接调 state.update，三道门（excess→ema144→body）在 collect 循环里
//! 手动补齐，顺序与 signals.rs 完全一致，确保信号集合等同真实 V20 run。

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_frozen_top60_from_quant_core, Candle, Direction, ParityRuleVersion,
    StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal, StrictVisualLongEntryState,
};
use serde::Serialize;

/// 回踩追踪窗口：突破后观察多少根完成棒判断挂单能否成交（与 pullback bin 一致 N=8）。
const PULLBACK_WINDOW_BARS: usize = 8;

/// 只测贴突破棒收盘价的挂单深度（depth=0 最易成交）。
const FILL_DEPTH: f64 = 0.0;

/// Maker 单边费率 2bps：仅用于入场挂单成交那一边。
const MAKER_COST_RATE_PER_SIDE: f64 = 0.0002;

/// Taker 单边费率 5bps：用于止损/目标出场那一边（真实止损多为市价吃单）。
const TAKER_COST_RATE_PER_SIDE: f64 = 0.0005;

/// 出场扫描是否跳过成交棒本身（true=从 fill_index+1 起判目标/止损，避免虚记同棒胜）。
const EXIT_SCAN_SKIPS_FILL_BAR: bool = true;

/// 之前「次开盘市价 + 全 maker」假设下 V20 short 的 maker net_r，作对比基线。
const MARKET_MAKER_BASELINE_R: f64 = 72.0;

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

/// 单个 V20 空头信号的挂单成交样本。
#[derive(Debug, Clone)]
struct ShortSample {
    /// 交易对，便于按币种归因。
    symbol: String,
    /// 入场棒时间戳（信号棒下一根开盘），用于按上海月分桶；无下一根时回落到信号棒时间。
    entry_time_ms: i64,
    /// 挂单成交模拟结果。
    outcome: LimitFillOutcome,
}

/// depth=0 挂单成交模拟结果。
#[derive(Debug, Clone, Copy)]
struct LimitFillOutcome {
    /// 回踩窗口内是否触及挂单价（成交）。
    filled: bool,
    /// 成交且先到目标记 true，先触止损记 false；未成交或窗口末未决为 None。
    hit_target: Option<bool>,
    /// 实现 R：成交且已决为带成本的实际 R；未成交记 Some(0.0)；成交但未决为 None。
    realized_r: Option<f64>,
    /// 逆向选择量化：仅未成交时有值——若按突破棒收盘价市价入场本会实现的 R（gross 无费，
    /// 对齐 +72R 基线口径）。正值即「挂单没成交却本可盈利」的错失 runner。
    missed_r: Option<f64>,
}

/// 把 pullback 归一化档位换算成绝对挂单价：depth=0 贴突破棒收盘，depth=1 贴被突破边界。
fn limit_price_at_depth(signal: &StrictVisualBreakoutSignal, depth: f64) -> f64 {
    let boundary = match signal.direction {
        Direction::Long => signal.range.upper,
        Direction::Short => signal.range.lower,
    };
    signal.breakout_close + (boundary - signal.breakout_close) * depth
}

/// depth=0 挂单在回踩窗口内是否成交，成交后追踪到目标/止损并按「入 maker 出 taker」扣成本。
/// 未成交则量化「若市价追突破本会实现多少 R」。无目标/无止损的信号视为未决。
fn simulate_limit_fill(
    candles: &[Candle],
    signal: &StrictVisualBreakoutSignal,
) -> LimitFillOutcome {
    let limit_price = limit_price_at_depth(signal, FILL_DEPTH);
    let signal_index = signal.signal_index;
    let mut fill_index: Option<usize> = None;
    // 回踩窗口：信号棒之后的 N 根内，价格是否回抽触及挂单价。
    for offset in 1..=PULLBACK_WINDOW_BARS {
        let idx = signal_index + offset;
        let Some(candle) = candles.get(idx) else {
            break;
        };
        // 空头挂单在突破价上方，价格反弹 high 触及即成交。
        let touched = match signal.direction {
            Direction::Short => candle.high >= limit_price,
            Direction::Long => candle.low <= limit_price,
        };
        if touched {
            fill_index = Some(idx);
            break;
        }
    }

    let Some(fill_index) = fill_index else {
        // 未成交：realized 记 0（可直接汇总），并量化逆向选择丢掉的市价追突破 R。
        return LimitFillOutcome {
            filled: false,
            hit_target: None,
            realized_r: Some(0.0),
            missed_r: breakout_entry_realized_r(candles, signal),
        };
    };

    let (Some(target), Some(stop)) = (
        signal.measured_move_target_price,
        signal.breakout_candle_extreme_stop_price,
    ) else {
        return LimitFillOutcome {
            filled: true,
            hit_target: None,
            realized_r: None,
            missed_r: None,
        };
    };

    let risk = (limit_price - stop).abs();
    if risk <= 0.0 {
        return LimitFillOutcome {
            filled: true,
            hit_target: None,
            realized_r: None,
            missed_r: None,
        };
    }

    // 出场扫描起点：跳过成交棒本身，避免同棒既触挂单又触目标/止损虚记胜负。
    let scan_start = if EXIT_SCAN_SKIPS_FILL_BAR {
        fill_index + 1
    } else {
        fill_index
    };
    for candle in candles.iter().skip(scan_start) {
        let (hit_stop, hit_target) = match signal.direction {
            Direction::Short => (candle.high >= stop, candle.low <= target),
            Direction::Long => (candle.low <= stop, candle.high >= target),
        };
        // 同棒都触保守记止损。成本口径：入场挂单 maker、出场市价 taker。
        if hit_stop {
            let cost_r =
                (limit_price * MAKER_COST_RATE_PER_SIDE + stop * TAKER_COST_RATE_PER_SIDE) / risk;
            return LimitFillOutcome {
                filled: true,
                hit_target: Some(false),
                realized_r: Some(-1.0 - cost_r),
                missed_r: None,
            };
        }
        if hit_target {
            let reward = (target - limit_price).abs();
            let cost_r =
                (limit_price * MAKER_COST_RATE_PER_SIDE + target * TAKER_COST_RATE_PER_SIDE) / risk;
            return LimitFillOutcome {
                filled: true,
                hit_target: Some(true),
                realized_r: Some(reward / risk - cost_r),
                missed_r: None,
            };
        }
    }

    // 窗口后仍未触发：未决，不计胜负。
    LimitFillOutcome {
        filled: true,
        hit_target: None,
        realized_r: None,
        missed_r: None,
    }
}

/// 逆向选择量化：假设在突破棒收盘价直接市价追入，从信号棒下一根起先到目标还是先触止损，
/// 返回实现 R（gross 无费，对齐 +72R 基线口径）；无目标/无止损或 risk 非正返回 None。
/// 止损记 -1.0，目标记 reward/risk，同棒都触保守记止损。
fn breakout_entry_realized_r(
    candles: &[Candle],
    signal: &StrictVisualBreakoutSignal,
) -> Option<f64> {
    let (target, stop) = (
        signal.measured_move_target_price?,
        signal.breakout_candle_extreme_stop_price?,
    );
    let entry = signal.breakout_close;
    let risk = (entry - stop).abs();
    if risk <= 0.0 {
        return None;
    }
    for candle in candles.iter().skip(signal.signal_index + 1) {
        let (hit_stop, hit_target) = match signal.direction {
            Direction::Short => (candle.high >= stop, candle.low <= target),
            Direction::Long => (candle.low <= stop, candle.high >= target),
        };
        if hit_stop {
            return Some(-1.0);
        }
        if hit_target {
            return Some((target - entry).abs() / risk);
        }
    }
    None
}

/// 对单个交易对逐棒复放，收集通过 V20 三门的空头信号的挂单成交样本。
/// 三门顺序 excess→ema144→body 与 signals.rs 完全一致，让本 bin 信号集合等同真实 V20 run。
fn collect_symbol_samples(
    symbol: &str,
    candles: &[Candle],
    tick_size: f64,
    rule_version: ParityRuleVersion,
    variant: StrictVisualBreakoutResearchVariant,
    window_start_ms: i64,
) -> Vec<ShortSample> {
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
        // 本 bin 只研究 V20 空头；多头一律跳过。
        if signal.direction != Direction::Short {
            continue;
        }
        // 门 1 excess：越界幅度 / 源 ATR ≤ 上限。
        if let Some(max_excess_atr) = variant.soft_breakout_short_max_excess_atr() {
            if signal.source_atr <= 0.0
                || signal.breakout_excess / signal.source_atr > max_excess_atr
            {
                continue;
            }
        }
        // 门 2 ema144：(突破收盘 - EMA144)/ATR ≥ 下限；ema144/atr 缺失则保守放行（同 signals.rs）。
        if let Some(min_close_vs_ema144_atr) = variant.soft_breakout_short_min_close_vs_ema144_atr()
        {
            if let (Some(ema144), Some(atr)) = (point.ema144, point.atr14) {
                if atr > 0.0 && (signal.breakout_close - ema144) / atr < min_close_vs_ema144_atr {
                    continue;
                }
            }
        }
        // 门 3 body：|突破收盘 - 突破开盘| / 源 ATR ≤ 上限。
        if let Some(max_body_ratio) = variant.soft_breakout_short_max_body_ratio() {
            if signal.source_atr <= 0.0
                || (signal.breakout_close - signal.breakout_open).abs() / signal.source_atr
                    > max_body_ratio
            {
                continue;
            }
        }
        // 入场棒 = 信号棒下一根；无下一根时回落到信号棒时间戳。
        let entry_time_ms = candles
            .get(signal.signal_index + 1)
            .map(|c| c.timestamp_ms)
            .unwrap_or(candles[signal.signal_index].timestamp_ms);
        samples.push(ShortSample {
            symbol: symbol.to_owned(),
            entry_time_ms,
            outcome: simulate_limit_fill(candles, &signal),
        });
    }
    samples
}

/// 成交批聚合。所有 R 已按「入 maker 出 taker」扣成本。
#[derive(Debug, Default, Serialize)]
struct FilledSummary {
    /// 信号总数（成交+未成交）。
    signals: u64,
    /// 触及挂单价的信号数。
    filled: u64,
    /// 成交率 = filled / signals。
    fill_rate_percent: f64,
    /// 成交且窗口内产生胜负判定的样本数。
    decided: u64,
    /// 成交并先到目标的样本数。
    target_hits: u64,
    /// 成交后胜率 = target_hits / decided。
    win_rate_percent: f64,
    /// 已决样本累计实现 R。
    net_r_total: f64,
    /// 已决样本平均 R（仅成交已决）。
    mean_r_over_decided: f64,
    /// 全信号平均 R（未成交按 0 计入分母），反映「挂单策略」整体每信号期望。
    mean_r_over_all_signals: f64,
    /// 利润因子 = 正 R 之和 / |负 R 之和|（已决样本）。
    profit_factor: f64,
}

/// 未成交批聚合，暴露 maker 逆向选择。
#[derive(Debug, Default, Serialize)]
struct UnfilledSummary {
    /// 未成交信号数。
    count: u64,
    /// 其中「若市价追突破本会盈利」的数量（missed_r > 0）。
    would_profit_count: u64,
    /// 丢掉的正 missed_r 之和（逆向选择错过的 runner 收益，gross）。
    missed_r_positive_total: f64,
    /// 全部 missed_r 之和（含避开的亏损，做上下文参照）。
    missed_r_total: f64,
}

/// 单个上海月的成交批稳定性。
#[derive(Debug, Serialize)]
struct MonthlyStability {
    /// 上海自然月标签。
    month: String,
    /// 该月信号总数。
    signals: u64,
    /// 该月成交数。
    filled: u64,
    /// 该月已决累计实现 R。
    net_r_total: f64,
    /// 该月已决平均 R。
    mean_r_over_decided: f64,
    /// 该月成交后胜率。
    win_rate_percent: f64,
}

/// 与「次开盘市价 + 全 maker」+72R 基线的对比。
#[derive(Debug, Serialize)]
struct BaselineComparison {
    /// 市价假设基线 R。
    market_maker_baseline_r: f64,
    /// 真实挂单成交批的实现 net_r。
    maker_fill_net_r: f64,
    /// 差额 = maker_fill_net_r - 基线。
    delta_vs_baseline: f64,
    /// 逆向选择丢掉的正 missed_r 之和。
    adverse_selection_missed_r: f64,
}

/// 百分比helper。
fn pct(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

/// 成交批的共享折叠：全局与逐月用同一算法，避免口径漂移。
fn fold_filled(samples: &[&ShortSample]) -> FilledSummary {
    let signals = samples.len() as u64;
    let mut filled = 0_u64;
    let mut decided = 0_u64;
    let mut target_hits = 0_u64;
    let mut net_r_total = 0.0_f64;
    let mut gross_win = 0.0_f64;
    let mut gross_loss = 0.0_f64;
    for s in samples {
        if s.outcome.filled {
            filled += 1;
        }
        // 已决样本：filled 且 realized_r 有值且 hit_target 有判定。
        if let (Some(r), Some(hit)) = (s.outcome.realized_r, s.outcome.hit_target) {
            decided += 1;
            net_r_total += r;
            if hit {
                target_hits += 1;
            }
            if r > 0.0 {
                gross_win += r;
            } else {
                gross_loss += -r;
            }
        }
    }
    FilledSummary {
        signals,
        filled,
        fill_rate_percent: pct(filled, signals),
        decided,
        target_hits,
        win_rate_percent: pct(target_hits, decided),
        net_r_total,
        mean_r_over_decided: if decided > 0 {
            net_r_total / decided as f64
        } else {
            0.0
        },
        mean_r_over_all_signals: if signals > 0 {
            net_r_total / signals as f64
        } else {
            0.0
        },
        profit_factor: if gross_loss > 0.0 {
            gross_win / gross_loss
        } else {
            0.0
        },
    }
}

/// 顶层报告。
#[derive(Debug, Serialize)]
struct Report {
    /// 变体 slug。
    variant: String,
    /// 回踩窗口棒数。
    pullback_window_bars: usize,
    /// 挂单深度。
    fill_depth: f64,
    /// 费率口径（bps）。
    maker_in_bps: f64,
    taker_out_bps: f64,
    /// 出场扫描是否跳过成交棒本身。
    exit_scan_skips_fill_bar: bool,
    /// 成交批聚合。
    filled: FilledSummary,
    /// 未成交批聚合。
    unfilled: UnfilledSummary,
    /// 逐月稳定性。
    by_month: Vec<MonthlyStability>,
    /// 与 +72R 基线对比。
    comparison: BaselineComparison,
}

/// 聚合未成交批。
fn fold_unfilled(samples: &[ShortSample]) -> UnfilledSummary {
    let mut count = 0_u64;
    let mut would_profit_count = 0_u64;
    let mut missed_r_positive_total = 0.0_f64;
    let mut missed_r_total = 0.0_f64;
    for s in samples {
        if s.outcome.filled {
            continue;
        }
        count += 1;
        if let Some(missed) = s.outcome.missed_r {
            missed_r_total += missed;
            if missed > 0.0 {
                would_profit_count += 1;
                missed_r_positive_total += missed;
            }
        }
    }
    UnfilledSummary {
        count,
        would_profit_count,
        missed_r_positive_total,
        missed_r_total,
    }
}

/// 按入场棒上海月分桶输出逐月稳定性。
fn summarize_by_month(samples: &[ShortSample]) -> Vec<MonthlyStability> {
    let mut by_month: BTreeMap<String, Vec<&ShortSample>> = BTreeMap::new();
    for s in samples {
        by_month
            .entry(shanghai_month(s.entry_time_ms))
            .or_default()
            .push(s);
    }
    by_month
        .into_iter()
        .map(|(month, group)| {
            let f = fold_filled(&group);
            MonthlyStability {
                month,
                signals: f.signals,
                filled: f.filled,
                net_r_total: f.net_r_total,
                mean_r_over_decided: f.mean_r_over_decided,
                win_rate_percent: f.win_rate_percent,
            }
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let variant = StrictVisualBreakoutResearchVariant::V20SoftBreakoutShortBodyGate;
    let rule_version = ParityRuleVersion::CandidateV20;

    let dataset = load_frozen_top60_from_quant_core()
        .await
        .context("加载 60 币种冻结宇宙失败")?;

    let mut samples: Vec<ShortSample> = Vec::new();
    for symbol in &dataset.symbols {
        samples.extend(collect_symbol_samples(
            &symbol.symbol,
            &symbol.candles,
            symbol.tick_size,
            rule_version,
            variant,
            dataset.window_start_ms,
        ));
    }

    let refs: Vec<&ShortSample> = samples.iter().collect();
    let filled = fold_filled(&refs);
    let unfilled = fold_unfilled(&samples);
    let by_month = summarize_by_month(&samples);

    let comparison = BaselineComparison {
        market_maker_baseline_r: MARKET_MAKER_BASELINE_R,
        maker_fill_net_r: filled.net_r_total,
        delta_vs_baseline: filled.net_r_total - MARKET_MAKER_BASELINE_R,
        adverse_selection_missed_r: unfilled.missed_r_positive_total,
    };

    let report = Report {
        variant: variant.slug().to_owned(),
        pullback_window_bars: PULLBACK_WINDOW_BARS,
        fill_depth: FILL_DEPTH,
        maker_in_bps: MAKER_COST_RATE_PER_SIDE * 10_000.0,
        taker_out_bps: TAKER_COST_RATE_PER_SIDE * 10_000.0,
        exit_scan_skips_fill_bar: EXIT_SCAN_SKIPS_FILL_BAR,
        filled,
        unfilled,
        by_month,
        comparison,
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
