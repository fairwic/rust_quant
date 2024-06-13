pub mod redis_operations;
pub mod support_resistance;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use log::{error, trace};
use serde::{Deserialize, Serialize};
use ta::indicators::{ExponentialMovingAverage, MovingAverageConvergenceDivergence, RelativeStrengthIndex};
use ta::Next;
use tokio;
use rbatis::RBatis;
use redis::aio::MultiplexedConnection;
use tracing::info;
use tracing::debug;
use crate::trading::model::market::candles::{CandlesEntity, CandlesModel};
use crate::trading::strategy::redis_operations::{Candle, RedisOperations};
use crate::trading::strategy::support_resistance::SupportResistance;
use crate::time_util;
use crate::trading::okx::trade::CandleData;
use std::collections::VecDeque;

// 枚举表示止损策略的选择
#[derive(Clone, Copy, Debug)]
pub enum StopLossStrategy {
    Amount(f64),
    Percent(f64),
}

#[derive(Clone, Copy, Debug)]
pub enum StrategyType {
    BreakoutUp,
    BreakoutDown,
    Macd,
    MacdWithEma,
    Boll,
}


pub struct Strategy {
    rb: RBatis,
    redis: MultiplexedConnection,
    rsi: RelativeStrengthIndex,
    ema_1h: ExponentialMovingAverage,
    macd: MovingAverageConvergenceDivergence,
}

impl Strategy {
    pub fn new(db: RBatis, redis: MultiplexedConnection) -> Self {
        Self {
            rb: db,
            redis,
            rsi: RelativeStrengthIndex::new(14).unwrap(),
            ema_1h: ExponentialMovingAverage::new(12 * 5).unwrap(),
            macd: MovingAverageConvergenceDivergence::new(12, 26, 9).unwrap(),
        }
    }


    fn calculate_ema(candles: &[CandlesEntity], period: usize) -> Vec<(i64, f64)> {
        let mut ema = ExponentialMovingAverage::new(period).unwrap();
        let mut ema_values = Vec::with_capacity(candles.len());
        for candle in candles {
            let price = candle.c.parse::<f64>().unwrap_or(0.0);
            let ema_value = ema.next(price);
            ema_values.push((candle.ts, ema_value));
        }
        ema_values
    }

    fn calculate_macd(candles: &[CandlesEntity], fast_period: usize, slow_period: usize, signal_period: usize) -> Vec<(i64, f64, f64)> {
        let mut macd = MovingAverageConvergenceDivergence::new(fast_period, slow_period, signal_period).unwrap();
        let mut macd_values = Vec::with_capacity(candles.len());
        for candle in candles {
            let price = candle.c.parse::<f64>().unwrap_or(0.0);
            let macd_value = macd.next(price);
            macd_values.push((candle.ts, macd_value.macd, macd_value.signal));
        }
        macd_values
    }

    fn apply_fibonacci_levels(
        position: &mut f64,
        funds: &mut f64,
        current_price: f64,
        entry_price: f64,
        fib_levels: &[f64],
        fib_triggered: &mut [bool],
    ) -> f64 {
        let mut remaining_position = *position;
        for (idx, &level) in fib_levels.iter().enumerate() {
            let target_price = entry_price * (1.0 + level);
            if current_price >= target_price && !fib_triggered[idx] {
                let sell_amount = remaining_position * 0.1;
                if sell_amount < 1e-8 {
                    continue;
                }
                *funds += sell_amount * current_price;
                remaining_position -= sell_amount;
                fib_triggered[idx] = true;
                info!("Fibonacci profit taking at level: {}, price: {}, sell amount: {}, remaining position: {}", level, current_price, sell_amount, remaining_position);
                if remaining_position <= 1e-8 {
                    break;
                }
            }
        }
        remaining_position
    }

    pub async fn short_term_strategy(&mut self, candles_5m: &[CandlesEntity], candles_1h: &[CandlesEntity]) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut entry_price = 0.0;
        let fib_levels: [f64; 6] = [0.0236, 0.0382, 0.05, 0.0618, 0.0786, 0.1];
        let mut fib_triggered = [false; 6];

        let ema_1h_values = Self::calculate_ema(candles_1h, 12);

        let prices_5m: Vec<f64> = candles_5m.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let low_prices_5m: Vec<f64> = candles_5m.iter().map(|c| c.l.parse::<f64>().unwrap_or(0.0)).collect();
        let high_prices_5m: Vec<f64> = candles_5m.iter().map(|c| c.h.parse::<f64>().unwrap_or(0.0)).collect();

        for i in 1..candles_5m.len() {
            let current_price = prices_5m[i];
            let prev_price = prices_5m[i - 1];
            let timestamp = time_util::mill_time_to_datetime_shanghai(candles_5m[i].ts).unwrap();
            let ema_1h_value = ema_1h_values.iter().filter(|&&(ts, _)| ts <= candles_5m[i].ts).last().map(|&(_, ema)| ema).unwrap_or(0.0);

            let bullish_engulfing = current_price > prev_price && low_prices_5m[i] < low_prices_5m[i - 1];
            let bearish_engulfing = current_price < prev_price && high_prices_5m[i] > high_prices_5m[i - 1];

            println!("timestamp: {}, bullish_engulfing: {}, bearish_engulfing: {}, current_price: {}, ema_1h_value: {}", timestamp, bullish_engulfing, bearish_engulfing, current_price, ema_1h_value);

            if bullish_engulfing && current_price > ema_1h_value && position == 0.0 {
                position = funds / current_price;
                entry_price = current_price;
                funds = 0.0;
                info!("Buy at time: {}, price: {}, position: {}", timestamp, current_price, position);
                fib_triggered = [false; 6];
            } else if bearish_engulfing && current_price < ema_1h_value && position == 0.0 {
                position = funds / current_price;
                entry_price = current_price;
                funds = 0.0;
                info!("Sell at time: {}, price: {}, position: {}", timestamp, current_price, position);
                fib_triggered = [false; 6];
            } else if position > 0.0 && (current_price < ema_1h_value || current_price < entry_price) {
                funds = position * current_price;
                position = 0.0;
                info!("Sell at time: {}, price: {}, funds: {}", timestamp, current_price, funds);
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position > 0.0 {
                position = Self::apply_fibonacci_levels(&mut position, &mut funds, current_price, entry_price, &fib_levels, &mut fib_triggered);
            }
        }

        if position > 0.0 {
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
                funds = position * last_price;
                position = 0.0;
                info!("Final sell at price: {}, funds: {}", last_price, funds);
                if funds > initial_funds {
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
        };

        info!("Final Win rate: {}", win_rate);
        (funds, win_rate)
    }

    pub async fn macd_ema_strategy(&mut self, candles_5m: &[CandlesEntity]) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut ema_20 = ExponentialMovingAverage::new(20).unwrap();
        let mut ema_50 = ExponentialMovingAverage::new(50).unwrap();
        let mut ema_100 = ExponentialMovingAverage::new(100).unwrap();
        let mut ema_200 = ExponentialMovingAverage::new(200).unwrap();
        let mut macd = MovingAverageConvergenceDivergence::new(12, 26, 9).unwrap();

        let prices_5m: Vec<f64> = candles_5m.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();

        for i in 0..candles_5m.len() {
            let current_price = prices_5m[i];
            let ema_20_value = ema_20.next(current_price);
            let ema_50_value = ema_50.next(current_price);
            let ema_100_value = ema_100.next(current_price);
            let ema_200_value = ema_200.next(current_price);
            let macd_value = macd.next(current_price);

            let timestamp = time_util::mill_time_to_datetime_shanghai(candles_5m[i].ts).unwrap();

            let bullish_crossover = macd_value.macd > macd_value.signal;
            let bearish_crossover = macd_value.macd < macd_value.signal;

            println!("timestamp: {}, EMA 20: {}, EMA 50: {}, EMA 100: {}, EMA 200: {}, MACD: {}, Signal: {}, Histogram: {}, Bullish Crossover: {}, Bearish Crossover: {}",
                     timestamp, ema_20_value, ema_50_value, ema_100_value, ema_200_value, macd_value.macd, macd_value.signal, macd_value.histogram, bullish_crossover, bearish_crossover);

            if ema_20_value > ema_50_value && bullish_crossover && position == 0.0 {
                position = funds / current_price;
                funds = 0.0;
                info!("Buy at time: {}, price: {}, position: {}", timestamp, current_price, position);
            } else if ema_20_value < ema_50_value && bearish_crossover && position == 0.0 {
                position = -funds / current_price;
                funds = 0.0;
                info!("Sell at time: {}, price: {}, position: {}", timestamp, current_price, position);
            } else if position > 0.0 && (ema_20_value < ema_50_value || bearish_crossover) {
                funds = position * current_price;
                position = 0.0;
                info!("Sell (close long) at time: {}, price: {}, funds: {}", timestamp, current_price, funds);
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position < 0.0 && (ema_20_value > ema_50_value || bullish_crossover) {
                funds = -position * current_price;
                position = 0.0;
                info!("Buy (close short) at time: {}, price: {}, funds: {}", timestamp, current_price, funds);
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
        }

        if position > 0.0 {
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
                funds = position * last_price;
                position = 0.0;
                info!("Final sell at price: {}, funds: {}", last_price, funds);
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
        } else if position < 0.0 {
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
                funds = -position * last_price;
                position = 0.0;
                info!("Final buy at price: {}, funds: {}", last_price, funds);
                if funds > initial_funds {
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
        };

        info!("Final Win rate: {}", win_rate);
        (funds, win_rate)
    }


    pub async fn short_strategy(&self, candles: &[CandlesEntity], breakout_period: usize, confirmation_period: usize, volume_threshold: f64, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut short_position = 0.0; // 做空持仓
        let mut wins = 0;
        let mut losses = 0;
        let mut entry_price = 0.0; // 记录每次开仓时的价格
        const FIB_LEVELS: [f64; 6] = [0.0236, 0.0382, 0.05, 0.0618, 0.0786, 0.1]; // 斐波那契回撤级别
        let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发

        let mut lowest_low_window = VecDeque::with_capacity(breakout_period);
        let mut volume_window = VecDeque::with_capacity(breakout_period);
        let mut avg_volume = 0.0;

        for i in 0..candles.len() {
            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);
            let volume = candles[i].vol.parse::<f64>().unwrap_or(0.0);
            let timestamp = time_util::mill_time_to_datetime_shanghai(candles[i].ts).unwrap();

            // 更新窗口数据
            if lowest_low_window.len() == breakout_period {
                lowest_low_window.pop_front();
                volume_window.pop_front();
            }
            lowest_low_window.push_back(price);
            volume_window.push_back(volume);
            avg_volume = volume_window.iter().sum::<f64>() / volume_window.len() as f64;

            if i >= breakout_period {
                // 计算跌破信号
                let lowest_low = *lowest_low_window.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

                // 检查是否发生假跌破
                if price < lowest_low && short_position == 0.0 && volume > avg_volume * volume_threshold {
                    // 确认跌破
                    let mut valid_breakdown = true;
                    for j in 1..confirmation_period {
                        if i + j < candles.len() {
                            let confirm_price = candles[i + j].c.parse::<f64>().unwrap_or(0.0);
                            let confirm_volume = candles[i + j].vol.parse::<f64>().unwrap_or(0.0);
                            if confirm_price >= lowest_low || confirm_volume <= avg_volume * volume_threshold {
                                valid_breakdown = false;
                                break;
                            }
                        }
                    }
                    if valid_breakdown {
                        // 确认跌破下轨，开空
                        short_position = funds / price;
                        entry_price = price; // 记录开仓价格
                        funds = 0.0;
                        fib_triggered = [false; 6]; // 重置斐波那契触发标记
                        info!("Breakdown Short at time: {}, price: {}, position: {}", timestamp, price, short_position);
                    }
                } else if short_position > 0.0 {
                    // 计算当前空头持仓的价值
                    let current_value = short_position * price;

                    // 止损逻辑
                    let stop_loss_triggered = match stop_loss_strategy {
                        StopLossStrategy::Amount(stop_loss_amount) => current_value > entry_price * short_position + stop_loss_amount,
                        StopLossStrategy::Percent(stop_loss_percent) => current_value > entry_price * short_position * (1.0 + stop_loss_percent),
                    };

                    if stop_loss_triggered {
                        // 止损买入
                        funds = initial_funds - current_value;
                        short_position = 0.0;
                        losses += 1; // 更新亏损计数
                        info!("Stop loss (short) at time: {}, price: {}, funds: {}", timestamp, price, funds);
                        continue;
                    }

                    // 斐波那契止盈逻辑
                    let mut remaining_position = short_position;
                    for (idx, &level) in FIB_LEVELS.iter().enumerate() {
                        let fib_price = entry_price * (1.0 - level); // 计算斐波那契目标价格
                        if price <= fib_price && !fib_triggered[idx] {
                            let buy_amount = remaining_position * 0.1; // 例如每次买回 10% 的仓位
                            if buy_amount < 1e-8 { // 防止非常小的数值
                                continue;
                            }
                            funds += buy_amount * price;
                            remaining_position -= buy_amount;
                            fib_triggered[idx] = true; // 记录该斐波那契级别已经触发
                            info!("Fibonacci profit taking at level: {}, price: {}, buy amount: {}, remaining position: {}", level, price, buy_amount, remaining_position);

                            // 如果剩余仓位为零，更新win或loss
                            if remaining_position <= 1e-8 {
                                short_position = 0.0;
                                if funds > initial_funds {
                                    wins += 1;
                                } else {
                                    losses += 1;
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        // 如果最后一次操作是买入，则在最后一个收盘价卖出
        if short_position > 0.0 {
            if let Some(last_candle) = candles.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
                funds = initial_funds - (short_position * last_price);
                short_position = 0.0;
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
                info!("Final buy to close short at price: {}, funds: {}", last_price, funds);
            }
        }

        let win_rate = if wins + losses > 0 {
            wins as f64 / (wins + losses) as f64
        } else {
            0.0
        };

        (funds, win_rate)
    }

    pub async fn breakout_strategy(&self, candles: &[CandlesEntity], breakout_period: usize, confirmation_period: usize, volume_threshold: f64, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut entry_price = 0.0; // 记录每次开仓时的价格
        let fib_levels = [0.236, 0.382, 0.5, 0.618, 0.786, 1.0]; // 斐波那契回撤级别
        let fib_levels = [0.0236, 0.0382, 0.05, 0.0618, 0.0786, 0.1]; // 斐波那契回撤级别
        let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发

        for i in breakout_period..candles.len() {
            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);
            let volume = candles[i].vol.parse::<f64>().unwrap_or(0.0); // 假设 Candle 结构体包含成交量字段 `v`
            let timestamp = time_util::mill_time_to_datetime_shanghai(candles[i].ts).unwrap();

            // 计算突破信号
            let highest_high = candles[i - breakout_period..i].iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
            let lowest_low = candles[i - breakout_period..i].iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

            // 计算前几个周期的平均成交量
            let avg_volume: f64 = candles[i - breakout_period..i].iter().map(|c| c.vol.parse::<f64>().unwrap_or(0.0)).sum::<f64>() / breakout_period as f64;

            // 检查是否发生假突破
            if price > highest_high && position == 0.0 && volume > avg_volume * volume_threshold {
                // 确认突破
                let mut valid_breakout = true;
                for j in 1..confirmation_period {
                    if i + j < candles.len() {
                        let confirm_price = candles[i + j].c.parse::<f64>().unwrap_or(0.0);
                        let confirm_volume = candles[i + j].vol.parse::<f64>().unwrap_or(0.0);
                        if confirm_price <= highest_high || confirm_volume <= avg_volume * volume_threshold {
                            valid_breakout = false;
                            break;
                        }
                    }
                }
                if valid_breakout {
                    // 确认突破上轨，买入
                    position = funds / price;
                    entry_price = price; // 记录开仓价格
                    funds = 0.0;
                    fib_triggered = [false; 6]; // 重置斐波那契触发标记
                    info!("Breakout Buy at time: {}, price: {}, position: {}", timestamp, price, position);
                }
            } else if price < lowest_low && position > 0.0 && volume > avg_volume * volume_threshold {
                // 确认跌破，卖出
                let mut valid_breakdown = true;
                for j in 1..confirmation_period {
                    if i + j < candles.len() {
                        let confirm_price = candles[i + j].c.parse::<f64>().unwrap_or(0.0);
                        let confirm_volume = candles[i + j].vol.parse::<f64>().unwrap_or(0.0);
                        if confirm_price >= lowest_low || confirm_volume <= avg_volume * volume_threshold {
                            valid_breakdown = false;
                            break;
                        }
                    }
                }
                if valid_breakdown {
                    funds = position * price;
                    position = 0.0;
                    if funds > initial_funds {
                        wins += 1;
                    } else {
                        losses += 1;
                    }
                    info!("Breakout Sell at time: {}, price: {}, funds: {}", timestamp, price, funds);
                }
            } else if position > 0.0 {
                // 计算当前持仓的价值
                let current_value = position * price;

                // 止损逻辑
                let stop_loss_triggered = match stop_loss_strategy {
                    StopLossStrategy::Amount(stop_loss_amount) => current_value < entry_price * position - stop_loss_amount,
                    StopLossStrategy::Percent(stop_loss_percent) => current_value < entry_price * position * (1.0 - stop_loss_percent),
                };

                if stop_loss_triggered {
                    // 止损卖出
                    funds = current_value;
                    position = 0.0;
                    losses += 1; // 更新亏损计数
                    info!("Stop loss at time: {}, price: {}, funds: {}", timestamp, price, funds);
                    continue;
                }

                // 斐波那契止盈逻辑
                let mut remaining_position = position;
                for (idx, &level) in fib_levels.iter().enumerate() {
                    let fib_price = entry_price * (1.0 + level); // 计算斐波那契目标价格
                    if price >= fib_price && !fib_triggered[idx] {
                        let sell_amount = remaining_position * 0.1; // 例如每次卖出 10% 的仓位
                        if sell_amount < 1e-8 { // 防止非常小的数值
                            continue;
                        }
                        funds += sell_amount * price;
                        remaining_position -= sell_amount;
                        fib_triggered[idx] = true; // 记录该斐波那契级别已经触发
                        info!("Fibonacci profit taking at level: {}, price: {}, sell amount: {}, remaining position: {}", level, price, sell_amount, remaining_position);

                        // 如果剩余仓位为零，更新win或loss
                        if remaining_position <= 1e-8 {
                            position = 0.0;
                            if funds > initial_funds {
                                wins += 1;
                            } else {
                                losses += 1;
                            }
                            break;
                        }
                    }
                }
            }
        }

        // 如果最后一次操作是买入，则在最后一个收盘价卖出
        if position > 0.0 {
            if let Some(last_candle) = candles.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
                funds = position * last_price;
                position = 0.0;
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
                info!("Final sell at price: {}, funds: {}", last_price, funds);
            }
        }

        let win_rate = if wins + losses > 0 {
            wins as f64 / (wins + losses) as f64
        } else {
            0.0
        };

        (funds, win_rate)
    }

    //
    // pub async fn brakeout_startegy_test(&mut self, ins_id: &str, time: &str, fast_period: usize, slow_period: usize, signal_period: usize, breakout_period: usize, confirmation_period: usize, volume_threshold: f64, stop_loss_strategy: StopLossStrategy) -> anyhow::Result<(f64, f64)> {
    //     let mysql_candles_5m = Self::fetch_candles_from_mysql(&self.rb, ins_id, time).await?;
    //     if mysql_candles_5m.is_empty() {
    //         info!("No candles to process.");
    //         return Ok((0.00, 0.00));
    //     }
    //
    //     let (macd_ema_funds, macd_ema_win_rate) = self.long_strategy(&mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
    //     info!("MACD EMA Strategy - Final funds: {}, Win rate: {}", macd_ema_funds, macd_ema_win_rate);
    //
    //     Ok((macd_ema_funds, macd_ema_win_rate))
    // }
    //
    // pub async fn brakedown_startegy_test(&mut self, ins_id: &str, time: &str, fast_period: usize, slow_period: usize, signal_period: usize, breakout_period: usize, confirmation_period: usize, volume_threshold: f64, stop_loss_strategy: StopLossStrategy) -> anyhow::Result<(f64, f64)> {
    //     let mysql_candles_5m = Self::fetch_candles_from_mysql(&self.rb, ins_id, time).await?;
    //     if mysql_candles_5m.is_empty() {
    //         info!("No candles to process.");
    //         return Ok((0.00, 0.00));
    //     }
    //
    //     let (macd_ema_funds, macd_ema_win_rate) = self.short_strategy(&mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
    //     info!("MACD EMA Strategy - Final funds: {}, Win rate: {}", macd_ema_funds, macd_ema_win_rate);
    //
    //     Ok((macd_ema_funds, macd_ema_win_rate))
    // }
}
