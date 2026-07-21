use super::types::{SignalResult, TradeRecord, TradingState};
use rust_quant_core::config::random_backtest_is_enabled;

/// 按记录数量计算冻结初始止损对应的价格风险金额。
fn initial_risk_amount(
    initial_stop_price: Option<f64>,
    open_price: f64,
    quantity: f64,
) -> Option<f64> {
    initial_stop_price
        .filter(|price| price.is_finite())
        .map(|price| (open_price - price).abs() * quantity)
        .filter(|risk| risk.is_finite() && *risk > 0.0)
}
/// 记录交易入场
pub fn record_trade_entry(state: &mut TradingState, option_type: String, signal: &SignalResult) {
    // 批量回测的时候不进行记录
    let trade_position = state.trade_position.clone().unwrap();
    // 随机测试的时候不记录详情日志了
    if random_backtest_is_enabled() {
        return;
    }
    state.trade_records.push(TradeRecord {
        option_type,
        open_position_time: trade_position.open_position_time.clone(),
        close_position_time: Some(trade_position.open_position_time.clone()),
        open_price: trade_position.open_price,
        signal_open_position_time: trade_position.signal_open_position_time.clone(),
        signal_status: trade_position.signal_status,
        close_price: trade_position.close_price,
        profit_loss: trade_position.profit_loss,
        quantity: trade_position.position_nums,
        full_close: false,
        close_type: "".to_string(),
        win_num: 0,
        loss_num: 0,
        signal_value: signal.single_value.clone(),
        signal_result: signal.single_result.clone(),
        stop_loss_source: None,         // 开仓时不记录止损来源
        stop_loss_update_history: None, // 开仓时不记录止损更新历史
        initial_stop_price: trade_position.initial_stop_price,
        initial_risk_amount: initial_risk_amount(
            trade_position.initial_stop_price,
            trade_position.open_price,
            trade_position.position_nums,
        ),
        net_profit_r: None,
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
    record_trade_exit_with_full_close(state, exit_time, signal, close_type, closing_quantity, true);
}

/// 记录交易出场，并显式区分部分平仓与最终平仓。
pub fn record_trade_exit_with_full_close(
    state: &mut TradingState,
    exit_time: String,
    signal: &SignalResult,
    close_type: &str,
    closing_quantity: f64,
    full_close: bool,
) {
    let trade_position = state.trade_position.clone().unwrap();
    // 随机测试的时候不记录详情日志了
    if random_backtest_is_enabled() {
        return;
    }
    let risk_amount = initial_risk_amount(
        trade_position.initial_stop_price,
        trade_position.open_price,
        closing_quantity,
    );
    state.trade_records.push(TradeRecord {
        option_type: "close".to_string(),
        open_position_time: trade_position.open_position_time.clone(),
        signal_open_position_time: trade_position.signal_open_position_time.clone(),
        close_position_time: Some(exit_time),
        open_price: trade_position.open_price,
        close_price: trade_position.close_price,
        signal_status: trade_position.signal_status,
        profit_loss: trade_position.profit_loss,
        quantity: closing_quantity,
        full_close,
        close_type: close_type.to_string(),
        win_num: state.wins,
        loss_num: state.losses,
        signal_value: signal.single_value.clone(),
        signal_result: signal.single_result.clone(),
        // 只有触发信号K线止损时才记录止损来源
        stop_loss_source: if close_type == "Signal_Kline_Stop_Loss" {
            trade_position.stop_loss_source.clone()
        } else {
            None
        },
        // 只有平仓时记录止损更新历史
        stop_loss_update_history: if trade_position.stop_loss_updates.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&trade_position.stop_loss_updates).unwrap_or_default())
        },
        initial_stop_price: trade_position.initial_stop_price,
        initial_risk_amount: risk_amount,
        net_profit_r: risk_amount.map(|risk| trade_position.profit_loss / risk),
    });
}
