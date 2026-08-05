/// 按候选 K 自身的前十根原始均量标记历史异常量，再计算当前 K 的过滤量比。
///
/// `volume_at` 只允许返回调用时点已经完成的 K 线成交量。把算法集中在这里，是为了让入场和
/// v12 持仓保护共享完全相同的因果分母，而不是维护两套近似实现。
pub(crate) fn causal_filtered_volume_ratio<F>(
    candle_count: usize,
    latest_idx: usize,
    min_ratio: f64,
    volume_at: F,
) -> Result<(f64, usize), &'static str>
where
    F: Fn(usize) -> Option<f64>,
{
    const HISTORY_CANDLES: usize = 10;
    const MIN_RETAINED_CANDLES: usize = 5;

    if !min_ratio.is_finite() || min_ratio <= 0.0 {
        return Err("filtered_volume_v3_ratio_policy_invalid");
    }
    if latest_idx >= candle_count {
        return Err("filtered_volume_v3_not_ready");
    }
    let history_start = latest_idx
        .checked_sub(HISTORY_CANDLES)
        .ok_or("filtered_volume_v3_not_ready")?;
    let mut retained_sum = 0.0;
    let mut retained_candles = 0usize;
    for candidate_idx in history_start..latest_idx {
        let marking_start = candidate_idx
            .checked_sub(HISTORY_CANDLES)
            .ok_or("filtered_volume_v3_not_ready")?;
        let mut marking_sum = 0.0;
        for history_idx in marking_start..candidate_idx {
            marking_sum += valid_volume(volume_at(history_idx))?;
        }
        let marking_average = marking_sum / HISTORY_CANDLES as f64;
        let candidate_volume = valid_volume(volume_at(candidate_idx))?;
        let marked = marking_average > 0.0 && candidate_volume >= marking_average * min_ratio;
        if !marked {
            retained_sum += candidate_volume;
            retained_candles += 1;
        }
    }
    if retained_candles < MIN_RETAINED_CANDLES {
        return Err("filtered_volume_v3_insufficient_retained_history");
    }
    let baseline = retained_sum / retained_candles as f64;
    if !baseline.is_finite() || baseline <= 0.0 {
        return Err("filtered_volume_v3_baseline_invalid");
    }
    let current_volume = valid_volume(volume_at(latest_idx))?;
    Ok((current_volume / baseline, retained_candles))
}

fn valid_volume(volume: Option<f64>) -> Result<f64, &'static str> {
    volume
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or("filtered_volume_v3_volume_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_ratio_excludes_history_items_marked_against_their_own_prior_ten() {
        let mut volumes = vec![10.0; 30];
        volumes[15] = 25.0;
        volumes[25] = 25.0;

        let (ratio, retained) =
            causal_filtered_volume_ratio(volumes.len(), 25, 2.5, |idx| volumes.get(idx).copied())
                .unwrap();

        assert_eq!(retained, 9);
        assert!((ratio - 2.5).abs() < 1e-12);
    }

    #[test]
    fn exactly_the_threshold_is_marked_and_removed_from_the_current_denominator() {
        let mut volumes = vec![10.0; 30];
        volumes[15] = 25.0;
        volumes[25] = 20.0;

        let (ratio, retained) =
            causal_filtered_volume_ratio(volumes.len(), 25, 2.5, |idx| volumes.get(idx).copied())
                .unwrap();

        assert_eq!(retained, 9);
        assert!((ratio - 2.0).abs() < 1e-12);
    }
}
