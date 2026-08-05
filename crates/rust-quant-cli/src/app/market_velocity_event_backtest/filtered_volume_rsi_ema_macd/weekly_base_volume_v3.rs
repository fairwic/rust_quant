use super::super::computed_candles::bollinger_bands_from_closes;
use super::super::filtered_volume_baseline::causal_filtered_volume_ratio;
use super::super::{
    BollingerConflictSignalEvidence, CompletedCandleEntrySignalEvidence, ComputedCandle,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection, RsiDivergenceSignalEvidence,
    MS_15M,
};
use super::macd_v2;
use super::weekly_p90_anchor_rsi_divergence_v9;
use super::{
    ema_candidate, positive_indicator, BranchCandidate, FilteredVolumeEvidence,
    FilteredVolumeRsiEmaMacdSignal, PivotKind, DIVERGENCE_PIVOT_WING_CANDLES,
    DOJI_MAX_BODY_RANGE_RATIO, FILTERED_VOLUME_MIN_RATIO, FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
    FILTERED_VOLUME_V3_ATR_STOP_SOURCE, FILTERED_VOLUME_V3_BEARISH_ENGULFING_STOP_SOURCE,
    FILTERED_VOLUME_V3_BULLISH_ENGULFING_STOP_SOURCE, FILTERED_VOLUME_V3_LOWER_WICK_STOP_SOURCE,
    FILTERED_VOLUME_V3_UPPER_WICK_STOP_SOURCE, REVERSAL_WICK_MIN_RANGE_RATIO,
    RSI_DIVERGENCE_MIN_DELTA, RSI_OVERBOUGHT, RSI_OVERSOLD,
};

/// 一周包含 672 根 15m K 线；当前 K 线永远不进入自己的 `vol_ccy` 百分位样本。
pub(crate) const WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES: usize = 672;
/// nearest-rank P90 的一基秩为 605，因此零基实现固定读取下标 604。
pub(crate) const WEEKLY_VOLUME_CCY_P90_INDEX: usize = 604;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReversalPattern {
    BullishEngulfing,
    LowerWick,
    BearishEngulfing,
    UpperWick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PatternCandidate {
    direction: MarketVelocityTradeDirection,
    pattern: ReversalPattern,
}

#[derive(Debug, Clone, PartialEq)]
struct RsiBranchResult {
    candidates: Vec<BranchCandidate>,
    pattern: Option<PatternCandidate>,
    divergences: Vec<RsiDivergenceSignalEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// 候选 K 的成交额及其当时可见的 672 根历史 P90 门槛。
pub(super) struct WeeklyBaseVolumeEvidence {
    /// 候选 K 自身的 `vol_ccy`。
    pub(super) current: f64,
    /// 候选 K 之前 672 根 `vol_ccy` 的 nearest-rank P90。
    pub(super) p90: f64,
}

/// 按最新文档独立计算 v3，避免新基础成交量和风险语义改变 V1/V2 的历史结果。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_optional_overlays(
        candles,
        completed_count,
        args,
        None,
        None,
        false,
        FILTERED_VOLUME_MIN_RATIO,
        false,
    )
}

/// 只增加中性 RSI 长下影做多候选，其余成交量、分支合并和风险语义完整复用 v3。
pub(super) fn signal_with_neutral_rsi_lower_wick_long(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_optional_overlays(
        candles,
        completed_count,
        args,
        None,
        None,
        true,
        FILTERED_VOLUME_MIN_RATIO,
        false,
    )
}

/// 复用 v3 的成交量、指标与风险契约，仅在候选合并阶段叠加布林反向冲突。
pub(super) fn signal_with_bollinger_conflict(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
    period: usize,
    standard_deviation_multiplier: f64,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_optional_overlays(
        candles,
        completed_count,
        args,
        Some((period, standard_deviation_multiplier)),
        None,
        false,
        FILTERED_VOLUME_MIN_RATIO,
        false,
    )
}

/// 复用 v3 契约，但只限制 EMA 延续候选距离 EMA144；RSI/MACD 候选保持独立。
pub(super) fn signal_with_ema144_distance_gate(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
    max_distance_atr: f64,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_optional_overlays(
        candles,
        completed_count,
        args,
        None,
        Some(max_distance_atr),
        false,
        FILTERED_VOLUME_MIN_RATIO,
        false,
    )
}

/// 复用 v3 的其余候选和风险合同，仅替换 RSI 背离并降低本版本的过滤量比门槛。
pub(super) fn signal_with_weekly_p90_anchor_rsi_divergence(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
    filtered_volume_min_ratio: f64,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_optional_overlays(
        candles,
        completed_count,
        args,
        None,
        None,
        false,
        filtered_volume_min_ratio,
        true,
    )
}

fn signal_with_optional_overlays(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
    bollinger_policy: Option<(usize, f64)>,
    ema144_max_distance_atr: Option<f64>,
    allow_neutral_rsi_lower_wick_long: bool,
    filtered_volume_min_ratio: f64,
    use_weekly_p90_anchor_rsi_divergence: bool,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("filtered_volume_v3_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("filtered_volume_v3_not_ready")?;
    if !valid_ohlc(latest) {
        return Err("filtered_volume_v3_current_ohlc_invalid");
    }

    let volume = filtered_volume_evidence(candles, latest_idx, filtered_volume_min_ratio)?;
    if volume.ratio < filtered_volume_min_ratio {
        return Err("filtered_volume_v3_volume_not_confirmed");
    }
    let weekly_volume = weekly_volume_ccy_evidence(candles, latest_idx)?;

    // 文档把任一指标未预热定义为整轮信号失败，不能因某个分支暂时不用该值而放行。
    let rsi14 = finite_option(latest.rsi14).ok_or("filtered_volume_v3_rsi_not_ready")?;
    let macd_dif = finite_option(latest.macd_line).ok_or("filtered_volume_v3_macd_not_ready")?;
    let ema12 = positive_indicator(latest.ema12).ok_or("filtered_volume_v3_ema_not_ready")?;
    let ema144 = positive_indicator(latest.ema144).ok_or("filtered_volume_v3_ema_not_ready")?;
    let ema169 = positive_indicator(latest.ema169).ok_or("filtered_volume_v3_ema_not_ready")?;
    let ema696 = positive_indicator(latest.ema696).ok_or("filtered_volume_v3_ema_not_ready")?;
    let atr14 = positive_indicator(latest.atr14).ok_or("filtered_volume_v3_atr_not_ready")?;
    if ema144_max_distance_atr.is_some_and(|maximum| !positive(maximum)) {
        return Err("filtered_volume_v5_ema144_distance_policy_invalid");
    }

    let mut rsi = rsi_branch(
        candles,
        latest_idx,
        rsi14,
        filtered_volume_min_ratio,
        use_weekly_p90_anchor_rsi_divergence,
    );
    let mut candidates = rsi.candidates.clone();
    let ema144_distance_atr = (latest.candle.close - ema144).abs() / atr14;
    let mut ema_candidate_blocked_by_distance = false;
    if let Some(candidate) = ema_candidate(latest, rsi14, ema12, ema144, ema696) {
        // 距离门禁只撤销追涨/追空候选，不能否决同根 K 线上独立成立的 RSI/MACD 信号。
        if ema144_max_distance_atr.is_some_and(|maximum| ema144_distance_atr > maximum) {
            ema_candidate_blocked_by_distance = true;
        } else {
            candidates.push(candidate);
        }
    }
    let (macd_candidates, macd_divergences) = match (
        args.entry_filtered_volume_macd_zero_band_atr_multiplier,
        args.entry_filtered_volume_macd_min_normalized_dif_improvement,
    ) {
        (Some(zero_band_multiplier), Some(min_improvement)) => {
            macd_v2::macd_candidates(candles, latest_idx, zero_band_multiplier, min_improvement)
        }
        _ => (Vec::new(), Vec::new()),
    };
    candidates.extend(macd_candidates);
    // v6/v8 只补充 v3 原本没有方向的中性 RSI 长下影机会，避免改写既有交易的
    // trigger、结构止损或多空冲突结果，保证入场消融可以归因到真正新增的交易。
    if allow_neutral_rsi_lower_wick_long
        && candidates.is_empty()
        && rsi14 > RSI_OVERSOLD
        && rsi14 <= RSI_OVERBOUGHT
        && dominant_wick(latest, PivotKind::Low)
    {
        let pattern = PatternCandidate {
            direction: MarketVelocityTradeDirection::Long,
            pattern: ReversalPattern::LowerWick,
        };
        candidates.push(BranchCandidate {
            direction: pattern.direction,
            trigger: "neutral_rsi_lower_wick_long",
        });
        rsi.pattern = Some(pattern);
    }

    let original_has_long = candidates
        .iter()
        .any(|candidate| candidate.direction == MarketVelocityTradeDirection::Long);
    let original_has_short = candidates
        .iter()
        .any(|candidate| candidate.direction == MarketVelocityTradeDirection::Short);
    let no_signal_error = if use_weekly_p90_anchor_rsi_divergence {
        "filtered_volume_v9_no_branch_signal"
    } else if ema144_max_distance_atr.is_some() {
        "filtered_volume_v5_no_branch_signal"
    } else if bollinger_policy.is_some() {
        "filtered_volume_v4_no_branch_signal"
    } else {
        "filtered_volume_v3_no_branch_signal"
    };
    let conflict_error = if use_weekly_p90_anchor_rsi_divergence {
        "filtered_volume_v9_direction_conflict"
    } else if ema144_max_distance_atr.is_some() {
        "filtered_volume_v5_direction_conflict"
    } else if bollinger_policy.is_some() {
        "filtered_volume_v4_direction_conflict"
    } else {
        "filtered_volume_v3_direction_conflict"
    };
    if !original_has_long && !original_has_short {
        return Err(no_signal_error);
    }
    if original_has_long && original_has_short {
        return Err(conflict_error);
    }

    let bollinger_conflict = if let Some((period, multiplier)) = bollinger_policy {
        let evidence = bollinger_conflict_evidence(candles, latest_idx, period, multiplier)?;
        // 布林带只为已有反向候选投票；如果原策略没有方向，它绝不能自行创建交易。
        if original_has_short && evidence.touches_lower {
            candidates.push(BranchCandidate {
                direction: MarketVelocityTradeDirection::Long,
                trigger: "bollinger_lower_touch_counter_long",
            });
        }
        if original_has_long && evidence.touches_upper {
            candidates.push(BranchCandidate {
                direction: MarketVelocityTradeDirection::Short,
                trigger: "bollinger_upper_touch_counter_short",
            });
        }
        Some(evidence)
    } else {
        None
    };
    let has_long = candidates
        .iter()
        .any(|candidate| candidate.direction == MarketVelocityTradeDirection::Long);
    let has_short = candidates
        .iter()
        .any(|candidate| candidate.direction == MarketVelocityTradeDirection::Short);
    let direction = match (has_long, has_short) {
        (true, false) => MarketVelocityTradeDirection::Long,
        (false, true) => MarketVelocityTradeDirection::Short,
        (true, true) => return Err(conflict_error),
        (false, false) => return Err(no_signal_error),
    };
    let trigger = candidates
        .iter()
        .filter(|candidate| candidate.direction == direction)
        .map(|candidate| candidate.trigger)
        .collect::<Vec<_>>()
        .join("+");
    let participating_pattern = rsi.pattern.filter(|pattern| pattern.direction == direction);
    let (stop_price, stop_source) = stop_loss_at_signal(
        candles,
        latest_idx,
        latest.candle.close,
        atr14,
        direction,
        participating_pattern,
    )?;
    let take_profit_atr_multiplier =
        if (filtered_volume_min_ratio - FILTERED_VOLUME_MIN_RATIO).abs() <= f64::EPSILON {
            target_atr_multiplier(volume.ratio)
        } else {
            target_atr_multiplier_with_min_ratio(volume.ratio, filtered_volume_min_ratio)
        }
        .ok_or("filtered_volume_v3_target_tier_invalid")?;

    Ok(FilteredVolumeRsiEmaMacdSignal {
        direction,
        trigger,
        structure_stop_loss_price: stop_price,
        structure_stop_loss_source: stop_source,
        evidence: CompletedCandleEntrySignalEvidence {
            filtered_volume_ratio: volume.ratio,
            filtered_volume_retained_candles: volume.retained_candles,
            current_volume_ccy: Some(weekly_volume.current),
            weekly_volume_ccy_p90: Some(weekly_volume.p90),
            rsi14,
            macd_dif,
            ema12,
            ema144,
            ema169,
            ema696,
            atr14,
            take_profit_atr_multiplier: Some(take_profit_atr_multiplier),
            rsi_pattern_stop_participated: participating_pattern.is_some(),
            rsi_divergences: rsi.divergences,
            macd_divergences,
            bollinger_conflict,
            ema144_distance_atr: ema144_max_distance_atr.map(|_| ema144_distance_atr),
            ema144_max_distance_atr,
            ema_candidate_blocked_by_distance,
            anchor_entry: None,
            trend_managed_exit: None,
            isolated_family: None,
        },
    })
}

/// 仅使用截至 `latest_idx` 的 12 根收盘价计算触轨，最高/最低价只参与最终触达判断。
fn bollinger_conflict_evidence(
    candles: &[ComputedCandle],
    latest_idx: usize,
    period: usize,
    standard_deviation_multiplier: f64,
) -> Result<BollingerConflictSignalEvidence, &'static str> {
    let start = latest_idx
        .checked_add(1)
        .and_then(|end| end.checked_sub(period))
        .ok_or("filtered_volume_v4_bollinger_not_ready")?;
    let window = candles
        .get(start..=latest_idx)
        .filter(|items| items.len() == period)
        .ok_or("filtered_volume_v4_bollinger_not_ready")?;
    let closes = window
        .iter()
        .map(|candle| candle.candle.close)
        .collect::<Vec<_>>();
    let bands = bollinger_bands_from_closes(&closes, standard_deviation_multiplier)
        .ok_or("filtered_volume_v4_bollinger_invalid")?;
    let current = candles
        .get(latest_idx)
        .ok_or("filtered_volume_v4_bollinger_not_ready")?;
    Ok(BollingerConflictSignalEvidence {
        period,
        standard_deviation_multiplier,
        middle: bands.middle,
        upper: bands.upper,
        lower: bands.lower,
        touches_upper: current.candle.high >= bands.upper,
        touches_lower: current.candle.low <= bands.lower,
    })
}

/// v3 的 ATR 止盈档位与止损距离解耦，形态止损下也保持相同价格距离。
pub(super) fn target_atr_multiplier(filtered_volume_ratio: f64) -> Option<f64> {
    target_atr_multiplier_with_min_ratio(filtered_volume_ratio, FILTERED_VOLUME_MIN_RATIO)
}

/// 允许独立研究版本扩展第一档下界，同时保持 4 倍和 6 倍两个既有分档不变。
pub(super) fn target_atr_multiplier_with_min_ratio(
    filtered_volume_ratio: f64,
    filtered_volume_min_ratio: f64,
) -> Option<f64> {
    if !filtered_volume_ratio.is_finite()
        || !positive(filtered_volume_min_ratio)
        || filtered_volume_ratio < filtered_volume_min_ratio
    {
        return None;
    }
    if filtered_volume_ratio < 4.0 {
        Some(2.7)
    } else if filtered_volume_ratio < 6.0 {
        Some(3.6)
    } else {
        Some(4.5)
    }
}

/// 历史标记使用各自前十根原始量；只在当前基线中排除已经标记的历史放量。
pub(super) fn filtered_volume_evidence(
    candles: &[ComputedCandle],
    latest_idx: usize,
    filtered_volume_min_ratio: f64,
) -> Result<FilteredVolumeEvidence, &'static str> {
    let (ratio, retained_candles) = causal_filtered_volume_ratio(
        candles.len(),
        latest_idx,
        filtered_volume_min_ratio,
        |idx| candles.get(idx).map(|candle| candle.candle.volume),
    )?;
    Ok(FilteredVolumeEvidence {
        ratio,
        retained_candles,
    })
}

/// `vol_ccy` 百分位窗口同时校验时间连续性，避免缺失 K 线把七天窗口悄悄拉长。
pub(super) fn weekly_volume_ccy_evidence(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<WeeklyBaseVolumeEvidence, &'static str> {
    let start = latest_idx
        .checked_sub(WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES)
        .ok_or("filtered_volume_v3_weekly_volume_ccy_not_ready")?;
    let current = candles
        .get(latest_idx)
        .ok_or("filtered_volume_v3_weekly_volume_ccy_not_ready")?;
    let history = candles
        .get(start..latest_idx)
        .filter(|items| items.len() == WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES)
        .ok_or("filtered_volume_v3_weekly_volume_ccy_not_ready")?;
    let mut values = Vec::with_capacity(WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES);
    for (offset, candle) in history.iter().enumerate() {
        let remaining = WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES - offset;
        let expected_ts = current
            .candle
            .ts
            .checked_sub(remaining as i64 * MS_15M)
            .ok_or("filtered_volume_v3_weekly_volume_ccy_time_invalid")?;
        if candle.candle.ts != expected_ts {
            return Err("filtered_volume_v3_weekly_volume_ccy_not_continuous");
        }
        values.push(
            candle
                .volume_ccy
                .and_then(finite_non_negative)
                .ok_or("filtered_volume_v3_weekly_volume_ccy_invalid")?,
        );
    }
    values.sort_by(f64::total_cmp);
    let p90 = *values
        .get(WEEKLY_VOLUME_CCY_P90_INDEX)
        .ok_or("filtered_volume_v3_weekly_volume_ccy_not_ready")?;
    let current_volume_ccy = current
        .volume_ccy
        .filter(|value| positive(*value))
        .ok_or("filtered_volume_v3_current_volume_ccy_invalid")?;
    if current_volume_ccy < p90 {
        return Err("filtered_volume_v3_current_volume_ccy_below_p90");
    }
    Ok(WeeklyBaseVolumeEvidence {
        current: current_volume_ccy,
        p90,
    })
}

fn rsi_branch(
    candles: &[ComputedCandle],
    latest_idx: usize,
    current_rsi: f64,
    filtered_volume_min_ratio: f64,
    use_weekly_p90_anchor_rsi_divergence: bool,
) -> RsiBranchResult {
    let (candidates, divergences) = if use_weekly_p90_anchor_rsi_divergence {
        weekly_p90_anchor_rsi_divergence_v9::rsi_divergence_candidates(
            candles,
            latest_idx,
            current_rsi,
            filtered_volume_min_ratio,
        )
    } else {
        rsi_divergence_candidates(candles, latest_idx)
    };
    if !candidates.is_empty() {
        return RsiBranchResult {
            candidates,
            pattern: None,
            divergences,
        };
    }
    let pattern = if current_rsi <= RSI_OVERSOLD {
        bullish_reversal_pattern(candles, latest_idx).map(|pattern| PatternCandidate {
            direction: MarketVelocityTradeDirection::Long,
            pattern,
        })
    } else if current_rsi >= RSI_OVERBOUGHT {
        bearish_reversal_pattern(candles, latest_idx).map(|pattern| PatternCandidate {
            direction: MarketVelocityTradeDirection::Short,
            pattern,
        })
    } else {
        None
    };
    let candidates = pattern
        .map(|pattern| BranchCandidate {
            direction: pattern.direction,
            trigger: match pattern.pattern {
                ReversalPattern::BullishEngulfing => "rsi_oversold_bullish_engulfing_long",
                ReversalPattern::LowerWick => "rsi_oversold_lower_wick_long",
                ReversalPattern::BearishEngulfing => "rsi_overbought_bearish_engulfing_short",
                ReversalPattern::UpperWick => "rsi_overbought_upper_wick_short",
            },
        })
        .into_iter()
        .collect();
    RsiBranchResult {
        candidates,
        pattern,
        divergences,
    }
}

/// RSI 与 MACD 共用同一 p/q 价格枢轴，RSI 数值也固定读取枢轴 K 线而非确认 K 线。
fn rsi_divergence_candidates(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> (Vec<BranchCandidate>, Vec<RsiDivergenceSignalEvidence>) {
    let Some(pivot_idx) = latest_idx.checked_sub(DIVERGENCE_PIVOT_WING_CANDLES) else {
        return (Vec::new(), Vec::new());
    };
    let mut candidates = Vec::with_capacity(2);
    let mut evidence = Vec::with_capacity(2);
    for kind in [PivotKind::High, PivotKind::Low] {
        if !macd_v2::is_strict_price_pivot(candles, pivot_idx, kind) {
            continue;
        }
        let Some(reference_idx) = macd_v2::latest_reference_pivot(candles, pivot_idx, kind) else {
            continue;
        };
        let Some(pivot) = candles.get(pivot_idx) else {
            continue;
        };
        let Some(reference) = candles.get(reference_idx) else {
            continue;
        };
        let Some(pivot_rsi) = finite_option(pivot.rsi14) else {
            continue;
        };
        let Some(reference_rsi) = finite_option(reference.rsi14) else {
            continue;
        };
        let (direction, trigger, pivot_price, reference_price, confirmed) = match kind {
            PivotKind::High => (
                MarketVelocityTradeDirection::Short,
                "rsi_bearish_divergence_short",
                pivot.candle.high,
                reference.candle.high,
                pivot.candle.high > reference.candle.high
                    && pivot_rsi <= reference_rsi - RSI_DIVERGENCE_MIN_DELTA
                    && pivot_rsi >= RSI_OVERBOUGHT,
            ),
            PivotKind::Low => (
                MarketVelocityTradeDirection::Long,
                "rsi_bullish_divergence_long",
                pivot.candle.low,
                reference.candle.low,
                pivot.candle.low < reference.candle.low
                    && pivot_rsi >= reference_rsi + RSI_DIVERGENCE_MIN_DELTA
                    && pivot_rsi <= RSI_OVERSOLD,
            ),
        };
        if !confirmed {
            continue;
        }
        candidates.push(BranchCandidate { direction, trigger });
        evidence.push(RsiDivergenceSignalEvidence {
            comparison_mode: "confirmed_price_pivots",
            direction,
            pivot_ts_ms: pivot.candle.ts,
            reference_pivot_ts_ms: reference.candle.ts,
            pivot_price,
            reference_pivot_price: reference_price,
            pivot_rsi14: pivot_rsi,
            reference_pivot_rsi14: reference_rsi,
            pivot_filtered_volume_ratio: None,
            reference_filtered_volume_ratio: None,
            pivot_volume_ccy: None,
            reference_volume_ccy: None,
            pivot_weekly_volume_ccy_p90: None,
            reference_weekly_volume_ccy_p90: None,
            confirmation_ts_ms: None,
            confirmation_close: None,
            confirmation_break_price: None,
        });
    }
    (candidates, evidence)
}

fn bullish_reversal_pattern(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Option<ReversalPattern> {
    if bullish_engulfing(candles, latest_idx) {
        return Some(ReversalPattern::BullishEngulfing);
    }
    dominant_wick(candles.get(latest_idx)?, PivotKind::Low).then_some(ReversalPattern::LowerWick)
}

fn bearish_reversal_pattern(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Option<ReversalPattern> {
    if bearish_engulfing(candles, latest_idx) {
        return Some(ReversalPattern::BearishEngulfing);
    }
    dominant_wick(candles.get(latest_idx)?, PivotKind::High).then_some(ReversalPattern::UpperWick)
}

fn bullish_engulfing(candles: &[ComputedCandle], latest_idx: usize) -> bool {
    let Some(previous_idx) = latest_idx.checked_sub(1) else {
        return false;
    };
    let (Some(previous), Some(current)) = (candles.get(previous_idx), candles.get(latest_idx))
    else {
        return false;
    };
    valid_ohlc(previous)
        && valid_ohlc(current)
        && previous.candle.close < previous.candle.open
        && current.candle.close > current.candle.open
        && current.candle.open <= previous.candle.close
        && current.candle.close >= previous.candle.open
}

fn bearish_engulfing(candles: &[ComputedCandle], latest_idx: usize) -> bool {
    let Some(previous_idx) = latest_idx.checked_sub(1) else {
        return false;
    };
    let (Some(previous), Some(current)) = (candles.get(previous_idx), candles.get(latest_idx))
    else {
        return false;
    };
    valid_ohlc(previous)
        && valid_ohlc(current)
        && previous.candle.close > previous.candle.open
        && current.candle.close < current.candle.open
        && current.candle.open >= previous.candle.close
        && current.candle.close <= previous.candle.open
}

fn dominant_wick(candle: &ComputedCandle, kind: PivotKind) -> bool {
    if !valid_ohlc(candle) {
        return false;
    }
    let range = candle.candle.high - candle.candle.low;
    let body = (candle.candle.close - candle.candle.open).abs();
    if body / range <= DOJI_MAX_BODY_RANGE_RATIO {
        return false;
    }
    let upper = candle.candle.high - candle.candle.open.max(candle.candle.close);
    let lower = candle.candle.open.min(candle.candle.close) - candle.candle.low;
    match kind {
        PivotKind::Low => lower / range >= REVERSAL_WICK_MIN_RANGE_RATIO && lower > upper,
        PivotKind::High => upper / range >= REVERSAL_WICK_MIN_RANGE_RATIO && upper > lower,
    }
}

fn stop_loss_at_signal(
    candles: &[ComputedCandle],
    latest_idx: usize,
    entry_reference: f64,
    atr14: f64,
    direction: MarketVelocityTradeDirection,
    pattern: Option<PatternCandidate>,
) -> Result<(f64, &'static str), &'static str> {
    let latest = candles
        .get(latest_idx)
        .ok_or("filtered_volume_v3_stop_not_ready")?;
    let (price, source) = match pattern.map(|candidate| candidate.pattern) {
        Some(ReversalPattern::BullishEngulfing) => {
            let previous = candles
                .get(latest_idx - 1)
                .ok_or("filtered_volume_v3_stop_not_ready")?;
            (
                previous.candle.low.min(latest.candle.low),
                FILTERED_VOLUME_V3_BULLISH_ENGULFING_STOP_SOURCE,
            )
        }
        Some(ReversalPattern::LowerWick) => {
            (latest.candle.low, FILTERED_VOLUME_V3_LOWER_WICK_STOP_SOURCE)
        }
        Some(ReversalPattern::BearishEngulfing) => {
            let previous = candles
                .get(latest_idx - 1)
                .ok_or("filtered_volume_v3_stop_not_ready")?;
            (
                previous.candle.high.max(latest.candle.high),
                FILTERED_VOLUME_V3_BEARISH_ENGULFING_STOP_SOURCE,
            )
        }
        Some(ReversalPattern::UpperWick) => (
            latest.candle.high,
            FILTERED_VOLUME_V3_UPPER_WICK_STOP_SOURCE,
        ),
        None => {
            let distance = atr14 * FILTERED_VOLUME_STOP_ATR_MULTIPLIER;
            let price = match direction {
                MarketVelocityTradeDirection::Long => entry_reference - distance,
                MarketVelocityTradeDirection::Short => entry_reference + distance,
                MarketVelocityTradeDirection::Both => {
                    return Err("filtered_volume_v3_direction_invalid");
                }
            };
            (price, FILTERED_VOLUME_V3_ATR_STOP_SOURCE)
        }
    };
    if !positive(price) || !is_loss_side(entry_reference, price, direction) {
        return Err("filtered_volume_v3_stop_invalid");
    }
    Ok((price, source))
}

fn valid_ohlc(candle: &ComputedCandle) -> bool {
    let item = &candle.candle;
    item.open.is_finite()
        && item.high.is_finite()
        && item.low.is_finite()
        && item.close.is_finite()
        && item.open > 0.0
        && item.high > item.low
        && item.high >= item.open.max(item.close)
        && item.low <= item.open.min(item.close)
}

fn is_loss_side(entry: f64, stop: f64, direction: MarketVelocityTradeDirection) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => stop < entry,
        MarketVelocityTradeDirection::Short => stop > entry,
        MarketVelocityTradeDirection::Both => false,
    }
}

fn finite_option(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn finite_non_negative(value: f64) -> Option<f64> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        market_filtered_volume_rsi_ema_macd_v3_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_risk_config_detail,
        market_velocity_strategy_type, BacktestCandle,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION,
    };

    /// 构造所有长周期指标均已预热的连续 15m 样本，单测只改动目标规则所需字段。
    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    fn candles() -> Vec<ComputedCandle> {
        (0..700).map(candle).collect()
    }

    fn confirm_volume(candles: &mut [ComputedCandle]) {
        candles.last_mut().expect("signal candle").candle.volume = 30.0;
    }

    fn v3_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let args = market_filtered_volume_rsi_ema_macd_v3_research_args()
            .expect("v3 research args remain valid");
        signal(candles, completed_count, &args)
    }

    fn v6_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let mut args = market_filtered_volume_rsi_ema_macd_v3_research_args()
            .expect("v3 research args remain valid");
        args.paper_outcome_entry_rule_version =
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION.to_string();
        signal_with_neutral_rsi_lower_wick_long(candles, completed_count, &args)
    }

    #[test]
    fn weekly_volume_ccy_equal_to_nearest_rank_p90_is_accepted() {
        let candles = candles();
        let evidence = weekly_volume_ccy_evidence(&candles, candles.len() - 1).unwrap();

        assert_eq!(evidence.current, 100.0);
        assert_eq!(evidence.p90, 100.0);
    }

    #[test]
    fn weekly_volume_ccy_missing_value_or_time_gap_fails_closed() {
        let mut missing = candles();
        let latest_idx = missing.len() - 1;
        missing[latest_idx - 100].volume_ccy = None;
        assert_eq!(
            weekly_volume_ccy_evidence(&missing, latest_idx),
            Err("filtered_volume_v3_weekly_volume_ccy_invalid")
        );

        let mut gap = candles();
        gap[latest_idx - 100].candle.ts += 1;
        assert_eq!(
            weekly_volume_ccy_evidence(&gap, latest_idx),
            Err("filtered_volume_v3_weekly_volume_ccy_not_continuous")
        );
    }

    #[test]
    fn inclusive_oversold_engulfing_uses_two_candle_structure_stop() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        let previous = &mut candles[latest_idx - 1];
        previous.candle.open = 100.0;
        previous.candle.high = 100.5;
        previous.candle.low = 99.0;
        previous.candle.close = 99.5;
        let current = &mut candles[latest_idx];
        current.candle.open = 99.4;
        current.candle.high = 100.4;
        current.candle.low = 98.8;
        current.candle.close = 100.2;
        current.rsi14 = Some(RSI_OVERSOLD);
        confirm_volume(&mut candles);

        let signal = v3_signal(&candles, candles.len()).unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(signal.trigger, "rsi_oversold_bullish_engulfing_long");
        assert_eq!(
            signal.structure_stop_loss_source,
            FILTERED_VOLUME_V3_BULLISH_ENGULFING_STOP_SOURCE
        );
        assert!((signal.structure_stop_loss_price - 98.8).abs() < 1e-12);
        assert!(signal.evidence.rsi_pattern_stop_participated);
        assert_eq!(signal.evidence.take_profit_atr_multiplier, Some(2.7));
    }

    #[test]
    fn neutral_rsi_lower_wick_is_only_added_by_v6_and_keeps_structure_stop() {
        let mut candles = candles();
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.open = 100.0;
        latest.candle.high = 100.6;
        latest.candle.low = 98.0;
        latest.candle.close = 100.5;
        latest.rsi14 = Some(65.0);
        confirm_volume(&mut candles);

        assert_eq!(
            v3_signal(&candles, candles.len()),
            Err("filtered_volume_v3_no_branch_signal")
        );
        let signal = v6_signal(&candles, candles.len()).unwrap();
        assert_eq!(signal.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(signal.trigger, "neutral_rsi_lower_wick_long");
        assert_eq!(
            signal.structure_stop_loss_source,
            FILTERED_VOLUME_V3_LOWER_WICK_STOP_SOURCE
        );
        assert!((signal.structure_stop_loss_price - 98.0).abs() < 1e-12);
    }

    #[test]
    fn neutral_rsi_lower_wick_accepts_the_non_overbought_boundary() {
        let mut candles = candles();
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.open = 100.0;
        latest.candle.high = 100.6;
        latest.candle.low = 98.0;
        latest.candle.close = 100.5;
        latest.rsi14 = Some(RSI_OVERBOUGHT);
        confirm_volume(&mut candles);

        let signal = v6_signal(&candles, candles.len()).unwrap();
        assert_eq!(signal.trigger, "neutral_rsi_lower_wick_long");
    }

    #[test]
    fn neutral_rsi_lower_wick_does_not_replace_existing_v3_divergence() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        let pivot_idx = latest_idx - DIVERGENCE_PIVOT_WING_CANDLES;
        let reference_idx = pivot_idx - 16;
        candles[reference_idx].candle.low = 97.0;
        candles[reference_idx].rsi14 = Some(25.0);
        candles[pivot_idx].candle.low = 96.0;
        candles[pivot_idx].rsi14 = Some(27.0);
        let latest = &mut candles[latest_idx];
        latest.candle.open = 100.0;
        latest.candle.high = 100.6;
        latest.candle.low = 98.0;
        latest.candle.close = 100.5;
        latest.rsi14 = Some(65.0);
        confirm_volume(&mut candles);

        let v3 = v3_signal(&candles, candles.len()).unwrap();
        let v6 = v6_signal(&candles, candles.len()).unwrap();

        assert_eq!(v6.trigger, v3.trigger);
        assert_eq!(v6.direction, v3.direction);
        assert_eq!(v6.structure_stop_loss_price, v3.structure_stop_loss_price);
        assert_eq!(v6.structure_stop_loss_source, v3.structure_stop_loss_source);
    }

    #[test]
    fn rsi_divergence_uses_confirmed_p_and_latest_q_without_future_data() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        let pivot_idx = latest_idx - DIVERGENCE_PIVOT_WING_CANDLES;
        let reference_idx = pivot_idx - 16;
        candles[reference_idx].candle.low = 97.0;
        candles[reference_idx].rsi14 = Some(25.0);
        candles[pivot_idx].candle.low = 96.0;
        candles[pivot_idx].rsi14 = Some(27.0);
        confirm_volume(&mut candles);

        let before = v3_signal(&candles, candles.len()).unwrap();
        let completed_count = candles.len();
        let mut future = candle(completed_count);
        future.candle.low = 1.0;
        future.rsi14 = Some(99.0);
        candles.push(future);
        let after = v3_signal(&candles, completed_count).unwrap();

        assert_eq!(before, after);
        assert_eq!(before.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(before.trigger, "rsi_bullish_divergence_long");
        assert_eq!(
            before.structure_stop_loss_source,
            FILTERED_VOLUME_V3_ATR_STOP_SOURCE
        );
        assert_eq!(before.evidence.rsi_divergences.len(), 1);
        assert_eq!(
            before.evidence.rsi_divergences[0].pivot_ts_ms,
            candles[pivot_idx].candle.ts
        );
        assert_eq!(
            before.evidence.rsi_divergences[0].reference_pivot_ts_ms,
            candles[reference_idx].candle.ts
        );
    }

    #[test]
    fn fixed_atr_target_tiers_use_exact_boundaries() {
        assert_eq!(target_atr_multiplier(2.9999), None);
        assert_eq!(target_atr_multiplier(3.0), Some(2.7));
        assert_eq!(target_atr_multiplier(3.9999), Some(2.7));
        assert_eq!(target_atr_multiplier(4.0), Some(3.6));
        assert_eq!(target_atr_multiplier(5.9999), Some(3.6));
        assert_eq!(target_atr_multiplier(6.0), Some(4.5));
    }

    #[test]
    fn v3_has_independent_research_identity_and_no_max_holding_time() {
        let args = market_filtered_volume_rsi_ema_macd_v3_research_args().unwrap();
        assert_eq!(args.equity_max_holding_hours, None);
        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY
        );

        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET,
        )
        .unwrap();
        assert_eq!(
            manifest.strategy_key,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY
        );
        assert_eq!(manifest.channel, "research");
        assert_eq!(
            manifest.manifest_json["parameters"]["fast_momentum_filters"]
                ["filtered_volume_rsi_ema_macd"]["weekly_volume_ccy_gate"]["p90_zero_based_index"],
            WEEKLY_VOLUME_CCY_P90_INDEX
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["take_profit"]["mode"],
            "fixed_atr_distance_by_filtered_volume_ratio"
        );

        let risk = market_velocity_risk_config_detail(&args, 1.0);
        assert_eq!(risk["account_risk_fraction_per_trade"], 0.01);
        assert!(risk.to_string().len() <= 1_000);
    }
}
