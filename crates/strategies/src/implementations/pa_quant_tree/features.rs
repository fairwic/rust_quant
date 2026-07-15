use super::{PaBlocker, PaDirection, PaFeatureSnapshot, PaMarketRegime};
use crate::CandleItem;
use rust_quant_indicators::ATR;
use ta::indicators::ExponentialMovingAverage;
use ta::Next;

/// v1 特征与候选所需的最小已确认 K 线数量。
pub const PA_MIN_CANDLES: usize = 100;

/// 从信号时点及以前的已确认 K 线构建 PA 特征快照。
pub fn calculate_pa_features(candles: &[CandleItem]) -> Result<PaFeatureSnapshot, PaBlocker> {
    if candles.len() < PA_MIN_CANDLES || candles.iter().any(|candle| candle.confirm != 1) {
        return Err(PaBlocker::DataNotReady);
    }
    if candles.iter().any(|candle| !is_valid_candle(candle)) {
        return Err(PaBlocker::InvalidCandle);
    }

    // 只读取固定尾部窗口，使历史前缀决策不会受后来追加的 K 线影响。
    let window = &candles[candles.len() - PA_MIN_CANDLES..];
    let mut atr = ATR::new(14).map_err(|_| PaBlocker::DataNotReady)?;
    let mut ema = ExponentialMovingAverage::new(20).map_err(|_| PaBlocker::DataNotReady)?;
    let mut ema_values = Vec::with_capacity(window.len());
    let mut atr14 = 0.0;
    for candle in window {
        atr14 = atr.next(candle.h, candle.l, candle.c);
        ema_values.push(ema.next(candle.c));
    }
    if atr14 <= 0.0 || !atr14.is_finite() {
        return Err(PaBlocker::DataNotReady);
    }

    let last = window.last().ok_or(PaBlocker::DataNotReady)?;
    let ema20 = *ema_values.last().ok_or(PaBlocker::DataNotReady)?;
    let ema_slope = (ema20 - ema_values[ema_values.len() - 6]) / atr14;
    let recent20 = &window[window.len() - 20..];
    let range_high = recent20
        .iter()
        .map(|c| c.h)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_low = recent20.iter().map(|c| c.l).fold(f64::INFINITY, f64::min);
    let range_width = range_high - range_low;
    let range_position = if range_width > 0.0 {
        ((last.c - range_low) / range_width).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let efficiency = range_efficiency(recent20);
    let overlap = mean_overlap_ratio(&window[window.len() - 8..]);
    let trend_direction = if ema_slope > 0.05 && last.c > ema20 {
        Some(PaDirection::Long)
    } else if ema_slope < -0.05 && last.c < ema20 {
        Some(PaDirection::Short)
    } else {
        None
    };
    let regime = if efficiency <= 0.35 && range_width >= 3.0 * atr14 && ema_slope.abs() <= 0.25 {
        PaMarketRegime::Range
    } else if trend_direction.is_some() && efficiency > 0.35 {
        PaMarketRegime::Trend
    } else {
        PaMarketRegime::Chaos
    };
    let recent3_start = window.len() - 3;
    let recent3 = &window[recent3_start..];
    let recent_ema_touch = recent3.iter().enumerate().any(|(offset, candle)| {
        let ema_value = ema_values[recent3_start + offset];
        candle.l <= ema_value && candle.h >= ema_value
    });
    let pullback_depth = match trend_direction {
        Some(PaDirection::Long) => recent3
            .iter()
            .enumerate()
            .map(|(offset, candle)| (ema_values[recent3_start + offset] - candle.l) / atr14)
            .fold(0.0, f64::max),
        Some(PaDirection::Short) => recent3
            .iter()
            .enumerate()
            .map(|(offset, candle)| (candle.h - ema_values[recent3_start + offset]) / atr14)
            .fold(0.0, f64::max),
        None => 0.0,
    };
    let close_position = (last.c - last.l) / (last.h - last.l);
    let directional_reclaim_atr = match trend_direction {
        Some(PaDirection::Long) => (last.c - ema20) / atr14,
        Some(PaDirection::Short) => (ema20 - last.c) / atr14,
        None => 0.0,
    };
    let directional_close_strength = match trend_direction {
        Some(PaDirection::Long) => close_position,
        Some(PaDirection::Short) => 1.0 - close_position,
        None => 0.5,
    };
    let pullback_close_fraction_3 = match trend_direction {
        Some(PaDirection::Long) => {
            recent3
                .iter()
                .enumerate()
                .filter(|(offset, candle)| candle.c <= ema_values[recent3_start + *offset])
                .count() as f64
                / recent3.len() as f64
        }
        Some(PaDirection::Short) => {
            recent3
                .iter()
                .enumerate()
                .filter(|(offset, candle)| candle.c >= ema_values[recent3_start + *offset])
                .count() as f64
                / recent3.len() as f64
        }
        None => 0.0,
    };

    Ok(PaFeatureSnapshot {
        signal_ts: last.ts,
        atr14,
        ema20,
        ema_slope_atr_20_5: ema_slope,
        range_efficiency_20: efficiency,
        range_high_20: range_high,
        range_low_20: range_low,
        range_position_20: range_position,
        mean_overlap_ratio_8: overlap,
        always_in_score: always_in_score(window, &ema_values, trend_direction),
        signal_body_ratio: body_ratio(last),
        close_position,
        pullback_depth_atr_3: pullback_depth,
        directional_reclaim_atr,
        directional_close_strength,
        signal_range_atr: (last.h - last.l) / atr14,
        pullback_close_fraction_3,
        recent_ema_touch,
        regime,
        trend_direction,
    })
}

fn is_valid_candle(candle: &CandleItem) -> bool {
    [candle.o, candle.h, candle.l, candle.c, candle.v]
        .iter()
        .all(|value| value.is_finite())
        && candle.l >= 0.0
        && candle.v >= 0.0
        && candle.h > candle.l
        && candle.l <= candle.o
        && candle.l <= candle.c
        && candle.h >= candle.o
        && candle.h >= candle.c
}

fn body_ratio(candle: &CandleItem) -> f64 {
    (candle.c - candle.o).abs() / (candle.h - candle.l)
}

fn range_efficiency(candles: &[CandleItem]) -> f64 {
    let movement: f64 = candles
        .windows(2)
        .map(|pair| (pair[1].c - pair[0].c).abs())
        .sum();
    if movement == 0.0 {
        0.0
    } else {
        (candles.last().unwrap().c - candles.first().unwrap().c).abs() / movement
    }
}

fn mean_overlap_ratio(candles: &[CandleItem]) -> f64 {
    let total: f64 = candles
        .windows(2)
        .map(|pair| {
            let overlap = pair[0].h.min(pair[1].h) - pair[0].l.max(pair[1].l);
            let denominator = (pair[0].h - pair[0].l).min(pair[1].h - pair[1].l);
            if overlap > 0.0 {
                overlap / denominator
            } else {
                0.0
            }
        })
        .sum();
    total / (candles.len() - 1) as f64
}

fn always_in_score(
    candles: &[CandleItem],
    ema_values: &[f64],
    direction: Option<PaDirection>,
) -> f64 {
    let start = candles.len() - 10;
    let aligned = candles[start..]
        .iter()
        .zip(&ema_values[start..])
        .filter(|(candle, ema)| match direction {
            Some(PaDirection::Long) => candle.c > **ema,
            Some(PaDirection::Short) => candle.c < **ema,
            None => false,
        })
        .count();
    aligned as f64 / 10.0
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn trend_candles(count: usize) -> Vec<CandleItem> {
        (0..count)
            .map(|index| {
                // 缓坡让 EMA20 与价格保持可回踩距离，同时仍满足趋势效率与斜率门禁。
                let base = 100.0 + index as f64 * 0.1;
                let pullback = if index + 2 >= count { -0.8 } else { 0.0 };
                let open = base + pullback;
                let close = open + 0.25;
                CandleItem {
                    o: open,
                    h: close + 0.35,
                    l: open - 0.35,
                    c: close,
                    v: 10.0,
                    ts: index as i64,
                    confirm: 1,
                }
            })
            .collect()
    }

    #[test]
    fn rejects_unconfirmed_or_short_history() {
        assert_eq!(
            calculate_pa_features(&trend_candles(99)),
            Err(PaBlocker::DataNotReady)
        );
        let mut candles = trend_candles(100);
        candles[99].confirm = 0;
        assert_eq!(
            calculate_pa_features(&candles),
            Err(PaBlocker::DataNotReady)
        );
    }

    #[test]
    fn future_candles_do_not_change_prefix_features() {
        let candles = trend_candles(104);
        let before = calculate_pa_features(&candles[..100]).unwrap();
        let after = calculate_pa_features(&candles[..100]).unwrap();
        assert_eq!(before, after);
        assert_eq!(before.signal_ts, 99);
        assert!(before.directional_reclaim_atr.is_finite());
        assert!(before.directional_close_strength > 0.5);
        assert!(before.signal_range_atr > 0.0);
        assert!((0.0..=1.0).contains(&before.pullback_close_fraction_3));
    }
}
