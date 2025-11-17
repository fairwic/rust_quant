use super::super::types::TradeSide;
use super::recording::record_trade_entry;
use super::types::{BasicRiskStrategyConfig, SignalResult, TradePosition, TradingState};
use crate::CandleItem;
use okx::dto::common::PositionSide;
use okx::dto::EnumToStrTrait;
use std::collections::VecDeque;
use tracing::error;

/// 最终平仓处理
pub fn finalize_trading_state(
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
                move_stop_open_price_when_touch_price: None,
            },
            "结束平仓",
            profit,
        );
    }
}

/// 开多仓
pub fn open_long_position(
    risk_config: BasicRiskStrategyConfig,
    state: &mut TradingState,
    candle: &CandleItem,
    signal: &SignalResult,
    signal_open_time: Option<String>,
) {
    // 判断是否需要等待最优开仓位置
    if state.last_signal_result.is_some() {
        return;
    }
    let mut temp_trade_position = TradePosition {
        position_nums: state.funds / signal.open_price,
        open_price: signal.open_price,
        open_position_time: rust_quant_common::utils::time::mill_time_to_datetime(candle.ts)
            .unwrap(),
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Long,
        ..Default::default()
    };
    // 如果启用了设置预止损价格,则根据开仓方向设置预止损价格
    if risk_config.is_used_signal_k_line_stop_loss {
        temp_trade_position.signal_kline_stop_close_price = signal.signal_kline_stop_loss_price;
    }
    // 如果信号有最优止盈价格，则设置最优止盈价格
    if signal.best_take_profit_price.is_some() {
        temp_trade_position.best_take_profit_price = signal.best_take_profit_price;
    }

    // 如果启用了移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近,则设置移动止损价格
    if risk_config.is_move_stop_open_price_when_touch_price {
        temp_trade_position.move_stop_open_price_when_touch_price =
            signal.move_stop_open_price_when_touch_price;
    }

    // 如果启用了固定比例止盈,则设置固定比例止盈价格
    if risk_config.take_profit_ratio > 0.0 {
        if signal.signal_kline_stop_loss_price.is_none() {
            error!("signal_kline_stop_loss_price is none");
        }
        temp_trade_position.signal_high_low_diff =
            (signal.signal_kline_stop_loss_price.unwrap() - signal.open_price).abs();

        temp_trade_position.touch_take_ratio_profit_price = Some(
            signal.open_price
                + temp_trade_position.signal_high_low_diff * risk_config.take_profit_ratio,
        );
    }
   

    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;

    record_trade_entry(state, PositionSide::Long.as_str().to_owned(), signal);
}

/// 开空仓
pub fn open_short_position(
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
        open_position_time: rust_quant_common::utils::time::mill_time_to_datetime(candle.ts)
            .unwrap(),
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Short,
        ..Default::default()
    };
    if signal.best_take_profit_price.is_some() {
        trade_position.best_take_profit_price = signal.best_take_profit_price;
    }
    // 如果启用了设置预止损价格,则根据开仓方向设置预止损价格
    if risk_config.is_used_signal_k_line_stop_loss {
        trade_position.signal_kline_stop_close_price = signal.signal_kline_stop_loss_price;
    }
    // 如果启用了移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近,则设置移动止损价格
    if risk_config.is_move_stop_open_price_when_touch_price {
        trade_position.move_stop_open_price_when_touch_price =
            signal.move_stop_open_price_when_touch_price;
    }

    // 如果启用了按比例止盈,（开仓价格-止损价格）*比例
    if risk_config.take_profit_ratio > 0.0 {
        if signal.signal_kline_stop_loss_price.is_none() {
            error!("signal_kline_stop_loss_price is none");
        }
        trade_position.signal_high_low_diff =
            (signal.signal_kline_stop_loss_price.unwrap() - signal.open_price).abs();
        trade_position.touch_take_ratio_profit_price = Some(
            signal.open_price - trade_position.signal_high_low_diff * risk_config.take_profit_ratio,
        );
       
    }

    state.trade_position = Some(trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;

    record_trade_entry(state, PositionSide::Short.as_str().to_owned(), signal);
}

/// 平仓
pub fn close_position(
    state: &mut TradingState,
    candle: &CandleItem,
    signal: &SignalResult,
    close_type: &str,
    profit: f64,
) {
    use super::recording::record_trade_exit;

    let exit_time = rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap();
    let mut trade_position = state.trade_position.clone().unwrap();
    let quantity = trade_position.position_nums;

    // 手续费设定0.007,假设开仓平仓各收一次 (数量*价格 *0.07%)
    let mut profit_after_fee = 0.00;
    if profit != 0.00 {
        let fee = quantity * trade_position.open_price * 0.0007;
        profit_after_fee = profit - fee;
    }
    trade_position.profit_loss = profit_after_fee;
    state.trade_position = Some(trade_position);

    // 更新总利润和资金
    state.total_profit_loss += profit_after_fee;
    state.funds += profit_after_fee;

    // 更新胜率
    if profit > 0.0 {
        state.wins += 1;
    } else if profit < 0.00 {
        state.losses += 1;
    }

    // 根据平仓原因和盈亏设置正确的平仓类型
    record_trade_exit(state, exit_time, signal, close_type, quantity);

    // 更新总利润和资金
    state.trade_position = None;
}
