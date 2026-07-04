use anyhow::Result;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::args::{
    entry_trigger_filter_version_label, format_entry_trigger_filter_list,
    parse_paper_observation_args_from, MarketVelocityTradeDirection,
};

const MARKET_VELOCITY_STRATEGY_KEY: &str = "market_velocity";
const MARKET_VELOCITY_PRODUCT_SLUG: &str = "market-velocity-radar";
const MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_KEY: &str = "market_velocity_breakdown_short";
const MARKET_VELOCITY_BREAKDOWN_SHORT_PRODUCT_SLUG: &str = "market-velocity-breakdown-short";

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
    let strategy_key = if is_breakdown_short {
        MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_KEY
    } else {
        MARKET_VELOCITY_STRATEGY_KEY
    };
    let product_slug = if is_breakdown_short {
        MARKET_VELOCITY_BREAKDOWN_SHORT_PRODUCT_SLUG
    } else {
        MARKET_VELOCITY_PRODUCT_SLUG
    };
    let fast_momentum_filters_json = json!({
        "entry_min_rsi": args.entry_min_rsi,
        "entry_max_rsi": args.entry_max_rsi,
        "entry_min_rsi_delta": args.entry_min_rsi_delta,
        "entry_rsi_delta_lookback_candles": args.entry_rsi_delta_lookback_candles,
        "entry_bollinger_breakout": args.entry_bollinger_breakout,
        "entry_min_bollinger_bandwidth_expansion_pct": args.entry_min_bollinger_bandwidth_expansion_pct,
        "entry_min_recent_drawdown_pct": args.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": args.entry_recent_drawdown_lookback_candles,
        "entry_symbol_cooldown_candles": args.entry_symbol_cooldown_candles,
    });
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
        "execution": {
            "service_mode": "signal_only",
            "source_signal_type": strategy_key,
            "paper_outcome_sink": "web",
        },
        "parameters": {
            "event_source": args.event_source.label(),
            "trade_direction": args.trade_direction.label(),
            "stop_loss_pct": args.stop_loss_pct,
            "stop_loss_mode": args.stop_loss_mode.label(),
            "structure_stop_min_pct": args.structure_stop_min_pct,
            "target_r": args.target_rs.first().copied(),
            "entry_period": args.entry_period,
            "entry_max_distance_pct": args.entry_max_distance_pct,
            "entry_min_volume_ratio": args.entry_min_volume_ratio,
            "fast_momentum_filters": fast_momentum_filters_json,
            "entry_max_signal_pullback_pct": args.entry_max_signal_pullback_pct,
            "entry_max_gap_without_retest_pct": args.entry_max_gap_without_retest_pct,
            "entry_retest_tolerance_pct": args.entry_retest_tolerance_pct,
            "entry_retest_after_signal": args.entry_retest_after_signal,
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
    Ok(MarketVelocityPresetManifest {
        product_slug: product_slug.to_string(),
        symbol: "ALL".to_string(),
        channel: "production_default".to_string(),
        manifest_hash: sha256_manifest_hash(&canonical_json),
        strategy_key: strategy_key.to_string(),
        human_label: human_label_for_preset(preset).to_string(),
        risk_level: "high".to_string(),
        manifest_status: "production".to_string(),
        manifest_json,
        canonical_json,
    })
}

fn human_label_for_preset(preset: &str) -> &str {
    match preset {
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
