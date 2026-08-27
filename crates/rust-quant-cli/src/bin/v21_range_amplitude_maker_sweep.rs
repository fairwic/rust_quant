//! V21 区间振幅门扫描 —— 在 V20 三门基础上追加「震荡区间振幅」准入门，只读事后建模。
//!
//! 用户观察：震荡区间极窄时突破，measured-move 止盈紧贴入场，打满也被往返费率吃光（净负）。
//! 止盈幅度 = 区间高度（measured move），所以直接在源头卡区间振幅比在下游卡 target 距离更本质。
//!
//! 本 bin 复用 V20 maker 挂单成交模型（入 maker 0.02% / 出 taker 0.05%，depth=0，
//! 出场扫描跳过成交棒），在其之上一次性收集全部 V20 空头信号 + 两种口径的区间振幅，
//! 然后对 (分母 × 阈值) 网格做 post-hoc 过滤，回答：
//!   ① 每档砍掉多少笔；② 被砍的是否主要是小止盈亏损单；③ 剩余 net_r/PF/regime 是否改善。
//!
//! 两种振幅分母（用户要求都试）：
//!   - close 口径：(range.upper - range.lower) / breakout_close（与 body/excess 门 source_atr 归一族一致）
//!   - low   口径：(range.upper - range.lower) / range.lower（做空名义本金视角）
//! 阈值网格：0.0（=V20 基线，不砍）、0.010、0.015（用户基准）、0.020。

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_frozen_top60_from_quant_core, Candle, Direction, ParityRuleVersion,
    StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal, StrictVisualLongEntryState,
};
use serde::Serialize;

/// 回踩追踪窗口：突破后观察多少根完成棒判断挂单能否成交（与 V20 maker bin 一致 N=8）。
const PULLBACK_WINDOW_BARS: usize = 8;

/// 只测贴突破棒收盘价的挂单深度（depth=0 最易成交）。
const FILL_DEPTH: f64 = 0.0;

/// Maker 单边费率 2bps：仅用于入场挂单成交那一边。
const MAKER_COST_RATE_PER_SIDE: f64 = 0.0002;

/// Taker 单边费率 5bps：用于止损/目标出场那一边。
const TAKER_COST_RATE_PER_SIDE: f64 = 0.0005;

/// 出场扫描跳过成交棒本身，避免虚记同棒胜。
const EXIT_SCAN_SKIPS_FILL_BAR: bool = true;

/// 上海时区相对 UTC 的偏移（+8 小时），用于按上海自然月分桶做多窗验证。
const SHANGHAI_OFFSET_MS: i64 = 8 * 60 * 60 * 1000;

/// 区间振幅阈值网格（比例）：0.0 = V20 基线不砍，其余为用户 1.0%/1.5%/2.0% 三档。
const AMPLITUDE_THRESHOLDS: [f64; 4] = [0.0, 0.010, 0.015, 0.020];

/// 往返费率：入场 maker 0.02% + 出场 taker 0.05% = 0.07%。止盈/费率比的分母。
const ROUND_TRIP_COST_RATE: f64 = MAKER_COST_RATE_PER_SIDE + TAKER_COST_RATE_PER_SIDE;

/// 止盈/费率比阈值网格：K = target 移动幅度 / 往返费率。0 = 基线不砍。
const TARGET_COST_RATIO_THRESHOLDS: [f64; 5] = [0.0, 3.0, 5.0, 8.0, 12.0];

/// R 倍数门阈值网格。诊断显示 RR≥3 桶独占全部利润(+59.5R)，其余四桶合计 -20.6R，
/// 故围绕 3.0 上下扫描定位真实拐点，避免只取诊断桶边界造成的选择性偏差。
const REWARD_RISK_THRESHOLDS: [f64; 6] = [0.0, 2.0, 2.5, 3.0, 3.5, 4.0];

/// 把毫秒时间戳映射到上海时区的自然月标签（YYYY-MM）。
fn shanghai_month(timestamp_ms: i64) -> String {
    let Some(time) = Utc
        .timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
    else {
        return "invalid".to_owned();
    };
    format!("{:04}-{:02}", time.year(), time.month())
}

/// 准入门口径：前两个是固定区间振幅（分母不同），第三个是逐笔自适应的止盈/费率比。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum AmplitudeDenominator {
    /// 区间振幅 (upper-lower)/breakout_close。
    BreakoutClose,
    /// 区间振幅 (upper-lower)/range.lower。
    RangeLow,
    /// 止盈/费率比 |target-挂单价|/挂单价 ÷ 0.07%。逐笔自适应，不受币价绝对水平影响。
    TargetCostRatio,
    /// R 倍数门 |target-挂单价|/|挂单价-stop|。诊断显示利润全部来自 RR≥3 桶，
    /// RR 2~3 桶(占样本42.6%)净亏 -21.8R——胜率已掉到30%却拿不到足够大的赢单补偿。
    /// 相比区间振幅（代理变量），RR 是执行契约里信号确认时即可算的直接量。
    RewardRisk,
}

/// 单个 V20 空头信号样本：挂单成交结果 + 两种口径的区间振幅（供 post-hoc 扫描过滤）。
#[derive(Debug, Clone)]
struct ShortSample {
    /// 交易对，用于逐币种归因（判断增益是否被少数币种主导）。
    symbol: String,
    /// 入场棒时间戳（信号棒下一根开盘），按上海月分桶。
    entry_time_ms: i64,
    /// 挂单成交模拟结果。
    outcome: LimitFillOutcome,
    /// close 口径区间振幅。
    amplitude_close: f64,
    /// low 口径区间振幅。
    amplitude_low: f64,
    /// 止盈/费率比：|target - 挂单价| / 挂单价 ÷ 往返费率(maker+taker=0.07%)。
    /// 逐笔自适应——高价币/低波动期不会被固定振幅门一刀切。无 target 时为 0（必被砍）。
    target_cost_ratio: f64,
    /// R 倍数 = |target-挂单价| / |挂单价-stop|，即打满止盈能拿到几个 R（毛，未扣费）。
    /// 用于定位「止盈小」到底是分子(target)不足还是分母(risk)过宽。
    reward_risk: f64,
    /// 止损宽度相对源 ATR 的倍数 = |挂单价-stop| / source_atr。
    /// 若普遍 >>1，说明 min-1ATR 保护 + 突破棒整根极值把风险放大了。
    risk_atr_multiple: f64,
    /// 影线收紧后的 RR（用实体极值代替整根极值做止损）。空头原止损在上影线顶端外，
    /// 若上影线很长则止损被无谓拉宽；改用 max(open,close) 外一 tick 可收紧分母、抬高 RR。
    /// 收紧不适用（影线短或收紧后止损反而更远）时等于原 reward_risk。
    reward_risk_wick_tightened: f64,
    /// 上影线占整根高低幅度的比例，用于判断「长影线」的普遍程度。
    upper_wick_ratio: f64,
    /// 用影线收紧止损重跑成交模拟的真实结果（更紧止损更易被打，胜负会变）。
    outcome_wick_tightened: LimitFillOutcome,
    /// 该样本是否真的应用了收紧（影线足够长且收紧后止损仍有效）。
    wick_tightening_applied: bool,
}

/// depth=0 挂单成交模拟结果。
#[derive(Debug, Clone, Copy)]
struct LimitFillOutcome {
    /// 回踩窗口内是否触及挂单价（成交）。
    filled: bool,
    /// 成交且先到目标 true，先触止损 false；未成交或未决 None。
    hit_target: Option<bool>,
    /// 实现 R：成交已决为带成本实际 R；未成交 Some(0.0)；成交未决 None。
    realized_r: Option<f64>,
}

/// 把 pullback 归一化档位换算成绝对挂单价：depth=0 贴突破棒收盘。
fn limit_price_at_depth(signal: &StrictVisualBreakoutSignal, depth: f64) -> f64 {
    let boundary = match signal.direction {
        Direction::Long => signal.range.upper,
        Direction::Short => signal.range.lower,
    };
    signal.breakout_close + (boundary - signal.breakout_close) * depth
}

/// depth=0 挂单在回踩窗口内是否成交，成交后追踪到目标/止损并按「入 maker 出 taker」扣成本。
/// 与 V20 maker bin 的 simulate_limit_fill 完全同口径（本 bin 不量化 missed_r，扫描只关心成交批）。
/// stop_override 传 Some 时用该止损价替代信号自带止损（影线收紧实验用）——
/// 必须重跑成交模拟而不能只改 RR 指标，因为更紧的止损更容易被打，胜负会变。
fn simulate_limit_fill(
    candles: &[Candle],
    signal: &StrictVisualBreakoutSignal,
    stop_override: Option<f64>,
) -> LimitFillOutcome {
    let limit_price = limit_price_at_depth(signal, FILL_DEPTH);
    let signal_index = signal.signal_index;
    let mut fill_index: Option<usize> = None;
    for offset in 1..=PULLBACK_WINDOW_BARS {
        let idx = signal_index + offset;
        let Some(candle) = candles.get(idx) else {
            break;
        };
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
        return LimitFillOutcome {
            filled: false,
            hit_target: None,
            realized_r: Some(0.0),
        };
    };

    let (Some(target), Some(base_stop)) = (
        signal.measured_move_target_price,
        signal.breakout_candle_extreme_stop_price,
    ) else {
        return LimitFillOutcome {
            filled: true,
            hit_target: None,
            realized_r: None,
        };
    };
    // 影线收紧实验：用覆盖止损；未传则用信号自带止损。
    let stop = stop_override.unwrap_or(base_stop);

    let risk = (limit_price - stop).abs();
    if risk <= 0.0 {
        return LimitFillOutcome {
            filled: true,
            hit_target: None,
            realized_r: None,
        };
    }

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
        if hit_stop {
            let cost_r =
                (limit_price * MAKER_COST_RATE_PER_SIDE + stop * TAKER_COST_RATE_PER_SIDE) / risk;
            return LimitFillOutcome {
                filled: true,
                hit_target: Some(false),
                realized_r: Some(-1.0 - cost_r),
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
            };
        }
    }

    LimitFillOutcome {
        filled: true,
        hit_target: None,
        realized_r: None,
    }
}

/// 对单个交易对逐棒复放，收集通过 V20 三门的空头信号样本 + 两种口径区间振幅。
/// 三门顺序 excess→ema144→body 与 signals.rs 完全一致，让本 bin 信号集合等同真实 V20 run。
fn collect_symbol_samples(
    symbol: &str,
    candles: &[Candle],
    tick_size: f64,
    rule_version: ParityRuleVersion,
    variant: StrictVisualBreakoutResearchVariant,
    window_start_ms: i64,
    // false 时跳过 body 门（只保留 excess + ema144），用于验证 RR 门与 body 门的共线性：
    // 若 RR 门能替代 body 门且效果不降，策略更简洁、过拟合风险更低。
    apply_body_gate: bool,
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
        if candles[signal.signal_index].timestamp_ms < window_start_ms {
            continue;
        }
        if signal.direction != Direction::Short {
            continue;
        }
        // 门 1 excess。
        if let Some(max_excess_atr) = variant.soft_breakout_short_max_excess_atr() {
            if signal.source_atr <= 0.0
                || signal.breakout_excess / signal.source_atr > max_excess_atr
            {
                continue;
            }
        }
        // 门 2 ema144。
        if let Some(min_close_vs_ema144_atr) = variant.soft_breakout_short_min_close_vs_ema144_atr()
        {
            if let (Some(ema144), Some(atr)) = (point.ema144, point.atr14) {
                if atr > 0.0 && (signal.breakout_close - ema144) / atr < min_close_vs_ema144_atr {
                    continue;
                }
            }
        }
        // 门 3 body（可关闭，用于共线性验证）。
        if let (true, Some(max_body_ratio)) = (
            apply_body_gate,
            variant.soft_breakout_short_max_body_ratio(),
        ) {
            if signal.source_atr <= 0.0
                || (signal.breakout_close - signal.breakout_open).abs() / signal.source_atr
                    > max_body_ratio
            {
                continue;
            }
        }

        // 两种口径的区间振幅（供 post-hoc 扫描）。
        let range_height = signal.range.upper - signal.range.lower;
        let amplitude_close = if signal.breakout_close > 0.0 {
            range_height / signal.breakout_close
        } else {
            0.0
        };
        let amplitude_low = if signal.range.lower > 0.0 {
            range_height / signal.range.lower
        } else {
            0.0
        };

        // 止盈/费率比：以真实挂单价（入场价）为基准算 target 相对移动幅度，除以往返费率。
        // 与振幅门的区别：逐笔自适应，不受币价绝对水平和区间宽度绝对值影响。
        let limit_price = limit_price_at_depth(&signal, FILL_DEPTH);
        let target_cost_ratio = match signal.measured_move_target_price {
            Some(target) if limit_price > 0.0 => {
                let target_move_pct = (target - limit_price).abs() / limit_price;
                target_move_pct / ROUND_TRIP_COST_RATE
            }
            _ => 0.0,
        };

        // 影线收紧止损：空头原止损在突破棒整根 high 外一 tick；若上影线长，
        // 改用实体极值 max(open,close) 外一 tick，收紧 risk 分母以抬高 RR。
        // 只在收紧后的止损确实更近（且仍在入场价之上）时采用，否则退回原止损。
        let breakout_candle = &candles[signal.signal_index];
        let candle_span = breakout_candle.high - breakout_candle.low;
        let body_top = breakout_candle.open.max(breakout_candle.close);
        let upper_wick_ratio = if candle_span > 0.0 {
            (breakout_candle.high - body_top) / candle_span
        } else {
            0.0
        };
        let wick_tightened_stop = match signal.direction {
            Direction::Short => body_top + tick_size,
            Direction::Long => breakout_candle.open.min(breakout_candle.close) - tick_size,
        };
        let (reward_risk_wick_tightened, wick_tightening_applied) = match (
            signal.measured_move_target_price,
            signal.breakout_candle_extreme_stop_price,
        ) {
            (Some(target), Some(orig_stop)) => {
                // 收紧仅当新止损比原止损更靠近入场价，且方向上仍是有效止损。
                let usable = match signal.direction {
                    Direction::Short => {
                        wick_tightened_stop < orig_stop && wick_tightened_stop > limit_price
                    }
                    Direction::Long => {
                        wick_tightened_stop > orig_stop && wick_tightened_stop < limit_price
                    }
                };
                let stop = if usable {
                    wick_tightened_stop
                } else {
                    orig_stop
                };
                let risk = (limit_price - stop).abs();
                let rr = if risk > 0.0 {
                    (target - limit_price).abs() / risk
                } else {
                    0.0
                };
                (rr, usable)
            }
            _ => (0.0, false),
        };
        // 用收紧止损重跑成交模拟拿真实胜负（更紧止损更易被打，不能只改指标）。
        let outcome_wick_tightened = simulate_limit_fill(
            candles,
            &signal,
            wick_tightening_applied.then_some(wick_tightened_stop),
        );

        // R 倍数与止损宽度诊断：定位「小止盈」是 target 不足还是 risk 过宽。
        let (reward_risk, risk_atr_multiple) = match (
            signal.measured_move_target_price,
            signal.breakout_candle_extreme_stop_price,
        ) {
            (Some(target), Some(stop)) => {
                let risk = (limit_price - stop).abs();
                if risk > 0.0 {
                    let rr = (target - limit_price).abs() / risk;
                    let ram = if signal.source_atr > 0.0 {
                        risk / signal.source_atr
                    } else {
                        0.0
                    };
                    (rr, ram)
                } else {
                    (0.0, 0.0)
                }
            }
            _ => (0.0, 0.0),
        };

        let entry_time_ms = candles
            .get(signal.signal_index + 1)
            .map(|c| c.timestamp_ms)
            .unwrap_or(candles[signal.signal_index].timestamp_ms);
        samples.push(ShortSample {
            symbol: symbol.to_owned(),
            entry_time_ms,
            outcome: simulate_limit_fill(candles, &signal, None),
            amplitude_close,
            amplitude_low,
            target_cost_ratio,
            reward_risk,
            risk_atr_multiple,
            reward_risk_wick_tightened,
            upper_wick_ratio,
            outcome_wick_tightened,
            wick_tightening_applied,
        });
    }
    samples
}

/// 成交批聚合（R 已扣「入 maker 出 taker」成本）。
#[derive(Debug, Default, Serialize)]
struct FilledSummary {
    /// 信号总数（成交+未成交）。
    signals: u64,
    /// 触及挂单价的信号数。
    filled: u64,
    /// 成交率。
    fill_rate_percent: f64,
    /// 成交且窗口内已决样本数。
    decided: u64,
    /// 先到目标样本数。
    target_hits: u64,
    /// 成交后胜率。
    win_rate_percent: f64,
    /// 已决累计实现 R。
    net_r_total: f64,
    /// 已决平均 R。
    mean_r_over_decided: f64,
    /// 利润因子。
    profit_factor: f64,
    /// 正月数 / 总月数（regime 稳定性）。
    positive_months: u64,
    total_months: u64,
}

/// 单月成交批稳定性。
#[derive(Debug, Serialize)]
struct MonthlyStability {
    month: String,
    signals: u64,
    net_r_total: f64,
    win_rate_percent: f64,
}

/// 百分比 helper。
fn pct(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

/// 成交批共享折叠。
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
        profit_factor: if gross_loss > 0.0 {
            gross_win / gross_loss
        } else {
            0.0
        },
        positive_months: 0,
        total_months: 0,
    }
}

/// 按入场棒上海月分桶输出逐月稳定性。
fn summarize_by_month(samples: &[&ShortSample]) -> Vec<MonthlyStability> {
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
                net_r_total: f.net_r_total,
                win_rate_percent: f.win_rate_percent,
            }
        })
        .collect()
}

/// 被砍批（振幅 < 阈值）的画像：验证"砍掉的主要是小止盈亏损单"。
#[derive(Debug, Default, Serialize)]
struct CutBatchProfile {
    /// 被砍信号数。
    count: u64,
    /// 被砍批已决样本累计 R（负值越大说明砍得越对）。
    net_r_total: f64,
    /// 被砍批已决胜率。
    win_rate_percent: f64,
    /// 被砍批已决平均 R。
    mean_r_over_decided: f64,
}

/// 被砍批画像折叠。
fn fold_cut(samples: &[&ShortSample]) -> CutBatchProfile {
    let f = fold_filled(samples);
    CutBatchProfile {
        count: f.signals,
        net_r_total: f.net_r_total,
        win_rate_percent: f.win_rate_percent,
        mean_r_over_decided: f.mean_r_over_decided,
    }
}

/// 单币种在「基线 vs 加门」两种口径下的对比，用于判断增益是否被少数币种主导。
#[derive(Debug, Serialize)]
struct SymbolDelta {
    /// 交易对。
    symbol: String,
    /// 基线（不加振幅门）信号数。
    baseline_signals: u64,
    /// 加门后保留信号数。
    kept_signals: u64,
    /// 基线已决累计 R。
    baseline_net_r: f64,
    /// 加门后已决累计 R。
    kept_net_r: f64,
    /// 增量 = kept_net_r - baseline_net_r（正=该币种受益）。
    delta_net_r: f64,
}

/// 按币种对比基线与加门后的 net_R，输出按 delta 降序排列。
fn summarize_symbol_deltas(
    samples: &[ShortSample],
    denom: AmplitudeDenominator,
    threshold: f64,
) -> Vec<SymbolDelta> {
    let mut by_symbol: BTreeMap<String, Vec<&ShortSample>> = BTreeMap::new();
    for s in samples {
        by_symbol.entry(s.symbol.clone()).or_default().push(s);
    }
    let mut deltas: Vec<SymbolDelta> = by_symbol
        .into_iter()
        .map(|(symbol, group)| {
            let baseline = fold_filled(&group);
            let kept_refs: Vec<&ShortSample> = group
                .iter()
                .copied()
                .filter(|s| amplitude_of(s, denom) >= threshold)
                .collect();
            let kept = fold_filled(&kept_refs);
            SymbolDelta {
                symbol,
                baseline_signals: baseline.signals,
                kept_signals: kept.signals,
                baseline_net_r: baseline.net_r_total,
                kept_net_r: kept.net_r_total,
                delta_net_r: kept.net_r_total - baseline.net_r_total,
            }
        })
        .collect();
    deltas.sort_by(|a, b| {
        b.delta_net_r
            .partial_cmp(&a.delta_net_r)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    deltas
}

/// 单个 (分母 × 阈值) 扫描格。
#[derive(Debug, Serialize)]
struct SweepCell {
    denominator: AmplitudeDenominator,
    threshold: f64,
    /// 通过振幅门（保留）批的成交聚合。
    kept: FilledSummary,
    /// 被砍批画像。
    cut: CutBatchProfile,
    /// 保留批逐月稳定性。
    kept_by_month: Vec<MonthlyStability>,
    /// 逐币种基线 vs 加门对比（仅重点格填充，避免报告膨胀）。
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol_deltas: Option<Vec<SymbolDelta>>,
}

/// 取样本在指定口径下的振幅。
fn amplitude_of(sample: &ShortSample, denom: AmplitudeDenominator) -> f64 {
    match denom {
        AmplitudeDenominator::BreakoutClose => sample.amplitude_close,
        AmplitudeDenominator::RangeLow => sample.amplitude_low,
        AmplitudeDenominator::TargetCostRatio => sample.target_cost_ratio,
        AmplitudeDenominator::RewardRisk => sample.reward_risk,
    }
}

/// 对给定口径 + 阈值做 post-hoc 过滤，产出保留/被砍聚合。
fn run_cell(samples: &[ShortSample], denom: AmplitudeDenominator, threshold: f64) -> SweepCell {
    let kept_refs: Vec<&ShortSample> = samples
        .iter()
        .filter(|s| amplitude_of(s, denom) >= threshold)
        .collect();
    let cut_refs: Vec<&ShortSample> = samples
        .iter()
        .filter(|s| amplitude_of(s, denom) < threshold)
        .collect();

    let mut kept = fold_filled(&kept_refs);
    let kept_by_month = summarize_by_month(&kept_refs);
    kept.total_months = kept_by_month.len() as u64;
    kept.positive_months = kept_by_month.iter().filter(|m| m.net_r_total > 0.0).count() as u64;

    // 只对重点格（阈值>0）输出逐币种归因，基线格(0.0)无对比意义。
    let symbol_deltas = if threshold > 0.0 {
        Some(summarize_symbol_deltas(samples, denom, threshold))
    } else {
        None
    };

    SweepCell {
        denominator: denom,
        threshold,
        kept,
        cut: fold_cut(&cut_refs),
        kept_by_month,
        symbol_deltas,
    }
}

/// R 倍数 / 止损宽度分桶诊断：定位「小止盈」的真正来源。
#[derive(Debug, Serialize)]
struct DiagnosticBucket {
    /// 桶标签（区间描述）。
    bucket: String,
    /// 该桶信号数。
    signals: u64,
    /// 该桶占全样本比例。
    share_percent: f64,
    /// 该桶已决累计 R（扣费后）。
    net_r_total: f64,
    /// 该桶已决平均 R。
    mean_r: f64,
    /// 该桶胜率。
    win_rate_percent: f64,
    /// 该桶 reward_risk 均值（打满止盈能拿几个 R）。
    mean_reward_risk: f64,
    /// 该桶 risk/ATR 均值（止损宽度）。
    mean_risk_atr: f64,
}

/// 按给定取值函数与分桶边界做诊断分桶。
fn bucketize(
    samples: &[ShortSample],
    label: &str,
    value_of: impl Fn(&ShortSample) -> f64,
    edges: &[f64],
) -> Vec<DiagnosticBucket> {
    let total = samples.len() as f64;
    let mut out = Vec::new();
    for i in 0..=edges.len() {
        let (lo, hi) = (
            if i == 0 { f64::MIN } else { edges[i - 1] },
            if i == edges.len() { f64::MAX } else { edges[i] },
        );
        let group: Vec<&ShortSample> = samples
            .iter()
            .filter(|s| {
                let v = value_of(s);
                v >= lo && v < hi
            })
            .collect();
        if group.is_empty() {
            continue;
        }
        let f = fold_filled(&group);
        let n = group.len() as f64;
        out.push(DiagnosticBucket {
            bucket: format!(
                "{}[{}~{})",
                label,
                if i == 0 {
                    "-inf".to_owned()
                } else {
                    format!("{lo:.2}")
                },
                if i == edges.len() {
                    "inf".to_owned()
                } else {
                    format!("{hi:.2}")
                }
            ),
            signals: f.signals,
            share_percent: n / total * 100.0,
            net_r_total: f.net_r_total,
            mean_r: f.mean_r_over_decided,
            win_rate_percent: f.win_rate_percent,
            mean_reward_risk: group.iter().map(|s| s.reward_risk).sum::<f64>() / n,
            mean_risk_atr: group.iter().map(|s| s.risk_atr_multiple).sum::<f64>() / n,
        });
    }
    out
}

/// 影线收紧止损实验:同一批样本在原止损 vs 收紧止损下的真实成交对比。
#[derive(Debug, Serialize)]
struct WickExperimentRow {
    /// RR 门阈值（用哪套 RR 筛选:原 RR 还是收紧后 RR，见字段说明）。
    rr_threshold: f64,
    /// 用原止损 + 原 RR 筛选的结果（基准）。
    baseline_signals: u64,
    baseline_net_r: f64,
    baseline_pf: f64,
    baseline_win_rate: f64,
    /// 用收紧止损 + 收紧后 RR 筛选的结果。
    tightened_signals: u64,
    tightened_net_r: f64,
    tightened_pf: f64,
    tightened_win_rate: f64,
    /// 收紧组里实际应用了收紧的样本数。
    tightening_applied_count: u64,
}

/// 用收紧止损的 outcome 折叠（与 fold_filled 同算法，只换 outcome 来源）。
fn fold_filled_tightened(samples: &[&ShortSample]) -> FilledSummary {
    let signals = samples.len() as u64;
    let mut filled = 0_u64;
    let mut decided = 0_u64;
    let mut target_hits = 0_u64;
    let mut net_r_total = 0.0_f64;
    let mut gross_win = 0.0_f64;
    let mut gross_loss = 0.0_f64;
    for s in samples {
        let o = &s.outcome_wick_tightened;
        if o.filled {
            filled += 1;
        }
        if let (Some(r), Some(hit)) = (o.realized_r, o.hit_target) {
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
        profit_factor: if gross_loss > 0.0 {
            gross_win / gross_loss
        } else {
            0.0
        },
        positive_months: 0,
        total_months: 0,
    }
}

/// 顶层报告。
#[derive(Debug, Serialize)]
struct Report {
    /// 基础变体（V20）slug。
    base_variant: String,
    pullback_window_bars: usize,
    fill_depth: f64,
    maker_in_bps: f64,
    taker_out_bps: f64,
    /// 全部 V20 空头信号数（未加振幅门）。
    total_v20_short_signals: u64,
    /// 扫描格：两种口径 × 阈值网格。
    cells: Vec<SweepCell>,
    /// 影线收紧止损实验（在最优 RR 门基础上迭代）。
    wick_experiment: Vec<WickExperimentRow>,
    /// 上影线比例分桶：看「长影线」在样本里多普遍。
    upper_wick_buckets: Vec<DiagnosticBucket>,
    /// 共线性对照：去掉 body 门后的信号总数。
    total_no_body_gate_signals: u64,
    /// 共线性对照：去掉 body 门后的 RR 门扫描格。
    cells_no_body_gate: Vec<SweepCell>,
    /// R 倍数分桶诊断：打满止盈能拿几个 R。
    reward_risk_buckets: Vec<DiagnosticBucket>,
    /// 止损宽度分桶诊断：risk / source_atr。
    risk_atr_buckets: Vec<DiagnosticBucket>,
    /// 全样本 reward_risk 均值 / 中位数。
    reward_risk_mean: f64,
    reward_risk_median: f64,
    /// 全样本 risk/ATR 均值 / 中位数。
    risk_atr_mean: f64,
    risk_atr_median: f64,
}

/// 取中位数（就地排序副本）。
fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let variant = StrictVisualBreakoutResearchVariant::V20SoftBreakoutShortBodyGate;
    let rule_version = ParityRuleVersion::CandidateV20;

    let dataset = load_frozen_top60_from_quant_core()
        .await
        .context("加载 60 币种冻结宇宙失败")?;

    // 两套信号集：带 body 门（真实 V20）与不带 body 门（只 excess+ema144），
    // 用于验证 RR 门是否与 body 门共线——若不带 body 门时 RR 门效果不降，则可替代。
    let mut samples: Vec<ShortSample> = Vec::new();
    let mut samples_no_body: Vec<ShortSample> = Vec::new();
    for symbol in &dataset.symbols {
        samples.extend(collect_symbol_samples(
            &symbol.symbol,
            &symbol.candles,
            symbol.tick_size,
            rule_version,
            variant,
            dataset.window_start_ms,
            true,
        ));
        samples_no_body.extend(collect_symbol_samples(
            &symbol.symbol,
            &symbol.candles,
            symbol.tick_size,
            rule_version,
            variant,
            dataset.window_start_ms,
            false,
        ));
    }

    let mut cells = Vec::new();
    // 固定区间振幅门：两种分母 × 振幅阈值网格。
    for denom in [
        AmplitudeDenominator::BreakoutClose,
        AmplitudeDenominator::RangeLow,
    ] {
        for threshold in AMPLITUDE_THRESHOLDS {
            cells.push(run_cell(&samples, denom, threshold));
        }
    }
    // 逐笔自适应止盈/费率比门：K 阈值网格。
    for threshold in TARGET_COST_RATIO_THRESHOLDS {
        cells.push(run_cell(
            &samples,
            AmplitudeDenominator::TargetCostRatio,
            threshold,
        ));
    }
    // R 倍数门：期望值结构的直接门，非代理变量。
    for threshold in REWARD_RISK_THRESHOLDS {
        cells.push(run_cell(
            &samples,
            AmplitudeDenominator::RewardRisk,
            threshold,
        ));
    }
    // 共线性对照：去掉 body 门后同样扫 RR 门。
    let cells_no_body: Vec<SweepCell> = REWARD_RISK_THRESHOLDS
        .iter()
        .map(|&threshold| {
            run_cell(
                &samples_no_body,
                AmplitudeDenominator::RewardRisk,
                threshold,
            )
        })
        .collect();

    let report = Report {
        base_variant: variant.slug().to_owned(),
        pullback_window_bars: PULLBACK_WINDOW_BARS,
        fill_depth: FILL_DEPTH,
        maker_in_bps: MAKER_COST_RATE_PER_SIDE * 10_000.0,
        taker_out_bps: TAKER_COST_RATE_PER_SIDE * 10_000.0,
        total_v20_short_signals: samples.len() as u64,
        cells,
        wick_experiment: REWARD_RISK_THRESHOLDS
            .iter()
            .map(|&rr| {
                // 基准：原 RR 筛选 + 原止损结果。
                let base_refs: Vec<&ShortSample> =
                    samples.iter().filter(|s| s.reward_risk >= rr).collect();
                let base = fold_filled(&base_refs);
                // 收紧：用收紧后 RR 筛选 + 收紧止损的真实成交结果。
                let tight_refs: Vec<&ShortSample> = samples
                    .iter()
                    .filter(|s| s.reward_risk_wick_tightened >= rr)
                    .collect();
                let tight = fold_filled_tightened(&tight_refs);
                WickExperimentRow {
                    rr_threshold: rr,
                    baseline_signals: base.signals,
                    baseline_net_r: base.net_r_total,
                    baseline_pf: base.profit_factor,
                    baseline_win_rate: base.win_rate_percent,
                    tightened_signals: tight.signals,
                    tightened_net_r: tight.net_r_total,
                    tightened_pf: tight.profit_factor,
                    tightened_win_rate: tight.win_rate_percent,
                    tightening_applied_count: tight_refs
                        .iter()
                        .filter(|s| s.wick_tightening_applied)
                        .count() as u64,
                }
            })
            .collect(),
        upper_wick_buckets: bucketize(
            &samples,
            "upperWick",
            |s| s.upper_wick_ratio,
            &[0.1, 0.2, 0.3, 0.5],
        ),
        total_no_body_gate_signals: samples_no_body.len() as u64,
        cells_no_body_gate: cells_no_body,
        reward_risk_buckets: bucketize(
            &samples,
            "RR",
            |s| s.reward_risk,
            &[0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0],
        ),
        risk_atr_buckets: bucketize(
            &samples,
            "riskATR",
            |s| s.risk_atr_multiple,
            &[1.0, 1.5, 2.0, 3.0, 4.0, 6.0],
        ),
        reward_risk_mean: samples.iter().map(|s| s.reward_risk).sum::<f64>()
            / samples.len().max(1) as f64,
        reward_risk_median: median(samples.iter().map(|s| s.reward_risk).collect()),
        risk_atr_mean: samples.iter().map(|s| s.risk_atr_multiple).sum::<f64>()
            / samples.len().max(1) as f64,
        risk_atr_median: median(samples.iter().map(|s| s.risk_atr_multiple).collect()),
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
