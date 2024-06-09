pub mod redis_operations;
pub mod support_resistance;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use log::{trace};
use serde::{Deserialize, Serialize};
use ta::indicators::ExponentialMovingAverage;
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
            }
            Err(e) => {
                info!("Error fetching candles from MySQL: {}", e);
                Err(anyhow::anyhow!("Error fetching candles from MySQL"))
            }
        }
    }

    pub fn calculate_macd(&self, candles: &[Candle], fast_period: usize, slow_period: usize, signal_period: usize) -> Vec<(i64, f64, f64)> {
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let timestamps: Vec<i64> = candles.iter().map(|c| c.ts).collect();

        let mut ema_fast = ExponentialMovingAverage::new(fast_period).unwrap();
        let mut ema_slow = ExponentialMovingAverage::new(slow_period).unwrap();
        let mut signal_line = ExponentialMovingAverage::new(signal_period).unwrap();

        let mut macd_values = Vec::new();

        for (i, &price) in close_prices.iter().enumerate() {
            // 计算快速和慢速 EMA
            let fast_ema_value = ema_fast.next(price);
            let slow_ema_value = ema_slow.next(price);

            // 计算 MACD 值
            let macd_value = fast_ema_value - slow_ema_value;

            // 计算信号线（Signal Line）
            let signal_value = signal_line.next(macd_value);

            // 将结果存储在 macd_values 中
            macd_values.push((timestamps[i], macd_value, signal_value));

            // 假设时间戳是东八区时间，转换为UTC时间
            let offset = FixedOffset::west(8 * 3600);
            let datetime = NaiveDateTime::from_timestamp((timestamps[i] / 1000), ((timestamps[i] % 1000) * 1_000_000) as u32);
            let local_datetime = offset.from_local_datetime(&datetime).unwrap();
            let utc_datetime: DateTime<Utc> = local_datetime.with_timezone(&Utc);
            let formatted_time = utc_datetime.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

            // 调试信息
            debug!("datetime: {}", formatted_time);
            info!("Close price: {}, MACD value (DIFF): {}, Signal line (DEA): {}, Histogram (STICK): {}", price, macd_value, signal_value, macd_value - signal_value);
        }

        macd_values
    }

    async fn backtest(&self, candles: &[Candle], fast_period: usize, slow_period: usize, signal_period: usize, stop_loss_strategy: StopLossStrategy) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let win_rate = 0.00;

        let macd_values = self.calculate_macd(candles, fast_period, slow_period, signal_period);

        for i in 1..macd_values.len() {
            let (timestamp, macd_value, signal_value) = &macd_values[i];

            let timestamp = time_util::mill_time_to_datetime_SHANGHAI(*timestamp).unwrap();

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

    pub async fn main(&self, ins_id: &str, time: &str, fast_period: usize, slow_period: usize, signal_period: usize, stop_loss_strategy: StopLossStrategy) -> anyhow::Result<()> {
        let mysql_candles = Self::fetch_candles_from_mysql(&self.rb, ins_id, time).await?;
        if mysql_candles.is_empty() {
            info!("No candles to process.");
            return Ok(());
        }

        let mut redis_conn = self.redis.clone();
        let candle_structs: Vec<Candle> = mysql_candles.iter().map(|c| Candle { ts: c.ts, c: c.c.clone() }).collect();
        RedisOperations::save_candles_to_redis(&mut redis_conn, CandlesModel::get_tale_name(ins_id, time).as_str(), &candle_structs).await?;

        let redis_candles = RedisOperations::fetch_candles_from_redis(&mut redis_conn, CandlesModel::get_tale_name(ins_id, time).as_str()).await?;
        if redis_candles.is_empty() {
            info!("No candles found in Redis for MACD calculation.");
            return Ok(());
        }

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

        let (final_funds, win_rate) = self.backtest(&redis_candles, fast_period, slow_period, signal_period, stop_loss_strategy).await;
        info!("Final funds after backtesting: {}", final_funds);
        info!("Win rate: {}", win_rate);

        Ok(())
    }


    pub fn plot_kline_chart(&self, candles: &[Candle], file_path: &str) -> Result<(), Box<dyn Error>> {
        let root = BitMapBackend::new(file_path, (1024, 768)).into_drawing_area();
        root.fill(&WHITE)?;

        let x_range = 0..candles.len();
        let y_range = candles.iter().map(|x| x.c.parse::<f64>().unwrap_or(0.0)).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
            ..candles.iter().map(|x| x.c.parse::<f64>().unwrap_or(0.0)).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

        let mut chart = ChartBuilder::on(&root)
            .caption("K-Line Chart", ("sans-serif", 50))
            .x_label_area_size(30)
            .y_label_area_size(40)
            .margin(5)
            .build_cartesian_2d(x_range.clone(), y_range.clone())?;

        chart.configure_mesh().draw()?;

        for (i, candle) in candles.iter().enumerate() {
            let x = i;
            let open = candle.o.parse::<f64>().unwrap_or(0.0);
            let high = candle.h.parse::<f64>().unwrap_or(0.0);
            let low = candle.l.parse::<f64>().unwrap_or(0.0);
            let close = candle.c.parse::<f64>().unwrap_or(0.0);

            chart.draw_series(std::iter::once(CandleStick::new(
                x,
                open,
                high,
                low,
                close,
                &RED,
                &BLUE,
                15,
            )))?;
        }

        Ok(())
    }
}
