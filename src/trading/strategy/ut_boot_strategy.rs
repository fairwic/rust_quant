use serde::{Deserialize, Serialize};

use ta::indicators::{AverageTrueRange, ExponentialMovingAverage};
use ta::Next;
use tracing::{error, info};
use crate::time_util;
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

#[derive(Debug, Deserialize, Serialize)]
pub struct TradeRecord {
    pub open_position_time: String,
    pub close_position_time: String,
    pub open_price: f64,
    pub close_price: f64,
    pub profit_loss: f64,
    pub quantity: f64,
    pub full_close: bool,
    pub close_type: String,
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

    pub async fn run_test(
        candles_5m: &Vec<CandlesEntity>,
        fib_levels: &Vec<f64>,
        key_value: f64,
        atr_period: usize,
        heikin_ashi: bool
    ) -> (f64, f64, usize, Vec<TradeRecord>) {
        let initial_funds = 100.0; // 初始资金
        let mut funds = initial_funds; // 当前资金
        let mut position: f64 = 0.0; // 当前持仓量,显式指定为 f64 类型
        let mut wins = 0; // 赢的次数
        let mut losses = 0; // 输的次数
        let mut open_trades = 0; // 开仓次数
        let mut entry_price = 0.0; // 记录每次开仓时的价格
        let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发
        let mut trade_completed = true; // 交易完成标志
        let max_loss_percent = 0.02; // 最大损失百分比设置为2%

        let mut trade_records = Vec::new(); // 记录所有交易的详细信息
       let mut entry_time = String::new(); // 记录每次开仓的时间点
        let mut initial_quantity = 0.0; // 记录每次开仓的数量

        for (i, candle) in candles_5m.iter().enumerate() {
            let signal = UtBootStrategy::get_trade_signal(&candles_5m[..=i], key_value, atr_period, heikin_ashi);
            //记录信号到数据库中
            // 添加日志记录
            info!("Time: {:?}, funds: {}, Price: {}, Buy: {}, Sell: {}, key_value: {}, atr_period: {}",
                time_util::mill_time_to_datetime_shanghai(candle.ts), funds, signal.price, signal.should_buy, signal.should_sell, key_value, atr_period);

            if signal.should_buy && position.abs() < f64::EPSILON && trade_completed {
                position = funds / signal.price;
                initial_quantity = position; // 记录开仓数量
                entry_price = signal.price; // 记录开仓价格
                entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap(); // 记录开仓时间
                funds = 0.0;
                open_trades += 1;
                fib_triggered = [false; 6]; // 重置斐波那契触发标记
                trade_completed = false; // 标记交易未完成
                info!("Buy at time: {:?}, price: {}, position: {}, funds after buy: {}",
                    entry_time, signal.price, position, funds);
            } else if (signal.should_sell || signal.price < entry_price * (1.0 - max_loss_percent)) && position > 0.0 {
                let exit_time = time_util::mill_time_to_datetime(candle.ts).unwrap(); // 记录平仓时间
                let profit_loss = position * signal.price - initial_funds; // 计算盈利或损失
                funds += position * signal.price; // 累加当前平仓收益
                trade_records.push(TradeRecord {
                    open_position_time: entry_time.clone(),
                    close_position_time: exit_time,
                    open_price: entry_price,
                    close_price: signal.price,
                    profit_loss,
                    quantity: initial_quantity,
                    full_close: true,
                    close_type: if signal.should_sell { "止盈".to_string() } else { "止损".to_string() },
                });
                position = 0.0;
                trade_completed = true; // 标记交易完成
                info!("Sell (close long) at time: {:?}, price: {}, funds after sell: {}, profit/loss: {}",
                    entry_time, signal.price, funds, profit_loss);
                if profit_loss > 0.0 {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position > 0.0 {
                // // 斐波那契止盈逻辑
                // let mut remaining_position = position;
                // for (idx, &level) in fib_levels.iter().enumerate() {
                //     let fib_price = entry_price * (1.0 + level); // 计算斐波那契目标价格
                //     if signal.price >= fib_price && !fib_triggered[idx] {
                //         let sell_amount = remaining_position * 0.1; // 按仓位的10%
                //         if sell_amount < 1e-8 { // 防止非常小的数值
                //             continue;
                //         }
                //         funds += sell_amount * signal.price; // 累加当前平仓收益
                //         remaining_position -= sell_amount;
                //         fib_triggered[idx] = true; // 记录该斐波那契级别已经触发
                //         info!("Fibonacci profit taking at level: {:?}, time: {}, price: {}, sell amount: {}, remaining position: {}, funds after profit taking: {}",
                //             time_util::mill_time_to_datetime_shanghai(candle.ts), level, signal.price, sell_amount, remaining_position, funds);
                //         // 如果剩余仓位为零，更新win或loss
                //         if remaining_position <= 1e-8 {
                //             let exit_time = time_util::mill_time_to_datetime(candle.ts).unwrap(); // 记录平仓时间
                //             let profit_loss = funds - initial_funds; // 计算盈利或损失
                //             trade_records.push(TradeRecord {
                //                 open_position_time: entry_time.clone(),
                //                 close_position_time: exit_time,
                //                 open_price: entry_price,
                //                 close_price: signal.price,
                //                 profit_loss,
                //                 quantity: initial_quantity,
                //                 full_close: true,
                //                 close_type: "止盈".to_string(),
                //             });
                //             position = 0.0;
                //             trade_completed = true; // 标记交易完成
                //             if funds > initial_funds {
                //                 wins += 1;
                //             } else {
                //                 losses += 1;
                //             }
                //             break;
                //         }
                //     }
                // }
                // // 更新持仓
                // position = remaining_position;
            }
        }

        if position > 0.0 {
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
                    error!("Failed to parse price: {}", e);
                    0.0
                });
                let exit_time = time_util::mill_time_to_datetime(last_candle.ts).unwrap(); // 记录平仓时间
                let profit_loss = position * last_price - initial_funds; // 计算盈利或损失
                funds += position * last_price; // 累加当前平仓收益
                trade_records.push(TradeRecord {
                    open_position_time: entry_time.clone(),
                    close_position_time: exit_time,
                    open_price: entry_price,
                    close_price: last_price,
                    profit_loss,
                    quantity: initial_quantity,
                    full_close: true,
                    close_type: "止盈".to_string(),
                });
                position = 0.0;
                trade_completed = true; // 标记交易完成
                info!("Final sell at price: {}, funds after final sell: {}, profit/loss: {}",
                    last_price, funds, profit_loss);
                if profit_loss > 0.0 {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
        }

        let win_rate = if wins + losses > 0 {
            wins as f64 / (wins + losses) as f64
        } else {
            0.0
        }; // 计算胜率

        info!("Final Win rate: {}", win_rate);
        (funds, win_rate, open_trades, trade_records) // 返回最终资金,胜率和开仓次数及交易记录
    }
}
