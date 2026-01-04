use super::super::types::TradeSide;
use super::recording::record_trade_entry;
use super::types::{BasicRiskStrategyConfig, SignalResult, TradePosition, TradingState};
use crate::CandleItem;
use rust_quant_domain::enums::PositionSide;
use tracing::error;

/// 最终平仓处理
pub fn finalize_trading_state(trading_state: &mut TradingState, candle_item_list: &[CandleItem]) {
    let mut trade_position = match trading_state.trade_position.clone() {
        Some(p) => p,
        None => return,
    };

    let last_candle = match candle_item_list.last() {
        Some(c) => c,
        None => return,
    };

    let last_price = last_candle.c;
    trade_position.close_price = Some(last_price);

    let profit = match trade_position.trade_side {
        TradeSide::Long => (last_price - trade_position.open_price) * trade_position.position_nums,
        TradeSide::Short => (trade_position.open_price - last_price) * trade_position.position_nums,
    };

    close_position(
        trading_state,
        last_candle,
        &SignalResult {
            should_buy: false,
            should_sell: true,
            open_price: last_price,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            signal_kline_stop_loss_price: None,
            ts: last_candle.ts,
            single_value: Some("结束平仓".to_string()),
            single_result: Some("结束平仓".to_string()),
            move_stop_open_price_when_touch_price: None,
            counter_trend_pullback_take_profit_price: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
        },
        "结束平仓",
        profit,
    );
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
        open_position_time: match rust_quant_common::utils::time::mill_time_to_datetime(candle.ts) {
            Ok(s) => s,
            Err(_) => String::new(),
        },
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Long,
        ..Default::default()
    };
    //设置止盈止损价格
    set_long_stop_close_price(risk_config, signal, &mut temp_trade_position);

    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;

    record_trade_entry(state, PositionSide::Long.as_str().to_owned(), signal);
}

pub fn set_long_stop_close_price(
    risk_config: BasicRiskStrategyConfig,
    signal: &SignalResult,
    temp_trade_position: &mut TradePosition,
) {
    // 如果信号k线路止损
    if let Some(is_used_signal_k_line_stop_loss) = risk_config.is_used_signal_k_line_stop_loss {
        if is_used_signal_k_line_stop_loss {
            temp_trade_position.signal_kline_stop_close_price = signal.signal_kline_stop_loss_price;
        }
    }
    // 如果atr止盈，则使用atr盈亏比止盈
    // 如果启用了atr止盈
    // if let Some(atr_take_profit_ratio) = risk_config.atr_take_profit_ratio {
    //     if atr_take_profit_ratio > 0.0 {
    //         if signal.atr_stop_loss_price.is_none() {
    //             error!("atr_stop_loss_price is none");
    //         }
    //         let atr_stop_loss_price = signal.atr_stop_loss_price.unwrap();
    //         let diff_price = (atr_stop_loss_price - signal.open_price).abs();

    //         temp_trade_position.atr_take_ratio_profit_price =
    //             Some(signal.open_price + (diff_price * atr_take_profit_ratio));
    //     }
    // }
    //atr止损
    if let Some(p) = signal.atr_stop_loss_price {
        temp_trade_position.atr_stop_loss_price = Some(p);
    }
    // 如果启用了移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近,则设置移动止损价格
    if let Some(is_move_stop_open_price_when_touch_price) =
        risk_config.is_move_stop_open_price_when_touch_price
    {
        if is_move_stop_open_price_when_touch_price {
            temp_trade_position.move_stop_open_price_when_touch_price =
                signal.move_stop_open_price_when_touch_price;
        }
    }

    // 如果启用了固定比例止盈,则设置固定比例止盈价格
    if let Some(fixed_take_profit_ratio) = risk_config.fixed_signal_kline_take_profit_ratio {
        if fixed_take_profit_ratio > 0.0 {
            if signal.signal_kline_stop_loss_price.is_none() {
                error!("signal_kline_stop_loss_price is none");
            }
            if let Some(p) = signal.signal_kline_stop_loss_price {
                temp_trade_position.signal_high_low_diff = (p - signal.open_price).abs();
            } else {
                error!("signal_kline_stop_loss_price is none");
            }

            temp_trade_position.atr_take_ratio_profit_price = Some(
                signal.open_price
                    + temp_trade_position.signal_high_low_diff * fixed_take_profit_ratio,
            );
        }
    }
    // 如果启用了逆势回调止盈，且均线是空头排列时做多
    if let Some(is_counter_trend) = risk_config.is_counter_trend_pullback_take_profit {
        if is_counter_trend && signal.counter_trend_pullback_take_profit_price.is_some() {
            // 检查是否是空头排列（逆势做多）
            // 设置逆势回调止盈价格
            temp_trade_position.counter_trend_pullback_take_profit_price =
                signal.counter_trend_pullback_take_profit_price;
        }
    }

    // 设置三级止盈价格
    if signal.atr_take_profit_level_1.is_some() {
        temp_trade_position.atr_take_profit_level_1 = signal.atr_take_profit_level_1;
        temp_trade_position.atr_take_profit_level_2 = signal.atr_take_profit_level_2;
        temp_trade_position.atr_take_profit_level_3 = signal.atr_take_profit_level_3;
        temp_trade_position.reached_take_profit_level = 0;
    }
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
    let mut temp_trade_position = TradePosition {
        position_nums: state.funds / signal.open_price,
        open_price: signal.open_price,
        open_position_time: match rust_quant_common::utils::time::mill_time_to_datetime(candle.ts) {
            Ok(s) => s,
            Err(_) => String::new(),
        },
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Short,
        ..Default::default()
    };
    //设置止盈止损价格
    set_short_stop_close_price(risk_config, signal, &mut temp_trade_position);

    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;

    record_trade_entry(state, PositionSide::Short.as_str().to_owned(), signal);
}

pub fn set_short_stop_close_price(
    risk_config: BasicRiskStrategyConfig,
    signal: &SignalResult,
    temp_trade_position: &mut TradePosition,
) {
    //atr比例止盈
    // 如果启用了atr止盈
    if let Some(atr_take_profit_ratio) = risk_config.atr_take_profit_ratio {
        if atr_take_profit_ratio > 0.0 {
            if signal.atr_stop_loss_price.is_none() {
                error!("atr_stop_loss_price is none");
            }
            if let Some(atr_stop_loss_price) = signal.atr_stop_loss_price {
                let diff_price = (atr_stop_loss_price - signal.open_price).abs();

                temp_trade_position.atr_take_ratio_profit_price =
                    Some(signal.open_price - (diff_price * atr_take_profit_ratio));
            } else {
                error!("atr_stop_loss_price is none");
            }
        }
    }

    //atr止损
    if let Some(p) = signal.atr_stop_loss_price {
        temp_trade_position.atr_stop_loss_price = Some(p);
    }

    // 如果启用了设置预止损价格,则根据开仓方向设置预止损价格
    if let Some(is_used_signal_k_line_stop_loss) = risk_config.is_used_signal_k_line_stop_loss {
        if is_used_signal_k_line_stop_loss {
            temp_trade_position.signal_kline_stop_close_price = signal.signal_kline_stop_loss_price;
        }
    }
    // 如果启用了移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近,则设置移动止损价格
    if let Some(is_move_stop_open_price_when_touch_price) =
        risk_config.is_move_stop_open_price_when_touch_price
    {
        if is_move_stop_open_price_when_touch_price {
            temp_trade_position.move_stop_open_price_when_touch_price =
                signal.move_stop_open_price_when_touch_price;
        }
    }

    // 如果启用了按比例止盈,（开仓价格-止损价格）*比例
    // if let Some(fixe_take_profit_ratio) = risk_config.fixed_signal_kline_take_profit_ratio {
    //     if fixe_take_profit_ratio > 0.0 {
    //         if signal.signal_kline_stop_loss_price.is_none() {
    //             temp_trade_position.signal_high_low_diff =
    //                 (signal.signal_kline_stop_loss_price.unwrap() - signal.open_price).abs();
    //             temp_trade_position.atr_take_ratio_profit_price = Some(
    //                 signal.open_price
    //                     - temp_trade_position.signal_high_low_diff * fixe_take_profit_ratio,
    //             );
    //         }
    //         error!("signal_kline_stop_loss_price is none");
    //     }
    // }

    // 如果启用了逆势回调止盈，且均线是多头排列时做空
    if let Some(is_counter_trend) = risk_config.is_counter_trend_pullback_take_profit {
        if is_counter_trend && signal.counter_trend_pullback_take_profit_price.is_some() {
            // 检查是否是多头排列（逆势做空）
            // 设置逆势回调止盈价格
            temp_trade_position.counter_trend_pullback_take_profit_price =
                signal.counter_trend_pullback_take_profit_price;
        }
    }

    // 设置三级止盈价格
    if signal.atr_take_profit_level_1.is_some() {
        temp_trade_position.atr_take_profit_level_1 = signal.atr_take_profit_level_1;
        temp_trade_position.atr_take_profit_level_2 = signal.atr_take_profit_level_2;
        temp_trade_position.atr_take_profit_level_3 = signal.atr_take_profit_level_3;
        temp_trade_position.reached_take_profit_level = 0;
    }
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

    let exit_time = match rust_quant_common::utils::time::mill_time_to_datetime(candle.ts) {
        Ok(s) => s,
        Err(_) => String::new(),
    };

    let mut trade_position = match state.trade_position.clone() {
        Some(p) => p,
        None => return,
    };
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
