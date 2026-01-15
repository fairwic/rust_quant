use super::indicators::get_multi_indicator_values;
use super::position::finalize_trading_state;
use super::shadow_trading::ShadowTradeManager;
use super::signal::deal_signal;
use super::types::{BackTestResult, BasicRiskStrategyConfig, SignalResult, TradingState};
use super::utils::calculate_win_rate;
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

    // 使用 ShadowTradeManager 管理影子交易
    let mut shadow_manager = ShadowTradeManager::new();

    for (i, candle) in candles_list.iter().enumerate() {
        // 计算自定义指标
        let mut multi_indicator_values = build_values(indicator_combine, &candle);

        candle_buffer.push(candle.clone());

        // --- Shadow Trading: 更新所有活跃的影子交易 ---
        shadow_manager.update_trades(candle);

        if candle_buffer.len() < window_size {
            continue;
        }

        // Get the view of the sliding window
        let current_slice = &candle_buffer[candle_buffer.len() - window_size..];

        let mut signal = strategy(current_slice, &mut multi_indicator_values);
        if i < 500 {
            continue;
        }

        // 处理过滤信号记录（使用 ShadowTradeManager）
        if !signal.filter_reasons.is_empty() {
            shadow_manager.process_filtered_signal(&signal, candle, inst_id);
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

    // --- Finalize: 结束所有剩余的影子交易 ---
    if let Some(last_candle) = candles_list.last() {
        shadow_manager.finalize(last_candle);
    }

    if !candle_buffer.is_empty() {
        finalize_trading_state(&mut trading_state, &candle_buffer);
    }

    BackTestResult {
        funds: trading_state.funds,
        win_rate: calculate_win_rate(trading_state.wins, trading_state.losses),
        open_trades: trading_state.open_position_times,
        trade_records: trading_state.trade_records,
        filtered_signals: shadow_manager.into_filtered_signals(),
    }
}
