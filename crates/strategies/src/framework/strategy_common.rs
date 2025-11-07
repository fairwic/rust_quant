use rust_quant_indicators::trend::ema_indicator::EmaIndicator;
use rust_quant_indicators::enums::common_enums::TradeSide;
use rust_quant_indicators::equal_high_low_indicator::EqualHighLowValue;
use rust_quant_indicators::fair_value_gap_indicator::FairValueGapValue;
use rust_quant_indicators::leg_detection_indicator::LegDetectionValue;
use rust_quant_indicators::market_structure_indicator::MarketStructureValue;
use rust_quant_indicators::premium_discount_indicator::PremiumDiscountValue;
use rust_quant_indicators::rsi_rma_indicator::RsiIndicator;
use rust_quant_indicators::trend::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::{
    EmaSignalValue, IndicatorCombine, KlineHammerSignalValue, VegasIndicatorSignalValue,
    VegasStrategy,
};
use rust_quant_indicators::volume_indicator::VolumeRatioIndicator;
use rust_quant_market::models::CandleEntity;
use rust_quant_common::strategy::nwe_strategy::NweStrategy;
use rust_quant_common::strategy::top_contract_strategy::{TopContractData, TopContractSingleData};
use rust_quant_common::utils::fibonacci::FIBONACCI_ONE_POINT_TWO_THREE_SIX;
use crate::{time_util, CandleItem};
use chrono::{DateTime, Utc};
use hmac::digest::typenum::Min;
use okx::dto::common::PositionSide;
use okx::dto::EnumToStrTrait;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::env;
use std::time::Instant;
use ta::indicators::BollingerBands;
use ta::Close;
use ta::DataItem;
use ta::High;
use ta::Low;
use ta::Next;
use ta::Open;
use ta::Volume;
use tracing::Level;
use tracing::{error, info};
use tracing::{span, warn};

/// 通用回测策略能力接口，便于不同策略复用统一回测与落库流程
pub trait BackTestAbleStrategyTrait {
    fn strategy_type(&self) -> crate::trading::strategy::StrategyType;
    fn config_json(&self) -> Option<String>;
    fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult;
}

impl BackTestAbleStrategyTrait for VegasStrategy {
    fn strategy_type(&self) -> crate::trading::strategy::StrategyType {
        crate::trading::strategy::StrategyType::Vegas
    }

    fn config_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        VegasStrategy::run_test(self, candles, risk_strategy_config)
    }
}

impl BackTestAbleStrategyTrait for NweStrategy {
    fn strategy_type(&self) -> crate::trading::strategy::StrategyType {
        crate::trading::strategy::StrategyType::Nwe
    }

    fn config_json(&self) -> Option<String> {
        // 仅记录配置内容
        serde_json::to_string(&self.config).ok()
    }

    fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        NweStrategy::run_test(self, candles, risk_strategy_config)
    }
}
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
    //交易类型
    pub option_type: String,
    //实际开仓时间
    pub open_position_time: String,
    //信号开仓时间
    pub signal_open_position_time: Option<String>,
    //平仓时间
    pub close_position_time: Option<String>,
    //开仓价格
    pub open_price: f64,
    //信号状态
    pub signal_status: i32,
    //平仓价格
    pub close_price: Option<f64>,
    //盈亏
    pub profit_loss: f64,
    //开仓数量
    pub quantity: f64,
    //是否全平
    pub full_close: bool,
    //平仓类型
    pub close_type: String,
    //盈利次数
    pub win_num: i64,
    //亏损次数
    pub loss_num: i64,
    //信号值
    pub signal_value: Option<String>,
    //信号结果
    pub signal_result: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SignalResult {
    pub should_buy: bool,
    pub should_sell: bool,
    //开仓价格
    pub open_price: f64,
    //止损价格
    pub signal_kline_stop_loss_price: Option<f64>,
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
                let exit_time = rust_quant_common::utils::time::mill_time_to_datetime(*ts).unwrap();

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
                level, rust_quant_common::utils::time::mill_time_to_datetime_shanghai(*ts), signal.open_price, sell_amount, remaining_position, *funds
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
    let exit_time = rust_quant_common::utils::time::mill_time_to_datetime(current_candle.ts).unwrap();

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
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct BasicRiskStrategyConfig {
    pub is_used_signal_k_line_stop_loss: bool, //(开仓K线止盈止损),多单时,当价格低于入场k线的最低价时,止损;空单时,
    // 价格高于入场k线的最高价时,止损
    pub max_loss_percent: f64, // 最大止损百分比(避免当k线振幅过大，使用k线最低/高价止损时候，造成太大的亏损)
    pub take_profit_ratio: f64, // 止盈比例，比如当盈利超过1.5:1时，直接止盈，适用短线策略
    // 1:1时候设置止损价格为开仓价格(保本)，价格到达赢利点1:2的时候，设置止损价格为开仓价格+1:1(保证本金+1:1的利润),当赢利点达到1：3的时候，设置止损价格为开仓价格+1:2(保证本金+1:2的利润)
    pub is_one_k_line_diff_stop_loss: bool, // 是否使用固定止损最大止损为1:1开多+(当前k线的最高价-最低价) 开空-
                                            // (当前k线的最高价-最低价)
}

impl Default for BasicRiskStrategyConfig {
    fn default() -> Self {
        Self {
            is_used_signal_k_line_stop_loss: true,
            max_loss_percent: 0.02,              // 默认3%止损
            take_profit_ratio: 0.00,             // 默认1%盈利开始启用动态止盈
            is_one_k_line_diff_stop_loss: false, // 默认不使用移动止损
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
    ema_signal_value.ema6_value = ema_indicator.ema6_indicator.next(data.c());
    ema_signal_value.ema7_value = ema_indicator.ema7_indicator.next(data.c());

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
        warn!(duration_ms = ema_start.elapsed().as_millis(), "计算EMA");
    }

    // 计算volume - 避免重复调用data_item.v()
    let volume_start = Instant::now();
    if let Some(volume_indicator) = &mut indicator_combine.volume_indicator {
        vegas_indicator_signal_value.volume_value.volume_value = volume;
        vegas_indicator_signal_value.volume_value.volume_ratio = volume_indicator.next(volume);
        vegas_indicator_signal_value
            .volume_value
            .is_increasing_than_pre = volume_indicator.is_increasing_than_pre();
        vegas_indicator_signal_value
            .volume_value
            .is_decreasing_than_pre = volume_indicator.is_decreasing_than_pre();
    }
    if volume_start.elapsed().as_millis() > 10 {
        warn!(
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
        warn!(duration_ms = rsi_start.elapsed().as_millis(), "计算RSI");
    }

    // 计算bollinger - 同样避免重复调用
    let bb_start = Instant::now();
    if let Some(bollinger_indicator) = &mut indicator_combine.bollinger_indicator {
        let bollinger_value = bollinger_indicator.next(data_item);
        vegas_indicator_signal_value.bollinger_value.upper = bollinger_value.upper;
        vegas_indicator_signal_value.bollinger_value.lower = bollinger_value.lower;
        vegas_indicator_signal_value.bollinger_value.middle = bollinger_value.average;
        vegas_indicator_signal_value
            .bollinger_value
            .consecutive_touch_times = bollinger_value.consecutive_touch_times;
    }
    if bb_start.elapsed().as_millis() > 10 {
        warn!(
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
        vegas_indicator_signal_value.kline_hammer_value = KlineHammerSignalValue {
            is_hammer: kline_hammer_value.is_hammer,
            is_hanging_man: kline_hammer_value.is_hanging_man,
            down_shadow_ratio: kline_hammer_value.down_shadow_ratio,
            up_shadow_ratio: kline_hammer_value.up_shadow_ratio,
            body_ratio: kline_hammer_value.body_ratio,
            is_long_signal: false,
            is_short_signal: false,
        };
        // vegas_indicator_signal_value.kline_hammer_value.is_hammer = kline_hammer_value.is_hammer;
        // vegas_indicator_signal_value
        //     .kline_hammer_value
        //     .is_hanging_man = kline_hammer_value.is_hanging_man;
        // vegas_indicator_signal_value
        //     .kline_hammer_value
        //     .down_shadow_ratio = kline_hammer_value.down_shadow_ratio;
        // vegas_indicator_signal_value
        //     .kline_hammer_value
        //     .up_shadow_ratio = kline_hammer_value.up_shadow_ratio;
        // vegas_indicator_signal_value.kline_hammer_value.body_ratio = kline_hammer_value.body_ratio;
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
    use tracing::{info, warn};
    // 初始化阶段
    let mut trading_state = TradingState::default();
    use std::collections::VecDeque;
    let mut candle_item_list: VecDeque<CandleItem> = VecDeque::with_capacity(candles_list.len());
    // 基于指标组合动态计算回看窗口
    let dynamic_lookback = indicator_combine
        .max_required_lookback()
        .max(min_data_length);

    for (i, candle) in candles_list.iter().enumerate() {
        // 计算指标值
        let mut multi_indicator_values = get_multi_indicator_values(indicator_combine, &candle);

        // 将新数据添加到列表，如果超过最大回溯期，删除最旧的数据
        candle_item_list.push_back(candle.clone());
        if candle_item_list.len() > dynamic_lookback {
            let _ = candle_item_list.pop_front();
        }
        // 计算交易信号
        // 在热身期内不生成交易信号
        // if candle_item_list.len() < dynamic_lookback {
        //     continue;
        // }
        let mut signal = strategy(
            candle_item_list.make_contiguous(),
            &mut multi_indicator_values,
        );

        // 处理交易信号前检查是否值得处理（性能优化）
        let should_process_signal = signal.should_buy
            || signal.should_sell
            || trading_state.trade_position.is_some()
            || trading_state.last_signal_result.is_some();

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
        open_trades: trading_state.open_position_times,
        trade_records: trading_state.trade_records,
    };

    result
}

/// 通用回测引擎：支持自定义指标组合与指标值结构
pub fn run_back_test_generic<IC, IV>(
    mut strategy: impl FnMut(&[CandleItem], &mut IV) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IC,
    mut build_values: impl FnMut(&mut IC, &CandleItem) -> IV,
) -> BackTestResult {
    use tracing::{info, warn};
    let mut trading_state = TradingState::default();
    use std::collections::VecDeque;
    let mut candle_item_list: VecDeque<CandleItem> = VecDeque::with_capacity(candles_list.len());
    // 由调用方控制所需窗口，这里仅保证最小长度
    let dynamic_lookback = min_data_length;

    for (i, candle) in candles_list.iter().enumerate() {
        // 计算自定义指标
        let mut multi_indicator_values = build_values(indicator_combine, &candle);

        candle_item_list.push_back(candle.clone());
        if candle_item_list.len() > dynamic_lookback {
            let _ = candle_item_list.pop_front();
        }
        if candle_item_list.len() < dynamic_lookback {
            continue;
        }

        let mut signal = strategy(
            candle_item_list.make_contiguous(),
            &mut multi_indicator_values,
        );
        if i < 500 {
            continue;
        }

        let should_process_signal = signal.should_buy
            || signal.should_sell
            || trading_state.trade_position.is_some()
            || trading_state.last_signal_result.is_some();

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
    finalize_trading_state(&mut trading_state, &candle_item_list);
    BackTestResult {
        funds: trading_state.funds,
        win_rate: calculate_win_rate(trading_state.wins, trading_state.losses),
        open_trades: trading_state.open_position_times,
        trade_records: trading_state.trade_records,
    }
}

pub fn parse_candle_to_data_item(candle: &CandlesEntity) -> CandleItem {
    CandleItem::builder()
        .c(candle.c.parse::<f64>().unwrap())
        .v(candle.vol_ccy.parse::<f64>().unwrap())
        .h(candle.h.parse::<f64>().unwrap())
        .l(candle.l.parse::<f64>().unwrap())
        .o(candle.o.parse::<f64>().unwrap())
        .confirm(candle.confirm.parse::<i32>().unwrap())
        .ts(candle.ts)
        .build()
        .unwrap()
}

fn finalize_trading_state(
    trading_state: &mut TradingState,
    candle_item_list: &VecDeque<CandleItem>,
) {
    if trading_state.trade_position.is_some() {
        let mut trade_position = trading_state.trade_position.clone().unwrap();
        let last_candle = candle_item_list.back().unwrap();
        let last_price = last_candle.c;
        trade_position.close_price = Some(last_price);

        let profit = match trade_position.trade_side {
            TradeSide::Long => {
                (last_price - trade_position.open_price) * trade_position.position_nums
            }
            TradeSide::Short => {
                (trade_position.open_price - last_price) * trade_position.position_nums
            }
            _ => 0.0,
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
                signal_kline_stop_loss_price: None,
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
    risk_config: &BasicRiskStrategyConfig,
    mut trading_state: TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
    candle_item_list: &VecDeque<CandleItem>,
    i: usize,
) -> TradingState {
    let current_open_price = signal.open_price;
    let current_low_price = candle.l;
    let current_high_price = candle.h;
    let current_close_price = candle.c;

    let mut trade_position = trading_state.trade_position.clone().unwrap();
    let entry_price = trade_position.open_price; // 先保存入场价格
    let position_nums = trade_position.position_nums.clone();

    //检查移动止盈
    if risk_config.is_one_k_line_diff_stop_loss {
        //如果设置了移动止盈价格
        if let Some(move_stop_loss_price) = trade_position.move_take_profit_price {
            match trade_position.trade_side {
                TradeSide::Long => {
                    if current_low_price <= move_stop_loss_price {
                        trade_position.close_price = Some(move_stop_loss_price);
                        //重新赋值
                        trading_state.trade_position = Some(trade_position.clone());
                        close_position(
                            &mut trading_state,
                            candle,
                            &signal,
                            "移动止盈",
                            (current_close_price - entry_price) * position_nums,
                        );
                        return trading_state;
                    }
                }
                TradeSide::Short => {
                    if current_high_price >= move_stop_loss_price {
                        trade_position.close_price = Some(move_stop_loss_price);
                        //重新赋值
                        trading_state.trade_position = Some(trade_position.clone());
                        close_position(
                            &mut trading_state,
                            candle,
                            &signal,
                            "移动止盈",
                            (entry_price - current_close_price) * position_nums,
                        );
                        return trading_state;
                    }
                }
                _ => {
                    // do nothing
                }
            }
        } else {
            if trade_position.touch_take_profit_price.is_some() {
                match trade_position.trade_side {
                    TradeSide::Long => {
                        //如果开仓后k线的最低价格和投入价格的差值 > 开仓信号线路最低价格和最高价格的差值，则设置最低止盈价格
                        if current_high_price > trade_position.touch_take_profit_price.unwrap() {
                            trade_position.move_take_profit_price = Some(entry_price * 0.99564);
                            trading_state.trade_position = Some(trade_position.clone());
                        }
                    }
                    TradeSide::Short => {
                        if current_low_price < trade_position.touch_take_profit_price.unwrap() {
                            //如果开仓后第一根k线有盈利，则设置止损价格为开仓价,保存本金
                            trade_position.move_take_profit_price = Some(entry_price * 1.00436);
                            trading_state.trade_position = Some(trade_position.clone());
                        }
                    }
                    _ => {
                        // do nothing
                    }
                }
            }
        }
    }

    //检查按收益比例止盈
    if risk_config.take_profit_ratio > 0.0 {
        match trade_position.trade_side {
            TradeSide::Long => {
                if current_high_price >= trade_position.touch_take_profit_price.unwrap() {
                    trade_position.signal_kline_stop_close_price = Some(trade_position.open_price);
                    //重新赋值
                    trading_state.trade_position = Some(trade_position.clone());
                    let profit = (trade_position.touch_take_profit_price.unwrap() - entry_price)
                        * trade_position.position_nums;

                    trade_position.close_price =
                        Some(trade_position.touch_take_profit_price.unwrap());
                    //重新赋值
                    trading_state.trade_position = Some(trade_position);
                    close_position(
                        &mut trading_state,
                        candle,
                        &signal,
                        "按收益比例止盈",
                        profit,
                    );
                    return trading_state;
                }
            }
            TradeSide::Short => {
                if current_low_price <= trade_position.touch_take_profit_price.unwrap() {
                    trade_position.signal_kline_stop_close_price = Some(trade_position.open_price);
                    //重新赋值
                    trading_state.trade_position = Some(trade_position.clone());
                    let profit = (entry_price - trade_position.touch_take_profit_price.unwrap())
                        * trade_position.position_nums;
                    trade_position.close_price =
                        Some(trade_position.touch_take_profit_price.unwrap());
                    //重新赋值
                    trading_state.trade_position = Some(trade_position);
                    close_position(
                        &mut trading_state,
                        candle,
                        &signal,
                        "按收益比例止盈",
                        profit,
                    );
                    return trading_state;
                }
            }
            _ => {}
        }
    }

    // 计算盈亏率
    let profit_pct = match trade_position.trade_side {
        TradeSide::Long => (current_low_price - entry_price) / entry_price,
        TradeSide::Short => {
            (entry_price - current_high_price) / entry_price // 做空的盈亏计算
        }
        _ => 0.0,
    };

    //计算盈亏
    let profit = match trade_position.trade_side {
        TradeSide::Long => (current_close_price - entry_price) * trade_position.position_nums,
        TradeSide::Short => (entry_price - current_close_price) * trade_position.position_nums,
        _ => 0.0,
    };

    //检查是否设置了最优止盈价格
    if let Some(best_take_profit_price) = trade_position.best_take_profit_price {
        match trade_position.trade_side {
            TradeSide::Long => {
                if current_high_price > best_take_profit_price {
                    let profit =
                        (best_take_profit_price - entry_price) * trade_position.position_nums;
                    trade_position.close_price = Some(best_take_profit_price);
                    //重新赋值
                    trading_state.trade_position = Some(trade_position);
                    close_position(&mut trading_state, candle, &signal, "最优止盈", profit);
                    return trading_state;
                }
            }
            TradeSide::Short => {
                if current_low_price < best_take_profit_price {
                    let profit =
                        (entry_price - best_take_profit_price) * trade_position.position_nums;
                    trade_position.close_price = Some(best_take_profit_price);
                    //重新赋值
                    trading_state.trade_position = Some(trade_position);
                    close_position(&mut trading_state, candle, &signal, "最优止盈", profit);
                    return trading_state;
                }
            }
            _ => {
                // do nothing
            }
        }
    }

    //先检查设置了是否预止损价格
    if let Some(signal_kline_stop_close_price) = trade_position.signal_kline_stop_close_price {
        match trade_position.trade_side.clone() {
            TradeSide::Long => {
                if current_close_price <= signal_kline_stop_close_price {
                    //重新计算利润
                    trade_position.close_price = Some(signal_kline_stop_close_price);
                    let profit = (signal_kline_stop_close_price - entry_price) * position_nums;
                    //重新赋值
                    trading_state.trade_position = Some(trade_position);
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
            TradeSide::Short => {
                if current_close_price >= signal_kline_stop_close_price {
                    //重新计算利润
                    trade_position.close_price = Some(signal_kline_stop_close_price);
                    let profit = (entry_price - signal_kline_stop_close_price) * position_nums;
                    //重新赋值
                    trading_state.trade_position = Some(trade_position);
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
            _ => {
                // do nothing
            }
        }
    }

    // 最后再检查最大止损
    if profit_pct < -risk_config.max_loss_percent {
        // println!(">>> 触发止损 <<< 开仓价:{}, 当前价:{}, 盈亏率:{:.2}% < 止损线:{:.2}%", entry_price, current_price, profit_pct * 100.0, -strategy_config.max_loss_percent * 100.0);
        trade_position.close_price = Some(current_open_price);
        //重新赋值
        trading_state.trade_position = Some(trade_position);
        close_position(&mut trading_state, candle, &signal, "最大亏损止损", profit);
        return trading_state;
    }
    trading_state
}

pub fn deal_signal(
    mut trading_state: TradingState,
    signal: &mut SignalResult,
    candle: &CandleItem,
    risk_config: BasicRiskStrategyConfig,
    candle_item_list: &VecDeque<CandleItem>,
    i: usize,
) -> TradingState {
    if signal.should_buy || signal.should_sell {
        if let Some(trade_position) = trading_state.trade_position.clone() {
            //如是反向仓位，优先判断一下止盈止损
            if (trade_position.trade_side == TradeSide::Long && signal.should_sell)
                || (trade_position.trade_side == TradeSide::Short && signal.should_buy)
            {
                trading_state = check_risk_config(
                    &risk_config,
                    trading_state,
                    signal,
                    candle,
                    candle_item_list,
                    i,
                );
            }
        }

        //使用更优点位开仓
        if signal.best_open_price.is_some() {
            trading_state.last_signal_result = Some(signal.clone());
        } else {
            trading_state.last_signal_result = None;
        }

        // 处理策略信号
        if signal.should_buy {
            handle_buy_signal_logic(risk_config, &mut trading_state, signal, candle);
        } else if signal.should_sell {
            handle_sell_signal_logic(risk_config, &mut trading_state, signal, candle);
        }
    } else {
        // 如果有持仓, 先进行风险检查
        if trading_state.trade_position.is_some() {
            trading_state = check_risk_config(
                &risk_config,
                trading_state,
                signal,
                candle,
                candle_item_list,
                i,
            );
        } else if trading_state.last_signal_result.is_some() {
            //要确保大于信号的开仓时间
            if candle.ts >= trading_state.last_signal_result.clone().unwrap().ts {
                let last_signal_result = trading_state.last_signal_result.clone().unwrap();
                if last_signal_result.should_buy {
                    //如果信号是买，但是当前价格低于信号的最优开仓价格，则使用信号的最优开仓价格
                    if candle.l <= last_signal_result.best_open_price.unwrap() {
                        signal.open_price = last_signal_result.best_open_price.unwrap();
                        signal.should_buy = true;
                        signal.signal_kline_stop_loss_price =
                            last_signal_result.signal_kline_stop_loss_price;
                        signal.single_value = last_signal_result.single_value;
                        signal.single_result = last_signal_result.single_result;

                        trading_state.last_signal_result = None;
                        let signal_open_position_time =
                            Some(rust_quant_common::utils::time::mill_time_to_datetime(last_signal_result.ts).unwrap());
                        open_long_position(
                            risk_config,
                            &mut trading_state,
                            candle,
                            signal,
                            signal_open_position_time,
                        );
                    }
                } else if last_signal_result.should_sell {
                    //如果信号是卖，但是当前价格高于信号的最优开仓价格，则使用信号的最优开仓价格
                    if candle.h > last_signal_result.best_open_price.unwrap() {
                        signal.open_price = last_signal_result.best_open_price.unwrap();
                        signal.should_sell = true;
                        signal.signal_kline_stop_loss_price =
                            last_signal_result.signal_kline_stop_loss_price;
                        signal.single_value = last_signal_result.single_value;
                        signal.single_result = last_signal_result.single_result;

                        trading_state.last_signal_result = None;
                        let signal_open_position_time =
                            Some(rust_quant_common::utils::time::mill_time_to_datetime(last_signal_result.ts).unwrap());
                        open_short_position(
                            risk_config,
                            &mut trading_state,
                            candle,
                            signal,
                            signal_open_position_time,
                        );
                    }
                }
            }
        }
    }
    trading_state
}

/// 处理买入信号的逻辑
fn handle_buy_signal_logic(
    risk_config: BasicRiskStrategyConfig,
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) {
    if trading_state.trade_position.is_none() {
        //不使用最优开仓价格，直接开多仓
        open_long_position(risk_config, trading_state, candle, signal, None);
    } else if trading_state.trade_position.is_some() {
        let mut trade_position = trading_state.trade_position.clone().unwrap();
        if trade_position.trade_side == TradeSide::Short {
            // 持有空单，先平空单
            let profit =
                (trade_position.open_price - signal.open_price) * trade_position.position_nums;
            trade_position.close_price = Some(signal.open_price);
            //重新赋值
            trading_state.trade_position = Some(trade_position);
            close_position(trading_state, candle, signal, "反向信号触发平仓", profit);

            // 然后开多仓
            open_long_position(risk_config, trading_state, candle, signal, None);
        }
    } else {
        //todo 如果已持有多单，则不执行任何操作
    }
}

/// 处理卖出信号的逻辑
fn handle_sell_signal_logic(
    risk_config: BasicRiskStrategyConfig,
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) {
    if trading_state.trade_position.is_none() {
        //不使用最优开仓价格，直接开空仓
        open_short_position(risk_config, trading_state, candle, signal, None);
    } else if trading_state.trade_position.is_some()
        && trading_state.trade_position.clone().unwrap().trade_side == TradeSide::Long
    {
        let mut trade_position = trading_state.trade_position.clone().unwrap();
        // 持有多单，先平多单
        let profit = (signal.open_price - trade_position.open_price) * trade_position.position_nums;
        trade_position.close_price = Some(signal.open_price);

        //重新赋值
        trading_state.trade_position = Some(trade_position);
        close_position(trading_state, candle, signal, "反向信号平仓", profit);

        // 然后开空仓
        open_short_position(risk_config, trading_state, candle, signal, None);
    } else {
        //todo  如果已持有空单，则不执行任何操作
    }
}

#[derive(Debug, Clone, Default)]
pub struct TradePosition {
    //持仓数量
    pub position_nums: f64,
    //实际开仓价格
    pub open_price: f64,
    //实际平仓价格
    pub close_price: Option<f64>,
    //盈亏
    pub profit_loss: f64,
    //斐波那契触发价格
    pub triggered_fib_levels: HashSet<usize>,
    //交易方向
    pub trade_side: TradeSide,
    //是否使用最优开仓价格
    pub is_use_best_open_price: bool,
    //信号开仓时间
    pub signal_open_position_time: Option<String>,
    //实际开仓时间
    pub open_position_time: String,
    //最优止盈价格
    pub best_take_profit_price: Option<f64>,
    //信号线止损价格
    pub signal_kline_stop_close_price: Option<f64>,
    //触发移动最少止盈价格
    pub touch_take_profit_price: Option<f64>,
    //触发移动止盈价格
    pub move_take_profit_price: Option<f64>,
    //信号状态
    pub signal_status: i32,
    //信号线最高最低价差
    pub signal_high_low_diff: f64,
}

/// 交易状态结构体
#[derive(Debug, Clone)]
pub struct TradingState {
    //资金
    pub funds: f64,
    //盈利次数
    pub wins: i64,
    //亏损次数
    pub losses: i64,
    //开仓次数
    pub open_position_times: usize,
    //上一次信号结果
    pub last_signal_result: Option<SignalResult>,
    //总盈亏
    pub total_profit_loss: f64,
    //持仓记录
    pub trade_records: Vec<TradeRecord>,
    //交易持仓
    pub trade_position: Option<TradePosition>,
}
impl Default for TradingState {
    fn default() -> Self {
        Self {
            funds: 100.0,
            wins: 0,
            losses: 0,
            open_position_times: 0,
            last_signal_result: None,
            total_profit_loss: 0.0,
            trade_records: Vec::with_capacity(3000),
            trade_position: None,
        }
    }
}

/// 开多仓
fn open_long_position(
    risk_config: BasicRiskStrategyConfig,
    state: &mut TradingState,
    candle: &CandleItem,
    signal: &SignalResult,
    signal_open_time: Option<String>,
) {
    //判断是否需要等待最优开仓位置
    if state.last_signal_result.is_some() {
        return;
    }
    let mut temp_trade_position = TradePosition {
        position_nums: state.funds / signal.open_price,
        open_price: signal.open_price,
        open_position_time: rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap(),
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Long,
        ..Default::default()
    };
    // 如果启用了设置预止损价格,则根据开仓方向设置预止损价格
    if risk_config.is_used_signal_k_line_stop_loss {
        temp_trade_position.signal_kline_stop_close_price = signal.signal_kline_stop_loss_price;
    }
    //如果信号有最优止盈价格，则设置最优止盈价格
    if signal.best_take_profit_price.is_some() {
        // 如果持仓为0，则设置最优止盈价格,否则不进行更新
        temp_trade_position.best_take_profit_price = signal.best_take_profit_price;
    }

    // 如果启用了移动止盈,则设置移动止盈价格为当前k线的最高价
    if risk_config.take_profit_ratio > 0.0 {
        if signal.signal_kline_stop_loss_price.is_none() {
            error!("signal_kline_stop_loss_price is none");
        }
        temp_trade_position.signal_high_low_diff = (signal.signal_kline_stop_loss_price.unwrap() - signal.open_price).abs();
        temp_trade_position.touch_take_profit_price = Some(
            signal.open_price
                + temp_trade_position.signal_high_low_diff * risk_config.take_profit_ratio,
        );
    }

    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;
    // state.position = state.funds / signal.open_price;
    // state.initial_quantity = state.position;
    // state.open_price = signal.open_price;
    // state.open_position_time = rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap();
    // state.open_trades += 1;
    // state.total_profit_loss = 0.0;
    // state.trade_side = Some(TradeSide::Long);
    // state.last_signal_result = None;
    // state.is_use_best_open_price = false;

    record_trade_entry(state, PositionSide::Long.as_str().to_owned(), signal);
}

/// 开空仓
fn open_short_position(
    risk_config: BasicRiskStrategyConfig,
    state: &mut TradingState,
    candle: &CandleItem,
    signal: &SignalResult,
    signal_open_time: Option<String>,
) {
    if state.last_signal_result.is_some() {
        return;
    }
    let mut trade_position = TradePosition {
        position_nums: state.funds / signal.open_price,
        open_price: signal.open_price,
        open_position_time: rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap(),
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Short,
        ..Default::default()
    };
    if signal.best_take_profit_price.is_some() {
        // 如果持仓为0，则设置最优止盈价格,否则不进行更新
        trade_position.best_take_profit_price = signal.best_take_profit_price;
    }
    // 如果启用了设置预止损价格,则根据开仓方向设置预止损价格
    if risk_config.is_used_signal_k_line_stop_loss {
        trade_position.signal_kline_stop_close_price = signal.signal_kline_stop_loss_price;
    }
    //如果启用了按比例止盈,（开仓价格-止损价格）*比例
    if risk_config.take_profit_ratio > 0.0 {
        if signal.signal_kline_stop_loss_price.is_none() {
            error!("signal_kline_stop_loss_price is none");
        }
        trade_position.signal_high_low_diff =
            (signal.signal_kline_stop_loss_price.unwrap() - signal.open_price).abs();
        trade_position.touch_take_profit_price = Some(
            signal.open_price - trade_position.signal_high_low_diff * risk_config.take_profit_ratio,
        );
    }

    state.trade_position = Some(trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;
    // state.position = state.funds / signal.open_price;
    // state.initial_quantity = state.position;
    // state.open_price = signal.open_price;
    // state.open_position_time = rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap();
    // state.open_trades += 1;
    // state.total_profit_loss = 0.0;
    // state.trade_side = Some(TradeSide::Short);
    // state.last_signal_result = None;
    // state.is_use_best_open_price = false;

    record_trade_entry(state, PositionSide::Short.as_str().to_owned(), &signal);
}

/// 记录交易入场
fn record_trade_entry(state: &mut TradingState, option_type: String, signal: &SignalResult) {
    //批量回测的时候不进行记录
    let trade_position = state.trade_position.clone().unwrap();
    //随机测试的时候不记录详情日志了
    if env::var("ENABLE_RANDOM_TEST").unwrap_or_default() == "true" {
        return;
    }
    state.trade_records.push(TradeRecord {
        option_type,
        open_position_time: trade_position.open_position_time.clone(),
        close_position_time: Some(trade_position.open_position_time.clone()),
        //开仓价格
        open_price: trade_position.open_price,
        //信号开仓价格
        signal_open_position_time: trade_position.signal_open_position_time.clone(),
        //信号状态
        signal_status: trade_position.signal_status as i32,
        //平仓价格
        close_price: trade_position.close_price.clone(),
        profit_loss: trade_position.profit_loss,
        quantity: trade_position.position_nums,
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
    let exit_time = rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap();
    let mut trade_position = state.trade_position.clone().unwrap();
    let quantity = trade_position.position_nums;

    //手续费设定0.007,假设开仓平仓各收一次 (数量*价格 *0.07%)
    let fee = quantity * trade_position.open_price * 0.0007;
    let profit_after_fee = profit - fee;
    trade_position.profit_loss = profit_after_fee;
    //重新赋值
    state.trade_position = Some(trade_position);

    //更新总利润和资金
    state.total_profit_loss += profit_after_fee;
    state.funds += profit_after_fee;

    //更新胜率
    if profit > 0.0 {
        state.wins += 1;
    } else {
        state.losses += 1;
    }

    // 根据平仓原因和盈亏设置正确的平仓类型
    record_trade_exit(state, exit_time, signal, close_type, quantity);

    // 更新总利润和资金
    state.trade_position = None;
}

/// 记录交易出场
fn record_trade_exit(
    state: &mut TradingState,
    exit_time: String,
    signal: &SignalResult,
    close_type: &str,
    closing_quantity: f64, // Add parameter for quantity being closed
) {
    let trade_position = state.trade_position.clone().unwrap();
    //随机测试的时候不记录详情日志了
    if env::var("ENABLE_RANDOM_TEST").unwrap_or_default() == "true" {
        return;
    }
    state.trade_records.push(TradeRecord {
        option_type: "close".to_string(),
        open_position_time: trade_position.open_position_time.clone(),
        signal_open_position_time: trade_position.signal_open_position_time.clone(),
        close_position_time: Some(exit_time),
        open_price: trade_position.open_price,
        close_price: trade_position.close_price.clone(),
        signal_status: trade_position.signal_status as i32,
        profit_loss: trade_position.profit_loss,
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
//
