use anyhow::Result;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::args::{
    entry_trigger_filter_version_label, format_entry_trigger_filter_list,
    format_entry_trigger_rank_blocklist, parse_paper_observation_args_from,
};

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
    let has_rank_blocklist = !args.entry_trigger_rank_blocklist.is_empty();
    let allowlist_label = format_entry_trigger_filter_list(&args.entry_trigger_allowlist);
    let blocklist_label = format_entry_trigger_filter_list(&args.entry_trigger_blocklist);
    let rank_blocklist_label =
        format_entry_trigger_rank_blocklist(&args.entry_trigger_rank_blocklist);
    let entry_trigger_rank_blocklist = args
        .entry_trigger_rank_blocklist
        .iter()
        .map(|block| {
            json!({
                "trigger": block.trigger,
                "min_new_rank": block.min_new_rank,
                "max_new_rank": block.max_new_rank,
            })
        })
        .collect::<Vec<_>>();
    let manifest_json = json!({
        "manifest_schema_version": 1,
        "strategy_key": "market_velocity",
        "strategy_family": "market_velocity",
        "preset": preset,
        "rule_version": args.paper_outcome_entry_rule_version,
        "product": {
            "slug": "market-velocity-radar",
            "symbol": "ALL",
            "timeframe": "15m",
        },
        "execution": {
            "service_mode": "signal_only",
            "source_signal_type": "market_velocity",
            "paper_outcome_sink": "web",
        },
        "parameters": {
            "event_source": args.event_source.label(),
            "trade_direction": args.trade_direction.label(),
            "stop_loss_pct": args.stop_loss_pct,
            "target_r": args.target_rs.first().copied(),
            "entry_period": args.entry_period,
            "entry_max_distance_pct": args.entry_max_distance_pct,
            "entry_min_volume_ratio": args.entry_min_volume_ratio,
            "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
            "min_delta_rank": args.min_delta_rank,
            "max_delta_rank": args.max_delta_rank,
            "max_new_rank": args.max_new_rank,
            "min_price_change_pct": args.min_price_change_pct,
            "chase_top_rank": args.chase_top_rank,
            "chase_price_change_pct": args.chase_price_change_pct,
            "stop_reentry_mode": args.stop_reentry_mode.label(),
            "fvg_entry_mode": args.fvg_entry_mode.label(),
            "runner_target_r": args.runner_target_r,
            "runner_fraction": args.runner_fraction,
            "runner_stop_r": args.runner_stop_r,
        },
        "filters": {
            "entry_trigger_allowlist": args.entry_trigger_allowlist,
            "entry_trigger_blocklist": args.entry_trigger_blocklist,
            "entry_trigger_rank_blocklist": entry_trigger_rank_blocklist,
            "entry_trigger_filter_version": entry_trigger_filter_version_label(
                has_allowlist,
                has_blocklist,
                has_rank_blocklist,
            ),
            "entry_trigger_allowlist_label": allowlist_label,
            "entry_trigger_blocklist_label": blocklist_label,
            "entry_trigger_rank_blocklist_label": rank_blocklist_label,
            "symbol_blocklist": args.symbol_blocklist,
        },
    });
    let canonical_json = canonical_manifest_json(&manifest_json)?;
    Ok(MarketVelocityPresetManifest {
        product_slug: "market-velocity-radar".to_string(),
        symbol: "ALL".to_string(),
        channel: "production_default".to_string(),
        manifest_hash: sha256_manifest_hash(&canonical_json),
        strategy_key: "market_velocity".to_string(),
        human_label: human_label_for_preset(preset).to_string(),
        risk_level: "high".to_string(),
        manifest_status: "production".to_string(),
        manifest_json,
        canonical_json,
    })
}

fn human_label_for_preset(preset: &str) -> &str {
    match preset {
        "research_momentum_0375sl_27r_reclaim13_22_v1" => {
            "Market Velocity 0.0375SL 2.7R reclaim13-22 v1"
        }
        "momentum_03sl_20r_v5" => "Market Velocity 0.03SL 2.0R momentum v5",
        "research_episode_momentum_03sl_24r_rank5_30_v1" => {
            "Market Velocity episode 0.03SL 2.4R rank5-30 v1"
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
