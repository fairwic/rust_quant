use super::{
    computed_candles::FAST_MOMENTUM_ATR_PERIOD, ComputedCandle, MarketVelocityTradeDirection,
    MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION,
};

/// 普通 RSI 反转分支使用的历史窗口，不包含当前信号 K 线。
pub(super) const RSI_VOLUME_TREND_LOOKBACK_CANDLES: usize = 96;
/// 普通 RSI 反转分支允许的最小累计反向涨跌幅，单位为百分比。
pub(super) const RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT: f64 = 8.0;
/// 线性趋势分支的最低拟合优度；净幅分支与趋势分支满足任一即可。
pub(super) const RSI_VOLUME_MIN_TREND_R_SQUARED: f64 = 0.70;
/// RSI14 严格低于该值时评估普通做多反转。
pub(super) const RSI_VOLUME_OVERSOLD: f64 = 30.0;
/// RSI14 严格高于该值时评估普通做空反转。
pub(super) const RSI_VOLUME_OVERBOUGHT: f64 = 70.0;
/// 横盘特例要求前两根已完成 K 线的 RSI14 落在该闭区间内。
pub(super) const RSI_VOLUME_SIDEWAYS_RSI_MIN: f64 = 40.0;
/// 横盘特例要求前两根已完成 K 线的 RSI14 落在该闭区间内。
pub(super) const RSI_VOLUME_SIDEWAYS_RSI_MAX: f64 = 60.0;
/// 当前信号量能只与此前五根已完成 15m K 线均量比较。
pub(super) const RSI_VOLUME_LOOKBACK_CANDLES: usize = 5;
/// 当前成交量至少达到前五根均量的两倍才允许产生信号。
pub(super) const RSI_VOLUME_MIN_RATIO: f64 = 2.0;
/// v3/v4 当前信号量能只与此前四根已完成 15m K 线均量比较。
pub(super) const RSI_VOLUME_V3_LOOKBACK_CANDLES: usize = 4;
/// v3/v4 全部分支统一要求当前成交量至少达到此前四根均量的 1.5 倍。
pub(super) const RSI_VOLUME_V3_MIN_RATIO: f64 = 1.5;
/// v5 从最近十根历史 K 线构造当前量比基线。
pub(super) const RSI_VOLUME_V5_LOOKBACK_CANDLES: usize = 10;
/// v5 标记历史放量和确认当前放量都使用两倍阈值。
pub(super) const RSI_VOLUME_V5_MIN_RATIO: f64 = 2.0;
/// v2 用信号前 96 根带宽的低分位判断相对窄带，避免给不同价格尺度的币使用固定带宽。
pub(super) const RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES: usize = 96;
/// 前一根布林带宽必须位于上述窗口最低 20% 才视为压缩。
pub(super) const RSI_VOLUME_NARROW_BAND_PERCENTILE: usize = 20;
/// MACD 线与信号线相对收盘价都不超过 0.15% 才视为接近零轴。
pub(super) const RSI_VOLUME_MACD_NEAR_ZERO_MAX_PCT: f64 = 0.15;
/// v2 常规背离只在最近 48 根内寻找已经因果确认的历史价格拐点。
pub(super) const RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES: usize = 48;
/// 历史拐点左右各需三根已完成 K 线确认，当前 K 线不参与右侧确认。
pub(super) const RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES: usize = 3;
/// RSI 至少相差 3 点才接受背离，避免把浮点噪声当作动量衰减。
pub(super) const RSI_VOLUME_DIVERGENCE_MIN_RSI_DELTA: f64 = 3.0;
/// v3/v4/v5 在 RSI 已处于 30/70 极值时只要求同点背离至少改善一分。
pub(super) const RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA: f64 = 1.0;
/// v3/v4/v5 做多限制上影线、做空限制下影线，均不得超过实体的 45%。
pub(super) const RSI_VOLUME_V3_OPPOSING_WICK_MAX_BODY_MULTIPLE: f64 = 0.45;
/// v3/v4/v5 初始保护位固定为入场价反方向 1.5 倍 ATR14。
pub(super) const RSI_VOLUME_V3_STOP_ATR_MULTIPLIER: f64 = 1.5;
/// v3/v4/v5 ATR 周期与回测预计算指标保持一致。
pub(super) const RSI_VOLUME_V3_STOP_ATR_PERIOD: usize = FAST_MOMENTUM_ATR_PERIOD;
/// v3/v4/v5 候选止损来源；选择器据此禁止回退到固定百分比止损。
pub(super) const RSI_VOLUME_V3_ATR_STOP_SOURCE: &str = "rsi_volume_regime_atr14_1_5";

/// RSI 量价策略的不可变入场语义版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RsiVolumeRegimeVersion {
    /// 前两根 RSI 中性区间判断横盘的原始研究版本。
    V1,
    /// 使用布林压缩、MACD 零轴和同点 RSI 背离的研究版本。
    V2,
    /// 使用四根量比、极值背离、压缩突破、96 根净幅和 ATR 止损的研究版本。
    V3,
    /// 移除压缩突破，只保留极值背离、96 根净幅和 ATR 止损的研究版本。
    V4,
    /// 使用剔除历史放量后的十根均量，其余入场与风险语义继承 v4。
    V5,
}

/// 从审计规则版本解析策略语义；未知旧调用保持 v1，避免静默改变既有回放。
pub(super) fn rsi_volume_regime_version(entry_rule_version: &str) -> RsiVolumeRegimeVersion {
    match entry_rule_version {
        MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION => RsiVolumeRegimeVersion::V3,
        MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION => RsiVolumeRegimeVersion::V4,
        MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION => RsiVolumeRegimeVersion::V5,
        MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION => RsiVolumeRegimeVersion::V2,
        _ => RsiVolumeRegimeVersion::V1,
    }
}

/// 已在信号时点确认的 RSI 放量入场及其版本化保护止损。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RsiVolumeRegimeSignal {
    /// 最终交易方向；各入场分支在此冻结方向。
    pub(super) direction: MarketVelocityTradeDirection,
    /// 可审计的入场触发标签。
    pub(super) trigger: String,
    /// 位于亏损侧的候选止损价格；v3/v4/v5 最终会覆盖为 ATR14 止损。
    pub(super) structure_stop_loss_price: f64,
    /// 结构止损的来源标签。
    pub(super) structure_stop_loss_source: &'static str,
}

/// 保留 v1 的测试入口，确保旧的 RSI 放量反转与横盘突破语义可以独立回放。
#[cfg(test)]
fn rsi_volume_regime_signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    min_volume_ratio: f64,
    max_stop_loss_pct: f64,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    rsi_volume_regime_signal_for_version(
        candles,
        completed_count,
        min_volume_ratio,
        max_stop_loss_pct,
        RsiVolumeRegimeVersion::V1,
    )
}

/// 按冻结版本评估 RSI 量价信号，运行时必须显式传入审计版本。
pub(super) fn rsi_volume_regime_signal_for_version(
    candles: &[ComputedCandle],
    completed_count: usize,
    min_volume_ratio: f64,
    max_stop_loss_pct: f64,
    version: RsiVolumeRegimeVersion,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("rsi_volume_regime_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    let volume_ratio = if version == RsiVolumeRegimeVersion::V5 {
        v5_filtered_current_volume_ratio(candles, latest_idx)?
    } else {
        let volume_lookback = match version {
            RsiVolumeRegimeVersion::V3 | RsiVolumeRegimeVersion::V4 => {
                RSI_VOLUME_V3_LOOKBACK_CANDLES
            }
            RsiVolumeRegimeVersion::V1 | RsiVolumeRegimeVersion::V2 => RSI_VOLUME_LOOKBACK_CANDLES,
            RsiVolumeRegimeVersion::V5 => unreachable!("v5 uses its filtered volume baseline"),
        };
        let volume_history_start = latest_idx
            .checked_sub(volume_lookback)
            .ok_or("rsi_volume_regime_not_ready")?;
        let volume_history = candles
            .get(volume_history_start..latest_idx)
            .filter(|items| items.len() == volume_lookback)
            .ok_or("rsi_volume_regime_not_ready")?;
        let volume_sum = volume_history
            .iter()
            .map(|item| item.candle.volume)
            .sum::<f64>();
        let volume_average = volume_sum / volume_lookback as f64;
        if !volume_average.is_finite() || volume_average <= 0.0 {
            return Err("rsi_volume_regime_not_ready");
        }
        latest.candle.volume / volume_average
    };
    if !volume_ratio.is_finite() || volume_ratio < min_volume_ratio {
        return Err("rsi_volume_regime_volume_not_confirmed");
    }

    let candidate = match version {
        RsiVolumeRegimeVersion::V1 => v1_candidate(candles, latest_idx)?,
        RsiVolumeRegimeVersion::V2 => v2_candidate(candles, latest_idx)?,
        RsiVolumeRegimeVersion::V3 => v3_candidate(candles, latest_idx)?,
        RsiVolumeRegimeVersion::V4 => v4_candidate(candles, latest_idx)?,
        RsiVolumeRegimeVersion::V5 => v4_candidate(candles, latest_idx)?,
    };
    let opposing_wick_max_body_multiple = match version {
        RsiVolumeRegimeVersion::V3 | RsiVolumeRegimeVersion::V4 | RsiVolumeRegimeVersion::V5 => {
            RSI_VOLUME_V3_OPPOSING_WICK_MAX_BODY_MULTIPLE
        }
        RsiVolumeRegimeVersion::V1 | RsiVolumeRegimeVersion::V2 => 1.0,
    };
    validate_opposing_wick(latest, candidate.direction, opposing_wick_max_body_multiple)?;
    match version {
        RsiVolumeRegimeVersion::V3 | RsiVolumeRegimeVersion::V4 | RsiVolumeRegimeVersion::V5 => {
            apply_atr_stop(latest, candidate)
        }
        RsiVolumeRegimeVersion::V1 | RsiVolumeRegimeVersion::V2 => {
            validate_structure_stop(latest.candle.close, candidate, max_stop_loss_pct)
        }
    }
}

/// 构造 v5 的因果成交量基线。
///
/// 最近十根历史 K 线中的每一根，都只用它自己之前的十根原始成交量判断是否达到两倍；
/// 标记阶段不递归剔除更早的放量，避免结果依赖遍历顺序。当前 K 线不进入均量分母，
/// 即使它达到放量阈值也始终保留为最终比值的分子。
fn v5_filtered_current_volume_ratio(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<f64, &'static str> {
    let history_start = latest_idx
        .checked_sub(RSI_VOLUME_V5_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_not_ready")?;
    let history = candles
        .get(history_start..latest_idx)
        .filter(|items| items.len() == RSI_VOLUME_V5_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_not_ready")?;
    let mut filtered_sum = 0.0;
    let mut filtered_count = 0usize;

    for (offset, candidate) in history.iter().enumerate() {
        let candidate_idx = history_start + offset;
        let marking_start = candidate_idx
            .checked_sub(RSI_VOLUME_V5_LOOKBACK_CANDLES)
            .ok_or("rsi_volume_regime_not_ready")?;
        let marking_history = candles
            .get(marking_start..candidate_idx)
            .filter(|items| items.len() == RSI_VOLUME_V5_LOOKBACK_CANDLES)
            .ok_or("rsi_volume_regime_not_ready")?;
        let marking_sum = marking_history
            .iter()
            .map(|item| item.candle.volume)
            .sum::<f64>();
        let marking_average = marking_sum / RSI_VOLUME_V5_LOOKBACK_CANDLES as f64;
        if !marking_average.is_finite() || marking_average <= 0.0 {
            return Err("rsi_volume_regime_not_ready");
        }
        if candidate.candle.volume >= marking_average * RSI_VOLUME_V5_MIN_RATIO {
            continue;
        }
        filtered_sum += candidate.candle.volume;
        filtered_count += 1;
    }

    if filtered_count == 0 {
        return Err("rsi_volume_regime_filtered_volume_history_empty");
    }
    let filtered_average = filtered_sum / filtered_count as f64;
    if !filtered_average.is_finite() || filtered_average <= 0.0 {
        return Err("rsi_volume_regime_not_ready");
    }
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    Ok(latest.candle.volume / filtered_average)
}

/// 保留 v1 的中性 RSI 横盘判断，保证既有回测可以按原规则重放。
fn v1_candidate(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    let previous_two = latest_idx
        .checked_sub(2)
        .and_then(|start| candles.get(start..latest_idx))
        .filter(|history| history.len() == 2)
        .ok_or("rsi_volume_regime_not_ready")?;
    let sideways_rsi = previous_two.iter().all(|item| {
        item.rsi14.is_some_and(|rsi| {
            (RSI_VOLUME_SIDEWAYS_RSI_MIN..=RSI_VOLUME_SIDEWAYS_RSI_MAX).contains(&rsi)
        })
    });
    if sideways_rsi {
        sideways_break_signal(latest, previous_two)
    } else {
        normal_rsi_reversal_signal(candles, latest_idx)
    }
}

/// v2 先处理同点 RSI 背离，再判断压缩突破，最后才回退到原 96 根反转背景。
fn v2_candidate(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    if let Some(signal) = rsi_divergence_signal(
        candles,
        latest_idx,
        RSI_VOLUME_DIVERGENCE_MIN_RSI_DELTA,
        false,
    )? {
        return Ok(signal);
    }
    let rsi = candles
        .get(latest_idx)
        .and_then(|item| item.rsi14)
        .ok_or("rsi_volume_regime_not_ready")?;
    let rsi_is_extreme = rsi < RSI_VOLUME_OVERSOLD || rsi > RSI_VOLUME_OVERBOUGHT;
    if rsi_is_extreme && pre_breakout_sideways_context(candles, latest_idx)? {
        return bollinger_break_signal(candles, latest_idx);
    }
    normal_rsi_reversal_signal(candles, latest_idx)
}

/// v3 独立收集背离、压缩突破与 96 根净幅信号；同根 K 线方向冲突时拒绝交易。
fn v3_candidate(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let mut candidates = Vec::with_capacity(3);
    if let Some(signal) = rsi_divergence_signal(
        candles,
        latest_idx,
        RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA,
        true,
    )? {
        candidates.push(signal);
    }
    if matches!(pre_breakout_sideways_context(candles, latest_idx), Ok(true)) {
        if let Some(signal) = v3_bollinger_break_signal(candles, latest_idx)? {
            candidates.push(signal);
        }
    }
    if let Ok(Some(signal)) = v3_net_move_signal(candles, latest_idx) {
        candidates.push(signal);
    }
    let direction = candidates
        .first()
        .map(|signal| signal.direction)
        .ok_or("rsi_volume_regime_no_entry_branch_confirmed")?;
    if candidates
        .iter()
        .any(|signal| signal.direction != direction)
    {
        return Err("rsi_volume_regime_branch_direction_conflict");
    }
    let trigger = candidates
        .iter()
        .map(|signal| signal.trigger.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let mut selected = candidates.remove(0);
    selected.trigger = trigger;
    Ok(selected)
}

/// v4/v5 不再读取布林带或 MACD 横盘上下文，只收集极值背离与 96 根净幅分支。
fn v4_candidate(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let mut candidates = Vec::with_capacity(2);
    if let Some(signal) = rsi_divergence_signal(
        candles,
        latest_idx,
        RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA,
        true,
    )? {
        candidates.push(signal);
    }
    if let Ok(Some(signal)) = v3_net_move_signal(candles, latest_idx) {
        candidates.push(signal);
    }
    let direction = candidates
        .first()
        .map(|signal| signal.direction)
        .ok_or("rsi_volume_regime_no_entry_branch_confirmed")?;
    if candidates
        .iter()
        .any(|signal| signal.direction != direction)
    {
        return Err("rsi_volume_regime_branch_direction_conflict");
    }
    let trigger = candidates
        .iter()
        .map(|signal| signal.trigger.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let mut selected = candidates.remove(0);
    selected.trigger = trigger;
    Ok(selected)
}

/// 横盘后必须同时出现成交量继续增加和收盘突破，避免把区间内放量噪声当成趋势启动。
fn sideways_break_signal(
    latest: &ComputedCandle,
    previous_two: &[ComputedCandle],
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let previous = previous_two.last().ok_or("rsi_volume_regime_not_ready")?;
    if latest.candle.volume <= previous.candle.volume {
        return Err("rsi_volume_regime_sideways_volume_not_rising");
    }
    let range_high = previous_two
        .iter()
        .map(|item| item.candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_low = previous_two
        .iter()
        .map(|item| item.candle.low)
        .fold(f64::INFINITY, f64::min);
    if latest.candle.close > latest.candle.open && latest.candle.close > range_high {
        return Ok(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Long,
            trigger: "rsi_sideways_volume_price_breakout_long".to_string(),
            structure_stop_loss_price: range_low,
            structure_stop_loss_source: "sideways_two_candle_range_low",
        });
    }
    if latest.candle.close < latest.candle.open && latest.candle.close < range_low {
        return Ok(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Short,
            trigger: "rsi_sideways_volume_price_breakdown_short".to_string(),
            structure_stop_loss_price: range_high,
            structure_stop_loss_source: "sideways_two_candle_range_high",
        });
    }
    Err("rsi_volume_regime_sideways_break_not_confirmed")
}

/// 判断信号前是否同时处于布林带相对低分位和 MACD 双线零轴附近。
///
/// 使用前一根而不是当前突破 K 线，避免突破本身扩张带宽或拉开 MACD 后反向污染横盘状态。
fn pre_breakout_sideways_context(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<bool, &'static str> {
    let start = latest_idx
        .checked_sub(RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let history = candles
        .get(start..latest_idx)
        .filter(|items| items.len() == RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let previous = history
        .last()
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let mut bandwidths = history
        .iter()
        .map(|item| item.bollinger_bandwidth_pct)
        .collect::<Option<Vec<_>>>()
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    if bandwidths.iter().any(|value| !valid_positive(*value)) {
        return Err("rsi_volume_regime_sideways_context_not_ready");
    }
    bandwidths.sort_by(f64::total_cmp);
    let percentile_idx = (bandwidths.len() - 1) * RSI_VOLUME_NARROW_BAND_PERCENTILE / 100;
    let narrow_band_cutoff = bandwidths[percentile_idx];
    let previous_bandwidth = previous
        .bollinger_bandwidth_pct
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let close = previous.candle.close;
    let macd_line = previous
        .macd_line
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let macd_signal_line = previous
        .macd_signal_line
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    if !valid_positive(close) || !macd_line.is_finite() || !macd_signal_line.is_finite() {
        return Err("rsi_volume_regime_sideways_context_not_ready");
    }
    let macd_max_abs_pct = macd_line.abs().max(macd_signal_line.abs()) / close * 100.0;
    Ok(previous_bandwidth <= narrow_band_cutoff
        && macd_max_abs_pct <= RSI_VOLUME_MACD_NEAR_ZERO_MAX_PCT)
}

/// 在压缩背景中只接受 RSI 极值方向上的真实收盘突破，避免把触带当作突破。
fn bollinger_break_signal(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    let previous = latest_idx
        .checked_sub(1)
        .and_then(|idx| candles.get(idx))
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let rsi = latest.rsi14.ok_or("rsi_volume_regime_not_ready")?;
    let upper = previous
        .bollinger_upper
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let lower = previous
        .bollinger_lower
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    if rsi > RSI_VOLUME_OVERBOUGHT
        && latest.candle.close > latest.candle.open
        && latest.candle.close > upper
    {
        return Ok(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Long,
            trigger: "rsi_overbought_narrow_band_breakout_long".to_string(),
            structure_stop_loss_price: lower,
            structure_stop_loss_source: "prebreakout_bollinger_lower",
        });
    }
    if rsi < RSI_VOLUME_OVERSOLD
        && latest.candle.close < latest.candle.open
        && latest.candle.close < lower
    {
        return Ok(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Short,
            trigger: "rsi_oversold_narrow_band_breakdown_short".to_string(),
            structure_stop_loss_price: upper,
            structure_stop_loss_source: "prebreakout_bollinger_upper",
        });
    }
    Err("rsi_volume_regime_sideways_break_not_confirmed")
}

/// v3 的压缩突破只看价格、方向 K 线与已确认横盘背景，RSI 不参与延续方向判断。
fn v3_bollinger_break_signal(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<Option<RsiVolumeRegimeSignal>, &'static str> {
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    let previous = latest_idx
        .checked_sub(1)
        .and_then(|idx| candles.get(idx))
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let upper = previous
        .bollinger_upper
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    let lower = previous
        .bollinger_lower
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_sideways_context_not_ready")?;
    if latest.candle.close > latest.candle.open && latest.candle.close > upper {
        return Ok(Some(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Long,
            trigger: "narrow_band_zero_macd_breakout_long".to_string(),
            structure_stop_loss_price: latest.candle.low,
            structure_stop_loss_source: "signal_candle_low_before_atr_stop",
        }));
    }
    if latest.candle.close < latest.candle.open && latest.candle.close < lower {
        return Ok(Some(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Short,
            trigger: "narrow_band_zero_macd_breakdown_short".to_string(),
            structure_stop_loss_price: latest.candle.high,
            structure_stop_loss_source: "signal_candle_high_before_atr_stop",
        }));
    }
    Ok(None)
}

/// 用当前价格与同一历史价格枢轴上的 RSI 比较背离，不允许价格和 RSI 各取不同时间点。
///
/// v2 将背离作为不依赖 30/70 的独立入场依据；v3/v4/v5 则显式要求当前 RSI 处于对应极值区。
/// 各版本的成交量门槛都在进入候选分支前统一校验。
fn rsi_divergence_signal(
    candles: &[ComputedCandle],
    latest_idx: usize,
    min_rsi_delta: f64,
    require_current_rsi_extreme: bool,
) -> Result<Option<RsiVolumeRegimeSignal>, &'static str> {
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    let rsi = latest.rsi14.ok_or("rsi_volume_regime_not_ready")?;
    if !require_current_rsi_extreme || rsi < RSI_VOLUME_OVERSOLD {
        if let Some(pivot) = latest_confirmed_divergence_pivot(candles, latest_idx, true) {
            if latest.candle.low < pivot.price && rsi >= pivot.rsi + min_rsi_delta {
                return Ok(Some(RsiVolumeRegimeSignal {
                    direction: MarketVelocityTradeDirection::Long,
                    trigger: "rsi_bullish_divergence_volume_long".to_string(),
                    structure_stop_loss_price: latest.candle.low,
                    structure_stop_loss_source: "rsi_divergence_signal_candle_low",
                }));
            }
        }
    }
    if !require_current_rsi_extreme || rsi > RSI_VOLUME_OVERBOUGHT {
        if let Some(pivot) = latest_confirmed_divergence_pivot(candles, latest_idx, false) {
            if latest.candle.high > pivot.price && rsi + min_rsi_delta <= pivot.rsi {
                return Ok(Some(RsiVolumeRegimeSignal {
                    direction: MarketVelocityTradeDirection::Short,
                    trigger: "rsi_bearish_divergence_volume_short".to_string(),
                    structure_stop_loss_price: latest.candle.high,
                    structure_stop_loss_source: "rsi_divergence_signal_candle_high",
                }));
            }
        }
    }
    Ok(None)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DivergencePivot {
    price: f64,
    rsi: f64,
}

/// 倒序返回最近一个已由左右窗口确认的枢轴；右侧确认全部早于当前信号 K 线。
fn latest_confirmed_divergence_pivot(
    candles: &[ComputedCandle],
    latest_idx: usize,
    low_pivot: bool,
) -> Option<DivergencePivot> {
    let wing = RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES;
    let start = latest_idx.saturating_sub(RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES);
    let first_center = start.checked_add(wing)?;
    let last_center = latest_idx.checked_sub(wing + 1)?;
    if first_center > last_center {
        return None;
    }
    (first_center..=last_center).rev().find_map(|center| {
        let candidate = candles.get(center)?;
        let range = candles.get(center - wing..=center + wing)?;
        let price = if low_pivot {
            candidate.candle.low
        } else {
            candidate.candle.high
        };
        let is_pivot = range.iter().enumerate().all(|(offset, item)| {
            offset == wing
                || if low_pivot {
                    item.candle.low >= price
                } else {
                    item.candle.high <= price
                }
        }) && range.iter().enumerate().any(|(offset, item)| {
            offset != wing
                && if low_pivot {
                    item.candle.low > price
                } else {
                    item.candle.high < price
                }
        });
        if !is_pivot {
            return None;
        }
        Some(DivergencePivot {
            price,
            rsi: candidate.rsi14?,
        })
    })
}

/// v3/v4/v5 的趋势反转分支只接受 96 根首开到末收的 8% 净幅，不再接受线性回归替代条件。
fn v3_net_move_signal(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<Option<RsiVolumeRegimeSignal>, &'static str> {
    let start = latest_idx
        .checked_sub(RSI_VOLUME_TREND_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let history = candles
        .get(start..latest_idx)
        .filter(|items| items.len() == RSI_VOLUME_TREND_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let first_open = history
        .first()
        .map(|item| item.candle.open)
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let last_close = history
        .last()
        .map(|item| item.candle.close)
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let net_move_pct = (last_close - first_open) / first_open * 100.0;
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    if net_move_pct <= -RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT {
        return Ok(Some(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Long,
            trigger: "opposite_96_net_decline_volume_long".to_string(),
            structure_stop_loss_price: latest.candle.low,
            structure_stop_loss_source: "signal_candle_low_before_atr_stop",
        }));
    }
    if net_move_pct >= RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT {
        return Ok(Some(RsiVolumeRegimeSignal {
            direction: MarketVelocityTradeDirection::Short,
            trigger: "opposite_96_net_rise_volume_short".to_string(),
            structure_stop_loss_price: latest.candle.high,
            structure_stop_loss_source: "signal_candle_high_before_atr_stop",
        }));
    }
    Ok(None)
}

/// 普通分支用 RSI 极值定方向，并要求此前 96 根的线性趋势或 8% 净幅任一成立。
fn normal_rsi_reversal_signal(
    candles: &[ComputedCandle],
    latest_idx: usize,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let latest = candles
        .get(latest_idx)
        .ok_or("rsi_volume_regime_not_ready")?;
    let rsi = latest.rsi14.ok_or("rsi_volume_regime_not_ready")?;
    let direction = if rsi < RSI_VOLUME_OVERSOLD {
        MarketVelocityTradeDirection::Long
    } else if rsi > RSI_VOLUME_OVERBOUGHT {
        MarketVelocityTradeDirection::Short
    } else {
        return Err("rsi_volume_regime_rsi_not_extreme");
    };
    if !opposite_history_confirmed(candles, latest_idx, direction)? {
        return Err("rsi_volume_regime_opposite_history_not_confirmed");
    }
    match direction {
        MarketVelocityTradeDirection::Long => Ok(RsiVolumeRegimeSignal {
            direction,
            trigger: "rsi_oversold_volume_reversal_long".to_string(),
            structure_stop_loss_price: latest.candle.low,
            structure_stop_loss_source: "rsi_signal_candle_low",
        }),
        MarketVelocityTradeDirection::Short => Ok(RsiVolumeRegimeSignal {
            direction,
            trigger: "rsi_overbought_volume_reversal_short".to_string(),
            structure_stop_loss_price: latest.candle.high,
            structure_stop_loss_source: "rsi_signal_candle_high",
        }),
        MarketVelocityTradeDirection::Both => Err("rsi_volume_regime_invalid_direction"),
    }
}

/// 检查信号前 96 根的净幅或线性趋势；两个历史条件是 OR，不读取当前信号 K 线。
fn opposite_history_confirmed(
    candles: &[ComputedCandle],
    latest_idx: usize,
    direction: MarketVelocityTradeDirection,
) -> Result<bool, &'static str> {
    let start = latest_idx
        .checked_sub(RSI_VOLUME_TREND_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let history = candles
        .get(start..latest_idx)
        .filter(|items| items.len() == RSI_VOLUME_TREND_LOOKBACK_CANDLES)
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let first_open = history
        .first()
        .map(|item| item.candle.open)
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let last_close = history
        .last()
        .map(|item| item.candle.close)
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_opposite_history_not_ready")?;
    let net_move_pct = match direction {
        MarketVelocityTradeDirection::Long => (first_open - last_close) / first_open * 100.0,
        MarketVelocityTradeDirection::Short => (last_close - first_open) / first_open * 100.0,
        MarketVelocityTradeDirection::Both => return Ok(false),
    };
    Ok(net_move_pct >= RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT
        || linear_trend_confirmed(history, direction))
}

/// 对 96 根收盘价计算线性回归方向与 R²；途中反弹会自然降低拟合度。
fn linear_trend_confirmed(
    history: &[ComputedCandle],
    direction: MarketVelocityTradeDirection,
) -> bool {
    let sample_count = history.len() as f64;
    let mean_x = (sample_count - 1.0) / 2.0;
    let mean_y = history.iter().map(|item| item.candle.close).sum::<f64>() / sample_count;
    if !valid_positive(mean_y) {
        return false;
    }
    let (covariance, variance_x, variance_y) = history.iter().enumerate().fold(
        (0.0, 0.0, 0.0),
        |(covariance, variance_x, variance_y), (idx, item)| {
            let x_distance = idx as f64 - mean_x;
            let y_distance = item.candle.close - mean_y;
            (
                covariance + x_distance * y_distance,
                variance_x + x_distance * x_distance,
                variance_y + y_distance * y_distance,
            )
        },
    );
    if !valid_positive(variance_x) || !valid_positive(variance_y) {
        return false;
    }
    let slope = covariance / variance_x;
    let r_squared = covariance * covariance / (variance_x * variance_y);
    r_squared >= RSI_VOLUME_MIN_TREND_R_SQUARED
        && match direction {
            MarketVelocityTradeDirection::Long => slope < 0.0,
            MarketVelocityTradeDirection::Short => slope > 0.0,
            MarketVelocityTradeDirection::Both => false,
        }
}

/// 按版本冻结的实体倍数拒绝反向长影线；零实体无法形成可审计比例，直接阻塞。
fn validate_opposing_wick(
    latest: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    max_body_multiple: f64,
) -> Result<(), &'static str> {
    let body = (latest.candle.close - latest.candle.open).abs();
    if !body.is_finite() || body <= 0.0 {
        return Err("rsi_volume_regime_zero_body");
    }
    let upper_wick = latest.candle.high - latest.candle.open.max(latest.candle.close);
    let lower_wick = latest.candle.open.min(latest.candle.close) - latest.candle.low;
    match direction {
        MarketVelocityTradeDirection::Long if upper_wick > body * max_body_multiple => {
            Err("rsi_volume_regime_long_upper_wick_blocked")
        }
        MarketVelocityTradeDirection::Short if lower_wick > body * max_body_multiple => {
            Err("rsi_volume_regime_short_lower_wick_blocked")
        }
        MarketVelocityTradeDirection::Both => Err("rsi_volume_regime_invalid_direction"),
        _ => Ok(()),
    }
}

/// v3/v4/v5 以信号收盘价为候选入场价冻结 1.5 ATR14 保护位，不再应用固定百分比上限。
fn apply_atr_stop(
    latest: &ComputedCandle,
    mut signal: RsiVolumeRegimeSignal,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    let entry_price = latest.candle.close;
    let atr = latest
        .atr14
        .filter(|value| valid_positive(*value))
        .ok_or("rsi_volume_regime_atr_not_ready")?;
    let distance = atr * RSI_VOLUME_V3_STOP_ATR_MULTIPLIER;
    signal.structure_stop_loss_price = match signal.direction {
        MarketVelocityTradeDirection::Long => entry_price - distance,
        MarketVelocityTradeDirection::Short => entry_price + distance,
        MarketVelocityTradeDirection::Both => {
            return Err("rsi_volume_regime_invalid_direction");
        }
    };
    signal.structure_stop_loss_source = RSI_VOLUME_V3_ATR_STOP_SOURCE;
    if !valid_positive(signal.structure_stop_loss_price) {
        return Err("rsi_volume_regime_atr_stop_invalid");
    }
    Ok(signal)
}

/// 强制结构止损位于亏损侧且不超过策略最大初始风险。
fn validate_structure_stop(
    entry_price: f64,
    signal: RsiVolumeRegimeSignal,
    max_stop_loss_pct: f64,
) -> Result<RsiVolumeRegimeSignal, &'static str> {
    if !valid_positive(entry_price) || !valid_positive(signal.structure_stop_loss_price) {
        return Err("rsi_volume_regime_structure_stop_invalid");
    }
    let is_loss_side = match signal.direction {
        MarketVelocityTradeDirection::Long => signal.structure_stop_loss_price < entry_price,
        MarketVelocityTradeDirection::Short => signal.structure_stop_loss_price > entry_price,
        MarketVelocityTradeDirection::Both => false,
    };
    if !is_loss_side {
        return Err("rsi_volume_regime_structure_stop_invalid");
    }
    let distance_pct = (signal.structure_stop_loss_price - entry_price).abs() / entry_price;
    if !distance_pct.is_finite() || distance_pct > max_stop_loss_pct {
        return Err("rsi_volume_regime_structure_stop_too_wide");
    }
    Ok(signal)
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    fn computed(idx: usize, open: f64, high: f64, low: f64, close: f64) -> ComputedCandle {
        ComputedCandle {
            volume_ccy: None,
            candle: BacktestCandle {
                ts: idx as i64 * 15 * 60 * 1_000,
                open,
                high,
                low,
                close,
                volume: 40.0,
            },
            sma: Some(close),
            ema: Some(close),
            ema12: None,
            ema144: None,
            ema169: None,
            ema696: None,
            previous_volume_avg: Some(50.0),
            previous_range_avg: Some(1.0),
            rsi14: Some(35.0),
            atr14: Some(1.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: None,
            macd_signal_line: None,
            macd_histogram: None,
        }
    }

    fn declining_history() -> Vec<ComputedCandle> {
        (0..RSI_VOLUME_TREND_LOOKBACK_CANDLES)
            .map(|idx| {
                let close = 100.0 - idx as f64 * 0.04;
                computed(idx, close + 0.02, close + 0.1, close - 0.1, close)
            })
            .collect()
    }

    fn v2_signal(candles: &[ComputedCandle]) -> Result<RsiVolumeRegimeSignal, &'static str> {
        rsi_volume_regime_signal_for_version(
            candles,
            candles.len(),
            RSI_VOLUME_MIN_RATIO,
            0.03,
            RsiVolumeRegimeVersion::V2,
        )
    }

    fn v3_signal(candles: &[ComputedCandle]) -> Result<RsiVolumeRegimeSignal, &'static str> {
        rsi_volume_regime_signal_for_version(
            candles,
            candles.len(),
            RSI_VOLUME_V3_MIN_RATIO,
            0.03,
            RsiVolumeRegimeVersion::V3,
        )
    }

    fn v4_signal(candles: &[ComputedCandle]) -> Result<RsiVolumeRegimeSignal, &'static str> {
        rsi_volume_regime_signal_for_version(
            candles,
            candles.len(),
            RSI_VOLUME_V3_MIN_RATIO,
            0.03,
            RsiVolumeRegimeVersion::V4,
        )
    }

    fn v5_signal(candles: &[ComputedCandle]) -> Result<RsiVolumeRegimeSignal, &'static str> {
        rsi_volume_regime_signal_for_version(
            candles,
            candles.len(),
            RSI_VOLUME_V5_MIN_RATIO,
            0.03,
            RsiVolumeRegimeVersion::V5,
        )
    }

    fn v3_net_history(first_open: f64, last_close: f64) -> Vec<ComputedCandle> {
        (0..RSI_VOLUME_TREND_LOOKBACK_CANDLES)
            .map(|idx| {
                let progress = idx as f64 / (RSI_VOLUME_TREND_LOOKBACK_CANDLES - 1) as f64;
                let close = first_open + (last_close - first_open) * progress;
                computed(idx, close, close + 0.2, close - 0.2, close)
            })
            .collect()
    }

    fn narrow_band_context() -> Vec<ComputedCandle> {
        (0..RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES)
            .map(|idx| {
                let mut item = computed(idx, 100.0, 100.4, 99.6, 100.0);
                // 前两根 RSI 故意不在旧中性区间，证明 v2 不再读取该判断。
                item.rsi14 = Some(if idx % 2 == 0 { 35.0 } else { 65.0 });
                item.bollinger_middle = Some(100.0);
                item.bollinger_upper = Some(100.5);
                item.bollinger_lower = Some(99.5);
                item.bollinger_bandwidth_pct = Some(1.0);
                item.macd_line = Some(0.05);
                item.macd_signal_line = Some(0.04);
                item
            })
            .collect()
    }

    #[test]
    fn normal_oversold_long_accepts_linear_decline_via_or_branch() {
        let mut candles = declining_history();
        let mut signal = computed(96, 96.0, 96.1, 95.4, 95.8);
        signal.rsi14 = Some(29.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let result = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect("linear decline should satisfy the OR history gate");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.structure_stop_loss_price, 95.4);
    }

    #[test]
    fn normal_overbought_short_mirrors_the_linear_trend_branch() {
        let mut candles = (0..RSI_VOLUME_TREND_LOOKBACK_CANDLES)
            .map(|idx| {
                let close = 100.0 + idx as f64 * 0.04;
                computed(idx, close - 0.02, close + 0.1, close - 0.1, close)
            })
            .collect::<Vec<_>>();
        let mut signal = computed(96, 103.8, 104.4, 103.7, 104.0);
        signal.rsi14 = Some(71.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let result = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect("linear rise should allow the mirrored overbought short");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(result.structure_stop_loss_price, 104.4);
    }

    #[test]
    fn eight_percent_net_move_passes_even_without_a_stable_linear_trend() {
        let mut candles = (0..RSI_VOLUME_TREND_LOOKBACK_CANDLES)
            .map(|idx| {
                let close = if idx + 1 == RSI_VOLUME_TREND_LOOKBACK_CANDLES {
                    91.0
                } else if idx % 2 == 0 {
                    101.0
                } else {
                    99.0
                };
                computed(idx, 100.0, close + 0.2, close - 0.2, close)
            })
            .collect::<Vec<_>>();
        let mut signal = computed(96, 91.2, 91.3, 90.5, 91.0);
        signal.rsi14 = Some(29.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let result = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect("8% net decline is the OR alternative to a stable regression trend");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
    }

    #[test]
    fn sideways_upside_break_uses_range_low_stop_without_96_candle_history() {
        let prefix = (0..3)
            .map(|idx| computed(idx, 100.0, 100.2, 99.8, 100.0))
            .collect::<Vec<_>>();
        let mut first = computed(3, 100.0, 100.5, 99.5, 100.1);
        first.rsi14 = Some(50.0);
        first.candle.volume = 60.0;
        let mut second = computed(4, 100.1, 100.6, 99.6, 100.0);
        second.rsi14 = Some(55.0);
        second.candle.volume = 70.0;
        let mut latest = computed(5, 100.2, 101.3, 100.0, 101.0);
        latest.rsi14 = Some(65.0);
        latest.candle.volume = 100.0;
        let mut candles = prefix;
        candles.extend([first, second, latest]);

        let result = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect("sideways breakout should not require the normal 96-candle history");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.structure_stop_loss_price, 99.5);
    }

    #[test]
    fn sideways_downside_break_uses_range_high_stop() {
        let prefix = (0..3)
            .map(|idx| computed(idx, 100.0, 100.2, 99.8, 100.0))
            .collect::<Vec<_>>();
        let mut first = computed(3, 100.0, 100.5, 99.5, 100.1);
        first.rsi14 = Some(45.0);
        first.candle.volume = 60.0;
        let mut second = computed(4, 100.1, 100.6, 99.6, 100.0);
        second.rsi14 = Some(50.0);
        second.candle.volume = 70.0;
        let mut latest = computed(5, 99.9, 100.1, 98.8, 99.0);
        latest.rsi14 = Some(35.0);
        latest.candle.volume = 100.0;
        let mut candles = prefix;
        candles.extend([first, second, latest]);

        let result = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect("sideways breakdown should produce a structural short");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(result.structure_stop_loss_price, 100.6);
    }

    #[test]
    fn v2_uses_narrow_band_and_macd_zero_context_instead_of_previous_rsi() {
        let mut candles = narrow_band_context();
        let mut signal = computed(96, 100.2, 101.2, 100.1, 101.0);
        signal.rsi14 = Some(71.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let result = v2_signal(&candles)
            .expect("overbought close above a compressed upper band should break out long");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.trigger, "rsi_overbought_narrow_band_breakout_long");
        assert_eq!(result.structure_stop_loss_price, 99.5);
    }

    #[test]
    fn v2_rejects_sideways_classification_when_macd_is_not_near_zero() {
        let mut candles = narrow_band_context();
        let previous = candles.last_mut().expect("context has previous candle");
        previous.macd_line = Some(0.5);
        previous.macd_signal_line = Some(0.4);
        let mut signal = computed(96, 100.2, 101.2, 100.1, 101.0);
        signal.rsi14 = Some(71.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let error = v2_signal(&candles)
            .expect_err("MACD away from zero must not classify a flat price history as sideways");

        assert_eq!(error, "rsi_volume_regime_opposite_history_not_confirmed");
    }

    #[test]
    fn v2_bullish_divergence_with_volume_enters_without_96_candle_history() {
        let mut candles = (0..24)
            .map(|idx| computed(idx, 100.0, 100.5, 96.0, 99.5))
            .collect::<Vec<_>>();
        let pivot_idx = 12;
        candles[pivot_idx].candle.low = 95.0;
        candles[pivot_idx].rsi14 = Some(35.0);
        let mut signal = computed(24, 94.3, 94.9, 94.0, 94.8);
        signal.rsi14 = Some(45.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let result = v2_signal(&candles)
            .expect("lower price low with a three-point higher RSI and volume should enter long");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.trigger, "rsi_bullish_divergence_volume_long");
        assert_eq!(result.structure_stop_loss_price, 94.0);
    }

    #[test]
    fn v2_bearish_divergence_with_volume_enters_short() {
        let mut candles = (0..24)
            .map(|idx| computed(idx, 100.0, 104.0, 99.5, 100.5))
            .collect::<Vec<_>>();
        let pivot_idx = 12;
        candles[pivot_idx].candle.high = 105.0;
        candles[pivot_idx].rsi14 = Some(65.0);
        let mut signal = computed(24, 105.7, 106.0, 105.1, 105.3);
        signal.rsi14 = Some(55.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let result = v2_signal(&candles)
            .expect("higher price high with a three-point lower RSI and volume should enter short");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(result.trigger, "rsi_bearish_divergence_volume_short");
        assert_eq!(result.structure_stop_loss_price, 106.0);
    }

    #[test]
    fn v2_divergence_ignores_candles_after_the_signal_time() {
        let mut candles = (0..24)
            .map(|idx| computed(idx, 100.0, 100.5, 96.0, 99.5))
            .collect::<Vec<_>>();
        candles[12].candle.low = 95.0;
        candles[12].rsi14 = Some(20.0);
        let mut signal = computed(24, 94.3, 94.9, 94.0, 94.8);
        signal.rsi14 = Some(25.0);
        signal.candle.volume = 100.0;
        candles.push(signal);
        let completed_count = candles.len();
        let mut future = computed(25, 94.8, 120.0, 80.0, 110.0);
        future.rsi14 = Some(90.0);
        future.candle.volume = 1_000.0;
        candles.push(future);

        let result = rsi_volume_regime_signal_for_version(
            &candles,
            completed_count,
            RSI_VOLUME_MIN_RATIO,
            0.03,
            RsiVolumeRegimeVersion::V2,
        )
        .expect("future candle must not change the already visible divergence signal");

        assert_eq!(result.trigger, "rsi_bullish_divergence_volume_long");
    }

    #[test]
    fn v3_volume_ratio_uses_only_previous_four_candles_and_accepts_exactly_one_point_five() {
        let mut candles = v3_net_history(100.0, 92.0);
        candles[91].candle.volume = 1_000.0;
        let mut signal = computed(96, 91.4, 91.5, 90.7, 91.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result = v3_signal(&candles)
            .expect("the fifth prior candle must not contaminate the four-candle average");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.structure_stop_loss_price, 89.5);
        assert_eq!(
            result.structure_stop_loss_source,
            RSI_VOLUME_V3_ATR_STOP_SOURCE
        );
    }

    #[test]
    fn v5_excludes_history_at_or_above_two_times_from_the_current_ten_candle_average() {
        let mut candles = v3_net_history(100.0, 92.0);
        candles
            .iter_mut()
            .for_each(|item| item.candle.volume = 10.0);
        // 索引 86 恰好为自身前十根均量的两倍，索引 94 为三倍；两者都必须被标记。
        candles[86].candle.volume = 20.0;
        candles[94].candle.volume = 30.0;
        let mut signal = computed(96, 91.4, 91.5, 90.7, 91.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 20.0;
        candles.push(signal);

        let result = v5_signal(&candles).expect(
            "current volume must use the ten-candle average after marked spikes are removed",
        );
        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);

        let v4_error = v4_signal(&candles)
            .expect_err("v4 must retain its original unfiltered previous-four baseline");
        assert_eq!(v4_error, "rsi_volume_regime_volume_not_confirmed");
    }

    #[test]
    fn v5_keeps_a_history_candle_below_two_times_in_the_current_average() {
        let mut candles = v3_net_history(100.0, 92.0);
        candles
            .iter_mut()
            .for_each(|item| item.candle.volume = 10.0);
        candles[86].candle.volume = 19.0;
        let mut signal = computed(96, 91.4, 91.5, 90.7, 91.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 20.0;
        candles.push(signal);

        let error = v5_signal(&candles)
            .expect_err("a 1.9x history candle must remain in the denominator and lower the ratio");
        assert_eq!(error, "rsi_volume_regime_volume_not_confirmed");
    }

    #[test]
    fn v5_requires_twenty_completed_history_candles_for_causal_marking() {
        let candles = (0..20)
            .map(|idx| computed(idx, 100.0, 100.2, 99.8, 100.1))
            .collect::<Vec<_>>();

        let error = v5_filtered_current_volume_ratio(&candles, 19)
            .expect_err("ten recent candles plus each candle's prior-ten baseline are required");
        assert_eq!(error, "rsi_volume_regime_not_ready");
    }

    #[test]
    fn v3_bullish_divergence_accepts_one_rsi_point_only_below_thirty() {
        let mut candles = (0..24)
            .map(|idx| computed(idx, 100.0, 100.5, 96.0, 99.5))
            .collect::<Vec<_>>();
        candles[12].candle.low = 95.0;
        candles[12].rsi14 = Some(28.0);
        let mut signal = computed(24, 94.2, 95.0, 94.0, 94.8);
        signal.rsi14 = Some(29.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result =
            v3_signal(&candles).expect("one RSI point must pass below the strict threshold");
        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.trigger, "rsi_bullish_divergence_volume_long");
        assert_eq!(result.structure_stop_loss_price, 93.3);

        candles.last_mut().expect("signal candle").rsi14 = Some(30.0);
        let error = v3_signal(&candles).expect_err("RSI equal to 30 is not strictly oversold");
        assert_eq!(error, "rsi_volume_regime_no_entry_branch_confirmed");
    }

    #[test]
    fn v3_bearish_divergence_accepts_one_rsi_point_only_above_seventy() {
        let mut candles = (0..24)
            .map(|idx| computed(idx, 100.0, 104.0, 99.5, 100.5))
            .collect::<Vec<_>>();
        candles[12].candle.high = 105.0;
        candles[12].rsi14 = Some(72.0);
        let mut signal = computed(24, 105.8, 106.0, 105.0, 105.2);
        signal.rsi14 = Some(71.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result =
            v3_signal(&candles).expect("one RSI point must pass above the strict threshold");
        assert_eq!(result.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(result.trigger, "rsi_bearish_divergence_volume_short");
        assert_eq!(result.structure_stop_loss_price, 106.7);
    }

    #[test]
    fn v3_sideways_breakout_ignores_current_rsi_and_uses_atr_stop() {
        let mut candles = narrow_band_context();
        let mut signal = computed(96, 100.2, 101.2, 100.1, 101.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result = v3_signal(&candles).expect("sideways breakout must not read current RSI");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.trigger, "narrow_band_zero_macd_breakout_long");
        assert_eq!(result.structure_stop_loss_price, 99.5);
    }

    #[test]
    fn v4_removes_sideways_breakout_on_both_sides_but_keeps_v3_replay() {
        let mut long_candles = narrow_band_context();
        let mut long_signal = computed(96, 100.2, 101.2, 100.1, 101.0);
        long_signal.rsi14 = Some(71.0);
        long_signal.candle.volume = 60.0;
        long_candles.push(long_signal);

        assert_eq!(
            v3_signal(&long_candles)
                .expect("v3 sideways long must remain replayable")
                .trigger,
            "narrow_band_zero_macd_breakout_long"
        );
        assert_eq!(
            v4_signal(&long_candles).expect_err("v4 must remove the sideways long branch"),
            "rsi_volume_regime_no_entry_branch_confirmed"
        );

        let mut short_candles = narrow_band_context();
        let mut short_signal = computed(96, 99.8, 99.9, 98.8, 99.0);
        short_signal.rsi14 = Some(29.0);
        short_signal.candle.volume = 60.0;
        short_candles.push(short_signal);

        assert_eq!(
            v3_signal(&short_candles)
                .expect("v3 sideways short must remain replayable")
                .trigger,
            "narrow_band_zero_macd_breakdown_short"
        );
        assert_eq!(
            v4_signal(&short_candles).expect_err("v4 must remove the sideways short branch"),
            "rsi_volume_regime_no_entry_branch_confirmed"
        );
    }

    #[test]
    fn v4_keeps_extreme_rsi_divergence_branch() {
        let mut candles = (0..24)
            .map(|idx| computed(idx, 100.0, 100.5, 96.0, 99.5))
            .collect::<Vec<_>>();
        candles[12].candle.low = 95.0;
        candles[12].rsi14 = Some(28.0);
        let mut signal = computed(24, 94.2, 95.0, 94.0, 94.8);
        signal.rsi14 = Some(29.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result = v4_signal(&candles).expect("v4 must keep the RSI divergence branch");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.trigger, "rsi_bullish_divergence_volume_long");
    }

    #[test]
    fn v4_keeps_net_move_branch_without_current_rsi_extreme() {
        let mut candles = v3_net_history(100.0, 92.0);
        let mut signal = computed(96, 91.4, 91.5, 90.7, 91.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result = v4_signal(&candles).expect("v4 must keep the 96-candle net-move branch");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.trigger, "opposite_96_net_decline_volume_long");
    }

    #[test]
    fn v3_net_move_is_mirrored_and_does_not_require_current_candle_color() {
        let mut long_candles = v3_net_history(100.0, 92.0);
        let mut bearish_signal = computed(96, 91.4, 91.5, 90.7, 91.0);
        bearish_signal.rsi14 = Some(50.0);
        bearish_signal.candle.volume = 60.0;
        long_candles.push(bearish_signal);
        let long =
            v3_signal(&long_candles).expect("a bearish current candle may trigger the long branch");
        assert_eq!(long.direction, MarketVelocityTradeDirection::Long);

        let mut short_candles = v3_net_history(100.0, 108.0);
        let mut bullish_signal = computed(96, 108.6, 109.2, 108.5, 109.0);
        bullish_signal.rsi14 = Some(50.0);
        bullish_signal.candle.volume = 60.0;
        short_candles.push(bullish_signal);
        let short = v3_signal(&short_candles)
            .expect("a bullish current candle may trigger the short branch");
        assert_eq!(short.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(short.structure_stop_loss_price, 110.5);
    }

    #[test]
    fn v3_does_not_accept_linear_trend_below_eight_percent() {
        let mut candles = declining_history();
        let mut signal = computed(96, 96.0, 96.1, 95.4, 95.8);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let error = v3_signal(&candles).expect_err("v3 removed the regression-only alternative");

        assert_eq!(error, "rsi_volume_regime_no_entry_branch_confirmed");
    }

    #[test]
    fn v3_applies_forty_five_percent_wick_limit_and_rejects_zero_body() {
        let mut allowed = v3_net_history(100.0, 92.0);
        let mut allowed_signal = computed(96, 92.0, 92.44, 90.7, 91.0);
        allowed_signal.rsi14 = Some(50.0);
        allowed_signal.candle.volume = 60.0;
        allowed.push(allowed_signal);
        assert!(v3_signal(&allowed).is_ok());

        let mut blocked = v3_net_history(100.0, 92.0);
        let mut blocked_signal = computed(96, 92.0, 92.46, 90.7, 91.0);
        blocked_signal.rsi14 = Some(50.0);
        blocked_signal.candle.volume = 60.0;
        blocked.push(blocked_signal);
        assert_eq!(
            v3_signal(&blocked).expect_err("upper wick above 45% must block a long"),
            "rsi_volume_regime_long_upper_wick_blocked"
        );

        let mut zero_body = v3_net_history(100.0, 92.0);
        let mut doji = computed(96, 91.0, 91.1, 90.5, 91.0);
        doji.rsi14 = Some(50.0);
        doji.candle.volume = 60.0;
        zero_body.push(doji);
        assert_eq!(
            v3_signal(&zero_body).expect_err("a zero body has no valid wick ratio"),
            "rsi_volume_regime_zero_body"
        );
    }

    #[test]
    fn v3_blocks_opposing_branches_on_the_same_candle() {
        let mut candles = narrow_band_context();
        candles[0].candle.open = 110.0;
        candles[0].candle.high = 110.2;
        let mut signal = computed(96, 99.8, 99.9, 98.8, 99.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let error =
            v3_signal(&candles).expect_err("breakdown short conflicts with net-decline long");

        assert_eq!(error, "rsi_volume_regime_branch_direction_conflict");
    }

    #[test]
    fn v3_joins_same_direction_branch_reasons_and_enters_once() {
        let mut candles = narrow_band_context();
        candles[0].candle.open = 92.0;
        candles[0].candle.low = 91.8;
        let mut signal = computed(96, 99.8, 99.9, 98.8, 99.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);

        let result = v3_signal(&candles).expect("both short branches must merge into one signal");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(
            result.trigger,
            "narrow_band_zero_macd_breakdown_short+opposite_96_net_rise_volume_short"
        );
    }

    #[test]
    fn v3_ignores_candles_after_the_completed_signal_count() {
        let mut candles = v3_net_history(100.0, 92.0);
        let mut signal = computed(96, 91.4, 91.5, 90.7, 91.0);
        signal.rsi14 = Some(50.0);
        signal.candle.volume = 60.0;
        candles.push(signal);
        let completed_count = candles.len();
        let mut future = computed(97, 91.0, 130.0, 70.0, 120.0);
        future.rsi14 = Some(90.0);
        future.candle.volume = 2_000.0;
        candles.push(future);

        let result = rsi_volume_regime_signal_for_version(
            &candles,
            completed_count,
            RSI_VOLUME_V3_MIN_RATIO,
            0.03,
            RsiVolumeRegimeVersion::V3,
        )
        .expect("future candles must not affect the completed signal");

        assert_eq!(result.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(result.structure_stop_loss_price, 89.5);
    }

    #[test]
    fn long_upper_wick_blocks_an_otherwise_valid_signal() {
        let mut candles = declining_history();
        let mut signal = computed(96, 96.0, 98.0, 95.4, 95.8);
        signal.rsi14 = Some(29.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let error = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect_err("long upper wick must block long entry");

        assert_eq!(error, "rsi_volume_regime_long_upper_wick_blocked");
    }

    #[test]
    fn short_lower_wick_blocks_an_otherwise_valid_signal() {
        let mut candles = (0..RSI_VOLUME_TREND_LOOKBACK_CANDLES)
            .map(|idx| {
                let close = 100.0 + idx as f64 * 0.04;
                computed(idx, close - 0.02, close + 0.1, close - 0.1, close)
            })
            .collect::<Vec<_>>();
        let mut signal = computed(96, 103.8, 104.4, 101.0, 104.0);
        signal.rsi14 = Some(71.0);
        signal.candle.volume = 100.0;
        candles.push(signal);

        let error = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect_err("short lower wick must block short entry");

        assert_eq!(error, "rsi_volume_regime_short_lower_wick_blocked");
    }

    #[test]
    fn current_volume_ratio_is_a_hard_gate() {
        let mut candles = declining_history();
        let mut signal = computed(96, 96.0, 96.1, 95.4, 95.8);
        signal.rsi14 = Some(29.0);
        signal.candle.volume = 70.0;
        candles.push(signal);

        let error = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect_err("sub-threshold current volume must block entry");

        assert_eq!(error, "rsi_volume_regime_volume_not_confirmed");
    }

    #[test]
    fn structure_stop_wider_than_three_percent_blocks_entry() {
        let prefix = (0..3)
            .map(|idx| computed(idx, 100.0, 100.2, 99.8, 100.0))
            .collect::<Vec<_>>();
        let mut first = computed(3, 100.0, 100.5, 95.0, 100.1);
        first.rsi14 = Some(50.0);
        first.candle.volume = 60.0;
        let mut second = computed(4, 100.1, 100.6, 99.6, 100.0);
        second.rsi14 = Some(55.0);
        second.candle.volume = 70.0;
        let mut latest = computed(5, 100.2, 101.3, 100.0, 101.0);
        latest.candle.volume = 100.0;
        let mut candles = prefix;
        candles.extend([first, second, latest]);

        let error = rsi_volume_regime_signal(&candles, candles.len(), RSI_VOLUME_MIN_RATIO, 0.03)
            .expect_err("too-wide range stop must block instead of using a fixed fallback");

        assert_eq!(error, "rsi_volume_regime_structure_stop_too_wide");
    }

    #[test]
    fn research_identity_is_independent_and_not_paper_eligible() {
        use crate::app::market_velocity_event_backtest::{
            market_rsi_volume_regime_v1_research_args, market_rsi_volume_regime_v2_research_args,
            market_rsi_volume_regime_v3_research_args, market_rsi_volume_regime_v4_research_args,
            market_rsi_volume_regime_v5_research_args, market_velocity_paper_observation_usage,
            market_velocity_paper_strategy_preset_manifest, market_velocity_strategy_detail,
            market_velocity_strategy_type, MarketVelocityEventSource, MarketVelocityStopLossMode,
            MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY, MARKET_RSI_VOLUME_REGIME_V1_PRESET,
            MARKET_RSI_VOLUME_REGIME_V2_PRESET, MARKET_RSI_VOLUME_REGIME_V3_PRESET,
            MARKET_RSI_VOLUME_REGIME_V4_PRESET, MARKET_RSI_VOLUME_REGIME_V5_PRESET,
        };

        let args = market_rsi_volume_regime_v1_research_args()
            .expect("build RSI volume regime research args");
        assert_eq!(args.event_source, MarketVelocityEventSource::Kline15m);
        assert!(args.entry_rsi_volume_regime);
        assert_eq!(args.entry_min_volume_ratio, RSI_VOLUME_MIN_RATIO);
        assert_eq!(args.trade_direction, MarketVelocityTradeDirection::Both);
        assert_eq!(
            args.stop_loss_mode,
            MarketVelocityStopLossMode::StructureOrFixed
        );
        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY
        );
        assert_eq!(
            market_velocity_strategy_detail(&args)["version_status"],
            "research_unvalidated"
        );

        let manifest =
            market_velocity_paper_strategy_preset_manifest(MARKET_RSI_VOLUME_REGIME_V1_PRESET)
                .expect("build RSI volume regime manifest");
        assert_eq!(manifest.strategy_key, MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY);
        assert_eq!(manifest.channel, "research");
        assert_eq!(
            manifest.manifest_json["execution"]["paper_observation_eligible"],
            false
        );
        assert!(
            !market_velocity_paper_observation_usage().contains(MARKET_RSI_VOLUME_REGIME_V1_PRESET)
        );

        let v2_args = market_rsi_volume_regime_v2_research_args()
            .expect("build RSI volume regime v2 research args");
        assert_eq!(
            rsi_volume_regime_version(&v2_args.paper_outcome_entry_rule_version),
            RsiVolumeRegimeVersion::V2
        );
        assert_eq!(
            market_velocity_strategy_type(&v2_args),
            MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY
        );
        let v2_manifest =
            market_velocity_paper_strategy_preset_manifest(MARKET_RSI_VOLUME_REGIME_V2_PRESET)
                .expect("build RSI volume regime v2 manifest");
        assert_eq!(v2_manifest.channel, "research");
        assert_eq!(
            v2_manifest.manifest_json["execution"]["paper_observation_eligible"],
            false
        );
        assert!(
            !market_velocity_paper_observation_usage().contains(MARKET_RSI_VOLUME_REGIME_V2_PRESET)
        );

        let v3_args = market_rsi_volume_regime_v3_research_args()
            .expect("build RSI volume regime v3 research args");
        assert_eq!(v3_args.entry_min_volume_ratio, RSI_VOLUME_V3_MIN_RATIO);
        assert_eq!(
            rsi_volume_regime_version(&v3_args.paper_outcome_entry_rule_version),
            RsiVolumeRegimeVersion::V3
        );
        assert_eq!(
            market_velocity_strategy_type(&v3_args),
            MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY
        );
        let v3_manifest =
            market_velocity_paper_strategy_preset_manifest(MARKET_RSI_VOLUME_REGIME_V3_PRESET)
                .expect("build RSI volume regime v3 manifest");
        assert_eq!(v3_manifest.channel, "research");
        assert_eq!(
            v3_manifest.manifest_json["parameters"]["fast_momentum_filters"]["rsi_volume_regime"]
                ["stop_loss"]["fixed_percentage_fallback"],
            false
        );
        assert_eq!(
            v3_manifest.manifest_json["parameters"]["stop_loss_mode"],
            "atr14_x_1_5"
        );
        assert!(
            !market_velocity_paper_observation_usage().contains(MARKET_RSI_VOLUME_REGIME_V3_PRESET)
        );

        let v4_args = market_rsi_volume_regime_v4_research_args()
            .expect("build RSI volume regime v4 research args");
        assert_eq!(
            rsi_volume_regime_version(&v4_args.paper_outcome_entry_rule_version),
            RsiVolumeRegimeVersion::V4
        );
        assert_eq!(
            market_velocity_strategy_type(&v4_args),
            MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY
        );
        assert_eq!(
            market_velocity_strategy_detail(&v4_args)["entry_sideways_breakout_enabled"],
            false
        );
        let v4_manifest =
            market_velocity_paper_strategy_preset_manifest(MARKET_RSI_VOLUME_REGIME_V4_PRESET)
                .expect("build RSI volume regime v4 manifest");
        assert_eq!(v4_manifest.channel, "research");
        assert_eq!(
            v4_manifest.manifest_json["parameters"]["fast_momentum_filters"]["rsi_volume_regime"]
                ["sideways_breakout_enabled"],
            false
        );
        assert_eq!(
            v4_manifest.manifest_json["parameters"]["stop_loss_mode"],
            "atr14_x_1_5"
        );
        assert!(
            !market_velocity_paper_observation_usage().contains(MARKET_RSI_VOLUME_REGIME_V4_PRESET)
        );

        let v5_args = market_rsi_volume_regime_v5_research_args()
            .expect("build RSI volume regime v5 research args");
        assert_eq!(v5_args.entry_min_volume_ratio, RSI_VOLUME_V5_MIN_RATIO);
        assert_eq!(
            rsi_volume_regime_version(&v5_args.paper_outcome_entry_rule_version),
            RsiVolumeRegimeVersion::V5
        );
        assert_eq!(
            market_velocity_strategy_type(&v5_args),
            MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY
        );
        assert_eq!(
            market_velocity_strategy_detail(&v5_args)["entry_sideways_breakout_enabled"],
            false
        );
        assert_eq!(
            market_velocity_strategy_detail(&v5_args)["entry_volume_baseline_mode"],
            "causal_previous_10_excluding_marked_spikes"
        );
        let v5_manifest =
            market_velocity_paper_strategy_preset_manifest(MARKET_RSI_VOLUME_REGIME_V5_PRESET)
                .expect("build RSI volume regime v5 manifest");
        assert_eq!(v5_manifest.channel, "research");
        assert_eq!(
            v5_manifest.manifest_json["parameters"]["fast_momentum_filters"]["rsi_volume_regime"]
                ["volume_baseline"]["exclude_marked_historical_spikes_from_current_average"],
            true
        );
        assert_eq!(
            v5_manifest.manifest_json["parameters"]["fast_momentum_filters"]["rsi_volume_regime"]
                ["volume_baseline"]["current_candle_in_average"],
            false
        );
        assert_eq!(
            v5_manifest.manifest_json["parameters"]["stop_loss_mode"],
            "atr14_x_1_5"
        );
        assert!(
            !market_velocity_paper_observation_usage().contains(MARKET_RSI_VOLUME_REGIME_V5_PRESET)
        );
    }
}
