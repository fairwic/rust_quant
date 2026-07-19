use anyhow::Result;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use rust_quant_services::market::MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_PRESET;

use super::args::{
    entry_trigger_filter_version_label, format_entry_trigger_filter_list,
    parse_paper_observation_args_from, MarketVelocityTradeDirection,
};
use super::directional_reversal::{
    EXHAUSTION_CURRENT_CLUSTER_CANDLES, EXHAUSTION_SWING_RADIUS_CANDLES,
    EXHAUSTION_VOLUME_LOOKBACK_CANDLES, OPPOSITE_DURATION_MIN_R_SQUARED,
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
const MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_exhaustionvol1_15m_v4";
const MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_r18_30_scale4_both_deferlong3_exhaustionvol1_15m_v5";
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
    let args = parse_paper_observation_args_from(["--paper-strategy-preset", preset])?;
    let has_allowlist = !args.entry_trigger_allowlist.is_empty();
    let has_blocklist = !args.entry_trigger_blocklist.is_empty();
    let allowlist_label = format_entry_trigger_filter_list(&args.entry_trigger_allowlist);
    let blocklist_label = format_entry_trigger_filter_list(&args.entry_trigger_blocklist);
    let is_breakdown_short = args.trade_direction == MarketVelocityTradeDirection::Short;
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
    let strategy_key = if is_opposite_move_reversal {
        MARKET_MOMENTUM_OPPOSITE_MOVE_STRATEGY_KEY
    } else if is_breakdown_short {
        MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_KEY
    } else {
        MARKET_VELOCITY_STRATEGY_KEY
    };
    let product_slug = if is_opposite_move_reversal {
        MARKET_MOMENTUM_OPPOSITE_MOVE_PRODUCT_SLUG
    } else if is_breakdown_short {
        MARKET_VELOCITY_BREAKDOWN_SHORT_PRODUCT_SLUG
    } else {
        MARKET_VELOCITY_PRODUCT_SLUG
    };
    let is_breakdown_short_live_cutover =
        is_breakdown_short && preset == MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CUTOVER_PRESET;
    let execution_json = if is_breakdown_short_live_cutover {
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
        "entry_min_recent_drawdown_pct": args.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": args.entry_recent_drawdown_lookback_candles,
        "entry_opposite_move_lookback_candles": args.entry_opposite_move_lookback_candles,
        "entry_min_opposite_net_move_pct": args.entry_min_opposite_net_move_pct,
        "entry_min_opposite_duration_candles": args.entry_min_opposite_duration_candles,
        "entry_opposite_duration_min_r_squared": OPPOSITE_DURATION_MIN_R_SQUARED,
        "entry_min_exhaustion_volume_dominance_ratio": args.entry_min_exhaustion_volume_dominance_ratio,
        "entry_btc_96_max_abs_net_move_pct": args.entry_btc_96_max_abs_net_move_pct,
        "entry_exhaustion_volume_lookback_candles": EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
        "entry_exhaustion_current_cluster_candles": EXHAUSTION_CURRENT_CLUSTER_CANDLES,
        "entry_exhaustion_swing_radius_candles": EXHAUSTION_SWING_RADIUS_CANDLES,
        "entry_defer_bearish_continuation": args.entry_defer_bearish_continuation,
        "entry_defer_bullish_continuation": args.entry_defer_bullish_continuation,
        "entry_require_opposite_reversal_confirmation": args.entry_require_opposite_reversal_confirmation,
        "entry_require_reversal_average_reclaim": args.entry_require_reversal_average_reclaim,
        "entry_defer_max_wait_candles": args.entry_defer_max_wait_candles,
        "entry_symbol_cooldown_candles": args.entry_symbol_cooldown_candles,
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
            "kline_volume_rank_lookback_candles": if args.kline_volume_rank_velocity { json!(96) } else { Value::Null },
            "kline_volume_rank_quote_turnover": if args.kline_volume_rank_velocity { "vol_ccy_x_close" } else { "off" },
            "trade_direction": args.trade_direction.label(),
            "stop_loss_pct": args.stop_loss_pct,
            "stop_loss_mode": args.stop_loss_mode.label(),
            "structure_stop_min_pct": args.structure_stop_min_pct,
            "target_r": args.target_rs.first().copied(),
            "take_profit": if args.volume_atr_take_profit {
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
    let (channel, manifest_status) = if is_opposite_move_reversal {
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
