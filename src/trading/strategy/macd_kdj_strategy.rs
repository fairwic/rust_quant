use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::indicator::kdj_simple_indicator::KDJ;
use crate::trading::indicator::macd_simple_indicator;
use crate::trading::indicator::macd_simple_indicator::MacdSimpleIndicator;
use crate::trading::strategy::profit_stop_loss::ProfitStopLoss;

#[derive(Debug, Deserialize, Serialize)]
pub struct SignalResult {
    pub should_buy: bool,
    pub should_sell: bool,
    pub price: f64,
    pub should_short: bool,
    pub should_cover: bool,
}

pub struct MacdKdjStrategy {}


impl MacdKdjStrategy {
    // 修正KDJ计算逻辑
    pub fn calculate_kdj_with_bcwsma(candles: &[CandlesEntity], period: usize, signal_period: usize) -> Vec<KDJ> {
        let mut kdjs = Vec::with_capacity(candles.len());
        let mut k = 50.0;
        let mut d = 50.0;

        fn bcwsma(s: f64, l: usize, m: f64, prev: f64) -> f64 {
            (m * s + (l as f64 - m) * prev) / l as f64
        }

        for i in 0..candles.len() {
            if i >= period - 1 {
                let slice = &candles[i + 1 - period..=i];
                let (mut high, mut low) = (f64::MIN, f64::MAX);

                for c in slice {
                    let h = c.h.parse::<f64>().unwrap_or(f64::MIN);
                    let l = c.l.parse::<f64>().unwrap_or(f64::MAX);
                    high = high.max(h);
                    low = low.min(l);
                }

                let close = candles[i].c.parse::<f64>().unwrap_or(0.0);
                let rsv = if high == low {
                    50.0
                } else {
                    (close - low) / (high - low) * 100.0
                };

                // 使用signal_period来平滑K和D值
                k = bcwsma(rsv, signal_period, 1.0, k);
                d = bcwsma(k, signal_period, 1.0, d);
                let j = 3.0 * k - 2.0 * d;
                kdjs.push(KDJ { k, d, j });
            } else {
                kdjs.push(KDJ { k: 50.0, d: 50.0, j: 50.0 });
            }
        }

        kdjs
    }

    pub async fn run_test(candles_5m: &Vec<CandlesEntity>, fib_levels: &Vec<f64>, stop_loss_percent: f64, kdj_period: usize, signal_period: usize) -> (f64, f64, usize) {
        let mut initial_funds = 100.0;
        let mut current_funds = initial_funds;
        let mut position: f64 = 0.0;
        let mut entry_price: f64 = 0.0;
        let mut win_count = 0;
        let mut loss_count = 0;
        let mut trade_count = 0;
        let mut is_long = true;
        let mut total_profit = 0.0;

        let min_data_length = kdj_period.max(26 + 9).max(signal_period);


        for (i, candle) in candles_5m.iter().enumerate() {
            let available_data_length = i + 1;
            if available_data_length < min_data_length {
                continue;
            }
            let signal_data = &candles_5m[i + 1 - min_data_length..=i];
            let signal = Self::get_trade_signal(signal_data, kdj_period, signal_period, stop_loss_percent, position, entry_price, is_long);

            let entry_price_clone = entry_price;  // 克隆 entry_price
            Self::process_signals(&mut current_funds, &mut position, &mut entry_price, &mut is_long, initial_funds, &mut win_count, &mut loss_count, &mut trade_count, &mut total_profit, signal.price, candle.ts, &signal, stop_loss_percent, &fib_levels, entry_price_clone);

            // Self::process_signals(&mut current_funds, &mut position, &mut entry_price, &mut is_long, initial_funds, &mut win_count, &mut loss_count, &mut trade_count, &mut total_profit, signal.price, candle.ts, &signal, stop_loss_percent, &fib_levels, entry_price.clone());
        }

        if position.abs() > 0.0 {
            Self::final_close_trade(candles_5m, &mut current_funds, &mut position, initial_funds, &mut win_count, &mut loss_count, &mut total_profit, is_long, entry_price);
        }

        let win_rate = if win_count + loss_count > 0 {
            win_count as f64 / (win_count + loss_count) as f64
        } else {
            0.0
        };

        info!("Final Win rate: {}, Total Profit: {}", win_rate, total_profit);
        (current_funds, win_rate, trade_count)
    }

    fn get_trade_signal(candles: &[CandlesEntity], kdj_period: usize, signal_period: usize, stop_loss_percent: f64, position: f64, entry_price: f64, is_long: bool) -> SignalResult {
        let macd_values = MacdSimpleIndicator::calculate_macd(candles, 12, 26, 9);
        let kdjs = Self::calculate_kdj_with_bcwsma(candles, kdj_period, signal_period);

        let last_index = candles.len() - 1;
        let current_price = candles[last_index].c.parse::<f64>().unwrap_or(0.0);
        let (_, macd_value, signal_value) = macd_values[last_index];
        let kdj = &kdjs[last_index];

        let macd_above_zero = macd_value > 0.0 && signal_value > 0.0;
        let macd_golden_cross = macd_value > signal_value && kdj.k > kdj.d;
        let kdj_golden_cross = kdj.k > kdj.d;
        let macd_death_cross = macd_value < signal_value;
        let kdj_death_cross = kdj.k < kdj.d;

        let should_buy = is_long && ((macd_golden_cross && kdj_golden_cross) || (macd_above_zero && kdj_golden_cross && !macd_death_cross));
        let should_sell = is_long && ((macd_death_cross && kdj_death_cross) || (!macd_above_zero && kdj_death_cross) || (position > 0.0 && current_price < entry_price * (1.0 - stop_loss_percent)));
        let should_short = !is_long && ((macd_death_cross && kdj_death_cross) || (!macd_above_zero && kdj_death_cross));
        let should_cover = !is_long && ((macd_golden_cross && kdj_golden_cross) || (position > 0.0 && current_price > entry_price * (1.0 + stop_loss_percent)));
        let false_signal = !macd_above_zero && kdj_golden_cross;

        SignalResult {
            should_buy: should_buy && position.abs() < f64::EPSILON && !false_signal,
            should_sell: position > 0.0 && should_sell,
            should_short: should_short && position.abs() < f64::EPSILON,
            should_cover: position > 0.0 && should_cover,
            price: current_price,
        }
    }

    // 修改process_signals函数以考虑盈亏金额和斐波那契级别
    fn process_signals(
        funds: &mut f64,
        position: &mut f64,
        entry_price: &mut f64,
        is_long: &mut bool,
        initial_funds: f64,
        wins: &mut usize,
        losses: &mut usize,
        trades: &mut usize,
        total_profit: &mut f64,
        current_price: f64,
        timestamp: i64,
        signal: &SignalResult,
        stop_loss_percent: f64,
        fib_levels: &Vec<f64>,
        original_entry_price: f64,
    ) {
        if signal.should_buy {
            *position = *funds / current_price;
            *entry_price = current_price;
            *funds = 0.0;
            *is_long = true;
            *trades += 1;
            info!("Buy at time: {}, price: {}, position: {}", timestamp, current_price, *position);
        } else if signal.should_sell {
            *funds = *position * current_price;
            let profit = *funds - initial_funds;
            *total_profit += profit;
            *position = 0.0;
            info!("Sell at time: {}, price: {}, funds: {}, profit: {}", timestamp, current_price, *funds, profit);
            if profit > 0.0 {
                *wins += 1;
            } else {
                *losses += 1;
            }
        } else if signal.should_short {
            *position = *funds / current_price;
            *entry_price = current_price;
            *funds = 0.0;
            *is_long = false;
            *trades += 1;
            info!("Short at time: {}, price: {}, position: {}", timestamp, current_price, *position);
        } else if signal.should_cover {
            *funds = *position * (2.0 * *entry_price - current_price);
            let profit = *funds - initial_funds;
            *total_profit += profit;
            *position = 0.0;
            info!("Cover at time: {}, price: {}, funds: {}, profit: {}", timestamp, current_price, *funds, profit);
            if profit > 0.0 {
                *wins += 1;
            } else {
                *losses += 1;
            }
        } else if *position > 0.0 {
            for &level in fib_levels.iter() {
                if *is_long && current_price >= original_entry_price * (1.0 + level) {
                    let sell_amount = *position * 0.1;
                    *funds += sell_amount * current_price;
                    *position -= sell_amount;
                    info!("Fibonacci level reached. Partial sell at time: {}, price: {}, amount: {}, funds: {}, position: {}", timestamp, current_price, sell_amount, *funds, *position);
                } else if !*is_long && current_price <= original_entry_price * (1.0 - level) {
                    let cover_amount = *position * 0.1;
                    *funds += cover_amount * (2.0 * original_entry_price - current_price);
                    *position -= cover_amount;
                    info!("Fibonacci level reached. Partial cover at time: {}, price: {}, amount: {}, funds: {}, position: {}", timestamp, current_price, cover_amount, *funds, *position);
                }
            }
            debug!("Hold position (possible shakeout) at time: {}", timestamp);
        }
    }

    // 修改final_close_trade函数以考虑盈亏金额
    fn final_close_trade(
        candles_5m: &[CandlesEntity],
        funds: &mut f64,
        position: &mut f64,
        initial_funds: f64,
        wins: &mut usize,
        losses: &mut usize,
        total_profit: &mut f64,
        is_long: bool,
        entry_price: f64,
    ) {
        if let Some(last_candle) = candles_5m.last() {
            let last_price = last_candle.c.parse::<f64>().unwrap_or(0.0);
            if is_long {
                *funds = *position * last_price;
            } else {
                *funds = *position * (2.0 * entry_price - last_price);
            }
            let profit = *funds - initial_funds;
            *total_profit += profit;
            *position = 0.0;
            info!("Final {} at price: {}, funds: {}, profit: {}", if is_long { "sell" } else { "cover" }, last_price, *funds, profit);
            if profit > 0.0 {
                *wins += 1;
            } else {
                *losses += 1;
            }
        }
    }
}
