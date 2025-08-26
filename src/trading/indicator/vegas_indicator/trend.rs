use super::config::EmaTouchTrendSignalConfig;
use super::signal::{EmaSignalValue, EmaTouchTrendSignalValue};
use crate::trading::indicator::is_big_kline::IsBigKLineIndicator;
use crate::CandleItem;

/// 检查EMA趋势
pub fn check_ema_touch_trend(
    data_items: &[CandleItem],
    ema_value: EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> EmaTouchTrendSignalValue {
    let mut ema_touch_trend_value = EmaTouchTrendSignalValue::default();
    let last_data_item = data_items.last().expect("数据不能为空");

    // 判断多头排列
    if is_bullish_trend(&ema_value) {
        ema_touch_trend_value.is_uptrend = true;
        check_bullish_signals(data_items, &ema_value, config, &mut ema_touch_trend_value);
    }
    // 判断空头排列
    else if is_bearish_trend(&ema_value) {
        ema_touch_trend_value.is_downtrend = true;
        check_bearish_signals(data_items, &ema_value, config, &mut ema_touch_trend_value);
    }

    ema_touch_trend_value
}

/// 判断是否为多头趋势
fn is_bullish_trend(ema_value: &EmaSignalValue) -> bool {
    ema_value.ema2_value > ema_value.ema3_value && ema_value.ema3_value > ema_value.ema4_value
}

/// 判断是否为空头趋势
fn is_bearish_trend(ema_value: &EmaSignalValue) -> bool {
    ema_value.ema1_value < ema_value.ema2_value
        && ema_value.ema2_value < ema_value.ema3_value
        && ema_value.ema3_value < ema_value.ema4_value
}

/// 检查多头信号
fn check_bullish_signals(
    data_items: &[CandleItem],
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
    trend_value: &mut EmaTouchTrendSignalValue,
) {
    let last_item = data_items.last().expect("数据不能为空");

    // 检查EMA2触碰信号
    if check_ema2_touch_signal(last_item, ema_value, config) {
        trend_value.is_long_signal = true;
        return;
    }

    // 检查EMA4/EMA5触碰信号
    if check_ema45_touch_signal_bullish(last_item, ema_value, config) {
        trend_value.is_in_uptrend_touch_ema4_ema5_nums += 1;

        if last_item.l() <= ema_value.ema4_value {
            trend_value.is_in_uptrend_touch_ema4 = true;
        } else {
            trend_value.is_in_uptrend_touch_ema5 = true;
        }
        trend_value.is_long_signal = true;
    }

    // 检查EMA7触碰信号（短期多头vs长期空头）
    if check_ema7_touch_signal_bullish(last_item, ema_value, config) {
        trend_value.is_touch_ema7_nums += 1;
        trend_value.is_touch_ema7 = true;
        trend_value.is_short_signal = true;
        trend_value.is_long_signal = false;
    }
}

/// 检查空头信号
fn check_bearish_signals(
    data_items: &[CandleItem],
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
    trend_value: &mut EmaTouchTrendSignalValue,
) {
    let last_item = data_items.last().expect("数据不能为空");

    // 检查EMA2触碰信号
    if check_ema2_touch_signal_bearish(last_item, ema_value, config) {
        trend_value.is_short_signal = true;
        trend_value.is_touch_ema2 = true;
        return;
    }

    // 检查EMA4/EMA5触碰信号
    if check_ema45_touch_signal_bearish(last_item, ema_value, config) {
        trend_value.is_touch_ema4_ema5_nums += 1;

        if last_item.h() * config.price_with_ema_high_ratio >= ema_value.ema4_value {
            trend_value.is_touch_ema4 = true;
        } else {
            trend_value.is_touch_ema5 = true;
        }
        trend_value.is_short_signal = true;
    }

    // 检查EMA7触碰信号（短期空头vs长期多头）
    if check_ema7_touch_signal_bearish(last_item, ema_value, config) {
        trend_value.is_touch_ema7_nums += 1;
        trend_value.is_touch_ema7 = true;
        trend_value.is_long_signal = true;
        trend_value.is_short_signal = false;
    }
}

/// 检查EMA2触碰信号（多头）
fn check_ema2_touch_signal(
    last_item: &CandleItem,
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> bool {
    ema_value.ema1_value > ema_value.ema2_value
        && last_item.l() <= ema_value.ema2_value * config.price_with_ema_high_ratio
        && ema_value.ema1_value > ema_value.ema2_value * config.ema1_with_ema2_ratio
        && last_item.o() > ema_value.ema2_value
        && last_item.c() > ema_value.ema2_value
}

/// 检查EMA2触碰信号（空头）
fn check_ema2_touch_signal_bearish(
    last_item: &CandleItem,
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> bool {
    last_item.h() >= ema_value.ema2_value * config.price_with_ema_low_ratio
        && ema_value.ema2_value > ema_value.ema1_value * config.ema1_with_ema2_ratio
        && last_item.o() < ema_value.ema2_value
        && last_item.c() < ema_value.ema2_value
}

/// 检查EMA4/EMA5触碰信号（多头）
fn check_ema45_touch_signal_bullish(
    last_item: &CandleItem,
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> bool {
    let condition_1 = last_item.o() > ema_value.ema4_value;
    let condition_2 = last_item.l() <= ema_value.ema4_value * config.ema3_with_ema4_ratio
        || last_item.l() <= ema_value.ema5_value * config.ema4_with_ema5_ratio;
    let condition_3 = ema_value.ema4_value * config.ema3_with_ema4_ratio <= ema_value.ema3_value
        || ema_value.ema4_value * config.ema4_with_ema5_ratio <= ema_value.ema3_value;

    condition_1 && condition_2 && condition_3
}

/// 检查EMA4/EMA5触碰信号（空头）
fn check_ema45_touch_signal_bearish(
    last_item: &CandleItem,
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> bool {
    let condition_1 = last_item.o() < ema_value.ema4_value;
    let condition_2 = (last_item.h() * config.price_with_ema_high_ratio >= ema_value.ema4_value)
        || (last_item.h() * config.price_with_ema_high_ratio >= ema_value.ema5_value);
    let condition_3 = (ema_value.ema3_value * config.ema3_with_ema4_ratio < ema_value.ema4_value)
        || (ema_value.ema3_value * config.ema3_with_ema4_ratio < ema_value.ema5_value);

    condition_1 && condition_2 && condition_3
}

/// 检查EMA7触碰信号（多头环境中的空头信号）
fn check_ema7_touch_signal_bullish(
    last_item: &CandleItem,
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> bool {
    // 短期多头趋势 && 长期空头趋势
    let short_term_bullish = ema_value.ema1_value > ema_value.ema2_value
        && ema_value.ema2_value > ema_value.ema3_value
        && ema_value.ema3_value > ema_value.ema4_value;

    let long_term_bearish = ema_value.ema4_value < ema_value.ema5_value
        && ema_value.ema5_value < ema_value.ema6_value
        && ema_value.ema6_value < ema_value.ema7_value;

    if short_term_bullish && long_term_bearish {
        last_item.h() >= ema_value.ema7_value
            && ema_value.ema5_value * config.ema5_with_ema7_ratio > ema_value.ema7_value
    } else {
        false
    }
}

/// 检查EMA7触碰信号（空头环境中的多头信号）
fn check_ema7_touch_signal_bearish(
    last_item: &CandleItem,
    ema_value: &EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> bool {
    // 短期空头趋势 && 长期多头趋势
    let short_term_bearish = ema_value.ema1_value < ema_value.ema2_value
        && ema_value.ema2_value < ema_value.ema3_value
        && ema_value.ema3_value < ema_value.ema4_value;

    let long_term_bullish = ema_value.ema4_value > ema_value.ema5_value
        && ema_value.ema5_value > ema_value.ema6_value
        && ema_value.ema6_value > ema_value.ema7_value;

    if short_term_bearish && long_term_bullish {
        last_item.l() <= ema_value.ema7_value
            && ema_value.ema7_value * config.ema5_with_ema7_ratio < ema_value.ema5_value
    } else {
        false
    }
}

/// 检查突破条件
pub fn check_breakthrough_conditions(
    data_items: &[CandleItem],
    ema_value: EmaSignalValue,
    breakthrough_threshold: f64,
) -> (bool, bool) {
    if data_items.len() < 2 {
        return (false, false);
    }

    let current_price = data_items.last().expect("数据不能为空").c;
    let prev_price = data_items[data_items.len() - 2].c;

    // 向上突破条件：当前价格突破ema2上轨，且前一根K线价格低于EMA2
    let price_above = current_price > ema_value.ema2_value * (1.0 + breakthrough_threshold)
        && prev_price < ema_value.ema2_value;

    // 向下突破条件：当前价格突破ema2下轨，且前一根K线价格高于EMA2
    let price_below = (current_price < ema_value.ema1_value
        && current_price < ema_value.ema2_value * (1.0 - breakthrough_threshold)
        && prev_price > ema_value.ema2_value)
        || (current_price < ema_value.ema5_value * (1.0 - breakthrough_threshold)
            && prev_price > ema_value.ema5_value);

    (price_above, price_below)
}

/// 检查突破确认
pub fn check_breakthrough_confirmation(data_items: &[CandleItem], is_upward: bool) -> bool {
    // 实现突破确认逻辑
    // 可以检查:
    // 1. 突破后的持续性
    // 2. 回测支撑/阻力的表现
    // 3. 成交量配合
    true // 临时返回值
}

/// 计算动态回调幅度
pub fn calculate_dynamic_pullback_threshold(_data_items: &[CandleItem]) -> f64 {
    // 实现动态回调幅度计算逻辑
    // 可以考虑:
    // 1. 价格波动性
    // 2. 均线角度
    // 3. 成交量变化
    0.005 // 临时返回值
}

/// 获取有效的RSI
pub fn get_valid_rsi(data_items: &[CandleItem], rsi_value: f64, ema_value: EmaSignalValue) -> f64 {
    // 如果当前k线价格波动比较大，且k线的实体部分占比大于80%,
    // 表明当前k线为大阳线或者大阴线，则不使用rsi指标,因为大概率趋势还会继续
    let is_big_k_line =
        IsBigKLineIndicator::new(70.0).is_big_k_line(data_items.last().expect("数据不能为空"));

    if is_big_k_line {
        50.0
    } else {
        rsi_value
    }
}
