use std::collections::VecDeque;
use rust_quant_indicators::trend::vegas::{IndicatorCombine, VegasIndicatorSignalValue};
use crate::CandleItem;
use super::types::{BackTestResult, BasicRiskStrategyConfig, SignalResult, TradingState};
use super::indicators::get_multi_indicator_values;
use super::signal::deal_signal;
use super::position::finalize_trading_state;
use super::utils::calculate_win_rate;

/// 回测引擎：支持Vegas策略
pub fn run_back_test(
    mut strategy: impl FnMut(&[CandleItem], &mut VegasIndicatorSignalValue) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IndicatorCombine,
) -> BackTestResult {
    // 初始化阶段
    let mut trading_state = TradingState::default();
    let mut candle_item_list: VecDeque<CandleItem> = VecDeque::with_capacity(candles_list.len());
    // 基于指标组合动态计算回看窗口
    let dynamic_lookback = indicator_combine
        .max_required_lookback()
        .max(min_data_length);

    for (i, candle) in candles_list.iter().enumerate() {
        // 计算指标值
        let mut multi_indicator_values = get_multi_indicator_values(indicator_combine, &candle);

        // 将新数据添加到列表，如果超过最大回溯期，删除最旧的数据
        candle_item_list.push_back(candle.clone());
        if candle_item_list.len() > dynamic_lookback {
            let _ = candle_item_list.pop_front();
        }

        let mut signal = strategy(
            candle_item_list.make_contiguous(),
            &mut multi_indicator_values,
        );

        // 处理交易信号前检查是否值得处理（性能优化）
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
                &candle_item_list,
                i,
            );
        }
    }
    // 最终平仓处理
    finalize_trading_state(&mut trading_state, &candle_item_list);

    // 构建结果
    BackTestResult {
        funds: trading_state.funds,
        win_rate: calculate_win_rate(trading_state.wins, trading_state.losses),
        open_trades: trading_state.open_position_times,
        trade_records: trading_state.trade_records,
    }
}

/// 通用回测引擎：支持自定义指标组合与指标值结构
pub fn run_back_test_generic<IC, IV>(
    mut strategy: impl FnMut(&[CandleItem], &mut IV) -> SignalResult,
    candles_list: &Vec<CandleItem>,
    basic_risk_config: BasicRiskStrategyConfig,
    min_data_length: usize,
    indicator_combine: &mut IC,
    mut build_values: impl FnMut(&mut IC, &CandleItem) -> IV,
) -> BackTestResult {
    let mut trading_state = TradingState::default();
    let mut candle_item_list: VecDeque<CandleItem> = VecDeque::with_capacity(candles_list.len());
    // 由调用方控制所需窗口，这里仅保证最小长度
    let dynamic_lookback = min_data_length;

    for (i, candle) in candles_list.iter().enumerate() {
        // 计算自定义指标
        let mut multi_indicator_values = build_values(indicator_combine, &candle);

        candle_item_list.push_back(candle.clone());
        if candle_item_list.len() > dynamic_lookback {
            let _ = candle_item_list.pop_front();
        }
        if candle_item_list.len() < dynamic_lookback {
            continue;
        }

        let mut signal = strategy(
            candle_item_list.make_contiguous(),
            &mut multi_indicator_values,
        );
        if i < 500 {
            continue;
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
                &candle_item_list,
                i,
            );
        }
    }
    finalize_trading_state(&mut trading_state, &candle_item_list);
    BackTestResult {
        funds: trading_state.funds,
        win_rate: calculate_win_rate(trading_state.wins, trading_state.losses),
        open_trades: trading_state.open_position_times,
        trade_records: trading_state.trade_records,
    }
}

