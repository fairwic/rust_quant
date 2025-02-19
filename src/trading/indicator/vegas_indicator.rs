use std::fmt::Display;
use std::sync::Arc;

use ta::{Close, DataItem, High, Low, Next};
use ta::indicators::ExponentialMovingAverage;

use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common::{BackTestResult, SignalResult, StrategyCommonTrait};
use crate::trading::strategy::strategy_common;

pub struct VegasIndicator {
    ema1_length: usize,
    ema2_length: usize,
    ema3_length: usize,
}
impl Display for VegasIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vegas indicator :ema0:{} ema2:{} ema3:{}", self.ema1_length, self.ema2_length, self.ema3_length)
    }
}
impl VegasIndicator {
    pub fn new(ema1: usize, ema2: usize, ema3: usize) -> Self {
        Self {
            ema1_length: ema1,
            ema2_length: ema2,
            ema3_length: ema3,
        }
    }
    pub fn calculate(&mut self, data: &[DataItem]) -> (f64, f64, f64) {
        let mut ema1 = ExponentialMovingAverage::new(self.ema1_length).unwrap();
        let mut ema2 = ExponentialMovingAverage::new(self.ema2_length).unwrap();
        let mut ema3 = ExponentialMovingAverage::new(self.ema3_length).unwrap();
        let length = data.len();
        let mut ema1_value = 0.00;
        let mut ema2_value = 0.00;
        let mut ema3_value = 0.00;

        for (i, datum) in data.into_iter().enumerate() {
            //判断一下如果数据到了满足每个ema周期开始的时候才调用next方法
            ema1_value = ema1.next(datum.close());
            ema2_value = ema2.next(datum.close());
            ema3_value = ema3.next(datum.close());
            // if length - i + 1 <= self.ema1_length {
            // }
            // if length - i + 1 <= self.ema2_length {
            // }
            // if length - i + 1 <= self.ema3_length {
            // }
        }
        (ema1_value, ema2_value, ema3_value)
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
            price: 0.0,
            ts: 0,
            single_detail:None
        };
        //组装数据
        let data_items = self.convert_to_data_items(&data.to_vec());
        let (ema1_value, ema2_value, ema3_value) = self.calculate(&data_items);

        signal_result.price = data.last().unwrap().c.parse::<f64>().unwrap();
        signal_result.ts = data.last().unwrap().ts;
        //判断信号值
        if ema1_value > ema2_value && ema2_value > ema3_value && signal_result.price > ema2_value {
            //金叉
            signal_result.should_buy = true;
            signal_result.single_detail = Some(format!(" 金叉，ema1:{}, ema2:{}, ema3:{}", ema1_value, ema2_value, ema3_value));
        }
        if ema1_value < ema2_value && ema2_value < ema3_value && signal_result.price < ema2_value {
            signal_result.should_sell = true;
            signal_result.single_detail = Some(format!("死叉，ema1:{}, ema2:{}, ema3:{}", ema1_value, ema2_value, ema3_value));
        }

        println!("ema1:{}, ema2:{}, ema3:{}", ema1_value, ema2_value, ema3_value);
        signal_result
    }

    /// Runs the backtest asynchronously.
    pub async fn run_test(
        &mut self,
        data: &Arc<Vec<CandlesEntity>>,
        fib_levels: &Vec<f64>,
        max_loss_percent: f64,
        is_need_fibonacci_profit: bool,
        is_open_long: bool,
        is_open_short: bool,
        is_jude_trade_time: bool,
    ) -> BackTestResult {
        // Determine the minimum data length required for the backtest
        let min_data_length = self.get_min_data_length();
        // Execute the external run_test function with appropriate parameters
        let res = strategy_common::run_test(
            |candles| {
                // Generate trade signals using the strategy
                self.get_trade_signal(candles)
            },
            &data, // Extract candle data from generic data D
            fib_levels,
            max_loss_percent,
            min_data_length,
            is_need_fibonacci_profit,
            is_open_long,
            is_open_short,
            is_jude_trade_time,
        ); // Await the asynchronous run_test function

        res // Return the result of the backtest
    }
    pub fn get_min_data_length(&mut self) -> usize {
        self.ema1_length.max(self.ema2_length).max(self.ema3_length)
    }
}
