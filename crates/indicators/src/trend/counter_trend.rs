use crate::trend::signal_weight::{SignalCondition, SignalType};
use rust_quant_common::CandleItem;
use rust_quant_domain::SignalResult;

pub trait CounterTrendSignalResult {
    fn should_buy(&self) -> bool;
    fn should_sell(&self) -> bool;
    fn set_counter_trend_pullback_take_profit_price(&mut self, price: Option<f64>);
}

impl CounterTrendSignalResult for SignalResult {
    fn should_buy(&self) -> bool {
        self.should_buy.unwrap_or(false)
    }
    fn should_sell(&self) -> bool {
        self.should_sell.unwrap_or(false)
    }
    fn set_counter_trend_pullback_take_profit_price(&mut self, price: Option<f64>) {
        self.counter_trend_pullback_take_profit_price = price;
    }
}

/// 计算逆势回调止盈价格
///
/// 逻辑说明：
/// - 做多时（空头排列下逆势做多）：止盈价格 = 连续下跌K线起点最高价 * (1 - 回调比例)
/// - 做空时（多头排列下逆势做空）：止盈价格 = 连续上涨K线起点最低价 * (1 + 回调比例)
pub fn calculate_counter_trend_pullback_take_profit_price(
    data_items: &[CandleItem],
    signal_result: &mut impl CounterTrendSignalResult,
    _conditions: &[(SignalType, SignalCondition)],
    ema1_value: f64,
) {
    if data_items.len() < 3 {
        return;
    }

    let pullback_ratio = 0.32; // 默认回调比例0.30%

    // 做多时：找到连续下跌K线的起点最高价
    if signal_result.should_buy() && data_items.last().unwrap().o < ema1_value {
        if let Some(take_profit_price) =
            find_consecutive_down_candles_high(data_items, pullback_ratio)
        {
            signal_result.set_counter_trend_pullback_take_profit_price(Some(take_profit_price));
        }
    }

    // 做空时：找到连续上涨K线的起点最低价
    if signal_result.should_sell() && data_items.last().unwrap().o > ema1_value {
        if let Some(take_profit_price) = find_consecutive_up_candles_low(data_items, pullback_ratio)
        {
            signal_result.set_counter_trend_pullback_take_profit_price(Some(take_profit_price));
        }
    }
}

/// 找到连续下跌K线的回调止盈价格
///
/// 当前信号K线不要求是下跌K线，但从倒数第二根K线开始必须是连续下跌的
/// 计算方式：从连续下跌起点的最高价到信号K线（包含）之间的最低价，取30%回调位置
/// 止盈价格 = 最高 - (最高价 - 最低价) * 回调比例
fn find_consecutive_down_candles_high(
    data_items: &[CandleItem],
    pullback_ratio: f64,
) -> Option<f64> {
    if data_items.len() < 3 {
        return None;
    }
    let last_idx = data_items.len() - 1;
    let second_last_candle = &data_items[last_idx - 1];

    // 倒数第二根K线必须是下跌K线
    if second_last_candle.c >= second_last_candle.o {
        return None;
    }
    // 从倒数第二根K线开始记录连续下跌的起点
    let mut hightest = data_items.last().unwrap().h;
    let mut lowest = data_items.last().unwrap().l;
    // 从倒数第二根K线开始往前找连续下跌的K线
    for i in (0..last_idx).rev() {
        let candle = &data_items[i];
        // 判断是否为下跌K线（收盘价 < 开盘价）
        if candle.c < candle.o {
            if candle.h > hightest {
                hightest = candle.h;
            }
            if candle.l < lowest {
                lowest = candle.l;
            }
        } else {
            break;
        }
    }
    Some(hightest - (hightest - lowest) * pullback_ratio)
}

/// 找到连续上涨K线的回调止盈价格
///
/// 当前信号K线不要求是上涨K线，但从倒数第二根K线开始必须是连续上涨的
/// 计算方式：从连续上涨起点的最低价到信号K线（包含）之间的最高价，取30%回调位置
/// 止盈价格 = 最低价格+ (最高价 - 最低价) * 回调比例
fn find_consecutive_up_candles_low(data_items: &[CandleItem], pullback_ratio: f64) -> Option<f64> {
    if data_items.len() < 3 {
        return None;
    }
    let last_idx = data_items.len() - 1;
    let second_last_candle = &data_items[last_idx - 1];
    let mut lowest = data_items.last().unwrap().l;
    let mut hightest = data_items.last().unwrap().h;

    for i in (0..last_idx).rev() {
        let candle = &data_items[i];
        if candle.c > candle.o {
            if candle.l < lowest {
                lowest = candle.l;
            }
            if candle.h > hightest {
                hightest = candle.h;
            }
        } else {
            break;
        }
    }
    Some(lowest + (hightest - lowest) * pullback_ratio)
}
