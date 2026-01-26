use rust_quant_domain::entities::position;

use super::super::types::TradeSide;
use super::position::{
    close_position, open_long_position, open_short_position, set_long_stop_close_price,
    set_short_stop_close_price,
};
use super::risk::check_risk_config;
use super::types::{BasicRiskStrategyConfig, SignalResult, TradingState};
use crate::CandleItem;

const BLOCK_LONG_ENTRY_REASON: &str = "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG";
const BLOCK_SHORT_ENTRY_REASON: &str = "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT";

fn should_block_long_entry(signal: &SignalResult) -> bool {
    signal
        .filter_reasons
        .iter()
        .any(|r| r == BLOCK_LONG_ENTRY_REASON)
}

fn should_block_short_entry(signal: &SignalResult) -> bool {
    signal
        .filter_reasons
        .iter()
        .any(|r| r == BLOCK_SHORT_ENTRY_REASON)
}

/// 处理交易信号
pub fn deal_signal(
    mut trading_state: TradingState,
    signal: &mut SignalResult,
    candle: &CandleItem,
    risk_config: BasicRiskStrategyConfig,
    candle_item_list: &[CandleItem],
    i: usize,
) -> TradingState {
    //先检查设置了是否预止损价格
    // if signal.ts == 1762747200000 {
    //     println!("signal: {:#?}", signal);
    //     println!("trading_state: {:#?}", trading_state.trade_position);
    // }
    // 1. 优先进行风控检查 (确保每根K线的最高/最低价都能触发止损/止盈)
    // 即使当前K线产生了新信号，也必须先检查由于K线波动导致的止损
    if trading_state.trade_position.is_some() {
        trading_state = check_risk_config(&risk_config, trading_state, signal, candle);
    }

    let block_long_entry = signal.should_buy && should_block_long_entry(signal);
    let block_short_entry = signal.should_sell && should_block_short_entry(signal);

    // 无持仓时遇到“禁止开仓”的信号：当作无新信号处理（保留 last_signal_result 的机会）
    // 但若已有仓位，则允许其作为“反向信号仅平仓”的触发源（在 handle_* 中实现不反手开仓）。
    let mut has_entry_signal = signal.should_buy || signal.should_sell;
    if trading_state.trade_position.is_none() && (block_long_entry || block_short_entry) {
        signal.should_buy = false;
        signal.should_sell = false;
        signal.best_open_price = None;
        has_entry_signal = false;
    }

    if has_entry_signal {
        if let Some(mut trade_position) = trading_state.trade_position.clone() {
            // 如是反向仓位，优先判断一下止盈止损
            if (trade_position.trade_side == TradeSide::Long && signal.should_sell)
                || (trade_position.trade_side == TradeSide::Short && signal.should_buy)
            {
                trading_state = check_risk_config(&risk_config, trading_state, signal, candle);
            } else {
                //如果再一次出发点了相同的信号方向，则进行止盈止损的信号更新
                if signal.should_buy {
                    // println!("出现连续的多头信号{}",rust_quant_common::utils::time::mill_time_to_datetime(signal.ts).unwrap());
                    set_long_stop_close_price(risk_config, signal, &mut trade_position);
                } else if signal.should_sell {
                    // println!("出现连续的空头信号{}",rust_quant_common::utils::time::mill_time_to_datetime(signal.ts).unwrap());
                    set_short_stop_close_price(risk_config, signal, &mut trade_position);
                }
                trading_state.trade_position = Some(trade_position);
            }
        }

        // 使用更优点位开仓
        if signal.best_open_price.is_some() {
            trading_state.last_signal_result = Some(signal.clone());
        } else {
            trading_state.last_signal_result = None;
        }

        // 处理策略信号
        if signal.should_buy {
            handle_buy_signal_logic(
                risk_config,
                &mut trading_state,
                signal,
                candle,
                block_long_entry,
            );
        } else if signal.should_sell {
            handle_sell_signal_logic(
                risk_config,
                &mut trading_state,
                signal,
                candle,
                block_short_entry,
            );
        }
    } else {
        // 如果没有新信号
        if trading_state.trade_position.is_some() {
            // 风控已在顶部检查过，此处无需再次检查
        } else if trading_state.last_signal_result.is_some() {
            // 要确保大于信号的开仓时间
            if candle.ts >= trading_state.last_signal_result.clone().unwrap().ts {
                let last_signal_result = trading_state.last_signal_result.clone().unwrap();
                if last_signal_result.should_buy {
                    // 如果信号是买，但是当前价格低于信号的最优开仓价格，则使用信号的最优开仓价格
                    if let Some(best_price) = last_signal_result.best_open_price {
                        if candle.l <= best_price {
                            signal.open_price = best_price;
                            signal.should_buy = true;
                            signal.signal_kline_stop_loss_price =
                                last_signal_result.signal_kline_stop_loss_price;
                            signal.single_value = last_signal_result.single_value;
                            signal.single_result = last_signal_result.single_result;

                            trading_state.last_signal_result = None;
                            let signal_open_position_time = Some(
                                rust_quant_common::utils::time::mill_time_to_datetime(
                                    last_signal_result.ts,
                                )
                                .unwrap(),
                            );
                            open_long_position(
                                risk_config,
                                &mut trading_state,
                                candle,
                                signal,
                                signal_open_position_time,
                            );
                        }
                    }
                } else if last_signal_result.should_sell {
                    // 如果信号是卖，但是当前价格高于信号的最优开仓价格，则使用信号的最优开仓价格
                    if let Some(best_price) = last_signal_result.best_open_price {
                        if candle.h > best_price {
                            signal.open_price = best_price;
                            signal.should_sell = true;
                            signal.signal_kline_stop_loss_price =
                                last_signal_result.signal_kline_stop_loss_price;
                            signal.single_value = last_signal_result.single_value;
                            signal.single_result = last_signal_result.single_result;

                            trading_state.last_signal_result = None;
                            let signal_open_position_time = Some(
                                rust_quant_common::utils::time::mill_time_to_datetime(
                                    last_signal_result.ts,
                                )
                                .unwrap(),
                            );
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
    }
    trading_state
}

/// 处理买入信号的逻
fn handle_buy_signal_logic(
    risk_config: BasicRiskStrategyConfig,
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
    block_open: bool,
) {
    if trading_state.trade_position.is_none() {
        if block_open {
            return;
        }
        // 不使用最优开仓价格，直接开多仓
        open_long_position(risk_config, trading_state, candle, signal, None);
    } else if let Some(trade_position) = trading_state.trade_position.clone() {
        if trade_position.trade_side == TradeSide::Short {
            // 持有空单，先平空单
            let profit =
                (trade_position.open_price - signal.open_price) * trade_position.position_nums;
            let mut pos = trade_position;
            pos.close_price = Some(signal.open_price);
            trading_state.trade_position = Some(pos);
            close_position(
                trading_state,
                candle,
                signal,
                if block_open {
                    "反向信号触发平仓(趋势过滤)"
                } else {
                    "反向信号触发平仓"
                },
                profit,
            );

            // 然后开多仓（若被趋势过滤则只平仓不反手）
            if !block_open {
                open_long_position(risk_config, trading_state, candle, signal, None);
            }
        }
    }
}

/// 处理卖出信号的逻辑
fn handle_sell_signal_logic(
    risk_config: BasicRiskStrategyConfig,
    trading_state: &mut TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
    block_open: bool,
) {
    if trading_state.trade_position.is_none() {
        if block_open {
            return;
        }
        // 不使用最优开仓价格，直接开空仓
        open_short_position(risk_config, trading_state, candle, signal, None);
    } else if let Some(trade_position) = trading_state.trade_position.clone() {
        if trade_position.trade_side == TradeSide::Long {
            // 持有多单，先平多单
            let profit =
                (signal.open_price - trade_position.open_price) * trade_position.position_nums;
            let mut pos = trade_position;
            pos.close_price = Some(signal.open_price);
            trading_state.trade_position = Some(pos);
            close_position(
                trading_state,
                candle,
                signal,
                if block_open {
                    "反向信号平仓(趋势过滤)"
                } else {
                    "反向信号平仓"
                },
                profit,
            );

            // 然后开空仓（若被趋势过滤则只平仓不反手）
            if !block_open {
                open_short_position(risk_config, trading_state, candle, signal, None);
            }
        }
    }
}
