use super::super::types::TradeSide;
use super::position::close_position;
use super::types::{BasicRiskStrategyConfig, SignalResult, TradingState};
use crate::CandleItem;

/// 风险管理，检查止盈止损配置
pub fn check_risk_config(
    risk_config: &BasicRiskStrategyConfig,
    mut trading_state: TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
    candle_item_list: &[CandleItem],
    _i: usize,
) -> TradingState {

    // if signal.ts == 1762819200000 {
    //     println!("signal: {:#?}", signal);
    //     println!("trading_state: {:#?}", trading_state.trade_position);
    // }
    // if signal.ts == 1763395200000 || signal.ts == 1763481600000 {
    //     println!("signal: {:#?}", signal);
    //     println!("trading_state: {:#?}", trading_state.trade_position);
    // }
    let current_open_price = signal.open_price;
    let current_low_price = candle.l;
    let current_high_price = candle.h;
    let current_close_price = candle.c;

    let mut trade_position = trading_state.trade_position.clone().unwrap();
    let entry_price = trade_position.open_price;
    let position_nums = trade_position.position_nums.clone();

    // 检查移动止盈
    if let Some(is_move_stop_open_price_when_touch_price) =
        risk_config.is_move_stop_open_price_when_touch_price
    {
        if let Some(move_stop_loss_price) = trade_position.move_stop_open_price {
            match trade_position.trade_side {
                TradeSide::Long => {
                    if current_low_price <= move_stop_loss_price {
                        trade_position.close_price = Some(move_stop_loss_price);
                        trading_state.trade_position = Some(trade_position.clone());
                        close_position(
                            &mut trading_state,
                            candle,
                            &signal,
                            "移动(开仓价格止损)",
                            0.00,
                        );
                        return trading_state;
                    }
                }
                TradeSide::Short => {
                    if current_high_price >= move_stop_loss_price {
                        trade_position.close_price = Some(move_stop_loss_price);
                        trading_state.trade_position = Some(trade_position.clone());
                        close_position(
                            &mut trading_state,
                            candle,
                            &signal,
                            "移动(开仓价格止损)",
                            0.00,
                        );
                        return trading_state;
                    }
                }
            }
        } else {
            // 如果启用了移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
            if trade_position
                .move_stop_open_price_when_touch_price
                .is_some()
            {
                match trade_position.trade_side {
                    TradeSide::Long => {
                        if current_high_price
                            >= trade_position
                                .move_stop_open_price_when_touch_price
                                .unwrap()
                        {
                            trade_position.move_stop_open_price = Some(entry_price);
                            trading_state.trade_position = Some(trade_position.clone());
                        }
                    }
                    TradeSide::Short => {
                        if current_low_price
                            <= trade_position
                                .move_stop_open_price_when_touch_price
                                .unwrap()
                        {
                            trade_position.move_stop_open_price = Some(entry_price);
                            trading_state.trade_position = Some(trade_position.clone());
                        }
                    }
                }
            }
        }
    }

    // 检查按atr收益比例止盈
    if let Some(atr_take_profit_ratio) = risk_config.atr_take_profit_ratio {
        if atr_take_profit_ratio > 0.0 {
            match trade_position.trade_side {
                TradeSide::Long => {
                    if let Some(touch_price) = trade_position.atr_take_ratio_profit_price {
                        if current_high_price >= touch_price {
                            let profit = (touch_price - entry_price) * trade_position.position_nums;
                            trade_position.close_price = Some(touch_price);
                            //
                            trading_state.trade_position = Some(trade_position);
                            close_position(
                                &mut trading_state,
                                candle,
                                &signal,
                                "atr按收益比例止盈",
                                profit,
                            );
                            return trading_state;
                        }
                    }
                }
                TradeSide::Short => {
                    if let Some(touch_price) = trade_position.atr_take_ratio_profit_price {
                        if current_low_price <= touch_price {
                            let profit = (entry_price - touch_price) * trade_position.position_nums;
                            trade_position.close_price = Some(touch_price);
                            //
                            trading_state.trade_position = Some(trade_position);
                            close_position(
                                &mut trading_state,
                                candle,
                                &signal,
                                "atr按收益比例止盈",
                                profit,
                            );
                            return trading_state;
                        }
                    }
                }
            }
        }
    }

    // 计算盈亏率
    let profit_pct = match trade_position.trade_side {
        TradeSide::Long => (current_low_price - entry_price) / entry_price,
        TradeSide::Short => (entry_price - current_high_price) / entry_price,
    };

    // 计算盈亏
    let profit = match trade_position.trade_side {
        TradeSide::Long => (current_close_price - entry_price) * trade_position.position_nums,
        TradeSide::Short => (entry_price - current_close_price) * trade_position.position_nums,
    };

    // 检查是否设置了固定信号线比例止盈价格
    if let Some(fixed_take_profit_price) = trade_position.fixed_take_profit_price {
        match trade_position.trade_side {
            TradeSide::Long => {
                if current_high_price > fixed_take_profit_price {
                    let profit =
                        (fixed_take_profit_price - entry_price) * trade_position.position_nums;
                    trade_position.close_price = Some(fixed_take_profit_price);
                    trading_state.trade_position = Some(trade_position);
                    close_position(&mut trading_state, candle, &signal, "固定信号线比例止盈", profit);
                    return trading_state;
                }
            }
            TradeSide::Short => {
                if current_low_price < fixed_take_profit_price {
                    let profit =
                        (entry_price - fixed_take_profit_price) * trade_position.position_nums;
                    trade_position.close_price = Some(fixed_take_profit_price);
                    trading_state.trade_position = Some(trade_position);
                    close_position(&mut trading_state, candle, &signal, "固定信号线比例止盈", profit);
                    return trading_state;
                }
            }
        }
    }

    //是否设置了做多止盈价格
    if let Some(long_signal_take_profit_price) = signal.long_signal_take_profit_price {
        if current_high_price > long_signal_take_profit_price
            && trade_position.trade_side == TradeSide::Long
        {
            trade_position.close_price = Some(long_signal_take_profit_price);
            trading_state.trade_position = Some(trade_position);
            close_position(
                &mut trading_state,
                candle,
                &signal,
                "做多触达指标动态止盈",
                profit,
            );
            return trading_state;
        }
    }

    //是否设置做空止盈价格
    if let Some(short_signal_take_profit_price) = signal.short_signal_take_profit_price {
        if current_low_price < short_signal_take_profit_price
            && trade_position.trade_side == TradeSide::Short
        {
            trade_position.close_price = Some(short_signal_take_profit_price);
            trading_state.trade_position = Some(trade_position);
            close_position(
                &mut trading_state,
                candle,
                &signal,
                "做空触达指标动态止盈",
                profit,
            );
            return trading_state;
        }
    }
    
    
    // 检查逆势回调止盈
    if let Some(counter_trend_take_profit_price) = trade_position.counter_trend_pullback_take_profit_price {
        match trade_position.trade_side {
            TradeSide::Long => {
                // 做多时，价格达到回调止盈价格（高于止盈价格）
                if current_high_price >= counter_trend_take_profit_price {
                    let profit = (counter_trend_take_profit_price - entry_price) * position_nums;
                    trade_position.close_price = Some(counter_trend_take_profit_price);
                    trading_state.trade_position = Some(trade_position);
                    close_position(
                        &mut trading_state,
                        candle,
                        &signal,
                        "逆势回调止盈",
                        profit,
                    );
                    return trading_state;
                }
            }
            TradeSide::Short => {
                // 做空时，价格达到回调止盈价格（低于止盈价格）
                if current_low_price <= counter_trend_take_profit_price {
                    let profit = (entry_price - counter_trend_take_profit_price) * position_nums;
                    trade_position.close_price = Some(counter_trend_take_profit_price);
                    trading_state.trade_position = Some(trade_position);
                    close_position(
                        &mut trading_state,
                        candle,
                        &signal,
                        "逆势回调止盈",
                        profit,
                    );
                    return trading_state;
                }
            }
        }
    }
    // 先检查设置了信号k线路的价格
    if let Some(signal_kline_stop_close_price) = trade_position.signal_kline_stop_close_price {
        match trade_position.trade_side.clone() {
            TradeSide::Long => {
                if current_close_price <= signal_kline_stop_close_price {
                    trade_position.close_price = Some(signal_kline_stop_close_price);
                    let profit = (signal_kline_stop_close_price - entry_price) * position_nums;
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
                    trade_position.close_price = Some(signal_kline_stop_close_price);
                    let profit = (entry_price - signal_kline_stop_close_price) * position_nums;
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
        }
    }
    // 最后再检查最大止损
    if profit_pct < -risk_config.max_loss_percent {
        trade_position.close_price = Some(current_open_price);
        trading_state.trade_position = Some(trade_position);
        close_position(&mut trading_state, candle, &signal, "最大亏损止损", profit);
        return trading_state;
    }
    trading_state
}
