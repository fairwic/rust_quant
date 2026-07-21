use super::{
    market_velocity_live_handoff_config_from_env,
    market_velocity_live_handoff_runtime_config_from_env,
    run_market_velocity_live_handoff_with_dependencies,
    validate_market_velocity_live_handoff_signal_config, MarketVelocityLiveHandoffDependencies,
};
use crate::app::market_velocity_strategy_config::load_market_velocity_signal_config_for_selector;
use anyhow::{anyhow, bail, Context, Result};
use std::time::Duration;

const LIVE_HANDOFF_LANES_ENV: &str = "MARKET_VELOCITY_LIVE_HANDOFF_LANES";

/// 标识 signal-worker 内一个固定的策略配置通道。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MarketVelocityLiveHandoffLane {
    strategy_key: String,
    preset: String,
}

/// 在一个 signal-worker 内顺序执行多份不可变策略快照，避免为每个 preset 常驻一个容器。
pub async fn run_market_velocity_live_handoff_multi_runtime_from_env() -> Result<()> {
    let runtime_config = market_velocity_live_handoff_runtime_config_from_env()?;
    let config = market_velocity_live_handoff_config_from_env()?;
    let dependencies = MarketVelocityLiveHandoffDependencies::new(&config)?;
    let lanes = parse_live_handoff_lanes(
        &std::env::var(LIVE_HANDOFF_LANES_ENV)
            .with_context(|| format!("signal-worker requires {LIVE_HANDOFF_LANES_ENV}"))?,
    )?;
    let mut signal_configs = Vec::with_capacity(lanes.len());
    for lane in lanes {
        let signal_config = load_market_velocity_signal_config_for_selector(
            &dependencies.pool,
            &lane.strategy_key,
            None,
            Some(&lane.preset),
        )
        .await?
        .ok_or_else(|| {
            anyhow!(
                "enabled {} strategy_config not found for handoff preset {}",
                lane.strategy_key,
                lane.preset
            )
        })?;
        validate_lane_strategy_identity(&lane, &signal_config.strategy_slug)?;
        validate_market_velocity_live_handoff_signal_config(&signal_config)?;
        signal_configs.push((lane, signal_config));
    }

    loop {
        for (lane, signal_config) in &signal_configs {
            match run_market_velocity_live_handoff_with_dependencies(
                &config,
                &dependencies,
                Some(signal_config),
            )
            .await
            {
                Ok(report) => tracing::info!(
                    strategy_key = lane.strategy_key,
                    preset = lane.preset,
                    report = %report,
                    "Market Velocity live handoff lane completed"
                ),
                Err(error) if !runtime_config.run_once => tracing::error!(
                    strategy_key = lane.strategy_key,
                    preset = lane.preset,
                    error = %error,
                    "Market Velocity live handoff lane failed; retrying next fixed-delay cycle"
                ),
                Err(error) => return Err(error),
            }
        }
        if runtime_config.run_once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(runtime_config.interval_seconds)).await;
    }
}

fn parse_live_handoff_lanes(raw: &str) -> Result<Vec<MarketVelocityLiveHandoffLane>> {
    let mut lanes = Vec::new();
    for value in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (strategy_key, preset) = value.split_once('@').ok_or_else(|| {
            anyhow!("{LIVE_HANDOFF_LANES_ENV} entry must use strategy_key@preset: {value}")
        })?;
        let lane = MarketVelocityLiveHandoffLane {
            strategy_key: strategy_key.trim().to_string(),
            preset: preset.trim().to_string(),
        };
        if lane.strategy_key.is_empty() || lane.preset.is_empty() {
            bail!("{LIVE_HANDOFF_LANES_ENV} entry must use non-empty strategy_key@preset");
        }
        if !lanes.iter().any(|existing| existing == &lane) {
            lanes.push(lane);
        }
    }
    if lanes.is_empty() {
        bail!("{LIVE_HANDOFF_LANES_ENV} must contain at least one lane");
    }
    Ok(lanes)
}

/// 拒绝策略键与配置载荷身份错配，避免两个 handoff lane 共享错误的审计身份。
fn validate_lane_strategy_identity(
    lane: &MarketVelocityLiveHandoffLane,
    strategy_slug: &str,
) -> Result<()> {
    if strategy_slug == lane.strategy_key {
        return Ok(());
    }
    bail!(
        "handoff lane {}@{} loaded mismatched strategy_slug {}",
        lane.strategy_key,
        lane.preset,
        strategy_slug
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_require_strategy_key_and_deduplicate_exact_pairs() {
        assert_eq!(
            parse_live_handoff_lanes(
                "market_velocity@stable_long,market_velocity_breakdown_short@short,market_velocity@stable_long"
            )
            .unwrap(),
            vec![
                MarketVelocityLiveHandoffLane {
                    strategy_key: "market_velocity".to_string(),
                    preset: "stable_long".to_string(),
                },
                MarketVelocityLiveHandoffLane {
                    strategy_key: "market_velocity_breakdown_short".to_string(),
                    preset: "short".to_string(),
                },
            ]
        );
        assert!(parse_live_handoff_lanes("stable_long").is_err());
        assert!(parse_live_handoff_lanes("market_velocity@").is_err());
        assert!(parse_live_handoff_lanes(" , ").is_err());
    }

    #[test]
    fn lane_rejects_a_config_with_another_strategy_identity() {
        let lane = MarketVelocityLiveHandoffLane {
            strategy_key: "market_velocity_breakdown_short".to_string(),
            preset: "short_v6".to_string(),
        };
        assert!(validate_lane_strategy_identity(&lane, "market_velocity_breakdown_short").is_ok());
        assert!(validate_lane_strategy_identity(&lane, "market_velocity").is_err());
    }
}
