use anyhow::Result;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use rust_quant_services::market::MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_PRESET;

use super::args::{
    entry_trigger_filter_version_label, format_entry_trigger_filter_list,
    market_momentum_exhaustion_reversal_v1_research_args,
    market_momentum_exhaustion_reversal_v2_research_args,
    market_momentum_exhaustion_reversal_v3_research_args,
    market_volume_anchor_rsi_divergence_reversal_v1_research_args,
    market_volume_anchor_rsi_divergence_reversal_v2_research_args,
    market_volume_platform_break_trend_v1_research_args,
    market_volume_platform_break_trend_v2_research_args, parse_paper_observation_args_from,
    MarketVelocityTradeDirection,
};
use super::computed_candles::{
    FAST_MOMENTUM_ATR_PERIOD, FILTERED_VOLUME_EMA_FAST_PERIOD, FILTERED_VOLUME_EMA_MIDDLE_PERIOD,
    FILTERED_VOLUME_EMA_SECOND_MIDDLE_PERIOD, FILTERED_VOLUME_EMA_SLOW_PERIOD,
};
use super::directional_reversal::{
    EXHAUSTION_CURRENT_CLUSTER_CANDLES, EXHAUSTION_SWING_RADIUS_CANDLES,
    EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
};
use super::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v1::{
    MOMENTUM_EXHAUSTION_HYPOTHESIS, MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES,
    MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
};
use super::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v2::{
    MOMENTUM_EXHAUSTION_LIMIT_VALID_CANDLES, MOMENTUM_EXHAUSTION_V2_HYPOTHESIS,
};
use super::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v3::{
    MOMENTUM_EXHAUSTION_V3_HYPOTHESIS, MOMENTUM_EXHAUSTION_V3_WICK_MIN_RANGE_RATIO,
};
use super::filtered_volume_rsi_ema_macd::volume_anchor_rsi_divergence_reversal_v1::{
    ISOLATED_RSI_COMPARISON_MODE, VOLUME_ANCHOR_RSI_HYPOTHESIS,
};
use super::filtered_volume_rsi_ema_macd::volume_anchor_rsi_divergence_reversal_v2::{
    BEARISH_ANCHOR_RESET_RSI, BULLISH_ANCHOR_RESET_RSI, ISOLATED_RSI_V2_COMPARISON_MODE,
    MIN_INTERVENING_CANDLES, VOLUME_ANCHOR_RSI_V2_HYPOTHESIS,
};
use super::filtered_volume_rsi_ema_macd::volume_platform_break_trend_v1::{
    PLATFORM_BREAK_TREND_HYPOTHESIS, PLATFORM_CONFIRMATION_CANDLES, PLATFORM_LOOKBACK_CANDLES,
    PLATFORM_MAX_RANGE_ATR, PLATFORM_MIN_BODY_OPEN_RATIO, PLATFORM_MIN_BODY_RANGE_RATIO,
};
use super::filtered_volume_rsi_ema_macd::volume_platform_break_trend_v2::{
    PLATFORM_BREAK_TREND_V2_HYPOTHESIS, PLATFORM_V2_MAX_CENTER_SHIFT_ATR,
    PLATFORM_V2_MAX_FITTED_DRIFT_ATR, PLATFORM_V2_MIN_TOUCH_SEPARATION_CANDLES,
    PLATFORM_V2_TOUCH_ZONE_WIDTH_RATIO, PLATFORM_V2_TREND_R_SQUARED_MIN,
};
use super::filtered_volume_rsi_ema_macd::weekly_base_volume_bollinger_conflict_v4::{
    BOLLINGER_CONFLICT_PERIOD, BOLLINGER_CONFLICT_STDDEV_MULTIPLIER,
};
use super::filtered_volume_rsi_ema_macd::weekly_base_volume_ema144_proximity_v5::EMA144_MAX_DISTANCE_ATR;
use super::filtered_volume_rsi_ema_macd::weekly_base_volume_v3::{
    WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES, WEEKLY_VOLUME_CCY_P90_INDEX,
};
use super::filtered_volume_rsi_ema_macd::weekly_p90_anchor_rsi_trend_managed_counter15_v13::COUNTERTREND_TARGET_ATR_MULTIPLIER as V13_COUNTERTREND_TARGET_ATR_MULTIPLIER;
use super::filtered_volume_rsi_ema_macd::weekly_p90_anchor_rsi_trend_managed_v12::{
    COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES, COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT,
    COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO,
    COUNTERTREND_TARGET_ATR_MULTIPLIER as V12_COUNTERTREND_TARGET_ATR_MULTIPLIER,
    TREND_PLATFORM_CONFIRMATION_CANDLES, TREND_PLATFORM_LOOKBACK_CANDLES,
    TREND_PLATFORM_MAX_RANGE_ATR, TREND_PLATFORM_MIN_BODY_OPEN_RATIO,
    TREND_PLATFORM_MIN_BODY_RANGE_RATIO,
};
use super::filtered_volume_rsi_ema_macd::{
    DIVERGENCE_LOOKBACK_CANDLES, DIVERGENCE_PIVOT_WING_CANDLES, DOJI_MAX_BODY_RANGE_RATIO,
    EMA_BODY_MAX_OPEN_RATIO, EMA_BODY_MIN_OPEN_RATIO, EMA_BODY_MIN_RANGE_RATIO,
    FILTERED_VOLUME_HISTORY_CANDLES, FILTERED_VOLUME_MIN_RATIO,
    FILTERED_VOLUME_MIN_RETAINED_CANDLES, FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
    FILTERED_VOLUME_V9_MIN_RATIO, MACD_RSI_NEUTRAL_MAX, MACD_RSI_NEUTRAL_MIN,
    REVERSAL_WICK_MIN_RANGE_RATIO, RSI_DIVERGENCE_MIN_DELTA, RSI_OVERBOUGHT, RSI_OVERSOLD,
};
use super::rsi_volume_regime::{
    RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES, RSI_VOLUME_DIVERGENCE_MIN_RSI_DELTA,
    RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES, RSI_VOLUME_LOOKBACK_CANDLES,
    RSI_VOLUME_MACD_NEAR_ZERO_MAX_PCT, RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT, RSI_VOLUME_MIN_RATIO,
    RSI_VOLUME_MIN_TREND_R_SQUARED, RSI_VOLUME_NARROW_BAND_PERCENTILE, RSI_VOLUME_OVERBOUGHT,
    RSI_VOLUME_OVERSOLD, RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES, RSI_VOLUME_SIDEWAYS_RSI_MAX,
    RSI_VOLUME_SIDEWAYS_RSI_MIN, RSI_VOLUME_TREND_LOOKBACK_CANDLES,
    RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA, RSI_VOLUME_V3_LOOKBACK_CANDLES,
    RSI_VOLUME_V3_MIN_RATIO, RSI_VOLUME_V3_OPPOSING_WICK_MAX_BODY_MULTIPLE,
    RSI_VOLUME_V3_STOP_ATR_MULTIPLIER, RSI_VOLUME_V3_STOP_ATR_PERIOD,
    RSI_VOLUME_V5_LOOKBACK_CANDLES, RSI_VOLUME_V5_MIN_RATIO,
};
use super::{
    isolated_entry_family_product_slug, isolated_entry_family_strategy_key,
    market_filtered_volume_rsi_ema_macd_v10_research_args,
    market_filtered_volume_rsi_ema_macd_v11_research_args,
    market_filtered_volume_rsi_ema_macd_v12_research_args,
    market_filtered_volume_rsi_ema_macd_v13_research_args,
    market_filtered_volume_rsi_ema_macd_v1_research_args,
    market_filtered_volume_rsi_ema_macd_v2_research_args,
    market_filtered_volume_rsi_ema_macd_v3_research_args,
    market_filtered_volume_rsi_ema_macd_v4_research_args,
    market_filtered_volume_rsi_ema_macd_v5_research_args,
    market_filtered_volume_rsi_ema_macd_v9_research_args,
    market_momentum_direct_kline_v36_frozen_args, market_rsi_volume_regime_v1_research_args,
    market_rsi_volume_regime_v2_research_args, market_rsi_volume_regime_v3_research_args,
    market_rsi_volume_regime_v4_research_args, market_rsi_volume_regime_v5_research_args,
    uses_momentum_exhaustion_volume_tier_exit, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRODUCT_SLUG,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRODUCT_SLUG,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRODUCT_SLUG,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRODUCT_SLUG,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRODUCT_SLUG,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRODUCT_SLUG,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY, MARKET_MOMENTUM_DIRECT_KLINE_V36_PRESET,
    MARKET_MOMENTUM_DIRECT_KLINE_V36_PRODUCT_SLUG, MARKET_MOMENTUM_DIRECT_KLINE_V36_STRATEGY_KEY,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET, MARKET_RSI_VOLUME_REGIME_PRODUCT_SLUG,
    MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY, MARKET_RSI_VOLUME_REGIME_V1_PRESET,
    MARKET_RSI_VOLUME_REGIME_V2_PRESET, MARKET_RSI_VOLUME_REGIME_V3_PRESET,
    MARKET_RSI_VOLUME_REGIME_V4_PRESET, MARKET_RSI_VOLUME_REGIME_V5_PRESET,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET,
};

const MARKET_VELOCITY_STRATEGY_KEY: &str = "market_velocity";
const MARKET_VELOCITY_PRODUCT_SLUG: &str = "market-velocity-radar";
const MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_KEY: &str = "market_velocity_breakdown_short";
const MARKET_VELOCITY_BREAKDOWN_SHORT_PRODUCT_SLUG: &str = "market-velocity-breakdown-short";
const MARKET_MOMENTUM_OPPOSITE_MOVE_STRATEGY_KEY: &str = "market_momentum_opposite_move_reversal";
const MARKET_MOMENTUM_OPPOSITE_MOVE_PRODUCT_SLUG: &str = "market-momentum-opposite-move-reversal";
const MARKET_MOMENTUM_OPPOSITE_MOVE_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_volume_atr_both_15m_v1";
const MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_volume_atr_long_defer3_15m_v2";
const MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_15m_v3";
const MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_PRESET: &str = "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_exhaustionvol1_15m_v4";
const MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_PRESET: &str = "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_r18_30_scale4_both_deferlong3_exhaustionvol1_15m_v5";
const MARKET_MOMENTUM_OPPOSITE_MOVE_CONFIRMED_REVERSAL_PRESET: &str =
    "research_market_momentum_opposite_move_reversal_confirmed_both_defer3_volatr_r18_30_15m_v6";
const MARKET_MOMENTUM_OPPOSITE_MOVE_MEAN_RECLAIM_PRESET: &str =
    "research_market_momentum_opposite_move_reversal_mean_reclaim_both_defer3_volatr_r18_30_15m_v7";

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityPresetManifest {
    pub product_slug: String,
    pub symbol: String,
    pub channel: String,
    pub manifest_hash: String,
    pub strategy_key: String,
    pub human_label: String,
    pub risk_level: String,
    pub manifest_status: String,
    pub manifest_json: Value,
    pub canonical_json: String,
}

pub fn market_velocity_paper_strategy_preset_manifest(
    preset: &str,
) -> Result<MarketVelocityPresetManifest> {
    let args = if preset == MARKET_MOMENTUM_DIRECT_KLINE_V36_PRESET {
        market_momentum_direct_kline_v36_frozen_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET {
        market_filtered_volume_rsi_ema_macd_v1_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET {
        market_filtered_volume_rsi_ema_macd_v2_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET {
        market_filtered_volume_rsi_ema_macd_v3_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET {
        market_filtered_volume_rsi_ema_macd_v4_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET {
        market_filtered_volume_rsi_ema_macd_v5_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET {
        market_filtered_volume_rsi_ema_macd_v9_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET {
        market_filtered_volume_rsi_ema_macd_v10_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET {
        market_filtered_volume_rsi_ema_macd_v11_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET {
        market_filtered_volume_rsi_ema_macd_v12_research_args()?
    } else if preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET {
        market_filtered_volume_rsi_ema_macd_v13_research_args()?
    } else if preset == MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET {
        market_momentum_exhaustion_reversal_v1_research_args()?
    } else if preset == MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET {
        market_momentum_exhaustion_reversal_v2_research_args()?
    } else if preset == MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET {
        market_momentum_exhaustion_reversal_v3_research_args()?
    } else if preset == MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET {
        market_volume_anchor_rsi_divergence_reversal_v1_research_args()?
    } else if preset == MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET {
        market_volume_anchor_rsi_divergence_reversal_v2_research_args()?
    } else if preset == MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET {
        market_volume_platform_break_trend_v1_research_args()?
    } else if preset == MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET {
        market_volume_platform_break_trend_v2_research_args()?
    } else if preset == MARKET_RSI_VOLUME_REGIME_V1_PRESET {
        market_rsi_volume_regime_v1_research_args()?
    } else if preset == MARKET_RSI_VOLUME_REGIME_V2_PRESET {
        market_rsi_volume_regime_v2_research_args()?
    } else if preset == MARKET_RSI_VOLUME_REGIME_V3_PRESET {
        market_rsi_volume_regime_v3_research_args()?
    } else if preset == MARKET_RSI_VOLUME_REGIME_V4_PRESET {
        market_rsi_volume_regime_v4_research_args()?
    } else if preset == MARKET_RSI_VOLUME_REGIME_V5_PRESET {
        market_rsi_volume_regime_v5_research_args()?
    } else {
        parse_paper_observation_args_from(["--paper-strategy-preset", preset])?
    };
    if isolated_entry_family_strategy_key(&args.paper_outcome_entry_rule_version).is_some() {
        return isolated_entry_family_manifest(preset, &args);
    }
    let has_allowlist = !args.entry_trigger_allowlist.is_empty();
    let has_blocklist = !args.entry_trigger_blocklist.is_empty();
    let allowlist_label = format_entry_trigger_filter_list(&args.entry_trigger_allowlist);
    let blocklist_label = format_entry_trigger_filter_list(&args.entry_trigger_blocklist);
    let is_breakdown_short = args.trade_direction == MarketVelocityTradeDirection::Short;
    let is_direct_kline_v36 = preset == MARKET_MOMENTUM_DIRECT_KLINE_V36_PRESET;
    let is_filtered_volume_rsi_ema_macd = matches!(
        preset,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET
    );
    let is_filtered_volume_rsi_ema_macd_v2_plus = matches!(
        preset,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET
    );
    let is_filtered_volume_weekly_base = matches!(
        preset,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET
    );
    let is_filtered_volume_rsi_ema_macd_v10 =
        preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET;
    let is_filtered_volume_rsi_ema_macd_v11 =
        preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET;
    let is_filtered_volume_rsi_ema_macd_v12 =
        preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET;
    let is_filtered_volume_rsi_ema_macd_v13 =
        preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET;
    let is_filtered_volume_rsi_ema_macd_trend_managed =
        is_filtered_volume_rsi_ema_macd_v12 || is_filtered_volume_rsi_ema_macd_v13;
    let uses_filtered_volume_anchor_rsi = matches!(
        preset,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET
    );
    let filtered_volume_min_ratio = if uses_filtered_volume_anchor_rsi {
        FILTERED_VOLUME_V9_MIN_RATIO
    } else {
        FILTERED_VOLUME_MIN_RATIO
    };
    let filtered_volume_rsi_branch = if is_filtered_volume_rsi_ema_macd_v11
        || is_filtered_volume_rsi_ema_macd_trend_managed
    {
        json!({
            "oversold_max_inclusive": RSI_OVERSOLD,
            "overbought_min_inclusive": RSI_OVERBOUGHT,
            "divergence_mode": "nearest_directional_weekly_p90_filtered_volume_anchor_wick_or_next_touch",
            "divergence_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
            "pivot_wing_candles": 0,
            "minimum_rsi_improvement": 0.0,
            "equal_rsi_allowed": true,
            "anchor_and_current_require_filtered_ratio_and_weekly_p90": true,
            "anchor_required_for_entry": true,
            "directional_wick_min_full_range_ratio": REVERSAL_WICK_MIN_RANGE_RATIO,
            "doji_max_body_range_ratio": DOJI_MAX_BODY_RANGE_RATIO,
            "directional_wick_rule": "long_lower_wick_short_upper_wick_and_larger_than_opposite",
            "directional_wick_entry": "immediate_next_15m_open",
            "non_wick_entry": "immediate_next_candle_intrabar_strict_break_of_p_high_or_p_low",
            "gap_fill": "next_open_when_open_already_through_activation",
            "intrabar_fill": "p_high_for_long_p_low_for_short",
            "untriggered_setup_expiry_candles": 1,
            "intrabar_path_policy": "full_15m_bar_conservative_stop_first",
        })
    } else if is_filtered_volume_rsi_ema_macd_v10 {
        json!({
            "oversold_max_inclusive": RSI_OVERSOLD,
            "overbought_min_inclusive": RSI_OVERBOUGHT,
            "divergence_mode": "nearest_directional_weekly_p90_filtered_volume_anchor_next_close_confirmed",
            "divergence_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
            "pivot_wing_candles": 0,
            "minimum_rsi_improvement": 0.0,
            "equal_rsi_allowed": true,
            "anchor_and_current_require_filtered_ratio_and_weekly_p90": true,
            "engulfing_and_wick_fallback_enabled": false,
            "anchor_required_for_entry": true,
            "ema_macd_candidates_may_only_merge_with_confirmed_anchor": true,
            "next_close_confirmation_candles": 1,
            "next_close_confirmation_rule": "long_close_above_p_high_short_close_below_p_low",
            "unconfirmed_setup_expiry_candles": 1,
        })
    } else if uses_filtered_volume_anchor_rsi {
        json!({
            "oversold_max_inclusive": RSI_OVERSOLD,
            "overbought_min_inclusive": RSI_OVERBOUGHT,
            "divergence_mode": "nearest_directional_weekly_p90_filtered_volume_anchor",
            "divergence_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
            "pivot_wing_candles": 0,
            "minimum_rsi_improvement": 0.0,
            "equal_rsi_allowed": true,
            "anchor_and_current_require_filtered_ratio_and_weekly_p90": true,
            "engulfing_and_wick_fallback_enabled": true,
        })
    } else {
        json!({
            "oversold_max_inclusive": RSI_OVERSOLD,
            "overbought_min_inclusive": RSI_OVERBOUGHT,
            "divergence_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
            "pivot_wing_candles": DIVERGENCE_PIVOT_WING_CANDLES,
            "minimum_rsi_improvement": RSI_DIVERGENCE_MIN_DELTA,
            "reversal_wick_min_full_range_ratio": REVERSAL_WICK_MIN_RANGE_RATIO,
            "doji_max_body_range_ratio": DOJI_MAX_BODY_RANGE_RATIO,
            "engulfing_rule_enabled": true,
            "divergence_precedes_pattern": true,
        })
    };
    let is_filtered_volume_rsi_ema_macd_v4 =
        preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET;
    let is_filtered_volume_rsi_ema_macd_v5 =
        preset == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET;
    let is_rsi_volume_regime = matches!(
        preset,
        MARKET_RSI_VOLUME_REGIME_V1_PRESET
            | MARKET_RSI_VOLUME_REGIME_V2_PRESET
            | MARKET_RSI_VOLUME_REGIME_V3_PRESET
            | MARKET_RSI_VOLUME_REGIME_V4_PRESET
            | MARKET_RSI_VOLUME_REGIME_V5_PRESET
    );
    let is_rsi_volume_regime_v2 = preset == MARKET_RSI_VOLUME_REGIME_V2_PRESET;
    let is_rsi_volume_regime_v3 = preset == MARKET_RSI_VOLUME_REGIME_V3_PRESET;
    let is_rsi_volume_regime_v4 = preset == MARKET_RSI_VOLUME_REGIME_V4_PRESET;
    let is_rsi_volume_regime_v5 = preset == MARKET_RSI_VOLUME_REGIME_V5_PRESET;
    let uses_rsi_volume_atr_contract = is_rsi_volume_regime_v3
        || is_rsi_volume_regime_v4
        || is_rsi_volume_regime_v5
        || is_filtered_volume_rsi_ema_macd;
    let is_opposite_move_reversal = matches!(
        preset,
        MARKET_MOMENTUM_OPPOSITE_MOVE_PRESET
            | MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_PRESET
            | MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_PRESET
            | MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_PRESET
            | MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_PRESET
            | MARKET_MOMENTUM_OPPOSITE_MOVE_CONFIRMED_REVERSAL_PRESET
            | MARKET_MOMENTUM_OPPOSITE_MOVE_MEAN_RECLAIM_PRESET
    );
    let strategy_key = if is_direct_kline_v36 {
        MARKET_MOMENTUM_DIRECT_KLINE_V36_STRATEGY_KEY
    } else if is_filtered_volume_rsi_ema_macd_trend_managed {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_STRATEGY_KEY
    } else if is_filtered_volume_rsi_ema_macd_v11 {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY
    } else if is_filtered_volume_rsi_ema_macd_v10 {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY
    } else if uses_filtered_volume_anchor_rsi {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY
    } else if is_filtered_volume_weekly_base {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY
    } else if is_filtered_volume_rsi_ema_macd {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY
    } else if is_rsi_volume_regime {
        MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY
    } else if is_opposite_move_reversal {
        MARKET_MOMENTUM_OPPOSITE_MOVE_STRATEGY_KEY
    } else if is_breakdown_short {
        MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_KEY
    } else {
        MARKET_VELOCITY_STRATEGY_KEY
    };
    let product_slug = if is_direct_kline_v36 {
        MARKET_MOMENTUM_DIRECT_KLINE_V36_PRODUCT_SLUG
    } else if is_filtered_volume_rsi_ema_macd_trend_managed {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRODUCT_SLUG
    } else if is_filtered_volume_rsi_ema_macd_v11 {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRODUCT_SLUG
    } else if is_filtered_volume_rsi_ema_macd_v10 {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRODUCT_SLUG
    } else if uses_filtered_volume_anchor_rsi {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRODUCT_SLUG
    } else if is_filtered_volume_weekly_base {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRODUCT_SLUG
    } else if is_filtered_volume_rsi_ema_macd {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRODUCT_SLUG
    } else if is_rsi_volume_regime {
        MARKET_RSI_VOLUME_REGIME_PRODUCT_SLUG
    } else if is_opposite_move_reversal {
        MARKET_MOMENTUM_OPPOSITE_MOVE_PRODUCT_SLUG
    } else if is_breakdown_short {
        MARKET_VELOCITY_BREAKDOWN_SHORT_PRODUCT_SLUG
    } else {
        MARKET_VELOCITY_PRODUCT_SLUG
    };
    let is_breakdown_short_live_cutover =
        is_breakdown_short && preset == MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_PRESET;
    let execution_json = if is_direct_kline_v36 {
        json!({
            "service_mode": "research_backtest_only",
            "source_signal_type": strategy_key,
            "promotion_eligible": false,
            "paper_observation_eligible": false,
            "live_handoff_eligible": false,
            "rejection_reason": "direct_kline_v36_failed_profitability_stability_and_drawdown_gates",
        })
    } else if is_filtered_volume_rsi_ema_macd {
        json!({
            "service_mode": "research_backtest_only",
            "source_signal_type": strategy_key,
            "promotion_eligible": false,
            "paper_observation_eligible": false,
            "live_handoff_eligible": false,
            "rejection_reason": "filtered_volume_rsi_ema_macd_requires_backtest_validation",
        })
    } else if is_rsi_volume_regime {
        json!({
            "service_mode": "research_backtest_only",
            "source_signal_type": strategy_key,
            "promotion_eligible": false,
            "paper_observation_eligible": false,
            "live_handoff_eligible": false,
            "rejection_reason": "rsi_volume_regime_requires_backtest_validation",
        })
    } else if is_opposite_move_reversal {
        json!({
            "service_mode": "research_backtest_only",
            "source_signal_type": strategy_key,
            "promotion_eligible": false,
            "paper_observation_eligible": false,
            "live_handoff_eligible": false,
            "rejection_reason": "market_momentum_opposite_move_requires_research_validation",
        })
    } else if is_breakdown_short_live_cutover {
        json!({
            "service_mode": "api_trade_enabled",
            "source_signal_type": strategy_key,
            "live_handoff": "market_velocity_live_handoff",
        })
    } else {
        json!({
            "service_mode": "signal_only",
            "source_signal_type": strategy_key,
            "paper_outcome_sink": "web",
        })
    };
    let filtered_volume_macd_branch = if is_filtered_volume_rsi_ema_macd_v2_plus {
        json!({
            "enabled": args.entry_filtered_volume_macd_zero_band_atr_multiplier.is_some()
                && args.entry_filtered_volume_macd_min_normalized_dif_improvement.is_some(),
            "momentum_value": "dif",
            "normalization": "dif_divided_by_close_at_pivot",
            "pivot_candidate_offset_candles": DIVERGENCE_PIVOT_WING_CANDLES,
            "strict_pivot_comparison": true,
            "reference_pivot_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
            "zero_band": "Z_x_ATR14_at_each_pivot",
            "zero_band_atr_multiplier_Z": args.entry_filtered_volume_macd_zero_band_atr_multiplier,
            "minimum_normalized_dif_improvement_D_min": args.entry_filtered_volume_macd_min_normalized_dif_improvement,
            "top_pivot_rsi_min_inclusive": RSI_OVERBOUGHT,
            "bottom_pivot_rsi_max_inclusive": RSI_OVERSOLD,
            "unconfigured_parameter_policy": "disable_macd_branch",
        })
    } else {
        json!({
            "momentum_value": "dif_divided_by_close",
            "rsi_neutral_block_inclusive": [MACD_RSI_NEUTRAL_MIN, MACD_RSI_NEUTRAL_MAX],
            "divergence_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
            "pivot_wing_candles": DIVERGENCE_PIVOT_WING_CANDLES,
        })
    };
    let mut fast_momentum_filters_json = json!({
        "entry_min_rsi": args.entry_min_rsi,
        "entry_max_rsi": args.entry_max_rsi,
        "entry_min_rsi_delta": args.entry_min_rsi_delta,
        "entry_rsi_delta_lookback_candles": args.entry_rsi_delta_lookback_candles,
        "entry_bollinger_breakout": args.entry_bollinger_breakout,
        "entry_min_bollinger_bandwidth_expansion_pct": args.entry_min_bollinger_bandwidth_expansion_pct,
        "entry_min_body_ratio_pct": args.entry_min_body_ratio_pct,
        "entry_min_close_position_pct": args.entry_min_close_position_pct,
        "entry_min_range_expansion_ratio": args.entry_min_range_expansion_ratio,
        "entry_extreme_volume_contrarian": args.entry_extreme_volume_contrarian,
        "entry_extreme_volume_continuation": args.entry_extreme_volume_continuation,
        "entry_rsi_volume_regime": args.entry_rsi_volume_regime,
        "entry_filtered_volume_rsi_ema_macd": args.entry_filtered_volume_rsi_ema_macd,
        "filtered_volume_rsi_ema_macd": if is_filtered_volume_weekly_base {
            json!({
                "completed_15m_candles_only": true,
                "entry_fill": if is_filtered_volume_rsi_ema_macd_v11 || is_filtered_volume_rsi_ema_macd_trend_managed {
                    "pivot_directional_wick_next_open_else_immediate_next_candle_intrabar_break"
                } else if is_filtered_volume_rsi_ema_macd_v10 {
                    "next_open_after_one_completed_confirmation_candle"
                } else {
                    "next_completed_15m_candle_open"
                },
                "entry_candle_protection_check": true,
                "weekly_volume_ccy_gate": {
                    "source": "per_symbol_15m_table.vol_ccy",
                    "history_candles": WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES,
                    "current_candle_in_history": false,
                    "requires_continuous_history": true,
                    "percentile_method": "nearest_rank",
                    "p90_zero_based_index": WEEKLY_VOLUME_CCY_P90_INDEX,
                    "comparison": "current_greater_than_or_equal_to_p90",
                    "same_symbol_only": true,
                },
                "volume_baseline": {
                    "history_candles": FILTERED_VOLUME_HISTORY_CANDLES,
                    "historical_marking_previous_raw_candles": FILTERED_VOLUME_HISTORY_CANDLES,
                    "historical_and_current_min_ratio": filtered_volume_min_ratio,
                    "exclude_marked_historical_spikes": true,
                    "minimum_retained_history_candles": FILTERED_VOLUME_MIN_RETAINED_CANDLES,
                    "current_candle_in_denominator": false,
                },
                "volume_gate_operator": "filtered_ratio_and_weekly_volume_ccy_p90",
                "rsi_branch": filtered_volume_rsi_branch,
                "ema_branch": {
                    "periods": [FILTERED_VOLUME_EMA_FAST_PERIOD, FILTERED_VOLUME_EMA_MIDDLE_PERIOD, FILTERED_VOLUME_EMA_SLOW_PERIOD],
                    "minimum_body_range_ratio": EMA_BODY_MIN_RANGE_RATIO,
                    "body_open_ratio_strict_between": [EMA_BODY_MIN_OPEN_RATIO, EMA_BODY_MAX_OPEN_RATIO],
                    "long_rsi_strict_below": RSI_OVERBOUGHT,
                    "short_rsi_strict_above": RSI_OVERSOLD,
                    "max_distance_from_ema144_atr": if is_filtered_volume_rsi_ema_macd_v5 { json!(EMA144_MAX_DISTANCE_ATR) } else { Value::Null },
                    "distance_gate_scope": if is_filtered_volume_rsi_ema_macd_v5 { json!("ema_continuation_branch_only") } else { Value::Null },
                },
                "macd_branch": filtered_volume_macd_branch,
                "same_direction_policy": "merge_and_enter_once",
                "opposing_direction_policy": "block_entry",
                "bollinger_conflict_buffer": if is_filtered_volume_rsi_ema_macd_v4 {
                    json!({
                        "period": BOLLINGER_CONFLICT_PERIOD,
                        "standard_deviation_multiplier": BOLLINGER_CONFLICT_STDDEV_MULTIPLIER,
                        "standard_deviation_mode": "population",
                        "price_source": "completed_candle_closes",
                        "lower_touch": "current_low_less_than_or_equal_to_lower_adds_long_only_against_existing_short",
                        "upper_touch": "current_high_greater_than_or_equal_to_upper_adds_short_only_against_existing_long",
                        "standalone_entry_allowed": false,
                        "both_directions_policy": "block_entry",
                    })
                } else {
                    Value::Null
                },
                "open_position_policy": {
                    "same_direction_signal": "ignore_without_refreshing_orders",
                    "opposite_direction_signal": "close_then_reverse",
                },
                "excluded_legacy_entry_logic": if is_filtered_volume_rsi_ema_macd_v4 {
                    json!(["96_bar_move", "bollinger_breakout", "bos", "fvg", "choch"])
                } else {
                    json!(["96_bar_move", "bollinger", "bos", "fvg", "choch"])
                },
                "stop_loss": {
                    "priority": ["rsi_engulfing_or_wick_structure", "actual_fill_atr14_x_1_5"],
                    "atr_period": FAST_MOMENTUM_ATR_PERIOD,
                    "atr_multiplier": FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
                    "fixed_percentage_cap_or_fallback": false,
                    "invalid_structure_stop_at_fill": "round_trip_at_fill_and_charge_both_sides_without_r",
                },
                "position_sizing": {
                    "account_risk_fraction_per_trade": 0.01,
                    "quantity_formula": "entry_equity_x_risk_fraction_divided_by_initial_stop_distance",
                    "exchange_quantity_rounding_applied": false,
                },
                "take_profit": {
                    "mode": if is_filtered_volume_rsi_ema_macd_trend_managed { "trend_relation_then_filtered_volume_atr" } else { "fixed_atr_distance_by_filtered_volume_ratio" },
                    "independent_of_stop_distance": true,
                },
                "trend_managed_exit": if is_filtered_volume_rsi_ema_macd_trend_managed {
                    json!({
                        "frozen_at": "anchor_p_completed_close",
                        "long_term_ema_periods": [
                            FILTERED_VOLUME_EMA_FAST_PERIOD,
                            FILTERED_VOLUME_EMA_MIDDLE_PERIOD,
                            FILTERED_VOLUME_EMA_SECOND_MIDDLE_PERIOD,
                            FILTERED_VOLUME_EMA_SLOW_PERIOD
                        ],
                        "long_term_confirmation_candles": 3,
                        "ema696_each_candle_must_move_in_trend_direction": true,
                        "platform_lookback_candles": TREND_PLATFORM_LOOKBACK_CANDLES,
                        "platform_max_range_atr": TREND_PLATFORM_MAX_RANGE_ATR,
                        "platform_confirmation_candles": TREND_PLATFORM_CONFIRMATION_CANDLES,
                        "platform_min_body_range_ratio": TREND_PLATFORM_MIN_BODY_RANGE_RATIO,
                        "platform_min_body_open_ratio": TREND_PLATFORM_MIN_BODY_OPEN_RATIO,
                        "platform_break_requires_filtered_ratio_and_weekly_p90": true,
                        "trend_sources_policy": "platform_or_long_term_ema",
                        "opposing_trend_conflict_policy": "neutral",
                        "countertrend_target_atr": if is_filtered_volume_rsi_ema_macd_v13 {
                            V13_COUNTERTREND_TARGET_ATR_MULTIPLIER
                        } else {
                            V12_COUNTERTREND_TARGET_ATR_MULTIPLIER
                        },
                        "countertrend_exception_volume_ratio": COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO,
                        "countertrend_exception_lookback_candles": COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES,
                        "countertrend_exception_min_directional_net_move_pct": COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT,
                    })
                } else {
                    Value::Null
                },
                "trailing_stop": if is_filtered_volume_rsi_ema_macd_trend_managed {
                    json!({
                        "completed_candles_after_entry_only": true,
                        "filtered_volume_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                        "weekly_p90_required": false,
                        "first_level": "cost_adjusted_true_break_even",
                        "later_levels": "entry_plus_or_minus_incrementing_frozen_atr",
                        "price_close_must_be_beyond_proposed_stop": true,
                        "failed_price_gate_consumes_level": false,
                        "stop_must_remain_strictly_before_take_profit": true,
                        "new_stop_effective_from_next_candle": true,
                    })
                } else {
                    json!(false)
                },
                "maximum_holding_time": Value::Null,
            })
        } else if is_filtered_volume_rsi_ema_macd {
            json!({
                "completed_15m_candles_only": true,
                "volume_baseline": {
                    "history_candles": FILTERED_VOLUME_HISTORY_CANDLES,
                    "historical_marking_previous_raw_candles": FILTERED_VOLUME_HISTORY_CANDLES,
                    "historical_and_current_min_ratio": FILTERED_VOLUME_MIN_RATIO,
                    "exclude_marked_historical_spikes": true,
                    "minimum_retained_history_candles": FILTERED_VOLUME_MIN_RETAINED_CANDLES,
                    "current_candle_in_denominator": false,
                },
                "rsi_branch": {
                    "oversold_strict_below": RSI_OVERSOLD,
                    "overbought_strict_above": RSI_OVERBOUGHT,
                    "divergence_lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                    "pivot_wing_candles": DIVERGENCE_PIVOT_WING_CANDLES,
                    "minimum_rsi_improvement": RSI_DIVERGENCE_MIN_DELTA,
                    "reversal_wick_min_full_range_ratio": REVERSAL_WICK_MIN_RANGE_RATIO,
                    "doji_max_body_range_ratio": DOJI_MAX_BODY_RANGE_RATIO,
                    "engulfing_rule_enabled": false,
                },
                "ema_branch": {
                    "periods": [FILTERED_VOLUME_EMA_FAST_PERIOD, FILTERED_VOLUME_EMA_MIDDLE_PERIOD, FILTERED_VOLUME_EMA_SLOW_PERIOD],
                    "minimum_body_range_ratio": EMA_BODY_MIN_RANGE_RATIO,
                    "body_open_ratio_strict_between": [EMA_BODY_MIN_OPEN_RATIO, EMA_BODY_MAX_OPEN_RATIO],
                    "long_rsi_strict_below": RSI_OVERBOUGHT,
                    "short_rsi_strict_above": RSI_OVERSOLD,
                },
                "macd_branch": filtered_volume_macd_branch,
                "same_direction_policy": "merge_and_enter_once",
                "opposing_direction_policy": "block_entry",
                "excluded_legacy_entry_logic": ["96_bar_move", "bollinger", "bos", "fvg", "choch"],
                "stop_loss": {
                    "indicator": "atr",
                    "period": FAST_MOMENTUM_ATR_PERIOD,
                    "multiplier": FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
                    "fixed_percentage_cap_or_fallback": false,
                }
            })
        } else { Value::Null },
        "rsi_volume_regime": if args.entry_rsi_volume_regime {
            if is_rsi_volume_regime_v5 {
                json!({
                    "active_entry_branches": ["rsi_divergence", "opposite_96_net_move"],
                    "sideways_breakout_enabled": false,
                    "rsi_oversold_strict_below": RSI_VOLUME_OVERSOLD,
                    "rsi_overbought_strict_above": RSI_VOLUME_OVERBOUGHT,
                    "volume_baseline": {
                        "recent_history_candles": RSI_VOLUME_V5_LOOKBACK_CANDLES,
                        "historical_spike_marking_lookback_candles": RSI_VOLUME_V5_LOOKBACK_CANDLES,
                        "historical_spike_min_ratio": RSI_VOLUME_V5_MIN_RATIO,
                        "historical_spike_marking_uses_raw_previous_ten_average": true,
                        "historical_spike_marking_excludes_earlier_spikes": false,
                        "exclude_marked_historical_spikes_from_current_average": true,
                        "current_candle_in_average": false,
                        "current_candle_kept_as_ratio_numerator": true,
                        "minimum_current_volume_ratio": RSI_VOLUME_V5_MIN_RATIO,
                        "empty_filtered_history_policy": "block_entry",
                    },
                    "divergence_lookback_candles": RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES,
                    "divergence_pivot_wing_candles": RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES,
                    "divergence_min_rsi_delta": RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA,
                    "divergence_pairing": "same_historical_price_pivot_rsi",
                    "divergence_requires_current_rsi_extreme": true,
                    "opposite_history_lookback_candles": RSI_VOLUME_TREND_LOOKBACK_CANDLES,
                    "opposite_history_min_net_move_pct": RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT,
                    "opposite_history_operator": "net_move_only",
                    "opposite_history_requires_current_rsi_extreme": false,
                    "opposite_history_requires_directional_candle": false,
                    "opposite_history_mirrored_both_sides": true,
                    "same_direction_branch_policy": "join_reasons_and_enter_once",
                    "opposing_direction_branch_policy": "block_entry",
                    "opposing_wick_max_body_multiple": RSI_VOLUME_V3_OPPOSING_WICK_MAX_BODY_MULTIPLE,
                    "stop_loss": {
                        "indicator": "atr",
                        "period": RSI_VOLUME_V3_STOP_ATR_PERIOD,
                        "multiplier": RSI_VOLUME_V3_STOP_ATR_MULTIPLIER,
                        "fixed_percentage_fallback": false,
                    },
                })
            } else if is_rsi_volume_regime_v4 {
                json!({
                    "active_entry_branches": ["rsi_divergence", "opposite_96_net_move"],
                    "sideways_breakout_enabled": false,
                    "rsi_oversold_strict_below": RSI_VOLUME_OVERSOLD,
                    "rsi_overbought_strict_above": RSI_VOLUME_OVERBOUGHT,
                    "volume_lookback_candles": RSI_VOLUME_V3_LOOKBACK_CANDLES,
                    "minimum_volume_ratio": RSI_VOLUME_V3_MIN_RATIO,
                    "divergence_lookback_candles": RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES,
                    "divergence_pivot_wing_candles": RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES,
                    "divergence_min_rsi_delta": RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA,
                    "divergence_pairing": "same_historical_price_pivot_rsi",
                    "divergence_requires_current_rsi_extreme": true,
                    "opposite_history_lookback_candles": RSI_VOLUME_TREND_LOOKBACK_CANDLES,
                    "opposite_history_min_net_move_pct": RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT,
                    "opposite_history_operator": "net_move_only",
                    "opposite_history_requires_current_rsi_extreme": false,
                    "opposite_history_requires_directional_candle": false,
                    "opposite_history_mirrored_both_sides": true,
                    "same_direction_branch_policy": "join_reasons_and_enter_once",
                    "opposing_direction_branch_policy": "block_entry",
                    "opposing_wick_max_body_multiple": RSI_VOLUME_V3_OPPOSING_WICK_MAX_BODY_MULTIPLE,
                    "stop_loss": {
                        "indicator": "atr",
                        "period": RSI_VOLUME_V3_STOP_ATR_PERIOD,
                        "multiplier": RSI_VOLUME_V3_STOP_ATR_MULTIPLIER,
                        "fixed_percentage_fallback": false,
                    },
                })
            } else if is_rsi_volume_regime_v3 {
                json!({
                    "rsi_oversold_strict_below": RSI_VOLUME_OVERSOLD,
                    "rsi_overbought_strict_above": RSI_VOLUME_OVERBOUGHT,
                    "volume_lookback_candles": RSI_VOLUME_V3_LOOKBACK_CANDLES,
                    "minimum_volume_ratio": RSI_VOLUME_V3_MIN_RATIO,
                    "divergence_lookback_candles": RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES,
                    "divergence_pivot_wing_candles": RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES,
                    "divergence_min_rsi_delta": RSI_VOLUME_V3_DIVERGENCE_MIN_RSI_DELTA,
                    "divergence_pairing": "same_historical_price_pivot_rsi",
                    "divergence_requires_current_rsi_extreme": true,
                    "sideways_classifier": "previous_bollinger_bandwidth_low_percentile_and_macd_lines_near_zero",
                    "sideways_context_lookback_candles": RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES,
                    "sideways_bandwidth_percentile": RSI_VOLUME_NARROW_BAND_PERCENTILE,
                    "sideways_macd_max_abs_pct_of_price": RSI_VOLUME_MACD_NEAR_ZERO_MAX_PCT,
                    "sideways_breakout_requires_current_rsi_extreme": false,
                    "sideways_breakout_level": "previous_bollinger_band",
                    "sideways_breakout_requires_directional_candle": true,
                    "opposite_history_lookback_candles": RSI_VOLUME_TREND_LOOKBACK_CANDLES,
                    "opposite_history_min_net_move_pct": RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT,
                    "opposite_history_operator": "net_move_only",
                    "opposite_history_requires_current_rsi_extreme": false,
                    "opposite_history_requires_directional_candle": false,
                    "opposite_history_mirrored_both_sides": true,
                    "same_direction_branch_policy": "join_reasons_and_enter_once",
                    "opposing_direction_branch_policy": "block_entry",
                    "opposing_wick_max_body_multiple": RSI_VOLUME_V3_OPPOSING_WICK_MAX_BODY_MULTIPLE,
                    "stop_loss": {
                        "indicator": "atr",
                        "period": RSI_VOLUME_V3_STOP_ATR_PERIOD,
                        "multiplier": RSI_VOLUME_V3_STOP_ATR_MULTIPLIER,
                        "fixed_percentage_fallback": false,
                    },
                })
            } else if is_rsi_volume_regime_v2 {
                json!({
                    "rsi_oversold_strict_below": RSI_VOLUME_OVERSOLD,
                    "rsi_overbought_strict_above": RSI_VOLUME_OVERBOUGHT,
                    "sideways_classifier": "previous_bollinger_bandwidth_low_percentile_and_macd_lines_near_zero",
                    "sideways_context_lookback_candles": RSI_VOLUME_SIDEWAYS_CONTEXT_LOOKBACK_CANDLES,
                    "sideways_bandwidth_percentile": RSI_VOLUME_NARROW_BAND_PERCENTILE,
                    "sideways_macd_max_abs_pct_of_price": RSI_VOLUME_MACD_NEAR_ZERO_MAX_PCT,
                    "sideways_breakout_requires_current_rsi_extreme": true,
                    "sideways_breakout_level": "previous_bollinger_band",
                    "sideways_stop": "opposite_previous_bollinger_band",
                    "divergence_lookback_candles": RSI_VOLUME_DIVERGENCE_LOOKBACK_CANDLES,
                    "divergence_pivot_wing_candles": RSI_VOLUME_DIVERGENCE_PIVOT_WING_CANDLES,
                    "divergence_min_rsi_delta": RSI_VOLUME_DIVERGENCE_MIN_RSI_DELTA,
                    "divergence_pairing": "same_historical_price_pivot_rsi",
                    "divergence_requires_current_rsi_extreme": false,
                    "branch_priority": ["rsi_divergence", "sideways_breakout", "opposite_history_reversal"],
                    "opposite_history_lookback_candles": RSI_VOLUME_TREND_LOOKBACK_CANDLES,
                    "opposite_history_min_net_move_pct": RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT,
                    "opposite_history_min_r_squared": RSI_VOLUME_MIN_TREND_R_SQUARED,
                    "opposite_history_operator": "net_move_or_linear_trend",
                    "volume_lookback_candles": RSI_VOLUME_LOOKBACK_CANDLES,
                    "minimum_volume_ratio": RSI_VOLUME_MIN_RATIO,
                    "opposing_wick_max_body_multiple": 1.0,
                })
            } else {
                json!({
                    "rsi_oversold_strict_below": RSI_VOLUME_OVERSOLD,
                    "rsi_overbought_strict_above": RSI_VOLUME_OVERBOUGHT,
                    "sideways_previous_rsi_inclusive": [RSI_VOLUME_SIDEWAYS_RSI_MIN, RSI_VOLUME_SIDEWAYS_RSI_MAX],
                    "opposite_history_lookback_candles": RSI_VOLUME_TREND_LOOKBACK_CANDLES,
                    "opposite_history_min_net_move_pct": RSI_VOLUME_MIN_OPPOSITE_MOVE_PCT,
                    "opposite_history_min_r_squared": RSI_VOLUME_MIN_TREND_R_SQUARED,
                    "opposite_history_operator": "net_move_or_linear_trend",
                    "volume_lookback_candles": RSI_VOLUME_LOOKBACK_CANDLES,
                    "minimum_volume_ratio": RSI_VOLUME_MIN_RATIO,
                    "opposing_wick_max_body_multiple": 1.0,
                    "sideways_stop": "previous_two_candle_range_boundary",
                })
            }
        } else {
            Value::Null
        },
        "entry_relative_volume_at_time_10d": args.entry_relative_volume_at_time_10d,
        "entry_min_recent_drawdown_pct": args.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": args.entry_recent_drawdown_lookback_candles,
        "entry_opposite_move_lookback_candles": args.entry_opposite_move_lookback_candles,
        "entry_min_opposite_net_move_pct": args.entry_min_opposite_net_move_pct,
        "entry_min_opposite_duration_candles": args.entry_min_opposite_duration_candles,
        "entry_opposite_duration_min_r_squared": args.entry_opposite_duration_min_r_squared,
        "entry_min_exhaustion_volume_dominance_ratio": args.entry_min_exhaustion_volume_dominance_ratio,
        "entry_btc_96_max_abs_net_move_pct": args.entry_btc_96_max_abs_net_move_pct,
        "entry_exhaustion_volume_lookback_candles": EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
        "entry_exhaustion_current_cluster_candles": EXHAUSTION_CURRENT_CLUSTER_CANDLES,
        "entry_exhaustion_swing_radius_candles": EXHAUSTION_SWING_RADIUS_CANDLES,
        "entry_defer_bearish_continuation": args.entry_defer_bearish_continuation,
        "entry_defer_bullish_continuation": args.entry_defer_bullish_continuation,
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
    });
    if let (Some(filters), Some(minimum)) = (
        fast_momentum_filters_json.as_object_mut(),
        args.entry_btc_384_min_directional_net_move_pct,
    ) {
        filters.insert(
            "entry_btc_384_min_directional_net_move_pct".to_string(),
            json!(minimum),
        );
    }
    if args.entry_btc_require_current_directional_candle {
        if let Some(filters) = fast_momentum_filters_json.as_object_mut() {
            filters.insert(
                "entry_btc_require_current_directional_candle".to_string(),
                json!(true),
            );
        }
    }
    let manifest_json = json!({
        "manifest_schema_version": 1,
        "strategy_key": strategy_key,
        "strategy_family": strategy_key,
        "preset": preset,
        "rule_version": args.paper_outcome_entry_rule_version,
        "product": {
            "slug": product_slug,
            "symbol": "ALL",
            "timeframe": "15m",
        },
        "execution": execution_json,
        "parameters": {
            "event_source": args.event_source.label(),
            "kline_volume_rank_velocity": args.kline_volume_rank_velocity,
            "kline_volume_rank_require_turnover_growth": args.kline_volume_rank_require_turnover_growth,
            "kline_volume_rank_require_consecutive_improvement": args.kline_volume_rank_require_consecutive_improvement,
            "kline_current_live_only": args.kline_current_live_only,
            "kline_volume_rank_lookback_candles": if args.kline_volume_rank_velocity { json!(96) } else { Value::Null },
            "kline_volume_rank_quote_turnover": if args.kline_volume_rank_velocity { "vol_ccy_x_close" } else { "off" },
            "trade_direction": args.trade_direction.label(),
            "stop_loss_pct": if uses_rsi_volume_atr_contract { Value::Null } else { json!(args.stop_loss_pct) },
            "stop_loss_mode": if is_filtered_volume_rsi_ema_macd_v11 || is_filtered_volume_rsi_ema_macd_trend_managed { "actual_fill_atr14_x_1_5" } else if is_filtered_volume_weekly_base { "rsi_pattern_structure_else_actual_fill_atr14_x_1_5" } else if uses_rsi_volume_atr_contract { "atr14_x_1_5" } else { args.stop_loss_mode.label() },
            "structure_stop_min_pct": if uses_rsi_volume_atr_contract { Value::Null } else { json!(args.structure_stop_min_pct) },
            "target_r": if is_filtered_volume_weekly_base { Value::Null } else { json!(args.target_rs.first().copied()) },
            "entry_fill": if is_filtered_volume_rsi_ema_macd_v11 || is_filtered_volume_rsi_ema_macd_trend_managed {
                "pivot_directional_wick_next_open_else_immediate_next_candle_intrabar_break"
            } else if is_filtered_volume_rsi_ema_macd_v10 {
                "next_open_after_one_completed_confirmation_candle"
            } else if is_filtered_volume_weekly_base {
                "next_15m_open"
            } else {
                "signal_close"
            },
            "take_profit": if is_filtered_volume_rsi_ema_macd_trend_managed {
                json!({
                    "mode": "signal_frozen_trend_relation_then_fixed_atr_distance",
                    "atr_period": FAST_MOMENTUM_ATR_PERIOD,
                    "countertrend_default_target_atr_multiplier": if is_filtered_volume_rsi_ema_macd_v13 {
                        V13_COUNTERTREND_TARGET_ATR_MULTIPLIER
                    } else {
                        V12_COUNTERTREND_TARGET_ATR_MULTIPLIER
                    },
                    "countertrend_exception": {
                        "min_signal_filtered_volume_ratio": COUNTERTREND_EXCEPTION_MIN_VOLUME_RATIO,
                        "lookback_candles_excluding_p": COUNTERTREND_EXCEPTION_LOOKBACK_CANDLES,
                        "min_directional_net_move_pct": COUNTERTREND_EXCEPTION_MIN_NET_MOVE_PCT,
                    },
                    "volume_tiers": [
                        {"min_volume_ratio": filtered_volume_min_ratio, "max_volume_ratio_exclusive": 4.0, "target_atr_multiplier": 2.7},
                        {"min_volume_ratio": 4.0, "max_volume_ratio_exclusive": 6.0, "target_atr_multiplier": 3.6},
                        {"min_volume_ratio": 6.0, "max_volume_ratio_exclusive": Value::Null, "target_atr_multiplier": 4.5}
                    ],
                    "holding_volume_trailing": {
                        "min_filtered_volume_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                        "weekly_p90_required": false,
                        "first_level": "cost_adjusted_true_break_even",
                        "subsequent_levels_frozen_atr": [1.0, 2.0, "increment_by_one"],
                        "effective_from_next_completed_candle": true,
                    }
                })
            } else if is_filtered_volume_weekly_base {
                json!({
                    "mode": "fixed_atr_distance_by_filtered_volume_ratio",
                    "atr_period": FAST_MOMENTUM_ATR_PERIOD,
                    "independent_of_actual_stop_distance": true,
                    "tiers": [
                        {"min_volume_ratio": filtered_volume_min_ratio, "max_volume_ratio_exclusive": 4.0, "target_atr_multiplier": 2.7},
                        {"min_volume_ratio": 4.0, "max_volume_ratio_exclusive": 6.0, "target_atr_multiplier": 3.6},
                        {"min_volume_ratio": 6.0, "max_volume_ratio_exclusive": Value::Null, "target_atr_multiplier": 4.5}
                    ]
                })
            } else if is_filtered_volume_rsi_ema_macd {
                json!({
                    "mode": "filtered_volume_tiered_atr",
                    "stop_atr_multiplier": FILTERED_VOLUME_STOP_ATR_MULTIPLIER,
                    "tiers": [
                        {"min_volume_ratio": 3.0, "max_volume_ratio_exclusive": 4.0, "target_r": 1.8, "target_atr_multiplier": 2.7},
                        {"min_volume_ratio": 4.0, "max_volume_ratio_exclusive": 6.0, "target_r": 2.4, "target_atr_multiplier": 3.6},
                        {"min_volume_ratio": 6.0, "max_volume_ratio_exclusive": Value::Null, "target_r": 3.0, "target_atr_multiplier": 4.5}
                    ]
                })
            } else if args.volume_atr_take_profit {
                json!({
                    "mode": "volume_atr",
                    "atr_period": 14,
                    "volume_average_candles": 20,
                    "target_scale": args.volume_atr_target_scale,
                    "min_target_r": args.volume_atr_min_target_r,
                    "max_target_r": args.volume_atr_max_target_r,
                    "tiers": [
                        {"min_volume_ratio": 1.5, "atr_multiplier": 1.5},
                        {"min_volume_ratio": 2.0, "atr_multiplier": 2.0},
                        {"min_volume_ratio": 3.0, "atr_multiplier": 3.0}
                    ]
                })
            } else {
                json!({"mode": "fixed_r", "target_r": args.target_rs.first().copied()})
            },
            "entry_period": args.entry_period,
            "entry_max_distance_pct": args.entry_max_distance_pct,
            "entry_min_volume_ratio": args.entry_min_volume_ratio,
            "fast_momentum_filters": fast_momentum_filters_json,
            "entry_max_signal_pullback_pct": args.entry_max_signal_pullback_pct,
            "entry_max_gap_without_retest_pct": args.entry_max_gap_without_retest_pct,
            "entry_retest_tolerance_pct": args.entry_retest_tolerance_pct,
            "entry_retest_after_signal": args.entry_retest_after_signal,
            "cost_model": {
                "fee_bps_per_side": args.backtest_fee_bps_per_side,
                "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
                "slippage_model": "equivalent_proportional_trade_cost"
            },
            "entry_retest_max_wait_candles": args.entry_retest_max_wait_candles,
            "entry_retest_min_entry_open_gap_pct": args.entry_retest_min_entry_open_gap_pct,
            "entry_retest_open_fade_min_volume_ratio": args.entry_retest_open_fade_min_volume_ratio,
            "entry_defer_long_lower_wick_reversal": args.entry_defer_long_lower_wick_reversal,
            "entry_long_bullish_hammer_reversal": args.entry_long_bullish_hammer_reversal,
            "entry_require_two_stage_recovery": args.entry_require_two_stage_recovery,
            "entry_require_macd_negative_histogram_improving": args.entry_require_macd_negative_histogram_improving,
            "trend_timeframe": args.trend_timeframe.label(),
            "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
            "min_delta_rank": args.min_delta_rank,
            "max_delta_rank": args.max_delta_rank,
            "min_price_change_pct": args.min_price_change_pct,
            "max_price_change_pct": args.max_price_change_pct,
            "stop_reentry_mode": args.stop_reentry_mode.label(),
            "fvg_entry_mode": args.fvg_entry_mode.label(),
            "fvg_max_wait_candles": args.fvg_max_wait_candles,
            "fvg_impulse_retrace_fill_pct": args.fvg_impulse_retrace_fill_pct,
            "fvg_impulse_retrace_min_wait_candles": args.fvg_impulse_retrace_min_wait_candles,
            "runner_target_r": args.runner_target_r,
            "runner_fraction": args.runner_fraction,
            "runner_stop_r": args.runner_stop_r,
            "ignore_entry_signal_updates_while_open": args.ignore_entry_signal_updates_while_open,
        },
        "filters": {
            "entry_trigger_allowlist": args.entry_trigger_allowlist,
            "entry_trigger_blocklist": args.entry_trigger_blocklist,
            "entry_trigger_filter_version": entry_trigger_filter_version_label(
                has_allowlist,
                has_blocklist,
            ),
            "entry_trigger_allowlist_label": allowlist_label,
            "entry_trigger_blocklist_label": blocklist_label,
            "symbol_blocklist": args.symbol_blocklist,
        },
    });
    let canonical_json = canonical_manifest_json(&manifest_json)?;
    let (channel, manifest_status) = if is_direct_kline_v36
        || is_rsi_volume_regime
        || is_filtered_volume_rsi_ema_macd
        || is_opposite_move_reversal
    {
        ("research", "research")
    } else if is_breakdown_short_live_cutover {
        ("production_default", "production")
    } else if is_breakdown_short {
        ("paper_observing", "paper_observing")
    } else {
        ("production_default", "production")
    };
    Ok(MarketVelocityPresetManifest {
        product_slug: product_slug.to_string(),
        symbol: "ALL".to_string(),
        channel: channel.to_string(),
        manifest_hash: sha256_manifest_hash(&canonical_json),
        strategy_key: strategy_key.to_string(),
        human_label: human_label_for_preset(preset).to_string(),
        risk_level: "high".to_string(),
        manifest_status: manifest_status.to_string(),
        manifest_json,
        canonical_json,
    })
}

/// 为三个互斥入场家族生成独立 Research-only manifest，禁止继承旧混合分支描述。
fn isolated_entry_family_manifest(
    preset: &str,
    args: &super::MarketVelocityEventBacktestArgs,
) -> Result<MarketVelocityPresetManifest> {
    let rule_version = args.paper_outcome_entry_rule_version.as_str();
    let strategy_key = isolated_entry_family_strategy_key(rule_version)
        .expect("caller verifies isolated strategy version");
    let product_slug = isolated_entry_family_product_slug(rule_version)
        .expect("isolated strategy identity must include product slug");
    let (family, hypothesis, human_label, signal, entry_fill) = match rule_version {
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION => (
            "momentum_exhaustion_reversal",
            MOMENTUM_EXHAUSTION_HYPOTHESIS,
            "Market momentum exhaustion reversal isolated 15m research v1",
            json!({
                "prior_net_move_lookback_candles_excluding_signal": MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES,
                "minimum_absolute_net_move_pct": MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
                "decline_creates": "long",
                "rise_creates": "short",
                "price_confirmation": "directional_wick_or_immediate_next_candle_extreme_break",
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
            "Market momentum exhaustion reversal isolated 15m research v2",
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
            "Market momentum exhaustion reversal isolated 15m research v3 wick55",
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
            "Market volume-anchor RSI-divergence reversal isolated 15m research v1",
            json!({
                "comparison_mode": ISOLATED_RSI_COMPARISON_MODE,
                "lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                "nearest_qualified_anchor_only": true,
                "long_rsi_max_inclusive": RSI_OVERSOLD,
                "short_rsi_min_inclusive": RSI_OVERBOUGHT,
                "equal_rsi_allowed": true,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
                "historical_net_move_used": false,
            }),
            "directional_wick_next_open_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION => (
            "volume_anchor_rsi_divergence",
            VOLUME_ANCHOR_RSI_V2_HYPOTHESIS,
            "Market volume-anchor RSI-divergence reversal isolated 15m research v2",
            json!({
                "comparison_mode": ISOLATED_RSI_V2_COMPARISON_MODE,
                "lookback_candles": DIVERGENCE_LOOKBACK_CANDLES,
                "nearest_qualified_anchor_only": true,
                "fallback_to_older_anchor": false,
                "minimum_strictly_intervening_candles": MIN_INTERVENING_CANDLES,
                "long_rsi_max_inclusive": RSI_OVERSOLD,
                "short_rsi_min_inclusive": RSI_OVERBOUGHT,
                "bullish_anchor_invalid_if_intermediate_rsi_strictly_above": BULLISH_ANCHOR_RESET_RSI,
                "bearish_anchor_invalid_if_intermediate_rsi_strictly_below": BEARISH_ANCHOR_RESET_RSI,
                "intermediate_rsi_missing_policy": "fail_closed",
                "equal_rsi_allowed": true,
                "macd_used": false,
                "ema_used": false,
                "platform_used": false,
                "historical_net_move_used": false,
            }),
            "directional_wick_next_open_else_immediate_next_candle_intrabar_break",
        ),
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION => (
            "volume_platform_break_trend",
            PLATFORM_BREAK_TREND_HYPOTHESIS,
            "Market volume platform-break trend isolated 15m research v1",
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
            "Market volume horizontal platform-break trend isolated 15m research v2",
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
        _ => unreachable!("caller verifies isolated strategy version"),
    };
    let uses_volume_tier_exit = uses_momentum_exhaustion_volume_tier_exit(rule_version);
    let manifest_json = json!({
        "manifest_schema_version": 1,
        "strategy_key": strategy_key,
        "strategy_family": family,
        "preset": preset,
        "rule_version": rule_version,
        "hypothesis": hypothesis,
        "product": {
            "slug": product_slug,
            "symbol": "ALL",
            "timeframe": "15m",
        },
        "execution": {
            "service_mode": "research_backtest_only",
            "promotion_eligible": false,
            "paper_observation_eligible": false,
            "live_handoff_eligible": false,
            "rejection_reason": "isolated_entry_family_requires_backtest_and_out_of_sample_validation",
        },
        "parameters": {
            "event_source": args.event_source.label(),
            "completed_candles_only": true,
            "trade_direction": args.trade_direction.label(),
            "signal": signal,
            "volume_gate": {
                "filtered_volume_min_ratio": FILTERED_VOLUME_V9_MIN_RATIO,
                "history_candles": FILTERED_VOLUME_HISTORY_CANDLES,
                "minimum_retained_candles": FILTERED_VOLUME_MIN_RETAINED_CANDLES,
                "current_candle_in_average": false,
                "weekly_volume_ccy_lookback_candles": WEEKLY_VOLUME_CCY_LOOKBACK_CANDLES,
                "weekly_volume_ccy_nearest_rank_p90_zero_based_index": WEEKLY_VOLUME_CCY_P90_INDEX,
            },
            "entry_fill": entry_fill,
            "risk": {
                "initial_stop": "actual_fill_plus_or_minus_atr14_x_1_5",
                "take_profit": if uses_volume_tier_exit {
                    "actual_fill_plus_or_minus_signal_atr14_x_2_7_3_6_or_4_5"
                } else {
                    "actual_fill_plus_or_minus_atr14_x_1_5"
                },
                "gross_target_r": if uses_volume_tier_exit { Value::Null } else { json!(1.0) },
                "gross_target_r_by_filtered_volume_ratio": if uses_volume_tier_exit {
                    json!({
                        "2.5_to_below_4": 1.8,
                        "4_to_below_6": 2.4,
                        "6_or_above": 3.0,
                    })
                } else {
                    Value::Null
                },
                "account_risk_fraction_per_trade": 0.01,
                "maximum_holding_hours": args.equity_max_holding_hours,
                "fee_bps_per_side": args.backtest_fee_bps_per_side,
                "slippage_bps_per_side": args.backtest_slippage_bps_per_side,
                "profit_protection": false,
                "volume_trailing_stop": false,
            },
            "excluded_entry_logic": [
                "market_rank_events",
                "episodes",
                "bollinger",
                "bos",
                "fvg",
                "choch",
                "legacy_mixed_rsi_ema_macd",
            ],
        },
        "evaluation": {
            "top60_current_live_is_diagnostic_only": true,
            "out_of_sample_required_before_promotion": true,
            "entry_quality_horizons_candles": [1, 2, 4, 8, 16, 32],
            "primary_path_test": "plus_1r_before_minus_1r_and_mfe_mae",
        },
    });
    let canonical_json = canonical_manifest_json(&manifest_json)?;
    Ok(MarketVelocityPresetManifest {
        product_slug: product_slug.to_string(),
        symbol: "ALL".to_string(),
        channel: "research".to_string(),
        manifest_hash: sha256_manifest_hash(&canonical_json),
        strategy_key: strategy_key.to_string(),
        human_label: human_label.to_string(),
        risk_level: "high".to_string(),
        manifest_status: "research".to_string(),
        manifest_json,
        canonical_json,
    })
}

fn human_label_for_preset(preset: &str) -> &str {
    match preset {
        MARKET_MOMENTUM_OPPOSITE_MOVE_PRESET => {
            "Market Momentum opposite net move 10% N192 volume-tiered ATR both-side 15m v1"
        }
        MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_PRESET => {
            "Market Momentum opposite net move 10% N192 volume-tiered ATR deferred long 15m v2"
        }
        MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_PRESET => {
            "Market Momentum opposite net move 10% N192 or regression duration N96 R2 0.7 volume-tiered ATR both-side 15m v3"
        }
        MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_PRESET => {
            "Market Momentum opposite move v4 with exhaustion volume dominance"
        }
        MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_PRESET => {
            "Market Momentum opposite move v5 with 1.8R-3.0R volume ATR target band"
        }
        MARKET_MOMENTUM_OPPOSITE_MOVE_CONFIRMED_REVERSAL_PRESET => {
            "Market Momentum opposite move v6 with symmetric price reversal confirmation"
        }
        MARKET_MOMENTUM_OPPOSITE_MOVE_MEAN_RECLAIM_PRESET => {
            "Market Momentum opposite move v7 with EMA20 and SMA20 reversal reclaim"
        }
        MARKET_MOMENTUM_DIRECT_KLINE_V36_PRESET => {
            "Market Momentum direct completed-15m-kline long reversal v36 frozen"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET => {
            "Market filtered-volume RSI/EMA/MACD both-side 15m research v1"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET => {
            "Market filtered-volume RSI/EMA/MACD confirmed-pivot both-side 15m research v2"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET => {
            "Market weekly-base-volume filtered-volume RSI/EMA/MACD structure-risk 15m research v3"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET => {
            "Market weekly-base-volume filtered-volume RSI/EMA/MACD BB12x2.6 conflict-buffer 15m research v4"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET => {
            "Market weekly-base-volume filtered-volume RSI/EMA/MACD EMA144 one-ATR proximity 15m research v5"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET => {
            "Market filtered-volume 2.5 weekly-P90 anchor RSI-divergence 15m research v9"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET => {
            "Market filtered-volume 2.5 anchor RSI-divergence next-close-confirmed 15m research v10"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET => {
            "Market filtered-volume 2.5 anchor RSI-divergence wick-or-next-touch 15m research v11"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET => {
            "Market filtered-volume 2.5 anchor RSI-divergence trend-managed exit 15m research v12"
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET => {
            "Market filtered-volume 2.5 anchor RSI-divergence trend-managed countertrend-1.5-ATR exit 15m research v13"
        }
        MARKET_RSI_VOLUME_REGIME_V1_PRESET => {
            "Market RSI14 volume reversal and sideways breakout both-side 15m v1"
        }
        MARKET_RSI_VOLUME_REGIME_V2_PRESET => {
            "Market RSI14 volume Bollinger/MACD breakout and causal divergence both-side 15m v2"
        }
        MARKET_RSI_VOLUME_REGIME_V3_PRESET => {
            "Market RSI14 divergence, sideways breakout and net-8% reversal with ATR stop 15m v3"
        }
        MARKET_RSI_VOLUME_REGIME_V4_PRESET => {
            "Market RSI14 divergence and net-8% reversal without sideways breakout, ATR stop 15m v4"
        }
        MARKET_RSI_VOLUME_REGIME_V5_PRESET => {
            "Market RSI14 divergence and net-8% reversal with filtered 10-candle volume baseline, ATR stop 15m v5"
        }
        "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1" => {
            "Market Velocity 0.0375SL 1.7R reclaim/MA/pullback delta18-42 pchg5-10 v1"
        }
        "research_momentum_0375sl_27r_reclaim13_22_v1" => {
            "Market Velocity 0.0375SL 2.7R reclaim13-22 v1"
        }
        "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1" => {
            "Market Velocity 0.0375SL 2.6R gap0.5 retest0.3 reclaim13-22 v1"
        }
        "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1" => {
            "Market Velocity 0.0375SL 1.5R signal retest2 delta24-34 pchg5-10 v1"
        }
        "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1" => {
            "Market Velocity 0.0375SL 2.0R reclaim fvg wait5 delta20-40 pchg5-12 v1"
        }
        "research_momentum_0375sl_20r_reclaim_delta13_72_pchg5_v1" => {
            "Market Velocity 0.0375SL 2.0R reclaim delta13-72 pchg5 v1"
        }
        "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1" => {
            "Market Velocity 0.0375SL 2.0R breakout reclaim fvg wait10 delta20-40 pchg5-12 v1"
        }
        "research_momentum_0375sl_10r_breakout_reclaim_delta11_72_pchg4_12_dist14_vol11_v1" => {
            "Market Velocity 0.0375SL 1.0R breakout reclaim delta11-72 pchg4-12 dist14 vol11 v1"
        }
        "research_momentum_0375sl_10r_breakout_reclaim_ma_delta11_72_pchg4_12_dist14_vol11_ignore_v1" => {
            "Market Velocity 0.0375SL 1.0R breakout reclaim_ma ignore delta11-72 pchg4-12 dist14 vol11 v1"
        }
        "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1" => {
            "Market Velocity short 0.0375SL 1.0R 15m support breakdown delta5-72 pchg1.5-12 vol13 v1"
        }
        "research_momentum_short_04sl_10r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_prevlow_v2" => {
            "Market Velocity short 0.04SL 1.0R 15m support breakdown delta5-72 pchg1.5-12 vol11 prevlow v2"
        }
        "research_momentum_short_04sl_06r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_dist5_v3" => {
            "Market Velocity 15m short 0.04SL 0.6R support breakdown d5-72 pchg1.5-12 vol1.1 dist5 v3"
        }
        "research_momentum_short_04sl_06r_15m_support_breakdown_d5_72_pchg1_12_vol10_dist8_v4" => {
            "Market Velocity 15m short 0.04SL 0.6R support breakdown d5-72 pchg1-12 vol1.0 dist8 v4"
        }
        "research_momentum_short_04sl_065r_15m_support_breakdown_d1_100_pchg0p5_12_vol10_dist14_v5" => {
            "Market Velocity 15m short 0.04SL 0.65R support breakdown d1-100 pchg0.5-12 vol1.0 dist14 v5"
        }
        "research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6" => {
            "Market Velocity 15m short 0.04SL 1.0R support breakdown d5-100 pchg2-12 vol1.0 dist14 v6"
        }
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1" => {
            "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta20-40 pchg5-12 v1"
        }
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1" => {
            "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta15-40 pchg5-12 v1"
        }
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1" => {
            "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta15-40 pchg5-12 runner6R20 stop1 v1"
        }
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1" => {
            "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 delta15-40 pchg5-12 runner8R20 stop1 v1"
        }
        "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1" => {
            "Market Velocity 0.04SL 2.0R reclaim fvg wait10 delta15-40 pchg5-12 v1"
        }
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg wait10 delta15-40 pchg5-12 v1"
        }
        "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg wait10 delta20-40 pchg5-10 v1"
        }
        "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg wait12 delta20-40 pchg5-10 v1"
        }
        "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg wait14 dist3 pullback3 vol11 fill10 delta20-40 pchg5-10 v1"
        }
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg retest1 pullback3 vol11 delta20-40 pchg5-10 v2"
        }
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg retest1 gap0 pullback3 vol11 delta20-40 pchg5-10 v3"
        }
        "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4" => {
            "Market Velocity 0.04SL 1.8R reclaim fvg retest1 gap0 open-fade-vol2 pullback3 vol11 delta20-40 pchg5-10 v4"
        }
        "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1" => {
            "Market Velocity 0.04SL 1.8R reclaim retest1 dist3 pullback3 vol11 delta20-40 pchg5-10 v1"
        }
        "research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1" => {
            "Market Velocity 0.04SL 2.0R reclaim retest1 dist3 pullback3 vol11 delta20-40 pchg5-10 v1"
        }
        "research_momentum_04sl_18r_breakout_reclaim_retest1_delta20_40_pchg5_10_v1" => {
            "Market Velocity 0.04SL 1.8R breakout reclaim retest1 vol10 delta20-40 pchg5-10 v1"
        }
        "research_momentum_04sl_18r_breakout_reclaim_fvg_retest1_delta20_40_pchg5_8_v1" => {
            "Market Velocity 0.04SL 1.8R breakout reclaim fvg retest1 vol10 delta20-40 pchg5-8 v1"
        }
        "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1" => {
            "Market Velocity 0.04SL 2.0R breakout reclaim fvg wait10 minwait1 delta15-40 pchg5-12 v1"
        }
        "research_momentum_04sl_10r_kline15m_breakout_fvg20_vol13_dd35_v1" => {
            "Market Velocity 15m kline 0.04SL 1.0R breakout fvg20 vol13 dd35 v1"
        }
        "research_momentum_04sl_06r_kline15m_breakout_fvg20_vol13_dd35_v1" => {
            "Market Velocity 15m kline 0.04SL 0.6R breakout fvg20 vol13 dd35 v1"
        }
        "research_momentum_04sl_05r_kline15m_breakout_fvg30_vol13_dd35_v1" => {
            "Market Velocity 15m kline 0.04SL 0.5R breakout fvg30 vol13 dd35 v1"
        }
        "research_momentum_04sl_055r_kline15m_breakout_fvg30_vol13_dd35_v1" => {
            "Market Velocity 15m kline 0.04SL 0.55R breakout fvg30 vol13 dd35 v1"
        }
        "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1" => {
            "Market Velocity 15m kline 0.04SL 0.52R breakout fvg50 vol13 dd35 v1"
        }
        "research_momentum_04sl_10r_kline15m_direct_shape_reclaimema_vol12_body65_close80_rng15_v1" => {
            "Market Velocity 15m kline direct shape reclaim EMA 0.04SL 1.0R vol12 body65 close80 range1.5 v1"
        }
        "momentum_03sl_20r_v5" => "Market Velocity 0.03SL 2.0R momentum v5",
        "research_episode_momentum_03sl_24r_rank5_30_v1" => {
            "Market Velocity episode 0.03SL 2.4R rank5-30 v1"
        }
        "research_episode_momentum_05sl_20r_rank5_v1" => {
            "Market Velocity episode 0.05SL 2.0R rank5 v1"
        }
        "research_episode_momentum_05sl_30r_rank5_v1" => {
            "Market Velocity episode 0.05SL 3.0R rank5 v1"
        }
        "research_episode_runner_03sl_24r_8r30_v1" => {
            "Market Velocity episode runner 0.03SL 2.4R 8R30 v1"
        }
        _ => preset,
    }
}

fn canonical_manifest_json(value: &Value) -> Result<String> {
    Ok(serde_json::to_string(&canonical_json_value(value))?)
}

fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonical_json_value).collect()),
        Value::Object(object) => {
            let sorted = object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect::<Map<_, _>>())
        }
        _ => value.clone(),
    }
}

fn sha256_manifest_hash(canonical_json: &str) -> String {
    let digest = Sha256::digest(canonical_json.as_bytes());
    format!("sha256:{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_entry_family_manifests_have_distinct_identity_and_fixed_exit() {
        let manifests = [
            market_velocity_paper_strategy_preset_manifest(
                MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET,
            )
            .unwrap(),
            market_velocity_paper_strategy_preset_manifest(
                MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET,
            )
            .unwrap(),
            market_velocity_paper_strategy_preset_manifest(
                MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET,
            )
            .unwrap(),
        ];

        assert_ne!(manifests[0].strategy_key, manifests[1].strategy_key);
        assert_ne!(manifests[1].strategy_key, manifests[2].strategy_key);
        for manifest in manifests {
            assert_eq!(manifest.channel, "research");
            assert_eq!(manifest.manifest_status, "research");
            assert_eq!(
                manifest.manifest_json["parameters"]["risk"]["gross_target_r"],
                1.0
            );
            assert_eq!(
                manifest.manifest_json["execution"]["paper_observation_eligible"],
                false
            );
            assert_eq!(
                manifest.manifest_json["parameters"]["excluded_entry_logic"][6],
                "legacy_mixed_rsi_ema_macd"
            );
        }
    }

    #[test]
    fn v2_manifests_freeze_limit_entry_platform_quality_and_versioned_exits() {
        let momentum = market_velocity_paper_strategy_preset_manifest(
            MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET,
        )
        .unwrap();
        let platform = market_velocity_paper_strategy_preset_manifest(
            MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET,
        )
        .unwrap();
        let anchor = market_velocity_paper_strategy_preset_manifest(
            MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET,
        )
        .unwrap();

        assert_ne!(momentum.strategy_key, platform.strategy_key);
        assert_ne!(anchor.strategy_key, platform.strategy_key);
        assert_eq!(
            momentum.manifest_json["parameters"]["signal"]["directional_wick_limit_valid_candles"],
            12
        );
        assert_eq!(
            momentum.manifest_json["parameters"]["risk"]["gross_target_r_by_filtered_volume_ratio"]
                ["2.5_to_below_4"],
            1.8
        );
        assert_eq!(
            platform.manifest_json["parameters"]["signal"]["platform_width_atr_source"],
            "candle_immediately_before_break"
        );
        assert_eq!(
            platform.manifest_json["parameters"]["signal"]["minimum_touches_per_side"],
            2
        );
        assert_eq!(
            platform.manifest_json["parameters"]["risk"]["gross_target_r"],
            1.0
        );
        assert_eq!(
            anchor.manifest_json["parameters"]["signal"]["minimum_strictly_intervening_candles"],
            4
        );
        assert_eq!(
            anchor.manifest_json["parameters"]["signal"]["fallback_to_older_anchor"],
            false
        );
        assert_eq!(
            anchor.manifest_json["parameters"]["signal"]
                ["bullish_anchor_invalid_if_intermediate_rsi_strictly_above"],
            60.0
        );
        assert_eq!(
            anchor.manifest_json["parameters"]["signal"]
                ["bearish_anchor_invalid_if_intermediate_rsi_strictly_below"],
            40.0
        );
    }

    #[test]
    fn momentum_v3_manifest_changes_only_the_directional_wick_threshold_identity() {
        let v2 = market_velocity_paper_strategy_preset_manifest(
            MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET,
        )
        .unwrap();
        let v3 = market_velocity_paper_strategy_preset_manifest(
            MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET,
        )
        .unwrap();

        assert_ne!(v2.strategy_key, v3.strategy_key);
        assert_eq!(v3.channel, "research");
        assert_eq!(
            v3.manifest_json["parameters"]["signal"]["directional_wick_min_range_ratio"],
            0.55
        );
        assert_eq!(
            v2.manifest_json["parameters"]["signal"]["directional_wick_limit_valid_candles"],
            v3.manifest_json["parameters"]["signal"]["directional_wick_limit_valid_candles"]
        );
        assert_eq!(
            v2.manifest_json["parameters"]["risk"],
            v3.manifest_json["parameters"]["risk"]
        );
    }
}
