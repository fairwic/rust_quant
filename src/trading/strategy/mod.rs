pub mod arc;
pub mod comprehensive_strategy;
pub mod engulfing_strategy;
pub mod macd_kdj_strategy;
pub mod order;
pub mod profit_stop_loss;
pub mod redis_operations;
mod squeeze_strategy;
pub mod strategy_common;
pub mod strategy_manager;
pub mod support_resistance;
pub mod top_contract_strategy;
pub mod ut_boot_strategy;
pub mod nwe_strategy;

// 🆕 策略可扩展性框架
pub mod strategy_trait;
pub mod strategy_registry;
pub mod executor_common;
pub mod vegas_executor;
pub mod nwe_executor;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use log::{error, trace};
use serde::{Deserialize, Serialize};
use ta::indicators::{
    AverageTrueRange, BollingerBands, ExponentialMovingAverage, FastStochastic, KeltnerChannel,
    MovingAverageConvergenceDivergence, RelativeStrengthIndex, SlowStochastic,
};

use crate::time_util;
use crate::trading::indicator::kdj_simple_indicator::{KdjCandle, KDJ};
use crate::trading::indicator::macd_simple_indicator::MacdSimpleIndicator;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::model::strategy::strategy_job_signal_log;
use crate::trading::strategy::redis_operations::{RedisCandle, RedisOperations};
use crate::trading::strategy::support_resistance::SupportResistance;
use okx::dto::market_dto::CandleOkxRespDto;
use okx::dto::EnumToStrTrait;
use rbatis::RBatis;
use std::collections::VecDeque;
use std::fmt::Display;
use ta::{Close, High, Low, Next};
use tokio;
use tracing::debug;
use tracing::info;
// use crate::trading::strategy::ut_boot_strategy::SignalResult;

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
    MacdWithKdj,
    MacdWithEma,
    Boll,
    UtBoot,
    UtBootShort,
    Engulfing,
    TopContract,
    Vegas,
    Nwe,
}

impl StrategyType {
    /// 从字符串解析为策略类型
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "BreakoutUp" => Some(StrategyType::BreakoutUp),
            "BreakoutDown" => Some(StrategyType::BreakoutDown),
            "Macd" => Some(StrategyType::Macd),
            "MacdWithKdj" => Some(StrategyType::MacdWithKdj),
            "MacdWithEma" => Some(StrategyType::MacdWithEma),
            "Boll" => Some(StrategyType::Boll),
            "UtBoot" => Some(StrategyType::UtBoot),
            "UtBootShort" => Some(StrategyType::UtBootShort),
            "Engulfing" => Some(StrategyType::Engulfing),
            "TopContract" => Some(StrategyType::TopContract),
            "Vegas" => Some(StrategyType::Vegas),
            "Nwe" => Some(StrategyType::Nwe),
            _ => None,
        }
    }
}

impl EnumToStrTrait for StrategyType {
    fn as_str(&self) -> &'static str {
        match self {
            StrategyType::BreakoutUp => "BreakoutUp",
            StrategyType::BreakoutDown => "BreakoutDown",
            StrategyType::Macd => "Macd",
            StrategyType::MacdWithKdj => "MacdWithKdj",
            StrategyType::MacdWithEma => "MacdWithEma",
            StrategyType::Boll => "Boll",
            StrategyType::UtBoot => "UtBoot",
            StrategyType::UtBootShort => "UtBootShort",
            StrategyType::Engulfing => "Engulfing",
            StrategyType::TopContract => "TopContract",
            StrategyType::Vegas => "Vegas",
            StrategyType::Nwe => "Nwe",
        }
    }
}

// impl PartialEq for StrategyType {
//     fn eq(&self, other: &Self) -> bool {
//         self.to_string() == other.to_string()
//     }
// }

pub struct Strategy {
    rb: &'static RBatis,
    rsi: RelativeStrengthIndex,
    ema_1h: ExponentialMovingAverage,
    macd: MovingAverageConvergenceDivergence,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct UTBotAlert {
    atr: AverageTrueRange,
    ema_short: ExponentialMovingAverage,
    ema_long: ExponentialMovingAverage,
}

impl UTBotAlert {
    pub fn new(
        atr_period: usize,
        ema_short_period: usize,
        ema_long_period: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            atr: AverageTrueRange::new(atr_period)?,
            ema_short: ExponentialMovingAverage::new(ema_short_period)?,
            ema_long: ExponentialMovingAverage::new(ema_long_period)?,
        })
    }
}

impl Strategy {
    pub fn new(db: &'static RBatis) -> Self {
        Self {
            rb: db,
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

    pub async fn short_term_strategy(
        &mut self,
        candles_5m: &[CandlesEntity],
        candles_1h: &[CandlesEntity],
    ) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut entry_price = 0.0;
        let fib_levels: [f64; 6] = [0.0236, 0.0382, 0.05, 0.0618, 0.0786, 0.1];
        let mut fib_triggered = [false; 6];

        let ema_1h_values = Self::calculate_ema(candles_1h, 12);

        let prices_5m: Vec<f64> = candles_5m
            .iter()
            .map(|c| c.c.parse::<f64>().unwrap_or(0.0))
            .collect();
        let low_prices_5m: Vec<f64> = candles_5m
            .iter()
            .map(|c| c.l.parse::<f64>().unwrap_or(0.0))
            .collect();
        let high_prices_5m: Vec<f64> = candles_5m
            .iter()
            .map(|c| c.h.parse::<f64>().unwrap_or(0.0))
            .collect();

        for i in 1..candles_5m.len() {
            let current_price = prices_5m[i];
            let prev_price = prices_5m[i - 1];
            let timestamp = time_util::mill_time_to_datetime_shanghai(candles_5m[i].ts).unwrap();
            let ema_1h_value = ema_1h_values
                .iter()
                .filter(|&&(ts, _)| ts <= candles_5m[i].ts)
                .last()
                .map(|&(_, ema)| ema)
                .unwrap_or(0.0);

            let bullish_engulfing =
                current_price > prev_price && low_prices_5m[i] < low_prices_5m[i - 1];
            let bearish_engulfing =
                current_price < prev_price && high_prices_5m[i] > high_prices_5m[i - 1];

            println!("timestamp: {}, bullish_engulfing: {}, bearish_engulfing: {}, current_price: {}, ema_1h_value: {}", timestamp, bullish_engulfing, bearish_engulfing, current_price, ema_1h_value);

            if bullish_engulfing && current_price > ema_1h_value && position == 0.0 {
                position = funds / current_price;
                entry_price = current_price;
                funds = 0.0;
                info!(
                    "Buy at time: {}, price: {}, position: {}",
                    timestamp, current_price, position
                );
                fib_triggered = [false; 6];
            } else if bearish_engulfing && current_price < ema_1h_value && position == 0.0 {
                position = funds / current_price;
                entry_price = current_price;
                funds = 0.0;
                info!(
                    "Sell at time: {}, price: {}, position: {}",
                    timestamp, current_price, position
                );
                fib_triggered = [false; 6];
            } else if position > 0.0
                && (current_price < ema_1h_value || current_price < entry_price)
            {
                funds = position * current_price;
                position = 0.0;
                info!(
                    "Sell at time: {}, price: {}, funds: {}",
                    timestamp, current_price, funds
                );
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position > 0.0 {
                position = Self::apply_fibonacci_levels(
                    &mut position,
                    &mut funds,
                    current_price,
                    entry_price,
                    &fib_levels,
                    &mut fib_triggered,
                );
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

    pub async fn macd_ema_strategy(
        &mut self,
        candles_5m: &[CandlesEntity],
        stop_loss_percent: f64,
    ) -> (f64, f64, usize) {
        let initial_funds = 100.0; // 初始资金
        let mut funds = initial_funds; // 当前资金
        let mut position: f64 = 0.0; // 当前持仓量，显式指定为 f64 类型
        let mut wins = 0; // 赢的次数
        let mut losses = 0; // 输的次数
        let mut open_trades = 0; // 开仓次数
        let mut ema_20 = ExponentialMovingAverage::new(20).unwrap(); // 初始化20周期EMA
        let mut ema_50 = ExponentialMovingAverage::new(50).unwrap(); // 初始化50周期EMA
        let mut ema_100 = ExponentialMovingAverage::new(100).unwrap(); // 初始化100周期EMA
        let mut ema_200 = ExponentialMovingAverage::new(200).unwrap(); // 初始化200周期EMA
        let mut macd = MovingAverageConvergenceDivergence::new(12, 26, 9).unwrap(); // 初始化MACD指标

        let prices_5m: Vec<f64> = candles_5m
            .iter()
            .map(|c| {
                c.c.parse::<f64>().unwrap_or_else(|e| {
                    error!("Failed to parse price: {}", e);
                    0.0
                })
            })
            .collect(); // 提取5分钟的收盘价格数据

        // let stop_loss_percent = 0.05; // 设置止损百分比

        for i in 0..candles_5m.len() {
            // 遍历每个5分钟的蜡烛图数据
            let current_price = prices_5m[i]; // 当前价格
            let ema_20_value = ema_20.next(current_price); // 计算20周期EMA
            let ema_50_value = ema_50.next(current_price); // 计算50周期EMA
            let ema_100_value = ema_100.next(current_price); // 计算100周期EMA
            let ema_200_value = ema_200.next(current_price); // 计算200周期EMA
            let macd_value = macd.next(current_price); // 计算MACD值

            let timestamp = time_util::mill_time_to_datetime_shanghai(candles_5m[i].ts).unwrap(); // 转换时间戳

            let bullish_crossover = macd_value.macd > macd_value.signal; // 看涨交叉信号
            let bearish_crossover = macd_value.macd < macd_value.signal; // 看跌交叉信号

            if ema_20_value > ema_50_value && bullish_crossover && position.abs() < f64::EPSILON {
                // 当20周期EMA大于50周期EMA且出现看涨交叉时开多仓
                position = funds / current_price;
                funds = 0.0;
                open_trades += 1; // 记录开仓次数
                info!(
                    "Buy at time: {}, price: {}, position: {}",
                    timestamp, current_price, position
                );
            } else if position > 0.0
                && (ema_20_value < ema_50_value
                    || bearish_crossover
                    || current_price < position * (1.0 - stop_loss_percent))
            {
                // 平多仓的条件：20周期EMA小于50周期EMA，或出现看跌交叉，或价格达到止损线
                funds = position * current_price;
                position = 0.0;
                info!(
                    "Sell (close long) at time: {}, price: {}, funds: {}",
                    timestamp, current_price, funds
                );
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
        }

        if position > 0.0 {
            // 如果最后还有多仓未平仓，按最后一个价格平仓
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
                    error!("Failed to parse price: {}", e);
                    0.0
                });
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
        }; // 计算胜率

        info!("Final Win rate: {}", win_rate);
        (funds, win_rate, open_trades) // 返回最终资金，胜率和开仓次数
    }

    pub async fn kdj_macd_strategy(
        &mut self,
        candles_5m: &[CandlesEntity],
        stop_loss_percent: f64,
        kdj_period: usize,
        ema_period: usize,
    ) -> (f64, f64, usize) {
        let initial_funds = 100.0; // 初始资金
        let mut funds = initial_funds; // 当前资金
        let mut position: f64 = 0.0; // 当前持仓量，显式指定为 f64 类型
        let mut wins = 0; // 赢的次数
        let mut losses = 0; // 输的次数
        let mut open_trades = 0; // 开仓次数

        // 计算所有的 MACD 值
        let macd_values = MacdSimpleIndicator::calculate_macd(candles_5m, 12, 26, 9);

        let mut fast_stochastic = FastStochastic::new(14).unwrap(); // 初始化快速随机指标（FastStochastic）
        let mut slow_stochastic = SlowStochastic::new(14, 3).unwrap(); // 初始化慢速随机指标（SlowStochastic）
        let mut d_ema = ExponentialMovingAverage::new(3).unwrap(); // 初始化 D 值的指数移动平均

        let mut kdjs: Vec<KDJ> = Vec::new();

        for (i, candle) in candles_5m.iter().enumerate() {
            let current_price = candle.c.parse::<f64>().unwrap_or_else(|e| {
                error!("Failed to parse price: {}", e);
                0.0
            });

            let high_price = candle.h.parse::<f64>().unwrap_or(0.0);
            let low_price = candle.l.parse::<f64>().unwrap_or(0.0);

            let candle_data = KdjCandle {
                high: high_price,
                low: low_price,
                close: current_price,
            };

            // 计算快速随机指标的 K 值
            let fast_k = fast_stochastic.next(&candle_data);
            // 使用慢速随机指标平滑 K 值
            let slow_k = slow_stochastic.next(fast_k);

            // 计算 D 值（3 天 EMA）
            let d_value = d_ema.next(slow_k);

            // 计算 J 值
            let j_value = 3.0 * slow_k - 2.0 * d_value;

            kdjs.push(KDJ {
                k: slow_k,
                d: d_value,
                j: j_value,
            }); // 保存 KDJ 值

            let (timestamp, macd_value, signal_value) = macd_values[i]; // 获取预计算的 MACD 值

            let bullish_crossover = macd_value > signal_value; // 看涨交叉信号

            // 添加日志记录
            info!("Time: {}, Slow KDJ K: {}, D: {}, J: {}, MACD: {}, Signal: {}, Bullish Crossover: {}",
              timestamp, slow_k, d_value, j_value, macd_value, signal_value, bullish_crossover);

            if slow_k < 20.0 && d_value < 20.0 && bullish_crossover && position.abs() < f64::EPSILON
            {
                // 当 K 值和 D 值都小于 20 且 MACD 出现看涨交叉时开多仓
                position = funds / current_price;
                funds = 0.0;
                open_trades += 1; // 记录开仓次数
                info!(
                    "Buy at time: {}, price: {}, position: {}",
                    timestamp, current_price, position
                );
            } else if position > 0.0
                && (slow_k > 80.0
                    || macd_value < signal_value
                    || current_price < position * (1.0 - stop_loss_percent))
            {
                // 平多仓的条件：K 值大于 80，或 MACD 出现看跌交叉，或价格达到止损线
                funds = position * current_price;
                position = 0.0;
                info!(
                    "Sell (close long) at time: {}, price: {}, funds: {}",
                    timestamp, current_price, funds
                );
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
        }

        if position > 0.0 {
            // 如果最后还有多仓未平仓，按最后一个价格平仓
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
                    error!("Failed to parse price: {}", e);
                    0.0
                });
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
        }; // 计算胜率

        info!("Final Win rate: {}", win_rate);
        (funds, win_rate, open_trades) // 返回最终资金，胜率和开仓次数
    }

    // 线性回归计算
    fn calculate_linreg(candles: &[CandlesEntity], length: usize) -> Vec<f64> {
        let mut linreg_values = vec![0.0; candles.len()];

        for i in length..candles.len() {
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut sum_xy = 0.0;
            let mut sum_xx = 0.0;
            for j in 0..length {
                let x = j as f64;
                let y = candles[i - j].c.parse::<f64>().unwrap_or(0.0);
                sum_x += x;
                sum_y += y;
                sum_xy += x * y;
                sum_xx += x * x;
            }
            let slope =
                (length as f64 * sum_xy - sum_x * sum_y) / (length as f64 * sum_xx - sum_x * sum_x);
            linreg_values[i] = slope;
        }

        linreg_values
    }

    // ADX 计算
    fn calculate_adx(candles: &[CandlesEntity], adx_len: usize, di_len: usize) -> Vec<f64> {
        let mut adx_values = Vec::with_capacity(candles.len());
        let mut plus_dm = vec![0.0; candles.len()];
        let mut minus_dm = vec![0.0; candles.len()];
        let mut tr = vec![0.0; candles.len()];

        for i in 1..candles.len() {
            let high = candles[i].h.parse::<f64>().unwrap_or(0.0);
            let low = candles[i].l.parse::<f64>().unwrap_or(0.0);
            let close = candles[i].c.parse::<f64>().unwrap_or(0.0);
            let prev_close = candles[i - 1].c.parse::<f64>().unwrap_or(0.0);

            let up_move = high - candles[i - 1].h.parse::<f64>().unwrap_or(0.0);
            let down_move = candles[i - 1].l.parse::<f64>().unwrap_or(0.0) - low;

            plus_dm[i] = if up_move > down_move && up_move > 0.0 {
                up_move
            } else {
                0.0
            };
            minus_dm[i] = if down_move > up_move && down_move > 0.0 {
                down_move
            } else {
                0.0
            };

            tr[i] = (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs());
        }

        let plus_di = plus_dm
            .iter()
            .scan(0.0, |acc, &x| {
                *acc = *acc - *acc / di_len as f64 + x;
                Some(*acc)
            })
            .collect::<Vec<f64>>();

        let minus_di = minus_dm
            .iter()
            .scan(0.0, |acc, &x| {
                *acc = *acc - *acc / di_len as f64 + x;
                Some(*acc)
            })
            .collect::<Vec<f64>>();

        let tr_smooth = tr
            .iter()
            .scan(0.0, |acc, &x| {
                *acc = *acc - *acc / di_len as f64 + x;
                Some(*acc)
            })
            .collect::<Vec<f64>>();

        for i in 0..candles.len() {
            let plus_di_value = 100.0 * plus_di[i] / tr_smooth[i];
            let minus_di_value = 100.0 * minus_di[i] / tr_smooth[i];
            let dx = if plus_di_value + minus_di_value == 0.0 {
                0.0
            } else {
                100.0 * (plus_di_value - minus_di_value).abs() / (plus_di_value + minus_di_value)
            };

            let adx = if i < adx_len {
                dx
            } else {
                adx_values.iter().take(adx_len - 1).sum::<f64>() / (adx_len - 1) as f64
            };

            adx_values.push(adx);
        }

        adx_values
    }

    pub async fn comprehensive_strategy(
        candles_5m: &[CandlesEntity],
        atr_threshold: f64,
        ema_short_period: usize,
        ema_long_period: usize,
        atr_period: usize,
        adx_period: usize,
        adx_smoothing: usize,
        andean_length: usize,
        sig_length: usize,
        bb_mult: f64,
        kc_mult_high: f64,
        kc_mult_mid: f64,
        kc_mult_low: f64,
        ttm_length: usize,
        stop_loss_percent: f64,
    ) -> (f64, f64, usize) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position: f64 = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut open_trades = 0;

        let mut ut_bot_alert =
            UTBotAlert::new(atr_period, ema_short_period, ema_long_period).unwrap();

        let adx_values = Self::calculate_adx(candles_5m, adx_smoothing, adx_period);
        let mut ema_andean = ExponentialMovingAverage::new(sig_length).unwrap();

        let mut bb = BollingerBands::new(bb_mult as usize, ttm_length as f64).unwrap();
        let mut kc_high = KeltnerChannel::new(kc_mult_high as usize, ttm_length as f64).unwrap();
        let mut kc_mid = KeltnerChannel::new(kc_mult_mid as usize, ttm_length as f64).unwrap();
        let mut kc_low = KeltnerChannel::new(kc_mult_low as usize, ttm_length as f64).unwrap();

        let linreg_values = Self::calculate_linreg(candles_5m, ttm_length);

        for (i, candle) in candles_5m.iter().enumerate() {
            let current_price = candle.c.parse::<f64>().unwrap_or_else(|e| {
                eprintln!("Failed to parse price: {}", e);
                0.0
            });

            let high_price = candle.h.parse::<f64>().unwrap_or(0.0);
            let low_price = candle.l.parse::<f64>().unwrap_or(0.0);

            let candle_data = KdjCandle {
                high: high_price,
                low: low_price,
                close: current_price,
            };

            // 计算 ADX 值
            let adx_value = adx_values[i];

            // 计算 ATR 值
            let atr_value = ut_bot_alert.atr.next(&candle_data);

            // 计算短期和长期 EMA 值
            let ema_short_value = ut_bot_alert.ema_short.next(current_price);
            let ema_long_value = ut_bot_alert.ema_long.next(current_price);

            let bullish_crossover = ema_short_value > ema_long_value;
            let bearish_crossover = ema_short_value < ema_long_value;

            // 计算 Andean Oscillator
            let up1 = current_price.max(ema_andean.next(current_price));
            let up2 =
                (current_price * current_price).max(ema_andean.next(current_price * current_price));
            let dn1 = current_price.min(ema_andean.next(current_price));
            let dn2 =
                (current_price * current_price).min(ema_andean.next(current_price * current_price));

            let bull = (dn2 - dn1 * dn1).sqrt();
            let bear = (up2 - up1 * up1).sqrt();

            // 计算 TTM Squeeze
            let bb_value = bb.next(current_price);
            let kc_high_value = kc_high.next(current_price);
            let kc_mid_value = kc_mid.next(current_price);
            let kc_low_value = kc_low.next(current_price);

            let no_squeeze =
                bb_value.lower < kc_low_value.lower || bb_value.upper > kc_low_value.upper;
            let low_squeeze =
                bb_value.lower >= kc_low_value.lower && bb_value.upper <= kc_low_value.upper;
            let mid_squeeze =
                bb_value.lower >= kc_mid_value.lower && bb_value.upper <= kc_mid_value.upper;
            let high_squeeze =
                bb_value.lower >= kc_high_value.lower && bb_value.upper <= kc_high_value.upper;

            let mom = linreg_values[i];

            // 综合策略条件
            let buy_condition_adx = adx_value < atr_threshold;
            let buy_condition_andean = bull > bear;
            let buy_condition_ttm = mom > 0.0 && (low_squeeze || mid_squeeze) && !no_squeeze;

            let sell_condition_adx = adx_value >= atr_threshold;
            let sell_condition_andean = bull <= bear;
            let sell_condition_ttm = mom < 0.0 && (low_squeeze || mid_squeeze) && !no_squeeze;

            let buy_condition = buy_condition_adx && buy_condition_andean && buy_condition_ttm;
            let sell_condition = sell_condition_adx && sell_condition_andean && sell_condition_ttm;

            // 添加日志记录
            println!("Time: {},found:{} ADX: {}, EMA Short: {}, EMA Long: {}, ATR: {}, Bullish Crossover: {}, Bearish Crossover: {}, Bull: {}, Bear: {}, MOM: {}, Low Squeeze: {}, Mid Squeeze: {}, No Squeeze: {}",
                     candle.ts, funds, adx_value, ema_short_value, ema_long_value, atr_value, bullish_crossover, bearish_crossover, bull, bear, mom, low_squeeze, mid_squeeze, no_squeeze);

            if buy_condition && position.abs() < f64::EPSILON {
                // 当满足买入条件时开多仓
                position = funds / current_price;
                funds = 0.0;
                open_trades += 1;
                println!(
                    "Buy at time: {}, price: {}, position: {}",
                    candle.ts, current_price, position
                );
            } else if sell_condition && position > 0.0 {
                // 当满足卖出条件时平多仓
                funds = position * current_price;
                position = 0.0;
                println!(
                    "Sell (close long) at time: {}, price: {}, funds: {}",
                    candle.ts, current_price, funds
                );
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position > 0.0 && current_price < position * (1.0 - stop_loss_percent) {
                // 当价格达到止损线时平多仓
                funds = position * current_price;
                position = 0.0;
                losses += 1;
                println!(
                    "Stop loss at time: {}, price: {}, funds: {}",
                    candle.ts, current_price, funds
                );
            }
        }

        if position > 0.0 {
            // 如果最后还有多仓未平仓，按最后一个价格平仓
            if let Some(last_candle) = candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
                    eprintln!("Failed to parse price: {}", e);
                    0.0
                });
                funds = position * last_price;
                position = 0.0;
                println!("Final sell at price: {}, funds: {}", last_price, funds);
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

        println!("Final Win rate: {}", win_rate);
        (funds, win_rate, open_trades)
    }

    // pub async fn ut_bot_alert_strategy_all(&mut self, candles_5m: &[CandlesEntity], key_value: f64, atr_period: usize, heikin_ashi: bool) -> (f64, f64, usize) {
    //     let initial_funds = 100.0; // 初始资金
    //     let mut funds = initial_funds; // 当前资金
    //     let mut position: f64 = 0.0; // 当前持仓量,显式指定为 f64 类型
    //     let mut wins = 0; // 赢的次数
    //     let mut losses = 0; // 输的次数
    //     let mut open_trades = 0; // 开仓次数
    //     let mut entry_price = 0.0; // 记录每次开仓时的价格
    //     let fib_levels = [0.0236, 0.0382, 0.05, 0.0618, 0.0786, 0.1]; // 斐波那契回撤级别
    //     let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发
    //     let mut atr = AverageTrueRange::new(atr_period).unwrap(); // 初始化ATR指标
    //     let mut ema = ExponentialMovingAverage::new(1).unwrap(); // 初始化EMA指标
    //     let mut xatr_trailing_stop = 0.0; // 初始化xATRTrailingStop变量
    //     let mut pos = 0; // 初始化pos变量
    //     let mut prev_ema_value = 0.0; // 用于保存前一个EMA值
    //     let mut trade_completed = true; // 交易完成标志
    //     let max_loss_percent = 0.1; // 最大损失百分比设置为10%

    //     for (i, candle) in candles_5m.iter().enumerate() {
    //         let current_price = if heikin_ashi {
    //             // 如果使用平均K线,则计算平均K线的收盘价
    //             let open = candle.o.parse::<f64>().unwrap_or(0.0);
    //             let high = candle.h.parse::<f64>().unwrap_or(0.0);
    //             let low = candle.l.parse::<f64>().unwrap_or(0.0);
    //             let close = candle.c.parse::<f64>().unwrap_or(0.0);
    //             (open + high + low + close) / 4.0
    //         } else {
    //             candle.c.parse::<f64>().unwrap_or(0.0)
    //         };

    //         let high_price = candle.h.parse::<f64>().unwrap_or(0.0);
    //         let low_price = candle.l.parse::<f64>().unwrap_or(0.0);

    //         let prev_xatr_trailing_stop = if i > 0 { xatr_trailing_stop } else { 0.0 };

    //         let n_loss = key_value * atr.next(&KdjCandle { high: high_price, low: low_price, close: current_price });

    //         xatr_trailing_stop = if i == 0 {
    //             current_price
    //         } else if current_price > prev_xatr_trailing_stop && candles_5m[i - 1].c.parse::<f64>().unwrap_or(0.0) > prev_xatr_trailing_stop {
    //             let new_stop = current_price - n_loss;
    //             prev_xatr_trailing_stop.max(new_stop)
    //         } else if current_price < prev_xatr_trailing_stop && candles_5m[i - 1].c.parse::<f64>().unwrap_or(0.0) < prev_xatr_trailing_stop {
    //             let new_stop = current_price + n_loss;
    //             prev_xatr_trailing_stop.min(new_stop)
    //         } else if current_price > prev_xatr_trailing_stop {
    //             let new_stop = current_price - n_loss;
    //             new_stop
    //         } else {
    //             let new_stop = current_price + n_loss;
    //             new_stop
    //         };

    //         pos = if i == 0 {
    //             0
    //         } else if candles_5m[i - 1].c.parse::<f64>().unwrap_or(0.0) < prev_xatr_trailing_stop && current_price > xatr_trailing_stop {
    //             1
    //         } else if candles_5m[i - 1].c.parse::<f64>().unwrap_or(0.0) > prev_xatr_trailing_stop && current_price < xatr_trailing_stop {
    //             -1
    //         } else {
    //             pos
    //         };

    //         let ema_value = ema.next(current_price);
    //         let above = ema_value > xatr_trailing_stop && prev_ema_value <= prev_xatr_trailing_stop;
    //         let below = ema_value < xatr_trailing_stop && prev_ema_value >= prev_xatr_trailing_stop;
    //         prev_ema_value = ema_value; // 保存当前EMA值为下一次迭代的前一个EMA值

    //         let buy = current_price > xatr_trailing_stop && above;
    //         let sell = current_price < xatr_trailing_stop && below;

    //         // 添加日志记录
    //         info!("Time: {:?},funds:{}, Price: {}, EMA: {}, xATRTrailingStop: {}, Buy: {}, Sell: {}",
    //     time_util::mill_time_to_datetime_shanghai(candle.ts),funds, current_price, ema_value, xatr_trailing_stop, buy, sell);

    //         if buy && position.abs() < f64::EPSILON && trade_completed {
    //             position = funds / current_price;
    //             entry_price = current_price; // 记录开仓价格
    //             funds = 0.0;
    //             open_trades += 1;
    //             fib_triggered = [false; 6]; // 重置斐波那契触发标记
    //             trade_completed = false; // 标记交易未完成
    //             info!("Buy at time: {:?}, price: {}, position: {}, funds after buy: {}", time_util::mill_time_to_datetime_shanghai(candle.ts), current_price, position, funds);
    //         } else if (sell || current_price < entry_price * (1.0 - max_loss_percent)) && position > 0.0 {
    //             funds += position * current_price; // 累加当前平仓收益
    //             position = 0.0;
    //             trade_completed = true; // 标记交易完成
    //             info!("Sell (close long) at time: {:?}, price: {}, funds after sell: {}", time_util::mill_time_to_datetime_shanghai(candle.ts), current_price, funds);
    //             if funds > initial_funds {
    //                 wins += 1;
    //             } else {
    //                 losses += 1;
    //             }
    //         } else if position > 0.0 {
    //             // 斐波那契止盈逻辑
    //             let mut remaining_position = position;
    //             for (idx, &level) in fib_levels.iter().enumerate() {
    //                 let fib_price = entry_price * (1.0 + level); // 计算斐波那契目标价格
    //                 if current_price >= fib_price && !fib_triggered[idx] {
    //                     let sell_amount = remaining_position * 0.1; // 按仓位的10%
    //                     if sell_amount < 1e-8 { // 防止非常小的数值
    //                         continue;
    //                     }
    //                     funds += sell_amount * current_price; // 累加当前平仓收益
    //                     remaining_position -= sell_amount;
    //                     fib_triggered[idx] = true; // 记录该斐波那契级别已经触发
    //                     info!("Fibonacci profit taking at level: {:?}, time:{}, price: {}, sell amount: {}, remaining position: {}, funds after profit taking: {}",time_util::mill_time_to_datetime_shanghai(candle.ts),level, current_price, sell_amount, remaining_position, funds);
    //                     // 如果剩余仓位为零，更新win或loss
    //                     if remaining_position <= 1e-8 {
    //                         position = 0.0;
    //                         trade_completed = true; // 标记交易完成
    //                         if funds > initial_funds {
    //                             wins += 1;
    //                         } else {
    //                             losses += 1;
    //                         }
    //                         break;
    //                     }
    //                 }
    //             }
    //             // 更新持仓
    //             position = remaining_position;
    //         }
    //     }

    //     if position > 0.0 {
    //         if let Some(last_candle) = candles_5m.last() {
    //             let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
    //                 error!("Failed to parse price: {}", e);
    //                 0.0
    //             });
    //             funds += position * last_price; // 累加当前平仓收益
    //             position = 0.0;
    //             trade_completed = true; // 标记交易完成
    //             info!("Final sell at price: {}, funds after final sell: {}", last_price, funds);
    //             if funds > initial_funds {
    //                 wins += 1;
    //             } else {
    //                 losses += 1;
    //             }
    //         }
    //     }

    //     let win_rate = if wins + losses > 0 {
    //         wins as f64 / (wins + losses) as f64
    //     } else {
    //         0.0
    //     }; // 计算胜率

    //     info!("Final Win rate: {}", win_rate);
    //     (funds, win_rate, open_trades) // 返回最终资金,胜率和开仓次数
    // }

    // pub async fn ut_bot_alert_strategy_with_shorting(&mut self, candles_5m: &Vec<CandlesEntity>, fib_levels: &Vec<f64>, key_value: f64, atr_period: usize, heikin_ashi: bool) -> (f64, f64, usize) {
    //     let initial_funds = 100.0; // 初始资金
    //     let mut funds = initial_funds; // 当前资金
    //     let mut position: f64 = 0.0; // 当前持仓量, 显式指定为 f64 类型
    //     let mut wins = 0; // 赢的次数
    //     let mut losses = 0; // 输的次数
    //     let mut open_trades = 0; // 开仓次数
    //     let mut entry_price = 0.0; // 记录每次开仓时的价格
    //     let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发
    //     let mut trade_completed = true; // 交易完成标志
    //     let max_loss_percent = 0.1; // 最大损失百分比设置为10%
    //
    //     for (i, candle) in candles_5m.iter().enumerate() {
    //         let signal = ut_boot_strategy::UtBootStrategy::get_trade_signal(&candles_5m[..=i], key_value, atr_period, heikin_ashi);
    //
    //         // 添加日志记录
    //         info!("Time: {:?}, funds: {}, Price: {}, Buy: {}, Sell: {}, key_value: {}, atr_period: {}",
    //         time_util::mill_time_to_datetime_shanghai(candle.ts), funds, signal.price, signal.should_buy, signal.should_sell, key_value, atr_period);
    //
    //         if signal.should_sell && position.abs() < f64::EPSILON && trade_completed {
    //             // 做空逻辑
    //             position = funds / signal.price;
    //             entry_price = signal.price; // 记录开仓价格
    //             funds = 0.0;
    //             open_trades += 1;
    //             fib_triggered = [false; 6]; // 重置斐波那契触发标记
    //             trade_completed = false; // 标记交易未完成
    //             info!("Short at time: {:?}, price: {}, position: {}, funds after short: {}",
    //             time_util::mill_time_to_datetime_shanghai(candle.ts), signal.price, position, funds);
    //         } else if (signal.should_buy || signal.price > entry_price * (1.0 + max_loss_percent)) && position > 0.0 {
    //             // 平仓逻辑
    //             funds += position * (2.0 * entry_price - signal.price); // 计算做空平仓收益
    //             position = 0.0;
    //             trade_completed = true; // 标记交易完成
    //             info!("Cover (close short) at time: {:?}, price: {}, funds after cover: {}",
    //             time_util::mill_time_to_datetime_shanghai(candle.ts), signal.price, funds);
    //             if funds > initial_funds {
    //                 wins += 1;
    //             } else {
    //                 losses += 1;
    //             }
    //         } else if position > 0.0 {
    //             // 斐波那契止盈逻辑
    //             let mut remaining_position = position;
    //             for (idx, &level) in fib_levels.iter().enumerate() {
    //                 let fib_price = entry_price * (1.0 - level); // 计算斐波那契目标价格
    //                 if signal.price <= fib_price && !fib_triggered[idx] {
    //                     let cover_amount = remaining_position * 0.1; // 按仓位的10%
    //                     if cover_amount < 1e-8 { // 防止非常小的数值
    //                         continue;
    //                     }
    //                     funds += cover_amount * (2.0 * entry_price - signal.price); // 计算做空平仓收益
    //                     remaining_position -= cover_amount;
    //                     fib_triggered[idx] = true; // 记录该斐波那契级别已经触发
    //                     info!("Fibonacci profit taking at level: {:?}, time:{}, price: {}, cover amount: {}, remaining position: {}, funds after profit taking: {}",
    //                     time_util::mill_time_to_datetime_shanghai(candle.ts), level, signal.price, cover_amount, remaining_position, funds);
    //                     // 如果剩余仓位为零，更新win或loss
    //                     if remaining_position <= 1e-8 {
    //                         position = 0.0;
    //                         trade_completed = true; // 标记交易完成
    //                         if funds > initial_funds {
    //                             wins += 1;
    //                         } else {
    //                             losses += 1;
    //                         }
    //                         break;
    //                     }
    //                 }
    //             }
    //             // 更新持仓
    //             position = remaining_position;
    //         }
    //     }
    //
    //     if position > 0.0 {
    //         if let Some(last_candle) = candles_5m.last() {
    //             let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
    //                 error!("Failed to parse price: {}", e);
    //                 0.0
    //             });
    //             funds += position * (2.0 * entry_price - last_price); // 计算做空平仓收益
    //             position = 0.0;
    //             trade_completed = true; // 标记交易完成
    //             info!("Final cover at price: {}, funds after final cover: {}", last_price, funds);
    //             if funds > initial_funds {
    //                 wins += 1;
    //             } else {
    //                 losses += 1;
    //             }
    //         }
    //     }
    //
    //     let win_rate = if wins + losses > 0 {
    //         wins as f64 / (wins + losses) as f64
    //     } else {
    //         0.0
    //     }; // 计算胜率
    //
    //     info!("Final Win rate: {}", win_rate);
    //     (funds, win_rate, open_trades) // 返回最终资金,胜率和开仓次数
    // }

    pub async fn short_strategy(
        &self,
        candles: &[CandlesEntity],
        breakout_period: usize,
        confirmation_period: usize,
        volume_threshold: f64,
        stop_loss_strategy: StopLossStrategy,
    ) -> (f64, f64) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut short_position = 0.0; // 做空持仓
        let mut wins = 0;
        let mut losses = 0;
        let mut entry_price = 0.0; // 记录每次开仓时的价格
        let mut entry_highest_price = 0.0; // 记录开仓K线的最高价
        const FIB_LEVELS: [f64; 6] = [0.0236, 0.0382, 0.05, 0.0618, 0.0786, 0.1]; // 斐波那契回撤级别
        let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发

        for i in breakout_period..candles.len() {
            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);
            let volume = candles[i].vol.parse::<f64>().unwrap_or(0.0);
            let timestamp = time_util::mill_time_to_datetime_shanghai(candles[i].ts).unwrap();

            // 计算突破信号
            let highest_high = candles[i - breakout_period..i]
                .iter()
                .map(|c| c.c.parse::<f64>().unwrap_or(0.0))
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();
            let lowest_low = candles[i - breakout_period..i]
                .iter()
                .map(|c| c.c.parse::<f64>().unwrap_or(0.0))
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            // 计算前几个周期的平均成交量
            let avg_volume: f64 = candles[i - breakout_period..i]
                .iter()
                .map(|c| c.vol.parse::<f64>().unwrap_or(0.0))
                .sum::<f64>()
                / breakout_period as f64;

            // 检查是否发生假跌破
            if price < lowest_low && short_position == 0.0 && volume > avg_volume * volume_threshold
            {
                // 确认跌破
                let mut valid_breakdown = true;
                for j in 1..confirmation_period {
                    if i + j < candles.len() {
                        let confirm_price = candles[i + j].c.parse::<f64>().unwrap_or(0.0);
                        let confirm_volume = candles[i + j].vol.parse::<f64>().unwrap_or(0.0);
                        if confirm_price >= lowest_low
                            || confirm_volume <= avg_volume * volume_threshold
                        {
                            valid_breakdown = false;
                            break;
                        }
                    }
                }
                if valid_breakdown {
                    // 确认跌破下轨，开空
                    short_position = funds / price;
                    entry_price = price; // 记录开仓价格
                    entry_highest_price = highest_high; // 记录开仓K线的最高价
                    funds = 0.0;
                    fib_triggered = [false; 6]; // 重置斐波那契触发标记
                    info!(
                        "Breakdown Short  buy at time: {}, price: {}, position: {}",
                        timestamp, price, short_position
                    );
                }
            } else if short_position > 0.0 {
                // 计算当前空头持仓的价值
                let current_value = short_position * price;

                // 止损逻辑
                let stop_loss_triggered = match stop_loss_strategy {
                    StopLossStrategy::Amount(stop_loss_amount) => {
                        current_value > entry_price * short_position + stop_loss_amount
                    }
                    StopLossStrategy::Percent(stop_loss_percent) => {
                        current_value > entry_price * short_position * (1.0 + stop_loss_percent)
                    }
                };

                // 如果价格高于开仓K线的最高价，则触发止损
                // let price_stop_loss_triggered = price > entry_highest_price;
                let price_stop_loss_triggered = false;

                if stop_loss_triggered || price_stop_loss_triggered {
                    // 止损买入
                    funds = current_value;
                    short_position = 0.0;
                    losses += 1; // 更新亏损计数
                    info!(
                        "Stop loss (short) sell at time: {}, price: {}, funds: {}",
                        timestamp, price, funds
                    );
                    continue;
                }

                // 斐波那契止盈逻辑
                let mut remaining_position = short_position;
                for (idx, &level) in FIB_LEVELS.iter().enumerate() {
                    let fib_price = entry_price * (1.0 - level); // 计算斐波那契目标价格
                    if price <= fib_price && !fib_triggered[idx] {
                        let buy_amount = remaining_position * 0.1; // 例如每次买回 10% 的仓位
                        if buy_amount < 1e-8 {
                            // 防止非常小的数值
                            continue;
                        }
                        funds += buy_amount * price;
                        remaining_position -= buy_amount;
                        fib_triggered[idx] = true; // 记录该斐波那契级别已经触发
                        info!(
                        "Fibonacci profit taking at level: {}, price: {}, buy amount: {}, remaining position: {}",
                        level, price, buy_amount, remaining_position
                    );

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

        // 如果最后一次操作是买入，则在最后一个收盘价卖出
        if short_position > 0.0 {
            if let Some(last_candle) = candles.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
                funds = short_position * last_price;
                short_position = 0.0;
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
                info!(
                    "Final buy to close short at price: {}, funds: {}",
                    last_price, funds
                );
            }
        }

        let win_rate = if wins + losses > 0 {
            wins as f64 / (wins + losses) as f64
        } else {
            0.0
        };

        info!("Final results: funds: {}, win_rate: {}", funds, win_rate);

        (funds, win_rate)
    }

    pub async fn breakout_strategy(
        &self,
        candles: &[CandlesEntity],
        breakout_period: usize,
        confirmation_period: usize,
        volume_threshold: f64,
        stop_loss_strategy: StopLossStrategy,
    ) -> (f64, f64, i32) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position = 0.0;
        let mut wins = 0;
        let mut open_positon_nums = 0;
        let mut losses = 0;
        let mut entry_price = 0.0; // 记录每次开仓时的价格
        let fib_levels = [0.00436, 0.00682, 0.01, 0.01218, 0.01486, 0.02]; // 斐波那契回撤级别
        let mut fib_triggered = [false; 6]; // 用于记录每个斐波那契级别是否已经触发

        for i in breakout_period..candles.len() {
            let price = candles[i].c.parse::<f64>().unwrap_or(0.0);
            let volume = candles[i].vol.parse::<f64>().unwrap_or(0.0); // 假设 Candle 结构体包含成交量字段 `vol`
            let timestamp = time_util::mill_time_to_datetime_shanghai(candles[i].ts).unwrap();

            // 计算突破信号
            let highest_high = candles[i - breakout_period..i]
                .iter()
                .map(|c| c.c.parse::<f64>().unwrap_or(0.0))
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();
            let lowest_low = candles[i - breakout_period..i]
                .iter()
                .map(|c| c.c.parse::<f64>().unwrap_or(0.0))
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            // 计算前几个周期的平均成交量
            let avg_volume: f64 = candles[i - breakout_period..i]
                .iter()
                .map(|c| c.vol.parse::<f64>().unwrap_or(0.0))
                .sum::<f64>()
                / breakout_period as f64;

            // 检查是否发生假突破
            if price > highest_high && position == 0.0 && volume > avg_volume * volume_threshold {
                // 确认突破
                let mut valid_breakout = true;
                for j in 1..confirmation_period {
                    if i + j < candles.len() {
                        let confirm_price = candles[i + j].c.parse::<f64>().unwrap_or(0.0);
                        let confirm_volume = candles[i + j].vol.parse::<f64>().unwrap_or(0.0);
                        if confirm_price <= highest_high
                            || confirm_volume <= avg_volume * volume_threshold
                        {
                            valid_breakout = false;
                            break;
                        }
                    }
                }
                if valid_breakout {
                    open_positon_nums += 1;
                    // 确认突破上轨，买入
                    position = funds / price;
                    entry_price = price; // 记录开仓价格
                    funds = 0.0;
                    fib_triggered = [false; 6]; // 重置斐波那契触发标记
                    info!(
                        "Breakout Buy at time: {}, price: {}, position: {}",
                        timestamp, price, position
                    );
                }
            } else if price < lowest_low && position > 0.0 && volume > avg_volume * volume_threshold
            {
                // 确认跌破，卖出
                let mut valid_breakdown = true;
                for j in 1..confirmation_period {
                    if i + j < candles.len() {
                        let confirm_price = candles[i + j].c.parse::<f64>().unwrap_or(0.0);
                        let confirm_volume = candles[i + j].vol.parse::<f64>().unwrap_or(0.0);
                        if confirm_price >= lowest_low
                            || confirm_volume <= avg_volume * volume_threshold
                        {
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
                    info!(
                        "Breakout Sell at time: {}, price: {}, funds: {}",
                        timestamp, price, funds
                    );
                }
            } else if position > 0.0 {
                // 计算当前持仓的价值
                let current_value = position * price;

                // 止损逻辑
                let stop_loss_triggered = match stop_loss_strategy {
                    StopLossStrategy::Amount(stop_loss_amount) => {
                        current_value < entry_price * position - stop_loss_amount
                    }
                    StopLossStrategy::Percent(stop_loss_percent) => {
                        current_value < entry_price * position * (1.0 - stop_loss_percent)
                    }
                };

                if stop_loss_triggered {
                    // 止损卖出
                    funds = current_value;
                    position = 0.0;
                    losses += 1; // 更新亏损计数
                    info!(
                        "Stop loss at time: {}, price: {}, funds: {}",
                        timestamp, price, funds
                    );
                    continue;
                }

                // 斐波那契止盈逻辑
                let mut remaining_position = position;
                for (idx, &level) in fib_levels.iter().enumerate() {
                    let fib_price = entry_price * (1.0 + level); // 计算斐波那契目标价格
                    if price >= fib_price && !fib_triggered[idx] {
                        let sell_amount = remaining_position * 0.1; // 按仓位的10%
                                                                    // let sell_amount = remaining_position * level * 10.00; // 按斐波那契级别的百分比卖出
                        if sell_amount < 1e-8 {
                            // 防止非常小的数值
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
                // 更新持仓
                position = remaining_position;
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

        (funds, win_rate, open_positon_nums)
    }

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
