use super::directional_reversal::volume_atr_target_r_with_policy;
use super::rsi_volume_regime::{rsi_volume_regime_signal_for_version, rsi_volume_regime_version};
use super::{
    BacktestCandle, CompletedCandleEntrySignalEvidence, ComputedCandle, ConfirmedEvent,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION,
};

mod isolated_family_common;
mod macd_v2;
pub(super) mod momentum_exhaustion_reversal_v1;
pub(super) mod momentum_exhaustion_reversal_v2;
pub(super) mod momentum_exhaustion_reversal_v3;
pub(super) mod volume_anchor_rsi_divergence_reversal_v1;
pub(super) mod volume_anchor_rsi_divergence_reversal_v2;
pub(super) mod volume_platform_break_trend_v1;
pub(super) mod volume_platform_break_trend_v2;
pub(super) mod weekly_base_volume_bollinger_conflict_v4;
pub(super) mod weekly_base_volume_ema144_proximity_v5;
pub(super) mod weekly_base_volume_v3;
mod weekly_p90_anchor_rsi_divergence_next_close_v10;
mod weekly_p90_anchor_rsi_divergence_v9;
mod weekly_p90_anchor_rsi_divergence_wick_or_touch_v11;
pub(super) mod weekly_p90_anchor_rsi_trend_managed_counter15_v13;
pub(super) mod weekly_p90_anchor_rsi_trend_managed_v12;
pub(super) use weekly_p90_anchor_rsi_divergence_v9::FILTERED_VOLUME_V9_MIN_RATIO;

/// 历史异常量标记与当前基线都只观察紧邻信号前的十根已完成 K 线。
pub(super) const FILTERED_VOLUME_HISTORY_CANDLES: usize = 10;
/// 历史 K 线和当前信号 K 线都以三倍均量作为放量阈值。
pub(super) const FILTERED_VOLUME_MIN_RATIO: f64 = 3.0;
/// 剔除已标记历史放量后，至少保留五根才能形成当前成交量基线。
pub(super) const FILTERED_VOLUME_MIN_RETAINED_CANDLES: usize = 5;
/// RSI 与 MACD 背离只在当前信号前的 48 根已完成 K 线中寻找价格枢轴。
pub(super) const DIVERGENCE_LOOKBACK_CANDLES: usize = 48;
/// 历史价格枢轴必须由左右各三根更早的已完成 K 线确认。
pub(super) const DIVERGENCE_PIVOT_WING_CANDLES: usize = 3;
/// RSI 背离要求当前 RSI 相对同一价格枢轴至少改善一分。
pub(super) const RSI_DIVERGENCE_MIN_DELTA: f64 = 1.0;
/// RSI 极值反转影线必须至少占信号 K 线完整振幅的 60%。
pub(super) const REVERSAL_WICK_MIN_RANGE_RATIO: f64 = 0.60;
/// 实体不超过完整振幅 10% 时视为十字星，不允许进入影线反转分支。
pub(super) const DOJI_MAX_BODY_RANGE_RATIO: f64 = 0.10;
/// EMA 延续分支要求实体至少占完整振幅的 60%。
pub(super) const EMA_BODY_MIN_RANGE_RATIO: f64 = 0.60;
/// EMA 延续分支的信号实体相对开盘价必须严格大于 1%。
pub(super) const EMA_BODY_MIN_OPEN_RATIO: f64 = 0.01;
/// EMA 延续分支的信号实体相对开盘价必须严格小于 3%。
pub(super) const EMA_BODY_MAX_OPEN_RATIO: f64 = 0.03;
/// 全部分支共用信号收盘价反方向 1.5 倍 ATR14 的初始止损。
pub(super) const FILTERED_VOLUME_STOP_ATR_MULTIPLIER: f64 = 1.5;
/// 明细用该来源识别纯 ATR 止损，禁止通用选择器静默改回固定百分比。
pub(super) const FILTERED_VOLUME_ATR_STOP_SOURCE: &str = "filtered_volume_rsi_ema_macd_atr14_1_5";
/// v3 非形态信号在实际成交价反方向固定 1.5 ATR。
pub(super) const FILTERED_VOLUME_V3_ATR_STOP_SOURCE: &str = "filtered_volume_v3_atr14_1_5";
/// v3 看涨吞没使用组成形态的两根 K 线最低点。
pub(super) const FILTERED_VOLUME_V3_BULLISH_ENGULFING_STOP_SOURCE: &str =
    "filtered_volume_v3_bullish_engulfing_low";
/// v3 长下影线使用当前信号 K 线最低点。
pub(super) const FILTERED_VOLUME_V3_LOWER_WICK_STOP_SOURCE: &str =
    "filtered_volume_v3_lower_wick_low";
/// v3 看跌吞没使用组成形态的两根 K 线最高点。
pub(super) const FILTERED_VOLUME_V3_BEARISH_ENGULFING_STOP_SOURCE: &str =
    "filtered_volume_v3_bearish_engulfing_high";
/// v3 长上影线使用当前信号 K 线最高点。
pub(super) const FILTERED_VOLUME_V3_UPPER_WICK_STOP_SOURCE: &str =
    "filtered_volume_v3_upper_wick_high";
/// 形态保护位在下一根实际成交价处已经失效时，保留一笔立即退出的双边成本记录。
pub(super) const FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE: &str =
    "filtered_volume_v3_invalid_structure_stop_at_fill";

pub(super) const RSI_OVERSOLD: f64 = 30.0;
pub(super) const RSI_OVERBOUGHT: f64 = 70.0;
pub(super) const MACD_RSI_NEUTRAL_MIN: f64 = 40.0;
pub(super) const MACD_RSI_NEUTRAL_MAX: f64 = 60.0;

/// 新策略在信号时点冻结的方向、触发分支、ATR 止损和指标证据。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FilteredVolumeRsiEmaMacdSignal {
    /// 合并冲突后唯一可交易方向。
    pub(super) direction: MarketVelocityTradeDirection,
    /// 同向候选分支按 RSI、EMA、MACD 顺序合并后的稳定标签。
    pub(super) trigger: String,
    /// 信号收盘价反方向 1.5 ATR14 的止损价格。
    pub(super) structure_stop_loss_price: f64,
    /// 固定的纯 ATR 止损来源。
    pub(super) structure_stop_loss_source: &'static str,
    /// 入场判断使用的过滤量比与指标快照。
    pub(super) evidence: CompletedCandleEntrySignalEvidence,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct CompletedCandleEntrySignal {
    pub(super) direction: MarketVelocityTradeDirection,
    pub(super) trigger: String,
    pub(super) structure_stop_loss_price: f64,
    pub(super) structure_stop_loss_source: String,
    pub(super) evidence: Option<CompletedCandleEntrySignalEvidence>,
}

/// 在独立新策略与冻结 RSI 旧版本之间做显式版本分派，避免任一版本继承另一方入场逻辑。
pub(super) fn completed_candle_entry_signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<CompletedCandleEntrySignal, &'static str> {
    if args.entry_filtered_volume_rsi_ema_macd {
        let signal = filtered_volume_rsi_ema_macd_signal(candles, completed_count, args)?;
        return Ok(CompletedCandleEntrySignal {
            direction: signal.direction,
            trigger: signal.trigger,
            structure_stop_loss_price: signal.structure_stop_loss_price,
            structure_stop_loss_source: signal.structure_stop_loss_source.to_string(),
            evidence: Some(signal.evidence),
        });
    }
    let signal = rsi_volume_regime_signal_for_version(
        candles,
        completed_count,
        args.entry_min_volume_ratio,
        args.stop_loss_pct,
        rsi_volume_regime_version(&args.paper_outcome_entry_rule_version),
    )?;
    Ok(CompletedCandleEntrySignal {
        direction: signal.direction,
        trigger: signal.trigger,
        structure_stop_loss_price: signal.structure_stop_loss_price,
        structure_stop_loss_source: signal.structure_stop_loss_source.to_string(),
        evidence: None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 单个指标分支提出的方向候选；最终仍需经过多空冲突合并。
pub(super) struct BranchCandidate {
    /// 分支建议的开仓方向。
    pub(super) direction: MarketVelocityTradeDirection,
    /// 可审计的分支触发标签。
    pub(super) trigger: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// 当前 K 相对过滤后十根历史基线的成交量证据。
pub(super) struct FilteredVolumeEvidence {
    /// 当前成交量除以剔除历史异常量后的平均成交量。
    pub(super) ratio: f64,
    /// 剔除异常量后实际进入平均值的历史 K 线数量。
    pub(super) retained_candles: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PivotKind {
    Low,
    High,
}

/// 只读取 `completed_count` 之前的已完成 15m K 线，按三个并行分支生成唯一方向信号。
pub(super) fn filtered_volume_rsi_ema_macd_signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if args.paper_outcome_entry_rule_version
        == MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION
    {
        return momentum_exhaustion_reversal_v1::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
    {
        return momentum_exhaustion_reversal_v2::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
    {
        return momentum_exhaustion_reversal_v3::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION
    {
        return volume_anchor_rsi_divergence_reversal_v1::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION
    {
        return volume_anchor_rsi_divergence_reversal_v2::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION
    {
        return volume_platform_break_trend_v1::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION
    {
        return volume_platform_break_trend_v2::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION
    {
        return weekly_base_volume_v3::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION
    {
        return weekly_base_volume_bollinger_conflict_v4::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION
    {
        return weekly_base_volume_ema144_proximity_v5::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION
        || args.paper_outcome_entry_rule_version
            == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION
    {
        return weekly_base_volume_v3::signal_with_neutral_rsi_lower_wick_long(
            candles,
            completed_count,
            args,
        );
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_ENTRY_RULE_VERSION
    {
        return weekly_base_volume_v3::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION
    {
        return weekly_p90_anchor_rsi_divergence_v9::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
    {
        return weekly_p90_anchor_rsi_divergence_next_close_v10::signal(
            candles,
            completed_count,
            args,
        );
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
    {
        return weekly_p90_anchor_rsi_divergence_wick_or_touch_v11::signal(
            candles,
            completed_count,
            args,
        );
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
    {
        return weekly_p90_anchor_rsi_trend_managed_v12::signal(candles, completed_count, args);
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
    {
        return weekly_p90_anchor_rsi_trend_managed_counter15_v13::signal(
            candles,
            completed_count,
            args,
        );
    }
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("filtered_volume_strategy_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("filtered_volume_strategy_not_ready")?;
    let volume = filtered_current_volume_evidence(candles, latest_idx)?;
    if volume.ratio < FILTERED_VOLUME_MIN_RATIO {
        return Err("filtered_volume_strategy_volume_not_confirmed");
    }

    let rsi14 = latest
        .rsi14
        .filter(|value| value.is_finite())
        .ok_or("filtered_volume_strategy_rsi_not_ready")?;
    let macd_dif = latest
        .macd_line
        .filter(|value| value.is_finite())
        .ok_or("filtered_volume_strategy_macd_not_ready")?;
    let ema12 = positive_indicator(latest.ema12).ok_or("filtered_volume_strategy_ema_not_ready")?;
    let ema144 =
        positive_indicator(latest.ema144).ok_or("filtered_volume_strategy_ema_not_ready")?;
    let ema169 =
        positive_indicator(latest.ema169).ok_or("filtered_volume_strategy_ema_not_ready")?;
    let ema696 =
        positive_indicator(latest.ema696).ok_or("filtered_volume_strategy_ema_not_ready")?;

    let mut candidates = Vec::with_capacity(3);
    if let Some(candidate) = rsi_candidate(candles, latest_idx, rsi14) {
        candidates.push(candidate);
    }
    if let Some(candidate) = ema_candidate(latest, rsi14, ema12, ema144, ema696) {
        candidates.push(candidate);
    }
    let (macd_candidates, macd_divergences) = if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION
    {
        match (
            args.entry_filtered_volume_macd_zero_band_atr_multiplier,
            args.entry_filtered_volume_macd_min_normalized_dif_improvement,
        ) {
            (Some(zero_band_multiplier), Some(min_improvement)) => {
                macd_v2::macd_candidates(candles, latest_idx, zero_band_multiplier, min_improvement)
            }
            _ => (Vec::new(), Vec::new()),
        }
    } else {
        (
            legacy_macd_candidates(candles, latest_idx, rsi14, macd_dif),
            Vec::new(),
        )
    };
    candidates.extend(macd_candidates);

    let has_long = candidates
        .iter()
        .any(|candidate| candidate.direction == MarketVelocityTradeDirection::Long);
    let has_short = candidates
        .iter()
        .any(|candidate| candidate.direction == MarketVelocityTradeDirection::Short);
    let direction = match (has_long, has_short) {
        (true, false) => MarketVelocityTradeDirection::Long,
        (false, true) => MarketVelocityTradeDirection::Short,
        (true, true) => return Err("filtered_volume_strategy_direction_conflict"),
        (false, false) => return Err("filtered_volume_strategy_no_branch_signal"),
    };
    let trigger = candidates
        .iter()
        .filter(|candidate| candidate.direction == direction)
        .map(|candidate| candidate.trigger)
        .collect::<Vec<_>>()
        .join("+");
    let entry_price = latest.candle.close;
    let atr14 = positive_indicator(latest.atr14).ok_or("filtered_volume_strategy_atr_not_ready")?;
    let stop_distance = atr14 * FILTERED_VOLUME_STOP_ATR_MULTIPLIER;
    let structure_stop_loss_price = match direction {
        MarketVelocityTradeDirection::Long => entry_price - stop_distance,
        MarketVelocityTradeDirection::Short => entry_price + stop_distance,
        MarketVelocityTradeDirection::Both => unreachable!("merged direction is concrete"),
    };
    if !positive(structure_stop_loss_price) {
        return Err("filtered_volume_strategy_atr_stop_invalid");
    }

    Ok(FilteredVolumeRsiEmaMacdSignal {
        direction,
        trigger,
        structure_stop_loss_price,
        structure_stop_loss_source: FILTERED_VOLUME_ATR_STOP_SOURCE,
        evidence: CompletedCandleEntrySignalEvidence {
            filtered_volume_ratio: volume.ratio,
            filtered_volume_retained_candles: volume.retained_candles,
            current_volume_ccy: None,
            weekly_volume_ccy_p90: None,
            rsi14,
            macd_dif,
            ema12,
            ema144,
            ema169,
            ema696,
            atr14,
            take_profit_atr_multiplier: None,
            rsi_pattern_stop_participated: false,
            rsi_divergences: Vec::new(),
            macd_divergences,
            bollinger_conflict: None,
            ema144_distance_atr: None,
            ema144_max_distance_atr: None,
            ema_candidate_blocked_by_distance: false,
            anchor_entry: None,
            trend_managed_exit: None,
            isolated_family: None,
        },
    })
}

/// 使用与入场完全相同的过滤后量比快照选择逐笔目标 R。
pub(super) fn filtered_volume_target_r(filtered_volume_ratio: f64) -> Option<f64> {
    if !filtered_volume_ratio.is_finite() || filtered_volume_ratio < FILTERED_VOLUME_MIN_RATIO {
        return None;
    }
    if filtered_volume_ratio < 4.0 {
        Some(1.8)
    } else if filtered_volume_ratio < 6.0 {
        Some(2.4)
    } else {
        Some(3.0)
    }
}

/// 使用入场时冻结的过滤量比选择新策略目标；其他版本继续沿用既有 Volume-ATR 或固定 R。
pub(crate) fn effective_target_r_for_confirmed_signal(
    signal: &ConfirmedEvent,
    candles: &[BacktestCandle],
    selected_stop_loss_pct: f64,
    fallback_target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<f64> {
    if super::is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version) {
        if signal.structure_stop_loss_source.as_deref()
            == Some(FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE)
        {
            return Some(1.0);
        }
        let evidence = signal.entry_signal_evidence.as_ref()?;
        let target_distance = evidence.atr14 * evidence.take_profit_atr_multiplier?;
        let actual_risk_distance = signal.entry_price * selected_stop_loss_pct;
        return (target_distance.is_finite()
            && target_distance > 0.0
            && actual_risk_distance.is_finite()
            && actual_risk_distance > 0.0)
            .then_some(target_distance / actual_risk_distance);
    }
    if args.entry_filtered_volume_rsi_ema_macd {
        return signal
            .entry_signal_evidence
            .as_ref()
            .and_then(|evidence| filtered_volume_target_r(evidence.filtered_volume_ratio));
    }
    if args.volume_atr_take_profit {
        volume_atr_target_r_with_policy(
            candles,
            signal.event.ts,
            signal.entry_ts,
            signal.entry_price,
            selected_stop_loss_pct,
            args,
        )
    } else {
        Some(fallback_target_r)
    }
}

/// 为最近十根历史 K 线逐根使用其自身前十根原始均量做因果标记，再计算当前量比。
fn filtered_current_volume_evidence(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<FilteredVolumeEvidence, &'static str> {
    let history_start = latest_idx
        .checked_sub(FILTERED_VOLUME_HISTORY_CANDLES)
        .ok_or("filtered_volume_strategy_not_ready")?;
    let mut filtered_sum = 0.0;
    let mut retained_candles = 0usize;
    for candidate_idx in history_start..latest_idx {
        let marking_start = candidate_idx
            .checked_sub(FILTERED_VOLUME_HISTORY_CANDLES)
            .ok_or("filtered_volume_strategy_not_ready")?;
        let marking_average = average_volume(candles, marking_start, candidate_idx)?;
        let candidate_volume = positive_volume(candles, candidate_idx)?;
        if candidate_volume >= marking_average * FILTERED_VOLUME_MIN_RATIO {
            continue;
        }
        filtered_sum += candidate_volume;
        retained_candles += 1;
    }
    if retained_candles < FILTERED_VOLUME_MIN_RETAINED_CANDLES {
        return Err("filtered_volume_strategy_insufficient_retained_history");
    }
    let filtered_average = filtered_sum / retained_candles as f64;
    if !positive(filtered_average) {
        return Err("filtered_volume_strategy_volume_not_ready");
    }
    let current_volume = positive_volume(candles, latest_idx)?;
    Ok(FilteredVolumeEvidence {
        ratio: current_volume / filtered_average,
        retained_candles,
    })
}

fn average_volume(
    candles: &[ComputedCandle],
    start: usize,
    end: usize,
) -> Result<f64, &'static str> {
    let history = candles
        .get(start..end)
        .filter(|items| items.len() == FILTERED_VOLUME_HISTORY_CANDLES)
        .ok_or("filtered_volume_strategy_not_ready")?;
    let sum = history.iter().try_fold(0.0, |sum, item| {
        positive(item.candle.volume)
            .then_some(sum + item.candle.volume)
            .ok_or("filtered_volume_strategy_volume_not_ready")
    })?;
    Ok(sum / FILTERED_VOLUME_HISTORY_CANDLES as f64)
}

fn positive_volume(candles: &[ComputedCandle], idx: usize) -> Result<f64, &'static str> {
    candles
        .get(idx)
        .map(|item| item.candle.volume)
        .filter(|value| positive(*value))
        .ok_or("filtered_volume_strategy_volume_not_ready")
}

/// RSI 分支先检查同一价格枢轴上的极值背离；没有背离时才允许对应长影线反转。
fn rsi_candidate(
    candles: &[ComputedCandle],
    latest_idx: usize,
    rsi14: f64,
) -> Option<BranchCandidate> {
    let latest = candles.get(latest_idx)?;
    if rsi14 < RSI_OVERSOLD {
        if let Some(pivot_idx) = latest_confirmed_price_pivot(candles, latest_idx, PivotKind::Low) {
            let pivot = candles.get(pivot_idx)?;
            if latest.candle.low < pivot.candle.low
                && rsi14 >= pivot.rsi14? + RSI_DIVERGENCE_MIN_DELTA
            {
                return Some(BranchCandidate {
                    direction: MarketVelocityTradeDirection::Long,
                    trigger: "rsi_bullish_divergence_long",
                });
            }
        }
        if has_dominant_reversal_wick(latest, PivotKind::Low) {
            return Some(BranchCandidate {
                direction: MarketVelocityTradeDirection::Long,
                trigger: "rsi_oversold_lower_wick_long",
            });
        }
    } else if rsi14 > RSI_OVERBOUGHT {
        if let Some(pivot_idx) = latest_confirmed_price_pivot(candles, latest_idx, PivotKind::High)
        {
            let pivot = candles.get(pivot_idx)?;
            if latest.candle.high > pivot.candle.high
                && rsi14 + RSI_DIVERGENCE_MIN_DELTA <= pivot.rsi14?
            {
                return Some(BranchCandidate {
                    direction: MarketVelocityTradeDirection::Short,
                    trigger: "rsi_bearish_divergence_short",
                });
            }
        }
        if has_dominant_reversal_wick(latest, PivotKind::High) {
            return Some(BranchCandidate {
                direction: MarketVelocityTradeDirection::Short,
                trigger: "rsi_overbought_upper_wick_short",
            });
        }
    }
    None
}

fn has_dominant_reversal_wick(latest: &ComputedCandle, kind: PivotKind) -> bool {
    let range = latest.candle.high - latest.candle.low;
    if !positive(range) {
        return false;
    }
    let body = (latest.candle.close - latest.candle.open).abs();
    if !body.is_finite() || body / range <= DOJI_MAX_BODY_RANGE_RATIO {
        return false;
    }
    let upper_wick = latest.candle.high - latest.candle.open.max(latest.candle.close);
    let lower_wick = latest.candle.open.min(latest.candle.close) - latest.candle.low;
    match kind {
        PivotKind::Low => {
            lower_wick / range >= REVERSAL_WICK_MIN_RANGE_RATIO && lower_wick > upper_wick
        }
        PivotKind::High => {
            upper_wick / range >= REVERSAL_WICK_MIN_RANGE_RATIO && upper_wick > lower_wick
        }
    }
}

/// EMA 分支只接受严格多空排列、价格站在 EMA12 趋势侧和 1%-3% 的方向大实体。
fn ema_candidate(
    latest: &ComputedCandle,
    rsi14: f64,
    ema12: f64,
    ema144: f64,
    ema696: f64,
) -> Option<BranchCandidate> {
    let range = latest.candle.high - latest.candle.low;
    let body = (latest.candle.close - latest.candle.open).abs();
    if !positive(range) || !positive(latest.candle.open) {
        return None;
    }
    let body_range_ratio = body / range;
    let body_open_ratio = body / latest.candle.open;
    let body_confirmed = body_range_ratio >= EMA_BODY_MIN_RANGE_RATIO
        && body_open_ratio > EMA_BODY_MIN_OPEN_RATIO
        && body_open_ratio < EMA_BODY_MAX_OPEN_RATIO;
    if !body_confirmed {
        return None;
    }
    if ema12 > ema144
        && ema144 > ema696
        && latest.candle.close > ema12
        && latest.candle.close > latest.candle.open
        && rsi14 < RSI_OVERBOUGHT
    {
        return Some(BranchCandidate {
            direction: MarketVelocityTradeDirection::Long,
            trigger: "ema_bullish_continuation_long",
        });
    }
    if ema12 < ema144
        && ema144 < ema696
        && latest.candle.close < ema12
        && latest.candle.close < latest.candle.open
        && rsi14 > RSI_OVERSOLD
    {
        return Some(BranchCandidate {
            direction: MarketVelocityTradeDirection::Short,
            trigger: "ema_bearish_continuation_short",
        });
    }
    None
}

/// 保留 v1 的当前 K 线对历史枢轴语义，只用于复现已有回测结果。
fn legacy_macd_candidates(
    candles: &[ComputedCandle],
    latest_idx: usize,
    rsi14: f64,
    macd_dif: f64,
) -> Vec<BranchCandidate> {
    if (MACD_RSI_NEUTRAL_MIN..=MACD_RSI_NEUTRAL_MAX).contains(&rsi14) {
        return Vec::new();
    }
    let Some(latest) = candles.get(latest_idx) else {
        return Vec::new();
    };
    if !positive(latest.candle.close) {
        return Vec::new();
    }
    let current_normalized_dif = macd_dif / latest.candle.close;
    let mut candidates = Vec::with_capacity(2);
    if let Some(pivot_idx) = latest_confirmed_price_pivot(candles, latest_idx, PivotKind::High) {
        if let Some(pivot) = candles.get(pivot_idx) {
            if let Some(pivot_normalized_dif) = normalized_macd_dif(pivot) {
                if latest.candle.high > pivot.candle.high
                    && current_normalized_dif < pivot_normalized_dif
                {
                    candidates.push(BranchCandidate {
                        direction: MarketVelocityTradeDirection::Short,
                        trigger: "macd_bearish_divergence_short",
                    });
                }
            }
        }
    }
    if let Some(pivot_idx) = latest_confirmed_price_pivot(candles, latest_idx, PivotKind::Low) {
        if let Some(pivot) = candles.get(pivot_idx) {
            if let Some(pivot_normalized_dif) = normalized_macd_dif(pivot) {
                if latest.candle.low < pivot.candle.low
                    && current_normalized_dif > pivot_normalized_dif
                {
                    candidates.push(BranchCandidate {
                        direction: MarketVelocityTradeDirection::Long,
                        trigger: "macd_bullish_divergence_long",
                    });
                }
            }
        }
    }
    candidates
}

fn normalized_macd_dif(candle: &ComputedCandle) -> Option<f64> {
    let close = positive(candle.candle.close).then_some(candle.candle.close)?;
    let dif = candle.macd_line.filter(|value| value.is_finite())?;
    Some(dif / close)
}

/// 倒序选择最近一个左右各三根均已完成的价格枢轴，当前 K 线不参与右侧确认。
fn latest_confirmed_price_pivot(
    candles: &[ComputedCandle],
    latest_idx: usize,
    kind: PivotKind,
) -> Option<usize> {
    let wing = DIVERGENCE_PIVOT_WING_CANDLES;
    let start = latest_idx.saturating_sub(DIVERGENCE_LOOKBACK_CANDLES);
    let first_center = start.checked_add(wing)?;
    let last_center = latest_idx.checked_sub(wing + 1)?;
    if first_center > last_center {
        return None;
    }
    (first_center..=last_center)
        .rev()
        .find(|center| is_price_pivot(candles, *center, wing, kind))
}

fn is_price_pivot(candles: &[ComputedCandle], center: usize, wing: usize, kind: PivotKind) -> bool {
    let Some(candidate) = candles.get(center) else {
        return false;
    };
    let Some(range) = candles.get(center - wing..=center + wing) else {
        return false;
    };
    let price = match kind {
        PivotKind::Low => candidate.candle.low,
        PivotKind::High => candidate.candle.high,
    };
    range.iter().enumerate().all(|(offset, item)| {
        offset == wing
            || match kind {
                PivotKind::Low => item.candle.low >= price,
                PivotKind::High => item.candle.high <= price,
            }
    }) && range.iter().enumerate().any(|(offset, item)| {
        offset != wing
            && match kind {
                PivotKind::Low => item.candle.low > price,
                PivotKind::High => item.candle.high < price,
            }
    })
}

fn positive_indicator(value: Option<f64>) -> Option<f64> {
    value.filter(|value| positive(*value))
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        build_computed_candles, effective_target_r_for_confirmed_signal,
        market_filtered_volume_rsi_ema_macd_v1_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_strategy_detail,
        market_velocity_strategy_type, select_stop_loss_for_confirmed_signal, BacktestCandle,
        ConfirmedEvent, RadarEvent, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY,
    };

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            volume_ccy: None,
            candle: BacktestCandle {
                ts: idx as i64 * 900_000,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 10.0,
            },
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
        (0..60).map(candle).collect()
    }

    fn confirm_volume(candles: &mut [ComputedCandle]) {
        candles.last_mut().expect("signal candle").candle.volume = 30.0;
    }

    fn v1_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let args = market_filtered_volume_rsi_ema_macd_v1_research_args()
            .expect("v1 research args remain valid");
        filtered_volume_rsi_ema_macd_signal(candles, completed_count, &args)
    }

    #[test]
    fn current_volume_equal_to_three_times_filtered_average_is_accepted() {
        let mut candles = candles();
        confirm_volume(&mut candles);
        let latest_idx = candles.len() - 1;
        let evidence = filtered_current_volume_evidence(&candles, latest_idx).unwrap();

        assert_eq!(evidence.retained_candles, 10);
        assert!((evidence.ratio - 3.0).abs() < 1e-12);
    }

    #[test]
    fn marked_historical_spike_is_excluded_from_current_average() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        candles[latest_idx - 4].candle.volume = 40.0;
        confirm_volume(&mut candles);

        let evidence = filtered_current_volume_evidence(&candles, latest_idx).unwrap();

        assert_eq!(evidence.retained_candles, 9);
        assert!((evidence.ratio - 3.0).abs() < 1e-12);
    }

    #[test]
    fn fewer_than_five_retained_history_candles_blocks_entry() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        for (offset, volume) in [1_000.0, 4_000.0, 16_000.0, 64_000.0, 256_000.0, 1_024_000.0]
            .into_iter()
            .enumerate()
        {
            candles[latest_idx - 6 + offset].candle.volume = volume;
        }
        candles[latest_idx].candle.volume = 10_000_000.0;

        assert_eq!(
            filtered_current_volume_evidence(&candles, latest_idx),
            Err("filtered_volume_strategy_insufficient_retained_history")
        );
    }

    #[test]
    fn rsi_bottom_divergence_enters_long_with_pure_atr_stop() {
        let mut candles = candles();
        let pivot_idx = 40;
        candles[pivot_idx].candle.low = 95.0;
        candles[pivot_idx].rsi14 = Some(27.0);
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.low = 94.0;
        latest.rsi14 = Some(29.0);
        confirm_volume(&mut candles);

        let signal = v1_signal(&candles, candles.len()).unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(signal.trigger, "rsi_bullish_divergence_long");
        assert_eq!(
            signal.structure_stop_loss_source,
            FILTERED_VOLUME_ATR_STOP_SOURCE
        );
        assert!((signal.structure_stop_loss_price - 97.5).abs() < 1e-12);
        assert!((signal.evidence.filtered_volume_ratio - 3.0).abs() < 1e-12);
    }

    #[test]
    fn future_candle_cannot_change_causal_divergence_signal() {
        let mut candles = candles();
        candles[40].candle.low = 95.0;
        candles[40].rsi14 = Some(27.0);
        let latest_idx = candles.len() - 1;
        candles[latest_idx].candle.low = 94.0;
        candles[latest_idx].rsi14 = Some(29.0);
        confirm_volume(&mut candles);
        let before = v1_signal(&candles, candles.len()).unwrap();
        let completed_count = candles.len();
        let mut future = candle(completed_count);
        future.candle.low = 1.0;
        future.rsi14 = Some(99.0);
        candles.push(future);

        let after = v1_signal(&candles, completed_count).unwrap();

        assert_eq!(before, after);
    }

    #[test]
    fn overbought_dominant_upper_wick_enters_short_but_doji_does_not() {
        let mut candles = candles();
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.open = 100.0;
        latest.candle.close = 100.5;
        latest.candle.high = 104.0;
        latest.candle.low = 99.8;
        latest.rsi14 = Some(71.0);
        confirm_volume(&mut candles);
        let signal = v1_signal(&candles, candles.len()).unwrap();
        assert_eq!(signal.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(signal.trigger, "rsi_overbought_upper_wick_short");

        let latest = candles.last_mut().expect("signal candle");
        latest.candle.close = 100.1;
        assert_eq!(
            v1_signal(&candles, candles.len()),
            Err("filtered_volume_strategy_no_branch_signal")
        );
    }

    #[test]
    fn ema_bullish_continuation_requires_strict_rsi_boundary() {
        let mut candles = candles();
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.open = 100.0;
        latest.candle.close = 102.0;
        latest.candle.high = 102.2;
        latest.candle.low = 99.8;
        latest.ema12 = Some(100.0);
        latest.ema144 = Some(99.0);
        latest.ema696 = Some(98.0);
        latest.rsi14 = Some(50.0);
        confirm_volume(&mut candles);
        let signal = v1_signal(&candles, candles.len()).unwrap();
        assert_eq!(signal.trigger, "ema_bullish_continuation_long");

        candles.last_mut().expect("signal candle").rsi14 = Some(70.0);
        assert_eq!(
            v1_signal(&candles, candles.len()),
            Err("filtered_volume_strategy_no_branch_signal")
        );
    }

    #[test]
    fn macd_bottom_divergence_uses_normalized_dif_and_neutral_rsi_blocks_it() {
        let mut candles = candles();
        let pivot_idx = 40;
        candles[pivot_idx].candle.low = 95.0;
        candles[pivot_idx].macd_line = Some(-1.0);
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.low = 94.0;
        latest.macd_line = Some(-0.5);
        latest.rsi14 = Some(35.0);
        confirm_volume(&mut candles);
        let signal = v1_signal(&candles, candles.len()).unwrap();
        assert_eq!(signal.trigger, "macd_bullish_divergence_long");

        candles.last_mut().expect("signal candle").rsi14 = Some(40.0);
        assert_eq!(
            v1_signal(&candles, candles.len()),
            Err("filtered_volume_strategy_no_branch_signal")
        );
    }

    #[test]
    fn opposing_branch_candidates_block_the_trade() {
        let mut candles = candles();
        candles[40].candle.low = 95.0;
        candles[40].rsi14 = Some(27.0);
        candles[40].macd_line = Some(1.0);
        candles[45].candle.high = 105.0;
        candles[45].macd_line = Some(1.0);
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.low = 94.0;
        latest.candle.high = 106.0;
        latest.rsi14 = Some(29.0);
        latest.macd_line = Some(0.5);
        confirm_volume(&mut candles);

        assert_eq!(
            v1_signal(&candles, candles.len()),
            Err("filtered_volume_strategy_direction_conflict")
        );
    }

    #[test]
    fn filtered_volume_target_tiers_use_exact_boundaries() {
        assert_eq!(filtered_volume_target_r(2.9999), None);
        assert_eq!(filtered_volume_target_r(3.0), Some(1.8));
        assert_eq!(filtered_volume_target_r(3.9999), Some(1.8));
        assert_eq!(filtered_volume_target_r(4.0), Some(2.4));
        assert_eq!(filtered_volume_target_r(5.9999), Some(2.4));
        assert_eq!(filtered_volume_target_r(6.0), Some(3.0));
    }

    #[test]
    fn computed_candles_precompute_all_three_ema_periods() {
        let raw = (0..700)
            .map(|idx| BacktestCandle {
                ts: idx * 900_000,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            })
            .collect();

        let computed = build_computed_candles(raw, 20);

        assert_eq!(computed[10].ema12, None);
        assert_eq!(computed[11].ema12, Some(100.0));
        assert_eq!(computed[142].ema144, None);
        assert_eq!(computed[143].ema144, Some(100.0));
        assert_eq!(computed[694].ema696, None);
        assert_eq!(computed[695].ema696, Some(100.0));
    }

    #[test]
    fn preset_and_manifest_keep_the_strategy_research_only_and_independent() {
        let args = market_filtered_volume_rsi_ema_macd_v1_research_args().unwrap();
        assert!(args.entry_filtered_volume_rsi_ema_macd);
        assert!(!args.entry_rsi_volume_regime);
        assert!(!args.entry_bollinger_breakout);
        assert_eq!(args.entry_min_volume_ratio, FILTERED_VOLUME_MIN_RATIO);
        assert_eq!(args.equity_max_holding_hours, Some(48));
        assert_eq!(args.backtest_fee_bps_per_side, Some(5.0));
        assert_eq!(args.backtest_slippage_bps_per_side, 3.0);
        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY
        );
        assert_eq!(
            market_velocity_strategy_detail(&args)["version_status"],
            "research_unvalidated"
        );
        assert_eq!(
            market_velocity_strategy_detail(&args)["paper_live_eligible"],
            false
        );

        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET,
        )
        .unwrap();
        assert_eq!(
            manifest.strategy_key,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY
        );
        assert_eq!(manifest.channel, "research");
        assert_eq!(manifest.manifest_status, "research");
        assert_eq!(
            manifest.manifest_json["execution"]["live_handoff_eligible"],
            false
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["fast_momentum_filters"]
                ["filtered_volume_rsi_ema_macd"]["excluded_legacy_entry_logic"],
            serde_json::json!(["96_bar_move", "bollinger", "bos", "fvg", "choch"])
        );
    }

    #[test]
    fn frozen_evidence_drives_target_and_pure_atr_stop_without_three_percent_cap() {
        let mut candles = candles();
        let latest = candles.last_mut().expect("signal candle");
        latest.candle.open = 100.0;
        latest.candle.close = 102.0;
        latest.candle.high = 102.2;
        latest.candle.low = 99.8;
        latest.candle.volume = 45.0;
        latest.ema12 = Some(100.0);
        latest.ema144 = Some(99.0);
        latest.ema696 = Some(98.0);
        latest.rsi14 = Some(50.0);
        latest.atr14 = Some(10.0);
        let entry_price = latest.candle.close;
        let signal = v1_signal(&candles, candles.len()).unwrap();
        let args = market_filtered_volume_rsi_ema_macd_v1_research_args().unwrap();
        let confirmed = ConfirmedEvent {
            event: RadarEvent {
                id: 1,
                exchange: "okx".to_string(),
                symbol: "TEST-USDT-SWAP".to_string(),
                ts: candles.len() as i64 * 900_000,
                detected_at: "2026-07-01 00:00:00+00".to_string(),
                new_rank: 0,
                delta_rank: 0,
                current_price: entry_price,
                price_change_pct: 2.0,
            },
            direction: signal.direction,
            entry_ts: candles.len() as i64 * 900_000,
            entry_price,
            entry_idx: candles.len(),
            trigger: signal.trigger,
            structure_stop_loss_price: Some(signal.structure_stop_loss_price),
            structure_stop_loss_source: Some(signal.structure_stop_loss_source.to_string()),
            entry_signal_evidence: Some(signal.evidence),
        };

        let stop = select_stop_loss_for_confirmed_signal(&confirmed, &args);
        let target = effective_target_r_for_confirmed_signal(
            &confirmed,
            &[],
            stop.stop_loss_pct,
            1.0,
            &args,
        );

        assert!((stop.price - 87.0).abs() < 1e-12);
        assert!(stop.stop_loss_pct > 0.14);
        assert_eq!(target, Some(2.4));
    }
}
