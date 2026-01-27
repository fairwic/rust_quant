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
            stop_loss_source: None,
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
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::None,
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
    // 记录入场K线振幅，用于1R固定止损与保本触发
    let raw_range = (candle.h - candle.l).abs();
    let k_range = raw_range.max(signal.open_price * 0.001);
    temp_trade_position.signal_high_low_diff = k_range;
    if raw_range > 0.0 && candle.l > 0.0 {
        temp_trade_position.entry_kline_amplitude = Some(raw_range / candle.l.max(1e-9));
        temp_trade_position.entry_kline_close_pos = Some((candle.c - candle.l) / raw_range);
    }
    if risk_config
        .is_move_stop_open_price_when_touch_price
        .unwrap_or(false)
    {
        temp_trade_position.move_stop_open_price_when_touch_price =
            Some(signal.open_price + k_range);
    }
    //设置止盈止损价格
    set_long_stop_close_price(risk_config, signal, &mut temp_trade_position);

    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;

    record_trade_entry(state, PositionSide::Long.as_str().to_owned(), signal);
}

// ============================================================================
// 止盈止损设置 - 公共逻辑
// ============================================================================

/// 设置止盈止损价格的公共逻辑（Long/Short共用）
///
/// 处理：信号K线止损、ATR止损、移动止损、逆势回调止盈、三级止盈价格
fn set_stop_close_price_common(
    risk_config: &BasicRiskStrategyConfig,
    signal: &SignalResult,
    position: &mut TradePosition,
) {
    // 1. 信号K线止损 + 更新历史记录
    if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
        if let Some(new_price) = signal.signal_kline_stop_loss_price {
            let source = signal
                .stop_loss_source
                .clone()
                .unwrap_or_else(|| "Unknown".to_string());

            if let Some(old_price) = position.signal_kline_stop_close_price {
                // 这是更新操作
                let sequence = position.stop_loss_updates.len() as i32;
                let update = rust_quant_domain::value_objects::StopLossUpdate::update(
                    sequence,
                    signal.ts,
                    signal.ts, // 使用信号时间作为K线时间
                    source.clone(),
                    old_price,
                    new_price,
                );
                position.stop_loss_updates.push(update);
            } else {
                // 首次设置
                let update = rust_quant_domain::value_objects::StopLossUpdate::initial(
                    signal.ts,
                    signal.ts,
                    source.clone(),
                    new_price,
                );
                position.stop_loss_updates.push(update);
            }

            position.signal_kline_stop_close_price = Some(new_price);
            position.stop_loss_source = Some(source);
        }
    }

    // 2. ATR止损
    if let Some(p) = signal.atr_stop_loss_price {
        position.atr_stop_loss_price = Some(p);
    }

    // 3. 移动止损触发价格
    if risk_config
        .is_move_stop_open_price_when_touch_price
        .unwrap_or(false)
    {
        position.move_stop_open_price_when_touch_price =
            signal.move_stop_open_price_when_touch_price;
    }

    // 4. 逆势回调止盈
    if risk_config
        .is_counter_trend_pullback_take_profit
        .unwrap_or(false)
        && signal.counter_trend_pullback_take_profit_price.is_some()
    {
        position.counter_trend_pullback_take_profit_price =
            signal.counter_trend_pullback_take_profit_price;
    }

    // 5. 三级止盈价格
    if signal.atr_take_profit_level_1.is_some() {
        position.atr_take_profit_level_1 = signal.atr_take_profit_level_1;
        position.atr_take_profit_level_2 = signal.atr_take_profit_level_2;
        position.atr_take_profit_level_3 = signal.atr_take_profit_level_3;
        position.reached_take_profit_level = 0;
    }
}

// ============================================================================
// 止盈止损设置 - Long/Short 特定逻辑
// ============================================================================

pub fn set_long_stop_close_price(
    risk_config: BasicRiskStrategyConfig,
    signal: &SignalResult,
    temp_trade_position: &mut TradePosition,
) {
    // ============ Long特有逻辑 ============

    // 1. 止盈方向验证（做多止盈价格必须高于开仓价）
    if risk_config.validate_signal_tp.unwrap_or(false) {
        if let Some(tp_price) = signal.long_signal_take_profit_price {
            if tp_price > temp_trade_position.open_price {
                temp_trade_position.long_signal_take_profit_price = Some(tp_price);
            }
        }
    } else {
        temp_trade_position.long_signal_take_profit_price = signal.long_signal_take_profit_price;
    }

    // 2. 固定比例止盈（Long: open_price + diff * ratio）
    if let Some(fixed_take_profit_ratio) = risk_config.fixed_signal_kline_take_profit_ratio {
        if fixed_take_profit_ratio > 0.0 {
            if let Some(p) = signal.signal_kline_stop_loss_price {
                temp_trade_position.signal_high_low_diff = (p - signal.open_price).abs();
                temp_trade_position.atr_take_ratio_profit_price = Some(
                    signal.open_price
                        + temp_trade_position.signal_high_low_diff * fixed_take_profit_ratio,
                );
            } else {
                error!("signal_kline_stop_loss_price is none");
            }
        }
    }

    // ============ 公共逻辑 ============
    set_stop_close_price_common(&risk_config, signal, temp_trade_position);
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
    // 记录入场K线振幅，用于1R固定止损与保本触发
    let raw_range = (candle.h - candle.l).abs();
    let k_range = raw_range.max(signal.open_price * 0.001);
    temp_trade_position.signal_high_low_diff = k_range;
    if raw_range > 0.0 && candle.l > 0.0 {
        temp_trade_position.entry_kline_amplitude = Some(raw_range / candle.l.max(1e-9));
        temp_trade_position.entry_kline_close_pos = Some((candle.c - candle.l) / raw_range);
    }
    if risk_config
        .is_move_stop_open_price_when_touch_price
        .unwrap_or(false)
    {
        temp_trade_position.move_stop_open_price_when_touch_price =
            Some(signal.open_price - k_range);
    }
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
    // ============ Short特有逻辑 ============

    // 1. 止盈方向验证（做空止盈价格必须低于开仓价）
    if risk_config.validate_signal_tp.unwrap_or(false) {
        if let Some(tp_price) = signal.short_signal_take_profit_price {
            if tp_price < temp_trade_position.open_price {
                temp_trade_position.short_signal_take_profit_price = Some(tp_price);
            }
        }
    } else {
        temp_trade_position.short_signal_take_profit_price = signal.short_signal_take_profit_price;
    }

    // 2. ATR比例止盈（Short: open_price - diff * ratio）
    if let Some(atr_take_profit_ratio) = risk_config.atr_take_profit_ratio {
        if atr_take_profit_ratio > 0.0 {
            if let Some(atr_stop_loss_price) = signal.atr_stop_loss_price {
                let diff_price = (atr_stop_loss_price - signal.open_price).abs();
                temp_trade_position.atr_take_ratio_profit_price =
                    Some(signal.open_price - (diff_price * atr_take_profit_ratio));
            } else {
                error!("atr_stop_loss_price is none");
            }
        }
    }

    // ============ 公共逻辑 ============
    set_stop_close_price_common(&risk_config, signal, temp_trade_position);
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
