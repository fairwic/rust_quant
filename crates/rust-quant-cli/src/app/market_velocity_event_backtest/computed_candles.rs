use super::{BacktestCandle, ComputedCandle};
use rust_quant_indicators::momentum::macd::MacdSimpleIndicator;
use rust_quant_indicators::volatility::ATR;

pub(crate) const FAST_MOMENTUM_RSI_PERIOD: usize = 14;
pub(crate) const FAST_MOMENTUM_ATR_PERIOD: usize = 14;
pub(crate) const FAST_MOMENTUM_BOLLINGER_PERIOD: usize = 20;
const FAST_MOMENTUM_MACD_FAST_PERIOD: usize = 12;
const FAST_MOMENTUM_MACD_SLOW_PERIOD: usize = 26;
const FAST_MOMENTUM_MACD_SIGNAL_PERIOD: usize = 9;
pub(crate) const FILTERED_VOLUME_EMA_FAST_PERIOD: usize = 12;
pub(crate) const FILTERED_VOLUME_EMA_MIDDLE_PERIOD: usize = 144;
pub(crate) const FILTERED_VOLUME_EMA_SECOND_MIDDLE_PERIOD: usize = 169;
pub(crate) const FILTERED_VOLUME_EMA_SLOW_PERIOD: usize = 696;
const FAST_MOMENTUM_BOLLINGER_STDDEV: f64 = 2.0;

/// 由一个完整收盘价窗口冻结的布林带数值；策略只能组合这些数值，不能在指标层产生方向。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BollingerBandsSnapshot {
    /// 窗口收盘价的算术平均值。
    pub(super) middle: f64,
    /// 中轨加标准差倍数后的上轨。
    pub(super) upper: f64,
    /// 中轨减标准差倍数后的下轨。
    pub(super) lower: f64,
}

/// 构建 15m/高周期派生 K 线指标，供入场、趋势确认和 paper observation 共用。
pub fn build_computed_candles(candles: Vec<BacktestCandle>, period: usize) -> Vec<ComputedCandle> {
    let mut computed = Vec::with_capacity(candles.len());
    let mut rsi_average_gain_loss: Option<(f64, f64)> = None;
    let mut atr = ATR::new(FAST_MOMENTUM_ATR_PERIOD)
        .expect("FAST_MOMENTUM_ATR_PERIOD is a positive constant");
    let ema_values = ema_close_series(&candles, period);
    let ema12_values = ema_close_series(&candles, FILTERED_VOLUME_EMA_FAST_PERIOD);
    let ema144_values = ema_close_series(&candles, FILTERED_VOLUME_EMA_MIDDLE_PERIOD);
    let ema169_values = ema_close_series(&candles, FILTERED_VOLUME_EMA_SECOND_MIDDLE_PERIOD);
    let ema696_values = ema_close_series(&candles, FILTERED_VOLUME_EMA_SLOW_PERIOD);
    let macd_values = MacdSimpleIndicator::calculate_close_series(
        candles.iter().map(|candle| candle.close),
        FAST_MOMENTUM_MACD_FAST_PERIOD,
        FAST_MOMENTUM_MACD_SLOW_PERIOD,
        FAST_MOMENTUM_MACD_SIGNAL_PERIOD,
    )
    .unwrap_or_else(|| vec![None; candles.len()]);
    for i in 0..candles.len() {
        let sma = if i + 1 >= period {
            simple_average(
                candles[i + 1 - period..=i]
                    .iter()
                    .map(|candle| candle.close),
            )
        } else {
            None
        };
        let ema = ema_values[i];
        let previous_volume_avg = if i >= period {
            simple_average(candles[i - period..i].iter().map(|candle| candle.volume))
        } else {
            None
        };
        let previous_range_avg = if i >= period {
            simple_average(candles[i - period..i].iter().map(candle_range))
        } else {
            None
        };
        let rsi14 = rsi_average_gain_loss_at(&candles, i, &mut rsi_average_gain_loss).and_then(
            |(average_gain, average_loss)| rsi_from_average_gain_loss(average_gain, average_loss),
        );
        let atr14 = atr.next(candles[i].high, candles[i].low, candles[i].close);
        let atr14 = valid_positive(atr14).then_some(atr14);
        let (bollinger_middle, bollinger_upper, bollinger_lower, bollinger_bandwidth_pct) =
            bollinger_bands_at(&candles, i)
                .map(|bands| (Some(bands.0), Some(bands.1), Some(bands.2), bands.3))
                .unwrap_or((None, None, None, None));
        let macd = macd_values.get(i).copied().flatten();
        computed.push(ComputedCandle {
            candle: candles[i].clone(),
            volume_ccy: None,
            sma,
            ema,
            ema12: ema12_values[i],
            ema144: ema144_values[i],
            ema169: ema169_values[i],
            ema696: ema696_values[i],
            previous_volume_avg,
            previous_range_avg,
            rsi14,
            atr14,
            bollinger_middle,
            bollinger_upper,
            bollinger_lower,
            bollinger_bandwidth_pct,
            macd_line: macd.map(|value| value.macd_line),
            macd_signal_line: macd.map(|value| value.signal_line),
            macd_histogram: macd.map(|value| value.histogram),
        });
    }
    computed
}

/// 以首个完整窗口的 SMA 为种子计算 EMA，保证不同周期共享一致的预热语义。
fn ema_close_series(candles: &[BacktestCandle], period: usize) -> Vec<Option<f64>> {
    let mut values = vec![None; candles.len()];
    if period == 0 || candles.len() < period {
        return values;
    }
    let seed_idx = period - 1;
    let Some(mut previous) = simple_average(candles[..period].iter().map(|candle| candle.close))
    else {
        return values;
    };
    values[seed_idx] = Some(previous);
    let multiplier = 2.0 / (period as f64 + 1.0);
    for idx in period..candles.len() {
        if !valid_positive(candles[idx].close) {
            break;
        }
        previous = (candles[idx].close - previous) * multiplier + previous;
        values[idx] = Some(previous);
    }
    values
}

/// 按 Wilder RSI 的平滑方式维护 RSI14 的平均涨跌幅，避免每根 K 线重复扫描历史。
fn rsi_average_gain_loss_at(
    candles: &[BacktestCandle],
    idx: usize,
    previous_average: &mut Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    if idx < FAST_MOMENTUM_RSI_PERIOD {
        return None;
    }
    if idx == FAST_MOMENTUM_RSI_PERIOD {
        let mut gain_sum = 0.0;
        let mut loss_sum = 0.0;
        for window_idx in 1..=FAST_MOMENTUM_RSI_PERIOD {
            let delta = candles[window_idx].close - candles[window_idx - 1].close;
            if !delta.is_finite() {
                return None;
            }
            if delta >= 0.0 {
                gain_sum += delta;
            } else {
                loss_sum += delta.abs();
            }
        }
        let average = (
            gain_sum / FAST_MOMENTUM_RSI_PERIOD as f64,
            loss_sum / FAST_MOMENTUM_RSI_PERIOD as f64,
        );
        *previous_average = Some(average);
        return Some(average);
    }
    let (previous_gain, previous_loss) = (*previous_average)?;
    let delta = candles[idx].close - candles[idx - 1].close;
    if !delta.is_finite() {
        *previous_average = None;
        return None;
    }
    let gain = delta.max(0.0);
    let loss = (-delta).max(0.0);
    let period = FAST_MOMENTUM_RSI_PERIOD as f64;
    let average = (
        (previous_gain * (period - 1.0) + gain) / period,
        (previous_loss * (period - 1.0) + loss) / period,
    );
    *previous_average = Some(average);
    Some(average)
}

fn candle_range(candle: &BacktestCandle) -> f64 {
    candle.high - candle.low
}

/// 将 RSI 的平均涨跌幅转换为 0-100 分值，并处理单边上涨或无波动样本。
fn rsi_from_average_gain_loss(average_gain: f64, average_loss: f64) -> Option<f64> {
    if !average_gain.is_finite()
        || !average_loss.is_finite()
        || average_gain < 0.0
        || average_loss < 0.0
    {
        return None;
    }
    if average_loss == 0.0 {
        return Some(if average_gain == 0.0 { 50.0 } else { 100.0 });
    }
    let relative_strength = average_gain / average_loss;
    Some(100.0 - 100.0 / (1.0 + relative_strength))
}

/// 计算 20 期布林带和带宽，用于 15m 内生突破过滤而不是依赖高周期均线。
fn bollinger_bands_at(
    candles: &[BacktestCandle],
    idx: usize,
) -> Option<(f64, f64, f64, Option<f64>)> {
    if idx + 1 < FAST_MOMENTUM_BOLLINGER_PERIOD {
        return None;
    }
    let start = idx + 1 - FAST_MOMENTUM_BOLLINGER_PERIOD;
    let closes = candles[start..=idx]
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let bands = bollinger_bands_from_closes(&closes, FAST_MOMENTUM_BOLLINGER_STDDEV)?;
    let bandwidth =
        valid_positive(bands.middle).then_some((bands.upper - bands.lower) / bands.middle * 100.0);
    Some((bands.middle, bands.upper, bands.lower, bandwidth))
}

/// 使用总体标准差计算一个完整收盘价窗口，供不同策略参数复用同一布林带口径。
pub(super) fn bollinger_bands_from_closes(
    closes: &[f64],
    standard_deviation_multiplier: f64,
) -> Option<BollingerBandsSnapshot> {
    if closes.is_empty()
        || !standard_deviation_multiplier.is_finite()
        || standard_deviation_multiplier <= 0.0
    {
        return None;
    }
    let middle = simple_average(closes.iter().copied())?;
    let variance = closes
        .iter()
        .map(|close| (close - middle).powi(2))
        .sum::<f64>()
        / closes.len() as f64;
    if !variance.is_finite() || variance < 0.0 {
        return None;
    }
    let deviation = variance.sqrt() * standard_deviation_multiplier;
    let upper = middle + deviation;
    let lower = middle - deviation;
    (upper.is_finite() && lower.is_finite()).then_some(BollingerBandsSnapshot {
        middle,
        upper,
        lower,
    })
}

fn simple_average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut count = 0;
    let mut sum = 0.0;
    for value in values {
        if !valid_positive(value) {
            return None;
        }
        count += 1;
        sum += value;
    }
    (count > 0).then_some(sum / count as f64)
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
