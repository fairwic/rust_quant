use crate::app::tradingview_velocity_parity::model::Candle;

const RANGE_LENGTH: usize = 48;
const RECENT_COMPRESSION_LENGTH: usize = 5;
const MAX_WIDTH_RATIO: f64 = 0.03;
const TOUCH_BAND_RATIO: f64 = 0.0015;
const TOUCH_BAND_ATR: f64 = 0.35;
const MIN_TOUCH_GROUPS: usize = 2;
const MIN_TOUCH_GAP: usize = 2;
const MIN_CONTAINMENT: f64 = 0.80;
const MAX_BOUNDARY_DRIFT_RATIO: f64 = 0.003;
const MAX_TRUE_RANGE_CONTRACTION: f64 = 0.80;
const MAX_VOLUME_DRY_UP: f64 = 0.85;

/// V15 在突破棒之前冻结的真实箱体；EMA 接近不参与该结构定义。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeSqueezeBoxV15 {
    pub upper: f64,
    pub lower: f64,
    pub height: f64,
    pub median_volume: f64,
}

/// 只用 `index` 之前固定 48 根完成棒识别双侧触碰、收缩且缩量的真实箱体。
pub fn range_squeeze_box_v15(
    candles: &[Candle],
    index: usize,
    atr: f64,
) -> Option<RangeSqueezeBoxV15> {
    let start = index.checked_sub(RANGE_LENGTH)?;
    let history = candles.get(start..index)?;
    if atr <= 0.0 || history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }

    // 收缩与缩量是大多数窗口最先失败的廉价门禁；先做 O(48) 累加，
    // 避免为明显非横盘窗口反复分配并排序多个分位数数组。
    let older_length = RANGE_LENGTH - RECENT_COMPRESSION_LENGTH;
    let mut older_true_range_sum = 0.0;
    let mut recent_true_range_sum = 0.0;
    let mut older_volume_sum = 0.0;
    let mut recent_volume_sum = 0.0;
    for (offset, candle) in history.iter().enumerate() {
        let candle_true_range = true_range(candles, start + offset)?;
        if offset < older_length {
            older_true_range_sum += candle_true_range;
            older_volume_sum += candle.volume;
        } else {
            recent_true_range_sum += candle_true_range;
            recent_volume_sum += candle.volume;
        }
    }
    let older_true_range = older_true_range_sum / older_length as f64;
    let recent_true_range = recent_true_range_sum / RECENT_COMPRESSION_LENGTH as f64;
    let older_volume = older_volume_sum / older_length as f64;
    let recent_volume = recent_volume_sum / RECENT_COMPRESSION_LENGTH as f64;
    if older_true_range <= 0.0
        || recent_true_range / older_true_range > MAX_TRUE_RANGE_CONTRACTION
        || older_volume <= 0.0
        || recent_volume / older_volume > MAX_VOLUME_DRY_UP
    {
        return None;
    }

    let highs = history.iter().map(|candle| candle.high).collect::<Vec<_>>();
    let lows = history.iter().map(|candle| candle.low).collect::<Vec<_>>();
    let volumes = history
        .iter()
        .map(|candle| candle.volume)
        .collect::<Vec<_>>();
    let upper = nearest_rank(&highs, 0.85)?;
    let lower = nearest_rank(&lows, 0.15)?;
    let middle = (upper + lower) / 2.0;
    let height = upper - lower;
    if middle <= 0.0 || height <= 0.0 || height / middle > MAX_WIDTH_RATIO {
        return None;
    }

    let touch_band = (middle * TOUCH_BAND_RATIO).max(atr * TOUCH_BAND_ATR);
    if touch_groups(&highs, upper, touch_band) < MIN_TOUCH_GROUPS
        || touch_groups(&lows, lower, touch_band) < MIN_TOUCH_GROUPS
    {
        return None;
    }
    let containment = history
        .iter()
        .filter(|candle| candle.close >= lower && candle.close <= upper)
        .count() as f64
        / RANGE_LENGTH as f64;
    if containment < MIN_CONTAINMENT {
        return None;
    }

    let split = RANGE_LENGTH / 2;
    let older_upper = nearest_rank(&highs[..split], 0.85)?;
    let recent_upper = nearest_rank(&highs[split..], 0.85)?;
    let older_lower = nearest_rank(&lows[..split], 0.15)?;
    let recent_lower = nearest_rank(&lows[split..], 0.15)?;
    if (recent_upper - older_upper).abs() / middle > MAX_BOUNDARY_DRIFT_RATIO
        || (recent_lower - older_lower).abs() / middle > MAX_BOUNDARY_DRIFT_RATIO
    {
        return None;
    }

    Some(RangeSqueezeBoxV15 {
        upper,
        lower,
        height,
        median_volume: nearest_rank(&volumes, 0.50)?,
    })
}

/// Pine `ta.percentile_nearest_rank` 使用 `ceil(p*n)-1`，这里固定相同秩选择。
fn nearest_rank(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((percentile * sorted.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted.get(rank).copied()
}

/// 相邻触碰只按预注册的最小索引间隔抽取，避免一簇触碰无限增加样本数。
fn touch_groups(values: &[f64], edge: f64, band: f64) -> usize {
    let mut groups = 0;
    let mut last_touch = None;
    for (index, value) in values.iter().copied().enumerate() {
        if (value - edge).abs() <= band
            && last_touch.is_none_or(|previous| index - previous >= MIN_TOUCH_GAP)
        {
            groups += 1;
            last_touch = Some(index);
        }
    }
    groups
}

/// 使用前一根收盘计算与 Pine `ta.tr(true)` 相同的单根真实波幅。
fn true_range(candles: &[Candle], index: usize) -> Option<f64> {
    let candle = candles.get(index)?;
    let previous_close = index
        .checked_sub(1)
        .and_then(|previous| candles.get(previous))
        .map(|previous| previous.close)
        .unwrap_or(candle.close);
    Some(
        (candle.high - candle.low)
            .max((candle.high - previous_close).abs())
            .max((candle.low - previous_close).abs()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable_box() -> Vec<Candle> {
        (0..60)
            .map(|index| {
                let recent = index >= 55;
                let upper_touch = index % 4 == 0;
                let lower_touch = index % 4 == 2;
                Candle {
                    timestamp_ms: index as i64 * 900_000,
                    open: 100.0,
                    high: if recent {
                        100.4
                    } else if upper_touch {
                        101.0
                    } else {
                        100.6
                    },
                    low: if recent {
                        99.6
                    } else if lower_touch {
                        99.0
                    } else {
                        99.4
                    },
                    close: if index % 2 == 0 { 100.2 } else { 99.8 },
                    volume: if recent { 40.0 } else { 100.0 },
                }
            })
            .collect()
    }

    #[test]
    fn stable_two_sided_contraction_is_accepted_without_ema_context() {
        let candles = stable_box();
        let detected =
            range_squeeze_box_v15(&candles, candles.len(), 1.0).expect("valid frozen box");

        assert!((detected.upper - 101.0).abs() < 1e-9);
        assert!((detected.lower - 99.0).abs() < 1e-9);
        assert_eq!(detected.median_volume, 100.0);
    }

    #[test]
    fn directional_boundary_drift_is_rejected() {
        let mut candles = stable_box();
        for candle in &mut candles[36..] {
            candle.high += 1.0;
            candle.low += 1.0;
            candle.open += 1.0;
            candle.close += 1.0;
        }

        assert_eq!(range_squeeze_box_v15(&candles, candles.len(), 1.0), None);
    }
}
