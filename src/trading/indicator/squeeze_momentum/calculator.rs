use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use ta::{
    indicators::{BollingerBands, SimpleMovingAverage, TrueRange},
    Close, DataItem, High, Low, Next,
};
use tracing::info;

use crate::trading::{indicator::squeeze_momentum::service::calculate_linreg, strategy::strategy_common::BasicRiskStrategyConfig};
use crate::trading::indicator::squeeze_momentum::squeeze_config::{
    MomentumColor, SqueezeConfig, SqueezeResult, SqueezeState,
};
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common::{BackTestResult, SignalResult, TradeRecord};
use crate::trading::strategy::strategy_common;

pub struct SqueezeCalculator {
    config: SqueezeConfig,
    bb: BollingerBands,
    ma: SimpleMovingAverage,
    tr: TrueRange,
    range_ma: SimpleMovingAverage,
}

impl SqueezeCalculator {
    pub fn new(config: SqueezeConfig) -> Self {
        println!("config new:{:?}", config);
        let squeeze = Self {
            //这里bb传入的是kc的系数
            bb: BollingerBands::new(config.bb_length, config.kc_multi).unwrap(),
            ma: SimpleMovingAverage::new(config.kc_length).unwrap(),
            tr: TrueRange::new(),
            range_ma: SimpleMovingAverage::new(config.kc_length).unwrap(),
            config,
        };
        squeeze
    }

    pub fn determine_momentum_color(&self, val: f64, prev_val: Option<f64>) -> MomentumColor {
        if val > 0.0 {
            if let Some(prev) = prev_val {
                if val > prev {
                    MomentumColor::Lime
                } else {
                    MomentumColor::Green
                }
            } else {
                MomentumColor::Green
            }
        } else {
            if let Some(prev) = prev_val {
                if val < prev {
                    MomentumColor::Red
                } else {
                    MomentumColor::Maroon
                }
            } else {
                MomentumColor::Maroon
            }
        }
    }

    pub fn calculate_momentum(&self, closes: &[f64], highs: &[f64], lows: &[f64], period_kc_ma: f64, more_close: &[f64]) -> Result<Vec<f64>> {
        let period_highest = highs
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap();
        let period_lowest = lows
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap();

        let period_avg_hl = (period_highest + period_lowest) / 2.0;
        // // 使用 SimpleMovingAverage 来计算 kc_ma
        // let mut kc_ma = SimpleMovingAverage::new(self.config.kc_length)?;
        //
        // // 逐步计算 kc_ma 的值
        // for &close in closes.iter() {
        //     kc_ma.next(close);  // 逐步更新移动平均
        // }
        // // 获取最新的 kc_ma 值（即最后一个 close 值计算出的 SMA）
        // let period_kc_ma = kc_ma.next(closes.last().unwrap_or(&0.0).clone()); // 使用最后一个 `close` 值来计算
        //
        let period_avg_final = (period_avg_hl + period_kc_ma) / 2.0;
        let x = calculate_linreg(&closes, self.config.kc_length, 0).unwrap();
        // println!("X:{}", x);
        // let len = closes.len();
        // 创建一个 momentum_source 的副本
        // let mut momentum_source = closes.to_vec(); // 创建 `closes` 的副本
        // 修改 momentum_source 中的最后一个值
        // if let Some(last_value) = momentum_source.last_mut() {
        //     *last_value -= period_avg_final; // 将最后一个值减去 period_avg_final
        // }
        let momentum_source: Vec<f64> = more_close
            .iter()
            .map(|&close| close - period_avg_final)
            .collect();

        // 创建一个新的 Vec 来存储每个数据点的线性回归值
        let mut linreg_values: Vec<f64> = Vec::with_capacity(momentum_source.len());
        // 逐个计算 momentum_source 中每个数据的线性回归值
        for i in 0..momentum_source.len() {
            let start = if i > self.config.kc_length { i - self.config.kc_length + 1 } else { 0 };  // 计算线性回归的起始索引
            let end = i + 1;  // 线性回归的结束索引是当前元素的索引
            let subset = &momentum_source[start..end];
            // 计算当前子集的线性回归值
            // println!("subset:{:?}", subset);
            let linreg_value = calculate_linreg(&subset, self.config.kc_length, 0).unwrap_or(0.00);
            linreg_values.push(linreg_value);  //将计算得到的线性回归值加入结果向量
        }
        Ok(linreg_values)
    }

    pub fn calculate(&mut self, data: &[DataItem]) -> Result<SqueezeResult> {
        if data.len() < self.config.kc_length {
            return Err(anyhow::anyhow!("Insufficient data points"));
        }

        let mut closes = Vec::with_capacity(self.config.kc_length);
        let mut highs = Vec::with_capacity(self.config.kc_length);
        let mut lows = Vec::with_capacity(self.config.kc_length);

        let more_close_length = self.config.kc_length * 2 - 1;

        let mut more_closes = Vec::with_capacity(more_close_length);

        let mut last_bb = None;

        let mut ma = 0.0;
        let mut range_ma = 0.0;

        //计算布林带
        let window = &data[data.len() - self.config.bb_length..];
        // info!("bb windows length {:?}", window.len());
        for item in window {
            last_bb = Some(self.bb.next(item));
        }
        let window = &data[data.len() - more_close_length..];
        for item in window {
            more_closes.push(item.close());
        }

        //计算kc
        let window = &data[data.len() - (self.config.kc_length)..];
        // info!("kc windows length {:?}", window.len());
        for item in window {
            closes.push(item.close());
            highs.push(item.high());
            lows.push(item.low());

            ma = self.ma.next(item);
            let tr_val = self.tr.next(item);
            range_ma = self.range_ma.next(
                &DataItem::builder()
                    .close(tr_val)
                    .open(tr_val)
                    .high(tr_val)
                    .low(tr_val)
                    .volume(0.0)
                    .build()?,
            );
        }
        let bb_val = last_bb.ok_or_else(|| anyhow::anyhow!("Failed to calculate BB"))?;
        let momentum = self.calculate_momentum(&closes, &highs, &lows, ma, &more_closes)?;

        let upper_kc = ma + range_ma * self.config.kc_multi;
        let lower_kc = ma - range_ma * self.config.kc_multi;

        let squeeze_state = if bb_val.lower > lower_kc && bb_val.upper < upper_kc {
            SqueezeState::SqueezeOn
        } else if bb_val.lower < lower_kc && bb_val.upper > upper_kc {
            SqueezeState::SqueezeOff
        } else {
            SqueezeState::NoSqueeze
        };
        let momentum_color = self.determine_momentum_color(momentum.last().unwrap().clone(), momentum.get(momentum.len() - 2).copied());
        Ok(SqueezeResult {
            timestamp: 0, // 需要外部设置
            close: *closes.last().unwrap(),
            upper_bb: bb_val.upper,
            lower_bb: bb_val.lower,
            upper_kc,
            lower_kc,
            momentum,
            momentum_color,
            squeeze_state,
        })
    }

    pub fn convert_to_data_items(&self, prices: &Vec<CandlesEntity>) -> Vec<DataItem> {
        prices.iter().map(|candle| {
            DataItem::builder().open(candle.o.parse().unwrap()).high(candle.h.parse().unwrap()).low(candle.l.parse().unwrap()).close(candle.c.parse().unwrap()).volume(0.0).build().unwrap()
        }).collect()
    }

    pub fn get_trade_signal(&mut self, data: &[CandlesEntity]) -> SignalResult {
        let mut signal_result = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            ts: 0,
            single_value: None,
            single_result: None,
        };
        //组装数据
        let data_items = self.convert_to_data_items(&data.to_vec());
        let result_squeeze = self.calculate(&data_items);
        if result_squeeze.is_err() {
            return signal_result
        };
        if let Ok(res) = result_squeeze {
            signal_result.open_price = res.close;
            signal_result.ts = res.timestamp;
            match res.momentum_color {
                MomentumColor::Lime => {
                    signal_result.should_buy = true;
                }
                MomentumColor::Green => {}
                MomentumColor::Red => {
                    signal_result.should_sell = true;
                }
                MomentumColor::Maroon => {}
            }
        };
        signal_result
    }

    // /// Runs the backtest asynchronously.
    // pub async fn run_test(
    //     &mut self,
    //     data: &Arc<Vec<CandlesEntity>>,
    //     fib_levels: &Vec<f64>,
    //     max_loss_percent: f64,
    //     is_need_fibonacci_profit: bool,
    //     is_open_long: bool,
    //     is_open_short: bool,
    //     is_jude_trade_time: bool,
    // ) -> BackTestResult {
    //     // Determine the minimum data length required for the backtest
    //     // let min_data_length = self.get_min_data_length();
    //     // // Execute the external run_test function with appropriate parameters
    //     // let res = strategy_common::run_test(
    //     //     |candles| {
    //     //         // Generate trade signals using the strategy
    //     //         self.get_trade_signal(candles)
    //     //     },
    //     //     &data, // Extract candle data from generic data D
    //     //     fib_levels,
    //     //     TradingStrategyConfig::default(),
    //     //     min_data_length,
    //     //     is_open_long,
    //     //     is_open_short,
    //     //     is_jude_trade_time,
    //     // ); // Await the asynchronous run_test function

    //     // res // Return the result of the backtest
    // }
    pub fn get_min_data_length(&mut self) -> usize {
        self.config.bb_length.max(self.config.kc_length) * 2
    }
}
