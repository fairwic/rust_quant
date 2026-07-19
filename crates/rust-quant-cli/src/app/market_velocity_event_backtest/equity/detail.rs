use super::{
    FrameworkEquityCloseLegReport, FrameworkEquityTradeReport, MarketVelocityEventBacktestArgs,
};
use crate::app::market_velocity_event_backtest::directional_reversal::{
    EXHAUSTION_CURRENT_CLUSTER_CANDLES, EXHAUSTION_SWING_RADIUS_CANDLES,
    EXHAUSTION_VOLUME_LOOKBACK_CANDLES, OPPOSITE_DURATION_MIN_R_SQUARED,
};
use crate::app::market_velocity_event_backtest::{MarketVelocityEventSource, MS_15M};
use serde_json::{json, Value};

/// 返回持久化到 back_test_log/detail 的稳定策略类型。
pub fn market_velocity_strategy_type(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    match args.event_source {
        MarketVelocityEventSource::Episodes => "market_velocity_episode",
        MarketVelocityEventSource::RawEvents => "market_velocity_raw_events",
        MarketVelocityEventSource::RawState => "market_velocity_raw_state",
        MarketVelocityEventSource::Kline15m => "market_velocity_kline_15m",
    }
}

/// 构造 `back_test_log.strategy_detail`，让策略版本的入场参数可以独立复现。
pub fn market_velocity_strategy_detail(args: &MarketVelocityEventBacktestArgs) -> Value {
    json!({
        "source": "market_velocity_event_backtest",
        "event_source": match args.event_source {
            MarketVelocityEventSource::Episodes => "episodes",
            MarketVelocityEventSource::RawEvents => "raw_events",
            MarketVelocityEventSource::RawState => "raw_state",
            MarketVelocityEventSource::Kline15m => "kline_15m",
        },
        "kline_volume_rank_velocity": args.kline_volume_rank_velocity,
        "kline_volume_rank_require_turnover_growth": args.kline_volume_rank_require_turnover_growth,
        "kline_volume_rank_require_consecutive_improvement": args.kline_volume_rank_require_consecutive_improvement,
        "kline_volume_rank_lookback_candles": if args.kline_volume_rank_velocity { json!(96) } else { Value::Null },
        "kline_volume_rank_quote_turnover": if args.kline_volume_rank_velocity { "vol_ccy_x_close" } else { "off" },
        "trade_direction": args.trade_direction.label(),
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "entry_period": args.entry_period,
        "entry_max_distance_pct": args.entry_max_distance_pct,
        "entry_min_volume_ratio": args.entry_min_volume_ratio,
        "entry_min_rsi": args.entry_min_rsi,
        "entry_max_rsi": args.entry_max_rsi,
        "entry_min_rsi_delta": args.entry_min_rsi_delta,
        "entry_rsi_delta_lookback_candles": args.entry_rsi_delta_lookback_candles,
        "entry_bollinger_breakout": args.entry_bollinger_breakout,
        "entry_min_bollinger_bandwidth_expansion_pct": args.entry_min_bollinger_bandwidth_expansion_pct,
        "entry_min_recent_drawdown_pct": args.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": args.entry_recent_drawdown_lookback_candles,
        "entry_opposite_move_lookback_candles": args.entry_opposite_move_lookback_candles,
        "entry_min_opposite_net_move_pct": args.entry_min_opposite_net_move_pct,
        "entry_min_opposite_duration_candles": args.entry_min_opposite_duration_candles,
        "entry_min_exhaustion_volume_dominance_ratio": args.entry_min_exhaustion_volume_dominance_ratio,
        "entry_btc_96_max_abs_net_move_pct": args.entry_btc_96_max_abs_net_move_pct,
        "entry_btc_384_min_directional_net_move_pct": args.entry_btc_384_min_directional_net_move_pct,
        "entry_btc_require_current_directional_candle": args.entry_btc_require_current_directional_candle,
        "entry_opposite_duration_min_r_squared": OPPOSITE_DURATION_MIN_R_SQUARED,
        "volume_atr_take_profit": args.volume_atr_take_profit,
        "volume_atr_target_scale": args.volume_atr_target_scale,
        "volume_atr_min_target_r": args.volume_atr_min_target_r,
        "volume_atr_max_target_r": args.volume_atr_max_target_r,
        "entry_defer_bearish_continuation": args.entry_defer_bearish_continuation,
        "entry_defer_bullish_continuation": args.entry_defer_bullish_continuation,
        "entry_require_opposite_reversal_confirmation": args.entry_require_opposite_reversal_confirmation,
        "entry_require_reversal_average_reclaim": args.entry_require_reversal_average_reclaim,
        "entry_defer_max_wait_candles": args.entry_defer_max_wait_candles,
        "entry_symbol_cooldown_candles": args.entry_symbol_cooldown_candles,
        "entry_max_signal_pullback_pct": args.entry_max_signal_pullback_pct,
        "entry_max_gap_without_retest_pct": args.entry_max_gap_without_retest_pct,
        "entry_retest_tolerance_pct": args.entry_retest_tolerance_pct,
        "entry_retest_after_signal": args.entry_retest_after_signal,
        "entry_retest_max_wait_candles": args.entry_retest_max_wait_candles,
        "entry_retest_min_entry_open_gap_pct": args.entry_retest_min_entry_open_gap_pct,
        "entry_retest_open_fade_min_volume_ratio": args.entry_retest_open_fade_min_volume_ratio,
        "fvg_impulse_retrace_fill_pct": args.fvg_impulse_retrace_fill_pct,
        "fvg_impulse_retrace_min_wait_candles": args.fvg_impulse_retrace_min_wait_candles,
        "trend_timeframe": args.trend_timeframe.label(),
        "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
        "min_delta_rank": args.min_delta_rank,
        "max_delta_rank": args.max_delta_rank,
        "min_price_change_pct": args.min_price_change_pct,
        "entry_trigger_allowlist": &args.entry_trigger_allowlist,
        "entry_trigger_blocklist": &args.entry_trigger_blocklist,
        "symbol_blocklist": &args.symbol_blocklist,
    })
}

/// 构造 `back_test_log.risk_config`，显式区分旧默认费率与 v5+ 成本假设。
pub fn market_velocity_risk_config_detail(
    args: &MarketVelocityEventBacktestArgs,
    target_r: f64,
) -> Value {
    json!({
        "mode": "symbol_isolated_100u",
        "trade_direction": args.trade_direction.label(),
        "stop_loss_pct": args.stop_loss_pct,
        "take_profit_mode": if args.volume_atr_take_profit { "volume_atr" } else { "fixed_r" },
        "target_r": if args.volume_atr_take_profit { Value::Null } else { json!(target_r) },
        "target_r_placeholder": if args.volume_atr_take_profit { json!(target_r) } else { Value::Null },
        "volume_atr_target_scale": args.volume_atr_target_scale,
        "volume_atr_min_target_r": args.volume_atr_min_target_r,
        "volume_atr_max_target_r": args.volume_atr_max_target_r,
        "fee_bps_per_side": args.backtest_fee_bps_per_side,
        "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
        "profit_protect_after_r": args.profit_protect_after_r,
        "profit_protect_stop_r": args.profit_protect_stop_r,
        "runner_target_r": args.runner_target_r,
        "runner_fraction": args.runner_fraction,
        "runner_stop_r": args.runner_stop_r,
        "early_exit_no_profit_candles": args.early_exit_no_profit_candles,
        "stop_reentry_mode": args.stop_reentry_mode.label(),
        "fvg_entry_mode": args.fvg_entry_mode.label(),
        "fvg_lookback_candles": args.fvg_lookback_candles,
        "fvg_max_wait_candles": args.fvg_max_wait_candles,
    })
}

/// 构造回测明细的信号快照，保留成交量门禁、目标策略和成本口径。
pub(super) fn market_velocity_detail_signal_value(
    trade: &FrameworkEquityTradeReport,
    args: &MarketVelocityEventBacktestArgs,
) -> Value {
    json!({
        "source": "market_velocity_framework_replay",
        "rank_event_id": trade.event_id,
        "setup_ts": trade.signal_ts,
        "detected_at": &trade.detected_at,
        "entry_ts": trade.entry_ts,
        "deferred_wait_candles": if trade.trigger.contains("deferred_") {
            trade.entry_ts.saturating_sub(trade.signal_ts) / MS_15M
        } else {
            0
        },
        "entry_trigger": &trade.trigger,
        "trade_direction": if trade.price_change_pct < 0.0 { "short" } else { "long" },
        "new_rank": trade.new_rank,
        "delta_rank": trade.delta_rank,
        "price_change_pct": trade.price_change_pct,
        "target_r": trade.target_r,
        "take_profit_mode": if args.volume_atr_take_profit { "volume_atr" } else { "fixed_r" },
        "volume_atr_target_scale": args.volume_atr_target_scale,
        "volume_atr_min_target_r": args.volume_atr_min_target_r,
        "volume_atr_max_target_r": args.volume_atr_max_target_r,
        "fee_bps_per_side": args.backtest_fee_bps_per_side,
        "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
        "entry_defer_bearish_continuation": args.entry_defer_bearish_continuation,
        "entry_defer_bullish_continuation": args.entry_defer_bullish_continuation,
        "entry_require_opposite_reversal_confirmation": args.entry_require_opposite_reversal_confirmation,
        "entry_require_reversal_average_reclaim": args.entry_require_reversal_average_reclaim,
        "entry_defer_max_wait_candles": args.entry_defer_max_wait_candles,
        "entry_min_opposite_duration_candles": args.entry_min_opposite_duration_candles,
        "entry_opposite_duration_min_r_squared": OPPOSITE_DURATION_MIN_R_SQUARED,
        "entry_min_exhaustion_volume_dominance_ratio": args.entry_min_exhaustion_volume_dominance_ratio,
        "entry_btc_96_max_abs_net_move_pct": args.entry_btc_96_max_abs_net_move_pct,
        "entry_btc_384_min_directional_net_move_pct": args.entry_btc_384_min_directional_net_move_pct,
        "entry_btc_require_current_directional_candle": args.entry_btc_require_current_directional_candle,
        "entry_exhaustion_volume_lookback_candles": EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
        "entry_exhaustion_current_cluster_candles": EXHAUSTION_CURRENT_CLUSTER_CANDLES,
        "entry_exhaustion_swing_radius_candles": EXHAUSTION_SWING_RADIUS_CANDLES,
        "stop_loss_pct": args.stop_loss_pct,
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "event_source": match args.event_source {
            MarketVelocityEventSource::Episodes => "episodes",
            MarketVelocityEventSource::RawEvents => "raw_events",
            MarketVelocityEventSource::RawState => "raw_state",
            MarketVelocityEventSource::Kline15m => "kline_15m",
        },
        "kline_volume_rank_velocity": args.kline_volume_rank_velocity,
        "kline_volume_rank_require_turnover_growth": args.kline_volume_rank_require_turnover_growth,
        "kline_volume_rank_require_consecutive_improvement": args.kline_volume_rank_require_consecutive_improvement,
        "kline_volume_rank_lookback_candles": if args.kline_volume_rank_velocity { json!(96) } else { Value::Null },
    })
}

/// 为分腿平仓补充退出信息，不改变开仓时冻结的策略参数。
pub(super) fn market_velocity_detail_signal_value_for_leg(
    trade: &FrameworkEquityTradeReport,
    args: &MarketVelocityEventBacktestArgs,
    leg: &FrameworkEquityCloseLegReport,
) -> Value {
    let mut value = market_velocity_detail_signal_value(trade, args);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "exit_reason".to_string(),
            Value::String(leg.exit_reason.clone()),
        );
        object.insert(
            "runner_target_r".to_string(),
            args.runner_target_r.map_or(Value::Null, Value::from),
        );
        object.insert(
            "runner_fraction".to_string(),
            Value::from(args.runner_fraction),
        );
        object.insert("runner_stop_r".to_string(), Value::from(args.runner_stop_r));
        object.insert("leg_result_r".to_string(), Value::from(leg.result_r));
        object.insert("leg_full_close".to_string(), Value::from(leg.full_close));
    }
    value
}
