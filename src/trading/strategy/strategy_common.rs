use crate::trading::indicator::ema_indicator::EmaIndicator;
use crate::trading::indicator::enums::common_enums::TradeSide;
use crate::trading::indicator::equal_high_low_indicator::EqualHighLowValue;
use crate::trading::indicator::fair_value_gap_indicator::FairValueGapValue;
use crate::trading::indicator::leg_detection_indicator::LegDetectionValue;
use crate::trading::indicator::market_structure_indicator::MarketStructureValue;
use crate::trading::indicator::premium_discount_indicator::PremiumDiscountValue;
use crate::trading::indicator::rsi_rma_indicator::RsiIndicator;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use crate::trading::indicator::vegas_indicator::{
    EmaSignalValue, IndicatorCombine, VegasIndicatorSignalValue, VegasStrategy,
};
use crate::trading::indicator::volume_indicator::VolumeRatioIndicator;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::top_contract_strategy::{TopContractData, TopContractSingleData};
use crate::{time_util, CandleItem};
use chrono::{DateTime, Utc};
use hmac::digest::typenum::Min;
use okx::dto::common::PositionSide;
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
        BackTestResult {
            funds: 0.0,
            win_rate: 0.0,
            open_trades: 0,
            trade_records: vec![],
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TradeRecord {
    pub option_type: String,
    pub open_position_time: String,
    //信号开仓时间
    pub signal_open_position_time: Option<String>,
    pub close_position_time: Option<String>,
    pub open_price: f64,
    //信号状态
    pub signal_status: i32,
    //平仓价格
    pub close_price: Option<f64>,
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
    //止损价格
    pub stop_loss_price: Option<f64>,
    //最优开仓价格(通常设置为信号线的0.382位置出开仓)
    pub best_open_price: Option<f64>,
    //最优止盈价格(通常设置为信号线的价差的2倍率) 1:2 1:3 1:4 1:5
    pub best_take_profit_price: Option<f64>,
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

        // println!(
        //     "signal.price:{},fib_price:{},level:{}",
        //     signal.open_price, fib_price, level
        // );

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
                    signal_status: 0,
                    option_type: "fibonacci_close".to_string(),
                    open_position_time: entry_time.to_string(),
                    signal_open_position_time: Some(signal.ts.to_string()),
                    close_position_time: Some(exit_time),
                    open_price: entry_price,
                    close_price: Some(fib_price),
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
        signal_status: 0,
        option_type: "close".to_string(),
        open_position_time: entry_time.to_string(),
        signal_open_position_time: Some(entry_time.to_string()),
        close_position_time: Some(exit_time),
        open_price: *entry_price, // 在最终平仓时记录开仓价格
        close_price: Some(last_price),
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
pub struct MoveStopLoss {
    pub is_long: bool,
    pub is_short: bool,
    pub price: f64,
}
/// 止盈止损策略配置
#[derive(Debug, Clone, Copy)]
pub struct BasicRiskStrategyConfig {
    pub use_dynamic_tp: bool,                  // 是否使用动态止盈
    pub use_fibonacci_tp: bool,                // 是否使用斐波那契止盈
    pub max_loss_percent: f64,                 // 最大止损百分比
    pub profit_threshold: f64,                 // 盈利阈值，用于动态止盈
    pub is_move_stop_loss: bool,               //是否使用移动止损,当盈利之后,止损价格变成开仓价
    pub is_used_signal_k_line_stop_loss: bool, //是否使用最低价止损,当价格低于入场k线的最低价时,止损。或者空单的时候,价格高于入场k线的最高价时,止损
}

impl Default for BasicRiskStrategyConfig {
    fn default() -> Self {
        Self {
            is_used_signal_k_line_stop_loss: false,
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
        && ema_signal_value.ema3_value > ema_signal_value.ema4_value;
    // && ema_signal_value.ema4_value > ema_signal_value.ema5_value;
    //判断是否空头排列
    ema_signal_value.is_short_trend = ema_signal_value.ema1_value < ema_signal_value.ema2_value
        && ema_signal_value.ema2_value < ema_signal_value.ema3_value
        && ema_signal_value.ema3_value < ema_signal_value.ema4_value;
    // && ema_signal_value.ema4_value < ema_signal_value.ema5_value;

    ema_signal_value
}

/// 获取数据项和ema值
pub fn get_multi_indicator_values(
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

    // 计算Smart Money Concepts相关指标
    // 腿部识别
    let leg_start = Instant::now();
    if let Some(leg_detection_indicator) = &mut indicator_combine.leg_detection_indicator {
        // 这里假设leg_detection_indicator.next方法需要一个数据切片，但我们只传入单个data_item
        // 实际使用时需要根据指标实现进行调整
        vegas_indicator_signal_value.leg_detection_value = leg_detection_indicator.next(data_item);
    }
    if leg_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = leg_start.elapsed().as_millis(),
            "计算腿部识别"
        );
    }

    // 市场结构
    let structure_start = Instant::now();
    if let Some(market_structure_indicator) = &mut indicator_combine.market_structure_indicator {
        vegas_indicator_signal_value.market_structure_value =
            market_structure_indicator.next(data_item);
    }
    if structure_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = structure_start.elapsed().as_millis(),
            "计算市场结构"
        );
    }

    // 公平价值缺口
    let fvg_start = Instant::now();
    if let Some(fair_value_gap_indicator) = &mut indicator_combine.fair_value_gap_indicator {
        vegas_indicator_signal_value.fair_value_gap_value =
            fair_value_gap_indicator.next(data_item);
    }
    if fvg_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = fvg_start.elapsed().as_millis(),
            "计算公平价值缺口"
        );
    }

    // 等高/等低点
    let ehl_start = Instant::now();
    if let Some(equal_high_low_indicator) = &mut indicator_combine.equal_high_low_indicator {
        vegas_indicator_signal_value.equal_high_low_value =
            equal_high_low_indicator.next(data_item);
    }
    if ehl_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = ehl_start.elapsed().as_millis(),
            "计算等高/等低点"
        );
    }

    // 溢价/折扣区域
    let pd_start = Instant::now();
    if let Some(premium_discount_indicator) = &mut indicator_combine.premium_discount_indicator {
        vegas_indicator_signal_value.premium_discount_value =
            premium_discount_indicator.next(data_item);
    }
    if pd_start.elapsed().as_millis() > 10 {
        info!(
            duration_ms = pd_start.elapsed().as_millis(),
            "计算溢价/折扣区域"
        );
    }

    vegas_indicator_signal_value
}

/// 修改 run_test 函数签名
pub fn run_back_test(
    mut strategy: impl FnMut(&[CandleItem], &mut VegasIndicatorSignalValue) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IndicatorCombine,
) -> BackTestResult {
    // 预分配交易状态，减少初始化开销
    let mut trading_state = TradingState {
        pre_stop_close_price: None,
        funds: 100.0,
        position: 0.0,
        wins: 0,
        losses: 0,
        open_trades: 0,
        open_price: 0.0,
        close_price: None,
        best_take_profit_price: None,
        signal_status: 0,
        last_signal_result: None,
        is_use_best_open_price: false,
        move_stop_loss_price: None,
        trade_side: None,
        open_position_time: String::new(),
        signal_open_position_time: None,
        initial_quantity: 0.0,
        total_profit_loss: 0.0,
        triggered_fib_levels: HashSet::with_capacity(10), // 预分配适当容量
        trade_records: Vec::with_capacity(candles_list.len() / 10), // 假设平均每20根K线产生一笔交易
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
        let mut multi_indicator_values = get_multi_indicator_values(indicator_combine, &candle);

        // 将新数据添加到列表，如果超过最大回溯期，删除最旧的数据
        candle_item_list.push(candle.clone());
        if candle_item_list.len() > MAX_LOOKBACK {
            candle_item_list.remove(0);
        }
        // 计算交易信号
        let mut signal = strategy(&candle_item_list, &mut multi_indicator_values);

        if signal.should_buy || signal.should_sell {
            if signal.best_open_price.is_some() {
                trading_state.last_signal_result = Some(signal.clone());
                trading_state.is_use_best_open_price = true;
            } else {
                trading_state.is_use_best_open_price = false;
                trading_state.last_signal_result = None;
            }

            if signal.best_take_profit_price.is_some() {
                trading_state.best_take_profit_price = signal.best_take_profit_price;
            } else {
                trading_state.best_take_profit_price = None;
            }
        }
        // 处理交易信号前检查是否值得处理（性能优化）
        let should_process_signal = signal.should_buy
            || signal.should_sell
            || trading_state.position > 0.0
            || trading_state.last_signal_result.is_some(); // 有持仓时始终需要处理
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
        trading_state.close_price = Some(last_price);

        let profit = match trading_state.trade_side {
            Some(TradeSide::Long) => {
                (last_price - trading_state.open_price) * trading_state.position
            }
            Some(TradeSide::Short) => {
                (trading_state.open_price - last_price) * trading_state.position
            }
            None => 0.0,
        };

        close_position(
            trading_state,
            last_candle,
            &SignalResult {
                should_buy: false,
                should_sell: true,
                open_price: last_price,
                best_open_price: None,
                best_take_profit_price: None,
                stop_loss_price: None,
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
    let current_open_price = signal.open_price;
    let current_low_price = candle.l;
    let current_high_price = candle.h;
    let current_close_price = candle.c;
    let entry_price = trading_state.open_price; // 先保存入场价格

    let position = trading_state.position.clone();

    if signal.stop_loss_price.is_some() {
        let tp_price = signal.stop_loss_price.unwrap();
        match trading_state.trade_side {
            Some(TradeSide::Long) => {
                if current_close_price < tp_price {
                    close_position(
                        &mut trading_state,
                        candle,
                        &signal,
                        "信号线失效-止损",
                        (current_open_price - entry_price) * position,
                    );
                }
            }
            Some(TradeSide::Short) => {
                if current_close_price > tp_price {
                    close_position(
                        &mut trading_state,
                        candle,
                        &signal,
                        "信号线失效-止损",
                        (entry_price - current_open_price) * position,
                    );
                }
            }
            None => {
                // do nothing
            }
        }
    }

    //检查移动止盈
    if risk_config.is_move_stop_loss {
        //如果设置了移动止盈价格
        if let Some(move_stop_loss_price) = trading_state.move_stop_loss_price {
            match trading_state.trade_side {
                Some(TradeSide::Long) => {
                    if current_close_price <= move_stop_loss_price {
                        trading_state.close_price = Some(current_close_price);
                        close_position(
                            &mut trading_state,
                            candle,
                            &signal,
                            "移动止盈",
                            (current_close_price - entry_price) * position,
                        );
                        return trading_state;
                    }
                }
                Some(TradeSide::Short) => {
                    if current_close_price >= move_stop_loss_price {
                        trading_state.close_price = Some(current_close_price);
                        close_position(
                            &mut trading_state,
                            candle,
                            &signal,
                            "移动止盈",
                            (entry_price - current_close_price) * position,
                        );
                        return trading_state;
                    }
                }
                None => {
                    // do nothing
                }
            }
        } else {
            match trading_state.trade_side {
                Some(TradeSide::Long) => {
                    if current_open_price > entry_price {
                        //如果开仓后第一根k线有盈利，则设置止损价格为开仓价,保存本金
                        trading_state.move_stop_loss_price = Some(entry_price);
                    }
                }
                Some(TradeSide::Short) => {
                    if current_open_price < entry_price {
                        //如果开仓后第一根k线有盈利，则设置止损价格为开仓价,保存本金
                        trading_state.move_stop_loss_price = Some(entry_price);
                    }
                }
                None => {
                    // do nothing
                }
            }
        }
    }
    // 计算盈亏率
    let profit_pct = match trading_state.trade_side {
        Some(TradeSide::Long) => (current_open_price - entry_price) / entry_price,
        Some(TradeSide::Short) => {
            (entry_price - current_open_price) / entry_price // 做空的盈亏计算
        }
        None => 0.0,
    };

    //计算盈亏
    let profit = match trading_state.trade_side {
        Some(TradeSide::Long) => (current_open_price - entry_price) * trading_state.position,
        Some(TradeSide::Short) => (entry_price - current_open_price) * trading_state.position,
        None => 0.0,
    };

    //检查是否设置了最优止盈价格
    if let Some(best_take_profit_price) = trading_state.best_take_profit_price {
        match trading_state.trade_side {
            Some(TradeSide::Long) => {
                if current_high_price >= best_take_profit_price {
                    let profit = (best_take_profit_price - entry_price) * trading_state.position;
                    close_position(&mut trading_state, candle, &signal, "最优止盈", profit);
                    return trading_state;
                }
            }
            Some(TradeSide::Short) => {
                if current_low_price <= best_take_profit_price {
                    let profit = (entry_price - best_take_profit_price) * trading_state.position;
                    close_position(&mut trading_state, candle, &signal, "最优止盈", profit);
                    return trading_state;
                }
            }
            None => {
                // do nothing
            }
        }
    }

    // 1. 检查最大止损
    if profit_pct < -risk_config.max_loss_percent {
        // println!(">>> 触发止损 <<< 开仓价:{}, 当前价:{}, 盈亏率:{:.2}% < 止损线:{:.2}%", entry_price, current_price, profit_pct * 100.0, -strategy_config.max_loss_percent * 100.0);
        trading_state.close_price = Some(current_open_price);
        close_position(&mut trading_state, candle, &signal, "最大亏损止损", profit);
        return trading_state;
    }

    //检查设置了是否预止损价格
    if let Some(pre_stop_close_price) = trading_state.pre_stop_close_price {
        match trading_state.trade_side {
            Some(TradeSide::Long) => {
                if current_close_price <= pre_stop_close_price {
                    //重新计算利润
                    trading_state.close_price = Some(pre_stop_close_price);
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
            }
            Some(TradeSide::Short) => {
                if current_close_price >= pre_stop_close_price {
                    //重新计算利润
                    trading_state.close_price = Some(pre_stop_close_price);
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
            None => {
                // do nothing
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
    if risk_config.is_used_signal_k_line_stop_loss {
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
    } else if trading_state.last_signal_result.is_some() && trading_state.position <= 0.0 {
        //要确保大于信号的开仓时间
        if candle.ts >= trading_state.last_signal_result.clone().unwrap().ts {
            let last_signal_result = trading_state.last_signal_result.clone().unwrap();
            if last_signal_result.should_buy {
                //如果信号是买，但是当前价格低于信号的最优开仓价格，则使用信号的最优开仓价格
                if candle.l < last_signal_result.best_open_price.unwrap() {
                    signal.open_price = last_signal_result.best_open_price.unwrap();
                    signal.should_buy = true;
                    signal.stop_loss_price = last_signal_result.stop_loss_price;
                    signal.single_value = last_signal_result.single_value;
                    signal.single_result = last_signal_result.single_result;

                    trading_state.trade_side = Some(TradeSide::Long);
                    trading_state.last_signal_result = None;
                    trading_state.is_use_best_open_price = true;
                    trading_state.signal_open_position_time =
                        Some(time_util::mill_time_to_datetime(last_signal_result.ts).unwrap());
                    handle_buy_signal_logic(&mut trading_state, signal, candle);
                }
            } else if last_signal_result.should_sell {
                //如果信号是卖，但是当前价格高于信号的最优开仓价格，则使用信号的最优开仓价格
                if candle.h > last_signal_result.best_open_price.unwrap() {
                    signal.open_price = last_signal_result.best_open_price.unwrap();
                    signal.should_sell = true;
                    trading_state.trade_side = Some(TradeSide::Short);
                    signal.stop_loss_price = last_signal_result.stop_loss_price;
                    signal.single_value = last_signal_result.single_value;
                    signal.single_result = last_signal_result.single_result;

                    trading_state.last_signal_result = None;
                    trading_state.is_use_best_open_price = true;
                    trading_state.signal_open_position_time =
                        Some(time_util::mill_time_to_datetime(last_signal_result.ts).unwrap());

                    handle_sell_signal_logic(&mut trading_state, signal, candle);
                }
            }
        }
    }

    trading_state
}

/// 处理买入信号的逻辑
fn handle_buy_signal_logic(
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) {
    if trading_state.position <= 0.0 {
        //不使用最优开仓价格，直接开多仓
        if trading_state.last_signal_result.is_none() {
            open_long_position(trading_state, candle, signal);
        }
    } else if trading_state.trade_side.is_some()
        && trading_state.trade_side.unwrap() == TradeSide::Short
    {
        // 持有空单，先平空单
        let profit = (trading_state.open_price - signal.open_price) * trading_state.position;
        trading_state.close_price = Some(signal.open_price);

        close_position(trading_state, candle, signal, "反向信号触发平仓", profit);
        // 然后开多仓
        if trading_state.last_signal_result.is_none() {
            open_long_position(trading_state, candle, signal);
        }
    } else {
        //todo 如果已持有多单，则不执行任何操作
    }
}

/// 处理卖出信号的逻辑
fn handle_sell_signal_logic(
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) {
    if trading_state.position <= 0.0 {
        //不使用最优开仓价格，直接开空仓
        if trading_state.last_signal_result.is_none() {
            open_short_position(trading_state, candle, signal);
        }
    } else if trading_state.trade_side.is_some()
        && trading_state.trade_side.unwrap() == TradeSide::Long
    {
        // 持有多单，先平多单
        let profit = (signal.open_price - trading_state.open_price) * trading_state.position;
        trading_state.close_price = Some(signal.open_price);
        close_position(trading_state, candle, signal, "反向信号平仓", profit);
        // 然后开空仓
        if trading_state.last_signal_result.is_none() {
            open_short_position(trading_state, candle, signal);
        }
    } else {
        //todo  如果已持有空单，则不执行任何操作
    }
}

/// 交易状态结构体
#[derive(Debug, Clone)]
pub struct TradingState {
    pub funds: f64,
    pub position: f64,
    pub wins: i64,
    pub losses: i64,
    pub open_trades: usize,
    //实际开仓价格
    pub open_price: f64,
    //实际平仓价格
    pub close_price: Option<f64>,
    //预止损价格
    pub pre_stop_close_price: Option<f64>,
    //最优止盈价格
    pub best_take_profit_price: Option<f64>,
    pub move_stop_loss_price: Option<f64>,
    pub last_signal_result: Option<SignalResult>,
    pub is_use_best_open_price: bool,
    pub trade_side: Option<TradeSide>,
    //开仓价格
    pub open_position_time: String,
    //信号开仓时间
    pub signal_open_position_time: Option<String>,
    //signal_status 信号状态  0使用信号正常 -1信号错过 1使用信号的最优价格
    pub signal_status: i32,
    pub initial_quantity: f64,
    pub total_profit_loss: f64,
    pub triggered_fib_levels: HashSet<usize>,
    pub trade_records: Vec<TradeRecord>,
}

/// 开多仓
fn open_long_position(state: &mut TradingState, candle: &CandleItem, signal: &SignalResult) {
    state.position = state.funds / signal.open_price;
    state.initial_quantity = state.position;
    state.open_price = signal.open_price;
    state.open_position_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
    state.open_trades += 1;
    state.total_profit_loss = 0.0;
    state.trade_side = Some(TradeSide::Long);
    state.last_signal_result = None;
    state.is_use_best_open_price = false;

    record_trade_entry(state, PositionSide::Long.to_string(), signal);
}

/// 开空仓
fn open_short_position(state: &mut TradingState, candle: &CandleItem, signal: &SignalResult) {
    state.position = state.funds / signal.open_price;
    state.initial_quantity = state.position;
    state.open_price = signal.open_price;
    state.open_position_time = time_util::mill_time_to_datetime(candle.ts).unwrap();
    state.open_trades += 1;
    state.total_profit_loss = 0.0;
    state.trade_side = Some(TradeSide::Short);

    state.last_signal_result = None;
    state.is_use_best_open_price = false;

    record_trade_entry(state, PositionSide::Short.to_string(), &signal);
}

/// 记录交易入场
fn record_trade_entry(state: &mut TradingState, option_type: String, signal: &SignalResult) {
    // if false {
    //批量回测的时候不进行记录
    state.trade_records.push(TradeRecord {
        option_type,
        open_position_time: state.open_position_time.clone(),
        close_position_time: Some(state.open_position_time.clone()),
        //开仓价格
        open_price: state.open_price,
        //信号开仓价格
        signal_open_position_time: state.signal_open_position_time.clone(),
        //信号状态
        signal_status: state.signal_status as i32,
        //平仓价格
        close_price: state.close_price.clone(),
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

    //手续费设定0.007,假设开仓平仓各收一次 (数量*价格 *0.07%)
    let fee = state.position * state.open_price * 0.0007;
    println!("手续费: {}", fee);
    let profit_after_fee = profit - fee;
    state.total_profit_loss += profit_after_fee;
    state.funds += profit_after_fee;

    if profit > 0.0 {
        state.wins += 1;
    } else {
        state.losses += 1;
    }

    // 根据平仓原因和盈亏设置正确的平仓类型
    // let closing_quantity = state.position;
    record_trade_exit(state, exit_time, signal, close_type, state.position);

    // Set position to zero AFTER recording the exit with correct quantity
    state_init(state);
}

/// 初始化交易状态
pub fn state_init(state: &mut TradingState) {
    state.position = 0.0;
    state.open_price = 0.0;
    state.close_price = None;
    state.triggered_fib_levels.clear();
    state.trade_side = None;
    state.last_signal_result = None;
    state.is_use_best_open_price = false;
    state.signal_open_position_time = None;
    state.best_take_profit_price = None;
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
    state.trade_records.push(TradeRecord {
        option_type: "close".to_string(),
        open_position_time: state.open_position_time.clone(),
        signal_open_position_time: state.signal_open_position_time.clone(),
        signal_status: state.signal_status as i32,
        close_position_time: Some(exit_time),
        open_price: state.open_price,
        close_price: state.close_price,
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

/// 计算胜率
fn calculate_win_rate(wins: i64, losses: i64) -> f64 {
    if wins + losses > 0 {
        wins as f64 / (wins + losses) as f64
    } else {
        0.0
    }
}

// /// 处理策略信号时的利润计算
// fn handle_strategy_signals(state: &mut TradingState, signal: &SignalResult, candle: &CandleItem) {
//     if state.position > 0.0 {
//         // 计算当前利润，考虑多空方向
//         let current_profit = match state.trade_side {
//             Some(TradeSide::Long) => (signal.open_price - state.open_price) * state.position,
//             Some(TradeSide::Short) => (state.open_price - signal.open_price) * state.position,
//             None => 0.0,
//         };

//         // 处理平仓信号
//         if (state.trade_side.is_some()
//             && state.trade_side.unwrap() == TradeSide::Short
//             && signal.should_sell)
//             || (state.trade_side.is_some()
//                 && state.trade_side.unwrap() == TradeSide::Long
//                 && signal.should_buy)
//         {
//             close_position(state, candle, signal, "策略平仓", current_profit);
//         }
//     }
// }
