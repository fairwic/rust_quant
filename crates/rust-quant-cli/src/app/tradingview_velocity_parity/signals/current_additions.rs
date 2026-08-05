//! 当前 Pine `3cbbc9d8` 相对冻结 V1 新增的三个纯信号评估器。
//!
//! 调用方只能在已完成 K 线 `t` 收盘后调用；所有窗口均止于 `t`，
//! 返回值只冻结下一根开盘前已经可知的止损、目标与展示量比。

#[cfg(test)]
use super::super::model::IndicatorPoint;
use super::super::model::{Candle, IndicatorSeries};

const ENR_ANCHOR_MIN_DISTANCE: usize = 20;
const ENR_ANCHOR_MAX_DISTANCE: usize = 80;
const ENR_MAX_GAP_RATIO: f64 = 0.005;
const ENR_MAX_GAP_ATR: f64 = 0.50;
const ENR_VOLUME_MULTIPLIER_MIN: f64 = 1.25;
const ENR_ANCHOR_RSI_MIN: f64 = 70.0;
const ENR_SIGNAL_RSI_MIN: f64 = 55.0;
const ENR_SIGNAL_RSI_MAX: f64 = 70.0;
const ENR_BODY_RANGE_MIN: f64 = 0.60;
const ENR_BODY_ATR_MIN: f64 = 0.50;
const ENR_CLOSE_LOCATION_MAX: f64 = 0.20;
const ENR_BREAK_EVEN_RISK: f64 = 1.0;
const ENR_FINAL_TARGET_RISK: f64 = 1.5;

const BLR_LOCATION_LOOKBACK: usize = 48;
const BLR_SHADOW_MIN: f64 = 0.50;
const BLR_CLOSE_LOCATION_MIN: f64 = 0.75;
const BLR_BOTTOM_POSITION_MAX: f64 = 0.15;
const BLR_RSI_MIN: f64 = 35.0;
const BLR_RSI_MAX: f64 = 50.0;
const BLR_MIN_REWARD_RISK: f64 = 1.10;

const EMA596_RECLAIM_MAX_AGE: usize = 32;
const EMA596_STRUCTURE_LOOKBACK: usize = 4;
const EMA596_V10_STRUCTURE_LOOKBACK: usize = 8;
const EMA596_VOLUME_MEDIAN_LOOKBACK: usize = 20;
const EMA596_MEDIAN_VOLUME_RATIO_MIN: f64 = 2.5;
const EMA596_PREVIOUS_DISTANCE_ATR_MAX: f64 = 0.50;
const EMA596_CURRENT_DISTANCE_ATR_MIN: f64 = 1.0;
const EMA596_V10_CURRENT_DISTANCE_ATR_MAX: f64 = 2.5;
const EMA596_BODY_RANGE_MIN: f64 = 0.60;
const EMA596_TARGET_RISK: f64 = 2.0;
const EMA_SLOPE_LOOKBACK: usize = 3;
const EMA144_SLOPE_ATR_MIN: f64 = 0.015;
const EMA596_SLOPE_ATR_MIN: f64 = 0.0015;

/// 高位放量努力无结果空头在下一根开盘前冻结的风险参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct EffortNoResultShortResult {
    /// 两个高点的较高者上方一个 tick，并向上取整后的绝对止损价。
    pub(super) stop_price: f64,
    /// 从信号收盘到止损的 1R tick 数；达到后由 Broker 执行近似保本。
    pub(super) activation_ticks: i64,
    /// 以信号收盘风险计算并向内取整的 1.5R 最终目标 tick 数。
    pub(super) target_ticks: i64,
    /// Pine 悬浮信息使用的过滤量比；缺失不改变已经冻结的 `volume_event`。
    pub(super) display_volume_ratio: Option<f64>,
}

/// 布林下轨收回多头在信号收盘时可冻结的结构保护。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BollingerLowerReclaimLongResult {
    /// 信号低点下方一个 tick，并向下取整后的绝对止损价。
    pub(super) stop_price: f64,
    /// 信号时已经确定的布林中轨绝对目标价。
    pub(super) target_price: f64,
    /// Pine 悬浮信息使用的当前过滤量比。
    pub(super) display_volume_ratio: Option<f64>,
}

/// EMA596 收复接受后再次离轨多头的冻结结构保护。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Ema596ReclaimDepartureLongResult {
    /// 前四根已完成 K 线低点下方一个 tick 的绝对止损价。
    pub(super) stop_price: f64,
    /// 以信号收盘和结构止损计算的 2R 绝对目标价。
    pub(super) target_price: f64,
    /// 当前 `vol_ccy` 相对前 20 根 nearest-rank 中位数的量比。
    pub(super) display_volume_ratio: f64,
}

/// 评估“高位放量努力无结果 + 次高点拒绝 + 布林上轨收回”空头。
///
/// 锚点先在 `t-20..=t-80` 中按最高价唯一冻结；其 RSI 或量能随后失败时
/// 不会回退到更老的次优锚点，避免事后挑选有利对照。
pub(super) fn evaluate_effort_no_result_short(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<EffortNoResultShortResult> {
    let current = *candles.get(index)?;
    let point = indicators.get(index)?;
    let atr = positive(point.atr14?)?;
    let rsi = point.rsi14?;
    let bollinger_upper = point.bollinger_upper?;
    valid_tick(tick_size)?;
    if !current.is_valid() {
        return None;
    }

    let (anchor_index, anchor_high) = highest_enr_anchor(candles, index)?;
    let anchor = candles[anchor_index];
    let anchor_point = indicators.get(anchor_index)?;
    let anchor_rsi = anchor_point.rsi14?;
    let anchor_volume = positive(anchor.volume)?;

    let high_gap = anchor_high - current.high;
    let high_gap_ratio = high_gap / anchor_high;
    let high_gap_atr = high_gap / atr;
    let volume_multiple = current.volume / anchor_volume;
    let body_range_ratio = current.body() / current.range();
    let bear_body_atr = (current.open - current.close) / atr;
    let close_location = (current.close - current.low) / current.range();

    let matched = point.volume_event
        && high_gap > 0.0
        && anchor_high > 0.0
        && (high_gap_ratio <= ENR_MAX_GAP_RATIO || high_gap_atr <= ENR_MAX_GAP_ATR)
        && volume_multiple >= ENR_VOLUME_MULTIPLIER_MIN
        && anchor_rsi >= ENR_ANCHOR_RSI_MIN
        && (ENR_SIGNAL_RSI_MIN..=ENR_SIGNAL_RSI_MAX).contains(&rsi)
        && rsi < anchor_rsi
        && current.close < current.open
        && body_range_ratio >= ENR_BODY_RANGE_MIN
        && bear_body_atr >= ENR_BODY_ATR_MIN
        && close_location <= ENR_CLOSE_LOCATION_MAX
        && current.high > bollinger_upper
        && current.close < bollinger_upper;
    if !matched {
        return None;
    }

    let stop_price = round_up(anchor_high.max(current.high) + tick_size, tick_size);
    let risk_ticks = outward_distance_ticks(stop_price - current.close, tick_size)?;
    let activation_ticks = scaled_target_ticks(risk_ticks, ENR_BREAK_EVEN_RISK).max(1);
    let target_ticks = scaled_target_ticks(risk_ticks, ENR_FINAL_TARGET_RISK).max(1);

    Some(EffortNoResultShortResult {
        stop_price,
        activation_ticks,
        target_ticks,
        display_volume_ratio: point.filtered_volume_ratio,
    })
}

/// 评估“放量下破布林下轨后收回”多头。
///
/// 位置只由 `t-1..=t-48` 定义，信号棒不能用自身低点改写底部区间；
/// 1.1R 也只按信号收盘预检，不读取下一根真实开盘。
pub(super) fn evaluate_bollinger_lower_reclaim_long(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<BollingerLowerReclaimLongResult> {
    let current = *candles.get(index)?;
    let point = indicators.get(index)?;
    let previous = indicators.get(index.checked_sub(1)?)?;
    let previous_two = indicators.get(index.checked_sub(2)?)?;
    let prior = prior_candles(candles, index, BLR_LOCATION_LOOKBACK)?;
    valid_tick(tick_size)?;
    if !current.is_valid() {
        return None;
    }

    let prior_high = maximum_high(prior)?;
    let prior_low = minimum_low(prior)?;
    let prior_range = prior_high - prior_low;
    if prior_range <= 0.0 {
        return None;
    }

    let bollinger_lower = point.bollinger_lower?;
    let bollinger_middle = point.bollinger_middle?;
    let rsi = point.rsi14?;
    let previous_rsi = previous.rsi14?;
    let previous_two_rsi = previous_two.rsi14?;
    let macd = point.macd_histogram?;
    let previous_macd = previous.macd_histogram?;
    let previous_two_macd = previous_two.macd_histogram?;

    let lower_shadow = current.open.min(current.close) - current.low;
    let shadow_ratio = lower_shadow / current.range();
    let close_location = (current.close - current.low) / current.range();
    let bottom_position = ((current.low - prior_low) / prior_range).max(0.0);
    let stop_price = round_down(current.low - tick_size, tick_size);
    let target_price = round_down(bollinger_middle, tick_size);
    let risk = current.close - stop_price;
    let reward = target_price - current.close;
    let reward_risk = (risk > 0.0).then_some(reward / risk)?;

    let matched = point.volume_event
        && current.low < bollinger_lower
        && current.close > bollinger_lower
        && shadow_ratio >= BLR_SHADOW_MIN
        && close_location >= BLR_CLOSE_LOCATION_MIN
        && bottom_position <= BLR_BOTTOM_POSITION_MAX
        && (BLR_RSI_MIN..=BLR_RSI_MAX).contains(&rsi)
        && rsi > previous_rsi
        && previous_rsi > previous_two_rsi
        && macd < 0.0
        && previous_macd < 0.0
        && previous_two_macd < 0.0
        && macd > previous_macd
        && previous_macd > previous_two_macd
        && target_price > current.close
        && reward_risk >= BLR_MIN_REWARD_RISK;
    matched.then_some(BollingerLowerReclaimLongResult {
        stop_price,
        target_price,
        display_volume_ratio: point.filtered_volume_ratio,
    })
}

/// 评估 EMA596 已收复并至少接受一根后，放量形成 HH/HL 的 D2 多头。
///
/// 最近一次收复必须发生在 `t-1..=t-32`，收复至信号期间每根收盘都保持在
/// EMA596 上方；慢线否决只读取信号前已经完成的斜率。
pub(super) fn evaluate_ema596_reclaim_departure_long(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<Ema596ReclaimDepartureLongResult> {
    evaluate_ema596_reclaim_departure_long_with_policy(
        candles, indicators, index, tick_size, false, false,
    )
}

/// V10 在相同 D2 合同上增加慢线非负斜率、八根结构与最大离轨限制。
pub(super) fn evaluate_ema596_reclaim_departure_long_v10(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<Ema596ReclaimDepartureLongResult> {
    evaluate_ema596_reclaim_departure_long_with_policy(
        candles, indicators, index, tick_size, true, false,
    )
}

/// V11 在 V10 结构门禁上增加 RSI 不超过 70 的防追涨确认。
pub(super) fn evaluate_ema596_reclaim_departure_long_v11(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<Ema596ReclaimDepartureLongResult> {
    evaluate_ema596_reclaim_departure_long_with_policy(
        candles, indicators, index, tick_size, true, true,
    )
}

fn evaluate_ema596_reclaim_departure_long_with_policy(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
    strict_v10: bool,
    residual_v11: bool,
) -> Option<Ema596ReclaimDepartureLongResult> {
    let current = *candles.get(index)?;
    let point = indicators.get(index)?;
    let previous_index = index.checked_sub(1)?;
    let previous = indicators.get(previous_index)?;
    let atr = positive(point.atr14?)?;
    let previous_atr = positive(previous.atr14?)?;
    let ema596 = point.ema596?;
    let previous_ema596 = previous.ema596?;
    let weekly_p90 = point.weekly_volume_p90?;
    valid_tick(tick_size)?;
    if !current.is_valid() {
        return None;
    }

    let reclaim_index = latest_ema596_cross_up(candles, indicators, index)?;
    let reclaim_age = index - reclaim_index;
    if !(1..=EMA596_RECLAIM_MAX_AGE).contains(&reclaim_age)
        || !ema596_reclaim_held(candles, indicators, reclaim_index, index)
    {
        return None;
    }

    let previous_distance_atr = (candles[previous_index].close - previous_ema596) / previous_atr;
    let current_distance_atr = (current.close - ema596) / atr;
    let structure_lookback = if strict_v10 {
        EMA596_V10_STRUCTURE_LOOKBACK
    } else {
        EMA596_STRUCTURE_LOOKBACK
    };
    let prior = prior_candles(candles, index, structure_lookback)?;
    let prior_high = maximum_high(prior)?;
    let prior_low = minimum_low(prior)?;
    let median_volume = nearest_rank_median(
        &prior_candles(candles, index, EMA596_VOLUME_MEDIAN_LOOKBACK)?
            .iter()
            .map(|candle| candle.volume)
            .collect::<Vec<_>>(),
    )?;
    let median_volume = positive(median_volume)?;
    let median_volume_ratio = current.volume / median_volume;
    let prior_slow_bear = prior_slow_bear_trend(indicators, index);
    let v10_slow_trend_ready = index
        .checked_sub(1)
        .and_then(|previous| ema_slopes_at(indicators, previous))
        .is_some_and(|(ema12_slope, ema144_slope, ema596_slope)| {
            ema12_slope > 0.0 && ema144_slope >= 0.0 && ema596_slope >= 0.0
        });
    let body_range_ratio = current.body() / current.range();

    let stop_price = round_down(prior_low - tick_size, tick_size);
    let risk = current.close - stop_price;
    let target_price = round_down(current.close + EMA596_TARGET_RISK * risk, tick_size);

    let matched = point.weekly_volume_ready
        && current.volume >= weekly_p90
        && median_volume_ratio >= EMA596_MEDIAN_VOLUME_RATIO_MIN
        && previous_distance_atr > 0.0
        && previous_distance_atr <= EMA596_PREVIOUS_DISTANCE_ATR_MAX
        && current_distance_atr >= EMA596_CURRENT_DISTANCE_ATR_MIN
        && (!strict_v10 || current_distance_atr <= EMA596_V10_CURRENT_DISTANCE_ATR_MAX)
        && (!residual_v11 || point.rsi14.is_some_and(|rsi| rsi <= 70.0))
        && if strict_v10 {
            v10_slow_trend_ready
        } else {
            !prior_slow_bear
        }
        && current.close > prior_high
        && current.high > candles[previous_index].high
        && current.low > candles[previous_index].low
        && current.close > current.open
        && body_range_ratio >= EMA596_BODY_RANGE_MIN
        && risk > 0.0
        && target_price > current.close;
    matched.then_some(Ema596ReclaimDepartureLongResult {
        stop_price,
        target_price,
        display_volume_ratio: median_volume_ratio,
    })
}

/// 在 `t-20..=t-80` 选择最高价锚点；相同高点保留最近者且不做次优回退。
fn highest_enr_anchor(candles: &[Candle], index: usize) -> Option<(usize, f64)> {
    let mut selected = None;
    for offset in ENR_ANCHOR_MIN_DISTANCE..=ENR_ANCHOR_MAX_DISTANCE {
        let anchor_index = index.checked_sub(offset)?;
        let high = candles.get(anchor_index)?.high;
        if !high.is_finite() {
            return None;
        }
        if selected.is_none_or(|(_, selected_high)| high > selected_high) {
            selected = Some((anchor_index, high));
        }
    }
    selected
}

/// 在信号前 32 根内向近处查找最近一次收盘上穿 EMA596。
fn latest_ema596_cross_up(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
) -> Option<usize> {
    let oldest = index.saturating_sub(EMA596_RECLAIM_MAX_AGE).max(1);
    (oldest..=index).rev().find(|candidate| {
        let previous = candidate - 1;
        let current_ema = indicators.get(*candidate).and_then(|point| point.ema596);
        let previous_ema = indicators.get(previous).and_then(|point| point.ema596);
        current_ema
            .zip(previous_ema)
            .is_some_and(|(current_ema, previous_ema)| {
                candles[*candidate].close > current_ema && candles[previous].close <= previous_ema
            })
    })
}

/// 要求收复棒到信号棒的每个已完成收盘都保持在当时 EMA596 上方。
fn ema596_reclaim_held(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    reclaim_index: usize,
    signal_index: usize,
) -> bool {
    (reclaim_index..=signal_index).all(|index| {
        indicators
            .get(index)
            .and_then(|point| point.ema596)
            .is_some_and(|ema596| candles[index].close > ema596)
    })
}

/// 用信号前一根的慢线斜率判定已存在的下跌趋势，避免当前放量棒反向改写门禁。
fn prior_slow_bear_trend(indicators: &IndicatorSeries, index: usize) -> bool {
    let current_slopes = ema_slopes_at(indicators, index);
    let previous_slopes = index
        .checked_sub(1)
        .and_then(|previous| ema_slopes_at(indicators, previous));
    current_slopes
        .zip(previous_slopes)
        .is_some_and(|(_, (_, ema144, ema596))| {
            ema144 <= -EMA144_SLOPE_ATR_MIN && ema596 <= -EMA596_SLOPE_ATR_MIN
        })
}

/// 返回 EMA12/144/596 的三棒 ATR 归一化斜率，结果为无量纲比例。
fn ema_slopes_at(indicators: &IndicatorSeries, index: usize) -> Option<(f64, f64, f64)> {
    let earlier_index = index.checked_sub(EMA_SLOPE_LOOKBACK)?;
    let current = indicators.get(index)?;
    let earlier = indicators.get(earlier_index)?;
    let atr = positive(current.atr14?)?;
    let divisor = EMA_SLOPE_LOOKBACK as f64 * atr;
    Some((
        (current.ema12? - earlier.ema12?) / divisor,
        (current.ema144? - earlier.ema144?) / divisor,
        (current.ema596? - earlier.ema596?) / divisor,
    ))
}

/// 返回不含信号棒的固定长度历史切片，历史不足时失败关闭。
fn prior_candles(candles: &[Candle], index: usize, length: usize) -> Option<&[Candle]> {
    let start = index.checked_sub(length)?;
    candles.get(start..index)
}

fn maximum_high(candles: &[Candle]) -> Option<f64> {
    if candles.is_empty() || candles.iter().any(|candle| !candle.high.is_finite()) {
        return None;
    }
    Some(
        candles
            .iter()
            .map(|candle| candle.high)
            .fold(f64::NEG_INFINITY, f64::max),
    )
}

fn minimum_low(candles: &[Candle]) -> Option<f64> {
    if candles.is_empty() || candles.iter().any(|candle| !candle.low.is_finite()) {
        return None;
    }
    Some(
        candles
            .iter()
            .map(|candle| candle.low)
            .fold(f64::INFINITY, f64::min),
    )
}

/// 使用 Pine nearest-rank 的 50 分位数，而非偶数样本两中值平均。
fn nearest_rank_median(values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (0.5 * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted.get(rank - 1).copied()
}

fn valid_tick(tick_size: f64) -> Option<()> {
    (tick_size.is_finite() && tick_size > 0.0).then_some(())
}

fn positive(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}

/// 风险距离始终向外取整为至少一个 tick，防止保护价被舍入到信号收盘内侧。
fn outward_distance_ticks(distance: f64, tick_size: f64) -> Option<i64> {
    (distance.is_finite() && distance > 0.0).then(|| ((distance / tick_size).ceil() as i64).max(1))
}

/// 目标向内取整为整数 tick，与 Pine 的固定风险倍数订单保持一致。
fn scaled_target_ticks(risk_ticks: i64, multiple: f64) -> i64 {
    ((risk_ticks as f64 * multiple).floor() as i64).max(1)
}

fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: f64 = 0.25;

    fn base_candle(index: usize) -> Candle {
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open: 99.5,
            high: 100.5,
            low: 99.0,
            close: 100.0,
            volume: 10.0,
        }
    }

    fn base_point() -> IndicatorPoint {
        IndicatorPoint {
            rsi14: Some(60.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema596: Some(100.0),
            ema696: Some(100.0),
            atr14: Some(2.0),
            ..IndicatorPoint::default()
        }
    }

    fn base_fixture(length: usize) -> (Vec<Candle>, IndicatorSeries) {
        (
            (0..length).map(base_candle).collect(),
            IndicatorSeries {
                points: (0..length).map(|_| base_point()).collect(),
            },
        )
    }

    fn configure_enr(
        candles: &mut [Candle],
        indicators: &mut IndicatorSeries,
        index: usize,
        anchor_offset: usize,
    ) {
        let anchor_index = index - anchor_offset;
        candles[anchor_index].high = 105.0;
        candles[anchor_index].volume = 80.0;
        indicators.points[anchor_index].rsi14 = Some(70.0);
        candles[index] = Candle {
            timestamp_ms: index as i64 * 900_000,
            open: 104.0,
            high: 104.5,
            low: 99.5,
            close: 100.5,
            volume: 100.0,
        };
        indicators.points[index].volume_event = true;
        indicators.points[index].filtered_volume_ratio = Some(4.2);
        indicators.points[index].rsi14 = Some(60.0);
        indicators.points[index].atr14 = Some(2.0);
        indicators.points[index].bollinger_upper = Some(104.0);
    }

    #[test]
    fn enr_includes_both_anchor_window_boundaries() {
        for anchor_offset in [ENR_ANCHOR_MIN_DISTANCE, ENR_ANCHOR_MAX_DISTANCE] {
            let index = 100;
            let (mut candles, mut indicators) = base_fixture(102);
            configure_enr(&mut candles, &mut indicators, index, anchor_offset);

            let result = evaluate_effort_no_result_short(&candles, &indicators, index, TICK)
                .expect("20 and 80 are inclusive anchor offsets");

            assert_eq!(result.stop_price, 105.25);
            assert_eq!(result.activation_ticks, 19);
            assert_eq!(result.target_ticks, 28);
            assert_eq!(result.display_volume_ratio, Some(4.2));
        }
    }

    #[test]
    fn enr_does_not_fallback_after_highest_anchor_fails() {
        let index = 100;
        let (mut candles, mut indicators) = base_fixture(102);
        configure_enr(&mut candles, &mut indicators, index, 30);
        candles[index - 30].high = 104.75;
        indicators.points[index - 30].rsi14 = Some(75.0);
        candles[index - 20].high = 105.0;
        candles[index - 20].volume = 80.0;
        indicators.points[index - 20].rsi14 = Some(69.0);

        assert!(evaluate_effort_no_result_short(&candles, &indicators, index, TICK).is_none());
    }

    fn configure_blr(candles: &mut [Candle], indicators: &mut IndicatorSeries, index: usize) {
        candles[index - 10].high = 110.0;
        candles[index - 20].low = 90.0;
        candles[index] = Candle {
            timestamp_ms: index as i64 * 900_000,
            open: 95.5,
            high: 97.0,
            low: 91.0,
            close: 96.0,
            volume: 30.0,
        };
        indicators.points[index].volume_event = true;
        indicators.points[index].filtered_volume_ratio = Some(3.1);
        indicators.points[index].rsi14 = Some(45.0);
        indicators.points[index - 1].rsi14 = Some(40.0);
        indicators.points[index - 2].rsi14 = Some(35.0);
        indicators.points[index].macd_histogram = Some(-1.0);
        indicators.points[index - 1].macd_histogram = Some(-2.0);
        indicators.points[index - 2].macd_histogram = Some(-3.0);
        indicators.points[index].bollinger_lower = Some(93.0);
        indicators.points[index].bollinger_middle = Some(103.0);
    }

    #[test]
    fn blr_freezes_prior_range_stop_and_middle_target() {
        let index = 60;
        let (mut candles, mut indicators) = base_fixture(62);
        configure_blr(&mut candles, &mut indicators, index);
        candles[index + 1] = Candle {
            high: 999.0,
            low: 1.0,
            ..base_candle(index + 1)
        };

        let result = evaluate_bollinger_lower_reclaim_long(&candles, &indicators, index, TICK)
            .expect("future bar must not affect a valid BLR signal");

        assert_eq!(result.stop_price, 90.75);
        assert_eq!(result.target_price, 103.0);
        assert_eq!(result.display_volume_ratio, Some(3.1));
    }

    #[test]
    fn blr_requires_three_contracting_negative_macd_bars() {
        let index = 60;
        let (mut candles, mut indicators) = base_fixture(62);
        configure_blr(&mut candles, &mut indicators, index);
        indicators.points[index - 1].macd_histogram = Some(-0.5);

        assert!(
            evaluate_bollinger_lower_reclaim_long(&candles, &indicators, index, TICK).is_none()
        );
    }

    fn configure_ema596_d2(candles: &mut [Candle], indicators: &mut IndicatorSeries, index: usize) {
        candles[index - 4] = Candle {
            timestamp_ms: (index - 4) as i64 * 900_000,
            open: 99.0,
            high: 99.75,
            low: 98.5,
            close: 99.5,
            volume: 10.0,
        };
        candles[index - 3] = Candle {
            timestamp_ms: (index - 3) as i64 * 900_000,
            open: 99.0,
            high: 100.0,
            low: 98.75,
            close: 99.5,
            volume: 10.0,
        };
        candles[index - 2] = Candle {
            timestamp_ms: (index - 2) as i64 * 900_000,
            open: 99.8,
            high: 100.75,
            low: 99.5,
            close: 100.25,
            volume: 10.0,
        };
        candles[index - 1] = Candle {
            timestamp_ms: (index - 1) as i64 * 900_000,
            open: 100.1,
            high: 101.0,
            low: 100.0,
            close: 100.5,
            volume: 10.0,
        };
        candles[index] = Candle {
            timestamp_ms: index as i64 * 900_000,
            open: 100.5,
            high: 103.0,
            low: 100.25,
            close: 102.5,
            volume: 30.0,
        };
        indicators.points[index].weekly_volume_ready = true;
        indicators.points[index].weekly_volume_p90 = Some(25.0);
    }

    #[test]
    fn ema596_d2_requires_prior_acceptance_and_freezes_two_r_target() {
        let index = 90;
        let (mut candles, mut indicators) = base_fixture(92);
        configure_ema596_d2(&mut candles, &mut indicators, index);

        let result = evaluate_ema596_reclaim_departure_long(&candles, &indicators, index, TICK)
            .expect("reclaim at t-2 is accepted before the D2 signal");

        assert_eq!(result.stop_price, 98.25);
        assert_eq!(result.target_price, 111.0);
        assert_eq!(result.display_volume_ratio, 3.0);
    }

    #[test]
    fn ema596_d2_rejects_same_bar_reclaim() {
        let index = 90;
        let (mut candles, mut indicators) = base_fixture(92);
        configure_ema596_d2(&mut candles, &mut indicators, index);
        candles[index - 2].close = 99.5;
        candles[index - 1] = Candle {
            timestamp_ms: (index - 1) as i64 * 900_000,
            open: 99.0,
            high: 100.0,
            low: 98.75,
            close: 99.5,
            volume: 10.0,
        };

        assert!(
            evaluate_ema596_reclaim_departure_long(&candles, &indicators, index, TICK).is_none()
        );
    }

    #[test]
    fn ema596_d2_rejects_joint_prior_slow_bear_slope() {
        let index = 90;
        let (mut candles, mut indicators) = base_fixture(92);
        configure_ema596_d2(&mut candles, &mut indicators, index);
        indicators.points[index - 4].ema144 = Some(100.2);
        indicators.points[index - 4].ema596 = Some(100.02);

        assert!(
            evaluate_ema596_reclaim_departure_long(&candles, &indicators, index, TICK).is_none()
        );
    }

    #[test]
    fn ema596_v11_rejects_overbought_departure_after_v10_structure_passes() {
        let index = 90;
        let (mut candles, mut indicators) = base_fixture(92);
        configure_ema596_d2(&mut candles, &mut indicators, index);
        indicators.points[index - 4].ema12 = Some(99.5);
        indicators.points[index - 4].ema144 = Some(99.7);
        indicators.points[index - 4].ema596 = Some(99.8);
        indicators.points[index - 2].ema12 = Some(99.8);
        indicators.points[index - 2].ema144 = Some(99.8);
        indicators.points[index - 2].ema596 = Some(99.9);
        indicators.points[index - 1].ema12 = Some(100.0);
        indicators.points[index - 1].ema144 = Some(99.9);
        indicators.points[index - 1].ema596 = Some(100.0);
        indicators.points[index].rsi14 = Some(72.0);

        assert!(
            evaluate_ema596_reclaim_departure_long_v10(&candles, &indicators, index, TICK)
                .is_some()
        );
        assert!(
            evaluate_ema596_reclaim_departure_long_v11(&candles, &indicators, index, TICK)
                .is_none()
        );
    }
}
