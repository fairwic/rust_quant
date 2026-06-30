use super::{
    BacktestCandle, ComputedCandle, FvgEntryMode, MarketVelocityEventBacktestArgs, MS_15M, MS_1H,
    MS_4H,
};
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FvgEntrySignal {
    /// 时间戳。
    pub entry_ts: i64,
    /// 入场价格。
    pub entry_price: f64,
    /// 入场15midx，用于记录新闻或情报分析结果。
    pub entry_15m_idx: usize,
    /// trigger，用于记录新闻或情报分析结果。
    pub trigger: String,
    /// 结构止损价格；为空时表示没有结构锚点。
    pub structure_stop_loss_price: Option<f64>,
    /// 结构止损来源；为空时表示没有结构锚点。
    pub structure_stop_loss_source: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FvgEntrySearch {
    Found(FvgEntrySignal),
    Blocked(String),
}
#[derive(Debug, Clone, PartialEq)]
struct BullishFvgZone {
    /// lower，用于行情、K 线或市场扫描。
    lower: f64,
    /// upper，用于行情、K 线或市场扫描。
    upper: f64,
    /// 时间戳。
    active_from_ts: i64,
}
/// 提供查找FVG入场的集中实现，避免回测策略调用方重复处理相同细节。
pub(super) fn find_fvg_entry(
    mode: FvgEntryMode,
    candles_4h: &[BacktestCandle],
    candles_1h: &[BacktestCandle],
    candles_15m: &[BacktestCandle],
    event_ts: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> FvgEntrySearch {
    match mode {
        FvgEntryMode::Off => FvgEntrySearch::Blocked("fvg_mode_off".to_string()),
        FvgEntryMode::M15To1h => find_entry_for_timeframes(
            candles_1h,
            candles_15m,
            candles_15m,
            event_ts,
            MS_1H,
            MS_15M,
            "fvg_15m_to_1h",
            args,
        ),
        FvgEntryMode::H1To4h => find_entry_for_timeframes(
            candles_4h,
            candles_1h,
            candles_15m,
            event_ts,
            MS_4H,
            MS_1H,
            "fvg_1h_to_4h",
            args,
        ),
        FvgEntryMode::M15SelfAfterSignal => {
            FvgEntrySearch::Blocked("fvg_15m_self_requires_original_signal".to_string())
        }
        FvgEntryMode::M15ImpulseRetrace => {
            FvgEntrySearch::Blocked("fvg_15m_impulse_requires_original_signal".to_string())
        }
    }
}

pub(super) fn find_15m_impulse_fvg_retrace_after_signal(
    candles_15m: &[BacktestCandle],
    computed_15m: &[ComputedCandle],
    event_ts: i64,
    original_trigger: &str,
    args: &MarketVelocityEventBacktestArgs,
) -> FvgEntrySearch {
    let completed = completed_candle_count(candles_15m, event_ts, MS_15M);
    if completed < 3 {
        return FvgEntrySearch::Blocked("fvg_no_15m_impulse_signal".to_string());
    }
    let signal_idx = completed - 1;
    let Some(zone) =
        recent_untouched_impulse_zone_before_signal(candles_15m, signal_idx, event_ts, args)
    else {
        return FvgEntrySearch::Blocked("fvg_no_recent_15m_impulse_gap".to_string());
    };
    let lower_band_upper =
        zone.lower + (zone.upper - zone.lower) * args.fvg_impulse_retrace_fill_pct / 100.0;
    let deadline = event_ts + MS_15M * args.fvg_max_wait_candles as i64;
    let first_retest_idx = signal_idx + 1 + args.fvg_impulse_retrace_min_wait_candles;
    for retest_idx in first_retest_idx..candles_15m.len() {
        let retest = &candles_15m[retest_idx];
        if retest.ts + MS_15M > deadline {
            break;
        }
        if !candle_contains_price(retest, lower_band_upper) {
            continue;
        }
        if is_breakout_base_trigger(original_trigger)
            && !has_breakout_failure_before_fill(computed_15m, signal_idx, retest_idx)
        {
            return FvgEntrySearch::Blocked("fvg_no_breakout_failure_before_fill".to_string());
        }
        return FvgEntrySearch::Found(FvgEntrySignal {
            entry_ts: retest.ts,
            entry_price: lower_band_upper,
            entry_15m_idx: retest_idx,
            trigger: format!("{original_trigger}+fvg_15m_impulse_retrace"),
            structure_stop_loss_price: Some(zone.lower),
            structure_stop_loss_source: Some("fvg_15m_impulse_lower".to_string()),
        });
    }
    FvgEntrySearch::Blocked("fvg_no_15m_impulse_limit_fill".to_string())
}

fn recent_untouched_impulse_zone_before_signal(
    candles_15m: &[BacktestCandle],
    signal_idx: usize,
    event_ts: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<BullishFvgZone> {
    let start = signal_idx
        .saturating_sub(args.fvg_lookback_candles.saturating_sub(1))
        .max(2);
    for idx in (start..=signal_idx).rev() {
        let current = &candles_15m[idx];
        let anchor = &candles_15m[idx - 2];
        if current.low <= anchor.high {
            continue;
        }
        let zone = BullishFvgZone {
            lower: anchor.high,
            upper: current.low,
            active_from_ts: current.ts + MS_15M,
        };
        if !zone_touched_before_event(candles_15m, &zone, event_ts, MS_15M) {
            return Some(zone);
        }
    }
    None
}

fn is_breakout_base_trigger(trigger: &str) -> bool {
    trigger.split_once('+').map_or(trigger, |(base, _)| base) == "breakout_previous_high"
}

fn has_breakout_failure_before_fill(
    computed_15m: &[ComputedCandle],
    signal_idx: usize,
    retest_idx: usize,
) -> bool {
    if signal_idx + 1 >= retest_idx {
        return false;
    }
    for failure_idx in signal_idx + 1..retest_idx {
        let Some(failure) = computed_15m.get(failure_idx) else {
            continue;
        };
        if failure.ema.is_some_and(|ema| failure.candle.close <= ema) {
            return true;
        }
    }
    false
}

pub(super) fn find_15m_self_fvg_entry_after_signal(
    candles_15m: &[BacktestCandle],
    event_ts: i64,
    original_trigger: &str,
    args: &MarketVelocityEventBacktestArgs,
) -> FvgEntrySearch {
    let start = first_candle_closing_after(candles_15m, event_ts, MS_15M).max(2);
    let deadline = event_ts + MS_15M * args.fvg_max_wait_candles as i64;
    for zone_idx in start..candles_15m.len() {
        let zone_candle = &candles_15m[zone_idx];
        if zone_candle.ts + MS_15M > deadline {
            break;
        }
        let anchor = &candles_15m[zone_idx - 2];
        if zone_candle.low <= anchor.high {
            continue;
        }
        for signal_idx in zone_idx + 1..candles_15m.len() {
            let signal = &candles_15m[signal_idx];
            if signal.ts + MS_15M > deadline {
                break;
            }
            if !bullish_midpoint_confirmation(signal) {
                continue;
            }
            if !candle_overlaps_zone(signal, anchor.high, zone_candle.low) {
                continue;
            }
            let entry_idx = signal_idx + 1;
            let Some(entry) = candles_15m.get(entry_idx) else {
                return FvgEntrySearch::Blocked("fvg_no_next_entry_candle".to_string());
            };
            return FvgEntrySearch::Found(FvgEntrySignal {
                entry_ts: entry.ts,
                entry_price: entry.open,
                entry_15m_idx: entry_idx,
                trigger: format!("{original_trigger}+fvg_15m_self_after_signal"),
                structure_stop_loss_price: Some(anchor.high),
                structure_stop_loss_source: Some("fvg_15m_self_lower".to_string()),
            });
        }
    }
    FvgEntrySearch::Blocked("fvg_no_15m_self_pullback_confirmation".to_string())
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
fn find_entry_for_timeframes(
    higher: &[BacktestCandle],
    lower: &[BacktestCandle],
    execution_15m: &[BacktestCandle],
    event_ts: i64,
    higher_ms: i64,
    lower_ms: i64,
    trigger: &str,
    args: &MarketVelocityEventBacktestArgs,
) -> FvgEntrySearch {
    let zones = recent_untouched_bullish_zones(
        higher,
        lower,
        event_ts,
        higher_ms,
        lower_ms,
        args.fvg_lookback_candles,
    );
    if zones.is_empty() {
        return FvgEntrySearch::Blocked("fvg_no_recent_untouched_zone".to_string());
    }
    let lower_start = first_candle_closing_after(lower, event_ts, lower_ms);
    let deadline = event_ts + lower_ms * args.fvg_max_wait_candles as i64;
    for signal_idx in lower_start..lower.len() {
        let signal = &lower[signal_idx];
        if signal.ts + lower_ms > deadline {
            break;
        }
        if !bullish_midpoint_confirmation(signal) {
            continue;
        }
        let Some(zone) = zones
            .iter()
            .find(|zone| candle_overlaps_zone(signal, zone.lower, zone.upper))
        else {
            continue;
        };
        let entry_idx = signal_idx + 1;
        let Some(entry) = lower.get(entry_idx) else {
            return FvgEntrySearch::Blocked("fvg_no_next_entry_candle".to_string());
        };
        let Some(entry_15m_idx) = execution_index_for_entry(execution_15m, entry.ts) else {
            return FvgEntrySearch::Blocked("fvg_no_15m_execution_candle".to_string());
        };
        return FvgEntrySearch::Found(FvgEntrySignal {
            entry_ts: entry.ts,
            entry_price: entry.open,
            entry_15m_idx,
            trigger: trigger.to_string(),
            structure_stop_loss_price: Some(zone.lower),
            structure_stop_loss_source: Some("fvg_zone_lower".to_string()),
        });
    }
    FvgEntrySearch::Blocked("fvg_no_pullback_confirmation".to_string())
}
/// 提供最近未触碰多头区域的集中实现，避免回测策略调用方重复处理相同细节。
fn recent_untouched_bullish_zones(
    higher: &[BacktestCandle],
    lower: &[BacktestCandle],
    event_ts: i64,
    higher_ms: i64,
    lower_ms: i64,
    lookback: usize,
) -> Vec<BullishFvgZone> {
    let completed = completed_candle_count(higher, event_ts, higher_ms);
    if completed < 3 {
        return Vec::new();
    }
    let start = completed.saturating_sub(lookback).max(2);
    let mut zones = Vec::new();
    for idx in start..completed {
        let current = &higher[idx];
        let anchor = &higher[idx - 2];
        if current.low > anchor.high {
            let zone = BullishFvgZone {
                lower: anchor.high,
                upper: current.low,
                active_from_ts: current.ts + higher_ms,
            };
            if !zone_touched_before_event(lower, &zone, event_ts, lower_ms) {
                zones.push(zone);
            }
        }
    }
    zones.reverse();
    zones
}
/// 提供区域touched之前event的集中实现，避免回测策略调用方重复处理相同细节。
fn zone_touched_before_event(
    lower: &[BacktestCandle],
    zone: &BullishFvgZone,
    event_ts: i64,
    lower_ms: i64,
) -> bool {
    lower.iter().any(|candle| {
        candle.ts >= zone.active_from_ts
            && candle.ts + lower_ms <= event_ts
            && candle_overlaps_zone(candle, zone.lower, zone.upper)
    })
}
/// 提供首个K 线收盘之后的集中实现，避免回测策略调用方重复处理相同细节。
fn first_candle_closing_after(candles: &[BacktestCandle], ts: i64, candle_ms: i64) -> usize {
    let mut left = 0;
    let mut right = candles.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if candles[mid].ts + candle_ms <= ts {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}
/// 提供已完成K 线数量的集中实现，避免回测策略调用方重复处理相同细节。
fn completed_candle_count(candles: &[BacktestCandle], ts: i64, candle_ms: i64) -> usize {
    let mut left = 0;
    let mut right = candles.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if candles[mid].ts + candle_ms <= ts {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}
/// 提供多头中点确认的集中实现，避免回测策略调用方重复处理相同细节。
fn bullish_midpoint_confirmation(candle: &BacktestCandle) -> bool {
    let midpoint = (candle.high + candle.low) / 2.0;
    candle.close > candle.open && candle.close >= midpoint
}
fn candle_overlaps_zone(candle: &BacktestCandle, lower: f64, upper: f64) -> bool {
    candle.low <= upper && candle.high >= lower
}

fn candle_contains_price(candle: &BacktestCandle, price: f64) -> bool {
    candle.low <= price && candle.high >= price
}
/// 提供执行索引for入场的集中实现，避免回测策略调用方重复处理相同细节。
fn execution_index_for_entry(candles_15m: &[BacktestCandle], entry_ts: i64) -> Option<usize> {
    let mut left = 0;
    let mut right = candles_15m.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if candles_15m[mid].ts < entry_ts {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    candles_15m
        .get(left)
        .filter(|candle| candle.ts == entry_ts)
        .map(|_| left)
}
