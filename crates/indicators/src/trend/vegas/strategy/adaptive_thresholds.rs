impl VegasStrategy {
    /// 基于信号时点已经确认的 K 线计算 ATR 与相对成交量分位数快照。
    ///
    /// 先把每根 K 线成交量除以自身前序均量，再比较这些无量纲比率。这样既消除币种间
    /// 成交量量级差异，也避免原始成交量随市场扩容后长期抬升造成分位数漂移。
    fn calculate_cross_asset_adaptive_value(
        &self,
        data_items: &[CandleItem],
    ) -> CrossAssetAdaptiveThresholdValue {
        let config = self.cross_asset_adaptive_threshold;
        if !config.is_open || data_items.is_empty() {
            return CrossAssetAdaptiveThresholdValue::default();
        }

        let Some(current) = data_items.last() else {
            return CrossAssetAdaptiveThresholdValue::default();
        };
        let atr_period = config.atr_period.max(1);
        let volume_lookback = config.volume_lookback_bars.max(1);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let atr_window = atr_period.saturating_mul(3).saturating_add(1);
        let mut atr = ATR::new(atr_period).expect("ATR period is clamped to at least one");
        for candle in data_items.iter().rev().take(atr_window).rev() {
            atr.next(candle.h, candle.l, candle.c);
        }
        let atr_value = atr.value_optional().unwrap_or_default();

        let current_index = data_items.len() - 1;
        let relative_volume_ratio = relative_volume_ratio_at(
            data_items,
            current_index,
            volume_baseline_bars,
        )
        .unwrap_or_default();

        // 当前比率不进入历史样本；线性计数即可得到经验分位数，无需逐 K 线分配并排序。
        let mut volume_sample_size = 0usize;
        let mut not_greater_count = 0usize;
        for index in (0..current_index).rev().take(volume_lookback) {
            let Some(historical_ratio) =
                relative_volume_ratio_at(data_items, index, volume_baseline_bars)
            else {
                continue;
            };
            volume_sample_size += 1;
            if historical_ratio <= relative_volume_ratio {
                not_greater_count += 1;
            }
        }
        let volume_percentile = if volume_sample_size == 0 {
            0.0
        } else {
            not_greater_count as f64 / volume_sample_size as f64
        };
        let is_ready = atr_value > 0.0 && volume_sample_size >= volume_lookback;
        let close = current.c.max(1e-9);

        CrossAssetAdaptiveThresholdValue {
            enabled: true,
            is_ready,
            atr_value,
            atr_ratio: atr_value / close,
            candle_range_atr_multiple: if atr_value > 0.0 {
                (current.h - current.l).abs() / atr_value
            } else {
                0.0
            },
            candle_body_atr_multiple: if atr_value > 0.0 {
                (current.c - current.o).abs() / atr_value
            } else {
                0.0
            },
            relative_volume_ratio,
            volume_percentile,
            volume_sample_size,
        }
    }

    /// 判断当前成交量是否达到配置要求；自适应模式数据不足时选择拒绝，而不是回退旧阈值。
    fn adaptive_volume_confirmed(
        &self,
        adaptive: &CrossAssetAdaptiveThresholdValue,
        legacy_confirmed: bool,
    ) -> bool {
        if !self.cross_asset_adaptive_threshold.is_open {
            return legacy_confirmed;
        }
        let min_volume_percentile = self
            .cross_asset_adaptive_threshold
            .effective_min_volume_percentile(adaptive.atr_ratio);
        adaptive.is_ready
            && adaptive.volume_percentile >= min_volume_percentile
    }
}

/// 复用 Vegas 既有“跳过紧邻前一根、比较更早 N 根均量”的成交量口径，并把结果转成无量纲比率。
fn relative_volume_ratio_at(
    candles: &[CandleItem],
    index: usize,
    baseline_bars: usize,
) -> Option<f64> {
    let baseline_bars = baseline_bars.max(1);
    if index < baseline_bars.saturating_add(1) {
        return None;
    }
    let start = index - baseline_bars - 1;
    let end = index - 1;
    let current_volume = candles.get(index)?.v;
    if !current_volume.is_finite() || current_volume < 0.0 {
        return None;
    }
    let mut sum = 0.0;
    for candle in &candles[start..end] {
        if !candle.v.is_finite() || candle.v < 0.0 {
            return None;
        }
        sum += candle.v;
    }
    let average = sum / baseline_bars as f64;
    if average <= 0.0 || !average.is_finite() {
        return None;
    }
    Some(current_volume / average)
}

#[cfg(test)]
mod cross_asset_adaptive_tests {
    use super::*;

    /// 构造可缩放的测试 K 线，用于验证跨币种归一化结果。
    fn candle(price_scale: f64, volume: f64, ts: i64) -> CandleItem {
        CandleItem {
            o: 100.0 * price_scale,
            h: 103.0 * price_scale,
            l: 99.0 * price_scale,
            c: 102.0 * price_scale,
            v: volume,
            ts,
            confirm: 1,
        }
    }

    /// 生成启用自适应阈值的最小策略实例。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            cross_asset_adaptive_threshold: CrossAssetAdaptiveThresholdConfig {
                is_open: true,
                atr_period: 14,
                volume_lookback_bars: 20,
                ..CrossAssetAdaptiveThresholdConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn adaptive_values_are_price_scale_invariant() {
        let low_price = (0..30)
            .map(|index| candle(1.0, (index + 1) as f64, index))
            .collect::<Vec<_>>();
        let high_price = (0..30)
            .map(|index| candle(1000.0, (index + 1) as f64, index))
            .collect::<Vec<_>>();

        let low = strategy().calculate_cross_asset_adaptive_value(&low_price);
        let high = strategy().calculate_cross_asset_adaptive_value(&high_price);

        assert!(low.is_ready && high.is_ready);
        assert!((low.atr_ratio - high.atr_ratio).abs() < 1e-12);
        assert!((low.candle_body_atr_multiple - high.candle_body_atr_multiple).abs() < 1e-12);
        assert!((low.volume_percentile - high.volume_percentile).abs() < 1e-12);
    }

    #[test]
    fn relative_volume_percentile_uses_only_prior_confirmed_ratios() {
        let mut candles = (1..=26)
            .map(|volume| candle(1.0, (volume % 7 + 10) as f64, volume))
            .collect::<Vec<_>>();
        candles.push(candle(1.0, 0.1, 27));

        let low = strategy().calculate_cross_asset_adaptive_value(&candles);
        candles.last_mut().expect("current candle").v = 10_000.0;
        let high = strategy().calculate_cross_asset_adaptive_value(&candles);

        assert_eq!(low.volume_sample_size, 20);
        assert_eq!(high.volume_sample_size, 20);
        assert_eq!(low.volume_percentile, 0.0);
        assert_eq!(high.volume_percentile, 1.0);
        assert!(low.relative_volume_ratio < high.relative_volume_ratio);
    }

    #[test]
    fn relative_volume_ratio_matches_existing_vegas_baseline_shape() {
        let candles = [10.0, 20.0, 30.0, 40.0, 999.0, 100.0]
            .into_iter()
            .enumerate()
            .map(|(index, volume)| candle(1.0, volume, index as i64))
            .collect::<Vec<_>>();

        let ratio = relative_volume_ratio_at(&candles, 5, 4).expect("ratio");

        assert!((ratio - 4.0).abs() < 1e-12);
    }

    #[test]
    fn disabled_adaptive_mode_preserves_legacy_volume_decision() {
        let strategy = VegasStrategy::default();
        let value = CrossAssetAdaptiveThresholdValue::default();

        assert!(strategy.adaptive_volume_confirmed(&value, true));
        assert!(!strategy.adaptive_volume_confirmed(&value, false));
    }

    #[test]
    fn enabled_adaptive_mode_fails_closed_when_history_is_incomplete() {
        let strategy = strategy();
        let value = CrossAssetAdaptiveThresholdValue {
            enabled: true,
            is_ready: false,
            volume_percentile: 1.0,
            ..CrossAssetAdaptiveThresholdValue::default()
        };

        assert!(!strategy.adaptive_volume_confirmed(&value, true));
    }
}
