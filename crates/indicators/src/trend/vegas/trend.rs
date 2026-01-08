use super::config::EmaTouchTrendSignalConfig;
use super::signal::{EmaSignalValue, EmaTouchTrendSignalValue};
use rust_quant_common::utils::time;
use rust_quant_common::CandleItem;

/// 检查EMA趋势
pub fn check_ema_touch_trend(
    data_items: &[CandleItem],
    ema_value: EmaSignalValue,
    config: &EmaTouchTrendSignalConfig,
) -> EmaTouchTrendSignalValue {
    let mut ema_touch_trend_value = EmaTouchTrendSignalValue::default();
    let last_data_item = data_items.last().expect("数据不能为空");

    // if data_items.last().unwrap().ts == 1762128000000 {
    //     println!("last_data_item: {:?}", data_items.last().unwrap());
    // }
    // 判断多头排列
    if is_bullish_trend(&ema_value) {
        ema_touch_trend_value.is_uptrend = true;
        check_bullish_signals(data_items, &ema_value, config, &mut ema_touch_trend_value);
    } else if is_bearish_trend(&ema_value) {
        // 判断空头排列
        ema_touch_trend_value.is_downtrend = true;
        check_bearish_signals(data_items, &ema_value, config, &mut ema_touch_trend_value);
        // if data_items.last().unwrap().ts == 1762128000000 {
        //     println!("ema_touch_trend_value: {:?}", ema_touch_trend_value);
        // }
    }

    // 判断ema刚好进入死叉(比如当前k线或者前面1～2根k线触发了死叉)，不能开多
    // 判断ema刚好进入金叉，不能开空
    // 使用当前的ema_value来检测交叉
    let (has_recent_golden_cross, has_recent_death_cross) = check_recent_ema_crossover(&ema_value);

    // 如果刚发生死叉，禁止开多
    if has_recent_death_cross && ema_touch_trend_value.is_long_signal {
        // println!("刚发生死叉，禁止开多");
        // println!("ema_value: {:?}", ema_value);
        // println!("ema_touch_trend_value: {:?}", ema_touch_trend_value);
        // println!(
        //     "time: {:?}",
        //     time::mill_time_to_datetime_shanghai(last_data_item.ts())
        // );
        ema_touch_trend_value.is_long_signal = false;
    }

    // 如果刚发生金叉，禁止开空
    if has_recent_golden_cross && ema_touch_trend_value.is_short_signal {
        // println!("刚发生金叉，禁止开空");
        // println!("ema_value: {:?}", ema_value);
        // println!("ema_touch_trend_value: {:?}", ema_touch_trend_value);
        // println!(
        //     "time: {:?}",
        //     time::mill_time_to_datetime_shanghai(last_data_item.ts())
        // );
        ema_touch_trend_value.is_short_signal = false;
    }

    ema_touch_trend_value
}

/// 检查近期EMA交叉（当前或前1～2根K线是否发生了金叉或死叉）
/// 返回: (是否发生金叉, 是否发生死叉)
///
/// 交叉结果在构建 `EmaSignalValue` 时计算并缓存，这里直接读取
fn check_recent_ema_crossover(current_ema: &EmaSignalValue) -> (bool, bool) {
    (current_ema.is_golden_cross, current_ema.is_death_cross)
}

/// 判断是否为多头趋势
fn is_bullish_trend(ema_value: &EmaSignalValue) -> bool {
    ema_value.ema1_value < ema_value.ema2_value && ema_value.ema2_value > ema_value.ema3_value
}

/// 判断是否为空头趋势
fn is_bearish_trend(ema_value: &EmaSignalValue) -> bool {
    ema_value.ema1_value < ema_value.ema2_value && ema_value.ema2_value < ema_value.ema3_value
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
    //如果k线是大实体阳线，则做多
    if last_item.body_ratio() > 0.8 && last_item.o() < last_item.c() {
        trend_value.is_long_signal = true;
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

    // // 检查EMA7触碰信号（短期空头vs长期多头）
    // if check_ema7_touch_signal_bearish(last_item, ema_value, config) {
    //     trend_value.is_touch_ema7_nums += 1;
    //     trend_value.is_touch_ema7 = true;
    //     trend_value.is_long_signal = true;
    //     trend_value.is_short_signal = false;
    // }
    //如果k线是大实体阴线，则做空
    if data_items.last().unwrap().ts == 1763049600000 {
        println!("last_item: {:?}", last_item);
        println!("body_ratio: {:?}", last_item.body_ratio());
        println!("o: {:?}", last_item.o());
        println!("c: {:?}", last_item.c());
    }
    if last_item.body_ratio() > 0.8 && last_item.o() > last_item.c() {
        trend_value.is_short_signal = true;
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
/// 返回 None 表示检测到极端行情（大利空/利多消息），应跳过后续交易信号判断
/// 返回 Some(rsi) 为正常 RSI 值
pub fn get_valid_rsi(
    data_items: &[CandleItem],
    rsi_value: f64,
    _ema_value: EmaSignalValue,
) -> Option<f64> {
    // 如果当前k线价格波动比较大，且k线的实体部分占比大于80%,
    // 表明当前k线为大阳线或者大阴线，则不使用rsi指标,因为大概率趋势还会继续
    if let Some(last_candle) = data_items.last() {
        let body = (last_candle.c() - last_candle.o()).abs();
        let total = last_candle.h() - last_candle.l();
        let body_ratio = if total > 0.0 { body / total } else { 0.0 };

        // RSI<30 且前一根K线跌幅>5% 且当前K线是阴线时，判断为大利空消息
        // 跳过后续交易信号判断，直接返回 None
        if rsi_value < 30.0 && data_items.len() >= 2 {
            let prev_candle = &data_items[data_items.len() - 2];
            if prev_candle.h() > 0.0 {
                let drop_ratio = (prev_candle.c() - prev_candle.h()) / prev_candle.h();
                let is_bearish = last_candle.c() < last_candle.o(); // 阴线
                if drop_ratio < -0.05 && is_bearish {
                    return None; // 极端行情，跳过交易
                }
            }
        }

        // RSI>70 且前一根K线涨幅>5% 且当前K线是阳线时，判断为大利多消息
        // 跳过后续交易信号判断，直接返回 None（不做空）
        if rsi_value > 70.0 && data_items.len() >= 2 {
            let prev_candle = &data_items[data_items.len() - 2];
            if prev_candle.l() > 0.0 {
                // 涨幅计算: (prev_close - prev_low) / prev_low
                let rise_ratio = (prev_candle.c() - prev_candle.l()) / prev_candle.l();
                let is_bullish = last_candle.c() > last_candle.o(); // 阳线
                if rise_ratio > 0.05 && is_bullish {
                    return None; // 极端行情，跳过交易
                }
            }
        }

        if body_ratio > 0.8 {
            Some(50.0) // 大阳线/大阴线，不使用RSI（返回中性值）
        } else {
            Some(rsi_value)
        }
    } else {
        Some(rsi_value)
    }
}
