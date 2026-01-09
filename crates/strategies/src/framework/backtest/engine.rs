use super::indicators::get_multi_indicator_values;
use super::position::finalize_trading_state;
use super::signal::deal_signal;
use super::types::{BackTestResult, BasicRiskStrategyConfig, SignalResult, TradingState};
use super::utils::calculate_win_rate;
use crate::framework::types::TradeSide;
use crate::CandleItem;
use rust_quant_indicators::trend::vegas::{IndicatorCombine, VegasIndicatorSignalValue};

/// 回测引擎：支持Vegas策略（内部转调通用引擎保持向后兼容）
/// 回测引擎：支持Vegas策略（内部转调通用引擎保持向后兼容）
pub fn run_back_test(
    inst_id: &str,
    strategy: impl FnMut(&[CandleItem], &mut VegasIndicatorSignalValue) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IndicatorCombine,
) -> BackTestResult {
    run_back_test_generic(
        inst_id,
        strategy,
        candles_list,
        basic_risk_config,
        min_data_length,
        indicator_combine,
        |ic, candle| get_multi_indicator_values(ic, candle),
    )
}

/// 通用回测引擎：支持自定义指标组合与指标值结构
pub fn run_back_test_generic<IC, IV>(
    inst_id: &str,
    mut strategy: impl FnMut(&[CandleItem], &mut IV) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IC,
    mut build_values: impl FnMut(&mut IC, &CandleItem) -> IV,
) -> BackTestResult {
    let mut trading_state = TradingState::default();

    // Optimization: Use a Vec with amortized O(1) shifting instead of VecDeque::make_contiguous
    let window_size = min_data_length;
    // Ensure reasonable capacity. If window_size is huge, we double it.
    // If small, we use at least 1024 to avoid frequent shifts.
    let capacity = if window_size > 0 {
        (window_size * 2).max(1024)
    } else {
        1024
    };
    let mut candle_buffer: Vec<CandleItem> = Vec::with_capacity(capacity);
    let mut filtered_signals: Vec<super::types::FilteredSignal> = Vec::new();
    let mut shadow_trades: Vec<super::types::ShadowTrade> = Vec::new();

    for (i, candle) in candles_list.iter().enumerate() {
        // 计算自定义指标
        let mut multi_indicator_values = build_values(indicator_combine, &candle);

        candle_buffer.push(candle.clone());

        // --- Shadow Trading Logic: Update Active Trades ---
        let mut completed_indices = Vec::new();
        for (idx, trade) in shadow_trades.iter_mut().enumerate() {
            let current_high = candle.h;
            let current_low = candle.l;
            let current_close = candle.c;

            // 更新浮盈浮亏
            match trade.direction {
                TradeSide::Long => {
                    let max_profit = (current_high - trade.entry_price) / trade.entry_price;
                    let max_loss = (current_low - trade.entry_price) / trade.entry_price;
                    trade.max_unrealized_profit = trade.max_unrealized_profit.max(max_profit);
                    trade.max_unrealized_loss = trade.max_unrealized_loss.min(max_loss);

                    // 检查止损
                    if let Some(sl) = trade.sl_price {
                        if current_low <= sl {
                            // 止损触发
                            let pnl = (sl - trade.entry_price) / trade.entry_price;
                            if let Some(signal) = filtered_signals.get_mut(trade.signal_index) {
                                signal.final_pnl = pnl;
                                signal.theoretical_loss = trade.max_unrealized_loss;
                                signal.theoretical_profit = trade.max_unrealized_profit;
                                signal.trade_result = "LOSS".to_string();
                            }
                            completed_indices.push(idx);
                            continue;
                        }
                    }

                    // 检查止盈
                    if let Some(tp) = trade.tp_price {
                        if current_high >= tp {
                            // 止盈触发
                            let pnl = (tp - trade.entry_price) / trade.entry_price;
                            if let Some(signal) = filtered_signals.get_mut(trade.signal_index) {
                                signal.final_pnl = pnl;
                                signal.theoretical_loss = trade.max_unrealized_loss;
                                signal.theoretical_profit = trade.max_unrealized_profit;
                                signal.trade_result = "WIN".to_string();
                            }
                            completed_indices.push(idx);
                            continue;
                        }
                    }
                }
                TradeSide::Short => {
                    let max_profit = (trade.entry_price - current_low) / trade.entry_price;
                    let max_loss = (trade.entry_price - current_high) / trade.entry_price;
                    trade.max_unrealized_profit = trade.max_unrealized_profit.max(max_profit);
                    trade.max_unrealized_loss = trade.max_unrealized_loss.min(max_loss);

                    // 检查止损
                    if let Some(sl) = trade.sl_price {
                        if current_high >= sl {
                            // 止损触发
                            let pnl = (trade.entry_price - sl) / trade.entry_price;
                            if let Some(signal) = filtered_signals.get_mut(trade.signal_index) {
                                signal.final_pnl = pnl;
                                signal.theoretical_loss = trade.max_unrealized_loss;
                                signal.theoretical_profit = trade.max_unrealized_profit;
                                signal.trade_result = "LOSS".to_string();
                            }
                            completed_indices.push(idx);
                            continue;
                        }
                    }

                    // 检查止盈
                    if let Some(tp) = trade.tp_price {
                        if current_low <= tp {
                            // 止盈触发
                            let pnl = (trade.entry_price - tp) / trade.entry_price;
                            if let Some(signal) = filtered_signals.get_mut(trade.signal_index) {
                                signal.final_pnl = pnl;
                                signal.theoretical_loss = trade.max_unrealized_loss;
                                signal.theoretical_profit = trade.max_unrealized_profit;
                                signal.trade_result = "WIN".to_string();
                            }
                            completed_indices.push(idx);
                            continue;
                        }
                    }
                }
            }
        }

        // 移除已完成的影子交易 (从后往前移除以避免索引失效)
        for idx in completed_indices.iter().rev() {
            shadow_trades.remove(*idx);
        }

        if candle_buffer.len() < window_size {
            continue;
        }

        // Get the view of the sliding window
        let current_slice = &candle_buffer[candle_buffer.len() - window_size..];

        let mut signal = strategy(current_slice, &mut multi_indicator_values);
        if i < 500 {
            continue;
        }

        // 处理过滤信号记录
        // 处理过滤信号记录
        if !signal.filter_reasons.is_empty() {
             use rust_quant_domain::SignalDirection;
             
             let direction = match signal.direction {
                 SignalDirection::Long => Some(TradeSide::Long),
                 SignalDirection::Short => Some(TradeSide::Short),
                 SignalDirection::Close => None,
                 SignalDirection::None => {
                     // 尝试从 should_buy/should_sell 推断
                     if signal.should_buy {
                         Some(TradeSide::Long)
                     } else if signal.should_sell {
                         Some(TradeSide::Short)
                     } else {
                         None
                     }
                 }
             };

            if let Some(direction) = direction {
                 let direction_str = match direction {
                    TradeSide::Long => "LONG",
                    TradeSide::Short => "SHORT",
                };

                // 创建 FilteredSignal 记录
                filtered_signals.push(super::types::FilteredSignal {
                    ts: candle.ts,
                    inst_id: inst_id.to_string(),
                    direction: direction_str.to_string(),
                    signal_price: candle.c,
                    filter_reasons: signal.filter_reasons.clone(),
                    indicator_snapshot: "{}".to_string(), // TODO: 序列化指标快照
                    theoretical_profit: 0.0,
                    theoretical_loss: 0.0,
                    final_pnl: 0.0,
                    trade_result: "RUNNING".to_string(),
                });

                // 创建 ShadowTrade
                let signal_index = filtered_signals.len() - 1;
                let entry_price = candle.c;
                
                // 确定止损价格 (优先使用 signal 中的止损，否则使用默认)
                let sl_price = if direction == TradeSide::Long {
                     signal.atr_stop_loss_price.or(signal.signal_kline_stop_loss_price)
                } else {
                     signal.atr_stop_loss_price.or(signal.signal_kline_stop_loss_price)
                };

                // 确定止盈价格 (优先使用 signal 中的止盈，否则使用 2R 止盈作为默认)
                let tp_price = if direction == TradeSide::Long {
                    signal.atr_take_profit_ratio_price
                        .or(signal.long_signal_take_profit_price)
                        // 若无明确止盈，假设 2R (Reward/Risk = 2)
                        // .or(sl_price.map(|sl| entry_price + (entry_price - sl) * 2.0)) 
                } else {
                    signal.atr_take_profit_ratio_price
                        .or(signal.short_signal_take_profit_price)
                        // .or(sl_price.map(|sl| entry_price - (sl - entry_price) * 2.0))
                };
                
                shadow_trades.push(super::types::ShadowTrade {
                    signal_index,
                    entry_price,
                    direction,
                    sl_price,
                    tp_price,
                    entry_time: candle.ts,
                    max_unrealized_profit: 0.0,
                    max_unrealized_loss: 0.0,
                });
            }
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
                current_slice,
                i,
            );
        }

        if candle_buffer.len() >= capacity {
            let remove_count = candle_buffer.len() - window_size;
            candle_buffer.drain(0..remove_count);
        }
    }

    // --- Finalize: Close remaining shadow trades ---
    if let Some(last_candle) = candles_list.last() {
        for trade in shadow_trades {
            if let Some(signal) = filtered_signals.get_mut(trade.signal_index) {
                let current_close = last_candle.c;
                // 计算最终未实现的盈亏
                let pnl = match trade.direction {
                    TradeSide::Long => (current_close - trade.entry_price) / trade.entry_price,
                    TradeSide::Short => (trade.entry_price - current_close) / trade.entry_price,
                };
                
                // 更新最大浮盈浮亏
                let (max_profit, max_loss) = match trade.direction {
                    TradeSide::Long => (
                        ((last_candle.h - trade.entry_price) / trade.entry_price).max(trade.max_unrealized_profit),
                        ((last_candle.l - trade.entry_price) / trade.entry_price).min(trade.max_unrealized_loss)
                    ),
                    TradeSide::Short => (
                         ((trade.entry_price - last_candle.l) / trade.entry_price).max(trade.max_unrealized_profit),
                         ((trade.entry_price - last_candle.h) / trade.entry_price).min(trade.max_unrealized_loss)
                    ),
                };

                signal.final_pnl = pnl;
                signal.theoretical_profit = max_profit;
                signal.theoretical_loss = max_loss;
                signal.trade_result = "END".to_string(); // Marked as closed at end
            }
        }
    }

    if !candle_buffer.is_empty() {
        finalize_trading_state(&mut trading_state, &candle_buffer);
    }

    BackTestResult {
        funds: trading_state.funds,
        win_rate: calculate_win_rate(trading_state.wins, trading_state.losses),
        open_trades: trading_state.open_position_times,
        trade_records: trading_state.trade_records,
        filtered_signals,
    }
}
