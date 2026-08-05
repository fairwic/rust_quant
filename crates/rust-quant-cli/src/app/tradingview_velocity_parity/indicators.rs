use super::model::{Candle, IndicatorPoint, IndicatorSeries, ParityRuleVersion};

const VOLUME_LOOKBACK: usize = 10;
const MIN_VALID_VOLUME_SAMPLES: usize = 5;
const WEEKLY_LOOKBACK: usize = 672;
const VOLUME_EVENT_RATIO: f64 = 2.5;

/// 按冻结 Pine 的初始化方式一次性计算全部基础指标。
///
/// 这里不复用仓库内旧指标对象：旧实现的 EMA/ATR 种子语义与 Pine 不完全相同，
/// 继续复用会把差异隐藏在信号层之后。
pub fn compute_indicators(candles: &[Candle], rule_version: ParityRuleVersion) -> IndicatorSeries {
    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();
    let volumes: Vec<f64> = candles.iter().map(|candle| candle.volume).collect();
    let true_ranges = true_ranges(candles);
    // V1～V8 必须保留原始 596/696 与 2.0，否则历史报告会在相同版本名下漂移。
    // 语义字段名暂时沿用 ema596/ema696；V9/V10 只改变其周期值，避免无关的大范围迁移。
    let (ema_structure_length, ema_regime_length, bollinger_multiplier) = if matches!(
        rule_version,
        ParityRuleVersion::CandidateV9
            | ParityRuleVersion::CandidateV10
            | ParityRuleVersion::CandidateV11
            | ParityRuleVersion::CandidateV12
            | ParityRuleVersion::CandidateV13
            | ParityRuleVersion::CandidateV14
            | ParityRuleVersion::CandidateV15
            | ParityRuleVersion::CandidateV16
            | ParityRuleVersion::CandidateV17
            | ParityRuleVersion::CandidateV18
            | ParityRuleVersion::CandidateV19
            | ParityRuleVersion::CandidateV20
    ) {
        (576, 676, 2.5)
    } else {
        (596, 696, 2.0)
    };
    let ema12 = pine_ema(&closes, 12);
    let ema144 = pine_ema(&closes, 144);
    let ema596 = pine_ema(&closes, ema_structure_length);
    let ema696 = pine_ema(&closes, ema_regime_length);
    let ema26 = pine_ema(&closes, 26);
    let atr14 = pine_rma(&true_ranges, 14);
    let rsi14 = pine_rsi(&closes, 14);
    let bollinger_middle = rolling_sma(&closes, 20);
    let bollinger_deviation = rolling_population_stddev(&closes, 20);
    let macd_line: Vec<f64> = ema12
        .iter()
        .zip(&ema26)
        .map(|(fast, slow)| fast.unwrap_or_default() - slow.unwrap_or_default())
        .collect();
    let macd_signal = pine_ema(&macd_line, 9);

    let mut raw_volume_spikes = vec![false; candles.len()];
    for index in VOLUME_LOOKBACK..candles.len() {
        let average = mean(&volumes[index - VOLUME_LOOKBACK..index]);
        raw_volume_spikes[index] = average > 0.0 && volumes[index] >= VOLUME_EVENT_RATIO * average;
    }

    let mut points = Vec::with_capacity(candles.len());
    for index in 0..candles.len() {
        let filtered_volume_ratio = filtered_volume_ratio(
            index,
            &volumes,
            &raw_volume_spikes,
            VOLUME_LOOKBACK,
            MIN_VALID_VOLUME_SAMPLES,
        );
        let weekly_slice = index
            .checked_sub(WEEKLY_LOOKBACK)
            .map(|start| &volumes[start..index]);
        let weekly_p80 = weekly_slice.and_then(|values| nearest_rank(values, 80.0));
        let weekly_p90 = weekly_slice.and_then(|values| nearest_rank(values, 90.0));
        let weekly_min_non_negative = weekly_slice
            .map(|values| values.iter().all(|value| *value >= 0.0))
            .unwrap_or(false);
        let weekly_ready = index >= WEEKLY_LOOKBACK + VOLUME_LOOKBACK
            && weekly_p90.is_some()
            && weekly_min_non_negative;
        let volume_event = filtered_volume_ratio
            .zip(weekly_p90)
            .is_some_and(|(ratio, p90)| {
                ratio >= VOLUME_EVENT_RATIO
                    && weekly_ready
                    && volumes[index] > 0.0
                    && volumes[index] >= p90
            });

        let bollinger_range =
            bollinger_deviation[index].map(|deviation| bollinger_multiplier * deviation);
        let macd_histogram = macd_signal[index].map(|signal| macd_line[index] - signal);
        points.push(IndicatorPoint {
            filtered_volume_ratio,
            volume_event,
            weekly_volume_p80: weekly_p80,
            weekly_volume_p90: weekly_p90,
            weekly_volume_ready: weekly_ready,
            rsi14: rsi14[index],
            ema12: ema12[index],
            ema144: ema144[index],
            ema596: ema596[index],
            ema696: ema696[index],
            atr14: atr14[index],
            bollinger_middle: bollinger_middle[index],
            bollinger_upper: bollinger_middle[index]
                .zip(bollinger_range)
                .map(|(middle, range)| middle + range),
            bollinger_lower: bollinger_middle[index]
                .zip(bollinger_range)
                .map(|(middle, range)| middle - range),
            macd_histogram,
        });
    }

    IndicatorSeries { points }
}

/// Pine `ta.crossover(a, b)` 的已完成 K 线等价判断。
pub fn crossover(current_a: f64, current_b: f64, previous_a: f64, previous_b: f64) -> bool {
    current_a > current_b && previous_a <= previous_b
}

/// 返回 Pine `ta.sma` 风格的滚动均值；历史不足时保持 `None`。
pub fn rolling_sma(values: &[f64], length: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; values.len()];
    if length == 0 {
        return result;
    }

    let mut sum = 0.0;
    for (index, value) in values.iter().enumerate() {
        sum += value;
        if index >= length {
            sum -= values[index - length];
        }
        if index + 1 >= length {
            result[index] = Some(sum / length as f64);
        }
    }
    result
}

/// Pine `ta.stdev(source, length)` 默认使用总体标准差，窗口不足时保持 `None`。
fn rolling_population_stddev(values: &[f64], length: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; values.len()];
    if length == 0 || values.len() < length {
        return result;
    }
    for index in length - 1..values.len() {
        let window = &values[index + 1 - length..=index];
        let average = mean(window);
        let variance = window
            .iter()
            .map(|value| {
                let difference = value - average;
                difference * difference
            })
            .sum::<f64>()
            / length as f64;
        result[index] = Some(variance.sqrt());
    }
    result
}

/// Pine `array.percentile_nearest_rank` 的 nearest-rank 分位数。
pub fn nearest_rank(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((percentile.clamp(0.0, 100.0) / 100.0) * sorted.len() as f64)
        .ceil()
        .max(1.0) as usize;
    sorted.get(rank.saturating_sub(1)).copied()
}

/// Pine `ta.ema` 使用首个有效源值作为递归种子。
fn pine_ema(values: &[f64], length: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; values.len()];
    if values.is_empty() || length == 0 {
        return result;
    }
    let alpha = 2.0 / (length as f64 + 1.0);
    let mut previous = values[0];
    result[0] = Some(previous);
    for index in 1..values.len() {
        previous = alpha * values[index] + (1.0 - alpha) * previous;
        result[index] = Some(previous);
    }
    result
}

/// Pine `ta.rma`：先用首个完整窗口的 SMA 初始化，再按 Wilder 递归。
fn pine_rma(values: &[f64], length: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; values.len()];
    if length == 0 || values.len() < length {
        return result;
    }
    let seed = mean(&values[..length]);
    let seed_index = length - 1;
    result[seed_index] = Some(seed);
    let mut previous = seed;
    for index in length..values.len() {
        previous = (previous * (length as f64 - 1.0) + values[index]) / length as f64;
        result[index] = Some(previous);
    }
    result
}

/// Pine `ta.rsi` 的 Wilder 上涨/下跌均值实现。
fn pine_rsi(closes: &[f64], length: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; closes.len()];
    if closes.len() <= length {
        return result;
    }
    // Pine 的首根 `ta.change(close)` 为 `na`。RSI 的 RMA 因此从第一个真实
    // 价格变化开始累计，首个 RSI 出现在索引 `length`，而不是 `length - 1`。
    let changes: Vec<f64> = closes.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let gains: Vec<f64> = changes.iter().map(|change| change.max(0.0)).collect();
    let losses: Vec<f64> = changes.iter().map(|change| (-change).max(0.0)).collect();
    let average_gains = pine_rma(&gains, length);
    let average_losses = pine_rma(&losses, length);
    for (change_index, value) in average_gains
        .into_iter()
        .zip(average_losses)
        .map(|(gain, loss)| match (gain, loss) {
            (Some(gain), Some(loss)) if gain == 0.0 && loss == 0.0 => Some(50.0),
            (Some(_), Some(0.0)) => Some(100.0),
            (Some(0.0), Some(_)) => Some(0.0),
            (Some(gain), Some(loss)) => Some(100.0 - 100.0 / (1.0 + gain / loss)),
            _ => None,
        })
        .enumerate()
    {
        result[change_index + 1] = value;
    }
    result
}

/// 计算 Pine `ta.tr(true)` 使用的真实波幅序列。
fn true_ranges(candles: &[Candle]) -> Vec<f64> {
    candles
        .iter()
        .enumerate()
        .map(|(index, candle)| {
            if index == 0 {
                candle.high - candle.low
            } else {
                let previous_close = candles[index - 1].close;
                (candle.high - candle.low)
                    .max((candle.high - previous_close).abs())
                    .max((candle.low - previous_close).abs())
            }
        })
        .collect()
}

/// 量比基准排除历史原始尖峰，但不递归使用过滤后事件，保持与 Pine 窗口语义一致。
fn filtered_volume_ratio(
    index: usize,
    volumes: &[f64],
    raw_volume_spikes: &[bool],
    lookback: usize,
    minimum_samples: usize,
) -> Option<f64> {
    let start = index.saturating_sub(lookback);
    let mut sum = 0.0;
    let mut count = 0usize;
    for history_index in start..index {
        if !raw_volume_spikes[history_index] {
            sum += volumes[history_index];
            count += 1;
        }
    }
    (count >= minimum_samples && sum > 0.0).then(|| volumes[index] / (sum / count as f64))
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(index: usize, volume: f64) -> Candle {
        let close = 100.0 + index as f64;
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open: close - 0.5,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume,
        }
    }

    #[test]
    fn nearest_rank_matches_pine_rank_selection() {
        assert_eq!(nearest_rank(&[4.0, 1.0, 3.0, 2.0], 90.0), Some(4.0));
        assert_eq!(nearest_rank(&[4.0, 1.0, 3.0, 2.0], 50.0), Some(2.0));
    }

    #[test]
    fn rsi_seed_ignores_the_missing_first_change() {
        let closes: Vec<f64> = (0..=14).map(|index| 100.0 + index as f64).collect();
        let rsi = pine_rsi(&closes, 14);

        assert!(rsi[13].is_none());
        assert_eq!(rsi[14], Some(100.0));
    }

    #[test]
    fn filtered_ratio_excludes_prior_raw_spike_without_recursive_filtering() {
        let mut candles: Vec<Candle> = (0..700).map(|index| candle(index, 10.0)).collect();
        candles[695].volume = 100.0;
        candles[699].volume = 30.0;
        let series = compute_indicators(&candles, ParityRuleVersion::CandidateV8);

        assert_eq!(series.points[699].filtered_volume_ratio, Some(3.0));
        assert!(series.points[699].volume_event);
    }

    #[test]
    fn weekly_event_waits_for_the_full_prior_history() {
        let mut candles: Vec<Candle> = (0..700).map(|index| candle(index, 10.0)).collect();
        candles[681].volume = 100.0;
        candles[682].volume = 100.0;
        let series = compute_indicators(&candles, ParityRuleVersion::CandidateV8);

        assert!(!series.points[681].volume_event);
        assert!(series.points[682].volume_event);
    }

    #[test]
    fn research_p80_uses_the_same_frozen_prior_week_as_p90() {
        let candles: Vec<Candle> = (0..700)
            .map(|index| candle(index, index as f64 + 1.0))
            .collect();
        let series = compute_indicators(&candles, ParityRuleVersion::CandidateV19);
        let prior_week = candles[10..682]
            .iter()
            .map(|candle| candle.volume)
            .collect::<Vec<_>>();

        assert_eq!(
            series.points[682].weekly_volume_p80,
            nearest_rank(&prior_week, 80.0)
        );
        assert_eq!(
            series.points[682].weekly_volume_p90,
            nearest_rank(&prior_week, 90.0)
        );
        assert!(series.points[682].weekly_volume_p80 < series.points[682].weekly_volume_p90);
    }

    #[test]
    fn bollinger_uses_population_deviation_and_macd_is_available() {
        let candles: Vec<Candle> = (0..700).map(|index| candle(index, 10.0)).collect();
        let series = compute_indicators(&candles, ParityRuleVersion::CandidateV8);
        let point = &series.points[699];
        let middle = point.bollinger_middle.expect("20 closes are available");
        let expected_stddev = (33.25_f64).sqrt();

        assert!((middle - 789.5).abs() < 1e-12);
        assert!(
            (point.bollinger_upper.expect("upper band") - (middle + 2.0 * expected_stddev)).abs()
                < 1e-12
        );
        assert!(
            (point.bollinger_lower.expect("lower band") - (middle - 2.0 * expected_stddev)).abs()
                < 1e-12
        );
        assert!(point.macd_histogram.is_some());
    }

    #[test]
    fn v9_uses_new_slow_ema_periods_and_two_point_five_bollinger_width() {
        let candles: Vec<Candle> = (0..700).map(|index| candle(index, 10.0)).collect();
        let closes = candles
            .iter()
            .map(|candle| candle.close)
            .collect::<Vec<_>>();
        let legacy = compute_indicators(&candles, ParityRuleVersion::CandidateV8);
        let v9 = compute_indicators(&candles, ParityRuleVersion::CandidateV9);
        let v9_point = &v9.points[699];
        let middle = v9_point.bollinger_middle.expect("20 closes are available");
        let expected_stddev = (33.25_f64).sqrt();

        assert_eq!(legacy.points[699].ema596, pine_ema(&closes, 596)[699]);
        assert_eq!(legacy.points[699].ema696, pine_ema(&closes, 696)[699]);
        assert_eq!(v9_point.ema596, pine_ema(&closes, 576)[699]);
        assert_eq!(v9_point.ema696, pine_ema(&closes, 676)[699]);
        assert!(
            (v9_point.bollinger_upper.expect("upper band") - (middle + 2.5 * expected_stddev))
                .abs()
                < 1e-12
        );
        assert!(
            (v9_point.bollinger_lower.expect("lower band") - (middle - 2.5 * expected_stddev))
                .abs()
                < 1e-12
        );
    }
}
