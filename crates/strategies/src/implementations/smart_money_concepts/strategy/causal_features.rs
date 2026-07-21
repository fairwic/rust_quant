use super::{average_true_range, CandleItem, CausalMarketStructureFeatures};

const ATR_PERIOD: usize = 14;
const MIN_FVG_GAP_ATR: f64 = 0.10;
const MIN_DISPLACEMENT_BODY_ATR: f64 = 0.80;

/// 使用逐根可确认的交替 pivot 状态机提取结构特征，避免未来函数和错配波段。
pub(super) fn causal_market_structure_features(
    candles: &[CandleItem],
    pivot_wing: usize,
) -> CausalMarketStructureFeatures {
    causal_market_structure_feature_series(candles, pivot_wing)
        .last()
        .copied()
        .unwrap_or_default()
}

/// pivot、CHoCH 与 FVG 状态只向前推进；每个输出位置只依赖该位置及此前完成 K 线。
pub(super) fn causal_market_structure_feature_series(
    candles: &[CandleItem],
    pivot_wing: usize,
) -> Vec<CausalMarketStructureFeatures> {
    let candidates = confirmed_causal_pivots(candles, pivot_wing);
    let mut alternating = Vec::<CausalPivot>::new();
    let mut candidate_idx = 0usize;
    let mut last_broken_high_idx = None::<usize>;
    let mut active_choch = None::<ActiveBullishChoch>;
    let mut active_fvgs = Vec::<ActiveBullishFvg>::new();
    let mut series = Vec::with_capacity(candles.len());
    for bar_idx in 0..candles.len() {
        let mut features = CausalMarketStructureFeatures::default();
        update_active_bullish_fvgs(candles, bar_idx, &mut active_fvgs, &mut features);
        while candidates
            .get(candidate_idx)
            .is_some_and(|pivot| pivot.confirmed_at == bar_idx)
        {
            push_alternating_pivot(&mut alternating, candidates[candidate_idx]);
            candidate_idx += 1;
        }
        if let Some(active) = active_choch {
            if candles[bar_idx].c < active.protected_low {
                active_choch = None;
            }
        }
        if let (Some(previous_idx), Some(high)) = (
            bar_idx.checked_sub(1),
            alternating
                .iter()
                .rev()
                .find(|pivot| pivot.kind == CausalPivotKind::High)
                .copied(),
        ) {
            if last_broken_high_idx != Some(high.index)
                && candles[previous_idx].c <= high.level
                && candles[bar_idx].c > high.level
            {
                let prior = paired_structure_bias(&alternating);
                last_broken_high_idx = Some(high.index);
                if prior == CausalStructureBias::Bearish {
                    if let Some(protected_low) = alternating
                        .iter()
                        .rev()
                        .find(|pivot| pivot.kind == CausalPivotKind::Low)
                        .map(|pivot| pivot.level)
                    {
                        active_choch = Some(ActiveBullishChoch {
                            event_idx: bar_idx,
                            break_level: high.level,
                            protected_low,
                        });
                    }
                }
                features.bullish_structure_break = true;
                features.bullish_bos = prior == CausalStructureBias::Bullish;
                features.bullish_choch = prior == CausalStructureBias::Bearish;
                features.prior_bearish_structure = prior == CausalStructureBias::Bearish;
                features.prior_bullish_structure = prior == CausalStructureBias::Bullish;
                features.bullish_structure_break_margin_atr =
                    average_true_range(&candles[..=bar_idx], ATR_PERIOD)
                        .filter(|atr| *atr > 0.0)
                        .map(|atr| (candles[bar_idx].c - high.level) / atr);
            }
        }
        features.latest_confirmed_swing_high = alternating
            .iter()
            .rev()
            .find(|pivot| pivot.kind == CausalPivotKind::High)
            .map(|pivot| pivot.level);
        features.latest_confirmed_swing_low = alternating
            .iter()
            .rev()
            .find(|pivot| pivot.kind == CausalPivotKind::Low)
            .map(|pivot| pivot.level);
        if let Some(active) = active_choch {
            features.bullish_choch_active = true;
            features.bullish_choch_age_bars = Some(bar_idx - active.event_idx);
            features.bullish_choch_break_level = Some(active.break_level);
        }
        series.push(features);
    }
    series
}

/// 只创建在当前时点已经由右侧 K 线确认的 pivot；双向 outside bar 因顺序未知而跳过。
fn confirmed_causal_pivots(candles: &[CandleItem], pivot_wing: usize) -> Vec<CausalPivot> {
    if pivot_wing == 0 || candles.len() < pivot_wing.saturating_mul(2).saturating_add(1) {
        return Vec::new();
    }
    let mut pivots = Vec::new();
    for center in pivot_wing..candles.len() - pivot_wing {
        let candle = &candles[center];
        let mut neighbours = candles[center - pivot_wing..center]
            .iter()
            .chain(&candles[center + 1..=center + pivot_wing]);
        let high = neighbours.clone().all(|item| candle.h >= item.h)
            && neighbours.clone().any(|item| candle.h > item.h);
        let low = neighbours.clone().all(|item| candle.l <= item.l)
            && neighbours.any(|item| candle.l < item.l);
        if high == low {
            continue;
        }
        pivots.push(CausalPivot {
            index: center,
            confirmed_at: center + pivot_wing,
            level: if high { candle.h } else { candle.l },
            kind: if high {
                CausalPivotKind::High
            } else {
                CausalPivotKind::Low
            },
        });
    }
    pivots
}

/// 相邻同类 pivot 只保留更极端且更晚确认的一个，形成可审计的交替摆动序列。
fn push_alternating_pivot(pivots: &mut Vec<CausalPivot>, candidate: CausalPivot) {
    let Some(latest) = pivots.last_mut() else {
        pivots.push(candidate);
        return;
    };
    if latest.kind != candidate.kind {
        pivots.push(candidate);
        return;
    }
    let replace = match candidate.kind {
        CausalPivotKind::High => candidate.level >= latest.level,
        CausalPivotKind::Low => candidate.level <= latest.level,
    };
    if replace {
        *latest = candidate;
    }
}

/// 只接受 H-L-H-L 的完整两轮结构，避免把不同波段的高低点独立拼成趋势。
fn paired_structure_bias(pivots: &[CausalPivot]) -> CausalStructureBias {
    let Some(sequence) = pivots.get(pivots.len().saturating_sub(4)..) else {
        return CausalStructureBias::Unknown;
    };
    if sequence.len() != 4
        || sequence[0].kind != CausalPivotKind::High
        || sequence[1].kind != CausalPivotKind::Low
        || sequence[2].kind != CausalPivotKind::High
        || sequence[3].kind != CausalPivotKind::Low
    {
        return CausalStructureBias::Unknown;
    }
    if sequence[2].level > sequence[0].level && sequence[3].level > sequence[1].level {
        CausalStructureBias::Bullish
    } else if sequence[2].level < sequence[0].level && sequence[3].level < sequence[1].level {
        CausalStructureBias::Bearish
    } else {
        CausalStructureBias::Unknown
    }
}

/// 更新全部未填补 FVG，并把当前最新有效区域写入本根结构快照。
fn update_active_bullish_fvgs(
    candles: &[CandleItem],
    bar_idx: usize,
    active_fvgs: &mut Vec<ActiveBullishFvg>,
    features: &mut CausalMarketStructureFeatures,
) {
    for active in active_fvgs.iter_mut() {
        if bar_idx > active.formation_idx {
            active.mitigated_pct = active
                .mitigated_pct
                .max((active.upper - candles[bar_idx].l) / (active.upper - active.lower) * 100.0)
                .clamp(0.0, 100.0);
        }
    }
    active_fvgs.retain(|active| active.mitigated_pct < 100.0);
    if let Some(zone) = valid_bullish_fvg(candles, bar_idx) {
        features.bullish_fvg = true;
        features.bullish_fvg_lower = Some(zone.lower);
        features.bullish_fvg_upper = Some(zone.upper);
        features.bullish_fvg_gap_atr = Some(zone.gap_atr);
        features.bullish_fvg_displacement_body_atr = Some(zone.displacement_body_atr);
        active_fvgs.push(ActiveBullishFvg {
            formation_idx: bar_idx,
            lower: zone.lower,
            upper: zone.upper,
            mitigated_pct: 0.0,
        });
    }
    if let Some(active) = active_fvgs.last() {
        features.active_bullish_fvg_lower = Some(active.lower);
        features.active_bullish_fvg_upper = Some(active.upper);
        features.active_bullish_fvg_age_bars = Some(bar_idx - active.formation_idx);
        features.active_bullish_fvg_mitigated_pct = Some(active.mitigated_pct);
    }
}

/// 要求中间位移 K 线向上、实体充分且 gap 相对 ATR 可见，避免把微小噪声当 FVG。
fn valid_bullish_fvg(candles: &[CandleItem], formation_idx: usize) -> Option<ValidBullishFvg> {
    let latest = candles.get(formation_idx)?;
    let displacement = candles.get(formation_idx.checked_sub(1)?)?;
    let anchor = candles.get(formation_idx.checked_sub(2)?)?;
    let atr = average_true_range(&candles[..=formation_idx], ATR_PERIOD)?;
    if atr <= 0.0 || latest.l <= anchor.h || displacement.c <= displacement.o {
        return None;
    }
    let gap_atr = (latest.l - anchor.h) / atr;
    let displacement_body_atr = (displacement.c - displacement.o) / atr;
    if displacement.c <= anchor.h
        || gap_atr < MIN_FVG_GAP_ATR
        || displacement_body_atr < MIN_DISPLACEMENT_BODY_ATR
    {
        return None;
    }
    Some(ValidBullishFvg {
        lower: anchor.h,
        upper: latest.l,
        gap_atr,
        displacement_body_atr,
    })
}

/// 逐根确认的 pivot 类型；高低点必须交替后才参与结构趋势判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CausalPivotKind {
    High,
    Low,
}

/// 带确认时点的因果 pivot，确保中心 K 线不会在右侧窗口完成前提前可见。
#[derive(Debug, Clone, Copy, PartialEq)]
struct CausalPivot {
    index: usize,
    confirmed_at: usize,
    level: f64,
    kind: CausalPivotKind,
}

/// 最近两轮完整交替摆动形成的结构方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CausalStructureBias {
    Unknown,
    Bearish,
    Bullish,
}

/// 仍未被保护低点收盘否定的多头 CHoCH 状态。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveBullishChoch {
    event_idx: usize,
    break_level: f64,
    protected_low: f64,
}

/// 已满足位移与 ATR 尺度要求的新生多头 FVG。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ValidBullishFvg {
    lower: f64,
    upper: f64,
    gap_atr: f64,
    displacement_body_atr: f64,
}

/// 尚未完全填补的多头 FVG 状态。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveBullishFvg {
    formation_idx: usize,
    lower: f64,
    upper: f64,
    mitigated_pct: f64,
}
