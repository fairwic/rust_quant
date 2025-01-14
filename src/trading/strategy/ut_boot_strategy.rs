use serde::{Deserialize, Serialize};
use ta::indicators::ExponentialMovingAverage;
use ta::Next;

use crate::trading::indicator::atr::ATR;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common::{run_test, SignalResult, TradeRecord};

#[derive(Deserialize, Serialize, Debug)]
pub struct UtBootStrategy {
    pub key_value: f64,
    pub atr_period: usize,
    pub heikin_ashi: bool,
}



impl UtBootStrategy {
    pub fn get_trade_signal(
        candles_5m: &[CandlesEntity],
        key_value: f64,
        atr_period: usize,
        heikin_ashi: bool,
    ) -> SignalResult {
        let mut atr = ATR::new(atr_period); // 初始化ATR指标
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
                // warn!(
                //     "pre_ema_value:{},prev_xatr_trailing_stop{}",
                //     prev_ema_value, prev_xatr_trailing_stop
                // );

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
                    // warn!("pre_price:{}", pre_close);
                }
                // println!("time:{:?},current_atr:{},prev_xatr_trailing_stop:{},ema:{},current_price:{}\
                // ,xatr_trailing_stop:{},above:{},below:{},pre_ema_value:{},prev_xatr_trailing_stop{}",
                //   time_util::mill_time_to_datetime_shanghai(candle.ts),  current_atr,prev_xatr_trailing_stop,ema_value, current_price, xatr_trailing_stop, above, below,prev_ema_value,prev_xatr_trailing_stop);

                // 记录开仓价格或卖出价格
                price = current_price;
                //记录时间
                ts = candle.ts;
            }
        }
        SignalResult {
            should_buy,
            should_sell,
            price,
            ts,
        } // 返回是否应该开仓和是否应该卖出的信号, 开仓或卖出价格
    }

    /// 运行回测
    pub async fn run_test(
        candles_5m: &Vec<CandlesEntity>,
        fib_levels: &Vec<f64>,
        max_loss_percent: f64,
        is_need_fibonacci_profit: bool,
        is_open_long: bool,
        is_open_short: bool,
        ut_boot_strategy: UtBootStrategy,
        is_jude_trade_time: bool,
    ) -> (f64, f64, usize, Vec<TradeRecord>) {
        let min_data_length = ut_boot_strategy.atr_period + 1;
        let res = run_test(
            |candles| {
                Self::get_trade_signal(
                    candles,
                    ut_boot_strategy.key_value,
                    ut_boot_strategy.atr_period,
                    ut_boot_strategy.heikin_ashi,
                )
            },
            candles_5m,
            fib_levels,
            max_loss_percent,
            min_data_length,
            is_need_fibonacci_profit,
            is_open_long,
            is_open_short,
            is_jude_trade_time,
        );
        // println!("res= {:#?}", json!(res));
        res
    }
}
