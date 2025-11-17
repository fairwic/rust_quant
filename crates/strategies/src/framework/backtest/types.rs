use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use super::super::types::TradeSide;

/// 回测结果
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

/// 交易记录
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

/// 信号结果
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
    //移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
    pub move_stop_open_price_when_touch_price: Option<f64>,
    pub ts: i64,
    pub single_value: Option<String>,
    pub single_result: Option<String>,
}

/// 持仓信息
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

    //触发K线固定比例止盈价格
    pub touch_take_ratio_profit_price: Option<f64>,

    //触发K线开仓价格止损(当达到一个特定的价格位置的时候，移动止损线到开仓价格)
    pub move_stop_open_price: Option<f64>,
    //触发K线开仓价格止损(当达到一个特定的价格位置的时候，移动止损线到开仓价格)
    pub move_stop_open_price_when_touch_price: Option<f64>,
    //信号状态
    pub signal_status: i32,
    //信号线最高最低价差
    pub signal_high_low_diff: f64,
}

/// 交易状态
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

/// 止盈止损策略配置
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct BasicRiskStrategyConfig {
    pub is_used_signal_k_line_stop_loss: bool, //(开仓K线止盈止损),多单时,当价格低于入场k线的最低价时,止损;空单时,
    // 价格高于入场k线的最高价时,止损
    pub max_loss_percent: f64, // 最大止损百分比(避免当k线振幅过大，使用k线最低/高价止损时候，造成太大的亏损)
    pub take_profit_ratio: f64, // 止盈比例，比如当盈利超过1.5:1时，直接止盈，适用短线策略
    // 1:1时候设置止损价格为开仓价格(保本)，价格到达赢利点1:2的时候，设置止损价格为开仓价格+1:1(保证本金+1:1的利润),当赢利点达到1：3的时候，设置止损价格为开仓价格+1:2(保证本金+1:2的利润)
    pub is_one_k_line_diff_stop_loss: bool, // 是否使用固定止损最大止损为1:1开多+(当前k线的最高价-最低价) 开空-
    pub is_move_stop_open_price_when_touch_price: bool, // 是否使用移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
    // (当前k线的最高价-最低价)
}

impl Default for BasicRiskStrategyConfig {
    fn default() -> Self {
        Self {
            is_used_signal_k_line_stop_loss: true,
            max_loss_percent: 0.02,              // 默认3%止损
            take_profit_ratio: 0.00,             // 默认1%盈利开始启用动态止盈
            is_one_k_line_diff_stop_loss: false, // 默认不使用移动止损(移动止损价格到信号线的开仓价格)
            is_move_stop_open_price_when_touch_price: false, // 默认不使用移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
        }
    }
}

/// 移动止损
pub struct MoveStopLoss {
    pub is_long: bool,
    pub is_short: bool,
    pub price: f64,
}

