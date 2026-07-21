use super::super::types::TradeSide;
use super::recording::{record_trade_entry, record_trade_exit_with_full_close};
use super::risk::compute_initial_stop_price;
use super::types::{BasicRiskStrategyConfig, SignalResult, TradePosition, TradingState};
use crate::CandleItem;
use rust_quant_domain::enums::PositionSide;
use tracing::debug;

/// Historical backtest fee rate used when a strategy has not opted into a newer cost model.
const LEGACY_BACKTEST_TRADE_FEE_RATE: f64 = 0.0007;

/// 返回回测仓位乘数；允许 0 到 1 之间的值用于非全仓标准化回测。
fn position_size_multiplier(risk_config: &BasicRiskStrategyConfig) -> f64 {
    risk_config
        .position_leverage
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0)
}

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
    let leverage = position_size_multiplier(&risk_config);
    let mut temp_trade_position = TradePosition {
        position_nums: (state.funds / signal.open_price) * leverage,
        open_price: signal.open_price,
        open_position_time: rust_quant_common::utils::time::mill_time_to_datetime(candle.ts)
            .unwrap_or_default(),
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Long,
        trade_fee_rate: risk_config.trade_fee_rate,
        ..Default::default()
    };
    // 记录入场K线振幅，用于固定比例止盈计算
    let raw_range = (candle.h - candle.l).abs();
    let k_range = raw_range.max(signal.open_price * 0.001);
    temp_trade_position.signal_high_low_diff = k_range;
    if raw_range > 0.0 && candle.l > 0.0 {
        temp_trade_position.entry_kline_amplitude = Some(raw_range / candle.l.max(1e-9));
        temp_trade_position.entry_kline_close_pos = Some((candle.c - candle.l) / raw_range);
    }
    //设置止盈止损价格
    set_long_stop_close_price(risk_config, signal, &mut temp_trade_position);
    temp_trade_position.initial_stop_price =
        compute_initial_stop_price(&temp_trade_position, &risk_config);
    apply_first_retest_take_profit(signal, &mut temp_trade_position);
    if signal.signal_kline_stop_loss_price.is_none()
        && signal.stop_loss_source.as_deref() == Some("RepairLong_NoSignalKline")
    {
        temp_trade_position.stop_loss_source = signal.stop_loss_source.clone();
    }
    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;
    record_trade_entry(state, PositionSide::Long.as_str().to_owned(), signal);
}
// ============================================================================
// 止盈止损设置 - 公共逻辑
// ============================================================================
/// 判断信号止损是否位于当前信号价格的保护一侧；移动止损可越过原始入场价。
fn is_protective_signal_stop(trade_side: TradeSide, reference_price: f64, stop_price: f64) -> bool {
    stop_price.is_finite()
        && reference_price.is_finite()
        && match trade_side {
            TradeSide::Long => stop_price < reference_price,
            TradeSide::Short => stop_price > reference_price,
        }
}

/// 设置止盈止损价格的公共逻辑（Long/Short共用）
/// 处理：信号K线止损、ATR止损、移动止损、逆势回调止盈、三级止盈价格
fn set_stop_close_price_common(
    risk_config: &BasicRiskStrategyConfig,
    signal: &SignalResult,
    position: &mut TradePosition,
) {
    let disable_signal_kline_updates = position.trade_side == TradeSide::Long
        && position.stop_loss_source.as_deref() == Some("RepairLong_NoSignalKline");
    // 1. 信号K线止损 + 更新历史记录
    if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) && !disable_signal_kline_updates
    {
        if let Some(new_price) = signal.signal_kline_stop_loss_price.filter(|price| {
            is_protective_signal_stop(position.trade_side, signal.open_price, *price)
        }) {
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
    // 3. 三级止盈价格
    if !position.fixed_take_profit_only && signal.atr_take_profit_level_1.is_some() {
        position.atr_take_profit_level_1 = signal.atr_take_profit_level_1;
        position.atr_take_profit_level_2 = signal.atr_take_profit_level_2;
        position.atr_take_profit_level_3 = signal.atr_take_profit_level_3;
        position.reached_take_profit_level = 0;
    }
}
// ============================================================================
// 止盈止损设置 - Long/Short 特定逻辑
// ============================================================================
/// 更新 交易执行与风控 状态，并保留调用方需要的结果或错误信息。
pub fn set_long_stop_close_price(
    risk_config: BasicRiskStrategyConfig,
    signal: &SignalResult,
    temp_trade_position: &mut TradePosition,
) {
    // ============ Long特有逻辑 ============
    // 1. 信号止盈价格（做多）
    if !temp_trade_position.fixed_take_profit_only {
        temp_trade_position.long_signal_take_profit_price = signal.long_signal_take_profit_price;
    }
    // 2. 固定比例止盈（Long: open_price + diff * ratio）
    if !temp_trade_position.fixed_take_profit_only {
        if let Some(fixed_take_profit_ratio) = risk_config.fixed_signal_kline_take_profit_ratio {
            if fixed_take_profit_ratio > 0.0 {
                if let Some(p) = signal.signal_kline_stop_loss_price {
                    temp_trade_position.signal_high_low_diff = (p - signal.open_price).abs();
                    temp_trade_position.atr_take_ratio_profit_price = Some(
                        signal.open_price
                            + temp_trade_position.signal_high_low_diff * fixed_take_profit_ratio,
                    );
                } else {
                    debug!("skip fixed take profit: protective signal stop is unavailable");
                }
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
    let leverage = position_size_multiplier(&risk_config);
    let mut temp_trade_position = TradePosition {
        position_nums: (state.funds / signal.open_price) * leverage,
        open_price: signal.open_price,
        open_position_time: rust_quant_common::utils::time::mill_time_to_datetime(candle.ts)
            .unwrap_or_default(),
        signal_open_position_time: signal_open_time,
        trade_side: TradeSide::Short,
        trade_fee_rate: risk_config.trade_fee_rate,
        ..Default::default()
    };
    // 记录入场K线振幅，用于固定比例止盈计算
    let raw_range = (candle.h - candle.l).abs();
    let k_range = raw_range.max(signal.open_price * 0.001);
    temp_trade_position.signal_high_low_diff = k_range;
    if raw_range > 0.0 && candle.l > 0.0 {
        temp_trade_position.entry_kline_amplitude = Some(raw_range / candle.l.max(1e-9));
        temp_trade_position.entry_kline_close_pos = Some((candle.c - candle.l) / raw_range);
    }
    //设置止盈止损价格
    set_short_stop_close_price(risk_config, signal, &mut temp_trade_position);
    temp_trade_position.initial_stop_price =
        compute_initial_stop_price(&temp_trade_position, &risk_config);
    apply_first_retest_take_profit(signal, &mut temp_trade_position);
    apply_short_profit_protection(signal, &mut temp_trade_position);
    state.trade_position = Some(temp_trade_position);
    state.open_position_times += 1;
    state.last_signal_result = None;
    record_trade_entry(state, PositionSide::Short.as_str().to_owned(), signal);
}

/// 根据版本化信号声明，为空头冻结盈利保护的触发价与止损价。
///
/// 触发价依赖最终初始止损，因此必须在 `initial_stop_price` 选定后计算；风险检查会在触发 K 线
/// 完成后更新移动止损，使保本价只从下一根 K 线起生效。
fn apply_short_profit_protection(signal: &SignalResult, position: &mut TradePosition) {
    if position.trade_side != TradeSide::Short {
        return;
    }
    let protection = if signal
        .dynamic_adjustments
        .iter()
        .any(|adjustment| adjustment == "SHORT_PROFIT_PROTECTION_1_5R")
    {
        Some((1.5, 0.0))
    } else if signal
        .dynamic_adjustments
        .iter()
        .any(|adjustment| adjustment == "SHORT_PROFIT_LOCK_2R_TO_1R")
    {
        Some((2.0, 1.0))
    } else {
        None
    };
    let Some((trigger_r, stop_r)) = protection else {
        return;
    };
    let Some(initial_stop) = position.initial_stop_price else {
        return;
    };
    if !initial_stop.is_finite() || initial_stop <= position.open_price {
        return;
    }
    let initial_r = initial_stop - position.open_price;
    position.profit_protection_trigger_price = Some(position.open_price - initial_r * trigger_r);
    position.profit_protection_stop_price = Some(position.open_price - initial_r * stop_r);
}

/// 按显式版本化信号冻结唯一价格目标，或用最终有效止损换算 R 目标。
///
/// 形态止损可能被最大亏损门禁收紧，所以这里不复用原始形态距离。
/// 新目标只是上限；已有 ATR 或指标目标更近时仍优先早退出。
fn apply_first_retest_take_profit(signal: &SignalResult, position: &mut TradePosition) {
    const FAILED_AUCTION_POC_ONLY: &str = "VOLUME_PROFILE_FAILED_AUCTION_POC_ONLY";
    const CAP_PREFIX: &str = "LIQUIDITY_SWEEP_FIRST_RETEST_TP_R:";
    const ONLY_PREFIX: &str = "LIQUIDITY_SWEEP_FIRST_RETEST_TP_ONLY_R:";
    const VOLUME_PROFILE_ONLY_PREFIX: &str = "VOLUME_PROFILE_VALUE_AREA_BREAKOUT_TP_ONLY_R:";
    const DONCHIAN_ONLY_PREFIX: &str = "DONCHIAN_VOLUME_BREAKOUT_TP_ONLY_R:";
    const DONCHIAN_ACCEPTANCE_ONLY_PREFIX: &str = "DONCHIAN_BREAKOUT_ACCEPTANCE_TP_ONLY_R:";

    if signal
        .dynamic_adjustments
        .iter()
        .any(|adjustment| adjustment == FAILED_AUCTION_POC_ONLY)
    {
        let explicit_target = match position.trade_side {
            TradeSide::Long => signal.long_signal_take_profit_price,
            TradeSide::Short => signal.short_signal_take_profit_price,
        };
        if let Some(target) = explicit_target.filter(|target| {
            target.is_finite()
                && match position.trade_side {
                    TradeSide::Long => *target > position.open_price,
                    TradeSide::Short => *target < position.open_price,
                }
        }) {
            position.fixed_take_profit_price = Some(target);
            position.fixed_take_profit_only = true;
            position.atr_take_profit_level_1 = None;
            position.atr_take_profit_level_2 = None;
            position.atr_take_profit_level_3 = None;
            position.atr_take_ratio_profit_price = None;
            position.long_signal_take_profit_price = None;
            position.short_signal_take_profit_price = None;
        }
        return;
    }

    let take_profit = signal.dynamic_adjustments.iter().find_map(|adjustment| {
        let (value, replace_existing) = adjustment
            .strip_prefix(ONLY_PREFIX)
            .map(|value| (value, true))
            .or_else(|| {
                adjustment
                    .strip_prefix(VOLUME_PROFILE_ONLY_PREFIX)
                    .map(|value| (value, true))
            })
            .or_else(|| {
                adjustment
                    .strip_prefix(DONCHIAN_ONLY_PREFIX)
                    .map(|value| (value, true))
            })
            .or_else(|| {
                adjustment
                    .strip_prefix(DONCHIAN_ACCEPTANCE_ONLY_PREFIX)
                    .map(|value| (value, true))
            })
            .or_else(|| {
                adjustment
                    .strip_prefix(CAP_PREFIX)
                    .map(|value| (value, false))
            })?;
        value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| (value, replace_existing))
    });
    let (Some((take_profit_r, replace_existing)), Some(initial_stop)) =
        (take_profit, position.initial_stop_price)
    else {
        return;
    };
    let initial_risk = (initial_stop - position.open_price).abs();
    if !initial_risk.is_finite() || initial_risk <= 0.0 {
        return;
    }

    let r_target = match position.trade_side {
        TradeSide::Long => position.open_price + initial_risk * take_profit_r,
        TradeSide::Short => position.open_price - initial_risk * take_profit_r,
    };
    if replace_existing {
        position.fixed_take_profit_price = Some(r_target);
        position.fixed_take_profit_only = true;
        position.atr_take_profit_level_1 = None;
        position.atr_take_profit_level_2 = None;
        position.atr_take_profit_level_3 = None;
        position.atr_take_ratio_profit_price = None;
        position.long_signal_take_profit_price = None;
        position.short_signal_take_profit_price = None;
        return;
    }
    let existing_targets = [
        position.atr_take_profit_level_3,
        position.atr_take_ratio_profit_price,
        position.long_signal_take_profit_price,
        position.short_signal_take_profit_price,
    ];
    position.fixed_take_profit_price = existing_targets
        .into_iter()
        .flatten()
        .filter(|target| {
            target.is_finite()
                && match position.trade_side {
                    TradeSide::Long => *target > position.open_price,
                    TradeSide::Short => *target < position.open_price,
                }
        })
        .chain(std::iter::once(r_target))
        .min_by(|left, right| {
            (left - position.open_price)
                .abs()
                .partial_cmp(&(right - position.open_price).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
}
/// 更新 交易执行与风控 状态，并保留调用方需要的结果或错误信息。
pub fn set_short_stop_close_price(
    risk_config: BasicRiskStrategyConfig,
    signal: &SignalResult,
    temp_trade_position: &mut TradePosition,
) {
    // ============ Short特有逻辑 ============
    // 1. 信号止盈价格（做空）
    if !temp_trade_position.fixed_take_profit_only {
        temp_trade_position.short_signal_take_profit_price = signal.short_signal_take_profit_price;
    }
    // 2. ATR比例止盈（Short: open_price - diff * ratio）
    if !temp_trade_position.fixed_take_profit_only {
        if let Some(atr_take_profit_ratio) = risk_config.atr_take_profit_ratio {
            if atr_take_profit_ratio > 0.0 {
                if let Some(atr_stop_loss_price) = signal.atr_stop_loss_price {
                    let diff_price = (atr_stop_loss_price - signal.open_price).abs();
                    temp_trade_position.atr_take_ratio_profit_price =
                        Some(signal.open_price - (diff_price * atr_take_profit_ratio));
                } else {
                    debug!("skip ATR take profit: ATR stop is unavailable");
                }
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
    let exit_time =
        rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap_or_default();
    let mut trade_position = match state.trade_position.clone() {
        Some(p) => p,
        None => return,
    };
    let quantity = trade_position.position_nums;
    let fee_rate = trade_position
        .trade_fee_rate
        .unwrap_or(LEGACY_BACKTEST_TRADE_FEE_RATE);
    let close_price = trade_position.close_price.unwrap_or(signal.open_price);
    let fee = quantity * (trade_position.open_price + close_price) * fee_rate;
    let profit_after_fee = profit - fee;
    let trade_profit_after_fee = trade_position.profit_loss + profit_after_fee;
    trade_position.profit_loss = profit_after_fee;
    state.trade_position = Some(trade_position);
    // 更新总利润和资金
    state.total_profit_loss += profit_after_fee;
    state.funds += profit_after_fee;
    // 更新胜率
    if trade_profit_after_fee > 0.0 {
        state.wins += 1;
    } else if trade_profit_after_fee < 0.00 {
        state.losses += 1;
    }
    // 根据平仓原因和盈亏设置正确的平仓类型
    record_trade_exit(state, exit_time, signal, close_type, quantity);
    // 更新总利润和资金
    state.trade_position = None;
}

/// 部分平仓并保留剩余仓位，供显式启用的分批止盈回测使用。
pub fn partial_close_position(
    state: &mut TradingState,
    candle: &CandleItem,
    signal: &SignalResult,
    close_type: &str,
    close_price: f64,
    closing_quantity: f64,
) {
    let exit_time =
        rust_quant_common::utils::time::mill_time_to_datetime(candle.ts).unwrap_or_default();
    let mut trade_position = match state.trade_position.clone() {
        Some(p) => p,
        None => return,
    };
    let quantity = closing_quantity.min(trade_position.position_nums).max(0.0);
    if quantity <= 0.0 || trade_position.position_nums <= 0.0 {
        return;
    }
    let fee_rate = trade_position
        .trade_fee_rate
        .unwrap_or(LEGACY_BACKTEST_TRADE_FEE_RATE);
    let gross_profit = match trade_position.trade_side {
        TradeSide::Long => (close_price - trade_position.open_price) * quantity,
        TradeSide::Short => (trade_position.open_price - close_price) * quantity,
    };
    let fee = quantity * (trade_position.open_price + close_price) * fee_rate;
    let profit_after_fee = gross_profit - fee;
    let cumulative_profit_after_fee = trade_position.profit_loss + profit_after_fee;
    trade_position.position_nums -= quantity;
    trade_position.close_price = Some(close_price);
    trade_position.profit_loss = profit_after_fee;
    state.trade_position = Some(trade_position);
    state.total_profit_loss += profit_after_fee;
    state.funds += profit_after_fee;
    record_trade_exit_with_full_close(state, exit_time, signal, close_type, quantity, false);
    if let Some(position) = state.trade_position.as_mut() {
        position.profit_loss = cumulative_profit_after_fee;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::SignalDirection;

    fn candle(ts: i64, close: f64) -> CandleItem {
        CandleItem {
            ts,
            o: close,
            h: close + 1.0,
            l: close - 1.0,
            c: close,
            v: 1.0,
            confirm: 1,
        }
    }

    fn signal(ts: i64, price: f64, direction: SignalDirection) -> SignalResult {
        SignalResult {
            should_buy: direction == SignalDirection::Long,
            should_sell: direction == SignalDirection::Short,
            open_price: price,
            ts,
            direction,
            ..Default::default()
        }
    }

    #[test]
    fn open_long_position_allows_fractional_position_leverage() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            position_leverage: Some(0.6),
            ..Default::default()
        };

        open_long_position(
            risk,
            &mut state,
            &candle(1, 100.0),
            &signal(1, 100.0, SignalDirection::Long),
            None,
        );

        let position = state.trade_position.expect("position should open");
        assert!((position.position_nums - 0.6).abs() < 1e-9);
    }

    #[test]
    fn repair_long_ignores_later_signal_kline_stop_updates() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            is_used_signal_k_line_stop_loss: Some(true),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Long);
        entry.stop_loss_source = Some("RepairLong_NoSignalKline".to_string());

        open_long_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let mut later_signal = signal(2, 105.0, SignalDirection::Long);
        later_signal.signal_kline_stop_loss_price = Some(103.0);
        later_signal.stop_loss_source = Some("Engulfing_Volume_Confirmed".to_string());
        let position = state.trade_position.as_mut().expect("position should open");
        set_long_stop_close_price(risk, &later_signal, position);

        assert_eq!(position.signal_kline_stop_close_price, None);
        assert_eq!(
            position.stop_loss_source.as_deref(),
            Some("RepairLong_NoSignalKline")
        );
        assert!(position.stop_loss_updates.is_empty());
    }

    #[test]
    fn open_short_position_allows_fractional_position_leverage() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            position_leverage: Some(0.6),
            ..Default::default()
        };

        open_short_position(
            risk,
            &mut state,
            &candle(1, 100.0),
            &signal(1, 100.0, SignalDirection::Short),
            None,
        );

        let position = state.trade_position.expect("position should open");
        assert!((position.position_nums - 0.6).abs() < 1e-9);
    }

    #[test]
    fn short_profit_protection_uses_frozen_initial_risk() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry
            .dynamic_adjustments
            .push("SHORT_PROFIT_PROTECTION_1_5R".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position should open");
        assert_eq!(position.initial_stop_price, Some(102.0));
        assert_eq!(position.profit_protection_trigger_price, Some(97.0));
        assert_eq!(position.profit_protection_stop_price, Some(100.0));
        assert_eq!(position.move_stop_open_price, None);
    }

    #[test]
    fn short_profit_lock_freezes_two_r_trigger_and_one_r_stop() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry
            .dynamic_adjustments
            .push("SHORT_PROFIT_LOCK_2R_TO_1R".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position should open");
        assert_eq!(position.initial_stop_price, Some(102.0));
        assert_eq!(position.profit_protection_trigger_price, Some(96.0));
        assert_eq!(position.profit_protection_stop_price, Some(98.0));
        assert!(!position.profit_protection_armed);
    }

    #[test]
    fn first_retest_take_profit_caps_far_target_at_frozen_two_r() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry.short_signal_take_profit_price = Some(90.0);
        entry
            .dynamic_adjustments
            .push("LIQUIDITY_SWEEP_FIRST_RETEST_TP_R:2".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position should open");
        assert_eq!(position.initial_stop_price, Some(102.0));
        assert_eq!(position.fixed_take_profit_price, Some(96.0));
    }

    #[test]
    fn first_retest_take_profit_keeps_nearer_existing_target() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry.short_signal_take_profit_price = Some(97.0);
        entry
            .dynamic_adjustments
            .push("LIQUIDITY_SWEEP_FIRST_RETEST_TP_R:2".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position should open");
        assert_eq!(position.fixed_take_profit_price, Some(97.0));
    }

    #[test]
    fn first_retest_exact_r_replaces_and_freezes_existing_targets() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry.short_signal_take_profit_price = Some(97.0);
        entry
            .dynamic_adjustments
            .push("LIQUIDITY_SWEEP_FIRST_RETEST_TP_ONLY_R:2".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.as_mut().expect("short position");
        assert_eq!(position.fixed_take_profit_price, Some(96.0));
        assert!(position.fixed_take_profit_only);
        assert_eq!(position.short_signal_take_profit_price, None);
        assert_eq!(position.atr_take_ratio_profit_price, None);

        let mut later_signal = signal(2, 99.0, SignalDirection::Short);
        later_signal.short_signal_take_profit_price = Some(98.0);
        set_short_stop_close_price(risk, &later_signal, position);
        assert_eq!(position.short_signal_take_profit_price, None);
        assert_eq!(position.fixed_take_profit_price, Some(96.0));
    }

    #[test]
    fn volume_profile_breakout_exact_r_reuses_final_effective_initial_stop() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry
            .dynamic_adjustments
            .push("VOLUME_PROFILE_VALUE_AREA_BREAKOUT_TP_ONLY_R:2".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position");
        assert_eq!(position.initial_stop_price, Some(102.0));
        assert_eq!(position.fixed_take_profit_price, Some(96.0));
        assert!(position.fixed_take_profit_only);
        assert_eq!(position.short_signal_take_profit_price, None);
        assert_eq!(position.atr_take_ratio_profit_price, None);
    }

    #[test]
    fn failed_auction_uses_frozen_poc_as_only_take_profit() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry.short_signal_take_profit_price = Some(97.5);
        entry
            .dynamic_adjustments
            .push("VOLUME_PROFILE_FAILED_AUCTION_POC_ONLY".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position");
        assert_eq!(position.fixed_take_profit_price, Some(97.5));
        assert!(position.fixed_take_profit_only);
        assert_eq!(position.atr_take_profit_level_3, None);
        assert_eq!(position.short_signal_take_profit_price, None);
    }

    #[test]
    fn donchian_breakout_freezes_two_r_from_effective_stop() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Long);
        entry
            .dynamic_adjustments
            .push("DONCHIAN_VOLUME_BREAKOUT_TP_ONLY_R:2".to_string());

        open_long_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("long position");
        assert_eq!(position.initial_stop_price, Some(98.0));
        assert_eq!(position.fixed_take_profit_price, Some(104.0));
        assert!(position.fixed_take_profit_only);
    }

    #[test]
    fn donchian_acceptance_freezes_two_r_from_effective_stop() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            dynamic_max_loss: Some(false),
            ..Default::default()
        };
        let mut entry = signal(1, 100.0, SignalDirection::Short);
        entry
            .dynamic_adjustments
            .push("DONCHIAN_BREAKOUT_ACCEPTANCE_TP_ONLY_R:2".to_string());

        open_short_position(risk, &mut state, &candle(1, 100.0), &entry, None);

        let position = state.trade_position.expect("short position");
        assert_eq!(position.initial_stop_price, Some(102.0));
        assert_eq!(position.fixed_take_profit_price, Some(96.0));
        assert!(position.fixed_take_profit_only);
    }

    #[test]
    fn open_positions_reject_signal_stops_on_the_profit_side() {
        let risk = BasicRiskStrategyConfig {
            is_used_signal_k_line_stop_loss: Some(true),
            ..Default::default()
        };
        let mut long_state = TradingState::default();
        let mut long_signal = signal(1, 100.0, SignalDirection::Long);
        long_signal.signal_kline_stop_loss_price = Some(101.0);
        open_long_position(risk, &mut long_state, &candle(1, 100.0), &long_signal, None);
        let long_position = long_state.trade_position.expect("long position");
        assert_eq!(long_position.signal_kline_stop_close_price, None);
        assert!(long_position.stop_loss_updates.is_empty());

        let mut short_state = TradingState::default();
        let mut short_signal = signal(1, 100.0, SignalDirection::Short);
        short_signal.signal_kline_stop_loss_price = Some(99.0);
        open_short_position(
            risk,
            &mut short_state,
            &candle(1, 100.0),
            &short_signal,
            None,
        );
        let short_position = short_state.trade_position.expect("short position");
        assert_eq!(short_position.signal_kline_stop_close_price, None);
        assert!(short_position.stop_loss_updates.is_empty());
    }

    #[test]
    fn trade_records_freeze_initial_stop_and_net_profit_r() {
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            is_used_signal_k_line_stop_loss: Some(true),
            trade_fee_rate: Some(0.0),
            ..Default::default()
        };
        let mut state = TradingState::default();
        let mut entry = signal(1, 100.0, SignalDirection::Long);
        entry.signal_kline_stop_loss_price = Some(99.0);

        open_long_position(risk, &mut state, &candle(1, 100.0), &entry, None);
        let entry_record = state.trade_records.first().expect("entry record");
        assert_eq!(entry_record.initial_stop_price, Some(99.0));
        assert_eq!(entry_record.initial_risk_amount, Some(1.0));
        assert_eq!(entry_record.net_profit_r, None);

        if let Some(position) = state.trade_position.as_mut() {
            position.close_price = Some(102.0);
        }
        close_position(&mut state, &candle(2, 102.0), &entry, "test", 2.0);

        let close_record = state.trade_records.last().expect("close record");
        assert_eq!(close_record.initial_stop_price, Some(99.0));
        assert_eq!(close_record.initial_risk_amount, Some(1.0));
        assert_eq!(close_record.net_profit_r, Some(2.0));
    }

    #[test]
    fn close_position_uses_configured_trade_fee_rate() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            trade_fee_rate: Some(0.00005),
            ..Default::default()
        };
        open_long_position(
            risk,
            &mut state,
            &candle(1, 100.0),
            &signal(1, 100.0, SignalDirection::Long),
            None,
        );
        state.trade_position.as_mut().unwrap().close_price = Some(102.0);

        close_position(
            &mut state,
            &candle(2, 102.0),
            &signal(2, 102.0, SignalDirection::Long),
            "test",
            2.0,
        );

        let close_record = state
            .trade_records
            .iter()
            .find(|record| record.full_close)
            .expect("close record");
        assert!((close_record.profit_loss - 1.9899).abs() < 1e-9);
    }

    #[test]
    fn close_position_keeps_legacy_fee_rate_when_not_configured() {
        let mut state = TradingState::default();
        open_long_position(
            BasicRiskStrategyConfig::default(),
            &mut state,
            &candle(1, 100.0),
            &signal(1, 100.0, SignalDirection::Long),
            None,
        );
        state.trade_position.as_mut().unwrap().close_price = Some(102.0);

        close_position(
            &mut state,
            &candle(2, 102.0),
            &signal(2, 102.0, SignalDirection::Long),
            "test",
            2.0,
        );

        let close_record = state
            .trade_records
            .iter()
            .find(|record| record.full_close)
            .expect("close record");
        assert!((close_record.profit_loss - 1.8586).abs() < 1e-9);
    }

    #[test]
    fn close_position_counts_fee_adjusted_loss_as_loss() {
        let mut state = TradingState::default();
        let risk = BasicRiskStrategyConfig {
            trade_fee_rate: Some(0.00005),
            ..Default::default()
        };
        open_long_position(
            risk,
            &mut state,
            &candle(1, 100.0),
            &signal(1, 100.0, SignalDirection::Long),
            None,
        );
        state.trade_position.as_mut().unwrap().close_price = Some(100.005);

        close_position(
            &mut state,
            &candle(2, 100.005),
            &signal(2, 100.005, SignalDirection::Long),
            "test",
            0.005,
        );

        assert_eq!(state.wins, 0);
        assert_eq!(state.losses, 1);
    }
}
