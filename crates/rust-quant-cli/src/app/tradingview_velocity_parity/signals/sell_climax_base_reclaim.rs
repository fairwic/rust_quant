use super::super::model::{
    Candle, Direction, EntryIntent, ExitPolicy, IndicatorSeries, SignalFamily,
};
use super::util::round_down;

const SOURCE_MIN_DISTANCE: usize = 2;
const SOURCE_MAX_DISTANCE: usize = 6;
const BASE_MIN_BARS: usize = 2;
const BASE_MAX_BARS: usize = 5;
const SOURCE_BODY_RANGE_MIN: f64 = 0.70;
const BASE_RETEST_ATR: f64 = 0.25;
const TRIGGER_BODY_RANGE_MIN: f64 = 0.70;
const TRIGGER_BODY_ATR_MIN: f64 = 0.75;
const TRIGGER_STRUCTURE_LOOKBACK: usize = 4;
const TRIGGER_VOLUME_LOOKBACK: usize = 20;
const TRIGGER_VOLUME_MEDIAN_MULTIPLIER: f64 = 1.25;
const RSI_MIN: f64 = 35.0;
const RSI_MAX: f64 = 60.0;
const EMA576_SLOPE_LOOKBACK: usize = 12;
const MIN_SIGNAL_CLOSE_REWARD_RISK: f64 = 1.5;

/// 识别冻结的跨棒卖压衰竭形态，并只用信号收盘前的已完成 K 线构造订单意图。
pub(super) fn evaluate(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<EntryIntent> {
    if tick_size <= 0.0 || index < TRIGGER_VOLUME_LOOKBACK {
        return None;
    }
    let trigger = *candles.get(index)?;
    let trigger_point = indicators.get(index)?;

    // 先冻结最近完整量能事件；最近候选后续失败时禁止回退到更老事件。
    let source_index = (SOURCE_MIN_DISTANCE..=SOURCE_MAX_DISTANCE).find_map(|distance| {
        index.checked_sub(distance).filter(|candidate| {
            indicators
                .get(*candidate)
                .is_some_and(|point| point.volume_event)
        })
    })?;
    let source = candles[source_index];
    let source_point = indicators.get(source_index)?;
    let source_atr = source_point.atr14?;
    let source_lower = source_point.bollinger_lower?;
    let source_ema576 = source_point.ema596?;
    let source_range = source.range();
    let base = candles.get(source_index + 1..index)?;
    if !(BASE_MIN_BARS..=BASE_MAX_BARS).contains(&base.len())
        || source.close >= source.open
        || source_range <= 0.0
        || source.body() / source_range < SOURCE_BODY_RANGE_MIN
        || source.low >= source_lower
        || source.close >= source_ema576
        || base.iter().any(|candle| candle.close < source.close)
        || !base
            .iter()
            .any(|candle| candle.low <= source.low + BASE_RETEST_ATR * source_atr)
    {
        return None;
    }

    let atr = trigger_point.atr14?;
    let ema12 = trigger_point.ema12?;
    let ema144 = trigger_point.ema144?;
    let ema576 = trigger_point.ema596?;
    let ema676 = trigger_point.ema696?;
    let prior_ema576 = indicators
        .get(index.checked_sub(EMA576_SLOPE_LOOKBACK)?)?
        .ema596?;
    let rsi = trigger_point.rsi14?;
    let previous = indicators.get(index.checked_sub(1)?)?;
    let previous_rsi = previous.rsi14?;
    let macd = trigger_point.macd_histogram?;
    let previous_macd = previous.macd_histogram?;
    let trigger_range = trigger.range();
    let prior_high = candles
        .get(index.checked_sub(TRIGGER_STRUCTURE_LOOKBACK)?..index)?
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let median_volume = median(
        &candles[index - TRIGGER_VOLUME_LOOKBACK..index]
            .iter()
            .map(|candle| candle.volume)
            .collect::<Vec<_>>(),
    )?;
    let volume_ratio = trigger.volume / median_volume;

    if trigger.close <= trigger.open
        || trigger_range <= 0.0
        || trigger.body() / trigger_range < TRIGGER_BODY_RANGE_MIN
        || trigger.body() < TRIGGER_BODY_ATR_MIN * atr
        || trigger.close <= ema12
        || trigger.close <= ema576
        || trigger.close <= prior_high
        || median_volume <= 0.0
        || volume_ratio < TRIGGER_VOLUME_MEDIAN_MULTIPLIER
        || !(RSI_MIN..=RSI_MAX).contains(&rsi)
        || rsi <= previous_rsi
        || macd <= previous_macd
        || ema576 <= prior_ema576
        || trigger.close >= ema676
    {
        return None;
    }

    let structure_low = candles[source_index..=index]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let stop = round_down(structure_low - tick_size, tick_size);
    let target = round_down(ema676, tick_size);
    let risk = trigger.close - stop;
    let reward = target - trigger.close;
    if risk <= 0.0 || reward / risk < MIN_SIGNAL_CLOSE_REWARD_RISK {
        return None;
    }

    Some(EntryIntent {
        signal_index: index,
        signal_time_ms: trigger.timestamp_ms,
        direction: Direction::Long,
        families: vec![SignalFamily::SellClimaxBaseReclaimLong],
        signal_close: trigger.close,
        signal_atr: atr,
        stop_price: Some(stop),
        stop_ticks: None,
        target_price: Some(target),
        target_ticks: None,
        activation_ticks: None,
        exit_policy: ExitPolicy::Fixed,
        counter_trend: ema12 < ema144 && ema144 < ema676,
        signal_counter_trend_ema_age_bars_capped_600: None,
        counter_trend_structure_breakout_line: None,
        anchor_upthrust_target_consumption_ratio: None,
        active_parent_horizontal_anchor: None,
        strict_visual_range_length_bars: None,
        strict_visual_range_height: None,
        strict_visual_short_range_one_r_target: None,
        strict_visual_breakout_candle_extreme_stop: false,
        volume_ratio: Some(volume_ratio),
        rsi: Some(rsi),
        breakout_line: Some(prior_high),
    })
}

/// 计算偶数窗口的普通中位数，不把 nearest-rank 的下中位数混入量比定义。
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::model::IndicatorPoint;

    /// 构造只覆盖冻结规则所需字段的确定性形态，避免测试依赖指标预热细节。
    fn fixture() -> (Vec<Candle>, IndicatorSeries, usize) {
        let index = 30;
        let mut candles = (0..=index)
            .map(|bar| Candle {
                timestamp_ms: bar as i64 * 900_000,
                open: 99.0,
                high: 99.5,
                low: 98.6,
                close: 99.1,
                volume: 100.0,
            })
            .collect::<Vec<_>>();
        let source_index = index - 6;
        candles[source_index] = Candle {
            timestamp_ms: source_index as i64 * 900_000,
            open: 100.0,
            high: 100.0,
            low: 98.0,
            close: 98.5,
            volume: 300.0,
        };
        candles[source_index + 1].low = 98.4;
        candles[index] = Candle {
            timestamp_ms: index as i64 * 900_000,
            open: 99.0,
            high: 101.2,
            low: 98.9,
            close: 101.0,
            volume: 150.0,
        };

        let mut points = vec![IndicatorPoint::default(); candles.len()];
        points[source_index] = IndicatorPoint {
            volume_event: true,
            atr14: Some(2.0),
            bollinger_lower: Some(98.2),
            ema596: Some(99.0),
            ..IndicatorPoint::default()
        };
        points[index - EMA576_SLOPE_LOOKBACK].ema596 = Some(99.0);
        points[index - 1] = IndicatorPoint {
            rsi14: Some(45.0),
            macd_histogram: Some(0.1),
            ..IndicatorPoint::default()
        };
        points[index] = IndicatorPoint {
            rsi14: Some(50.0),
            ema12: Some(99.8),
            ema144: Some(100.0),
            ema596: Some(99.6),
            ema696: Some(106.0),
            atr14: Some(2.0),
            macd_histogram: Some(0.2),
            ..IndicatorPoint::default()
        };
        (candles, IndicatorSeries { points }, index)
    }

    #[test]
    fn canonical_sell_climax_base_reclaim_is_accepted() {
        let (candles, indicators, index) = fixture();
        let intent = evaluate(&candles, &indicators, index, 0.01).expect("形态应通过");
        assert_eq!(
            intent.families,
            vec![SignalFamily::SellClimaxBaseReclaimLong]
        );
        assert_eq!(intent.signal_index, index);
        assert!((intent.stop_price.expect("结构止损") - 97.99).abs() < 1e-9);
        assert_eq!(intent.target_price, Some(106.0));
    }

    #[test]
    fn base_close_below_source_close_invalidates_acceptance_failure() {
        let (mut candles, indicators, index) = fixture();
        candles[index - 3].close = 98.4;
        candles[index - 3].low = 98.3;
        assert!(evaluate(&candles, &indicators, index, 0.01).is_none());
    }

    #[test]
    fn nearest_invalid_volume_event_does_not_fall_back_to_older_source() {
        let (mut candles, mut indicators, index) = fixture();
        indicators.points[index - 2].volume_event = true;
        candles[index - 2].close = candles[index - 2].open + 0.1;
        assert!(evaluate(&candles, &indicators, index, 0.01).is_none());
    }
}
