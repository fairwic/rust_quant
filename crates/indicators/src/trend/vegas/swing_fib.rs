//! Swing 高低点识别 + Fibonacci 回撤入场信号
//!
//! 规则（与用户需求对齐）：
//! - 大趋势下跌 + 小趋势（腿部）下跌 + 下跌波段后的反弹，反弹进入 Fib 区间并放量 -> 做空
//! - 大趋势上涨 + 小趋势（腿部）上涨 + 上涨波段后的回调，回调进入 Fib 区间并放量 -> 做多

use super::config::FibRetracementSignalConfig;
use super::signal::{EmaSignalValue, FibRetracementSignalValue};
use super::trend;
use crate::leg_detection_indicator::LegDetectionValue;
use rust_quant_common::CandleItem;

/// 识别 swing 高低点（简单 N 根回看）
///
/// - `swing_high`: 窗口内最高价
/// - `swing_low`: 窗口内最低价
/// - `swing_is_upswing`: `swing_high_idx > swing_low_idx`（上涨波段后回调）
fn detect_swing_high_low(candles: &[CandleItem], lookback: usize) -> Option<(f64, f64, bool)> {
    if lookback < 2 || candles.len() < lookback {
        return None;
    }

    let start_idx = candles.len().saturating_sub(lookback);
    let window = &candles[start_idx..];

    let mut swing_high = f64::MIN;
    let mut swing_high_idx = 0usize;
    let mut swing_low = f64::MAX;
    let mut swing_low_idx = 0usize;

    for (i, candle) in window.iter().enumerate() {
        if candle.h > swing_high {
            swing_high = candle.h;
            swing_high_idx = start_idx + i;
        }
        if candle.l < swing_low {
            swing_low = candle.l;
            swing_low_idx = start_idx + i;
        }
    }

    let swing_is_upswing = swing_high_idx > swing_low_idx;
    Some((swing_high, swing_low, swing_is_upswing))
}

/// 计算当前价格相对于 swing 区间的位置（0=Low, 1=High）
fn calculate_retracement_ratio(price: f64, swing_high: f64, swing_low: f64) -> f64 {
    let range = swing_high - swing_low;
    if range <= 0.0 {
        return 0.5;
    }
    (price - swing_low) / range
}

/// 生成 Fib 回撤入场信号
pub fn generate_fib_retracement_signal(
    candles: &[CandleItem],
    ema: &EmaSignalValue,
    leg: &LegDetectionValue,
    volume_ratio: f64,
    config: &FibRetracementSignalConfig,
) -> FibRetracementSignalValue {
    let mut out = FibRetracementSignalValue {
        volume_ratio,
        leg_bullish: leg.is_bullish_leg,
        leg_bearish: leg.is_bearish_leg,
        major_bullish: trend::is_major_bullish_trend(ema),
        major_bearish: trend::is_major_bearish_trend(ema),
        ..Default::default()
    };

    if !config.is_open || candles.is_empty() {
        return out;
    }

    let Some((swing_high, swing_low, swing_is_upswing)) =
        detect_swing_high_low(candles, config.swing_lookback)
    else {
        return out;
    };

    let current_price = candles.last().map(|c| c.c).unwrap_or(0.0);
    let range = swing_high - swing_low;
    if range <= 0.0 || current_price <= 0.0 {
        return out;
    }

    let retracement_ratio = calculate_retracement_ratio(current_price, swing_high, swing_low);
    let in_zone =
        retracement_ratio >= config.fib_trigger_low && retracement_ratio <= config.fib_trigger_high;
    let volume_confirmed = volume_ratio >= config.min_volume_ratio;

    out.swing_high = swing_high;
    out.swing_low = swing_low;
    out.swing_is_upswing = swing_is_upswing;
    out.retracement_ratio = retracement_ratio;
    out.in_zone = in_zone;
    out.fib_price_low = swing_low + range * config.fib_trigger_low;
    out.fib_price_high = swing_low + range * config.fib_trigger_high;
    out.volume_confirmed = volume_confirmed;

    // 大趋势不明确时，避免触发（否则会变成震荡里频繁开仓）
    if config.strict_major_trend && !out.major_bullish && !out.major_bearish {
        return out;
    }

    let leg_ok_long = !config.require_leg_confirmation || out.leg_bullish;
    let leg_ok_short = !config.require_leg_confirmation || out.leg_bearish;

    // === 做空信号 ===
    // 条件: 大趋势空头 + 下跌波段后的反弹 + 反弹进入 Fib 区间 + 放量 + 腿部空头确认
    if out.major_bearish && leg_ok_short && !swing_is_upswing && in_zone && volume_confirmed {
        out.is_short_signal = true;
        out.suggested_stop_loss = swing_high * (1.0 + config.stop_loss_buffer_ratio);
    }

    // === 做多信号 ===
    // 条件: 大趋势多头 + 上涨波段后的回调 + 回调进入 Fib 区间 + 放量 + 腿部多头确认
    if out.major_bullish && leg_ok_long && swing_is_upswing && in_zone && volume_confirmed {
        out.is_long_signal = true;
        out.suggested_stop_loss = swing_low * (1.0 - config.stop_loss_buffer_ratio);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(o: f64, h: f64, l: f64, c: f64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            v: 1.0,
            ts: 0,
            confirm: 1,
        }
    }

    #[test]
    fn detects_swing_and_ratio() {
        let candles = vec![
            candle(10.0, 12.0, 9.5, 11.0),
            candle(11.0, 11.5, 8.0, 8.5),
            candle(8.5, 9.0, 7.0, 7.5),
            candle(7.5, 9.0, 7.2, 8.8),
        ];

        let (high, low, _) = detect_swing_high_low(&candles, 4).unwrap();
        assert!((high - 12.0).abs() < 1e-9);
        assert!((low - 7.0).abs() < 1e-9);

        let ratio = calculate_retracement_ratio(8.5, high, low);
        assert!(ratio > 0.0 && ratio < 1.0);
    }
}
