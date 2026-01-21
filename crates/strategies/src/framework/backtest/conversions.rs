use crate::framework::backtest::types::{BasicRiskStrategyConfig, SignalResult};

/// 将 domain 层的 `SignalResult` 转换为 strategies 可用的 `SignalResult`
pub fn convert_domain_signal(domain_signal: rust_quant_domain::SignalResult) -> SignalResult {
    SignalResult {
        should_buy: domain_signal.should_buy.unwrap_or(false),
        should_sell: domain_signal.should_sell.unwrap_or(false),
        open_price: domain_signal.open_price.unwrap_or(0.0),
        signal_kline_stop_loss_price: domain_signal.signal_kline_stop_loss_price,
        stop_loss_source: domain_signal.stop_loss_source,
        best_open_price: domain_signal.best_open_price,
        atr_take_profit_ratio_price: domain_signal.atr_take_profit_ratio_price,
        atr_stop_loss_price: domain_signal.atr_stop_loss_price,
        long_signal_take_profit_price: domain_signal.long_signal_take_profit_price,
        short_signal_take_profit_price: domain_signal.short_signal_take_profit_price,
        move_stop_open_price_when_touch_price: domain_signal
            .move_stop_open_price_when_touch_price
            .clone(),
        ts: domain_signal.ts.unwrap_or(0),
        single_value: domain_signal.single_value.map(|v| v.to_string()),
        single_result: domain_signal.single_result.map(|v| v.to_string()),
        counter_trend_pullback_take_profit_price: domain_signal
            .counter_trend_pullback_take_profit_price,
        is_ema_short_trend: None,
        is_ema_long_trend: None,
        atr_take_profit_level_1: None,
        atr_take_profit_level_2: None,
        atr_take_profit_level_3: None,
        filter_reasons: domain_signal.filter_reasons,
        direction: domain_signal.direction,
    }
}

/// 将 strategies 的基础风控配置转换为 domain 层所需结构
pub fn to_domain_basic_risk_config(
    cfg: &BasicRiskStrategyConfig,
) -> rust_quant_domain::BasicRiskConfig {
    rust_quant_domain::BasicRiskConfig {
        max_loss_percent: cfg.max_loss_percent,
        atr_take_profit_ratio: cfg.atr_take_profit_ratio,
        fix_signal_kline_take_profit_ratio: cfg.fixed_signal_kline_take_profit_ratio,
        is_used_signal_k_line_stop_loss: cfg.is_used_signal_k_line_stop_loss,
        is_counter_trend_pullback_take_profit: cfg.is_counter_trend_pullback_take_profit,
        is_move_stop_loss: cfg.is_one_k_line_diff_stop_loss,
        max_hold_time: None,
        max_leverage: None,
    }
}
