use crate::CandleItem;
use rust_quant_indicators::trend::ema_indicator::EmaIndicator;
use rust_quant_indicators::trend::vegas::{
    EmaSignalValue, IndicatorCombine, KlineHammerSignalValue, VegasIndicatorSignalValue,
};
use std::time::Instant;
use ta::Next; // ⭐ 需要导入Next trait才能使用next方法
use tracing::{info, warn};

/// 计算多个EMA值
pub fn calculate_ema(data: &CandleItem, ema_indicator: &mut EmaIndicator) -> EmaSignalValue {
    let prev_signal_value = ema_indicator.last_signal_value;
    let mut ema_signal_value = EmaSignalValue::default();
    ema_signal_value.ema1_value = ema_indicator.ema1_indicator.next(data.c());
    ema_signal_value.ema2_value = ema_indicator.ema2_indicator.next(data.c());
    ema_signal_value.ema3_value = ema_indicator.ema3_indicator.next(data.c());
    ema_signal_value.ema4_value = ema_indicator.ema4_indicator.next(data.c());
    ema_signal_value.ema5_value = ema_indicator.ema5_indicator.next(data.c());
    ema_signal_value.ema6_value = ema_indicator.ema6_indicator.next(data.c());
    ema_signal_value.ema7_value = ema_indicator.ema7_indicator.next(data.c());

    // 判断是否多头排列
    ema_signal_value.is_long_trend = ema_signal_value.ema1_value > ema_signal_value.ema2_value
        && ema_signal_value.ema2_value > ema_signal_value.ema3_value
        && ema_signal_value.ema3_value > ema_signal_value.ema4_value;

    // 判断是否空头排列
    ema_signal_value.is_short_trend = ema_signal_value.ema1_value < ema_signal_value.ema2_value
        && ema_signal_value.ema2_value < ema_signal_value.ema3_value
        && ema_signal_value.ema3_value < ema_signal_value.ema4_value;

    if let Some(prev) = prev_signal_value {
        let (is_golden_cross, is_death_cross) = detect_ema_crosses(&ema_signal_value, &prev);
        ema_signal_value.is_golden_cross = is_golden_cross;
        ema_signal_value.is_death_cross = is_death_cross;
    }

    ema_indicator.last_signal_value = Some(ema_signal_value);

    ema_signal_value
}

fn detect_ema_crosses(current: &EmaSignalValue, previous: &EmaSignalValue) -> (bool, bool) {
    let mut is_golden_cross =
        previous.ema1_value < previous.ema2_value && current.ema1_value > current.ema2_value;
    let mut is_death_cross =
        previous.ema1_value > previous.ema2_value && current.ema1_value < current.ema2_value;

    if !is_death_cross {
        let ema1_below = current.ema1_value < current.ema2_value
            && current.ema2_value < current.ema3_value
            && current.ema3_value < current.ema4_value;
        let ema1_cross_ema4 =
            previous.ema1_value >= previous.ema4_value && current.ema1_value < current.ema4_value;
        if ema1_below && ema1_cross_ema4 {
            is_death_cross = true;
        }
    }

    if !is_golden_cross {
        let ema1_above = current.ema1_value > current.ema2_value
            && current.ema2_value > current.ema3_value
            && current.ema3_value > current.ema4_value;
        let ema1_cross_ema4 =
            previous.ema1_value <= previous.ema4_value && current.ema1_value > current.ema4_value;
        if ema1_above && ema1_cross_ema4 {
            is_golden_cross = true;
        }
    }

    (is_golden_cross, is_death_cross)
}

/// 获取多个指标值
pub fn get_multi_indicator_values(
    indicator_combine: &mut IndicatorCombine,
    data_item: &CandleItem,
) -> VegasIndicatorSignalValue {
    let start = Instant::now();
    let mut vegas_indicator_signal_value = VegasIndicatorSignalValue::default();

    // 缓存频繁使用的值
    let close_price = data_item.c();
    let volume = data_item.v();

    // 计算EMA
    let ema_start = Instant::now();
    if let Some(ema_indicator) = &mut indicator_combine.ema_indicator {
        vegas_indicator_signal_value.ema_values = calculate_ema(data_item, ema_indicator);
    }
    if ema_start.elapsed().as_millis() > 10 {
        warn!(duration_ms = ema_start.elapsed().as_millis(), "计算EMA");
    }

    // 计算Volume
    let volume_start = Instant::now();
    if let Some(volume_indicator) = &mut indicator_combine.volume_indicator {
        vegas_indicator_signal_value.volume_value.volume_value = volume;
        vegas_indicator_signal_value.volume_value.volume_ratio = volume_indicator.next(volume);
        vegas_indicator_signal_value
            .volume_value
            .is_increasing_than_pre = volume_indicator.is_increasing_than_pre();
        vegas_indicator_signal_value
            .volume_value
            .is_decreasing_than_pre = volume_indicator.is_decreasing_than_pre();
    }
    if volume_start.elapsed().as_millis() > 10 {
        warn!(
            duration_ms = volume_start.elapsed().as_millis(),
            "计算Volume"
        );
    }

    // 计算RSI
    let rsi_start = Instant::now();
    if let Some(rsi_indicator) = &mut indicator_combine.rsi_indicator {
        vegas_indicator_signal_value.rsi_value.rsi_value = rsi_indicator.next(close_price);
    }
    if rsi_start.elapsed().as_millis() > 10 {
        warn!(duration_ms = rsi_start.elapsed().as_millis(), "计算RSI");
    }

    // 计算Bollinger
    let bb_start = Instant::now();
    if let Some(bollinger_indicator) = &mut indicator_combine.bollinger_indicator {
        let bollinger_value = bollinger_indicator.next(data_item);
        vegas_indicator_signal_value.bollinger_value.upper = bollinger_value.upper;
        vegas_indicator_signal_value.bollinger_value.lower = bollinger_value.lower;
        vegas_indicator_signal_value.bollinger_value.middle = bollinger_value.average;
        vegas_indicator_signal_value
            .bollinger_value
            .consecutive_touch_times = bollinger_value.consecutive_touch_times;
    }
    if bb_start.elapsed().as_millis() > 10 {
        warn!(
            duration_ms = bb_start.elapsed().as_millis(),
            "计算Bollinger"
        );
    }

    // 计算吞没形态
    let eng_start = Instant::now();
    if let Some(engulfing_indicator) = &mut indicator_combine.engulfing_indicator {
        let engulfing_value = engulfing_indicator.next(data_item);
        vegas_indicator_signal_value.engulfing_value.is_engulfing = engulfing_value.is_engulfing;
        vegas_indicator_signal_value.engulfing_value.body_ratio = engulfing_value.body_ratio;
    }
    if eng_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = eng_start.elapsed().as_millis(),
            "计算吞没形态"
        );
    }

    // 计算锤子形态
    let hammer_start = Instant::now();
    if let Some(kline_hammer_indicator) = &mut indicator_combine.kline_hammer_indicator {
        let kline_hammer_value = kline_hammer_indicator.next(data_item);
        vegas_indicator_signal_value.kline_hammer_value = KlineHammerSignalValue {
            is_hammer: kline_hammer_value.is_hammer,
            is_hanging_man: kline_hammer_value.is_hanging_man,
            down_shadow_ratio: kline_hammer_value.down_shadow_ratio,
            up_shadow_ratio: kline_hammer_value.up_shadow_ratio,
            body_ratio: kline_hammer_value.body_ratio,
            is_long_signal: false,
            is_short_signal: false,
        };
    }
    if hammer_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = hammer_start.elapsed().as_millis(),
            "计算锤子形态"
        );
    }

    // 腿部识别
    let leg_start = Instant::now();
    if let Some(leg_detection_indicator) = &mut indicator_combine.leg_detection_indicator {
        vegas_indicator_signal_value.leg_detection_value = leg_detection_indicator.next(data_item);
    }
    if leg_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = leg_start.elapsed().as_millis(),
            "计算腿部识别"
        );
    }

    vegas_indicator_signal_value
}
