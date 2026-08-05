use super::super::{ComputedCandle, MacdDivergenceSignalEvidence, MarketVelocityTradeDirection};
use super::{
    BranchCandidate, PivotKind, DIVERGENCE_LOOKBACK_CANDLES, DIVERGENCE_PIVOT_WING_CANDLES,
    RSI_OVERBOUGHT, RSI_OVERSOLD,
};

/// 在 t 收盘时只评估刚确认的 p=t-3，并与 p 前 48 根内最近同类枢轴 q 比较。
pub(super) fn macd_candidates(
    candles: &[ComputedCandle],
    latest_idx: usize,
    zero_band_atr_multiplier: f64,
    min_normalized_dif_improvement: f64,
) -> (Vec<BranchCandidate>, Vec<MacdDivergenceSignalEvidence>) {
    if !positive(zero_band_atr_multiplier) || !positive(min_normalized_dif_improvement) {
        return (Vec::new(), Vec::new());
    }
    let Some(pivot_idx) = latest_idx.checked_sub(DIVERGENCE_PIVOT_WING_CANDLES) else {
        return (Vec::new(), Vec::new());
    };
    let mut candidates = Vec::with_capacity(2);
    let mut evidence = Vec::with_capacity(2);
    for kind in [PivotKind::High, PivotKind::Low] {
        if !is_strict_price_pivot(candles, pivot_idx, kind) {
            continue;
        }
        let Some(reference_idx) = latest_reference_pivot(candles, pivot_idx, kind) else {
            continue;
        };
        if let Some((candidate, candidate_evidence)) = evaluate_divergence(
            candles,
            pivot_idx,
            reference_idx,
            kind,
            zero_band_atr_multiplier,
            min_normalized_dif_improvement,
        ) {
            candidates.push(candidate);
            evidence.push(candidate_evidence);
        }
    }
    (candidates, evidence)
}

fn evaluate_divergence(
    candles: &[ComputedCandle],
    pivot_idx: usize,
    reference_idx: usize,
    kind: PivotKind,
    zero_band_atr_multiplier: f64,
    min_normalized_dif_improvement: f64,
) -> Option<(BranchCandidate, MacdDivergenceSignalEvidence)> {
    let pivot = candles.get(pivot_idx)?;
    let reference = candles.get(reference_idx)?;
    let pivot_close = positive_value(pivot.candle.close)?;
    let reference_close = positive_value(reference.candle.close)?;
    let pivot_atr = positive_option(pivot.atr14)?;
    let reference_atr = positive_option(reference.atr14)?;
    let pivot_dif = finite_option(pivot.macd_line)?;
    let reference_dif = finite_option(reference.macd_line)?;
    let pivot_rsi = finite_option(pivot.rsi14)?;
    let pivot_normalized_dif = pivot_dif / pivot_close;
    let reference_normalized_dif = reference_dif / reference_close;
    let pivot_zero_band = zero_band_atr_multiplier * pivot_atr;
    let reference_zero_band = zero_band_atr_multiplier * reference_atr;
    if !positive(pivot_zero_band)
        || !positive(reference_zero_band)
        || !pivot_normalized_dif.is_finite()
        || !reference_normalized_dif.is_finite()
    {
        return None;
    }

    let (direction, trigger, pivot_price, reference_price, improvement) = match kind {
        PivotKind::High => {
            let improvement = reference_normalized_dif - pivot_normalized_dif;
            if !pivot.candle.high.is_finite()
                || !reference.candle.high.is_finite()
                || pivot.candle.high <= reference.candle.high
                || pivot_dif <= pivot_zero_band
                || reference_dif <= reference_zero_band
                || improvement < min_normalized_dif_improvement
                || pivot_rsi < RSI_OVERBOUGHT
            {
                return None;
            }
            (
                MarketVelocityTradeDirection::Short,
                "macd_bearish_divergence_short",
                pivot.candle.high,
                reference.candle.high,
                improvement,
            )
        }
        PivotKind::Low => {
            let improvement = pivot_normalized_dif - reference_normalized_dif;
            if !pivot.candle.low.is_finite()
                || !reference.candle.low.is_finite()
                || pivot.candle.low >= reference.candle.low
                || pivot_dif >= -pivot_zero_band
                || reference_dif >= -reference_zero_band
                || improvement < min_normalized_dif_improvement
                || pivot_rsi > RSI_OVERSOLD
            {
                return None;
            }
            (
                MarketVelocityTradeDirection::Long,
                "macd_bullish_divergence_long",
                pivot.candle.low,
                reference.candle.low,
                improvement,
            )
        }
    };

    Some((
        BranchCandidate { direction, trigger },
        MacdDivergenceSignalEvidence {
            direction,
            pivot_ts_ms: pivot.candle.ts,
            reference_pivot_ts_ms: reference.candle.ts,
            pivot_price,
            reference_pivot_price: reference_price,
            pivot_rsi14: pivot_rsi,
            pivot_dif,
            reference_pivot_dif: reference_dif,
            pivot_normalized_dif,
            reference_pivot_normalized_dif: reference_normalized_dif,
            normalized_dif_improvement: improvement,
            zero_band_atr_multiplier,
            min_normalized_dif_improvement,
        },
    ))
}

/// q 只能位于 p 之前 48 根内；倒序命中保证使用最近同类型枢轴。
pub(super) fn latest_reference_pivot(
    candles: &[ComputedCandle],
    pivot_idx: usize,
    kind: PivotKind,
) -> Option<usize> {
    let first = pivot_idx
        .saturating_sub(DIVERGENCE_LOOKBACK_CANDLES)
        .max(DIVERGENCE_PIVOT_WING_CANDLES);
    let last = pivot_idx.checked_sub(1)?;
    (first <= last).then_some(())?;
    (first..=last)
        .rev()
        .find(|idx| is_strict_price_pivot(candles, *idx, kind))
}

/// 相等高低点必须失败，避免宽松比较把平台边缘误认成唯一价格枢轴。
pub(super) fn is_strict_price_pivot(
    candles: &[ComputedCandle],
    center: usize,
    kind: PivotKind,
) -> bool {
    let wing = DIVERGENCE_PIVOT_WING_CANDLES;
    let Some(start) = center.checked_sub(wing) else {
        return false;
    };
    let Some(end) = center.checked_add(wing) else {
        return false;
    };
    let Some(candidate) = candles.get(center) else {
        return false;
    };
    let Some(window) = candles.get(start..=end) else {
        return false;
    };
    let candidate_price = price(candidate, kind);
    candidate_price.is_finite()
        && window.iter().enumerate().all(|(offset, candle)| {
            if offset == wing {
                return true;
            }
            let neighbor_price = price(candle, kind);
            neighbor_price.is_finite()
                && match kind {
                    PivotKind::High => candidate_price > neighbor_price,
                    PivotKind::Low => candidate_price < neighbor_price,
                }
        })
}

fn price(candle: &ComputedCandle, kind: PivotKind) -> f64 {
    match kind {
        PivotKind::High => candle.candle.high,
        PivotKind::Low => candle.candle.low,
    }
}

fn finite_option(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn positive_option(value: Option<f64>) -> Option<f64> {
    value.filter(|value| positive(*value))
}

fn positive_value(value: f64) -> Option<f64> {
    positive(value).then_some(value)
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::super::filtered_volume_rsi_ema_macd_signal;
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        market_filtered_volume_rsi_ema_macd_v2_research_args,
        market_velocity_paper_strategy_preset_manifest, BacktestCandle,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET,
    };

    const SIGNAL_IDX: usize = 59;
    const PIVOT_IDX: usize = SIGNAL_IDX - DIVERGENCE_PIVOT_WING_CANDLES;
    const REFERENCE_IDX: usize = 40;
    const Z: f64 = 0.10;
    const D_MIN: f64 = 0.005;

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            volume_ccy: None,
            candle: BacktestCandle {
                ts: idx as i64 * 900_000,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            },
            sma: None,
            ema: None,
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: None,
            previous_range_avg: None,
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: None,
            macd_histogram: None,
        }
    }

    fn candles() -> Vec<ComputedCandle> {
        (0..=SIGNAL_IDX).map(candle).collect()
    }

    fn top_pair(candles: &mut [ComputedCandle]) {
        candles[REFERENCE_IDX].candle.high = 105.0;
        candles[REFERENCE_IDX].macd_line = Some(1.0);
        candles[PIVOT_IDX].candle.high = 106.0;
        candles[PIVOT_IDX].macd_line = Some(0.5);
        candles[PIVOT_IDX].rsi14 = Some(70.0);
    }

    fn bottom_pair(candles: &mut [ComputedCandle]) {
        candles[REFERENCE_IDX].candle.low = 95.0;
        candles[REFERENCE_IDX].macd_line = Some(-1.0);
        candles[PIVOT_IDX].candle.low = 94.0;
        candles[PIVOT_IDX].macd_line = Some(-0.5);
        candles[PIVOT_IDX].rsi14 = Some(30.0);
    }

    fn confirm_volume(candles: &mut [ComputedCandle]) {
        candles[SIGNAL_IDX].candle.volume = 30.0;
    }

    #[test]
    fn top_divergence_uses_p_t_minus_three_and_latest_same_type_q() {
        let mut candles = candles();
        candles[30].candle.high = 104.0;
        candles[30].macd_line = Some(2.0);
        top_pair(&mut candles);
        candles[SIGNAL_IDX].candle.high = 105.5;
        candles[SIGNAL_IDX].macd_line = Some(-10.0);
        candles[SIGNAL_IDX].rsi14 = Some(5.0);

        let (candidates, evidence) = macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].direction, MarketVelocityTradeDirection::Short);
        assert_eq!(candidates[0].trigger, "macd_bearish_divergence_short");
        assert_eq!(evidence[0].pivot_ts_ms, candles[PIVOT_IDX].candle.ts);
        assert_eq!(
            evidence[0].reference_pivot_ts_ms,
            candles[REFERENCE_IDX].candle.ts
        );
        assert!((evidence[0].normalized_dif_improvement - D_MIN).abs() < 1e-12);
    }

    #[test]
    fn bottom_divergence_accepts_inclusive_rsi_and_d_min_boundaries() {
        let mut candles = candles();
        bottom_pair(&mut candles);

        let (candidates, evidence) = macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].direction, MarketVelocityTradeDirection::Long);
        assert_eq!(evidence[0].pivot_rsi14, RSI_OVERSOLD);

        candles[PIVOT_IDX].rsi14 = Some(RSI_OVERSOLD + 0.01);
        assert!(macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN).0.is_empty());
    }

    #[test]
    fn equal_neighbor_rejects_the_newly_confirmed_pivot() {
        let mut candles = candles();
        top_pair(&mut candles);
        candles[PIVOT_IDX + 1].candle.high = candles[PIVOT_IDX].candle.high;

        assert!(macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN).0.is_empty());
    }

    #[test]
    fn zero_band_and_minimum_improvement_are_fail_closed() {
        let mut candles = candles();
        top_pair(&mut candles);

        assert!(macd_candidates(&candles, SIGNAL_IDX, 0.25, D_MIN)
            .0
            .is_empty());
        assert!(macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN + 0.0001)
            .0
            .is_empty());
        candles[PIVOT_IDX].macd_line = Some(-0.5);
        assert!(macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN).0.is_empty());
    }

    #[test]
    fn reference_pivot_outside_previous_48_candles_is_ignored() {
        let mut candles = candles();
        candles[7].candle.high = 105.0;
        candles[7].macd_line = Some(1.0);
        candles[PIVOT_IDX].candle.high = 106.0;
        candles[PIVOT_IDX].macd_line = Some(0.5);
        candles[PIVOT_IDX].rsi14 = Some(70.0);

        assert!(macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN).0.is_empty());
    }

    #[test]
    fn candles_after_t_cannot_change_the_confirmed_result() {
        let mut candles = candles();
        top_pair(&mut candles);
        let before = macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN);
        let mut future = candle(SIGNAL_IDX + 1);
        future.candle.high = 1_000.0;
        future.macd_line = Some(100.0);
        candles.push(future);

        let after = macd_candidates(&candles, SIGNAL_IDX, Z, D_MIN);

        assert_eq!(before, after);
    }

    #[test]
    fn v2_dispatch_disables_unconfigured_macd_and_uses_confirmed_pair_when_configured() {
        let mut candles = candles();
        top_pair(&mut candles);
        confirm_volume(&mut candles);
        let mut args = market_filtered_volume_rsi_ema_macd_v2_research_args().unwrap();

        assert_eq!(
            filtered_volume_rsi_ema_macd_signal(&candles, candles.len(), &args),
            Err("filtered_volume_strategy_no_branch_signal")
        );

        args.entry_filtered_volume_macd_zero_band_atr_multiplier = Some(Z);
        args.entry_filtered_volume_macd_min_normalized_dif_improvement = Some(D_MIN);
        let signal = filtered_volume_rsi_ema_macd_signal(&candles, candles.len(), &args).unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(signal.trigger, "macd_bearish_divergence_short");
        assert_eq!(signal.evidence.macd_divergences.len(), 1);
        assert_eq!(
            signal.evidence.macd_divergences[0].pivot_ts_ms,
            candles[PIVOT_IDX].candle.ts
        );
        assert_eq!(
            signal.evidence.macd_divergences[0].reference_pivot_ts_ms,
            candles[REFERENCE_IDX].candle.ts
        );
    }

    #[test]
    fn v2_manifest_records_unconfigured_fail_closed_policy() {
        let args = market_filtered_volume_rsi_ema_macd_v2_research_args().unwrap();
        assert_eq!(
            args.paper_strategy_preset,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET
        );
        assert_eq!(
            args.entry_filtered_volume_macd_zero_band_atr_multiplier,
            None
        );
        assert_eq!(
            args.entry_filtered_volume_macd_min_normalized_dif_improvement,
            None
        );

        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET,
        )
        .unwrap();
        let macd = &manifest.manifest_json["parameters"]["fast_momentum_filters"]
            ["filtered_volume_rsi_ema_macd"]["macd_branch"];
        assert_eq!(manifest.channel, "research");
        assert_eq!(macd["enabled"], false);
        assert_eq!(macd["pivot_candidate_offset_candles"], 3);
        assert_eq!(macd["strict_pivot_comparison"], true);
        assert_eq!(macd["unconfigured_parameter_policy"], "disable_macd_branch");
    }
}
