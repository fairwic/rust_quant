use super::types::{SignalResult, TradeRecord};
use crate::CandleItem;
use std::collections::HashSet;

/// 处理斐波那契部分止盈逻辑
#[allow(clippy::too_many_arguments)]
pub fn process_fibonacci_levels(
    current_candle: &CandleItem,
    funds: &mut f64,
    position: &mut f64,
    entry_price: f64,
    signal: &SignalResult,
    fib_levels: &[f64],
    feibon_profil_levels: &[f64],
    entry_time: &str,
    ts: &i64,
    total_profit_loss: &mut f64,
    trade_records: &mut Vec<TradeRecord>,
    triggered_fib_levels: &mut HashSet<usize>,
    is_long: bool,
    wins: &mut i64,
    losses: &mut i64,
) -> f64 {
    let mut remaining_position = *position;

    for (idx, &level) in fib_levels.iter().enumerate() {
        if triggered_fib_levels.contains(&idx) {
            continue;
        }

        let fib_price = if is_long {
            entry_price * (1.0 + level)
        } else {
            entry_price * (1.0 - level)
        };

        if (is_long && signal.open_price >= fib_price)
            || (!is_long && signal.open_price <= fib_price)
        {
            let sell_amount = *position * feibon_profil_levels[idx];

            if sell_amount < 1e-8 {
                continue;
            }

            if is_long {
                *funds += sell_amount * (fib_price - entry_price);
            } else {
                *funds += sell_amount * (entry_price - fib_price);
            }

            remaining_position -= sell_amount;

            if remaining_position <= 1e-8 {
                close_remaining_position(
                    &entry_price,
                    funds,
                    position,
                    total_profit_loss,
                    trade_records,
                    current_candle,
                    entry_time,
                    wins,
                    losses,
                    triggered_fib_levels,
                    is_long,
                );
                continue;
            } else {
                let exit_time = rust_quant_common::utils::time::mill_time_to_datetime(*ts).unwrap();

                let profit_loss = if is_long {
                    sell_amount * (fib_price - entry_price)
                } else {
                    sell_amount * (entry_price - fib_price)
                };
                *total_profit_loss += profit_loss;

                trade_records.push(TradeRecord {
                    signal_status: 0,
                    option_type: "fibonacci_close".to_string(),
                    open_position_time: entry_time.to_string(),
                    signal_open_position_time: Some(signal.ts.to_string()),
                    close_position_time: Some(exit_time),
                    open_price: entry_price,
                    close_price: Some(fib_price),
                    profit_loss: *total_profit_loss,
                    quantity: sell_amount,
                    full_close: remaining_position <= 1e-8,
                    close_type: "斐波那契止盈".to_string(),
                    win_num: 0,
                    loss_num: 0,
                    signal_value: signal.single_value.clone(),
                    signal_result: signal.single_result.clone(),
                    stop_loss_source: None,
                    stop_loss_update_history: None,
                });
                triggered_fib_levels.insert(idx);
                tracing::info!(
                    "Fibonacci profit taking at level: {:?}, time: {:?}, price: {}, sell amount: {}, remaining position: {}, funds after profit taking: {}",
                    level, rust_quant_common::utils::time::mill_time_to_datetime_shanghai(*ts), signal.open_price, sell_amount, remaining_position, *funds
                );
            }
        }
    }
    remaining_position
}

/// 平仓剩余仓位
#[allow(clippy::too_many_arguments)]
pub fn close_remaining_position(
    entry_price: &f64,
    funds: &mut f64,
    position: &mut f64,
    total_profit_loss: &mut f64,
    trade_records: &mut Vec<TradeRecord>,
    current_candle: &CandleItem,
    entry_time: &str,
    wins: &mut i64,
    losses: &mut i64,
    triggered_fib_levels: &mut HashSet<usize>,
    is_long: bool,
) {
    let last_price = current_candle.c();
    let exit_time =
        rust_quant_common::utils::time::mill_time_to_datetime(current_candle.ts).unwrap();

    let current_profit_loss = if is_long {
        *position * (last_price - *entry_price)
    } else {
        *position * (*entry_price - last_price)
    };

    *funds += current_profit_loss;
    *total_profit_loss += current_profit_loss;

    if *total_profit_loss > 0.0 {
        *wins += 1;
    } else {
        *losses += 1;
    }

    trade_records.push(TradeRecord {
        signal_status: 0,
        option_type: "close".to_string(),
        open_position_time: entry_time.to_string(),
        signal_open_position_time: Some(entry_time.to_string()),
        close_position_time: Some(exit_time),
        open_price: *entry_price,
        close_price: Some(last_price),
        profit_loss: *total_profit_loss,
        quantity: *position,
        full_close: true,
        close_type: if is_long {
            "止盈".to_string()
        } else {
            "止损".to_string()
        },
        win_num: *wins,
        loss_num: *losses,
        signal_value: None,
        signal_result: None,
        stop_loss_source: None,
        stop_loss_update_history: None,
    });
    *position = 0.0;
    triggered_fib_levels.clear();
}
