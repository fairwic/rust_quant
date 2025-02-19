use std::cmp::{max, min};
use std::fmt::{Display, Formatter};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ta::indicators::ExponentialMovingAverage;
use ta::Next;
use tracing::warn;
use crate::trading::indicator::atr::ATR;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common::{BackTestResult, run_test, SignalResult, TradeRecord};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UtBootStrategy {
    pub key_value: f64,
    pub ema_period: usize,
    pub atr_period: usize,
    pub heikin_ashi: bool,
}
impl Display for UtBootStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "key_value:{},atr_period:{},ema:{},heikin_ashi:{}", self.key_value, self.atr_period, self.ema_period, self.heikin_ashi)
    }
}



impl UtBootStrategy {
    pub fn new(key_value: f64, ema_period: usize, atr_period: usize, is_heikin_ashi: bool) -> Self {
        UtBootStrategy {
            key_value,
            ema_period: ema_period,
            atr_period,
            heikin_ashi: is_heikin_ashi,
        }
    }

    //获取最小k线条数
    pub fn get_min_single_length(&self) -> usize {
        10 * self.ema_period.max(self.atr_period)
    }
    pub fn get_volatility_ratio(candles_5m: &[CandlesEntity], volatility_period: usize) -> f64 {
        // 新增1：波动率自适应系数（需在循环外初始化）
        let volatility_ratio = 1.0;
        if candles_5m.len() >= volatility_period { // 计算周期波动率
            let returns = candles_5m.iter().map(|c| (c.c.parse::<f64>().unwrap_or(0.0) / c.o.parse::<f64>().unwrap_or(1.0)).ln()).collect::<Vec<_>>();
            let stdev = returns.iter().fold(0.0, |acc, &x| acc + x.powi(2)).sqrt() / (returns.len() as f64).sqrt();
            return 1.0 + stdev * 3.0 // 波动越大系数越高
        };
        volatility_ratio
    }


    pub fn get_trade_signal(&self, candles_5m: &[CandlesEntity]) -> SignalResult {
        let mut atr = ATR::new(self.atr_period).unwrap(); // 初始化ATR指标
        let mut ema = ExponentialMovingAverage::new(self.ema_period).unwrap(); // 初始化EMA指标
        let mut xatr_trailing_stop = 0.0; // 初始化xATRTrailingStop变量
        let mut prev_ema_value = 0.0; // 用于保存前一个EMA值

        let mut should_buy = false;
        let mut should_sell = false;
        let mut price = 0.0;
        let mut ts: i64 = 0;

        let sing_result = SignalResult {
            should_buy,
            should_sell,
            price,
            ts,
            single_detail:None
        };
        // 增加三个新过滤条件
        let mut volume_ma = 0.0;
        let mut trend_confirmed = false;
        let mut volatility_ratio = 1.0;
        // println!("min_length:{}",min_length);
        //获取最小k线
        let min_single_len = self.get_min_single_length();
        if candles_5m.len() < min_single_len {
            warn!("数据不满足最小k线 candle_len:{},mim_length:{}",candles_5m.len(),min_single_len);
            return sing_result;
        }
        // 确保至少有 atr_period + 1 根 K 线
        if candles_5m.len() >= min_single_len {
            // 从满足 atr_period 要求的最新 K 线开始处理
            let start_index = candles_5m.len() - (min_single_len);
            let sub_candles = &candles_5m[start_index..];
            // println!("sub_candles{:#?}", sub_candles);
            for (i, candle) in sub_candles.into_iter().enumerate() {
                let current_price = if self.heikin_ashi {
                    // 如果使用平均K线,则计算平均K线的收盘价
                    let open = candle.o.parse::<f64>().unwrap_or(0.0);
                    let high = candle.h.parse::<f64>().unwrap_or(0.0);
                    let low = candle.l.parse::<f64>().unwrap_or(0.0);
                    let close = candle.c.parse::<f64>().unwrap_or(0.0);
                    (open + high + low + close) / 4.0
                } else {
                    candle.c.parse::<f64>().unwrap_or(0.0)
                };

                //成交量波动率
                // volatility_ratio=Self::get_volatility_ratio(&candles_5m[start_index+i-20..start_index+i],20);

                let high_price = candle.h.parse::<f64>().unwrap_or(0.0);
                let low_price = candle.l.parse::<f64>().unwrap_or(0.0);

                let prev_xatr_trailing_stop = xatr_trailing_stop;

                let current_atr = atr.next(high_price, low_price, current_price);
                // println!("current_atr:{}", current_atr);

                let n_loss = self.key_value * current_atr;
                // let current_atr = 0.00;
                // let n_loss = 0.00;
                xatr_trailing_stop = if i == 0 {
                    current_price
                } else if current_price > prev_xatr_trailing_stop && candles_5m[start_index + i - 1].c.parse::<f64>().unwrap_or(0.0) > prev_xatr_trailing_stop
                {
                    let new_stop = current_price - n_loss;
                    prev_xatr_trailing_stop.max(new_stop)
                } else if current_price < prev_xatr_trailing_stop && candles_5m[start_index + i - 1].c.parse::<f64>().unwrap_or(0.0) < prev_xatr_trailing_stop
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

                let above = ema_value > xatr_trailing_stop && prev_ema_value <= prev_xatr_trailing_stop;
                let below = ema_value < xatr_trailing_stop && prev_ema_value >= prev_xatr_trailing_stop;
                prev_ema_value = ema_value; // 保存当前EMA值为下一次迭代的前一个EMA值

                should_buy = current_price > xatr_trailing_stop && above;
                should_sell = current_price < xatr_trailing_stop && below;
                // println!("time:{:?},current_atr:{},prev_xatr_trailing_stop:{},ema:{},current_price:{}\
                // ,xatr_trailing_stop:{},above:{},below:{},pre_ema_value:{},prev_xatr_trailing_stop{}",
                //   time_util::mill_time_to_datetime_shanghai(candle.ts),  current_atr,prev_xatr_trailing_stop,ema_value, current_price, xatr_trailing_stop, above, below,prev_ema_value,prev_xatr_trailing_stop);
                // 记录开仓价格或卖出价格
                price = current_price;
                //记录时间
                ts = candle.ts;
            }
        }
        // 返回是否应该开仓和是否应该卖出的信号, 开仓或卖出价格
        SignalResult {
            should_buy,
            should_sell,
            price,
            ts,
            single_detail:None
        }
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
    ) -> BackTestResult {
        let min_data_length = ut_boot_strategy.get_min_single_length();
        let res = run_test(
            |candles| {
                ut_boot_strategy.get_trade_signal(candles)
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
