use serde::{Deserialize, Serialize};
use ta::indicators::{AverageTrueRange, ExponentialMovingAverage};
use ta::Next;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::indicator::kdj_simple_indicator::KdjCandle;

#[derive(Deserialize, Serialize, Debug)]
pub struct UtBootStrategy {
    pub key_value: f64,
    pub atr_period: usize,
    pub heikin_ashi: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SignalResult {
    pub should_buy: bool,
    pub should_sell: bool,
    pub price: f64,
}

impl UtBootStrategy {
    pub fn get_trade_signal(candles_5m: &[CandlesEntity], key_value: f64, atr_period: usize, heikin_ashi: bool) -> SignalResult {
        let mut atr = AverageTrueRange::new(atr_period).unwrap(); // 初始化ATR指标
        let mut ema = ExponentialMovingAverage::new(1).unwrap(); // 初始化EMA指标
        let mut xatr_trailing_stop = 0.0; // 初始化xATRTrailingStop变量
        let mut prev_ema_value = 0.0; // 用于保存前一个EMA值

        let mut should_buy = false;
        let mut should_sell = false;
        let mut price = 0.0;

        // 确保至少有 atr_period + 1 根 K 线
        if candles_5m.len() >= atr_period + 1 {
            // 从满足 atr_period 要求的最新 K 线开始处理
            let start_index = candles_5m.len() - (atr_period + 1);
            for (i, candle) in candles_5m[start_index..].iter().enumerate() {
                let current_price = if heikin_ashi {
                    // 如果使用平均K线,则计算平均K线的收盘价
                    let open = candle.o.parse::<f64>().unwrap_or(0.0);
                    let high = candle.h.parse::<f64>().unwrap_or(0.0);
                    let low = candle.l.parse::<f64>().unwrap_or(0.0);
                    let close = candle.c.parse::<f64>().unwrap_or(0.0);
                    (open + high + low + close) / 4.0
                } else {
                    candle.c.parse::<f64>().unwrap_or(0.0)
                };

                let high_price = candle.h.parse::<f64>().unwrap_or(0.0);
                let low_price = candle.l.parse::<f64>().unwrap_or(0.0);

                let prev_xatr_trailing_stop = xatr_trailing_stop;

                let n_loss = key_value * atr.next(&KdjCandle { high: high_price, low: low_price, close: current_price });

                xatr_trailing_stop = if i == 0 {
                    current_price
                } else if current_price > prev_xatr_trailing_stop && candles_5m[start_index + i - 1].c.parse::<f64>().unwrap_or(0.0) > prev_xatr_trailing_stop {
                    let new_stop = current_price - n_loss;
                    prev_xatr_trailing_stop.max(new_stop)
                } else if current_price < prev_xatr_trailing_stop && candles_5m[start_index + i - 1].c.parse::<f64>().unwrap_or(0.0) < prev_xatr_trailing_stop {
                    let new_stop = current_price + n_loss;
                    prev_xatr_trailing_stop.min(new_stop)
                } else if current_price > prev_xatr_trailing_stop {
                    current_price - n_loss
                } else {
                    current_price + n_loss
                };

                let ema_value = ema.next(current_price);
                let above = ema_value > xatr_trailing_stop && prev_ema_value <= prev_xatr_trailing_stop;
                let below = ema_value < xatr_trailing_stop && prev_ema_value >= prev_xatr_trailing_stop;
                prev_ema_value = ema_value; // 保存当前EMA值为下一次迭代的前一个EMA值

                should_buy = current_price > xatr_trailing_stop && above;
                should_sell = current_price < xatr_trailing_stop && below;

                // 记录开仓价格或卖出价格
                price = current_price;
            }
        }
        SignalResult { should_buy, should_sell, price } // 返回是否应该开仓和是否应该卖出的信号, 开仓或卖出价格
    }
}