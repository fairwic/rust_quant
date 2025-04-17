use crate::trading::indicator::ema_indicator::EmaIndicator;
use crate::trading::indicator::rsi_rma_indicator::RsiIndicator;
use crate::trading::indicator::vegas_indicator::{
    EmaSignalValue, IndicatorCombine, VegasIndicatorSignalValue, VegasStrategy,
};
use crate::trading::indicator::volume_indicator::VolumeRatioIndicator;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::okx::trade::{PosSide, Side};
use crate::trading::strategy::top_contract_strategy::{TopContractData, TopContractSingleData};
use crate::{time_util, CandleItem};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;
use ta::indicators::BollingerBands;
use ta::Close;
use ta::DataItem;
use ta::High;
use ta::Low;
use ta::Next;
use ta::Open;
use ta::Volume;
use tracing::span;
use tracing::Level;
use tracing::{error, info};

#[derive(Debug, Deserialize, Serialize)]
pub struct BackTestResult {
    pub funds: f64,
    pub win_rate: f64,
    pub open_trades: usize,
    pub trade_records: Vec<TradeRecord>,
}

impl Default for BackTestResult {
    fn default() -> Self {
        BackTestResult { funds: 0.0, win_rate: 0.0, open_trades: 0, trade_records: vec![] }
    }
}

pub trait BackTestTrait {
    fn to_string();
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TradeRecord {
    pub option_type: String,
    pub open_position_time: String,
    pub close_position_time: Option<String>,
    pub open_price: f64,
    pub close_price: f64,
    pub profit_loss: f64,
    pub quantity: f64,
    pub full_close: bool,
    pub close_type: String,
    pub win_num: i64,
    pub loss_num: i64,
    pub signal_value: Option<String>,
    pub signal_result: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SignalResult {
    pub should_buy: bool,
    pub should_sell: bool,
    //开仓价格
    pub open_price: f64,
    pub ts: i64,
    pub single_value: Option<String>,
    pub single_result: Option<String>,
}

fn parse_price(candle: &CandlesEntity) -> f64 {
    candle.c.parse::<f64>().unwrap_or_else(|e| {
        error!("Failed to parse price: {}", e);
        0.0
    })
}

fn calculate_profit_loss(is_long: bool, position: f64, entry_price: f64, exit_price: f64) -> f64 {
    if is_long {
        position * (exit_price - entry_price)
    } else {
        position * (entry_price - exit_price)
    }
}

/// 处理斐波那契部分止盈逻辑
pub fn process_fibonacci_levels(
    current_candle: &CandleItem,
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
    is_long: bool,                             // 是否为做多
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

        println!(
            "signal.price:{},fib_price:{},level:{}",
            signal.open_price, fib_price, level
        );

        if (is_long && signal.open_price >= fib_price)
            || (!is_long && signal.open_price <= fib_price)
        {
            // println!(" 触发斐波那契止损，fib_level:{},price:{}", level, signal.price);
            let sell_amount = *position * feibon_profil_levels[idx]; // 按斐波那契级别的比例止盈
                                                                     // println!(" 原来数量:{}", *position);
            if sell_amount < 1e-8 {
                // 防止非常小的数值
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
                    current_candle,
                    entry_time,
                    wins,
                    losses,
                    triggered_fib_levels, // 传入已触发的斐波那契级别的索引集合
                    is_long,              // 是否为做多,
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

                println!(
                    "记录平仓记录:signal_price:{},open_price:{}",
                    signal.open_price, entry_price
                );

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
                    signal_value: signal.single_value.clone(),
                    signal_result: signal.single_result.clone(),
                });
                triggered_fib_levels.insert(idx); // 标记该斐波那契级别已触发
                info!(
                "Fibonacci profit taking at level: {:?}, time: {:?}, price: {}, sell amount: {}, remaining position: {}, funds after profit taking: {}",
                level, time_util::mill_time_to_datetime_shanghai(*ts), signal.open_price, sell_amount, remaining_position, *funds
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
    current_candle: &CandleItem,
    entry_time: &str,
    wins: &mut i64,
    losses: &mut i64,
    triggered_fib_levels: &mut HashSet<usize>, // 传入已触发的斐波那契级别的索引集合
    is_long: bool,                             // 是否为做多
) {
    let last_price = current_candle.c;
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
        close_type: if is_long {
            "止盈".to_string()
        } else {
            "止损".to_string()
        },
        win_num: *wins,
        loss_num: *losses,
        signal_value: None,
        signal_result: None,
    });
    *position = 0.0;

    // info!("Final sell at price: {}, funds after final sell: {}, profit/loss: {}",last_price, *funds, *total_profit_loss);

    triggered_fib_levels.clear(); // 重置斐波那契级别触发记录
}

// /// 运行回测
// pub fn run_test_top_contract(
//     strategy: impl Fn(&TopContractSingleData) -> SignalResult,
//     data: &TopContractData,
//     fib_levels: &[f64],
//     max_loss_percent: f64,
//     min_data_length: usize,
//     is_need_fibonacci_profit: bool,
//     is_open_long: bool,
//     is_open_short: bool,
//     is_judge_trade_time: bool,
// ) -> BackTestResult {
//     let initial_funds = 100.0;
//     let mut funds = initial_funds;
//     let mut position: f64 = 0.0;
//     let mut wins: i64 = 0;
//     let mut losses: i64 = 0;
//     let mut open_trades = 0;
//     let mut entry_price = 0.0;
//     let mut is_long = true; // 标记当前持仓是否为做多
//     let feibon_profil_levels = vec![0.236, 0.382, 0.500, 0.618, 0.786, 1.0];

//     let mut trade_records = Vec::new();
//     let mut entry_time = String::new();
//     let mut initial_quantity = 0.0;
//     let mut total_profit_loss = 0.0; // 总的盈利或损失
//     let mut triggered_fib_levels = HashSet::new(); // 用于记录已触发的斐波那契级别

//     let accout_ratio_list = data.clone().account_ratio;
//     let position_ratio_list = data.clone().position_ratio;

//     for (i, candle) in data.candle_list.iter().enumerate() {
//         if i + 1 < min_data_length {
//             continue; // 确保有足够的K线数据
//         }
//         // println!("signal_data:{:?}", signal_data);
//         let top_contract_single_data = TopContractSingleData {
//             candle_list: candle.clone(),
//             account_ratio: accout_ratio_list.get(i).unwrap().clone(),
//             position_ratio: position_ratio_list.get(i).clone().unwrap().clone(),
//         };
//         //调用函数,获取信号
//         let signal = strategy(&top_contract_single_data);
//         // println!("signal_result:{:?}", signal);
//         // info!("ts:{},Time: {:?}, funds: {}, Price: {}, Buy: {}, Sell: {}",candle.ts,time_util::mill_time_to_datetime_shanghai(candle.ts),funds,signal.price,signal.should_buy,signal.should_sell);

//         if signal.should_buy {
//             info!("should.buy");
//             if position > 0.0 {
//                 if !is_long {
//                     // 平掉所有空单
//                     // println!("平掉所有空单");
//                     close_remaining_position(
//                         &entry_price,
//                         &mut funds,
//                         &mut position,
//                         &mut total_profit_loss,
//                         &mut trade_records,
//                         &candle,
//                         &entry_time,
//                         &mut wins,
//                         &mut losses,
//                         &mut triggered_fib_levels,
//                         is_long,
//                     );
//                 } else {
//                     // println!("已经存在多单")
//                     continue;
//                 }
//             } else {
//                 if is_open_long {
//                     //如果需要判断开仓时间，且当前时间不在开仓时间范围内
//                     if is_judge_trade_time && !time_util::is_within_business_hours(candle.ts) {
//                         continue;
//                     }
//                     // 开多仓
//                     position = funds / signal.open_price;
//                     initial_quantity = position;
//                     entry_price = signal.open_price;
//                     entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
//                     open_trades += 1;
//                     total_profit_loss = 0.0;
//                     is_long = true;
//                     // info!("Buy at time: {:?}, price: {}, position: {}, funds after buy: {}",entry_time, signal.price, position, funds);

//                     trade_records.push(TradeRecord {
//                         option_type: PosSide::LONG.to_string(),
//                         open_position_time: entry_time.clone(),
//                         close_position_time: Some(entry_time.clone()),
//                         open_price: entry_price,
//                         close_price: signal.open_price,
//                         profit_loss: total_profit_loss,
//                         quantity: initial_quantity,
//                         full_close: false,
//                         close_type: "".to_string(),
//                         win_num: 0,
//                         loss_num: 0,
//                         signal_value: signal.single_value,
//                         signal_result: signal.single_result,
//                     });
//                 }
//             }
//         } else if signal.should_sell {
//             if position > 0.0 {
//                 if is_long {
//                     // 平掉所有多单
//                     // println!("平掉所有多单");
//                     close_remaining_position(
//                         &entry_price,
//                         &mut funds,
//                         &mut position,
//                         &mut total_profit_loss,
//                         &mut trade_records,
//                         &candle,
//                         &entry_time,
//                         &mut wins,
//                         &mut losses,
//                         &mut triggered_fib_levels,
//                         is_long,
//                     );
//                 } else {
//                     // println!("已经存在空单")
//                     continue;
//                 }
//             } else {
//                 if is_open_short {
//                     //如果需要判断开仓时间，且当前时间不在开仓时间范围内
//                     if is_judge_trade_time && !time_util::is_within_business_hours(candle.ts) {
//                         continue;
//                     }
//                     // 开空仓
//                     position = funds / signal.open_price;
//                     initial_quantity = position;
//                     entry_price = signal.open_price;
//                     entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
//                     open_trades += 1;
//                     total_profit_loss = 0.0;
//                     is_long = false;
//                     // info!("Short at time: {:?}, price: {}, position: {}, funds after short: {}", entry_time, signal.price, position, funds);

//                     trade_records.push(TradeRecord {
//                         option_type: PosSide::SHORT.to_string(),
//                         open_position_time: entry_time.clone(),
//                         close_position_time: Some(entry_time.clone()),
//                         open_price: entry_price,
//                         close_price: signal.open_price,
//                         profit_loss: total_profit_loss,
//                         quantity: initial_quantity,
//                         full_close: false,
//                         close_type: "".to_string(),
//                         win_num: 0,
//                         loss_num: 0,
//                         signal_value: signal.single_value,
//                         signal_result: signal.single_result,
//                     });
//                 }
//             }
//         } else if (is_long
//             && (signal.open_price < entry_price * (1.0 - max_loss_percent))
//             && position > 0.0)
//             || (!is_long
//                 && (signal.open_price > entry_price * (1.0 + max_loss_percent))
//                 && position > 0.0)
//         {
//             // 全部平仓
//             // println!("触发止损");
//             let exit_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
//             let current_loss = if is_long {
//                 position * (signal.open_price - entry_price)
//             } else {
//                 position * (entry_price - signal.open_price)
//             };
//             total_profit_loss += current_loss;
//             if total_profit_loss > 0.0 {
//                 wins += 1;
//             } else {
//                 losses += 1;
//             }

//             funds += current_loss;
//             trade_records.push(TradeRecord {
//                 option_type: "close".to_string(),
//                 open_position_time: entry_time.clone(),
//                 close_position_time: Some(exit_time),
//                 open_price: entry_price,
//                 close_price: signal.open_price,
//                 profit_loss: total_profit_loss,
//                 quantity: initial_quantity,
//                 full_close: true,
//                 close_type: "止损".to_string(),
//                 win_num: wins,
//                 loss_num: losses,
//                 signal_value: signal.single_value,
//                 signal_result: signal.single_result,
//             });
//             position = 0.0;
//             triggered_fib_levels.clear(); // 重置斐波那契级别触发记录
//                                           // info!("Sell (close long) at time: {:?}, price: {}, funds after sell: {}, profit/loss: {}",entry_time, signal.price, funds, total_profit_loss);
//         } else if position > 0.0 {
//             if is_need_fibonacci_profit {
//                 // 斐波那契部分止盈逻辑
//                 position = process_fibonacci_levels(
//                     &candle,
//                     &mut funds,
//                     &mut position,
//                     entry_price,
//                     &signal,
//                     fib_levels,
//                     &feibon_profil_levels,
//                     &entry_time,
//                     &candle.ts,
//                     &mut total_profit_loss,
//                     &mut trade_records,
//                     &mut triggered_fib_levels, // 传入已触发的斐波那契级别的索引集合
//                     is_long,
//                     &mut wins,
//                     &mut losses,
//                 );
//             }
//         }
//     }

//     // 最后一次平仓
//     if position > 0.0 {
//         // println!("最后k线平仓了");
//         close_remaining_position(
//             &entry_price,
//             &mut funds,
//             &mut position,
//             &mut total_profit_loss,
//             &mut trade_records,
//             data.candle_list.last().unwrap(),
//             &entry_time,
//             &mut wins,
//             &mut losses,
//             &mut triggered_fib_levels, // 传入已触发的斐波那契级别的索引集合
//             is_long,
//         );
//         // println!("平仓了最终资金: {}", funds);
//     }

//     let win_rate = if wins + losses > 0 {
//         wins as f64 / (wins + losses) as f64
//     } else {
//         0.0
//     };
//     BackTestResult {
//         funds,
//         win_rate,
//         open_trades,
//         trade_records,
//     }
//     // info!("Final Win rate: {}", win_rate);
// }

pub struct MoveStopLoss {
    pub is_long: bool,
    pub is_short: bool,
    pub price: f64,
}
/// 止盈止损策略配置
#[derive(Debug, Clone, Copy)]
pub struct BasicRiskStrategyConfig {
    pub use_dynamic_tp: bool,             // 是否使用动态止盈
    pub use_fibonacci_tp: bool,           // 是否使用斐波那契止盈
    pub max_loss_percent: f64,            // 最大止损百分比
    pub profit_threshold: f64,            // 盈利阈值，用于动态止盈
    pub is_move_stop_loss: bool,          //是否使用移动止损,当盈利之后,止损价格变成开仓价
    pub is_set_low_price_stop_loss: bool, //是否使用最低价止损,当价格低于入场k线的最低价时,止损。或者空单的时候,价格高于入场k线的最高价时,止损
}

impl Default for BasicRiskStrategyConfig {
    fn default() -> Self {
        Self {
            is_set_low_price_stop_loss: false,
            use_dynamic_tp: false,
            use_fibonacci_tp: false,
            max_loss_percent: 0.02,   // 默认3%止损
            profit_threshold: 0.01,   // 默认1%盈利开始启用动态止盈
            is_move_stop_loss: false, // 默认不使用移动止损
        }
    }
}

/// 计算多个ema值
pub fn calculate_ema(data: &CandleItem, ema_indicator: &mut EmaIndicator) -> EmaSignalValue {
    let mut ema_signal_value = EmaSignalValue::default();
    ema_signal_value.ema1_value = ema_indicator.ema1_indicator.next(data.c());
    ema_signal_value.ema2_value = ema_indicator.ema2_indicator.next(data.c());
    ema_signal_value.ema3_value = ema_indicator.ema3_indicator.next(data.c());
    ema_signal_value.ema4_value = ema_indicator.ema4_indicator.next(data.c());
    ema_signal_value.ema5_value = ema_indicator.ema5_indicator.next(data.c());

    //判断是否多头排列
    ema_signal_value.is_long_trend = ema_signal_value.ema1_value > ema_signal_value.ema2_value
        && ema_signal_value.ema2_value > ema_signal_value.ema3_value
        && ema_signal_value.ema3_value > ema_signal_value.ema4_value
        && ema_signal_value.ema4_value > ema_signal_value.ema5_value;
    //判断是否空头排列
    ema_signal_value.is_short_trend = ema_signal_value.ema1_value < ema_signal_value.ema2_value
        && ema_signal_value.ema2_value < ema_signal_value.ema3_value
        && ema_signal_value.ema3_value < ema_signal_value.ema4_value
        && ema_signal_value.ema4_value < ema_signal_value.ema5_value;

    ema_signal_value
}

/// 获取数据项和ema值
pub fn get_multi_indivator_values(
    indicator_combine: &mut IndicatorCombine,
    data_item: &CandleItem,
) -> VegasIndicatorSignalValue {
    // 使用with_capacity预分配内存
    let start = Instant::now();
    let mut vegas_indicator_signal_value = VegasIndicatorSignalValue::default();

    // 缓存频繁使用的值
    let close_price = data_item.c();
    let volume = data_item.v();

    // 计算ema - 这是最耗时的操作之一
    let ema_start = Instant::now();
    if let Some(ema_indicator) = &mut indicator_combine.ema_indicator {
        vegas_indicator_signal_value.ema_values = calculate_ema(data_item, ema_indicator);
    }
    if ema_start.elapsed().as_millis() > 10 {
        info!(duration_ms = ema_start.elapsed().as_millis(), "计算EMA");
    }

    // 计算volume - 避免重复调用data_item.v()
    let volume_start = Instant::now();
    if let Some(volume_indicator) = &mut indicator_combine.volume_indicator {
        vegas_indicator_signal_value.volume_value.volume_value = volume;
        vegas_indicator_signal_value.volume_value.volume_ratio = volume_indicator.next(volume);
    }
    if volume_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = volume_start.elapsed().as_millis(),
            "计算Volume"
        );
    }

    // 计算rsi - 避免重复调用data_item.c()
    let rsi_start = Instant::now();
    if let Some(rsi_indicator) = &mut indicator_combine.rsi_indicator {
        vegas_indicator_signal_value.rsi_value.rsi_value = rsi_indicator.next(close_price);
    }
    if rsi_start.elapsed().as_millis() > 10 {
        info!(duration_ms = rsi_start.elapsed().as_millis(), "计算RSI");
    }

    // 计算bollinger - 同样避免重复调用
    let bb_start = Instant::now();
    if let Some(bollinger_indicator) = &mut indicator_combine.bollinger_indicator {
        let bollinger_value = bollinger_indicator.next(close_price);
        vegas_indicator_signal_value.bollinger_value.upper = bollinger_value.upper;
        vegas_indicator_signal_value.bollinger_value.lower = bollinger_value.lower;
        vegas_indicator_signal_value.bollinger_value.middle = bollinger_value.average;
    }
    if bb_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = bb_start.elapsed().as_millis(),
            "计算Bollinger"
        );
    }

    // 计算吞没形态
    let eng_start = Instant::now();
    if let Some(engulfing_indicator) = &mut indicator_combine.engulfing_indicator {
        let engulfing_value = engulfing_indicator.next(data_item);
        vegas_indicator_signal_value.engulfing_value.is_engulfing = engulfing_value.is_engulfing;
        vegas_indicator_signal_value.engulfing_value.body_ratio = engulfing_value.body_ratio;
    }
    if eng_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = eng_start.elapsed().as_millis(),
            "计算吞没形态"
        );
    }

    // 计算锤子形态
    let hammer_start = Instant::now();
    if let Some(kline_hammer_indicator) = &mut indicator_combine.kline_hammer_indicator {
        let kline_hammer_value = kline_hammer_indicator.next(data_item);
        vegas_indicator_signal_value.kline_hammer_value.is_hammer = kline_hammer_value.is_hammer;
        vegas_indicator_signal_value
            .kline_hammer_value
            .is_hanging_man = kline_hammer_value.is_hanging_man;
        vegas_indicator_signal_value
            .kline_hammer_value
            .down_shadow_ratio = kline_hammer_value.down_shadow_ratio;
        vegas_indicator_signal_value
            .kline_hammer_value
            .up_shadow_ratio = kline_hammer_value.up_shadow_ratio;
        vegas_indicator_signal_value.kline_hammer_value.body_ratio = kline_hammer_value.body_ratio;
    }

    if hammer_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = hammer_start.elapsed().as_millis(),
            "计算锤子形态"
        );
    }

    vegas_indicator_signal_value
}

// /// 获取最新的指标值
// pub fn get_neweast_indicator_values(
//     candles_list: &Vec<CandlesEntity>,
//     strategy: &mut impl FnMut(&[CandleItem], &mut VegasIndicatorSignalValue) -> SignalResult,
//     basic_risk_config: BasicRiskStrategyConfig,
// ) {
//     let mut indicator_combine = IndicatorCombine::default();
//     let mut candle_item_list = Vec::with_capacity(candles_list.len());
//     for (i, candle) in candles_list.iter().enumerate() {
//         // 获取数据项
//         let data_item = parse_candle_to_data_item(candle);

//         // 获取指标的值
//         let mut multi_indicator_values =
//             get_multi_indivator_values(&mut indicator_combine, &data_item);

//         candle_item_list.push(data_item);
//     }
// }

/// 修改 run_test 函数签名
pub fn run_back_test(
    mut strategy: impl FnMut(&[CandleItem], &mut VegasIndicatorSignalValue) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IndicatorCombine,
) -> BackTestResult {
    let function_start = Instant::now();
    let main_span = span!(Level::INFO, "run_back_test");
    let _main_guard = main_span.enter();

    // 预分配交易状态，减少初始化开销
    let mut trading_state = TradingState {
        pre_stop_close_price: None,
        funds: 100.0,
        position: 0.0,
        wins: 0,
        losses: 0,
        open_trades: 0,
        entry_price: 0.0,
        move_stop_loss_price: None,
        is_long: true,
        entry_time: String::new(),
        initial_quantity: 0.0,
        total_profit_loss: 0.0,
        triggered_fib_levels: HashSet::with_capacity(10), // 预分配适当容量
        trade_records: Vec::with_capacity(candles_list.len() / 20), // 假设平均每20根K线产生一笔交易
    };
    // 预分配K线容量
    let mut candle_item_list = Vec::with_capacity(candles_list.len());

    // K线处理循环
    // 批量处理，每1000根K线报告一次进度
    const MAX_LOOKBACK: usize = 5;
    // K线数据预处理 - 一次性解析所有数字

    let loop_start = Instant::now();

    for (i, candle) in candles_list.iter().enumerate() {
        // let parsed = &parsed_candles[i];
        // let data_item = parse_candle_to_data_item(candle);
        // 计算指标值
        let mut multi_indicator_values = get_multi_indivator_values(indicator_combine, &candle);

        // 将新数据添加到列表，如果超过最大回溯期，删除最旧的数据
        candle_item_list.push(candle.clone());
        if candle_item_list.len() > MAX_LOOKBACK {
            candle_item_list.remove(0);
        }

        // 计算交易信号
        let mut signal = strategy(&candle_item_list, &mut multi_indicator_values);

        // 处理交易信号前检查是否值得处理（性能优化）
        let should_process_signal =
            signal.should_buy || signal.should_sell || trading_state.position > 0.0; // 有持仓时始终需要处理
        if should_process_signal {
            trading_state = deal_signal(
                trading_state,
                &mut signal,
                candle,
                basic_risk_config,
                &candle_item_list,
                i,
            );
        }
    }
    // 最终平仓处理
    finalize_trading_state(&mut trading_state, &candle_item_list);

    // 构建结果
    let result = BackTestResult {
        funds: trading_state.funds,
        win_rate: calculate_win_rate(trading_state.wins, trading_state.losses),
        open_trades: trading_state.open_trades,
        trade_records: trading_state.trade_records,
    };
    // // 记录总执行时间
    // info!(
    //     total_duration_ms = function_start.elapsed().as_millis(),
    //     "run_back_test总执行时间"
    // );
    result
}

pub fn parse_candle_to_data_item(candle: &CandlesEntity) -> CandleItem {
    CandleItem::builder()
        .c(candle.c.parse::<f64>().unwrap())
        .v(candle.vol_ccy.parse::<f64>().unwrap())
        .h(candle.h.parse::<f64>().unwrap())
        .l(candle.l.parse::<f64>().unwrap())
        .o(candle.o.parse::<f64>().unwrap())
        .ts(candle.ts)
        .build()
        .unwrap()
}

fn finalize_trading_state(trading_state: &mut TradingState, candle_item_list: &Vec<CandleItem>) {
    if trading_state.position > 0.0 {
        let last_candle = candle_item_list.last().unwrap();
        let last_price = last_candle.c;

        let profit = if trading_state.is_long {
            (last_price - trading_state.entry_price) * trading_state.position
        } else {
            (trading_state.entry_price - last_price) * trading_state.position
        };

        close_position(
            trading_state,
            last_candle,
            &SignalResult {
                should_buy: false,
                should_sell: true,
                open_price: last_price,
                ts: last_candle.ts,
                single_value: Some("结束平仓".to_string()),
                single_result: Some("结束平仓".to_string()),
            },
            "结束平仓",
            profit,
        );
    }
}

/**
 * 风险管理，检查止盈止损配置
 */
pub fn check_risk_config(
    mut trading_state: TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
    risk_config: BasicRiskStrategyConfig,
    candle_item_list: &Vec<CandleItem>,
    i: usize,
) -> TradingState {
    let current_price = signal.open_price;
    let current_low_price = candle.l;
    let current_high_price = candle.h;
    let entry_price = trading_state.entry_price; // 先保存入场价格

    //检查移动止损
    if risk_config.is_move_stop_loss {
        //如果设置了移动止损价格
        if let Some(move_stop_loss_price) = trading_state.move_stop_loss_price {
            if trading_state.is_long {
                if current_low_price <= move_stop_loss_price {
                    close_position(&mut trading_state, candle, &signal, "移动止损", 0.0);
                    return trading_state;
                }
            } else {
                if current_high_price >= move_stop_loss_price {
                    close_position(&mut trading_state, candle, &signal, "移动止损", 0.0);
                    return trading_state;
                }
            }
        } else {
            if trading_state.is_long {
                if current_price > entry_price {
                    //如果开仓后第一根k线有盈利，则设置止损价格为开仓价,保存本金
                    trading_state.move_stop_loss_price = Some(entry_price);
                }
            } else {
                if current_price < entry_price {
                    //如果开仓后第一根k线有盈利，则设置止损价格为开仓价,保存本金
                    trading_state.move_stop_loss_price = Some(entry_price);
                }
            }
        }
    }
    // 计算盈亏率
    let profit_pct = if trading_state.is_long {
        (current_price - entry_price) / entry_price
    } else {
        (entry_price - current_price) / entry_price // 做空的盈亏计算
    };

    //计算盈亏
    let profit = if trading_state.is_long {
        (current_price - entry_price) * trading_state.position
    } else {
        (entry_price - current_price) * trading_state.position
    };

    // 1. 检查最大止损
    if profit_pct < -risk_config.max_loss_percent {
        // println!(">>> 触发止损 <<< 开仓价:{}, 当前价:{}, 盈亏率:{:.2}% < 止损线:{:.2}%", entry_price, current_price, profit_pct * 100.0, -strategy_config.max_loss_percent * 100.0);
        close_position(&mut trading_state, candle, &signal, "止损", profit);
        return trading_state;
    }

    //检查设置了是否预止损价格
    if let Some(pre_stop_close_price) = trading_state.pre_stop_close_price {
        if trading_state.is_long {
            if current_low_price <= pre_stop_close_price {
                //重新计算利润
                let profit = (pre_stop_close_price - entry_price) * trading_state.position;
                close_position(
                    &mut trading_state,
                    candle,
                    &signal,
                    "预止损-信号线失效",
                    profit,
                );
                return trading_state;
            }
        } else {
            if current_high_price >= pre_stop_close_price {
                //重新计算利润
                let profit = (entry_price - pre_stop_close_price) * trading_state.position;
                close_position(
                    &mut trading_state,
                    candle,
                    &signal,
                    "预止损-信号线失效",
                    profit,
                );
                return trading_state;
            }
        }
    }
    // 2. 检查动态止盈
    if risk_config.use_dynamic_tp && profit_pct > risk_config.profit_threshold {
        if i >= 2 {
            let prev_close = candle_item_list[i - 1].c;
            let prev_prev_close = candle_item_list[i - 2].c;
            // println!("\n检查动态止盈条件:");
            // println!("当前价: {}, 前一K线收盘价: {}, 前前K线收盘价: {}", current_price, prev_close, prev_prev_close);
            // println!("盈利率: {:.2}% > 启动阈值: {:.2}%",
            // profit_pct * 100.0, strategy_config.profit_threshold * 100.0);

            // 根据多空方向判断动态止盈条件
            let should_take_profit = if trading_state.is_long {
                current_price < prev_close && current_price < prev_prev_close
            } else {
                current_price > prev_close && current_price > prev_prev_close
            };

            if should_take_profit {
                println!(
                    ">>> 触发动态止盈 <<< 当前价突破前两根K线 {}->{}->{}",
                    prev_prev_close, prev_close, current_price
                );
                let mut modified_signal = signal.clone();
                modified_signal.single_value = Some("触发动态止盈,价格突破前两根k线".to_string());
                close_position(
                    &mut trading_state,
                    candle,
                    &modified_signal,
                    "动态止盈",
                    profit,
                );
            }
        }
    }

    trading_state
}

pub fn deal_signal(
    mut trading_state: TradingState,
    signal: &mut SignalResult,
    candle: &CandleItem,
    risk_config: BasicRiskStrategyConfig,
    candle_item_list: &Vec<CandleItem>,
    i: usize,
) -> TradingState {
    // 如果有持仓, 先进行风险检查
    if trading_state.position > 0.000 {
        trading_state = check_risk_config(
            trading_state,
            signal,
            candle,
            risk_config,
            &candle_item_list,
            i,
        );
    }

    // 如果启用了设置预止损价格,则根据开仓方向设置预止损价格
    if risk_config.is_set_low_price_stop_loss {
        if signal.should_buy {
            trading_state.pre_stop_close_price = Some(candle.l);
        }
        if signal.should_sell {
            trading_state.pre_stop_close_price = Some(candle.h);
        }
    }

    // 处理策略信号
    if signal.should_buy {
        handle_buy_signal_logic(&mut trading_state, signal, candle);
    } else if signal.should_sell {
        handle_sell_signal_logic(&mut trading_state, signal, candle);
    }

    trading_state
}

/// 处理买入信号的逻辑
fn handle_buy_signal_logic(
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) {
    if trading_state.position <= 0.000000000 {
        // 没有仓位，直接开多仓
        open_long_position(trading_state, candle, signal);
    } else if !trading_state.is_long {
        // 持有空单，先平空单
        let profit = (trading_state.entry_price - signal.open_price) * trading_state.position;
        close_position(trading_state, candle, signal, "反向信号触发平仓", profit);
        // 然后开多仓
        open_long_position(trading_state, candle, signal);
    }
    // 如果已持有多单，则不执行任何操作
}

/// 处理卖出信号的逻辑
fn handle_sell_signal_logic(
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) {
    if trading_state.position <= 0.000000000 {
        // 没有仓位，直接开空仓
        open_short_position(trading_state, candle, signal);
    } else if trading_state.is_long {
        // 持有多单，先平多单
        let profit = (signal.open_price - trading_state.entry_price) * trading_state.position;
        close_position(trading_state, candle, signal, "反向信号平仓", profit);
        // 然后开空仓
        open_short_position(trading_state, candle, signal);
    }
    // 如果已持有空单，则不执行任何操作
}

/// 交易状态结构体
#[derive(Debug, Clone)]
pub struct TradingState {
    pub funds: f64,
    pub position: f64,
    pub wins: i64,
    pub losses: i64,
    pub open_trades: usize,
    pub entry_price: f64,
    //预止损价格
    pub pre_stop_close_price: Option<f64>,
    pub move_stop_loss_price: Option<f64>,
    pub is_long: bool,
    pub entry_time: String,
    pub initial_quantity: f64,
    pub total_profit_loss: f64,
    pub triggered_fib_levels: HashSet<usize>,
    pub trade_records: Vec<TradeRecord>,
}

// /// 处理买入信号
// fn handle_buy_signal(
//     state: &mut TradingState,
//     candle: &CandleItem,
//     signal: &SignalResult,
//     is_open_long: bool,
//     is_judge_trade_time: bool,
// ) {
//     if state.position > 0.0 {
//         if !state.is_long {
//             close_remaining_position(
//                 &state.entry_price,
//                 &mut state.funds,
//                 &mut state.position,
//                 &mut state.total_profit_loss,
//                 &mut state.trade_records,
//                 candle,
//                 &state.entry_time,
//                 &mut state.wins,
//                 &mut state.losses,
//                 &mut state.triggered_fib_levels,
//                 state.is_long,
//             );
//         }
//     } else if is_open_long
//         && (!is_judge_trade_time || time_util::is_within_business_hours(candle.ts))
//     {
//         open_long_position(state, candle, signal);
//     }
// }

// /// 处理卖出信号
// fn handle_sell_signal(
//     state: &mut TradingState,
//     candle: &CandleItem,
//     signal: &SignalResult,
//     is_open_short: bool,
//     is_judge_trade_time: bool,
// ) {
//     if state.position > 0.0 {
//         if state.is_long {
//             close_remaining_position(
//                 &state.entry_price,
//                 &mut state.funds,
//                 &mut state.position,
//                 &mut state.total_profit_loss,
//                 &mut state.trade_records,
//                 candle,
//                 &state.entry_time,
//                 &mut state.wins,
//                 &mut state.losses,
//                 &mut state.triggered_fib_levels,
//                 state.is_long,
//             );
//         }
//     } else if is_open_short
//         && (!is_judge_trade_time || time_util::is_within_business_hours(candle.ts))
//     {
//         open_short_position(state, candle, signal);
//     }
// }

/// 开多仓
fn open_long_position(state: &mut TradingState, candle: &CandleItem, signal: &SignalResult) {
    state.position = state.funds / signal.open_price;
    state.initial_quantity = state.position;
    state.entry_price = signal.open_price;
    state.entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
    state.open_trades += 1;
    state.total_profit_loss = 0.0;
    state.is_long = true;

    record_trade_entry(state, PosSide::LONG.to_string(), signal);
}

/// 开空仓
fn open_short_position(state: &mut TradingState, candle: &CandleItem, signal: &SignalResult) {
    state.position = state.funds / signal.open_price;
    state.initial_quantity = state.position;
    state.entry_price = signal.open_price;
    state.entry_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
    state.open_trades += 1;
    state.total_profit_loss = 0.0;
    state.is_long = false;

    record_trade_entry(state, "SHORT".to_string(), &signal);
}

/// 记录交易入场
fn record_trade_entry(state: &mut TradingState, option_type: String, signal: &SignalResult) {
    if false {
        //批量回测的时候不进行记录
        state.trade_records.push(TradeRecord {
            option_type,
            open_position_time: state.entry_time.clone(),
            close_position_time: Some(state.entry_time.clone()),
            open_price: state.entry_price,
            close_price: state.entry_price,
            profit_loss: state.total_profit_loss,
            quantity: state.initial_quantity,
            full_close: false,
            close_type: "".to_string(),
            win_num: 0,
            loss_num: 0,
            signal_value: signal.single_value.clone(),
            signal_result: signal.single_result.clone(),
        });
    }
}

/// 辅助函数：获取前N根K线
fn get_previous_candles(current_candle: &CandlesEntity, n: usize) -> Option<Vec<&CandlesEntity>> {
    // 这个函数需要根据您的数据结构来实现
    // 返回前n根K线的数据
    None // 临时返回值，需要实现具体逻辑
}

/// 辅助函数：平仓
fn close_position(
    state: &mut TradingState,
    candle: &CandleItem,
    signal: &SignalResult,
    close_type: &str,
    profit: f64,
) {
    let exit_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
    // 更新总利润和资金
    state.move_stop_loss_price = None;

    state.total_profit_loss += profit;
    state.funds += profit;

    if profit > 0.0 {
        state.wins += 1;
    } else {
        state.losses += 1;
    }

    // 根据平仓原因和盈亏设置正确的平仓类型    let actual_close_type = if profit > 0.0 { "止盈" } else { "止损" };
    // Calculate the actual quantity being closed
    let closing_quantity = state.position;

    record_trade_exit(state, exit_time, signal, close_type, closing_quantity);

    // Set position to zero AFTER recording the exit with correct quantity
    state.position = 0.00000;
    state.triggered_fib_levels.clear();
}

/// 记录交易出场
fn record_trade_exit(
    state: &mut TradingState,
    exit_time: String,
    signal: &SignalResult,
    close_type: &str,
    closing_quantity: f64, // Add parameter for quantity being closed
) {
    //todo 批量回测的时候不进行记录
    if false {
        state.trade_records.push(TradeRecord {
            option_type: "close".to_string(),
            open_position_time: state.entry_time.clone(),
            close_position_time: Some(exit_time),
            open_price: state.entry_price,
            close_price: signal.open_price,
            profit_loss: state.total_profit_loss,
            quantity: closing_quantity, // Use the actual closing quantity, not initial_quantity
            full_close: true,
            close_type: close_type.to_string(),
            win_num: state.wins,
            loss_num: state.losses,
            signal_value: signal.single_value.clone(),
            signal_result: signal.single_result.clone(),
        });
    }
}

/// 计算胜率
fn calculate_win_rate(wins: i64, losses: i64) -> f64 {
    if wins + losses > 0 {
        wins as f64 / (wins + losses) as f64
    } else {
        0.0
    }
}

/// 处理策略信号时的利润计算
fn handle_strategy_signals(state: &mut TradingState, signal: &SignalResult, candle: &CandleItem) {
    if state.position > 0.0 {
        // 计算当前利润，考虑多空方向
        let current_profit = if state.is_long {
            (signal.open_price - state.entry_price) * state.position
        } else {
            (state.entry_price - signal.open_price) * state.position
        };

        // 处理平仓信号
        if (state.is_long && signal.should_sell) || (!state.is_long && signal.should_buy) {
            close_position(state, candle, signal, "策略平仓", current_profit);
        }
    }
}
