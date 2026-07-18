use super::super::{BacktestCandle, MS_15M};
use rust_quant_strategies::CandleItem;

const FRAMEWORK_SIGNAL_WARMUP_CANDLES: usize = 500;

/// 将信号时间对齐到第一根不早于信号的 K 线，供框架在生产可见的下一时点回放。
/// `raw_state` 事件可落在任意毫秒，不能要求它恰好等于 15m K 线起点，也不能回退到信号前 K 线。
pub(super) fn replay_entry_candle_ts(candles: &[BacktestCandle], entry_ts: i64) -> Option<i64> {
    let index = candles.partition_point(|candle| candle.ts < entry_ts);
    candles.get(index).map(|candle| candle.ts)
}

/// 构建框架回放 K 线，并补足策略框架初始化所需的固定预热窗口。
pub(super) fn framework_replay_candle_items(candles: &[BacktestCandle]) -> Vec<CandleItem> {
    let Some(first) = candles.first() else {
        return Vec::new();
    };
    let interval_ms = candles
        .get(1)
        .map(|second| second.ts - first.ts)
        .filter(|interval| *interval > 0)
        .unwrap_or(MS_15M);
    let mut items = Vec::with_capacity(candles.len() + FRAMEWORK_SIGNAL_WARMUP_CANDLES);
    for offset in (1..=FRAMEWORK_SIGNAL_WARMUP_CANDLES).rev() {
        let ts = first
            .ts
            .saturating_sub(interval_ms.saturating_mul(offset as i64));
        items.push(CandleItem {
            o: first.open,
            h: first.open,
            l: first.open,
            c: first.open,
            v: 0.0,
            ts,
            confirm: 1,
        });
    }
    items.extend(candles.iter().map(to_candle_item));
    items
}

fn to_candle_item(candle: &BacktestCandle) -> CandleItem {
    CandleItem {
        o: candle.open,
        h: candle.high,
        l: candle.low,
        c: candle.close,
        v: candle.volume,
        ts: candle.ts,
        confirm: 1,
    }
}
