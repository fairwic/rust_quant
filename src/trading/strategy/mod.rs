pub mod redis_operations;

use serde::{Deserialize, Serialize};
use ta::indicators::ExponentialMovingAverage;
use ta::Next;
use tokio;
use rbatis::RBatis;
use redis::aio::MultiplexedConnection;
use tracing::info;
use crate::trading::model::market::candles::{CandlesEntity, CandlesModel};
use crate::trading::strategy::redis_operations::{Candle, RedisOperations};

// 枚举表示止损策略的选择
pub enum StopLossStrategy {
    Amount(f64),
    Percent(f64),
}

pub struct Strategy {
    rb: RBatis,
    redis: MultiplexedConnection,
}

impl Strategy {
    pub fn new(db: RBatis, redis: MultiplexedConnection) -> Self {
        Self { rb: db, redis }
    }

    async fn fetch_candles_from_mysql(rb: &RBatis, ins_id: &str, time: &str) -> anyhow::Result<Vec<CandlesEntity>> {
        // 查询数据
        let candles_model = CandlesModel::new().await;
        let candles = candles_model.get_all(ins_id, time).await;
        match candles {
            Ok(data) => {
                info!("Fetched {} candles from MySQL", data.len());
                Ok(data)
            },
            Err(e) => {
                info!("Error fetching candles from MySQL: {}", e);
                Err(anyhow::anyhow!("Error fetching candles from MySQL"))
            }
        }
    }

    fn calculate_macd(&self, candles: &[Candle], fast_period: usize, slow_period: usize, signal_period: usize) -> Vec<(String, f64, f64)> {
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let timestamps: Vec<String> = candles.iter().map(|c| c.ts.clone()).collect();

        let mut ema_fast = ExponentialMovingAverage::new(fast_period).unwrap();
        let mut ema_slow = ExponentialMovingAverage::new(slow_period).unwrap();
        let mut signal_line = ExponentialMovingAverage::new(signal_period).unwrap();

        let mut macd_values = Vec::new();

        for &price in &close_prices {
            let macd_value = ema_fast.next(price) - ema_slow.next(price);
            let signal_value = signal_line.next(macd_value);
            macd_values.push((timestamps[macd_values.len()].clone(), macd_value, signal_value));
            info!("Close price: {}, MACD value: {}, Signal line: {}", price, macd_value, signal_value);
        }

        macd_values
    }

    async fn backtest(&self, candles: &[Candle], fast_period: usize, slow_period: usize, signal_period: usize, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;

        let macd_values = self.calculate_macd(candles, fast_period, slow_period, signal_period);

        for i in 1..macd_values.len() {
            let (timestamp, macd_value, signal_value) = &macd_values[i];
            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);

            if macd_values[i - 1].1 <= macd_values[i - 1].2 && macd_value > signal_value && position == 0.0 {
                // 买入
                position = funds / price;
                funds = 0.0;
                info!("Buy at time: {}, price: {}, position: {}", timestamp, price, position);
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
                    },
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
                    },
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

    pub async fn main(&self, ins_id: &str, time: &str, fast_period: usize, slow_period: usize, signal_period: usize, stop_loss_strategy: StopLossStrategy) -> anyhow::Result<()> {
        let candles = Self::fetch_candles_from_mysql(&self.rb, ins_id, time).await?;
        if candles.is_empty() {
            info!("No candles to process.");
            return Ok(());
        }

        let mut redis_conn = self.redis.clone();
        let candle_structs: Vec<Candle> = candles.iter().map(|c| Candle { ts: c.ts.clone(), c: c.c.clone() }).collect();
        RedisOperations::save_candles_to_redis(&mut redis_conn, &candle_structs).await?;

        let redis_candles = RedisOperations::fetch_candles_from_redis(&mut redis_conn).await?;
        if redis_candles.is_empty() {
            info!("No candles found in Redis for MACD calculation.");
            return Ok(());
        }

        let (final_funds, win_rate) = self.backtest(&redis_candles, fast_period, slow_period, signal_period, stop_loss_strategy).await;
        info!("Final funds after backtesting: {}", final_funds);
        info!("Win rate: {}", win_rate);

        Ok(())
    }
}