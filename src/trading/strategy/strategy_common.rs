use serde::{Deserialize, Serialize};
use tracing::{info, error};
use crate::time_util;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::ut_boot_strategy::TradeRecord;
use std::collections::HashSet;
use crate::trading::okx::trade::{PosSide, Side};

#[derive(Debug, Deserialize, Serialize)]
pub struct SignalResult {
    pub should_buy: bool,
    pub should_sell: bool,
    pub price: f64,
    pub ts: i64,
}

fn record_trade(
    trade_records: &mut Vec<TradeRecord>,
    option_type: &str,
    open_position_time: &str,
    close_position_time: Option<String>,
    open_price: f64,
    close_price: f64,
    profit_loss: f64,
    quantity: f64,
    full_close: bool,
    close_type: &str,
    win_num: i64,
    loss_num: i64,
) {
    trade_records.push(TradeRecord {
        option_type: option_type.to_string(),
        open_position_time: open_position_time.to_string(),
        close_position_time,
        open_price,
        close_price,
        profit_loss,
        quantity,
        full_close,
        close_type: close_type.to_string(),
        win_num,
        loss_num,
    });
}

fn parse_price(candle: &CandlesEntity) -> f64 {
    candle.c.parse::<f64>().unwrap_or_else(|e| {
        error!("Failed to parse price: {}", e);
        0.0
    })
}

fn calculate_profit_loss(
    is_long: bool,
    position: f64,
    entry_price: f64,
    exit_price: f64,
) -> f64 {
    if is_long {
        position * (exit_price - entry_price)
    } else {
        position * (entry_price - exit_price)
    }
}

/// 处理斐波那契部分止盈逻辑
pub fn process_fibonacci_levels(
    current_candle: &CandlesEntity,
    funds: &mut f64,
    position: &mut f64,
    entry_price: f64,
    signal: &SignalResult,
    fib_levels: &[f64],
    feibon_profil_levels: &[f64],
    entry_time: &str,
    ts: &i64,
    total_profit_loss: &mut f64,
    trade_records: &mut Vec<TradeRecord>,
    triggered_fib_levels: &mut HashSet<usize>, // 用于记录已触发的斐波那契级别的索引
    is_long: bool, // 是否为做多
    wins: &mut i64,
    losses: &mut i64,
) -> f64 {
    // println!(" 判断斐波那契止损");

    let mut remaining_position = *position;
    // println!("fib_levels:{:#?}", fib_levels);
    for (idx, &level) in fib_levels.iter().enumerate() {
        if triggered_fib_levels.contains(&idx) {
            continue; // 如果该斐波那契级别已触发，则跳过
        }
        let fib_price = if is_long {
            entry_price * (1.0 + level) // 做多情况下的斐波那契目标价格
        } else {
            entry_price * (1.0 - level) // 做空情况下的斐波那契目标价格
        };

        println!("signal.price:{},fib_price:{},level:{}", signal.price, fib_price, level);

        if (is_long && signal.price >= fib_price) || (!is_long && signal.price <= fib_price) {
            // println!(" 触发斐波那契止损，fib_level:{},price:{}", level, signal.price);
            let sell_amount = *position * feibon_profil_levels[idx]; // 按斐波那契级别的比例止盈
            // println!(" 原来数量:{}", *position);
            if sell_amount < 1e-8 { // 防止非常小的数值
                continue;
            }
            if is_long {
                *funds += sell_amount * (fib_price - entry_price); // 做多情况下累加当前平仓收益
            } else {
                *funds += sell_amount * (entry_price - fib_price); // 做空情况下累加当前平仓收益
            }
            // println!("after fib founds:{}", *funds);
            remaining_position -= sell_amount;
            // 如果减去卖出仓位之后，
            if remaining_position <= 1e-8 {
                // 剩余仓位为零
                close_remaining_position(
                    &entry_price,
                    funds,
                    position,
                    total_profit_loss,
                    trade_records,
                    &current_candle,
                    entry_time,
                    wins,
                    losses,
                    triggered_fib_levels, // 传入已触发的斐波那契级别的索引集合
                    is_long, // 是否为做多,
                );
                continue; // 结束函数执行，因为仓位已经为空
            } else {
                let exit_time = time_util::mill_time_to_datetime(*ts).unwrap();

                let profit_loss = if is_long {
                    sell_amount * (fib_price - entry_price) // 做多情况下计算该次部分止盈的利润
                } else {
                    sell_amount * (entry_price - fib_price) // 做空情况下计算该次部分止盈的利润
                };
                *total_profit_loss += profit_loss; // 累计总的盈利或损失


                println!("记录平仓记录:signal_price:{},open_price:{}", signal.price, entry_price);

                trade_records.push(TradeRecord {
                    option_type: "fibonacci_close".to_string(),
                    open_position_time: entry_time.to_string(),
                    close_position_time: Some(exit_time),
                    open_price: entry_price,
                    close_price: fib_price,
                    profit_loss: *total_profit_loss,
                    quantity: sell_amount,
                    full_close: remaining_position <= 1e-8,
                    close_type: "斐波那契止盈".to_string(),
                    win_num: 0,
                    loss_num: 0,
                });
                triggered_fib_levels.insert(idx); // 标记该斐波那契级别已触发
                info!(
                "Fibonacci profit taking at level: {:?}, time: {:?}, price: {}, sell amount: {}, remaining position: {}, funds after profit taking: {}",
                level, time_util::mill_time_to_datetime_shanghai(*ts), signal.price, sell_amount, remaining_position, *funds
            );
            }
        }
    }
    remaining_position
}

/// 平仓剩余仓位
pub fn close_remaining_position(
    entry_price: &f64,
    funds: &mut f64,
    position: &mut f64,
    total_profit_loss: &mut f64,
    trade_records: &mut Vec<TradeRecord>,
    current_candle: &CandlesEntity,
    entry_time: &str,
    wins: &mut i64,
    losses: &mut i64,
    triggered_fib_levels: &mut HashSet<usize>, // 传入已触发的斐波那契级别的索引集合
    is_long: bool, // 是否为做多
) {
    let last_price = parse_price(current_candle);
    let exit_time = time_util::mill_time_to_datetime(current_candle.ts).unwrap();

    let current_profit_loss = if is_long {
        *position * (last_price - *entry_price) // 做多情况下计算当前价值
    } else {
        *position * (*entry_price - last_price) // 做空情况下
    };

    *funds += current_profit_loss; // 做多情况下添加到资金

    *total_profit_loss += current_profit_loss; // 计总的利或失

    if *total_profit_loss > 0.0 {
        *wins += 1;
    } else {
        *losses += 1;
    }

    trade_records.push(TradeRecord {
        option_type: "close".to_string(),
        open_position_time: entry_time.to_string(),
        close_position_time: Some(exit_time),
        open_price: *entry_price, // 在最终平仓时记录开仓价格
        close_price: last_price,
        profit_loss: *total_profit_loss,
        quantity: *position,
        full_close: true,
        close_type: if is_long { "止盈".to_string() } else { "止损".to_string() },
        win_num: *wins,
        loss_num: *losses,
    });
    *position = 0.0;

    // info!("Final sell at price: {}, funds after final sell: {}, profit/loss: {}",last_price, *funds, *total_profit_loss);

    triggered_fib_levels.clear(); // 重置斐波那契级别触发记录
}

/// 运行回测
pub fn run_test(
    strategy: impl Fn(&[CandlesEntity]) -> SignalResult,
    candles_5m: &[CandlesEntity],
    fib_levels: &[f64],
    max_loss_percent: f64,
    min_data_length: usize,
    is_need_fibonacci_profit: bool,
    is_open_long: bool,
    is_open_short: bool,
    is_judge_trade_time: bool,
) -> (f64, f64, usize, Vec<TradeRecord>) {
    let initial_funds = 100.0;
    let mut funds = initial_funds;
    let mut position: f64 = 0.0;
    let mut wins: i64 = 0;
    let mut losses: i64 = 0;
    let mut open_trades = 0;
    let mut entry_price = 0.0;
    let mut is_long = true; // 标记当前持仓是否为做多
    let feibon_profil_levels = vec![0.236, 0.382, 0.500, 0.618, 0.786, 1.0];

    let mut trade_records = Vec::new();
    let mut entry_time = String::new();
    let mut initial_quantity = 0.0;
    let mut total_profit_loss = 0.0; // 总的盈利或损失
    let mut triggered_fib_levels = HashSet::new(); // 用于记录已触发的斐波那契级别

    for (i, candle) in candles_5m.iter().enumerate() {
        if i + 1 < min_data_length {
            continue; // 确保有足够的K线数据
        }
        let signal_data = &candles_5m[i + 1 - min_data_length..=i];
        //调用函数,获取信号
        let signal = strategy(signal_data);

        // info!("ts:{},Time: {:?}, funds: {}, Price: {}, Buy: {}, Sell: {}",candle.ts,time_util::mill_time_to_datetime_shanghai(candle.ts),funds,signal.price,signal.should_buy,signal.should_sell);

        if signal.should_buy {
            if position > 0.0 {
                if !is_long {
                    // 平掉所有空单
                    // println!("平掉所有空单");
                    close_remaining_position(
                        &entry_price,
                        &mut funds,
                        &mut position,
                        &mut total_profit_loss,
                        &mut trade_records,
                        &candles_5m[i],
                        &entry_time,
                        &mut wins,
                        &mut losses,
                        &mut triggered_fib_levels,
                        is_long,
                    );
                } else {
                    // println!("已经存在多单")
                    continue;
                }
            } else {
                if is_open_long {
                    //如果需要判断开仓时间，且当前时间不在开仓时间范围内
                    if is_judge_trade_time && !time_util::is_within_business_hours(candle.ts) {
                        continue;
                    }
                    // 开多仓
                    position = funds / signal.price;
                    initial_quantity = position;
                    entry_price = signal.price;
                    entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
                    open_trades += 1;
                    total_profit_loss = 0.0;
                    is_long = true;
                    // info!("Buy at time: {:?}, price: {}, position: {}, funds after buy: {}",entry_time, signal.price, position, funds);

                    trade_records.push(TradeRecord {
                        option_type: PosSide::LONG.to_string(),
                        open_position_time: entry_time.clone(),
                        close_position_time: Some(entry_time.clone()),
                        open_price: entry_price,
                        close_price: signal.price,
                        profit_loss: total_profit_loss,
                        quantity: initial_quantity,
                        full_close: false,
                        close_type: "".to_string(),
                        win_num: 0,
                        loss_num: 0,
                    });
                }
            }
        } else if signal.should_sell {
            if position > 0.0 {
                if is_long {
                    // 平掉所有多单
                    // println!("平掉所有多单");
                    close_remaining_position(
                        &entry_price,
                        &mut funds,
                        &mut position,
                        &mut total_profit_loss,
                        &mut trade_records,
                        &candles_5m[i],
                        &entry_time,
                        &mut wins,
                        &mut losses,
                        &mut triggered_fib_levels,
                        is_long,
                    );
                } else {
                    // println!("已经存在空单")
                    continue;
                }
            } else {
                if is_open_short {
                    //如果需要判断开仓时间，且当前时间不在开仓时间范围内
                    if is_judge_trade_time && !time_util::is_within_business_hours(candle.ts) {
                        continue;
                    }
                    // 开空仓
                    position = funds / signal.price;
                    initial_quantity = position;
                    entry_price = signal.price;
                    entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
                    open_trades += 1;
                    total_profit_loss = 0.0;
                    is_long = false;
                    // info!("Short at time: {:?}, price: {}, position: {}, funds after short: {}", entry_time, signal.price, position, funds);

                    trade_records.push(TradeRecord {
                        option_type: PosSide::SHORT.to_string(),
                        open_position_time: entry_time.clone(),
                        close_position_time: Some(entry_time.clone()),
                        open_price: entry_price,
                        close_price: signal.price,
                        profit_loss: total_profit_loss,
                        quantity: initial_quantity,
                        full_close: false,
                        close_type: "".to_string(),
                        win_num: 0,
                        loss_num: 0,
                    });
                }
            }
        } else if (is_long && (signal.price < entry_price * (1.0 - max_loss_percent)) && position > 0.0)
            || (!is_long && (signal.price > entry_price * (1.0 + max_loss_percent)) && position > 0.0) {
            // 全部平仓
            // println!("触发止损");
            let exit_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
            let current_loss = if is_long {
                position * (signal.price - entry_price)
            } else {
                position * (entry_price - signal.price)
            };
            total_profit_loss += current_loss;
            if total_profit_loss > 0.0 {
                wins += 1;
            } else {
                losses += 1;
            }

            funds += current_loss;
            trade_records.push(TradeRecord {
                option_type: "close".to_string(),
                open_position_time: entry_time.clone(),
                close_position_time: Some(exit_time),
                open_price: entry_price,
                close_price: signal.price,
                profit_loss: total_profit_loss,
                quantity: initial_quantity,
                full_close: true,
                close_type: "止损".to_string(),
                win_num: wins,
                loss_num: losses,
            });
            position = 0.0;
            triggered_fib_levels.clear(); // 重置斐波那契级别触发记录
            // info!("Sell (close long) at time: {:?}, price: {}, funds after sell: {}, profit/loss: {}",entry_time, signal.price, funds, total_profit_loss);
        } else if position > 0.0 {
            if is_need_fibonacci_profit {
                // 斐波那契部分止盈逻辑
                position = process_fibonacci_levels(
                    &candles_5m[i],
                    &mut funds,
                    &mut position,
                    entry_price,
                    &signal,
                    fib_levels,
                    &feibon_profil_levels,
                    &entry_time,
                    &candle.ts,
                    &mut total_profit_loss,
                    &mut trade_records,
                    &mut triggered_fib_levels, // 传入已触发的斐波那契级别的索引集合
                    is_long,
                    &mut wins,
                    &mut losses,
                );
            }
        }
    }

    // 最后一次平仓
    if position > 0.0 {
        // println!("最后k线平仓了");
        close_remaining_position(
            &entry_price,
            &mut funds,
            &mut position,
            &mut total_profit_loss,
            &mut trade_records,
            &candles_5m.last().unwrap(),
            &entry_time,
            &mut wins,
            &mut losses,
            &mut triggered_fib_levels, // 传入已触发的斐波那契级别的索引集合
            is_long,
        );
        // println!("平仓了最终资金: {}", funds);
    }

    let win_rate = if wins + losses > 0 {
        wins as f64 / (wins + losses) as f64
    } else {
        0.0
    };

    // info!("Final Win rate: {}", win_rate);
    (funds, win_rate, open_trades, trade_records)
}
