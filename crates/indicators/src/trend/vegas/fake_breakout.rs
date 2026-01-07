//! 假突破信号检测模块
//!
//! 基于第一性原理文档的量化定义：
//!
//! ## 看涨假突破（用于做空）
//! 1. K线最高价突破前高（前20根K线高点）
//! 2. 收盘价回落至前高下方
//! 3. 上影线长度 ≥ 实体长度 × 1.5
//! 4. 成交量 > 前3根K线平均成交量 × 1.2
//!
//! ## 看跌假突破（用于做多）
//! 1. K线最低价跌破前低（前20根K线低点）
//! 2. 收盘价回升至前低上方
//! 3. 下影线长度 ≥ 实体长度 × 1.5
//! 4. 成交量 > 前3根K线平均成交量 × 1.2

use rust_quant_common::CandleItem;
use serde::{Deserialize, Serialize};

/// 假突破检测参数配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FakeBreakoutConfig {
    /// 回溯K线数量（用于计算前高/前低）
    pub lookback_bars: usize,
    /// 影线与实体的最小比例
    pub shadow_body_ratio: f64,
    /// 成交量放大倍数
    pub volume_multiplier: f64,
    /// 成交量回溯K线数量
    pub volume_lookback_bars: usize,
    /// 收盘价回落深度（相对于前高/前低的百分比）
    pub close_depth_ratio: f64,
}

impl Default for FakeBreakoutConfig {
    fn default() -> Self {
        Self {
            lookback_bars: 20,
            shadow_body_ratio: 1.5,
            volume_multiplier: 1.2,
            volume_lookback_bars: 3,
            close_depth_ratio: 0.001, // 0.1% 深度
        }
    }
}

/// 假突破信号结果
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FakeBreakoutSignal {
    /// 看涨假突破（用于做空）：价格突破前高后回落
    pub is_bearish_fake_breakout: bool,
    /// 看跌假突破（用于做多）：价格跌破前低后回升
    pub is_bullish_fake_breakout: bool,
    /// 突破的关键价位（前高或前低）
    pub breakout_level: f64,
    /// 影线比例（上影线或下影线与实体的比例）
    pub shadow_ratio: f64,
    /// 成交量是否确认
    pub volume_confirmed: bool,
    /// 成交量比例（当前成交量 / 前N根平均成交量）
    pub volume_ratio: f64,
    /// 信号强度（0.0 - 1.0）
    pub strength: f64,
}

impl FakeBreakoutSignal {
    /// 是否有任何假突破信号
    pub fn has_signal(&self) -> bool {
        self.is_bearish_fake_breakout || self.is_bullish_fake_breakout
    }

    /// 获取信号方向（正数做多，负数做空，0无信号）
    pub fn direction(&self) -> i8 {
        if self.is_bullish_fake_breakout {
            1 // 看跌假突破 → 做多
        } else if self.is_bearish_fake_breakout {
            -1 // 看涨假突破 → 做空
        } else {
            0
        }
    }
}

/// 检测假突破信号
///
/// # 参数
/// - `candles`: K线数据切片，至少需要 lookback_bars + 1 根
/// - `config`: 假突破检测配置
///
/// # 返回
/// - `FakeBreakoutSignal`: 假突破信号结果
///
/// # 示例
/// ```ignore
/// let signal = detect_fake_breakout(&candles, &FakeBreakoutConfig::default());
/// if signal.is_bullish_fake_breakout {
///     // 执行做多逻辑
/// }
/// ```
pub fn detect_fake_breakout(
    candles: &[CandleItem],
    config: &FakeBreakoutConfig,
) -> FakeBreakoutSignal {
    let mut result = FakeBreakoutSignal::default();

    // 数据量不足
    let min_required = config.lookback_bars + 1;
    if candles.len() < min_required {
        return result;
    }

    let current = candles.last().expect("candles should not be empty");
    let lookback_candles = &candles[candles.len() - config.lookback_bars - 1..candles.len() - 1];

    // 计算前高和前低
    let (prev_high, prev_low) = calculate_high_low(lookback_candles);

    // 计算成交量比例
    let volume_ratio = calculate_volume_ratio(candles, config.volume_lookback_bars);
    result.volume_ratio = volume_ratio;
    result.volume_confirmed = volume_ratio >= config.volume_multiplier;

    // 计算K线形态
    let body = (current.c - current.o).abs();
    let upper_shadow = current.h - current.c.max(current.o);
    let lower_shadow = current.c.min(current.o) - current.l;

    // 检测看涨假突破（价格突破前高后回落 → 做空信号）
    if check_bearish_fake_breakout(current, prev_high, body, upper_shadow, config) {
        result.is_bearish_fake_breakout = true;
        result.breakout_level = prev_high;
        result.shadow_ratio = if body > 0.0 {
            upper_shadow / body
        } else {
            f64::MAX
        };
        result.strength = calculate_signal_strength(
            result.shadow_ratio,
            volume_ratio,
            config.shadow_body_ratio,
            config.volume_multiplier,
        );
    }

    // 检测看跌假突破（价格跌破前低后回升 → 做多信号）
    if check_bullish_fake_breakout(current, prev_low, body, lower_shadow, config) {
        result.is_bullish_fake_breakout = true;
        result.breakout_level = prev_low;
        result.shadow_ratio = if body > 0.0 {
            lower_shadow / body
        } else {
            f64::MAX
        };
        result.strength = calculate_signal_strength(
            result.shadow_ratio,
            volume_ratio,
            config.shadow_body_ratio,
            config.volume_multiplier,
        );
    }

    result
}

/// 检测看涨假突破（用于做空）
///
/// 条件：
/// 1. K线最高价突破前高
/// 2. 收盘价回落至前高下方（至少 close_depth_ratio 深度）
/// 3. 上影线长度 ≥ 实体长度 × shadow_body_ratio
/// 4. 成交量确认（在主函数中检查）
fn check_bearish_fake_breakout(
    current: &CandleItem,
    prev_high: f64,
    body: f64,
    upper_shadow: f64,
    config: &FakeBreakoutConfig,
) -> bool {
    // 条件1: 最高价突破前高
    let condition_1 = current.h > prev_high;

    // 条件2: 收盘价回落至前高下方（有一定深度）
    let close_depth = prev_high * (1.0 - config.close_depth_ratio);
    let condition_2 = current.c < close_depth;

    // 条件3: 上影线 ≥ 实体 × ratio
    // 如果实体为0（十字星），只要有上影线就算
    let condition_3 = if body > 0.0 {
        upper_shadow >= body * config.shadow_body_ratio
    } else {
        upper_shadow > 0.0
    };

    condition_1 && condition_2 && condition_3
}

/// 检测看跌假突破（用于做多）
///
/// 条件：
/// 1. K线最低价跌破前低
/// 2. 收盘价回升至前低上方（至少 close_depth_ratio 深度）
/// 3. 下影线长度 ≥ 实体长度 × shadow_body_ratio
/// 4. 成交量确认（在主函数中检查）
fn check_bullish_fake_breakout(
    current: &CandleItem,
    prev_low: f64,
    body: f64,
    lower_shadow: f64,
    config: &FakeBreakoutConfig,
) -> bool {
    // 条件1: 最低价跌破前低
    let condition_1 = current.l < prev_low;

    // 条件2: 收盘价回升至前低上方（有一定深度）
    let close_depth = prev_low * (1.0 + config.close_depth_ratio);
    let condition_2 = current.c > close_depth;

    // 条件3: 下影线 ≥ 实体 × ratio
    let condition_3 = if body > 0.0 {
        lower_shadow >= body * config.shadow_body_ratio
    } else {
        lower_shadow > 0.0
    };

    condition_1 && condition_2 && condition_3
}

/// 计算前高和前低
fn calculate_high_low(candles: &[CandleItem]) -> (f64, f64) {
    if candles.is_empty() {
        return (0.0, f64::MAX);
    }

    let mut high = f64::MIN;
    let mut low = f64::MAX;

    for candle in candles {
        high = high.max(candle.h);
        low = low.min(candle.l);
    }

    (high, low)
}

/// 计算成交量比例
fn calculate_volume_ratio(candles: &[CandleItem], lookback: usize) -> f64 {
    if candles.len() < lookback + 1 {
        return 0.0;
    }

    let current_volume = candles.last().map(|c| c.v).unwrap_or(0.0);

    // 计算前N根K线的平均成交量
    let start = candles.len().saturating_sub(lookback + 1);
    let end = candles.len() - 1;
    let volume_slice = &candles[start..end];

    if volume_slice.is_empty() {
        return 0.0;
    }

    let avg_volume: f64 = volume_slice.iter().map(|c| c.v).sum::<f64>() / volume_slice.len() as f64;

    if avg_volume > 0.0 {
        current_volume / avg_volume
    } else {
        0.0
    }
}

/// 计算信号强度（0.0 - 1.0）
fn calculate_signal_strength(
    shadow_ratio: f64,
    volume_ratio: f64,
    min_shadow_ratio: f64,
    min_volume_ratio: f64,
) -> f64 {
    // 影线强度：超过最小要求越多越强
    let shadow_strength = if shadow_ratio >= min_shadow_ratio {
        ((shadow_ratio - min_shadow_ratio) / min_shadow_ratio).min(1.0) * 0.5 + 0.5
    } else {
        0.0
    };

    // 成交量强度
    let volume_strength = if volume_ratio >= min_volume_ratio {
        ((volume_ratio - min_volume_ratio) / min_volume_ratio).min(1.0) * 0.5 + 0.5
    } else {
        volume_ratio / min_volume_ratio * 0.5
    };

    // 综合强度
    (shadow_strength * 0.6 + volume_strength * 0.4).min(1.0)
}

/// 假突破增强检测（带确认K线）
///
/// 在基础假突破检测的基础上，增加确认条件：
/// - 看涨假突破后，下一根K线收阴
/// - 看跌假突破后，下一根K线收阳
pub fn detect_confirmed_fake_breakout(
    candles: &[CandleItem],
    config: &FakeBreakoutConfig,
) -> FakeBreakoutSignal {
    // 需要至少2根K线来确认
    if candles.len() < config.lookback_bars + 2 {
        return FakeBreakoutSignal::default();
    }

    // 检测前一根K线的假突破
    let signal_candles = &candles[..candles.len() - 1];
    let mut signal = detect_fake_breakout(signal_candles, config);

    if !signal.has_signal() {
        return signal;
    }

    // 获取确认K线
    let confirm_candle = candles.last().expect("candles should not be empty");
    let is_bullish_confirm = confirm_candle.c > confirm_candle.o; // 阳线
    let is_bearish_confirm = confirm_candle.c < confirm_candle.o; // 阴线

    // 看涨假突破需要阴线确认
    if signal.is_bearish_fake_breakout && !is_bearish_confirm {
        signal.is_bearish_fake_breakout = false;
        signal.strength *= 0.5; // 降低强度
    }

    // 看跌假突破需要阳线确认
    if signal.is_bullish_fake_breakout && !is_bullish_confirm {
        signal.is_bullish_fake_breakout = false;
        signal.strength *= 0.5;
    }

    signal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_candle(o: f64, h: f64, l: f64, c: f64, v: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            v,
            ts,
            ..Default::default()
        }
    }

    #[test]
    fn test_bearish_fake_breakout() {
        // 构造看涨假突破场景：价格突破前高后回落
        let mut candles = vec![];

        // 前20根K线，高点在100
        for i in 0..20 {
            candles.push(create_candle(95.0, 100.0, 94.0, 96.0, 1000.0, i));
        }

        // 当前K线：最高价突破前高(102)，但收盘价回落(97)，上影线长
        candles.push(create_candle(98.0, 102.0, 97.0, 97.5, 1500.0, 20));

        let config = FakeBreakoutConfig::default();
        let signal = detect_fake_breakout(&candles, &config);

        assert!(signal.is_bearish_fake_breakout);
        assert!(!signal.is_bullish_fake_breakout);
        assert!(signal.volume_confirmed);
    }

    #[test]
    fn test_bullish_fake_breakout() {
        // 构造看跌假突破场景：价格跌破前低后回升
        let mut candles = vec![];

        // 前20根K线，低点在90
        for i in 0..20 {
            candles.push(create_candle(95.0, 100.0, 90.0, 96.0, 1000.0, i));
        }

        // 当前K线：最低价跌破前低(88)，但收盘价回升(92)，下影线长
        candles.push(create_candle(91.0, 93.0, 88.0, 92.0, 1500.0, 20));

        let config = FakeBreakoutConfig::default();
        let signal = detect_fake_breakout(&candles, &config);

        assert!(signal.is_bullish_fake_breakout);
        assert!(!signal.is_bearish_fake_breakout);
        assert!(signal.volume_confirmed);
    }

    #[test]
    fn test_no_fake_breakout() {
        // 正常突破，不是假突破
        let mut candles = vec![];

        for i in 0..20 {
            candles.push(create_candle(95.0, 100.0, 94.0, 96.0, 1000.0, i));
        }

        // 突破后收盘价保持在前高上方
        candles.push(create_candle(99.0, 105.0, 98.0, 104.0, 1500.0, 20));

        let config = FakeBreakoutConfig::default();
        let signal = detect_fake_breakout(&candles, &config);

        assert!(!signal.is_bearish_fake_breakout);
        assert!(!signal.is_bullish_fake_breakout);
    }
}
