use super::types::KeltnerChannelScalperThresholds;
use crate::CandleItem;

/// Keltner Channel 计算结果，供 re-entry snapshot 构造复用。
#[derive(Debug, Clone, Copy)]
pub(super) struct KeltnerBands {
    pub(super) basis: f64,
    pub(super) atr: f64,
    pub(super) inner_upper: f64,
    pub(super) inner_lower: f64,
    pub(super) outer_upper: f64,
    pub(super) outer_lower: f64,
}

/// 计算指定 end 位置的 EMA basis、ATR 和内外层 Keltner 通道。
pub(super) fn keltner_at(
    candles: &[CandleItem],
    end: usize,
    thresholds: &KeltnerChannelScalperThresholds,
) -> Option<KeltnerBands> {
    let length = thresholds.keltner_length;
    if length == 0 || end < length + 1 || end > candles.len() {
        return None;
    }
    let closes = candles[..end]
        .iter()
        .map(|candle| candle.c)
        .collect::<Vec<_>>();
    let basis = ema_at(&closes, length)?;
    let atr = atr_sma_at(candles, end, length)?;
    Some(KeltnerBands {
        basis,
        atr,
        inner_upper: basis + atr * thresholds.inner_multiplier,
        inner_lower: basis - atr * thresholds.inner_multiplier,
        outer_upper: basis + atr * thresholds.outer_multiplier,
        outer_lower: basis - atr * thresholds.outer_multiplier,
    })
}

/// 将 EMA basis 相对 trend length 前的变化归一化为 ATR 倍数。
pub(super) fn basis_slope_atr(
    candles: &[CandleItem],
    current_bands: &KeltnerBands,
    thresholds: &KeltnerChannelScalperThresholds,
) -> Option<f64> {
    if current_bands.atr <= f64::EPSILON {
        return Some(0.0);
    }
    let lookback = thresholds.adx_trend_length.max(1);
    let previous_end = candles.len().checked_sub(lookback)?;
    let previous_bands = keltner_at(candles, previous_end, thresholds)?;
    Some((current_bands.basis - previous_bands.basis) / current_bands.atr)
}

/// 计算当前 end 位置的 ADX，trend length 和 smoothing 分开传入以匹配策略配置。
pub(super) fn adx_at(
    candles: &[CandleItem],
    end: usize,
    trend_length: usize,
    smoothing: usize,
) -> Option<f64> {
    if trend_length == 0 || smoothing == 0 || end < trend_length + smoothing + 1 {
        return None;
    }
    let mut tr_values = Vec::with_capacity(end.saturating_sub(1));
    let mut plus_dm_values = Vec::with_capacity(end.saturating_sub(1));
    let mut minus_dm_values = Vec::with_capacity(end.saturating_sub(1));
    for index in 1..end {
        let current = &candles[index];
        let previous = &candles[index - 1];
        let up_move = current.h - previous.h;
        let down_move = previous.l - current.l;
        tr_values.push(true_range(candles, index)?);
        plus_dm_values.push(if up_move > down_move && up_move > 0.0 {
            up_move
        } else {
            0.0
        });
        minus_dm_values.push(if down_move > up_move && down_move > 0.0 {
            down_move
        } else {
            0.0
        });
    }
    if tr_values.len() < trend_length {
        return None;
    }

    let mut smooth_tr = tr_values[..trend_length].iter().sum::<f64>();
    let mut smooth_plus = plus_dm_values[..trend_length].iter().sum::<f64>();
    let mut smooth_minus = minus_dm_values[..trend_length].iter().sum::<f64>();
    let mut dx_values = Vec::with_capacity(tr_values.len() - trend_length + 1);
    dx_values.push(dx_from_smoothed(smooth_tr, smooth_plus, smooth_minus));
    for index in trend_length..tr_values.len() {
        smooth_tr = smooth_tr - smooth_tr / trend_length as f64 + tr_values[index];
        smooth_plus = smooth_plus - smooth_plus / trend_length as f64 + plus_dm_values[index];
        smooth_minus = smooth_minus - smooth_minus / trend_length as f64 + minus_dm_values[index];
        dx_values.push(dx_from_smoothed(smooth_tr, smooth_plus, smooth_minus));
    }
    if dx_values.len() < smoothing {
        return None;
    }
    let mut adx = dx_values[..smoothing].iter().sum::<f64>() / smoothing as f64;
    for &dx in &dx_values[smoothing..] {
        adx = (adx * (smoothing as f64 - 1.0) + dx) / smoothing as f64;
    }
    Some(adx)
}

/// 计算完整切片末端的 EMA 值。
fn ema_at(values: &[f64], period: usize) -> Option<f64> {
    if period == 0 || values.len() < period {
        return None;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut ema = values[0];
    for &value in &values[1..] {
        ema = alpha * value + (1.0 - alpha) * ema;
    }
    Some(ema)
}

/// 计算指定 end 位置的简单均值 ATR。
fn atr_sma_at(candles: &[CandleItem], end: usize, period: usize) -> Option<f64> {
    if period == 0 || end < period + 1 || end > candles.len() {
        return None;
    }
    let start = end - period;
    let sum = (start..end)
        .map(|index| true_range(candles, index))
        .sum::<Option<f64>>()?;
    Some(sum / period as f64)
}

/// 根据平滑 TR 和方向移动量计算单点 DX。
fn dx_from_smoothed(tr: f64, plus_dm: f64, minus_dm: f64) -> f64 {
    if tr <= 0.0 {
        return 0.0;
    }
    let plus_di = 100.0 * plus_dm / tr;
    let minus_di = 100.0 * minus_dm / tr;
    let total = plus_di + minus_di;
    if total <= 0.0 {
        0.0
    } else {
        100.0 * (plus_di - minus_di).abs() / total
    }
}

/// 计算单根 K 线的 true range，第一根使用自身收盘价作为前收。
fn true_range(candles: &[CandleItem], index: usize) -> Option<f64> {
    let candle = candles.get(index)?;
    let prev_close = if index == 0 {
        candle.c
    } else {
        candles.get(index - 1)?.c
    };
    Some(
        (candle.h - candle.l)
            .max((candle.h - prev_close).abs())
            .max((candle.l - prev_close).abs()),
    )
}
