use super::{
    profit_observation::{
        MINIMUM_LOCK_R, OBSERVATION_TRIGGER_R, POST_ONE_TRIGGER_R, PRE_ONE_EXIT_CLOSE_R,
        TARGET_COMPLETION_OFFSET,
    },
    FrameworkEquityCloseLegReport, FrameworkEquityTradeReport, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection,
};
use crate::app::market_velocity_event_backtest::directional_reversal::{
    EXHAUSTION_CURRENT_CLUSTER_CANDLES, EXHAUSTION_SWING_RADIUS_CANDLES,
    EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::{
    DIVERGENCE_LOOKBACK_CANDLES, DIVERGENCE_PIVOT_WING_CANDLES, FILTERED_VOLUME_ATR_STOP_SOURCE,
    FILTERED_VOLUME_HISTORY_CANDLES, FILTERED_VOLUME_MIN_RATIO,
    FILTERED_VOLUME_MIN_RETAINED_CANDLES, FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
    FILTERED_VOLUME_V9_MIN_RATIO,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v1::{
    MOMENTUM_EXHAUSTION_HYPOTHESIS, MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES,
    MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v2::{
    MOMENTUM_EXHAUSTION_LIMIT_VALID_CANDLES, MOMENTUM_EXHAUSTION_V2_HYPOTHESIS,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v3::{
    MOMENTUM_EXHAUSTION_V3_HYPOTHESIS, MOMENTUM_EXHAUSTION_V3_WICK_MIN_RANGE_RATIO,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::volume_anchor_rsi_divergence_reversal_v1::{
    ISOLATED_RSI_COMPARISON_MODE, VOLUME_ANCHOR_RSI_HYPOTHESIS,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::volume_anchor_rsi_divergence_reversal_v2::{
    BEARISH_ANCHOR_RESET_RSI, BULLISH_ANCHOR_RESET_RSI, ISOLATED_RSI_V2_COMPARISON_MODE,
    MIN_INTERVENING_CANDLES, VOLUME_ANCHOR_RSI_V2_HYPOTHESIS,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::volume_platform_break_trend_v1::{
    PLATFORM_BREAK_TREND_HYPOTHESIS, PLATFORM_CONFIRMATION_CANDLES,
    PLATFORM_LOOKBACK_CANDLES, PLATFORM_MAX_RANGE_ATR, PLATFORM_MIN_BODY_OPEN_RATIO,
    PLATFORM_MIN_BODY_RANGE_RATIO,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::volume_platform_break_trend_v2::{
    PLATFORM_BREAK_TREND_V2_HYPOTHESIS, PLATFORM_V2_MAX_CENTER_SHIFT_ATR,
    PLATFORM_V2_MAX_FITTED_DRIFT_ATR, PLATFORM_V2_MIN_TOUCH_SEPARATION_CANDLES,
    PLATFORM_V2_TOUCH_ZONE_WIDTH_RATIO, PLATFORM_V2_TREND_R_SQUARED_MIN,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::weekly_base_volume_bollinger_conflict_v4::{
    BOLLINGER_CONFLICT_PERIOD, BOLLINGER_CONFLICT_STDDEV_MULTIPLIER,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::weekly_base_volume_ema144_proximity_v5::EMA144_MAX_DISTANCE_ATR;
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::weekly_base_volume_v3::{
    WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES, WEEKLY_VOLUME_CCY_P90_INDEX,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::weekly_p90_anchor_rsi_trend_managed_v12::{
    COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES, COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT,
    COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO,
    COUNTERTREND_TARGET_ATR_MULTIPLIER as V12_COUNTERTREND_TARGET_ATR_MULTIPLIER,
    TREND_PLATFORM_CONFIRMATION_CANDLES, TREND_PLATFORM_LOOKBACK_CANDLES,
    TREND_PLATFORM_MAX_RANGE_ATR, TREND_PLATFORM_MIN_BODY_OPEN_RATIO,
    TREND_PLATFORM_MIN_BODY_RANGE_RATIO,
};
use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::weekly_p90_anchor_rsi_trend_managed_counter15_v13::COUNTERTREND_TARGET_ATR_MULTIPLIER as V13_COUNTERTREND_TARGET_ATR_MULTIPLIER;
use crate::app::market_velocity_event_backtest::rsi_volume_regime::{
    RSI_VOLUME_V3_ATR_STOP_SOURCE, RSI_VOLUME_V5_LOOKBACK_CANDLES, RSI_VOLUME_V5_MIN_RATIO,
};
use crate::app::market_velocity_event_backtest::{
    filtered_volume_weekly_base_preset, is_filtered_volume_weekly_base_version,
    is_isolated_entry_family_version, isolated_entry_family_strategy_key,
    uses_anchor_next_close_confirmation, uses_filtered_volume_2p5, uses_neutral_rsi_lower_wick_long,
    uses_anchor_wick_or_next_touch_entry, uses_momentum_exhaustion_volume_tier_exit,
    uses_target_completion_profit_observation,
    uses_trend_managed_volume_trailing_exit, uses_weekly_p90_anchor_rsi_divergence,
    MarketVelocityEventSource,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_DIRECT_KLINE_V36_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_DIRECT_KLINE_V36_STRATEGY_KEY, MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY,
    MARKET_RSI_VOLUME_REGIME_V1_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION, MS_15M,
};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime};
use rust_quant_domain::entities::BacktestDetail;
use serde_json::{json, Value};

/// 返回持久化到 back_test_log/detail 的稳定策略类型。
pub fn market_velocity_strategy_type(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    if let Some(strategy_key) =
        isolated_entry_family_strategy_key(&args.paper_outcome_entry_rule_version)
    {
        return strategy_key;
    }
    if matches!(
        args.paper_outcome_entry_rule_version.as_str(),
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
    ) {
        return MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_STRATEGY_KEY;
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
    {
        return MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY;
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
    {
        return MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY;
    }
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION
    {
        return MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY;
    }
    if is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version) {
        return MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY;
    }
    if matches!(
        args.paper_outcome_entry_rule_version.as_str(),
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION
    ) {
        return MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY;
    }
    if args.paper_outcome_entry_rule_version == MARKET_MOMENTUM_DIRECT_KLINE_V36_ENTRY_RULE_VERSION
    {
        return MARKET_MOMENTUM_DIRECT_KLINE_V36_STRATEGY_KEY;
    }
    if matches!(
        args.paper_outcome_entry_rule_version.as_str(),
        MARKET_RSI_VOLUME_REGIME_V1_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION
    ) {
        return MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY;
    }
    match args.event_source {
        MarketVelocityEventSource::Episodes => "market_velocity_episode",
        MarketVelocityEventSource::RawEvents => "market_velocity_raw_events",
        MarketVelocityEventSource::RawState => "market_velocity_raw_state",
        MarketVelocityEventSource::Kline15m => "market_velocity_kline_15m",
    }
}

/// 构造 `back_test_log.strategy_detail`，让策略版本的入场参数可以独立复现。
pub fn market_velocity_strategy_detail(args: &MarketVelocityEventBacktestArgs) -> Value {
    if is_isolated_entry_family_version(&args.paper_outcome_entry_rule_version) {
        return market_velocity_isolated_strategy_detail(args);
    }
    let is_direct_kline_v36 = args.paper_outcome_entry_rule_version
        == MARKET_MOMENTUM_DIRECT_KLINE_V36_ENTRY_RULE_VERSION;
    let is_filtered_volume_weekly_base =
        is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version);
    let is_filtered_volume_rsi_ema_macd = is_filtered_volume_weekly_base
        || matches!(
            args.paper_outcome_entry_rule_version.as_str(),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION
                | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION
        );
    let is_filtered_volume_rsi_ema_macd_v2_plus = is_filtered_volume_weekly_base
        || args.paper_outcome_entry_rule_version
            == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION;
    let is_filtered_volume_rsi_ema_macd_v4 = args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION;
    let is_filtered_volume_rsi_ema_macd_v5 = args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION;
    let uses_weekly_p90_anchor_rsi_divergence =
        uses_weekly_p90_anchor_rsi_divergence(&args.paper_outcome_entry_rule_version);
    let uses_anchor_next_close_confirmation =
        uses_anchor_next_close_confirmation(&args.paper_outcome_entry_rule_version);
    let uses_anchor_wick_or_next_touch_entry =
        uses_anchor_wick_or_next_touch_entry(&args.paper_outcome_entry_rule_version);
    let uses_trend_managed_volume_trailing_exit =
        uses_trend_managed_volume_trailing_exit(&args.paper_outcome_entry_rule_version);
    let countertrend_default_target_atr = if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
    {
        V13_COUNTERTREND_TARGET_ATR_MULTIPLIER
    } else {
        V12_COUNTERTREND_TARGET_ATR_MULTIPLIER
    };
    let is_rsi_volume_regime = matches!(
        args.paper_outcome_entry_rule_version.as_str(),
        MARKET_RSI_VOLUME_REGIME_V1_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION
    );
    let is_filtered_volume_v5 =
        args.paper_outcome_entry_rule_version == MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION;
    let has_sideways_breakout = is_rsi_volume_regime
        && !matches!(
            args.paper_outcome_entry_rule_version.as_str(),
            MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION
                | MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION
        );
    let is_research_locked =
        is_direct_kline_v36 || is_rsi_volume_regime || is_filtered_volume_rsi_ema_macd;
    let strategy_preset = if args.paper_strategy_preset.is_empty() {
        filtered_volume_weekly_base_preset(&args.paper_outcome_entry_rule_version)
            .unwrap_or(args.paper_strategy_preset.as_str())
    } else {
        args.paper_strategy_preset.as_str()
    };
    let mut detail = json!({
        "source": "market_velocity_event_backtest",
        "strategy_key": market_velocity_strategy_type(args),
        "strategy_preset": strategy_preset,
        "version_status": if is_direct_kline_v36 { json!("frozen_research_rejected") } else if is_rsi_volume_regime || is_filtered_volume_rsi_ema_macd { json!("research_unvalidated") } else { Value::Null },
        "promotion_eligible": if is_research_locked { json!(false) } else { Value::Null },
        "paper_live_eligible": if is_research_locked { json!(false) } else { Value::Null },
        "event_source": match args.event_source {
            MarketVelocityEventSource::Episodes => "episodes",
            MarketVelocityEventSource::RawEvents => "raw_events",
            MarketVelocityEventSource::RawState => "raw_state",
            MarketVelocityEventSource::Kline15m => "kline_15m",
        },
        "kline_volume_rank_velocity": args.kline_volume_rank_velocity,
        "kline_volume_rank_require_turnover_growth": args.kline_volume_rank_require_turnover_growth,
        "kline_volume_rank_require_consecutive_improvement": args.kline_volume_rank_require_consecutive_improvement,
        "kline_current_live_only": args.kline_current_live_only,
        "sample_limit": args.sample_limit,
        "sample_seed": &args.sample_seed,
        "event_start_ms": args.event_start_ms,
        "event_end_ms": args.event_end_ms,
        "kline_volume_rank_lookback_candles": if args.kline_volume_rank_velocity { json!(96) } else { Value::Null },
        "kline_volume_rank_quote_turnover": if args.kline_volume_rank_velocity { "vol_ccy_x_close" } else { "off" },
        "trade_direction": args.trade_direction.label(),
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "entry_period": args.entry_period,
        "entry_max_distance_pct": args.entry_max_distance_pct,
        "entry_min_volume_ratio": args.entry_min_volume_ratio,
        "entry_volume_baseline_mode": if is_filtered_volume_v5 { json!("causal_previous_10_excluding_marked_spikes") } else { Value::Null },
        "entry_volume_history_lookback_candles": if is_filtered_volume_v5 { json!(RSI_VOLUME_V5_LOOKBACK_CANDLES) } else { Value::Null },
        "entry_volume_historical_spike_min_ratio": if is_filtered_volume_v5 { json!(RSI_VOLUME_V5_MIN_RATIO) } else { Value::Null },
        "entry_volume_current_candle_in_average": if is_filtered_volume_v5 { json!(false) } else { Value::Null },
        "entry_min_rsi": args.entry_min_rsi,
        "entry_max_rsi": args.entry_max_rsi,
        "entry_min_rsi_delta": args.entry_min_rsi_delta,
        "entry_rsi_delta_lookback_candles": args.entry_rsi_delta_lookback_candles,
        "entry_bollinger_breakout": args.entry_bollinger_breakout,
        "entry_min_bollinger_bandwidth_expansion_pct": args.entry_min_bollinger_bandwidth_expansion_pct,
        "entry_min_range_expansion_ratio": args.entry_min_range_expansion_ratio,
        "entry_extreme_volume_contrarian": args.entry_extreme_volume_contrarian,
        "entry_extreme_volume_continuation": args.entry_extreme_volume_continuation,
        "entry_rsi_volume_regime": args.entry_rsi_volume_regime,
        "entry_sideways_breakout_enabled": if is_rsi_volume_regime { json!(has_sideways_breakout) } else { Value::Null },
        "entry_relative_volume_at_time_10d": args.entry_relative_volume_at_time_10d,
        "entry_min_recent_drawdown_pct": args.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": args.entry_recent_drawdown_lookback_candles,
        "entry_opposite_move_lookback_candles": args.entry_opposite_move_lookback_candles,
        "entry_min_opposite_net_move_pct": args.entry_min_opposite_net_move_pct,
        "entry_min_opposite_duration_candles": args.entry_min_opposite_duration_candles,
        "entry_opposite_duration_min_r_squared": args.entry_opposite_duration_min_r_squared,
        "entry_min_exhaustion_volume_dominance_ratio": args.entry_min_exhaustion_volume_dominance_ratio,
        "entry_btc_96_max_abs_net_move_pct": args.entry_btc_96_max_abs_net_move_pct,
        "entry_btc_384_min_directional_net_move_pct": args.entry_btc_384_min_directional_net_move_pct,
        "entry_btc_require_current_directional_candle": args.entry_btc_require_current_directional_candle,
        "volume_atr_take_profit": args.volume_atr_take_profit,
        "volume_atr_target_scale": args.volume_atr_target_scale,
        "volume_atr_min_target_r": args.volume_atr_min_target_r,
        "volume_atr_max_target_r": args.volume_atr_max_target_r,
        "entry_defer_bearish_continuation": args.entry_defer_bearish_continuation,
        "entry_defer_bullish_continuation": args.entry_defer_bullish_continuation,
        "entry_defer_long_lower_wick_reversal": args.entry_defer_long_lower_wick_reversal,
        "entry_long_bullish_hammer_reversal": args.entry_long_bullish_hammer_reversal,
        "entry_require_two_stage_recovery": args.entry_require_two_stage_recovery,
        "entry_require_macd_negative_histogram_improving": args.entry_require_macd_negative_histogram_improving,
        "entry_require_opposite_reversal_confirmation": args.entry_require_opposite_reversal_confirmation,
        "entry_require_reversal_average_reclaim": args.entry_require_reversal_average_reclaim,
        "entry_defer_max_wait_candles": args.entry_defer_max_wait_candles,
        "entry_symbol_cooldown_candles": args.entry_symbol_cooldown_candles,
        "entry_once_per_opposite_trend_state": args.entry_once_per_opposite_trend_state,
        "entry_once_per_historical_trend_state": args.entry_once_per_historical_trend_state,
        "entry_wait_setup_open_reclaim": args.entry_wait_setup_open_reclaim,
        "entry_opposite_trend_reset_confirm_candles": args.entry_opposite_trend_reset_confirm_candles,
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
    });
    if let Some(object) = detail.as_object_mut() {
        object.insert(
            "entry_filtered_volume_rsi_ema_macd".to_string(),
            Value::from(args.entry_filtered_volume_rsi_ema_macd),
        );
        if is_filtered_volume_rsi_ema_macd {
            object.insert(
                "entry_filtered_volume_history_candles".to_string(),
                Value::from(FILTERED_VOLUME_HISTORY_CANDLES),
            );
            object.insert(
                "entry_filtered_volume_spike_ratio".to_string(),
                Value::from(if uses_weekly_p90_anchor_rsi_divergence {
                    FILTERED_VOLUME_V9_MIN_RATIO
                } else {
                    FILTERED_VOLUME_MIN_RATIO
                }),
            );
            object.insert(
                "entry_filtered_volume_min_retained_candles".to_string(),
                Value::from(FILTERED_VOLUME_MIN_RETAINED_CANDLES),
            );
            object.insert(
                "entry_divergence_lookback_candles".to_string(),
                Value::from(DIVERGENCE_LOOKBACK_CANDLES),
            );
            object.insert(
                "entry_divergence_pivot_wing_candles".to_string(),
                if uses_weekly_p90_anchor_rsi_divergence {
                    Value::Null
                } else {
                    Value::from(DIVERGENCE_PIVOT_WING_CANDLES)
                },
            );
            object.insert(
                "entry_rsi_divergence_mode".to_string(),
                json!(if uses_anchor_wick_or_next_touch_entry {
                    "nearest_directional_weekly_p90_filtered_volume_anchor_wick_or_next_touch"
                } else if uses_anchor_next_close_confirmation {
                    "nearest_directional_weekly_p90_filtered_volume_anchor_next_close_confirmed"
                } else if uses_weekly_p90_anchor_rsi_divergence {
                    "nearest_directional_weekly_p90_filtered_volume_anchor"
                } else {
                    "confirmed_strict_price_pivots"
                }),
            );
            object.insert(
                "entry_rsi_divergence_rsi_min_delta".to_string(),
                if uses_weekly_p90_anchor_rsi_divergence {
                    json!(0.0)
                } else {
                    json!(1.0)
                },
            );
            object.insert(
                "entry_indicator_branches".to_string(),
                json!(["rsi", "ema", "macd_dif"]),
            );
            object.insert(
                "entry_excluded_legacy_logic".to_string(),
                if is_filtered_volume_rsi_ema_macd_v4 {
                    json!(["96_bar_move", "bollinger_breakout", "bos", "fvg", "choch"])
                } else {
                    json!(["96_bar_move", "bollinger", "bos", "fvg", "choch"])
                },
            );
            if is_filtered_volume_rsi_ema_macd_v2_plus {
                object.insert(
                    "entry_filtered_volume_macd_zero_band_atr_multiplier".to_string(),
                    json!(args.entry_filtered_volume_macd_zero_band_atr_multiplier),
                );
                object.insert(
                    "entry_filtered_volume_macd_min_normalized_dif_improvement".to_string(),
                    json!(args.entry_filtered_volume_macd_min_normalized_dif_improvement),
                );
                object.insert(
                    "entry_filtered_volume_macd_pivot_candidate_offset_candles".to_string(),
                    json!(DIVERGENCE_PIVOT_WING_CANDLES),
                );
                object.insert(
                    "entry_filtered_volume_macd_pivot_comparison".to_string(),
                    json!("strict"),
                );
                object.insert(
                    "entry_filtered_volume_macd_unconfigured_policy".to_string(),
                    json!("disable_macd_branch"),
                );
            }
            if is_filtered_volume_weekly_base {
                object.insert(
                    "entry_weekly_volume_ccy_source".to_string(),
                    json!("per_symbol_15m_table.vol_ccy"),
                );
                object.insert(
                    "entry_weekly_volume_ccy_lookback_candles".to_string(),
                    json!(WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES),
                );
                object.insert(
                    "entry_weekly_volume_ccy_p90_zero_based_index".to_string(),
                    json!(WEEKLY_VOLUME_CCY_P90_INDEX),
                );
                object.insert(
                    "entry_weekly_volume_ccy_requires_continuous_values".to_string(),
                    json!(true),
                );
                object.insert(
                    "entry_rsi_divergence_anchor_gate".to_string(),
                    if uses_anchor_wick_or_next_touch_entry {
                        json!({
                            "lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                            "nearest_qualified_anchor_only": true,
                            "filtered_volume_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                            "weekly_volume_ccy_gate": "anchor_own_previous_672_nearest_rank_p90",
                            "long_anchor_rsi_max_inclusive": 30.0,
                            "short_anchor_rsi_min_inclusive": 70.0,
                            "current_rsi_equal_to_anchor_allowed": true,
                            "right_confirmation_candles": 0,
                            "directional_wick_min_full_range_ratio": 0.60,
                            "doji_max_body_range_ratio": 0.10,
                            "directional_wick_entry": "immediate_next_15m_open",
                            "non_wick_entry": "immediate_next_candle_intrabar_strict_break_of_p_high_or_p_low",
                            "setup_expiry_candles": 1,
                            "ohlc_intrabar_path_policy": "full_15m_bar_conservative_stop_first",
                        })
                    } else if uses_anchor_next_close_confirmation {
                        json!({
                            "lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                            "nearest_qualified_anchor_only": true,
                            "filtered_volume_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                            "weekly_volume_ccy_gate": "anchor_own_previous_672_nearest_rank_p90",
                            "long_anchor_rsi_max_inclusive": 30.0,
                            "short_anchor_rsi_min_inclusive": 70.0,
                            "current_rsi_equal_to_anchor_allowed": true,
                            "right_confirmation_candles": 0,
                            "entry_confirmation_candles": 1,
                            "entry_confirmation_rule": "long_close_above_p_high_short_close_below_p_low",
                            "setup_expiry_candles": 1,
                            "anchor_required_for_entry": true,
                        })
                    } else if uses_weekly_p90_anchor_rsi_divergence {
                        json!({
                            "lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                            "nearest_qualified_anchor_only": true,
                            "filtered_volume_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                            "weekly_volume_ccy_gate": "anchor_own_previous_672_nearest_rank_p90",
                            "long_anchor_rsi_max_inclusive": 30.0,
                            "short_anchor_rsi_min_inclusive": 70.0,
                            "current_rsi_equal_to_anchor_allowed": true,
                            "right_confirmation_candles": 0,
                        })
                    } else {
                        Value::Null
                    },
                );
                object.insert(
                    "entry_fill_mode".to_string(),
                    json!(if uses_anchor_wick_or_next_touch_entry {
                        "pivot_directional_wick_next_open_else_immediate_next_candle_intrabar_break"
                    } else if uses_anchor_next_close_confirmation {
                        "next_open_after_one_completed_confirmation_candle"
                    } else {
                        "next_15m_open"
                    }),
                );
                object.insert(
                    "entry_stop_loss_mode".to_string(),
                    json!(if uses_anchor_wick_or_next_touch_entry {
                        "actual_fill_atr14_x_1_5"
                    } else {
                        "rsi_pattern_structure_else_actual_fill_atr14_x_1_5"
                    }),
                );
                object.insert(
                    "entry_take_profit_mode".to_string(),
                    json!(if uses_trend_managed_volume_trailing_exit {
                        "trend_relation_then_fixed_atr_distance"
                    } else {
                        "fixed_atr_distance_by_filtered_volume_ratio"
                    }),
                );
                object.insert(
                    "trend_managed_exit".to_string(),
                    if uses_trend_managed_volume_trailing_exit {
                        json!({
                            "frozen_at": "anchor_p_completed_close",
                            "long_term_ema_order": "ema12_ema144_ema169_ema696",
                            "long_term_confirmation_candles": 3,
                            "ema696_each_candle_must_move_in_trend_direction": true,
                            "platform_lookback_candles": TREND_PLATFORM_LOOKBACK_CANDLES,
                            "platform_max_range_atr": TREND_PLATFORM_MAX_RANGE_ATR,
                            "platform_confirmation_candles": TREND_PLATFORM_CONFIRMATION_CANDLES,
                            "platform_min_body_range_ratio": TREND_PLATFORM_MIN_BODY_RANGE_RATIO,
                            "platform_min_body_open_ratio": TREND_PLATFORM_MIN_BODY_OPEN_RATIO,
                            "countertrend_target_atr": countertrend_default_target_atr,
                            "countertrend_exception_min_filtered_volume_ratio": COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO,
                            "countertrend_exception_lookback_candles_excluding_p": COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES,
                            "countertrend_exception_min_directional_net_move_pct": COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT,
                            "holding_volume_trailing_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                            "holding_volume_weekly_p90_required": false,
                            "new_stop_effective_from_next_candle": true,
                        })
                    } else {
                        Value::Null
                    },
                );
                object.insert(
                    "entry_max_holding_time".to_string(),
                    args.equity_max_holding_hours
                        .map_or(Value::Null, Value::from),
                );
                object.insert(
                    "entry_bollinger_conflict_buffer".to_string(),
                    if is_filtered_volume_rsi_ema_macd_v4 {
                        json!({
                            "period": BOLLINGER_CONFLICT_PERIOD,
                            "standard_deviation_multiplier": BOLLINGER_CONFLICT_STDDEV_MULTIPLIER,
                            "lower_touch_counters_existing_short": true,
                            "upper_touch_counters_existing_long": true,
                            "standalone_entry_allowed": false,
                        })
                    } else {
                        Value::Null
                    },
                );
                object.insert(
                    "entry_ema144_proximity_gate".to_string(),
                    if is_filtered_volume_rsi_ema_macd_v5 {
                        json!({
                            "price_source": "completed_signal_candle_close",
                            "normalizer": "signal_candle_atr14",
                            "maximum_distance_atr_inclusive": EMA144_MAX_DISTANCE_ATR,
                            "scope": "ema_continuation_branch_only",
                        })
                    } else {
                        Value::Null
                    },
                );
                object.insert(
                    "entry_neutral_rsi_lower_wick_long".to_string(),
                    json!(uses_neutral_rsi_lower_wick_long(
                        &args.paper_outcome_entry_rule_version
                    )),
                );
            }
        }
    }
    detail
}

/// 为三个独立入场家族生成紧凑且互斥的策略快照，避免旧混合分支出现在审计 JSON。
fn market_velocity_isolated_strategy_detail(args: &MarketVelocityEventBacktestArgs) -> Value {
    let (family, hypothesis, entry_rule, entry_fill) = match args
        .paper_outcome_entry_rule_version
        .as_str()
    {
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION => (
            "momentum_exhaustion_reversal",
            MOMENTUM_EXHAUSTION_HYPOTHESIS,
            json!({
                "prior_net_move_lookback_candles_excluding_signal": MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES,
                "minimum_absolute_net_move_pct": MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
                "decline_creates": "long",
                "rise_creates": "short",
                "rsi_used": false,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
            }),
            "directional_wick_next_open_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION => (
            "momentum_exhaustion_reversal",
            MOMENTUM_EXHAUSTION_V2_HYPOTHESIS,
            json!({
                "prior_net_move_lookback_candles_excluding_signal": MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES,
                "minimum_absolute_net_move_pct": MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
                "decline_creates": "long",
                "rise_creates": "short",
                "directional_wick_limit_valid_candles": MOMENTUM_EXHAUSTION_LIMIT_VALID_CANDLES,
                "same_symbol_pending_policy": "newest_valid_p_replaces_older_unfilled_setup",
                "non_directional_wick_entry": "immediate_next_candle_extreme_break",
                "rsi_used": false,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
            }),
            "directional_wick_limit_at_p_extreme_12_candles_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION => (
            "momentum_exhaustion_reversal",
            MOMENTUM_EXHAUSTION_V3_HYPOTHESIS,
            json!({
                "prior_net_move_lookback_candles_excluding_signal": MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES,
                "minimum_absolute_net_move_pct": MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
                "decline_creates": "long",
                "rise_creates": "short",
                "directional_wick_min_range_ratio": MOMENTUM_EXHAUSTION_V3_WICK_MIN_RANGE_RATIO,
                "directional_wick_limit_valid_candles": MOMENTUM_EXHAUSTION_LIMIT_VALID_CANDLES,
                "same_symbol_pending_policy": "newest_valid_p_replaces_older_unfilled_setup",
                "non_directional_wick_entry": "immediate_next_candle_extreme_break",
                "rsi_used": false,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
            }),
            "directional_wick55_limit_at_p_extreme_12_candles_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION => (
            "volume_anchor_rsi_divergence",
            VOLUME_ANCHOR_RSI_HYPOTHESIS,
            json!({
                "comparison_mode": ISOLATED_RSI_COMPARISON_MODE,
                "anchor_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                "nearest_qualified_anchor_only": true,
                "long_rsi_max_inclusive": 30.0,
                "short_rsi_min_inclusive": 70.0,
                "equal_rsi_allowed": true,
                "historical_net_move_used": false,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
            }),
            "directional_wick_next_open_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION => (
            "volume_anchor_rsi_divergence",
            VOLUME_ANCHOR_RSI_V2_HYPOTHESIS,
            json!({
                "comparison_mode": ISOLATED_RSI_V2_COMPARISON_MODE,
                "anchor_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                "nearest_qualified_anchor_only": true,
                "fallback_to_older_anchor": false,
                "minimum_strictly_intervening_candles": MIN_INTERVENING_CANDLES,
                "long_rsi_max_inclusive": 30.0,
                "short_rsi_min_inclusive": 70.0,
                "bullish_anchor_invalid_if_intermediate_rsi_strictly_above": BULLISH_ANCHOR_RESET_RSI,
                "bearish_anchor_invalid_if_intermediate_rsi_strictly_below": BEARISH_ANCHOR_RESET_RSI,
                "intermediate_rsi_missing_policy": "fail_closed",
                "equal_rsi_allowed": true,
                "historical_net_move_used": false,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
            }),
            "directional_wick_next_open_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION => (
            "volume_platform_break_trend",
            PLATFORM_BREAK_TREND_HYPOTHESIS,
            json!({
                "platform_lookback_candles": PLATFORM_LOOKBACK_CANDLES,
                "platform_max_range_atr": PLATFORM_MAX_RANGE_ATR,
                "minimum_break_body_range_ratio": PLATFORM_MIN_BODY_RANGE_RATIO,
                "minimum_break_body_open_ratio": PLATFORM_MIN_BODY_OPEN_RATIO,
                "acceptance_confirmation_candles": PLATFORM_CONFIRMATION_CANDLES,
                "ema_order_confirmation_candles": 3,
                "ema_order": "ema12_ema144_ema169_ema696",
                "ema696_slope_changes_required": 3,
                "rsi_used": false,
                "macd_used": false,
                "historical_net_move_used": false,
                "reversal_wick_used": false,
            }),
            "next_15m_open_after_second_acceptance_close",
        ),
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION => (
            "volume_platform_break_trend",
            PLATFORM_BREAK_TREND_V2_HYPOTHESIS,
            json!({
                "platform_lookback_candles": PLATFORM_LOOKBACK_CANDLES,
                "platform_width_atr_source": "candle_immediately_before_break",
                "platform_max_range_atr": PLATFORM_MAX_RANGE_ATR,
                "first5_last5_close_mean_max_shift_atr": PLATFORM_V2_MAX_CENTER_SHIFT_ATR,
                "trend_rejection_r_squared_min": PLATFORM_V2_TREND_R_SQUARED_MIN,
                "trend_rejection_fitted_drift_atr_strictly_above": PLATFORM_V2_MAX_FITTED_DRIFT_ATR,
                "touch_zone_width_ratio": PLATFORM_V2_TOUCH_ZONE_WIDTH_RATIO,
                "minimum_touches_per_side": 2,
                "minimum_same_side_touch_separation_candles": PLATFORM_V2_MIN_TOUCH_SEPARATION_CANDLES,
                "minimum_break_body_range_ratio": PLATFORM_MIN_BODY_RANGE_RATIO,
                "minimum_break_body_open_ratio": PLATFORM_MIN_BODY_OPEN_RATIO,
                "acceptance_confirmation_candles": PLATFORM_CONFIRMATION_CANDLES,
                "ema_order_confirmation_candles": 3,
                "ema_order": "ema12_ema144_ema169_ema696",
                "ema696_slope_changes_required": 3,
                "rsi_used": false,
                "macd_used": false,
                "historical_net_move_used": false,
                "reversal_wick_used": false,
            }),
            "next_15m_open_after_second_acceptance_close",
        ),
        _ => unreachable!("isolated family version checked by caller"),
    };
    let strategy_preset = if args.paper_strategy_preset.is_empty() {
        filtered_volume_weekly_base_preset(&args.paper_outcome_entry_rule_version)
            .unwrap_or_default()
    } else {
        args.paper_strategy_preset.as_str()
    };
    let uses_volume_tier_exit =
        uses_momentum_exhaustion_volume_tier_exit(&args.paper_outcome_entry_rule_version);

    json!({
        "source": "market_velocity_event_backtest",
        "strategy_key": market_velocity_strategy_type(args),
        "strategy_family": family,
        "strategy_preset": strategy_preset,
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "hypothesis": hypothesis,
        "version_status": "research_unvalidated",
        "promotion_eligible": false,
        "paper_live_eligible": false,
        "event_source": "kline_15m",
        "completed_candles_only": true,
        "kline_current_live_only": args.kline_current_live_only,
        "sample_limit": args.sample_limit,
        "sample_seed": &args.sample_seed,
        "event_start_ms": args.event_start_ms,
        "event_end_ms": args.event_end_ms,
        "trade_direction": args.trade_direction.label(),
        "entry_logic": entry_rule,
        "entry_volume_gate": {
            "filtered_volume_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
            "history_candles": FILTERED_VOLUME_HISTORY_CANDLES,
            "minimum_retained_candles": FILTERED_VOLUME_MIN_RETAINED_CANDLES,
            "historical_spike_mark_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
            "current_candle_in_average": false,
            "weekly_volume_ccy_lookback_candles": WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES,
            "weekly_volume_ccy_nearest_rank_p90_zero_based_index": WEEKLY_VOLUME_CCY_P90_INDEX,
        },
        "entry_fill": entry_fill,
        "initial_stop": "actual_fill_plus_or_minus_atr14_x_1_5",
        "take_profit": if uses_volume_tier_exit {
            "actual_fill_plus_or_minus_signal_atr14_x_2_7_3_6_or_4_5"
        } else {
            "actual_fill_plus_or_minus_atr14_x_1_5_fixed_1r"
        },
        "maximum_holding_hours": args.equity_max_holding_hours,
        "account_risk_fraction_per_trade": 0.01,
        "fee_bps_per_side": args.backtest_fee_bps_per_side,
        "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
        "excluded_entry_logic": [
            "market_rank_events",
            "episodes",
            "bollinger",
            "bos",
            "fvg",
            "choch",
            "legacy_mixed_rsi_ema_macd",
        ],
        "exit_overlays_enabled": false,
    })
}

/// 构造 `back_test_log.risk_config`，显式区分旧默认费率与 v5+ 成本假设。
pub fn market_velocity_risk_config_detail(
    args: &MarketVelocityEventBacktestArgs,
    target_r: f64,
) -> Value {
    if is_isolated_entry_family_version(&args.paper_outcome_entry_rule_version) {
        let uses_volume_tier_exit =
            uses_momentum_exhaustion_volume_tier_exit(&args.paper_outcome_entry_rule_version);
        return json!({
            "mode": "symbol_isolated_100u_risk_sized",
            "position_sizing_contract": "entry_equity_x_1pct_divided_by_initial_stop_distance",
            "account_risk_fraction_per_trade": 0.01,
            "exchange_quantity_rounding_applied": false,
            "trade_direction": args.trade_direction.label(),
            "stop_loss_mode": "actual_fill_atr14_x_1_5",
            "initial_stop_atr_multiplier": FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
            "base_max_loss_enforced": false,
            "entry_candle_protection_check": true,
            "take_profit_mode": if uses_volume_tier_exit {
                "fixed_atr_distance_by_filtered_volume_ratio"
            } else {
                "fixed_atr_equal_to_initial_risk"
            },
            "take_profit_atr_multiplier": if uses_volume_tier_exit { Value::Null } else { json!(FILTERED_VOLUME_STOP_ATR_MULTIPLIER) },
            "take_profit_atr_multiplier_by_filtered_volume_ratio": if uses_volume_tier_exit {
                json!({
                    "2.5_to_below_4": 2.7,
                    "4_to_below_6": 3.6,
                    "6_or_above": 4.5,
                })
            } else {
                Value::Null
            },
            "gross_target_r": if uses_volume_tier_exit { Value::Null } else { json!(1.0) },
            "fee_bps_per_side": args.backtest_fee_bps_per_side,
            "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
            "maximum_holding_hours": args.equity_max_holding_hours,
            "profit_protection": false,
            "volume_trailing_stop": false,
        });
    }
    let is_filtered_volume_weekly_base =
        is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version);
    if is_filtered_volume_weekly_base {
        let uses_anchor_wick_or_next_touch_entry =
            uses_anchor_wick_or_next_touch_entry(&args.paper_outcome_entry_rule_version);
        let uses_profit_observation =
            uses_target_completion_profit_observation(&args.paper_outcome_entry_rule_version);
        let uses_trend_managed_exit =
            uses_trend_managed_volume_trailing_exit(&args.paper_outcome_entry_rule_version);
        let countertrend_default_target_atr = if args.paper_outcome_entry_rule_version
            == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
        {
            V13_COUNTERTREND_TARGET_ATR_MULTIPLIER
        } else {
            V12_COUNTERTREND_TARGET_ATR_MULTIPLIER
        };
        let filtered_volume_min_ratio =
            if uses_filtered_volume_2p5(&args.paper_outcome_entry_rule_version) {
                FILTERED_VOLUME_V9_MIN_RATIO
            } else {
                FILTERED_VOLUME_MIN_RATIO
            };
        // 即使 risk_config 列已扩容，也只保存真正生效的研究风险合同，避免无关空字段污染审计。
        return json!({
            "mode": "symbol_isolated_100u_risk_sized",
            "position_sizing_contract": "entry_equity_x_1pct_divided_by_initial_stop_distance",
            "account_risk_fraction_per_trade": 0.01,
            "exchange_quantity_rounding_applied": false,
            "trade_direction": args.trade_direction.label(),
            "stop_loss_mode": if uses_anchor_wick_or_next_touch_entry { "actual_fill_atr14_x_1_5" } else { "rsi_pattern_structure_else_actual_fill_atr14_x_1_5" },
            "base_max_loss_enforced": false,
            "entry_candle_protection_check": true,
            "initial_stop_atr_multiplier": FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
            "take_profit_mode": if uses_trend_managed_exit { "trend_relation_then_fixed_atr_distance" } else { "fixed_atr_distance_by_filtered_volume_ratio" },
            "filtered_volume_target_tiers": [
                {"min_ratio": filtered_volume_min_ratio, "max_ratio_exclusive": 4.0, "target_atr": 2.7},
                {"min_ratio": 4.0, "max_ratio_exclusive": 6.0, "target_atr": 3.6},
                {"min_ratio": 6.0, "max_ratio_exclusive": Value::Null, "target_atr": 4.5}
            ],
            "fee_bps_per_side": args.backtest_fee_bps_per_side,
            "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
            "maximum_holding_hours": args.equity_max_holding_hours,
            "trend_managed_exit": if uses_trend_managed_exit { json!({
                "countertrend_default_target_atr": countertrend_default_target_atr,
                "countertrend_exception_min_filtered_volume_ratio": COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO,
                "countertrend_exception_lookback_candles_excluding_p": COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES,
                "countertrend_exception_min_directional_net_move_pct": COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT,
                "holding_volume_min_filtered_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                "holding_volume_weekly_p90_required": false,
                "first_stop": "cost_adjusted_true_break_even",
                "later_stops": "incrementing_frozen_atr",
                "stop_update_effective": "next_completed_candle"
            }) } else { Value::Null },
            "profit_observation": if uses_profit_observation { json!({
                "mode": "target_completion_peak_retention_v1",
                "observation_trigger_r": OBSERVATION_TRIGGER_R,
                "pre_one_exit_close_r": PRE_ONE_EXIT_CLOSE_R,
                "post_one_trigger_r": POST_ONE_TRIGGER_R,
                "target_completion_offset": TARGET_COMPLETION_OFFSET,
                "minimum_lock_r": MINIMUM_LOCK_R,
                "peak_source": "completed_candle_high_low",
                "exit_confirmation": "completed_candle_close",
                "state_start": "entry_idx_plus_1",
                "new_lock_effective": "next_candle",
                "same_candle_priority": "active_stop_then_target_then_state"
            }) } else { Value::Null },
        });
    }
    json!({
        "mode": if is_filtered_volume_weekly_base { "symbol_isolated_100u_risk_sized" } else { "symbol_isolated_100u" },
        "position_sizing_contract": if is_filtered_volume_weekly_base { "entry_equity_x_1pct_divided_by_initial_stop_distance" } else { "legacy_symbol_isolated_100u" },
        "account_risk_fraction_per_trade": if is_filtered_volume_weekly_base { json!(0.01) } else { Value::Null },
        "exchange_quantity_rounding_applied": if is_filtered_volume_weekly_base { json!(false) } else { Value::Null },
        "trade_direction": args.trade_direction.label(),
        "stop_loss_pct": if args.entry_filtered_volume_rsi_ema_macd { Value::Null } else { json!(args.stop_loss_pct) },
        "stop_loss_pct_placeholder": if args.entry_filtered_volume_rsi_ema_macd { json!(args.stop_loss_pct) } else { Value::Null },
        "stop_loss_mode": if is_filtered_volume_weekly_base { "rsi_pattern_structure_else_actual_fill_atr14_x_1_5" } else if args.entry_filtered_volume_rsi_ema_macd { "atr14_x_1_5_no_cap" } else { args.stop_loss_mode.label() },
        "base_max_loss_enforced": !is_filtered_volume_weekly_base,
        "entry_candle_protection_check": is_filtered_volume_weekly_base,
        "take_profit_mode": if is_filtered_volume_weekly_base { "fixed_atr_distance_by_filtered_volume_ratio" } else if args.entry_filtered_volume_rsi_ema_macd { "filtered_volume_tiered_atr" } else if args.volume_atr_take_profit { "volume_atr" } else { "fixed_r" },
        "target_r": if args.volume_atr_take_profit { Value::Null } else { json!(target_r) },
        "target_r_placeholder": if args.volume_atr_take_profit { json!(target_r) } else { Value::Null },
        "volume_atr_target_scale": args.volume_atr_target_scale,
        "volume_atr_min_target_r": args.volume_atr_min_target_r,
        "volume_atr_max_target_r": args.volume_atr_max_target_r,
        "initial_stop_atr_multiplier": if args.entry_filtered_volume_rsi_ema_macd { json!(FILTERED_VOLUME_STOP_ATR_MULTIPLIER) } else { Value::Null },
        "filtered_volume_target_tiers": if is_filtered_volume_weekly_base { json!([
            {"min_ratio": 3.0, "max_ratio_exclusive": 4.0, "target_atr": 2.7},
            {"min_ratio": 4.0, "max_ratio_exclusive": 6.0, "target_atr": 3.6},
            {"min_ratio": 6.0, "max_ratio_exclusive": Value::Null, "target_atr": 4.5}
        ]) } else if args.entry_filtered_volume_rsi_ema_macd { json!([
            {"min_ratio": 3.0, "max_ratio_exclusive": 4.0, "target_r": 1.8, "target_atr": 2.7},
            {"min_ratio": 4.0, "max_ratio_exclusive": 6.0, "target_r": 2.4, "target_atr": 3.6},
            {"min_ratio": 6.0, "max_ratio_exclusive": Value::Null, "target_r": 3.0, "target_atr": 4.5}
        ]) } else { Value::Null },
        "fee_bps_per_side": args.backtest_fee_bps_per_side,
        "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
        "profit_protect_after_r": args.profit_protect_after_r,
        "profit_protect_stop_r": args.profit_protect_stop_r,
        "runner_target_r": args.runner_target_r,
        "runner_fraction": args.runner_fraction,
        "runner_stop_r": args.runner_stop_r,
        "early_exit_no_profit_candles": args.early_exit_no_profit_candles,
        "equity_max_holding_hours": args.equity_max_holding_hours,
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
    if uses_trend_managed_volume_trailing_exit(&args.paper_outcome_entry_rule_version) {
        return market_velocity_trend_managed_detail_signal_value(trade, args);
    }
    let is_filtered_volume_weekly_base =
        is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version);
    let is_isolated_entry_family =
        is_isolated_entry_family_version(&args.paper_outcome_entry_rule_version);
    let uses_momentum_volume_tier_exit =
        uses_momentum_exhaustion_volume_tier_exit(&args.paper_outcome_entry_rule_version);
    let mut detail = json!({
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
        "trade_direction": trade.direction.label(),
        "new_rank": trade.new_rank,
        "delta_rank": trade.delta_rank,
        "price_change_pct": trade.price_change_pct,
        "target_r": trade.target_r,
        "take_profit_mode": if uses_momentum_volume_tier_exit { "fixed_atr_distance_by_filtered_volume_ratio" } else if is_isolated_entry_family { "fixed_atr_equal_to_initial_risk" } else if is_filtered_volume_weekly_base { "fixed_atr_distance_by_filtered_volume_ratio" } else if args.entry_filtered_volume_rsi_ema_macd { "filtered_volume_tiered_atr" } else if args.volume_atr_take_profit { "volume_atr" } else { "fixed_r" },
        "volume_atr_target_scale": args.volume_atr_target_scale,
        "volume_atr_min_target_r": args.volume_atr_min_target_r,
        "volume_atr_max_target_r": args.volume_atr_max_target_r,
        "fee_bps_per_side": args.backtest_fee_bps_per_side,
        "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
        "entry_defer_bearish_continuation": args.entry_defer_bearish_continuation,
        "entry_defer_bullish_continuation": args.entry_defer_bullish_continuation,
        "entry_require_macd_negative_histogram_improving": args.entry_require_macd_negative_histogram_improving,
        "entry_require_opposite_reversal_confirmation": args.entry_require_opposite_reversal_confirmation,
        "entry_require_reversal_average_reclaim": args.entry_require_reversal_average_reclaim,
        "entry_extreme_volume_contrarian": args.entry_extreme_volume_contrarian,
        "entry_extreme_volume_continuation": args.entry_extreme_volume_continuation,
        "entry_rsi_volume_regime": args.entry_rsi_volume_regime,
        "entry_filtered_volume_rsi_ema_macd": args.entry_filtered_volume_rsi_ema_macd,
        "filtered_volume_ratio": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.filtered_volume_ratio),
        "filtered_volume_retained_candles": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.filtered_volume_retained_candles),
        "current_volume_ccy": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.current_volume_ccy),
        "weekly_volume_ccy_p90": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.weekly_volume_ccy_p90),
        "rsi14": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.rsi14),
        "macd_dif": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.macd_dif),
        "ema12": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.ema12),
        "ema144": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.ema144),
        "ema169": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.ema169),
        "ema696": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.ema696),
        "atr14": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.atr14),
        "take_profit_atr_multiplier": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.take_profit_atr_multiplier),
        "rsi_pattern_stop_participated": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.rsi_pattern_stop_participated),
        "bollinger_conflict_buffer": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.bollinger_conflict.as_ref()).map(|bollinger| json!({
            "period": bollinger.period,
            "standard_deviation_multiplier": bollinger.standard_deviation_multiplier,
            "middle": bollinger.middle,
            "upper": bollinger.upper,
            "lower": bollinger.lower,
            "touches_upper": bollinger.touches_upper,
            "touches_lower": bollinger.touches_lower,
            "standalone_entry_allowed": false,
        })),
        "ema144_distance_atr": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.ema144_distance_atr),
        "ema144_max_distance_atr": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.ema144_max_distance_atr),
        "ema_candidate_blocked_by_distance": trade.entry_signal_evidence.as_ref().map(|evidence| evidence.ema_candidate_blocked_by_distance),
        "trend_managed_exit": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.trend_managed_exit.as_ref()).map(|trend| json!({
            "market_regime": trend.market_regime,
            "trade_trend_relation": trend.trade_trend_relation,
            "long_term_bearish_confirmed": trend.long_term_bearish_confirmed,
            "long_term_bullish_confirmed": trend.long_term_bullish_confirmed,
            "ema696_recent": trend.ema696_recent,
            "bearish_platform_breakdown": trend.bearish_platform_breakdown.as_ref().map(|platform| json!({
                "direction": platform.direction,
                "break_ts_ms": platform.break_ts_ms,
                "confirmed_ts_ms": platform.confirmed_ts_ms,
                "platform_high": platform.platform_high,
                "platform_low": platform.platform_low,
                "platform_range_atr": platform.platform_range_atr,
                "atr_reference_ts_ms": platform.atr_reference_ts_ms,
                "platform_reference_atr14": platform.platform_reference_atr14,
                "close_center_shift_atr": platform.close_center_shift_atr,
                "close_regression_r_squared": platform.close_regression_r_squared,
                "fitted_close_drift_atr": platform.fitted_close_drift_atr,
                "upper_touch_count": platform.upper_touch_count,
                "lower_touch_count": platform.lower_touch_count,
                "break_body_range_ratio": platform.break_body_range_ratio,
                "break_body_open_ratio": platform.break_body_open_ratio,
                "filtered_volume_ratio": platform.filtered_volume_ratio,
                "current_volume_ccy": platform.current_volume_ccy,
                "weekly_volume_ccy_p90": platform.weekly_volume_ccy_p90,
            })),
            "bullish_platform_breakdown": trend.bullish_platform_breakdown.as_ref().map(|platform| json!({
                "direction": platform.direction,
                "break_ts_ms": platform.break_ts_ms,
                "confirmed_ts_ms": platform.confirmed_ts_ms,
                "platform_high": platform.platform_high,
                "platform_low": platform.platform_low,
                "platform_range_atr": platform.platform_range_atr,
                "atr_reference_ts_ms": platform.atr_reference_ts_ms,
                "platform_reference_atr14": platform.platform_reference_atr14,
                "close_center_shift_atr": platform.close_center_shift_atr,
                "close_regression_r_squared": platform.close_regression_r_squared,
                "fitted_close_drift_atr": platform.fitted_close_drift_atr,
                "upper_touch_count": platform.upper_touch_count,
                "lower_touch_count": platform.lower_touch_count,
                "break_body_range_ratio": platform.break_body_range_ratio,
                "break_body_open_ratio": platform.break_body_open_ratio,
                "filtered_volume_ratio": platform.filtered_volume_ratio,
                "current_volume_ccy": platform.current_volume_ccy,
                "weekly_volume_ccy_p90": platform.weekly_volume_ccy_p90,
            })),
            "prior_96_net_move_pct": trend.prior_96_net_move_pct,
            "countertrend_extreme_move_exception": trend.countertrend_extreme_move_exception,
            "volume_tier_take_profit_atr_multiplier": trend.volume_tier_take_profit_atr_multiplier,
            "selected_take_profit_atr_multiplier": trend.selected_take_profit_atr_multiplier,
            "target_policy": trend.target_policy,
        })),
        "isolated_family": trade.entry_signal_evidence.as_ref().and_then(|evidence| evidence.isolated_family.as_ref()).map(|family| json!({
            "family": family.family,
            "hypothesis": family.hypothesis,
            "prior_96_net_move_pct": family.prior_96_net_move_pct,
            "long_term_ema_confirmed": family.long_term_ema_confirmed,
            "ema696_recent": family.ema696_recent,
            "platform_breakdown": family.platform_breakdown.as_ref().map(|platform| json!({
                "direction": platform.direction,
                "break_ts_ms": platform.break_ts_ms,
                "confirmed_ts_ms": platform.confirmed_ts_ms,
                "platform_high": platform.platform_high,
                "platform_low": platform.platform_low,
                "platform_range_atr": platform.platform_range_atr,
                "atr_reference_ts_ms": platform.atr_reference_ts_ms,
                "platform_reference_atr14": platform.platform_reference_atr14,
                "close_center_shift_atr": platform.close_center_shift_atr,
                "close_regression_r_squared": platform.close_regression_r_squared,
                "fitted_close_drift_atr": platform.fitted_close_drift_atr,
                "upper_touch_count": platform.upper_touch_count,
                "lower_touch_count": platform.lower_touch_count,
                "break_body_range_ratio": platform.break_body_range_ratio,
                "break_body_open_ratio": platform.break_body_open_ratio,
                "filtered_volume_ratio": platform.filtered_volume_ratio,
                "current_volume_ccy": platform.current_volume_ccy,
                "weekly_volume_ccy_p90": platform.weekly_volume_ccy_p90,
            })),
        })),
        "initial_stop_source": &trade.initial_stop_source,
        "entry_relative_volume_at_time_10d": args.entry_relative_volume_at_time_10d,
        "entry_once_per_opposite_trend_state": args.entry_once_per_opposite_trend_state,
        "entry_once_per_historical_trend_state": args.entry_once_per_historical_trend_state,
        "entry_wait_setup_open_reclaim": args.entry_wait_setup_open_reclaim,
        "entry_opposite_trend_reset_confirm_candles": args.entry_opposite_trend_reset_confirm_candles,
        "entry_defer_max_wait_candles": args.entry_defer_max_wait_candles,
        "entry_min_opposite_duration_candles": args.entry_min_opposite_duration_candles,
        "entry_opposite_duration_min_r_squared": args.entry_opposite_duration_min_r_squared,
        "entry_min_exhaustion_volume_dominance_ratio": args.entry_min_exhaustion_volume_dominance_ratio,
        "entry_btc_96_max_abs_net_move_pct": args.entry_btc_96_max_abs_net_move_pct,
        "entry_btc_384_min_directional_net_move_pct": args.entry_btc_384_min_directional_net_move_pct,
        "entry_btc_require_current_directional_candle": args.entry_btc_require_current_directional_candle,
        "entry_exhaustion_volume_lookback_candles": EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
        "entry_exhaustion_current_cluster_candles": EXHAUSTION_CURRENT_CLUSTER_CANDLES,
        "entry_exhaustion_swing_radius_candles": EXHAUSTION_SWING_RADIUS_CANDLES,
        "stop_loss_pct": if args.entry_filtered_volume_rsi_ema_macd { Value::Null } else { json!(args.stop_loss_pct) },
        "stop_loss_pct_placeholder": if args.entry_filtered_volume_rsi_ema_macd { json!(args.stop_loss_pct) } else { Value::Null },
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
        "kline_current_live_only": args.kline_current_live_only,
        "kline_volume_rank_lookback_candles": if args.kline_volume_rank_velocity { json!(96) } else { Value::Null },
    });
    if args.paper_outcome_entry_rule_version
        == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION
        || is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version)
    {
        let divergences = trade
            .entry_signal_evidence
            .as_ref()
            .map(|evidence| {
                evidence
                    .macd_divergences
                    .iter()
                    .map(|divergence| {
                        json!({
                            "direction": divergence.direction.label(),
                            "pivot_ts_ms": divergence.pivot_ts_ms,
                            "reference_pivot_ts_ms": divergence.reference_pivot_ts_ms,
                            "pivot_price": divergence.pivot_price,
                            "reference_pivot_price": divergence.reference_pivot_price,
                            "pivot_rsi14": divergence.pivot_rsi14,
                            "pivot_dif": divergence.pivot_dif,
                            "reference_pivot_dif": divergence.reference_pivot_dif,
                            "pivot_normalized_dif": divergence.pivot_normalized_dif,
                            "reference_pivot_normalized_dif": divergence.reference_pivot_normalized_dif,
                            "normalized_dif_improvement": divergence.normalized_dif_improvement,
                            "zero_band_atr_multiplier": divergence.zero_band_atr_multiplier,
                            "min_normalized_dif_improvement": divergence.min_normalized_dif_improvement,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(object) = detail.as_object_mut() {
            if let Some(anchor_entry) = trade
                .entry_signal_evidence
                .as_ref()
                .and_then(|evidence| evidence.anchor_entry.as_ref())
            {
                object.insert(
                    "anchor_entry".to_string(),
                    json!({
                        "activation_mode": anchor_entry.activation_mode,
                        "pivot_body_range_ratio": anchor_entry.pivot_body_range_ratio,
                        "pivot_directional_wick_range_ratio": anchor_entry.pivot_directional_wick_range_ratio,
                        "pivot_opposite_wick_range_ratio": anchor_entry.pivot_opposite_wick_range_ratio,
                        "activation_price": anchor_entry.activation_price,
                        "activation_candle_ts_ms": anchor_entry.activation_candle_ts_ms,
                        "fill_price": anchor_entry.fill_price,
                        "fill_price_source": anchor_entry.fill_price_source,
                        "intrabar_path_policy": anchor_entry.intrabar_path_policy,
                    }),
                );
            }
            object.insert(
                "entry_filtered_volume_macd_zero_band_atr_multiplier".to_string(),
                json!(args.entry_filtered_volume_macd_zero_band_atr_multiplier),
            );
            object.insert(
                "entry_filtered_volume_macd_min_normalized_dif_improvement".to_string(),
                json!(args.entry_filtered_volume_macd_min_normalized_dif_improvement),
            );
            object.insert("macd_divergences".to_string(), json!(divergences));
            if is_filtered_volume_weekly_base {
                let rsi_divergences = trade
                    .entry_signal_evidence
                    .as_ref()
                    .map(|evidence| {
                        evidence
                            .rsi_divergences
                            .iter()
                            .map(|divergence| {
                                let mut value = json!({
                                    "comparison_mode": divergence.comparison_mode,
                                    "direction": divergence.direction.label(),
                                    "pivot_ts_ms": divergence.pivot_ts_ms,
                                    "reference_pivot_ts_ms": divergence.reference_pivot_ts_ms,
                                    "pivot_price": divergence.pivot_price,
                                    "reference_pivot_price": divergence.reference_pivot_price,
                                    "pivot_rsi14": divergence.pivot_rsi14,
                                    "reference_pivot_rsi14": divergence.reference_pivot_rsi14,
                                    "pivot_filtered_volume_ratio": divergence.pivot_filtered_volume_ratio,
                                    "reference_filtered_volume_ratio": divergence.reference_filtered_volume_ratio,
                                    "pivot_volume_ccy": divergence.pivot_volume_ccy,
                                    "reference_volume_ccy": divergence.reference_volume_ccy,
                                    "pivot_weekly_volume_ccy_p90": divergence.pivot_weekly_volume_ccy_p90,
                                    "reference_weekly_volume_ccy_p90": divergence.reference_weekly_volume_ccy_p90,
                                });
                                if let Some(object) = value.as_object_mut() {
                                    if let Some(confirmation_ts_ms) = divergence.confirmation_ts_ms {
                                        object.insert(
                                            "confirmation_ts_ms".to_string(),
                                            json!(confirmation_ts_ms),
                                        );
                                    }
                                    if let Some(confirmation_close) = divergence.confirmation_close {
                                        object.insert(
                                            "confirmation_close".to_string(),
                                            json!(confirmation_close),
                                        );
                                    }
                                    if let Some(confirmation_break_price) =
                                        divergence.confirmation_break_price
                                    {
                                        object.insert(
                                            "confirmation_break_price".to_string(),
                                            json!(confirmation_break_price),
                                        );
                                    }
                                }
                                value
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                object.insert("rsi_divergences".to_string(), json!(rsi_divergences));
            }
        }
    }
    detail
}

/// 趋势管理版本使用紧凑审计载荷，避免把旧策略的无关 false/null 参数复制进 5000 字符字段。
fn market_velocity_trend_managed_detail_signal_value(
    trade: &FrameworkEquityTradeReport,
    args: &MarketVelocityEventBacktestArgs,
) -> Value {
    let evidence = trade.entry_signal_evidence.as_ref();
    let anchor_entry = evidence
        .and_then(|item| item.anchor_entry.as_ref())
        .map(|anchor| {
            json!({
                "activation_mode": anchor.activation_mode,
                "pivot_body_range_ratio": anchor.pivot_body_range_ratio,
                "pivot_directional_wick_range_ratio": anchor.pivot_directional_wick_range_ratio,
                "pivot_opposite_wick_range_ratio": anchor.pivot_opposite_wick_range_ratio,
                "activation_price": anchor.activation_price,
                "activation_candle_ts_ms": anchor.activation_candle_ts_ms,
                "fill_price": anchor.fill_price,
                "fill_price_source": anchor.fill_price_source,
                "intrabar_path_policy": anchor.intrabar_path_policy,
            })
        });
    let rsi_divergences = evidence
        .map(|item| {
            item.rsi_divergences
                .iter()
                .map(|divergence| {
                    json!({
                        "comparison_mode": divergence.comparison_mode,
                        "direction": divergence.direction.label(),
                        "pivot_ts_ms": divergence.pivot_ts_ms,
                        "reference_pivot_ts_ms": divergence.reference_pivot_ts_ms,
                        "pivot_price": divergence.pivot_price,
                        "reference_pivot_price": divergence.reference_pivot_price,
                        "pivot_rsi14": divergence.pivot_rsi14,
                        "reference_pivot_rsi14": divergence.reference_pivot_rsi14,
                        "pivot_filtered_volume_ratio": divergence.pivot_filtered_volume_ratio,
                        "reference_filtered_volume_ratio": divergence.reference_filtered_volume_ratio,
                        "pivot_volume_ccy": divergence.pivot_volume_ccy,
                        "reference_volume_ccy": divergence.reference_volume_ccy,
                        "pivot_weekly_volume_ccy_p90": divergence.pivot_weekly_volume_ccy_p90,
                        "reference_weekly_volume_ccy_p90": divergence.reference_weekly_volume_ccy_p90,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let trend_managed_exit = evidence
        .and_then(|item| item.trend_managed_exit.as_ref())
        .map(|trend| {
            let mut platform_breakdowns = Vec::with_capacity(2);
            for platform in [
                trend.bearish_platform_breakdown.as_ref(),
                trend.bullish_platform_breakdown.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                platform_breakdowns.push(json!({
                    "direction": platform.direction,
                    "break_ts_ms": platform.break_ts_ms,
                    "confirmed_ts_ms": platform.confirmed_ts_ms,
                    "platform_high": platform.platform_high,
                    "platform_low": platform.platform_low,
                    "platform_range_atr": platform.platform_range_atr,
                    "atr_reference_ts_ms": platform.atr_reference_ts_ms,
                    "platform_reference_atr14": platform.platform_reference_atr14,
                    "close_center_shift_atr": platform.close_center_shift_atr,
                    "close_regression_r_squared": platform.close_regression_r_squared,
                    "fitted_close_drift_atr": platform.fitted_close_drift_atr,
                    "upper_touch_count": platform.upper_touch_count,
                    "lower_touch_count": platform.lower_touch_count,
                    "break_body_range_ratio": platform.break_body_range_ratio,
                    "break_body_open_ratio": platform.break_body_open_ratio,
                    "filtered_volume_ratio": platform.filtered_volume_ratio,
                    "current_volume_ccy": platform.current_volume_ccy,
                    "weekly_volume_ccy_p90": platform.weekly_volume_ccy_p90,
                }));
            }
            json!({
                "market_regime": trend.market_regime,
                "trade_trend_relation": trend.trade_trend_relation,
                "long_term_bearish_confirmed": trend.long_term_bearish_confirmed,
                "long_term_bullish_confirmed": trend.long_term_bullish_confirmed,
                "ema696_recent": trend.ema696_recent,
                "platform_breakdowns": platform_breakdowns,
                "prior_96_net_move_pct": trend.prior_96_net_move_pct,
                "countertrend_extreme_move_exception": trend.countertrend_extreme_move_exception,
                "volume_tier_take_profit_atr_multiplier": trend.volume_tier_take_profit_atr_multiplier,
                "selected_take_profit_atr_multiplier": trend.selected_take_profit_atr_multiplier,
                "target_policy": trend.target_policy,
            })
        });

    json!({
        "source": "market_velocity_framework_replay",
        "rank_event_id": trade.event_id,
        "setup_ts": trade.signal_ts,
        "detected_at": &trade.detected_at,
        "entry_ts": trade.entry_ts,
        "entry_trigger": &trade.trigger,
        "trade_direction": trade.direction.label(),
        "target_r": trade.target_r,
        "take_profit_mode": "trend_relation_then_fixed_atr_distance",
        "take_profit_atr_multiplier": evidence.and_then(|item| item.take_profit_atr_multiplier),
        "fee_bps_per_side": args.backtest_fee_bps_per_side,
        "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
        "filtered_volume_ratio": evidence.map(|item| item.filtered_volume_ratio),
        "filtered_volume_retained_candles": evidence.map(|item| item.filtered_volume_retained_candles),
        "current_volume_ccy": evidence.and_then(|item| item.current_volume_ccy),
        "weekly_volume_ccy_p90": evidence.and_then(|item| item.weekly_volume_ccy_p90),
        "rsi14": evidence.map(|item| item.rsi14),
        "macd_dif": evidence.map(|item| item.macd_dif),
        "ema12": evidence.map(|item| item.ema12),
        "ema144": evidence.map(|item| item.ema144),
        "ema169": evidence.map(|item| item.ema169),
        "ema696": evidence.map(|item| item.ema696),
        "atr14": evidence.map(|item| item.atr14),
        "rsi_divergences": rsi_divergences,
        "anchor_entry": anchor_entry,
        "trend_managed_exit": trend_managed_exit,
        "initial_stop_source": &trade.initial_stop_source,
        "initial_stop_atr_multiplier": FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
        "account_risk_fraction_per_trade": 0.01,
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "event_source": "kline_15m",
        "kline_current_live_only": args.kline_current_live_only,
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

/// 合并平仓决策摘要，同时保留开仓时冻结的指标快照。
///
/// 止损更新历史已有独立 `stop_loss_update_history` 文本列，不能再复制进 5000 字符的
/// `signal_value`；否则多次移动保护位会让整批回测在事务提交时失败。
fn market_velocity_close_signal_value(
    trade: &FrameworkEquityTradeReport,
    entry_signal_value: &Value,
) -> Value {
    let mut value = entry_signal_value.clone();
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    if let Some(exit_signal) = trade
        .close_signal_value
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
    {
        if let Some(exit_reason) = exit_signal.get("exit_reason") {
            object.insert("exit_reason".to_string(), exit_reason.clone());
        }
        if let Some(profit_observation) = exit_signal.get("profit_observation") {
            object.insert("profit_observation".to_string(), profit_observation.clone());
        }
    }
    value
}

/// 构建回测明细，把开仓证据、初始风险和分腿平仓统一转换为 legacy 落库结构。
pub fn build_market_velocity_backtest_details(
    trade: &FrameworkEquityTradeReport,
    back_test_id: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<BacktestDetail>> {
    let close_position_time = trade
        .close_position_time
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("market velocity trade missing close_position_time"))?;
    let signal_open_position_time = legacy_backtest_datetime(
        &trade.signal_open_position_time,
        "signal_open_position_time",
    )?;
    let open_position_time =
        legacy_backtest_datetime(&trade.open_position_time, "open_position_time")?;
    let close_position_time = legacy_backtest_datetime(close_position_time, "close_position_time")?;
    let entry_signal_value = market_velocity_detail_signal_value(trade, args);
    let signal_value = entry_signal_value.to_string();
    let close_signal_value =
        market_velocity_close_signal_value(trade, &entry_signal_value).to_string();
    let signal_result = "market_velocity_framework_replay".to_string();
    let strategy_type = market_velocity_strategy_type(args).to_string();
    // v3 允许信号 K 线颜色与最终交易方向相反，因此明细必须记录回测已冻结的方向，
    // 不能再从当前 K 线涨跌幅二次推断，否则落库后会把真实多单写成空单。
    let open_option_type = match trade.direction {
        MarketVelocityTradeDirection::Long => "long",
        MarketVelocityTradeDirection::Short => "short",
        MarketVelocityTradeDirection::Both => {
            return Err(anyhow::anyhow!(
                "market velocity trade detail requires a concrete direction"
            ));
        }
    };
    let open_price = trade.open_price.to_string();
    let quantity = trade.quantity.to_string();
    let initial_stop_price = initial_stop_price_for_trade_detail(trade);
    let stop_loss_source = if initial_stop_price.is_none() {
        None
    } else if trade.initial_stop_source.is_some() {
        trade.initial_stop_source.clone()
    } else if matches!(
        args.paper_outcome_entry_rule_version.as_str(),
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION
    ) {
        Some(FILTERED_VOLUME_ATR_STOP_SOURCE.to_string())
    } else if matches!(
        args.paper_outcome_entry_rule_version.as_str(),
        MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION
            | MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION
    ) {
        Some(RSI_VOLUME_V3_ATR_STOP_SOURCE.to_string())
    } else {
        None
    };
    let (win_nums, loss_nums) = match trade.outcome {
        "win" => (1, 0),
        "loss" => (0, 1),
        _ => (0, 0),
    };
    let mut details = vec![BacktestDetail::new(
        back_test_id,
        open_option_type.to_string(),
        strategy_type.clone(),
        trade.symbol.clone(),
        "15m".to_string(),
        open_position_time.clone(),
        Some(signal_open_position_time.clone()),
        trade.signal_status,
        open_position_time.clone(),
        open_price.clone(),
        None,
        "0".to_string(),
        quantity.clone(),
        "false".to_string(),
        String::new(),
        0,
        0,
        signal_value.clone(),
        signal_result.clone(),
        stop_loss_source.clone(),
        None,
        initial_stop_price,
        trade.initial_risk_amount,
        None,
    )];
    if trade.close_legs.is_empty() {
        details.push(BacktestDetail::new(
            back_test_id,
            "close".to_string(),
            strategy_type,
            trade.symbol.clone(),
            "15m".to_string(),
            open_position_time.clone(),
            Some(signal_open_position_time.clone()),
            trade.signal_status,
            close_position_time,
            open_price,
            trade.close_price.map(|value| value.to_string()),
            trade.profit_loss.to_string(),
            quantity,
            "true".to_string(),
            trade.close_type.clone(),
            win_nums,
            loss_nums,
            close_signal_value,
            signal_result,
            trade.close_stop_loss_source.clone().or(stop_loss_source),
            trade.stop_loss_update_history.clone(),
            initial_stop_price,
            trade.initial_risk_amount,
            trade.net_profit_r,
        ));
        return Ok(details);
    }
    for leg in &trade.close_legs {
        let leg_signal_value =
            market_velocity_detail_signal_value_for_leg(trade, args, leg).to_string();
        let leg_close_position_time =
            legacy_backtest_datetime(&leg.close_position_time, "leg.close_position_time")?;
        let (leg_win_nums, leg_loss_nums) = if leg.full_close {
            (win_nums, loss_nums)
        } else {
            (0, 0)
        };
        let leg_initial_risk_amount = trade
            .initial_risk_amount
            .filter(|_| trade.quantity.is_finite() && trade.quantity > 0.0)
            .map(|risk| risk * leg.quantity / trade.quantity)
            .filter(|risk| risk.is_finite() && *risk > 0.0);
        let leg_net_profit_r = leg_initial_risk_amount
            .map(|risk| leg.profit_loss / risk)
            .filter(|value| value.is_finite());
        details.push(BacktestDetail::new(
            back_test_id,
            "close".to_string(),
            strategy_type.clone(),
            trade.symbol.clone(),
            "15m".to_string(),
            open_position_time.clone(),
            Some(signal_open_position_time.clone()),
            trade.signal_status,
            leg_close_position_time,
            open_price.clone(),
            Some(leg.close_price.to_string()),
            leg.profit_loss.to_string(),
            leg.quantity.to_string(),
            leg.full_close.to_string(),
            leg.close_type.clone(),
            leg_win_nums,
            leg_loss_nums,
            leg_signal_value,
            signal_result.clone(),
            stop_loss_source.clone(),
            None,
            initial_stop_price,
            leg_initial_risk_amount,
            leg_net_profit_r,
        ));
    }
    Ok(details)
}

/// 从已冻结的价格风险金额还原入场保护价，供 legacy 明细表保留初始 R 审计证据。
fn initial_stop_price_for_trade_detail(trade: &FrameworkEquityTradeReport) -> Option<f64> {
    let risk_per_unit = trade
        .initial_risk_amount
        .filter(|risk| risk.is_finite() && *risk > 0.0)
        .filter(|_| trade.quantity.is_finite() && trade.quantity > 0.0)
        .map(|risk| risk / trade.quantity)?;
    let stop = match trade.direction {
        MarketVelocityTradeDirection::Long => trade.open_price - risk_per_unit,
        MarketVelocityTradeDirection::Short => trade.open_price + risk_per_unit,
        MarketVelocityTradeDirection::Both => return None,
    };
    (stop.is_finite() && stop > 0.0).then_some(stop)
}

/// 把历史回测明细接受的两种上海时间格式归一为无偏移量的数据库时间文本。
fn legacy_backtest_datetime(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if let Ok(value) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return Ok(value.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    DateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%:z")
        .map(|value| value.naive_local().format("%Y-%m-%d %H:%M:%S").to_string())
        .with_context(|| format!("invalid market velocity {field_name}: {value}"))
}
