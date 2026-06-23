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
const LOW_VOLUME_INSIDE_RANGE_ENTRY_REASON: &str = "LOW_VOLUME_INSIDE_RANGE_BLOCK_ENTRY";
const OPPOSITE_VALUE_AREA_ENTRY_REASON: &str = "VOLUME_PROFILE_OPPOSITE_VALUE_AREA_BLOCK_ENTRY";
const LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON: &str =
    "VOLUME_PROFILE_LOW_VOLUME_ABOVE_VALUE_AREA_BLOCK_ENTRY";
const SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON: &str =
    "VOLUME_PROFILE_SHORT_INSIDE_LOW_VOLUME_NODE_BLOCK_ENTRY";
const REBOUND_HAMMER_LONG_PROTECT_REASON: &str = "REBOUND_HAMMER_LONG_PROTECT";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReboundShortProtectMode {
    Off,
    TakeProfit,
    Breakeven,
}
/// 提供reboundshortprotectmode的集中实现，避免回测策略调用方重复处理相同细节。
fn rebound_short_protect_mode() -> ReboundShortProtectMode {
    match std::env::var("VEGAS_REBOUND_SHORT_PROTECT_MODE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "tp" | "take_profit" => ReboundShortProtectMode::TakeProfit,
        "breakeven" | "move_to_entry" => ReboundShortProtectMode::Breakeven,
        _ => ReboundShortProtectMode::Off,
    }
}
/// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
fn should_block_long_entry(signal: &SignalResult) -> bool {
    signal.filter_reasons.iter().any(|r| {
        r == BLOCK_LONG_ENTRY_REASON
            || r == LOW_VOLUME_INSIDE_RANGE_ENTRY_REASON
            || r == OPPOSITE_VALUE_AREA_ENTRY_REASON
            || r == LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON
    })
}
/// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
fn should_block_short_entry(signal: &SignalResult) -> bool {
    signal.filter_reasons.iter().any(|r| {
        r == BLOCK_SHORT_ENTRY_REASON
            || r == LOW_VOLUME_INSIDE_RANGE_ENTRY_REASON
            || r == OPPOSITE_VALUE_AREA_ENTRY_REASON
            || r == LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON
            || r == SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON
    })
}
/// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
fn has_entry_only_block_reason(signal: &SignalResult) -> bool {
    signal.filter_reasons.iter().any(|r| {
        r == LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON
            || r == SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON
    })
}
/// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
fn has_rebound_hammer_long_protect(signal: &SignalResult) -> bool {
    signal
        .filter_reasons
        .iter()
        .any(|r| r == REBOUND_HAMMER_LONG_PROTECT_REASON)
}
/// 处理交易信号
pub fn deal_signal(
    mut trading_state: TradingState,
    signal: &mut SignalResult,
    candle: &CandleItem,
    risk_config: BasicRiskStrategyConfig,
    _candle_item_list: &[CandleItem],
    _i: usize,
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
    if let Some(mut trade_position) = trading_state.trade_position.clone() {
        if trade_position.trade_side == TradeSide::Short && has_rebound_hammer_long_protect(signal)
        {
            match rebound_short_protect_mode() {
                ReboundShortProtectMode::TakeProfit => {
                    let profit = (trade_position.open_price - signal.open_price)
                        * trade_position.position_nums;
                    trade_position.close_price = Some(signal.open_price);
                    trading_state.trade_position = Some(trade_position);
                    close_position(
                        &mut trading_state,
                        candle,
                        signal,
                        "反弹锤子线保护止盈",
                        profit,
                    );
                    return trading_state;
                }
                ReboundShortProtectMode::Breakeven => {
                    let new_stop = trade_position.open_price;
                    let source = "ReboundHammer_Long_Breakeven".to_string();
                    if let Some(old_price) = trade_position.signal_kline_stop_close_price {
                        let sequence = trade_position.stop_loss_updates.len() as i32;
                        trade_position.stop_loss_updates.push(
                            rust_quant_domain::value_objects::StopLossUpdate::update(
                                sequence,
                                signal.ts,
                                signal.ts,
                                source.clone(),
                                old_price,
                                new_stop,
                            ),
                        );
                    } else {
                        trade_position.stop_loss_updates.push(
                            rust_quant_domain::value_objects::StopLossUpdate::initial(
                                signal.ts,
                                signal.ts,
                                source.clone(),
                                new_stop,
                            ),
                        );
                    }
                    trade_position.signal_kline_stop_close_price = Some(new_stop);
                    trade_position.stop_loss_source = Some(source);
                    trading_state.trade_position = Some(trade_position);
                }
                ReboundShortProtectMode::Off => {}
            }
        }
    }
    let block_long_entry = signal.should_buy && should_block_long_entry(signal);
    let block_short_entry = signal.should_sell && should_block_short_entry(signal);
    let ignore_entry_only_blocked_signal =
        (block_long_entry || block_short_entry) && has_entry_only_block_reason(signal);
    // 纯入场过滤不应触发平仓；其他过滤在有仓位时仍可作为“反向信号仅平仓”处理。
    // 无持仓时遇到任何禁止开仓信号，都当作无新信号处理。
    let mut has_entry_signal = signal.should_buy || signal.should_sell;
    if ignore_entry_only_blocked_signal
        || (trading_state.trade_position.is_none() && (block_long_entry || block_short_entry))
    {
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
#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::SignalDirection;
    /// 构造测试或回测用 K 线，减少样本初始化重复代码。
    fn candle(close: f64, ts: i64) -> CandleItem {
        CandleItem {
            o: close,
            h: close * 1.005,
            l: close * 0.995,
            c: close,
            v: 1.0,
            ts,
            confirm: 1,
        }
    }
    /// 封装低成交量内部区间买入信号，减少回测策略调用方重复实现相同细节。
    fn low_volume_inside_range_buy_signal(price: f64, ts: i64) -> SignalResult {
        SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: price,
            ts,
            filter_reasons: vec![LOW_VOLUME_INSIDE_RANGE_ENTRY_REASON.to_string()],
            direction: SignalDirection::Long,
            ..SignalResult::default()
        }
    }
    /// 封装反向价值区域卖出信号，减少回测策略调用方重复实现相同细节。
    fn opposite_value_area_sell_signal(price: f64, ts: i64) -> SignalResult {
        SignalResult {
            should_buy: false,
            should_sell: true,
            open_price: price,
            ts,
            filter_reasons: vec!["VOLUME_PROFILE_OPPOSITE_VALUE_AREA_BLOCK_ENTRY".to_string()],
            direction: SignalDirection::Short,
            ..SignalResult::default()
        }
    }
    /// 封装阻塞卖出信号，减少回测策略调用方重复实现相同细节。
    fn blocked_sell_signal(price: f64, ts: i64, reason: &str) -> SignalResult {
        SignalResult {
            should_buy: false,
            should_sell: true,
            open_price: price,
            ts,
            filter_reasons: vec![reason.to_string()],
            direction: SignalDirection::Short,
            ..SignalResult::default()
        }
    }
    /// 封装阻塞买入信号，减少回测策略调用方重复实现相同细节。
    fn blocked_buy_signal(price: f64, ts: i64, reason: &str) -> SignalResult {
        SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: price,
            ts,
            filter_reasons: vec![reason.to_string()],
            direction: SignalDirection::Long,
            ..SignalResult::default()
        }
    }
    #[test]
    fn low_volume_inside_range_blocks_new_long_entry() {
        let mut signal = low_volume_inside_range_buy_signal(100.0, 1);
        let state = deal_signal(
            TradingState::default(),
            &mut signal,
            &candle(100.0, 1),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert!(state.trade_position.is_none());
        assert_eq!(state.open_position_times, 0);
    }
    #[test]
    fn low_volume_inside_range_closes_short_without_reversing_long() {
        let mut signal = low_volume_inside_range_buy_signal(98.0, 2);
        let state = TradingState {
            trade_position: Some(super::super::types::TradePosition {
                trade_side: TradeSide::Short,
                open_price: 100.0,
                position_nums: 1.0,
                open_position_time: "2026-05-21 04:00:00".to_string(),
                signal_high_low_diff: 1.0,
                ..Default::default()
            }),
            ..TradingState::default()
        };
        let state = deal_signal(
            state,
            &mut signal,
            &candle(98.0, 2),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert!(state.trade_position.is_none());
        assert_eq!(state.open_position_times, 0);
        assert_eq!(state.trade_records.len(), 1);
    }
    #[test]
    fn opposite_value_area_blocks_new_short_entry() {
        let mut signal = opposite_value_area_sell_signal(100.0, 1);
        let state = deal_signal(
            TradingState::default(),
            &mut signal,
            &candle(100.0, 1),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert!(state.trade_position.is_none());
        assert_eq!(state.open_position_times, 0);
    }
    #[test]
    fn low_volume_above_value_area_blocks_both_entry_sides() {
        let mut long_signal =
            blocked_buy_signal(100.0, 1, LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON);
        let mut short_signal =
            blocked_sell_signal(100.0, 2, LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON);
        let long_state = deal_signal(
            TradingState::default(),
            &mut long_signal,
            &candle(100.0, 1),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        let short_state = deal_signal(
            TradingState::default(),
            &mut short_signal,
            &candle(100.0, 2),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert!(long_state.trade_position.is_none());
        assert!(short_state.trade_position.is_none());
        assert_eq!(long_state.open_position_times, 0);
        assert_eq!(short_state.open_position_times, 0);
    }
    #[test]
    fn short_inside_low_volume_node_blocks_new_short_entry() {
        let mut signal = blocked_sell_signal(100.0, 1, SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON);
        let state = deal_signal(
            TradingState::default(),
            &mut signal,
            &candle(100.0, 1),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert!(state.trade_position.is_none());
        assert_eq!(state.open_position_times, 0);
    }
    #[test]
    fn low_volume_above_value_area_does_not_close_existing_short() {
        let mut signal = blocked_buy_signal(98.0, 2, LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON);
        let state = TradingState {
            trade_position: Some(super::super::types::TradePosition {
                trade_side: TradeSide::Short,
                open_price: 100.0,
                position_nums: 1.0,
                open_position_time: "2026-05-21 04:00:00".to_string(),
                signal_high_low_diff: 1.0,
                ..Default::default()
            }),
            ..TradingState::default()
        };
        let state = deal_signal(
            state,
            &mut signal,
            &candle(98.0, 2),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert_eq!(
            state.trade_position.as_ref().map(|p| p.trade_side),
            Some(TradeSide::Short)
        );
        assert_eq!(state.trade_records.len(), 0);
    }
    #[test]
    fn short_inside_low_volume_node_does_not_close_existing_long() {
        let mut signal = blocked_sell_signal(102.0, 2, SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON);
        let state = TradingState {
            trade_position: Some(super::super::types::TradePosition {
                trade_side: TradeSide::Long,
                open_price: 100.0,
                position_nums: 1.0,
                open_position_time: "2026-05-21 04:00:00".to_string(),
                signal_high_low_diff: 1.0,
                ..Default::default()
            }),
            ..TradingState::default()
        };
        let state = deal_signal(
            state,
            &mut signal,
            &candle(102.0, 2),
            BasicRiskStrategyConfig::default(),
            &[],
            0,
        );
        assert_eq!(
            state.trade_position.as_ref().map(|p| p.trade_side),
            Some(TradeSide::Long)
        );
        assert_eq!(state.trade_records.len(), 0);
    }
}
