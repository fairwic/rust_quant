use crate::time_util;
use crate::trading::indicator::atr::ATR;
use crate::trading::indicator::kdj_simple_indicator::KdjCandle;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common::{SignalResult};
use clap::builder::Str;
use ndarray::{s, Array1};
use serde::{Deserialize, Serialize};
use std::env;
use ta::indicators::{AverageTrueRange, ExponentialMovingAverage, SimpleMovingAverage};
use ta::{DataItem, Next};
use tracing::{debug, error, info, warn};
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SqueezeState {
    SqueezeOn,  // 压缩状态开启
    SqueezeOff, // 压缩状态关闭
    NoSqueeze,  // 没有压缩状态
}

#[derive(Debug)]
pub struct SqueezeMomentumIndicator {
    pub upper_bb: f64,               // 布林带上轨
    pub lower_bb: f64,               // 布林带下轨
    pub upper_kc: f64,               // 凯尔特纳通道上轨
    pub lower_kc: f64,               // 凯尔特纳通道下轨
    pub squeeze_state: SqueezeState, // 压缩状态
    pub momentum: f64,               // 动量值
    pub basis: f64,                  // 布林带基础（中轨）
    pub dev: f64,                    // 布林带标准差
}
// 为 f64 实现 Close trait
// 使用 ta::indicators::SimpleMovingAverage 来计算 SMA
fn sma(data: &Array1<f64>, length: usize) -> Array1<f64> {
    let vec_data: Vec<DataItem> = data
        .iter()
        .map(|&x| {
            DataItem::builder()
                .close(x)
                .open(x)
                .high(x)
                .low(x)
                .volume(0.0)
                .build()
                .unwrap()
        })
        .collect();
    let mut sma_indicator = SimpleMovingAverage::new(length).unwrap();
    let result = vec_data
        .iter()
        .map(|x| sma_indicator.next(x))
        .collect::<Vec<f64>>();
    Array1::from(result)
}

// 计算标准差
fn stdev(data: &Array1<f64>, length: usize) -> Array1<f64> {
    let sma_data = sma(data, length);
    let mut result = Array1::zeros(data.len());
    for i in length..data.len() {
        let variance: f64 = data
            .slice(s![i - length..i])
            .iter()
            .map(|&x| (x - sma_data[i]).powi(2))
            .sum::<f64>()
            / length as f64;
        result[i] = variance.sqrt();
    }
    result
}

// 计算线性回归
fn linreg(data: &Array1<f64>, length: usize, offset: i32) -> Array1<f64> {
    let mut result = Array1::zeros(data.len());

    for i in length..data.len() {
        let window = data.slice(s![i - length..i]);
        let x: Vec<f64> = (0..length).map(|i| i as f64).collect();
        let y: Vec<f64> = window.iter().copied().collect();

        let sum_x: f64 = x.iter().sum();
        let sum_y: f64 = y.iter().sum();
        let sum_xx: f64 = x.iter().map(|&xi| xi * xi).sum();
        let sum_xy: f64 = x.iter().zip(y.iter()).map(|(&xi, &yi)| xi * yi).sum();

        let slope =
            (length as f64 * sum_xy - sum_x * sum_y) / (length as f64 * sum_xx - sum_x.powi(2));
        let intercept = (sum_y - slope * sum_x) / length as f64;

        // 应用偏移
        let regression_value = intercept + slope * (length as f64 - 1.0 - offset as f64);

        result[i] = regression_value;
    }

    result
}

// 判断是否满足 Squeeze 状态
fn check_squeeze(
    lower_bb: &Array1<f64>,
    upper_bb: &Array1<f64>,
    lower_kc: &Array1<f64>,
    upper_kc: &Array1<f64>,
) -> Vec<SqueezeState> {
    let mut squeeze_states = Vec::new();
    for i in 0..lower_bb.len() {
        if lower_bb[i] > lower_kc[i] && upper_bb[i] < upper_kc[i] {
            squeeze_states.push(SqueezeState::SqueezeOn); // 压缩状态开启
        } else if lower_bb[i] < lower_kc[i] && upper_bb[i] > upper_kc[i] {
            squeeze_states.push(SqueezeState::SqueezeOff); // 压缩状态关闭
        } else {
            squeeze_states.push(SqueezeState::NoSqueeze); // 无压缩状态
        }
    }
    squeeze_states
}

// 将 CandlesEntity 转换为 PriceType 类型的价格数据，并返回时间戳
fn to_price(candle: &CandlesEntity) -> (f64, f64, f64, f64, i64) {
    (
        candle.o.parse::<f64>().unwrap_or(0.0),
        candle.h.parse::<f64>().unwrap_or(0.0),
        candle.l.parse::<f64>().unwrap_or(0.0),
        candle.c.parse::<f64>().unwrap_or(0.0),
        candle.ts, // 返回时间戳
    )
}
impl SqueezeMomentumIndicator {
    pub fn get_trade_signal(
        candles_5m: &[CandlesEntity],
        key_value: f64,
        atr_period: usize,
        heikin_ashi: bool,
    ) -> SignalResult {
        let mut atr = ATR::new(atr_period).unwrap(); // 初始化ATR指标
        let mut ema = ExponentialMovingAverage::new(1).unwrap(); // 初始化EMA指标
        let mut xatr_trailing_stop = 0.0; // 初始化xATRTrailingStop变量
        let mut prev_ema_value = 0.0; // 用于保存前一个EMA值

        let mut should_buy = false;
        let mut should_sell = false;
        let mut price = 0.0;
        let mut ts: i64 = 0;

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

                let current_atr = atr.next(high_price, low_price, current_price);
                let n_loss = key_value * current_atr;

                // let current_atr = 0.00;
                // let n_loss = 0.00;

                xatr_trailing_stop = if i == 0 {
                    current_price
                } else if current_price > prev_xatr_trailing_stop
                    && candles_5m[start_index + i - 1]
                        .c
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        > prev_xatr_trailing_stop
                {
                    let new_stop = current_price - n_loss;
                    prev_xatr_trailing_stop.max(new_stop)
                } else if current_price < prev_xatr_trailing_stop
                    && candles_5m[start_index + i - 1]
                        .c
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        < prev_xatr_trailing_stop
                {
                    let new_stop = current_price + n_loss;
                    prev_xatr_trailing_stop.min(new_stop)
                } else if current_price > prev_xatr_trailing_stop {
                    current_price - n_loss
                } else {
                    current_price + n_loss
                };

                let ema_value = ema.next(current_price);
                warn!(
                    "pre_ema_value:{},prev_xatr_trailing_stop{}",
                    prev_ema_value, prev_xatr_trailing_stop
                );

                let above =
                    ema_value > xatr_trailing_stop && prev_ema_value <= prev_xatr_trailing_stop;
                let below =
                    ema_value < xatr_trailing_stop && prev_ema_value >= prev_xatr_trailing_stop;
                prev_ema_value = ema_value; // 保存当前EMA值为下一次迭代的前一个EMA值

                should_buy = current_price > xatr_trailing_stop && above;
                should_sell = current_price < xatr_trailing_stop && below;
                if i > 0 {
                    let pre_close = candles_5m[start_index + i - 1]
                        .c
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        .clone();
                    warn!("pre_price:{}", pre_close);
                }
                println!("time:{:?},current_atr:{},prev_xatr_trailing_stop:{},ema:{},current_price:{}\
                ,xatr_trailing_stop:{},above:{},below:{},pre_ema_value:{},prev_xatr_trailing_stop{}",
                         time_util::mill_time_to_datetime_shanghai(candle.ts), current_atr, prev_xatr_trailing_stop, ema_value, current_price, xatr_trailing_stop, above, below, prev_ema_value, prev_xatr_trailing_stop);

                // 记录开仓价格或卖出价格
                price = current_price;
                //记录时间
                ts = candle.ts;
            }
        }
        SignalResult {
            should_buy,
            should_sell,
            open_price: price,
            ts,
            tp_price: None,
            single_value: None,
            single_result: None,
        } // 返回是否应该开仓和是否应该卖出的信号, 开仓或卖出价格
    }
}
