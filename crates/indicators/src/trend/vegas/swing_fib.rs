//! Swing 高低点识别 + Fibonacci 回撤入场信号
//!
//! 规则（与用户需求对齐）：
//! - 大趋势下跌 + 小趋势（腿部）下跌 + 下跌波段后的反弹，反弹进入 Fib 区间并放量 -> 做空
//! - 大趋势上涨 + 小趋势（腿部）上涨 + 上涨波段后的回调，回调进入 Fib 区间并放量 -> 做多
use super::config::{CrossAssetAdaptiveThresholdConfig, FibRetracementSignalConfig};
use super::signal::{CrossAssetAdaptiveThresholdValue, EmaSignalValue, FibRetracementSignalValue};
use super::trend;
use crate::leg_detection_indicator::LegDetectionValue;
use rust_quant_common::CandleItem;

const STRONG_SAME_BAR_REVERSAL_FIB_RATIO: f64 = 0.382;
/// 前序冲击只能替代“巨量”确认，不能在当前成交量跌入滚动末端 5% 时继续开仓。
const MIN_DELAYED_ENTRY_VOLUME_PERCENTILE: f64 = 0.05;

/// 识别 swing 高低点（简单 N 根回看）
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
    adaptive_value: &CrossAssetAdaptiveThresholdValue,
    adaptive_config: &CrossAssetAdaptiveThresholdConfig,
    delayed_long_volume_activation_bars_ago: Option<usize>,
    delayed_short_volume_activation_bars_ago: Option<usize>,
    config: &FibRetracementSignalConfig,
) -> FibRetracementSignalValue {
    let mut out = FibRetracementSignalValue {
        volume_ratio,
        volume_percentile: adaptive_value.volume_percentile,
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
    let current_volume_confirmed = if adaptive_config.is_open {
        let min_volume_percentile =
            adaptive_config.effective_min_volume_percentile(adaptive_value.atr_ratio);
        adaptive_value.is_ready && adaptive_value.volume_percentile >= min_volume_percentile
    } else {
        volume_ratio >= config.min_volume_ratio
    };
    let delayed_entry_liquidity_confirmed = !adaptive_config.is_open
        || (adaptive_value.is_ready
            && adaptive_value.volume_percentile >= MIN_DELAYED_ENTRY_VOLUME_PERCENTILE);
    let delayed_long_volume_activation_bars_ago =
        delayed_long_volume_activation_bars_ago.filter(|_| delayed_entry_liquidity_confirmed);
    let delayed_short_volume_activation_bars_ago =
        delayed_short_volume_activation_bars_ago.filter(|_| delayed_entry_liquidity_confirmed);
    let long_volume_confirmed =
        current_volume_confirmed || delayed_long_volume_activation_bars_ago.is_some();
    let short_volume_confirmed =
        current_volume_confirmed || delayed_short_volume_activation_bars_ago.is_some();
    let swing_atr_multiple = if adaptive_value.atr_value > 0.0 {
        range / adaptive_value.atr_value
    } else {
        0.0
    };
    let swing_move_confirmed = !adaptive_config.is_open
        || (adaptive_value.is_ready
            && swing_atr_multiple >= adaptive_config.min_swing_atr_multiple.max(0.0));
    out.swing_high = swing_high;
    out.swing_low = swing_low;
    out.swing_is_upswing = swing_is_upswing;
    out.retracement_ratio = retracement_ratio;
    out.in_zone = in_zone;
    out.fib_price_low = swing_low + range * config.fib_trigger_low;
    out.fib_price_high = swing_low + range * config.fib_trigger_high;
    let selected_delayed_activation = if out.major_bullish {
        delayed_long_volume_activation_bars_ago
    } else if out.major_bearish {
        delayed_short_volume_activation_bars_ago
    } else {
        None
    };
    out.volume_confirmed = if out.major_bullish {
        long_volume_confirmed
    } else if out.major_bearish {
        short_volume_confirmed
    } else {
        current_volume_confirmed
    };
    out.used_delayed_volume_confirmation =
        !current_volume_confirmed && selected_delayed_activation.is_some();
    out.delayed_volume_activation_bars_ago = if out.used_delayed_volume_confirmation {
        selected_delayed_activation
    } else {
        None
    };
    out.swing_atr_multiple = swing_atr_multiple;
    // 大趋势不明确时，避免触发（否则会变成震荡里频繁开仓）
    if config.strict_major_trend && !out.major_bullish && !out.major_bearish {
        return out;
    }
    let leg_ok_long = !config.require_leg_confirmation || out.leg_bullish;
    let leg_ok_short = !config.require_leg_confirmation || out.leg_bearish;
    let strong_same_bar_reversal_short = candles.last().is_some_and(|candle| {
        candle.l == swing_low && retracement_ratio >= STRONG_SAME_BAR_REVERSAL_FIB_RATIO
    });
    // === 做空信号 ===
    // 条件: 大趋势空头 + 下跌波段后的反弹 + 反弹进入 Fib 区间 + 放量 + 腿部空头确认
    if out.major_bearish
        && leg_ok_short
        && !swing_is_upswing
        && in_zone
        && short_volume_confirmed
        && swing_move_confirmed
        && !strong_same_bar_reversal_short
    {
        out.is_short_signal = true;
        out.suggested_stop_loss = swing_high * (1.0 + config.stop_loss_buffer_ratio);
    }
    // === 做多信号 ===
    // 条件: 大趋势多头 + 上涨波段后的回调 + 回调进入 Fib 区间 + 放量 + 腿部多头确认
    if out.major_bullish
        && leg_ok_long
        && swing_is_upswing
        && in_zone
        && long_volume_confirmed
        && swing_move_confirmed
    {
        out.is_long_signal = true;
        out.suggested_stop_loss = swing_low * (1.0 - config.stop_loss_buffer_ratio);
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    /// 构造测试或回测用 K 线，减少样本初始化重复代码。
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

    #[test]
    fn blocks_short_when_signal_bar_sets_low_and_rebounds_above_fib_382() {
        let mut candles = vec![
            candle(11.5, 12.0, 11.0, 11.2),
            candle(11.2, 11.4, 9.5, 10.0),
            candle(10.0, 10.2, 8.0, 8.4),
            candle(8.4, 9.2, 7.0, 9.0),
        ];
        let ema = EmaSignalValue {
            ema2_value: 9.0,
            ema3_value: 10.0,
            ema4_value: 12.0,
            ..Default::default()
        };
        let leg = LegDetectionValue {
            is_bearish_leg: true,
            ..Default::default()
        };
        let config = FibRetracementSignalConfig {
            is_open: true,
            swing_lookback: 4,
            fib_trigger_low: 0.29,
            fib_trigger_high: 0.639,
            min_volume_ratio: 1.5,
            ..Default::default()
        };

        let blocked = generate_fib_retracement_signal(
            &candles,
            &ema,
            &leg,
            2.0,
            &CrossAssetAdaptiveThresholdValue::default(),
            &CrossAssetAdaptiveThresholdConfig::default(),
            None,
            None,
            &config,
        );
        assert!(!blocked.is_short_signal);

        candles.last_mut().unwrap().c = 8.5;
        let continuation = generate_fib_retracement_signal(
            &candles,
            &ema,
            &leg,
            2.0,
            &CrossAssetAdaptiveThresholdValue::default(),
            &CrossAssetAdaptiveThresholdConfig::default(),
            None,
            None,
            &config,
        );
        assert!(continuation.is_short_signal);
    }

    #[test]
    fn delayed_volume_shock_can_confirm_a_later_fib_pullback() {
        let candles = vec![
            candle(8.0, 9.0, 7.0, 8.0),
            candle(8.0, 10.0, 8.0, 9.0),
            candle(9.0, 12.0, 9.0, 11.0),
            candle(11.0, 11.0, 8.0, 9.0),
        ];
        let ema = EmaSignalValue {
            ema2_value: 11.0,
            ema3_value: 10.0,
            ema4_value: 9.0,
            ..Default::default()
        };
        let leg = LegDetectionValue {
            is_bullish_leg: true,
            ..Default::default()
        };
        let adaptive_value = CrossAssetAdaptiveThresholdValue {
            is_ready: true,
            atr_value: 1.0,
            atr_ratio: 0.02,
            volume_percentile: 0.5,
            ..Default::default()
        };
        let adaptive_config = CrossAssetAdaptiveThresholdConfig {
            is_open: true,
            min_volume_percentile: 0.95,
            min_swing_atr_multiple: 4.0,
            ..Default::default()
        };
        let config = FibRetracementSignalConfig {
            is_open: true,
            swing_lookback: 4,
            fib_trigger_low: 0.29,
            fib_trigger_high: 0.639,
            require_leg_confirmation: true,
            ..Default::default()
        };

        let without_activation = generate_fib_retracement_signal(
            &candles,
            &ema,
            &leg,
            1.0,
            &adaptive_value,
            &adaptive_config,
            None,
            None,
            &config,
        );
        assert!(!without_activation.is_long_signal);

        let very_low_liquidity = generate_fib_retracement_signal(
            &candles,
            &ema,
            &leg,
            1.0,
            &CrossAssetAdaptiveThresholdValue {
                volume_percentile: 0.04,
                ..adaptive_value
            },
            &adaptive_config,
            Some(2),
            None,
            &config,
        );
        assert!(!very_low_liquidity.is_long_signal);

        let delayed = generate_fib_retracement_signal(
            &candles,
            &ema,
            &leg,
            1.0,
            &adaptive_value,
            &adaptive_config,
            Some(2),
            None,
            &config,
        );
        assert!(delayed.is_long_signal);
        assert!(delayed.volume_confirmed);
        assert!(delayed.used_delayed_volume_confirmation);
        assert_eq!(delayed.delayed_volume_activation_bars_ago, Some(2));
    }
}
