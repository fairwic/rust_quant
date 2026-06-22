use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use rust_quant_domain::entities::MarketRankEvent;
use rust_quant_services::market::{
    build_market_velocity_entry_confirmation_from_candles,
    build_market_velocity_strategy_signal_request_with_entry_confirmation,
    MarketVelocityEntryConfirmation, MarketVelocityEntryConfirmationDecision,
    MarketVelocityStrategySignalConfig, MarketVelocityStrategySignalDecision,
};
use rust_quant_services::rust_quan_web::{
    build_market_velocity_scoped_worker_handoff_readiness,
    market_velocity_existing_execution_worker_path, ExecutionTaskClient, ExecutionTaskConfig,
    StrategySignalSubmitRequest,
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::{collections::BTreeMap, time::Duration};

use super::market_velocity_backfill::build_okx_http_client;

mod candidates;
mod entry_candles;
mod handoff;

pub use handoff::{
    build_market_velocity_live_preview_request, build_market_velocity_live_worker_handoff,
    build_market_velocity_live_worker_manifest, build_market_velocity_scoped_worker_env_overrides,
    market_velocity_required_live_owner_scope, market_velocity_scope_signal_to_live_owner,
    market_velocity_scoped_worker_apply_authorized, market_velocity_task_creation_apply_authorized,
};

use candidates::{load_market_velocity_live_candidate_events, normalize_candidate_limit};
use entry_candles::{load_market_velocity_live_entry_candles, MarketVelocityEntryCandleLoadStatus};
use handoff::run_market_velocity_scoped_worker_once;

pub const MARKET_VELOCITY_CREATE_TASK_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_THIS_CREATES_WEB_EXECUTION_TASK";
pub const MARKET_VELOCITY_REFRESH_READINESS_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_THIS_REFRESHES_OKX_READONLY_TASK_READINESS";
pub const MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LIVE_ORDERS";
const DEFAULT_OKX_REST_BASE: &str = "https://www.okx.com";
const DEFAULT_ENTRY_CANDLE_MAX_STALENESS_MINUTES: i64 = 45;
const DEFAULT_ENTRY_CANDLE_REQUEST_SLEEP_MS: u64 = 0;

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityLiveHandoffConfig {
    pub database_url: String,
    pub web_base_url: String,
    pub internal_secret: String,
    pub buyer_email: Option<String>,
    pub combo_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub event_id: Option<i64>,
    pub lookback_hours: i64,
    pub candidate_limit: u32,
    pub entry_candle_max_staleness_minutes: i64,
    pub entry_candle_on_demand_refresh: bool,
    pub entry_candle_okx_rest_base: String,
    pub entry_candle_proxy_url: Option<String>,
    pub entry_candle_request_sleep_ms: u64,
    pub refresh_readiness_apply: bool,
    pub refresh_readiness_confirm: Option<String>,
    pub create_task_apply: bool,
    pub create_task_confirm: Option<String>,
    pub run_scoped_worker_apply: bool,
    pub run_scoped_worker_confirm: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketVelocityLiveHandoffRuntimeConfig {
    pub run_once: bool,
    pub interval_seconds: u64,
}

pub async fn run_market_velocity_live_handoff_runtime_from_env() -> Result<()> {
    let runtime_config = market_velocity_live_handoff_runtime_config_from_env()?;
    loop {
        match run_market_velocity_live_handoff_from_env().await {
            Ok(report) => println!("{}", serde_json::to_string_pretty(&report)?),
            Err(error) if !runtime_config.run_once => {
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "status": "error",
                        "error": error.to_string(),
                        "run_once": false,
                    }))?
                );
            }
            Err(error) => return Err(error),
        }

        if runtime_config.run_once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(runtime_config.interval_seconds)).await;
    }
}

pub fn market_velocity_live_handoff_config_from_env() -> Result<MarketVelocityLiveHandoffConfig> {
    Ok(MarketVelocityLiveHandoffConfig {
        database_url: first_non_empty_env(&[
            "QUANT_CORE_DATABASE_URL",
            "POSTGRES_QUANT_CORE_DATABASE_URL",
        ])
        .context("market_velocity_live_handoff requires QUANT_CORE_DATABASE_URL")?,
        web_base_url: first_non_empty_env(&["RUST_QUAN_WEB_BASE_URL", "QUANT_WEB_BASE_URL"])
            .context("market_velocity_live_handoff requires RUST_QUAN_WEB_BASE_URL")?,
        internal_secret: first_non_empty_env(&[
            "EXECUTION_EVENT_SECRET",
            "RUST_QUAN_WEB_INTERNAL_SECRET",
        ])
        .context("market_velocity_live_handoff requires EXECUTION_EVENT_SECRET")?,
        buyer_email: first_non_empty_env(&[
            "MARKET_VELOCITY_LIVE_BUYER_EMAIL",
            "RECONCILIATION_SNAPSHOT_BUYER_EMAIL",
        ]),
        combo_id: parse_optional_i64_env(&[
            "MARKET_VELOCITY_LIVE_COMBO_ID",
            "MARKET_VELOCITY_COMBO_ID",
        ])?,
        credential_id: parse_optional_i64_env(&[
            "MARKET_VELOCITY_TASK_READINESS_CREDENTIAL_ID",
            "MARKET_VELOCITY_LIVE_CREDENTIAL_ID",
        ])?,
        event_id: parse_optional_i64_env(&[
            "MARKET_VELOCITY_SIGNAL_EVENT_ID",
            "MARKET_VELOCITY_LIVE_EVENT_ID",
        ])?,
        lookback_hours: parse_i64_env("MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS", 24)?.max(1),
        candidate_limit: normalize_candidate_limit(parse_i64_env(
            "MARKET_VELOCITY_LIVE_CANDIDATE_LIMIT",
            20,
        )?),
        entry_candle_max_staleness_minutes: parse_i64_env(
            "MARKET_VELOCITY_ENTRY_CANDLE_MAX_STALENESS_MINUTES",
            DEFAULT_ENTRY_CANDLE_MAX_STALENESS_MINUTES,
        )?
        .max(0),
        entry_candle_on_demand_refresh: parse_bool_env(
            "MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH",
            true,
        )?,
        entry_candle_okx_rest_base: first_non_empty_env(&[
            "MARKET_VELOCITY_ENTRY_CANDLE_OKX_REST_BASE",
            "MARKET_VELOCITY_BACKFILL_OKX_REST_BASE",
        ])
        .unwrap_or_else(|| DEFAULT_OKX_REST_BASE.to_string()),
        entry_candle_proxy_url: first_non_empty_env(&["MARKET_VELOCITY_ENTRY_CANDLE_PROXY_URL"])
            .filter(|value| value.starts_with("http://") || value.starts_with("https://")),
        entry_candle_request_sleep_ms: parse_u64_env(
            "MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS",
            DEFAULT_ENTRY_CANDLE_REQUEST_SLEEP_MS,
        )?,
        refresh_readiness_apply: parse_bool_env(
            "MARKET_VELOCITY_TASK_READINESS_REFRESH_APPLY",
            false,
        )?,
        refresh_readiness_confirm: first_non_empty_env(&[
            "MARKET_VELOCITY_TASK_READINESS_REFRESH_CONFIRM",
            "MARKET_VELOCITY_REFRESH_READINESS_CONFIRM",
        ]),
        create_task_apply: parse_bool_env("MARKET_VELOCITY_CREATE_TASK_APPLY", false)?,
        create_task_confirm: first_non_empty_env(&[
            "MARKET_VELOCITY_CREATE_TASK_CONFIRM",
            "MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM",
        ]),
        run_scoped_worker_apply: parse_bool_env("MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY", false)?,
        run_scoped_worker_confirm: first_non_empty_env(&[
            "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM",
            "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
        ]),
    })
}

pub fn market_velocity_live_handoff_runtime_config_from_env(
) -> Result<MarketVelocityLiveHandoffRuntimeConfig> {
    let envs = std::env::vars().collect::<BTreeMap<_, _>>();
    market_velocity_live_handoff_runtime_config_from_map(&envs)
}

fn market_velocity_live_handoff_runtime_config_from_map(
    envs: &BTreeMap<String, String>,
) -> Result<MarketVelocityLiveHandoffRuntimeConfig> {
    Ok(MarketVelocityLiveHandoffRuntimeConfig {
        run_once: parse_bool_from_map(envs, "MARKET_VELOCITY_LIVE_HANDOFF_RUN_ONCE", true)?,
        interval_seconds: parse_u64_from_map(
            envs,
            "MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS",
            60,
        )?
        .max(1),
    })
}

pub async fn run_market_velocity_live_handoff_from_env() -> Result<Value> {
    run_market_velocity_live_handoff(market_velocity_live_handoff_config_from_env()?).await
}

pub async fn run_market_velocity_live_handoff(
    config: MarketVelocityLiveHandoffConfig,
) -> Result<Value> {
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url: config.web_base_url.clone(),
        internal_secret: config.internal_secret.clone(),
    })?;
    let mut refresh_readiness = json!({
        "apply": config.refresh_readiness_apply,
        "mutation_scope": "web_signed_readonly_preflight_snapshot_refresh_only",
        "exchange_mutation_allowed": false,
    });
    if config.refresh_readiness_apply {
        let credential_id = config
            .credential_id
            .ok_or_else(|| anyhow!("MARKET_VELOCITY_TASK_READINESS_CREDENTIAL_ID is required"))?;
        if config.refresh_readiness_confirm.as_deref().map(str::trim)
            != Some(MARKET_VELOCITY_REFRESH_READINESS_CONFIRM_TOKEN)
        {
            bail!(
                "MARKET_VELOCITY_TASK_READINESS_REFRESH_CONFIRM={} is required",
                MARKET_VELOCITY_REFRESH_READINESS_CONFIRM_TOKEN
            );
        }
        let credential = client.check_internal_api_credential(credential_id).await?;
        refresh_readiness["credential_id"] = json!(credential.id);
        refresh_readiness["last_check_code"] = json!(credential.last_check_code);
        refresh_readiness["execution_readiness"] =
            json!(credential.execution_readiness.can_execute);
    }

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core database for market velocity live handoff")?;
    let signal_config = MarketVelocityStrategySignalConfig::from_env()?;
    let candidate_events = load_market_velocity_live_candidate_events(
        &pool,
        config.event_id,
        config.lookback_hours,
        config.candidate_limit,
        &signal_config,
    )
    .await?;
    if candidate_events.is_empty() {
        return Ok(build_market_velocity_no_live_candidate_response(
            &config,
            refresh_readiness,
        ));
    }
    let entry_candle_refresh_client = if config.entry_candle_on_demand_refresh {
        Some(build_okx_http_client(
            config.entry_candle_proxy_url.as_deref(),
        )?)
    } else {
        None
    };
    let mut skipped_candidates = Vec::new();
    let mut selected: Option<(
        MarketRankEvent,
        MarketVelocityEntryConfirmation,
        StrategySignalSubmitRequest,
        MarketVelocityEntryCandleLoadStatus,
    )> = None;
    let explicit_event_requested = config.event_id.is_some();

    for event in candidate_events {
        let candle_load = match load_market_velocity_live_entry_candles(
            &pool,
            entry_candle_refresh_client.as_ref(),
            &config,
            &event.symbol,
            signal_config.entry_confirmation_fetch_limit,
        )
        .await
        {
            Ok(candles) => candles,
            Err(error) if !explicit_event_requested => {
                skipped_candidates.push(json!({
                    "event_id": event.id,
                    "symbol": event.symbol,
                    "blocker_code": "market_velocity_entry_candles_unavailable",
                    "blocker_detail": error.to_string(),
                    "entry_candles": {
                        "source": "unavailable",
                        "refresh_attempted": config.entry_candle_on_demand_refresh,
                        "refreshed_from_exchange": false,
                        "db_error": null,
                        "candle_count": 0,
                    },
                }));
                continue;
            }
            Err(error) => return Err(error),
        };
        let candles = candle_load.candles.clone();
        let entry_confirmation = match build_market_velocity_entry_confirmation_from_candles(
            "15m",
            &candles,
            &signal_config.entry_confirmation_config(),
        ) {
            MarketVelocityEntryConfirmationDecision::Confirmed(confirmation) => confirmation,
            MarketVelocityEntryConfirmationDecision::Blocked(blocker) => {
                skipped_candidates.push(json!({
                    "event_id": event.id,
                    "symbol": event.symbol,
                    "blocker_code": "market_velocity_entry_confirmation_blocked",
                    "blocker_detail": format!("{:?}", blocker),
                    "entry_candles": candle_load.status,
                }));
                continue;
            }
        };
        if let Some(blocker_detail) = market_velocity_entry_confirmation_stale_blocker(
            &entry_confirmation,
            Utc::now(),
            config.entry_candle_max_staleness_minutes,
        ) {
            skipped_candidates.push(json!({
                "event_id": event.id,
                "symbol": event.symbol,
                "blocker_code": "market_velocity_entry_confirmation_stale",
                "blocker_detail": blocker_detail,
                "snapshot_at": entry_confirmation.snapshot_at,
                "entry_candles": candle_load.status,
            }));
            continue;
        }
        let signal = match build_market_velocity_strategy_signal_request_with_entry_confirmation(
            &event,
            &signal_config,
            Some(&entry_confirmation),
        )? {
            MarketVelocityStrategySignalDecision::Submit(signal) => signal,
            MarketVelocityStrategySignalDecision::Blocked(blocker) => {
                skipped_candidates.push(json!({
                    "event_id": event.id,
                    "symbol": event.symbol,
                    "blocker_code": format!("market_velocity_signal_{:?}", blocker),
                    "entry_candles": candle_load.status,
                }));
                continue;
            }
        };
        selected = Some((event, entry_confirmation, signal, candle_load.status));
        break;
    }

    let Some((event, entry_confirmation, signal, candle_load)) = selected else {
        let skipped_summary = summarize_skipped_candidates(&skipped_candidates);
        return Ok(json!({
            "status": "blocked",
            "blocker_code": "market_velocity_no_entry_confirmed_candidate",
            "candidate_scan": {
                "limit": config.candidate_limit,
                "evaluated": skipped_candidates.len(),
                "explicit_event_id": config.event_id,
            },
            "skipped_summary": skipped_summary,
            "skipped_candidates": skipped_candidates,
            "execution_path": market_velocity_existing_execution_worker_path(),
            "refresh_readiness": refresh_readiness,
        }));
    };
    let preview_request = build_market_velocity_live_preview_request(
        &signal,
        config.buyer_email.as_deref(),
        config.combo_id,
    )?;
    let preview = client
        .preview_market_velocity_execution_task_creation(preview_request)
        .await?;
    let skipped_summary = summarize_skipped_candidates(&skipped_candidates);
    let mut response = json!({
        "status": if preview.blocker_codes.is_empty() { "ready_for_task_creation" } else { "blocked" },
        "read_only": !config.create_task_apply,
        "mutation_allowed": config.create_task_apply,
        "exchange_mutation_allowed": false,
        "creates_new_order_system": false,
        "candidate_scan": {
            "limit": config.candidate_limit,
            "skipped": skipped_candidates.len(),
            "explicit_event_id": config.event_id,
        },
        "skipped_summary": skipped_summary,
        "skipped_candidates": skipped_candidates,
        "candidate": {
            "event_id": event.id,
            "exchange": event.exchange,
            "symbol": event.symbol,
            "entry_confirmation": entry_confirmation,
            "entry_candles": candle_load,
        },
        "web_owner_preview": preview,
        "execution_path": market_velocity_existing_execution_worker_path(),
        "refresh_readiness": refresh_readiness,
    });
    if !config.create_task_apply {
        response["next_apply_confirm"] = json!(MARKET_VELOCITY_CREATE_TASK_CONFIRM_TOKEN);
        return Ok(response);
    }
    if !market_velocity_task_creation_apply_authorized(true, config.create_task_confirm.as_deref())
    {
        bail!(
            "MARKET_VELOCITY_CREATE_TASK_CONFIRM={} is required",
            MARKET_VELOCITY_CREATE_TASK_CONFIRM_TOKEN
        );
    }
    if !signal_config.live_order_allowed || signal_config.paper_trade_required {
        bail!("live task creation requires MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED=true and MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED=false");
    }
    let (target_buyer_email, target_combo_id) =
        market_velocity_required_live_owner_scope(config.buyer_email.as_deref(), config.combo_id)?;
    if !preview.blocker_codes.is_empty() {
        bail!(
            "Web owner preview blocked task creation: {:?}",
            preview.blocker_codes
        );
    }
    let signal =
        market_velocity_scope_signal_to_live_owner(signal, target_buyer_email, target_combo_id)?;
    let dispatch = client.submit_strategy_signal(signal).await?;
    let next_worker = match dispatch.generated_tasks.first() {
        Some(task) => {
            let web_readiness = client.market_velocity_live_task_readiness(task.id).await?;
            let worker_handoff_readiness =
                build_market_velocity_scoped_worker_handoff_readiness(&web_readiness);
            Some(build_market_velocity_live_worker_handoff(
                task.id,
                web_readiness,
                worker_handoff_readiness,
            ))
        }
        None => None,
    };
    let scoped_worker_execution = if config.run_scoped_worker_apply {
        if !market_velocity_scoped_worker_apply_authorized(
            true,
            config.run_scoped_worker_confirm.as_deref(),
        ) {
            bail!(
                "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM={} is required before running scoped live worker",
                MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN
            );
        }
        let task = dispatch
            .generated_tasks
            .first()
            .ok_or_else(|| anyhow!("live task creation returned no execution task"))?;
        let handoff = next_worker
            .as_ref()
            .ok_or_else(|| anyhow!("scoped live worker handoff is unavailable"))?;
        if handoff.get("status").and_then(Value::as_str) != Some("ready_for_scoped_live_worker") {
            bail!("scoped live worker readiness is blocked; refusing live worker run");
        }
        let handled = run_market_velocity_scoped_worker_once(
            task.id,
            config
                .run_scoped_worker_confirm
                .as_deref()
                .unwrap_or(MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN),
        )
        .await?;
        json!({
            "apply": true,
            "status": "scoped_worker_ran_once",
            "task_id": task.id,
            "handled": handled,
        })
    } else {
        json!({
            "apply": false,
            "status": "not_requested",
            "next_apply_confirm": MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN,
        })
    };
    response["status"] = json!("execution_task_created");
    response["read_only"] = json!(false);
    response["mutation_allowed"] = json!(true);
    response["strategy_signal_id"] = json!(dispatch.inbox.id);
    response["generated_tasks"] = json!(dispatch.generated_tasks);
    response["next_worker_handoff"] = next_worker.unwrap_or_else(|| json!(null));
    response["scoped_worker_execution"] = scoped_worker_execution;
    Ok(response)
}

fn market_velocity_entry_confirmation_stale_blocker(
    confirmation: &MarketVelocityEntryConfirmation,
    now: DateTime<Utc>,
    max_staleness_minutes: i64,
) -> Option<String> {
    if max_staleness_minutes <= 0 {
        return None;
    }
    let age_minutes = market_velocity_entry_confirmation_age_minutes(confirmation, now);
    (age_minutes > max_staleness_minutes)
        .then(|| format!("EntryCandleStale:{age_minutes}m>{max_staleness_minutes}m"))
}

fn market_velocity_entry_confirmation_age_minutes(
    confirmation: &MarketVelocityEntryConfirmation,
    now: DateTime<Utc>,
) -> i64 {
    let age_seconds = now
        .signed_duration_since(confirmation.snapshot_at)
        .num_seconds()
        .max(0);
    (age_seconds + 59) / 60
}

fn summarize_skipped_candidates(skipped_candidates: &[Value]) -> Value {
    let mut by_blocker_detail = BTreeMap::<String, usize>::new();
    let mut by_symbol = BTreeMap::<String, usize>::new();
    for candidate in skipped_candidates {
        let blocker = candidate
            .get("blocker_detail")
            .or_else(|| candidate.get("blocker_code"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *by_blocker_detail.entry(blocker).or_default() += 1;
        if let Some(symbol) = candidate.get("symbol").and_then(Value::as_str) {
            *by_symbol.entry(symbol.to_string()).or_default() += 1;
        }
    }
    json!({
        "total": skipped_candidates.len(),
        "by_blocker_detail": by_blocker_detail,
        "by_symbol": by_symbol,
    })
}

fn build_market_velocity_no_live_candidate_response(
    config: &MarketVelocityLiveHandoffConfig,
    refresh_readiness: Value,
) -> Value {
    json!({
        "status": "no_candidate",
        "blocker_code": "market_velocity_no_live_candidate",
        "read_only": true,
        "mutation_allowed": false,
        "exchange_mutation_allowed": false,
        "creates_new_order_system": false,
        "candidate_scan": {
            "limit": config.candidate_limit,
            "evaluated": 0,
            "lookback_hours": config.lookback_hours,
            "explicit_event_id": config.event_id,
        },
        "automation": {
            "task_creation_apply": config.create_task_apply,
            "scoped_worker_apply": config.run_scoped_worker_apply,
            "entry_candle_on_demand_refresh": config.entry_candle_on_demand_refresh,
        },
        "next_action": "wait_for_next_market_velocity_event",
        "execution_path": market_velocity_existing_execution_worker_path(),
        "refresh_readiness": refresh_readiness,
    })
}

fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn parse_optional_i64_env(keys: &[&str]) -> Result<Option<i64>> {
    first_non_empty_env(keys)
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|error| anyhow!("{} must be an integer: {error}", keys[0]))
                .and_then(|parsed| {
                    if parsed > 0 {
                        Ok(parsed)
                    } else {
                        bail!("{} must be positive", keys[0])
                    }
                })
        })
        .transpose()
}

fn parse_i64_env(key: &str, default: i64) -> Result<i64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<i64>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_u64_env(key: &str, default: u64) -> Result<u64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| anyhow!("{key} must be an unsigned integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_bool_env(key: &str, default: bool) -> Result<bool> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" | "" => Ok(false),
        _ => bail!("{key} must be a boolean"),
    }
}

fn parse_bool_from_map(envs: &BTreeMap<String, String>, key: &str, default: bool) -> Result<bool> {
    let Some(value) = envs.get(key) else {
        return Ok(default);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" | "" => Ok(false),
        _ => bail!("{key} must be a boolean"),
    }
}

fn parse_u64_from_map(envs: &BTreeMap<String, String>, key: &str, default: u64) -> Result<u64> {
    envs.get(key)
        .map(|value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| anyhow!("{key} must be an unsigned integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};

    const LIVE_HANDOFF_ENV_KEYS: &[&str] = &[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
        "RUST_QUAN_WEB_BASE_URL",
        "QUANT_WEB_BASE_URL",
        "EXECUTION_EVENT_SECRET",
        "RUST_QUAN_WEB_INTERNAL_SECRET",
        "MARKET_VELOCITY_LIVE_BUYER_EMAIL",
        "RECONCILIATION_SNAPSHOT_BUYER_EMAIL",
        "MARKET_VELOCITY_LIVE_COMBO_ID",
        "MARKET_VELOCITY_COMBO_ID",
        "MARKET_VELOCITY_TASK_READINESS_CREDENTIAL_ID",
        "MARKET_VELOCITY_LIVE_CREDENTIAL_ID",
        "MARKET_VELOCITY_SIGNAL_EVENT_ID",
        "MARKET_VELOCITY_LIVE_EVENT_ID",
        "MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS",
        "MARKET_VELOCITY_LIVE_CANDIDATE_LIMIT",
        "MARKET_VELOCITY_ENTRY_CANDLE_MAX_STALENESS_MINUTES",
        "MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH",
        "MARKET_VELOCITY_ENTRY_CANDLE_OKX_REST_BASE",
        "MARKET_VELOCITY_ENTRY_CANDLE_PROXY_URL",
        "MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS",
        "MARKET_VELOCITY_TASK_READINESS_REFRESH_APPLY",
        "MARKET_VELOCITY_TASK_READINESS_REFRESH_CONFIRM",
        "MARKET_VELOCITY_REFRESH_READINESS_CONFIRM",
        "MARKET_VELOCITY_CREATE_TASK_APPLY",
        "MARKET_VELOCITY_CREATE_TASK_CONFIRM",
        "MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM",
        "MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY",
        "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM",
        "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
    ];

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn snapshot_live_handoff_env() -> Vec<(&'static str, Option<String>)> {
        LIVE_HANDOFF_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect()
    }

    fn restore_env(snapshot: Vec<(&'static str, Option<String>)>) {
        for (key, value) in snapshot {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    fn clear_live_handoff_env() {
        for key in LIVE_HANDOFF_ENV_KEYS {
            std::env::remove_var(key);
        }
    }

    fn sample_live_handoff_config() -> MarketVelocityLiveHandoffConfig {
        MarketVelocityLiveHandoffConfig {
            database_url: "postgres://postgres:postgres123@localhost:5432/quant_core".to_string(),
            web_base_url: "http://127.0.0.1:18000".to_string(),
            internal_secret: "local-dev-secret".to_string(),
            buyer_email: Some("buyer@example.com".to_string()),
            combo_id: Some(85),
            credential_id: Some(1),
            event_id: None,
            lookback_hours: 24,
            candidate_limit: 20,
            entry_candle_max_staleness_minutes: 45,
            entry_candle_on_demand_refresh: true,
            entry_candle_okx_rest_base: DEFAULT_OKX_REST_BASE.to_string(),
            entry_candle_proxy_url: None,
            entry_candle_request_sleep_ms: 0,
            refresh_readiness_apply: false,
            refresh_readiness_confirm: None,
            create_task_apply: false,
            create_task_confirm: None,
            run_scoped_worker_apply: false,
            run_scoped_worker_confirm: None,
        }
    }

    #[test]
    fn live_handoff_config_requires_internal_execution_secret() {
        let _guard = env_lock();
        let snapshot = snapshot_live_handoff_env();
        clear_live_handoff_env();
        std::env::set_var(
            "QUANT_CORE_DATABASE_URL",
            "postgres://postgres:postgres123@localhost:5432/quant_core",
        );
        std::env::set_var("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:18000");

        let error = market_velocity_live_handoff_config_from_env().expect_err("secret is required");

        restore_env(snapshot);
        assert!(
            error
                .to_string()
                .contains("market_velocity_live_handoff requires EXECUTION_EVENT_SECRET"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn live_handoff_config_accepts_execution_secret_from_env() {
        let _guard = env_lock();
        let snapshot = snapshot_live_handoff_env();
        clear_live_handoff_env();
        std::env::set_var(
            "QUANT_CORE_DATABASE_URL",
            "postgres://postgres:postgres123@localhost:5432/quant_core",
        );
        std::env::set_var("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:18000");
        std::env::set_var("EXECUTION_EVENT_SECRET", "local-dev-secret");

        let config = market_velocity_live_handoff_config_from_env().expect("config");

        restore_env(snapshot);
        assert_eq!(config.internal_secret, "local-dev-secret");
        assert_eq!(config.candidate_limit, 20);
        assert_eq!(config.lookback_hours, 24);
    }

    #[test]
    fn live_handoff_config_defaults_to_on_demand_entry_candle_refresh() {
        let _guard = env_lock();
        let snapshot = snapshot_live_handoff_env();
        clear_live_handoff_env();
        std::env::set_var(
            "QUANT_CORE_DATABASE_URL",
            "postgres://postgres:postgres123@localhost:5432/quant_core",
        );
        std::env::set_var("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:18000");
        std::env::set_var("EXECUTION_EVENT_SECRET", "local-dev-secret");

        let config = market_velocity_live_handoff_config_from_env().expect("config");

        restore_env(snapshot);
        assert!(config.entry_candle_on_demand_refresh);
        assert_eq!(config.entry_candle_okx_rest_base, "https://www.okx.com");
        assert_eq!(config.entry_candle_proxy_url, None);
        assert_eq!(config.entry_candle_request_sleep_ms, 0);
        assert!(!config.run_scoped_worker_apply);
        assert_eq!(config.run_scoped_worker_confirm, None);
    }

    #[test]
    fn no_live_candidate_response_is_non_error_signal_status() {
        let config = sample_live_handoff_config();
        let response = build_market_velocity_no_live_candidate_response(
            &config,
            json!({
                "apply": false,
                "exchange_mutation_allowed": false,
            }),
        );

        assert_eq!(response["status"], "no_candidate");
        assert_eq!(
            response["blocker_code"],
            "market_velocity_no_live_candidate"
        );
        assert_eq!(response["read_only"], true);
        assert_eq!(response["mutation_allowed"], false);
        assert_eq!(response["exchange_mutation_allowed"], false);
        assert_eq!(response["candidate_scan"]["limit"], 20);
        assert_eq!(response["candidate_scan"]["evaluated"], 0);
        assert_eq!(response["candidate_scan"]["lookback_hours"], 24);
        assert_eq!(
            response["next_action"],
            "wait_for_next_market_velocity_event"
        );
    }

    #[test]
    fn live_handoff_runtime_config_defaults_to_one_shot() {
        let envs = BTreeMap::new();
        let config =
            market_velocity_live_handoff_runtime_config_from_map(&envs).expect("runtime config");

        assert!(config.run_once);
        assert_eq!(config.interval_seconds, 60);
    }

    #[test]
    fn live_handoff_runtime_config_supports_rust_native_scheduler() {
        let envs = BTreeMap::from([
            (
                "MARKET_VELOCITY_LIVE_HANDOFF_RUN_ONCE".to_string(),
                "false".to_string(),
            ),
            (
                "MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS".to_string(),
                "30".to_string(),
            ),
        ]);
        let config =
            market_velocity_live_handoff_runtime_config_from_map(&envs).expect("runtime config");

        assert!(!config.run_once);
        assert_eq!(config.interval_seconds, 30);
    }

    #[test]
    fn entry_confirmation_freshness_blocks_stale_live_candles() {
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 11, 30, 0).unwrap();
        let fresh = sample_entry_confirmation(now - chrono::Duration::minutes(30));
        let stale = sample_entry_confirmation(now - chrono::Duration::minutes(90));

        assert_eq!(
            market_velocity_entry_confirmation_stale_blocker(&fresh, now, 45),
            None
        );
        assert_eq!(
            market_velocity_entry_confirmation_stale_blocker(&stale, now, 45).as_deref(),
            Some("EntryCandleStale:90m>45m")
        );
    }

    #[test]
    fn skipped_candidate_summary_groups_blockers_and_symbols() {
        let summary = summarize_skipped_candidates(&[
            json!({
                "symbol": "XLM-USDT-SWAP",
                "blocker_code": "market_velocity_entry_confirmation_blocked",
                "blocker_detail": "VolumeNotConfirmed"
            }),
            json!({
                "symbol": "XLM-USDT-SWAP",
                "blocker_code": "market_velocity_entry_confirmation_blocked",
                "blocker_detail": "VolumeNotConfirmed"
            }),
            json!({
                "symbol": "MRVL-USDT-SWAP",
                "blocker_code": "market_velocity_entry_confirmation_blocked",
                "blocker_detail": "PriceBelowAverages"
            }),
        ]);

        assert_eq!(summary["total"], 3);
        assert_eq!(summary["by_blocker_detail"]["VolumeNotConfirmed"], 2);
        assert_eq!(summary["by_blocker_detail"]["PriceBelowAverages"], 1);
        assert_eq!(summary["by_symbol"]["XLM-USDT-SWAP"], 2);
        assert_eq!(summary["by_symbol"]["MRVL-USDT-SWAP"], 1);
    }

    fn sample_entry_confirmation(snapshot_at: DateTime<Utc>) -> MarketVelocityEntryConfirmation {
        MarketVelocityEntryConfirmation {
            timeframe: "15m".to_string(),
            period: 20,
            trigger: "breakout_previous_high".to_string(),
            latest_close: 2.612,
            previous_close: Some(2.605),
            previous_high: Some(2.606),
            ma_value: 2.59435,
            ema_value: 2.595011,
            ma_distance_pct: 0.680325,
            ema_distance_pct: 0.654694,
            volume_ratio: Some(0.97158),
            candle_count: 80,
            snapshot_at,
        }
    }
}
