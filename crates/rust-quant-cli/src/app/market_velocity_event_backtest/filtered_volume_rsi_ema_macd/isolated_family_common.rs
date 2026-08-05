use super::super::{
    AnchorEntrySignalEvidence, CompletedCandleEntrySignalEvidence, ComputedCandle,
    IsolatedStrategyFamilySignalEvidence, MarketVelocityTradeDirection,
    RsiDivergenceSignalEvidence,
};
use super::weekly_base_volume_v3::{
    filtered_volume_evidence, weekly_volume_ccy_evidence, WeeklyBaseVolumeEvidence,
};
use super::{
    positive_indicator, FilteredVolumeEvidence, FilteredVolumeRsiEmaMacdSignal,
    DOJI_MAX_BODY_RANGE_RATIO, FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
    FILTERED_VOLUME_V3_ATR_STOP_SOURCE, REVERSAL_WICK_MIN_RANGE_RATIO,
};

/// 三个独立家族共同冻结的过滤量比门槛。
pub(super) const ISOLATED_FILTERED_VOLUME_MIN_RATIO: f64 = 2.5;
/// 三个独立家族共同使用 1.5 ATR 初始风险与 1.5 ATR 毛目标，即固定 1R。
pub(super) const ISOLATED_FIXED_TARGET_ATR_MULTIPLIER: f64 = 1.5;

/// 校验当前候选 K 的因果过滤量比与周成交额 P90，并返回可落库证据。
pub(super) fn current_volume_gate(
    candles: &[ComputedCandle],
    candidate_idx: usize,
) -> Result<(FilteredVolumeEvidence, WeeklyBaseVolumeEvidence), &'static str> {
    let volume =
        filtered_volume_evidence(candles, candidate_idx, ISOLATED_FILTERED_VOLUME_MIN_RATIO)?;
    if volume.ratio < ISOLATED_FILTERED_VOLUME_MIN_RATIO {
        return Err("isolated_family_filtered_volume_not_confirmed");
    }
    let weekly_volume = weekly_volume_ccy_evidence(candles, candidate_idx)?;
    Ok((volume, weekly_volume))
}

/// 根据 setup K 的方向性长影线冻结“下一开盘”或“下一根盘中触价”成交方式。
pub(super) fn anchor_entry_evidence(
    pivot: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
) -> Result<(AnchorEntrySignalEvidence, bool), &'static str> {
    let range = pivot.candle.high - pivot.candle.low;
    if !range.is_finite() || range <= 0.0 {
        return Err("isolated_family_pivot_range_invalid");
    }
    let body = (pivot.candle.close - pivot.candle.open).abs();
    let upper_wick = pivot.candle.high - pivot.candle.open.max(pivot.candle.close);
    let lower_wick = pivot.candle.open.min(pivot.candle.close) - pivot.candle.low;
    let body_ratio = body / range;
    let upper_wick_ratio = upper_wick.max(0.0) / range;
    let lower_wick_ratio = lower_wick.max(0.0) / range;
    let is_not_doji = body_ratio > DOJI_MAX_BODY_RANGE_RATIO;

    let (activation_price, directional_wick_ratio, opposite_wick_ratio, direct_wick_entry) =
        match direction {
            MarketVelocityTradeDirection::Long => (
                pivot.candle.high,
                lower_wick_ratio,
                upper_wick_ratio,
                is_not_doji
                    && lower_wick_ratio >= REVERSAL_WICK_MIN_RANGE_RATIO
                    && lower_wick_ratio > upper_wick_ratio,
            ),
            MarketVelocityTradeDirection::Short => (
                pivot.candle.low,
                upper_wick_ratio,
                lower_wick_ratio,
                is_not_doji
                    && upper_wick_ratio >= REVERSAL_WICK_MIN_RANGE_RATIO
                    && upper_wick_ratio > lower_wick_ratio,
            ),
            MarketVelocityTradeDirection::Both => {
                return Err("isolated_family_direction_invalid");
            }
        };
    if !activation_price.is_finite() || activation_price <= 0.0 {
        return Err("isolated_family_activation_price_invalid");
    }

    Ok((
        AnchorEntrySignalEvidence {
            activation_mode: if direct_wick_entry {
                "pivot_directional_wick_next_open"
            } else {
                "next_candle_intrabar_break"
            },
            pivot_body_range_ratio: body_ratio,
            pivot_directional_wick_range_ratio: directional_wick_ratio,
            pivot_opposite_wick_range_ratio: opposite_wick_ratio,
            activation_price,
            activation_candle_ts_ms: None,
            fill_price: None,
            fill_price_source: None,
            intrabar_path_policy: (!direct_wick_entry)
                .then_some("full_15m_bar_conservative_stop_first"),
        },
        direct_wick_entry,
    ))
}

/// 用共同的 1.5 ATR 风险/目标合同封装某一个独立入场假设的信号。
pub(super) fn fixed_one_r_signal(
    latest: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    trigger: &'static str,
    volume: FilteredVolumeEvidence,
    weekly_volume: WeeklyBaseVolumeEvidence,
    rsi_divergences: Vec<RsiDivergenceSignalEvidence>,
    anchor_entry: Option<AnchorEntrySignalEvidence>,
    isolated_family: IsolatedStrategyFamilySignalEvidence,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    atr_target_signal(
        latest,
        direction,
        trigger,
        volume,
        weekly_volume,
        rsi_divergences,
        anchor_entry,
        isolated_family,
        ISOLATED_FIXED_TARGET_ATR_MULTIPLIER,
        true,
    )
}

/// 用冻结的 1.5 ATR 初始风险和显式 ATR 目标封装独立家族信号。
///
/// 目标倍数必须在信号 K 完成时确定；成交等待期间不得根据后续 K 线重新分档。
/// `capture_legacy_indicators=false` 时，旧宽表指标字段只写零值，不读取 RSI/MACD/EMA。
pub(super) fn atr_target_signal(
    latest: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    trigger: &'static str,
    volume: FilteredVolumeEvidence,
    weekly_volume: WeeklyBaseVolumeEvidence,
    rsi_divergences: Vec<RsiDivergenceSignalEvidence>,
    anchor_entry: Option<AnchorEntrySignalEvidence>,
    isolated_family: IsolatedStrategyFamilySignalEvidence,
    take_profit_atr_multiplier: f64,
    capture_legacy_indicators: bool,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    let entry_price = latest.candle.close;
    if !entry_price.is_finite() || entry_price <= 0.0 {
        return Err("isolated_family_close_invalid");
    }
    if !take_profit_atr_multiplier.is_finite() || take_profit_atr_multiplier <= 0.0 {
        return Err("isolated_family_target_atr_invalid");
    }
    let atr14 = positive_indicator(latest.atr14).ok_or("isolated_family_atr14_not_ready")?;
    let stop_distance = atr14 * FILTERED_VOLUME_STOP_ATR_MULTIPLIER;
    let stop_price = match direction {
        MarketVelocityTradeDirection::Long => entry_price - stop_distance,
        MarketVelocityTradeDirection::Short => entry_price + stop_distance,
        MarketVelocityTradeDirection::Both => return Err("isolated_family_direction_invalid"),
    };
    if !stop_price.is_finite() || stop_price <= 0.0 {
        return Err("isolated_family_atr_stop_invalid");
    }

    Ok(FilteredVolumeRsiEmaMacdSignal {
        direction,
        trigger: trigger.to_string(),
        structure_stop_loss_price: stop_price,
        structure_stop_loss_source: FILTERED_VOLUME_V3_ATR_STOP_SOURCE,
        evidence: CompletedCandleEntrySignalEvidence {
            filtered_volume_ratio: volume.ratio,
            filtered_volume_retained_candles: volume.retained_candles,
            current_volume_ccy: Some(weekly_volume.current),
            weekly_volume_ccy_p90: Some(weekly_volume.p90),
            rsi14: if capture_legacy_indicators {
                finite_or_zero(latest.rsi14)
            } else {
                0.0
            },
            macd_dif: if capture_legacy_indicators {
                finite_or_zero(latest.macd_line)
            } else {
                0.0
            },
            ema12: if capture_legacy_indicators {
                finite_or_zero(latest.ema12)
            } else {
                0.0
            },
            ema144: if capture_legacy_indicators {
                finite_or_zero(latest.ema144)
            } else {
                0.0
            },
            ema169: if capture_legacy_indicators {
                finite_or_zero(latest.ema169)
            } else {
                0.0
            },
            ema696: if capture_legacy_indicators {
                finite_or_zero(latest.ema696)
            } else {
                0.0
            },
            atr14,
            take_profit_atr_multiplier: Some(take_profit_atr_multiplier),
            rsi_pattern_stop_participated: false,
            rsi_divergences,
            macd_divergences: Vec::new(),
            bollinger_conflict: None,
            ema144_distance_atr: None,
            ema144_max_distance_atr: None,
            ema_candidate_blocked_by_distance: false,
            anchor_entry,
            trend_managed_exit: None,
            isolated_family: Some(isolated_family),
        },
    })
}

/// 非本家族所需的指标只作为快照写入；未预热不得反向阻塞独立信号。
fn finite_or_zero(value: Option<f64>) -> f64 {
    value.filter(|item| item.is_finite()).unwrap_or(0.0)
}
