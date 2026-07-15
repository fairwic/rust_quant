use super::{
    args::entry_trigger_filter_version_label, select_stop_loss_for_confirmed_signal,
    summarize_target, timestamp_ms_to_rfc3339, trade_direction_for_event, BacktestCandle,
    ConfirmedEvent, MarketVelocityEventBacktestArgs, TradeResult, PAPER_OUTCOME_HORIZONS,
};
use anyhow::{bail, Context, Result};
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, MarketVelocityPaperOutcomeRequest,
};
use serde_json::json;
use std::collections::HashMap;

/// 构建 paper outcome 观测载荷，用于回测研究与 Web 侧纸面结果沉淀。
pub fn build_market_velocity_paper_outcomes(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<MarketVelocityPaperOutcomeRequest> {
    let confirmed_by_event_id = confirmed
        .iter()
        .map(|signal| (signal.event.id, signal))
        .collect::<HashMap<_, _>>();
    let mut outcomes = Vec::new();
    let entry_trigger_filter_version = entry_trigger_filter_version(args);
    for target_r in &args.target_rs {
        for (horizon_hours, horizon_ms) in PAPER_OUTCOME_HORIZONS {
            let (results, skipped_lock) =
                summarize_target(confirmed, candles_15m, *target_r, *horizon_ms, args);
            for result in results {
                let Some(event_id) = result.event_id else {
                    continue;
                };
                let Some(signal) = confirmed_by_event_id.get(&event_id) else {
                    continue;
                };
                let symbol = result
                    .symbol
                    .clone()
                    .unwrap_or_else(|| signal.event.symbol.clone());
                let entry_trigger = result.trigger.clone();
                let selected_stop_loss = select_stop_loss_for_confirmed_signal(signal, args);
                let entry_filter_payload = json!({
                    "entry_trigger_filter_version": entry_trigger_filter_version,
                    "entry_trigger_allowlist": &args.entry_trigger_allowlist,
                    "entry_trigger_blocklist": &args.entry_trigger_blocklist,
                });
                let filters_payload = json!({
                    "min_delta_rank": args.min_delta_rank,
                    "max_delta_rank": args.max_delta_rank,
                    "min_price_change_pct": args.min_price_change_pct,
                    "max_price_change_pct": args.max_price_change_pct,
                    "entry_max_distance_pct": args.entry_max_distance_pct,
                    "entry_min_volume_ratio": args.entry_min_volume_ratio,
                    "entry_max_gap_without_retest_pct": args.entry_max_gap_without_retest_pct,
                    "entry_retest_tolerance_pct": args.entry_retest_tolerance_pct,
                    "entry_retest_after_signal": args.entry_retest_after_signal,
                    "entry_retest_max_wait_candles": args.entry_retest_max_wait_candles,
                    "entry_retest_min_entry_open_gap_pct": args.entry_retest_min_entry_open_gap_pct,
                    "entry_retest_open_fade_min_volume_ratio": args.entry_retest_open_fade_min_volume_ratio,
                    "entry_min_body_ratio_pct": args.entry_min_body_ratio_pct,
                    "entry_min_close_position_pct": args.entry_min_close_position_pct,
                    "entry_min_range_expansion_ratio": args.entry_min_range_expansion_ratio,
                    "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
                    "max_15m_staleness_min": args.max_15m_staleness_min,
                    "max_4h_staleness_min": args.max_4h_staleness_min,
                });
                outcomes.push(MarketVelocityPaperOutcomeRequest {
                    rank_event_id: event_id,
                    exchange: signal.event.exchange.trim().to_ascii_lowercase(),
                    symbol,
                    target_r: *target_r,
                    horizon_hours: *horizon_hours,
                    entry_rule_version: args.paper_outcome_entry_rule_version.clone(),
                    entry_trigger: entry_trigger.clone(),
                    entry_price: result.entry_price,
                    entry_at: timestamp_ms_to_rfc3339(result.entry_ts),
                    outcome_status: result.outcome.label().to_string(),
                    exit_reason: result.reason.clone(),
                    result_r: result.r,
                    evaluated_at: timestamp_ms_to_rfc3339(result.exit_ts),
                    evaluation_payload: json!({
                        "source": "market_velocity_event_backtest",
                        "rank_event_id": event_id,
                        "detected_at": signal.event.detected_at,
                        "event_features": {
                            "new_rank": signal.event.new_rank,
                            "delta_rank": signal.event.delta_rank,
                            "current_price": signal.event.current_price,
                            "price_change_pct": signal.event.price_change_pct,
                        },
                        "target_r": target_r,
                        "horizon_hours": horizon_hours,
                        "trade_direction": trade_direction_for_event(&signal.event).label(),
                        "stop_loss_pct": args.stop_loss_pct,
                        "stop_loss_mode": args.stop_loss_mode.label(),
                        "selected_stop_loss_pct": selected_stop_loss.stop_loss_pct,
                        "selected_stop_loss_price": selected_stop_loss.price,
                        "selected_stop_loss_source": selected_stop_loss.source,
                        "entry_period": args.entry_period,
                        "entry_trigger": entry_trigger,
                        "entry_trigger_filter_version": entry_trigger_filter_version,
                        "trade_complete": result.complete,
                        "exit_ts": result.exit_ts,
                        "skipped_lock_count": skipped_lock,
                        "entry_rule_version": &args.paper_outcome_entry_rule_version,
                        "stop_reentry": stop_reentry_payload(&result, args),
                        "fvg_entry": fvg_entry_payload(args),
                        "profit_protection": profit_protection_payload(args),
                        "runner_exit": runner_exit_payload(args),
                        "early_exit": early_exit_payload(args),
                        "entry_filter": entry_filter_payload,
                        "filters": filters_payload
                    }),
                });
            }
        }
    }
    outcomes
}

/// 输出 paper outcome JSONL，供离线排查和生产调度日志取证使用。
pub(super) fn print_market_velocity_paper_outcomes_jsonl(
    outcomes: &[MarketVelocityPaperOutcomeRequest],
) -> Result<()> {
    for outcome in outcomes {
        println!(
            "paper_outcome_json\t{}",
            serde_json::to_string(outcome).context("serialize market velocity paper outcome")?
        );
    }
    println!("paper_outcomes_generated={}", outcomes.len());
    Ok(())
}

/// 向 Web owner service 提交 paper outcome，并硬性拒绝生成执行任务。
pub(super) async fn submit_market_velocity_paper_outcomes(
    outcomes: &[MarketVelocityPaperOutcomeRequest],
) -> Result<usize> {
    if outcomes.is_empty() {
        println!("paper_outcomes_submitted=0");
        return Ok(0);
    }
    let client = ExecutionTaskClient::new(quant_web_execution_task_config_from_env()?)?;
    let mut submitted = 0;
    for outcome in outcomes {
        let response = client
            .submit_market_velocity_paper_outcome(outcome.clone())
            .await
            .with_context(|| {
                format!(
                    "submit market velocity paper outcome rank_event_id={} target={}R horizon={}h",
                    outcome.rank_event_id, outcome.target_r, outcome.horizon_hours
                )
            })?;
        if response.generated_execution_task_count != 0 {
            bail!(
                "market velocity paper outcome endpoint generated {} execution tasks; expected observation-only",
                response.generated_execution_task_count
            );
        }
        submitted += 1;
    }
    println!("paper_outcomes_submitted={submitted}");
    Ok(submitted)
}

fn fvg_entry_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "mode": args.fvg_entry_mode.label(),
        "lookback_candles": args.fvg_lookback_candles,
        "max_wait_candles": args.fvg_max_wait_candles,
    })
}

fn profit_protection_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "enabled": args.profit_protect_after_r.is_some(),
        "activate_after_r": args.profit_protect_after_r,
        "stop_r": args.profit_protect_stop_r,
    })
}

fn runner_exit_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "enabled": args.runner_target_r.is_some(),
        "target_r": args.runner_target_r,
        "fraction": args.runner_fraction,
        "stop_r": args.runner_stop_r,
    })
}

fn early_exit_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "enabled": args.early_exit_no_profit_candles.is_some(),
        "no_profit_candles": args.early_exit_no_profit_candles,
    })
}

fn stop_reentry_payload(
    result: &TradeResult,
    args: &MarketVelocityEventBacktestArgs,
) -> serde_json::Value {
    let Some(reentry) = &result.reentry else {
        return json!({
            "mode": args.stop_reentry_mode.label(),
            "triggered": false,
        });
    };
    json!({
        "mode": reentry.mode.label(),
        "triggered": true,
        "original_entry_ts": reentry.original_entry_ts,
        "original_entry_price": reentry.original_entry_price,
        "original_exit_ts": reentry.original_exit_ts,
        "original_reason": reentry.original_reason,
        "original_r": reentry.original_r,
        "signal_ts": reentry.signal_ts,
        "reclaim_price": reentry.reclaim_price,
        "reentry_exit_reason": reentry.reentry_exit_reason,
        "reentry_r": reentry.reentry_r,
    })
}

fn entry_trigger_filter_version(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    entry_trigger_filter_version_label(
        !args.entry_trigger_allowlist.is_empty(),
        !args.entry_trigger_blocklist.is_empty(),
    )
}

fn quant_web_execution_task_config_from_env() -> Result<ExecutionTaskConfig> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .context("--paper-outcome-sink web requires RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL")?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .or_else(|_| std::env::var("ALPHA_EXECUTION_INTERNAL_SECRET"))
        .context(
            "--paper-outcome-sink web requires EXECUTION_EVENT_SECRET/RUST_QUAN_WEB_INTERNAL_SECRET/ALPHA_EXECUTION_INTERNAL_SECRET",
        )?;
    Ok(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{RadarEvent, MS_15M};

    #[test]
    fn paper_outcome_payload_keeps_event_features_without_execution_task_payload() {
        let args = MarketVelocityEventBacktestArgs {
            stop_loss_pct: 0.02,
            target_rs: vec![1.5, 2.0],
            paper_outcome_entry_rule_version: "rank_radar_4h_15m_v2".to_string(),
            entry_trigger_allowlist: vec![
                "breakout_previous_high".to_string(),
                "reclaim_ema".to_string(),
            ],
            max_delta_rank: Some(79),
            min_price_change_pct: Some(5.0),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let confirmed = vec![ConfirmedEvent {
            event: RadarEvent {
                id: 77,
                exchange: "okx".to_string(),
                symbol: "ETH-USDT-SWAP".to_string(),
                ts: 0,
                detected_at: "2026-06-15 00:00:00+00".to_string(),
                new_rank: 18,
                delta_rank: 12,
                current_price: 100.0,
                price_change_pct: 3.5,
            },
            entry_ts: MS_15M,
            entry_price: 100.0,
            entry_idx: 0,
            trigger: "breakout_previous_high".to_string(),
            structure_stop_loss_price: None,
            structure_stop_loss_source: None,
        }];
        let candles = HashMap::from([(
            "ETH-USDT-SWAP".to_string(),
            vec![BacktestCandle {
                ts: MS_15M,
                open: 100.0,
                high: 104.0,
                low: 99.0,
                close: 103.0,
                volume: 10.0,
            }],
        )]);
        let outcomes = build_market_velocity_paper_outcomes(&confirmed, &candles, &args);
        assert_eq!(outcomes.len(), 4);
        let first = &outcomes[0];
        assert_eq!(first.rank_event_id, 77);
        assert_eq!(first.exchange, "okx");
        assert_eq!(first.symbol, "ETH-USDT-SWAP");
        assert_eq!(first.target_r, 1.5);
        assert_eq!(first.horizon_hours, 24);
        assert_eq!(first.entry_rule_version, "rank_radar_4h_15m_v2");
        assert_eq!(
            first.entry_trigger.as_deref(),
            Some("breakout_previous_high")
        );
        assert_eq!(first.entry_price, 100.0);
        assert_eq!(first.outcome_status, "win");
        assert_eq!(first.exit_reason, "target_hit");
        assert_eq!(first.result_r, Some(1.5));
        assert_eq!(
            first.evaluation_payload["source"],
            "market_velocity_event_backtest"
        );
        assert_eq!(first.evaluation_payload["stop_loss_pct"], 0.02);
        assert_eq!(first.evaluation_payload["filters"]["max_delta_rank"], 79);
        assert_eq!(
            first.evaluation_payload["filters"]["min_price_change_pct"],
            5.0
        );
        assert_eq!(
            first.evaluation_payload["entry_trigger_filter_version"],
            "entry_trigger_allowlist_v1"
        );
        assert_eq!(
            first.evaluation_payload["entry_filter"]["entry_trigger_filter_version"],
            "entry_trigger_allowlist_v1"
        );
        assert_eq!(
            first.evaluation_payload["entry_filter"]["entry_trigger_allowlist"],
            serde_json::json!(["breakout_previous_high", "reclaim_ema"])
        );
        assert_eq!(first.evaluation_payload["event_features"]["new_rank"], 18);
        assert_eq!(first.evaluation_payload["event_features"]["delta_rank"], 12);
        assert_eq!(
            first.evaluation_payload["event_features"]["current_price"],
            100.0
        );
        assert_eq!(
            first.evaluation_payload["event_features"]["price_change_pct"],
            3.5
        );
        let serialized = serde_json::to_string(first).unwrap();
        assert!(!serialized.contains("execution_task"));
        assert!(!serialized.contains("buyer_email"));
    }
}
