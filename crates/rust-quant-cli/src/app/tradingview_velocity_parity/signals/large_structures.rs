use super::super::model::{Candle, IndicatorSeries};
use super::super::ranges::{longest_large_ascending_triangle, longest_large_horizontal_range};

/// 先验证突破棒动量与 EMA 门禁，再在冻结窗口集合中确认大型水平箱体。
#[allow(clippy::too_many_arguments)]
pub(super) fn horizontal_signal(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    atr: f64,
    take_profit_atr: Option<f64>,
    ema12: f64,
    ema144: f64,
    ema696: f64,
    rsi: f64,
) -> Option<(f64, f64)> {
    let previous = index
        .checked_sub(1)
        .and_then(|previous| indicators.get(previous))?;
    let gate = indicators.points[index].volume_event
        && take_profit_atr.is_some()
        && candles[index].close > candles[index].open
        && previous.rsi14? < 70.0
        && (70.0..=85.0).contains(&rsi)
        && ema12 > ema144
        && previous.ema12? <= previous.ema144?
        && candles[index].close > ema696;
    gate.then(|| {
        longest_large_horizontal_range(candles, index, atr, candles[index].close).map(|range| {
            (
                range.raw_high,
                take_profit_atr.expect("gate requires target"),
            )
        })
    })
    .flatten()
}

/// 先验证突破棒动量与 EMA 门禁，再确认大型上升三角的水平压力突破。
#[allow(clippy::too_many_arguments)]
pub(super) fn triangle_signal(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    atr: f64,
    take_profit_atr: Option<f64>,
    ema12: f64,
    ema144: f64,
    ema696: f64,
    rsi: f64,
) -> Option<(f64, f64)> {
    let previous = index
        .checked_sub(1)
        .and_then(|previous| indicators.get(previous))?;
    let gate = indicators.points[index].volume_event
        && take_profit_atr.is_some()
        && candles[index].close > candles[index].open
        && previous.rsi14? < 70.0
        && (70.0..=80.0).contains(&rsi)
        && ema12 > ema144
        && previous.ema12? <= previous.ema144?
        && ema144 > ema696;
    gate.then(|| {
        longest_large_ascending_triangle(candles, index, atr, candles[index].close).map(
            |triangle| {
                (
                    triangle.resistance,
                    take_profit_atr.expect("gate requires target"),
                )
            },
        )
    })
    .flatten()
}
