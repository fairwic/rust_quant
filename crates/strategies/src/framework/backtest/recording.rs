use super::types::{SignalResult, TradeRecord, TradingState};
use std::env;

/// 记录交易入场
pub fn record_trade_entry(state: &mut TradingState, option_type: String, signal: &SignalResult) {
    // 批量回测的时候不进行记录
    let trade_position = state.trade_position.clone().unwrap();
    // 随机测试的时候不记录详情日志了
    if env::var("ENABLE_RANDOM_TEST").unwrap_or_default() == "true" {
        return;
    }
    state.trade_records.push(TradeRecord {
        option_type,
        open_position_time: trade_position.open_position_time.clone(),
        close_position_time: Some(trade_position.open_position_time.clone()),
        open_price: trade_position.open_price,
        signal_open_position_time: trade_position.signal_open_position_time.clone(),
        signal_status: trade_position.signal_status as i32,
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

/// 记录交易出场
pub fn record_trade_exit(
    state: &mut TradingState,
    exit_time: String,
    signal: &SignalResult,
    close_type: &str,
    closing_quantity: f64,
) {
    let trade_position = state.trade_position.clone().unwrap();
    // 随机测试的时候不记录详情日志了
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
        quantity: closing_quantity,
        full_close: true,
        close_type: close_type.to_string(),
        win_num: state.wins,
        loss_num: state.losses,
        signal_value: signal.single_value.clone(),
        signal_result: signal.single_result.clone(),
    });
}
