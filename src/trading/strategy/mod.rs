pub mod redis_operations;
pub mod support_resistance;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use log::{error, trace};
use serde::{Deserialize, Serialize};
use ta::indicators::{ExponentialMovingAverage, RelativeStrengthIndex};
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

// 枚举表示止损策略的选择
#[derive(Clone, Copy)]
pub enum StopLossStrategy {
    Amount(f64),
    Percent(f64),
}

pub struct Strategy {
    rb: RBatis,
    redis: MultiplexedConnection,
    rsi: RelativeStrengthIndex,
}

impl Strategy {
    pub fn new(db: RBatis, redis: MultiplexedConnection) -> Self {
        Self {
            rb: db,
            redis,
            rsi: RelativeStrengthIndex::new(14).unwrap(), // 14-period RSI
        }
    }

    async fn fetch_candles_from_mysql(rb: &RBatis, ins_id: &str, time: &str) -> anyhow::Result<Vec<CandlesEntity>> {
        // 查询数据
        let candles_model = CandlesModel::new().await;
        let candles = candles_model.get_all(ins_id, time).await;
        match candles {
            Ok(data) => {
                info!("Fetched {} candles from MySQL", data.len());
                Ok(data)
            }
            Err(e) => {
                info!("Error fetching candles from MySQL: {}", e);
                Err(anyhow::anyhow!("Error fetching candles from MySQL"))
            }
        }
    }

    pub fn calculate_macd(&self, candles: &[CandlesEntity], fast_period: usize, slow_period: usize, signal_period: usize, window_size: usize) -> Vec<(i64, f64, f64, i32)> {
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let timestamps: Vec<i64> = candles.iter().map(|c| c.ts).collect();

        let mut ema_fast = ExponentialMovingAverage::new(fast_period).unwrap();
        let mut ema_slow = ExponentialMovingAverage::new(slow_period).unwrap();
        let mut signal_line = ExponentialMovingAverage::new(signal_period).unwrap();

        let mut macd_values: Vec<(i64, f64, f64, i32)> = Vec::new();
        let mut price_window = Vec::with_capacity(window_size);

        for (i, &price) in close_prices.iter().enumerate() {
            // 更新窗口数据
            price_window.push(price);
            if price_window.len() > window_size {
                price_window.remove(0);
            }

            // 计算窗口内的高点和低点
            let last_price_high = *price_window.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
            let last_price_low = *price_window.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

            // 计算快速和慢速 EMA
            let fast_ema_value = ema_fast.next(price);
            let slow_ema_value = ema_slow.next(price);

            // 计算 MACD 值
            let macd_value = fast_ema_value - slow_ema_value;

            // 计算信号线（Signal Line）
            let signal_value = signal_line.next(macd_value);

            // 检查MACD的高点和低点
            let last_macd_high = macd_values.iter().rev().take(window_size).map(|&(_, macd, _, _)| macd).max_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap()).unwrap_or(0.0);
            let last_macd_low = macd_values.iter().rev().take(window_size).map(|&(_, macd, _, _)| macd).min_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap()).unwrap_or(0.0);

            // 检测MACD是否发生偏离
            let mut divergence = 0;
            if price == last_price_high && macd_value < last_macd_high {
                divergence = -1;
            } else if price == last_price_low && macd_value > last_macd_low {
                divergence = 1;
            }

            if divergence != 0 {
                // 输出背离的时间点
                let offset = FixedOffset::west(8 * 3600);
                let datetime = NaiveDateTime::from_timestamp((timestamps[i] / 1000), ((timestamps[i] % 1000) * 1_000_000) as u32);
                let local_datetime = offset.from_local_datetime(&datetime).unwrap();
                let utc_datetime: DateTime<Utc> = local_datetime.with_timezone(&Utc);
                let formatted_time = utc_datetime.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                info!("Divergence detected at time: {}, type: {}", formatted_time, divergence);
            }

            // 将结果存储在 macd_values 中
            macd_values.push((timestamps[i], macd_value, signal_value, divergence));

            // 假设时间戳是东八区时间，转换为东八区时间
            let offset = FixedOffset::west(8 * 3600);
            let datetime = NaiveDateTime::from_timestamp((timestamps[i] / 1000), ((timestamps[i] % 1000) * 1_000_000) as u32);
            let local_datetime = offset.from_local_datetime(&datetime).unwrap();
            let utc_datetime: DateTime<Utc> = local_datetime.with_timezone(&Utc);
            let formatted_time = utc_datetime.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

            // 调试信息
            debug!("datetime: {}", formatted_time);
            info!("Close price: {}, MACD value (DIFF): {}, Signal line (DEA): {}, Histogram (STICK): {}, Divergence: {}", price, macd_value, signal_value, macd_value - signal_value, divergence);
        }

        macd_values
    }


    async fn backtest_rsi_macd(&mut self, candles: &[CandlesEntity], fast_period: usize, slow_period: usize, signal_period: usize, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let win_rate = 0.00;

        let macd_values = self.calculate_macd(candles, fast_period, slow_period, signal_period, 10);

        for i in 1..macd_values.len() {
            let (timestamp, macd_value, signal_value, divergence) = &macd_values[i];

            let timestamp = time_util::mill_time_to_datetime_SHANGHAI(*timestamp).unwrap();

            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);

            // 计算RSI
            let rsi_value = self.rsi.next(price);

            // 动量策略逻辑
            if rsi_value > 80.0 && position == 0.0 {
                // RSI高于70，卖出
                position = funds / price;
                funds = 0.0;
                info!("Momentum Sell at time: {}, price: {}, position: {}", timestamp, price, position);
            } else if rsi_value < 20.0 && position > 0.0 {
                // RSI低于30，买入
                funds = position * price;
                position = 0.0;
                info!("Momentum Buy at time: {}, price: {}, funds: {}", timestamp, price, funds);

                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position > 0.0 {
                let current_value = position * price;
                match stop_loss_strategy {
                    StopLossStrategy::Amount(stop_loss_amount) => {
                        if current_value < initial_funds - stop_loss_amount {
                            // 止损
                            funds = current_value;
                            position = 0.0;
                            info!("Stop loss at time: {}, price: {}, funds: {}", timestamp, price, funds);

                            if funds > initial_funds {
                                wins += 1;
                            } else {
                                losses += 1;
                            }
                        }
                    }
                    StopLossStrategy::Percent(stop_loss_percent) => {
                        if current_value < initial_funds * (1.0 - stop_loss_percent) {
                            // 止损
                            funds = current_value;
                            position = 0.0;
                            info!("Stop loss at time: {}, price: {}, funds: {}", timestamp, price, funds);

                            if funds > initial_funds {
                                wins += 1;
                            } else {
                                losses += 1;
                            }
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

        (funds, win_rate)
    }


    async fn backtest(&self, candles: &[CandlesEntity], fast_period: usize, slow_period: usize, signal_period: usize, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut entry_price = 0.0; // 记录每次开仓时的价格
        let fib_levels = [0.236, 0.382, 0.5, 0.618, 0.786, 1.0]; // 斐波那契回撤级别
        let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发

        let macd_values = self.calculate_macd(candles, fast_period, slow_period, signal_period, 10);
        println!("{:?}", macd_values);

        for i in 1..macd_values.len() {
            let (timestamp, macd_value, signal_value, divergence) = &macd_values[i];
            let timestamp = time_util::mill_time_to_datetime_SHANGHAI(*timestamp).unwrap();
            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);

            if macd_values[i - 1].1 <= macd_values[i - 1].2 && macd_value > signal_value && position == 0.0 {
                // 买入
                position = funds / price;
                entry_price = price; // 记录开仓价格
                funds = 0.0;
                info!("Buy at time: {}, price: {}, position: {}", timestamp, price, position);
                // 重置斐波那契触发标记
                fib_triggered = [false; 6];
            } else if macd_values[i - 1].1 >= macd_values[i - 1].2 && macd_value < signal_value && position > 0.0 {
                // 卖出
                funds = position * price;
                position = 0.0;
                info!("Sell at time: {}, price: {}, funds: {}", timestamp, price, funds);

                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
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
                    info!("Stop loss at time: {}, price: {}, funds: {}", timestamp, price, funds);

                    if funds > entry_price * position {
                        wins += 1;
                    } else {
                        losses += 1;
                    }
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
                        info!("Fibonacci profit taking at level: {},timestamp:{}, price: {}, sell amount: {}, remaining position: {}", level,timestamp, price, sell_amount, remaining_position);

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

        (funds, win_rate)
    }

    async fn breakout_backtest(&self, candles: &[CandlesEntity], breakout_period: usize, confirmation_period: usize, volume_threshold: f64, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
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
            let timestamp = time_util::mill_time_to_datetime_SHANGHAI(candles[i].ts).unwrap();

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


    pub async fn main(&mut self, ins_id: &str, time: &str, fast_period: usize, slow_period: usize, signal_period: usize, breakout_period: usize, confirmation_period: usize, volume_threshold: f64, stop_loss_strategy: StopLossStrategy) -> anyhow::Result<()> {
        let mysql_candles = Self::fetch_candles_from_mysql(&self.rb, ins_id, time).await?;
        if mysql_candles.is_empty() {
            info!("No candles to process.");
            return Ok(());
        }

        // let mut redis_conn = self.redis.clone();
        // let candle_structs: Vec<Candle> = mysql_candles.iter().map(|c| Candle { ts: c.ts, c: c.c.clone() }).collect();
        // RedisOperations::save_candles_to_redis(&mut redis_conn, CandlesModel::get_tale_name(ins_id, time).as_str(), &candle_structs).await?;

        //从redis中读取数据
        // let redis_candles = RedisOperations::fetch_candles_from_redis(&mut redis_conn, CandlesModel::get_tale_name(ins_id, time).as_str()).await?;
        // if redis_candles.is_empty() {
        //     info!("No candles found in Redis for MACD calculation.");
        //     return Ok(());
        // }

        // // 计算顶部压力位和底部支撑位
        // let (resistance_level, support_level) = SupportResistance::segment_data(&mysql_candles, 30); // 分段数据方法
        // info!("Segment Data - Resistance level: {}, Support level: {}", resistance_level, support_level);
        //
        // let (resistance_level, avage, support_level) = SupportResistance::bollinger_bands_latest(&mysql_candles, 20); // 布林带方法
        // info!("Bollinger Bands - Resistance level: {}, Support level: {} aveage level{}", resistance_level, support_level,avage);
        //
        // let (peaks, valleys) = SupportResistance::peaks_and_valleys(&mysql_candles); // 波峰和波谷方法
        // info!("Peaks: {:?}, Valleys: {:?}", peaks, valleys);
        //
        // let (fractal_highs, fractal_lows) = SupportResistance::fractal(&mysql_candles); // 分形理论方法
        // info!("Fractal Highs: {:?}, Fractal Lows: {:?}", fractal_highs, fractal_lows);
        //
        // let (resistance_level, support_level) = SupportResistance::manual_marking(&mysql_candles); // 手动标注方法
        // info!("Manual Marking - Resistance level: {}, Support level: {}", resistance_level, support_level);
        //
        // let (resistance_level, support_level) = SupportResistance::kama(&mysql_candles, 10); // KAMA 方法
        // info!("KAMA - Resistance level: {}, Support level: {}", resistance_level, support_level);
        //
        // let (overbought, oversold) = SupportResistance::cci(&mysql_candles, 20); // CCI 方法
        // info!("CCI - Overbought: {:?}, Oversold: {:?}", overbought, oversold);

        // // 动量策略回测
        // let (momentum_final_funds, momentum_win_rate) = self.backtest(&redis_candles, fast_period, slow_period, signal_period, stop_loss_strategy).await;
        // info!("Momentum Strategy - Final funds: {}, Win rate: {}", momentum_final_funds, momentum_win_rate);
        //
        // strategy.main("BTCUSD", "2021-01-01", 12, 26, 9, 20, 3, StopLossStrategy::Percent(0.05)).await?;

        // 突破策略回测
        let (breakout_final_funds, breakout_win_rate) = self.breakout_backtest(&mysql_candles, breakout_period, confirmation_period, volume_threshold as usize as f64, stop_loss_strategy).await;
        tracing::error!("Breakout Strategy - Final funds: {}, Win rate: {}", breakout_final_funds, breakout_win_rate);
        Ok(())
    }
}