use super::{BacktestCandle, FvgEntryMode, MarketVelocityEventBacktestArgs, MS_15M, MS_1H, MS_4H};
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
    }
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
        if signal.ts > deadline {
            break;
        }
        if !bullish_midpoint_confirmation(signal) {
            continue;
        }
        let Some(_zone) = zones
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
