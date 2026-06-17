use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use okx::dto::market_dto::CandleOkxRespDto;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{
    MarketRankEvent, MarketRankEventType, MarketRankTechnicalSnapshot,
};
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_services::market::{
    build_market_velocity_entry_confirmation_from_candles,
    build_market_velocity_strategy_signal_request_with_entry_confirmation,
    MarketVelocityEntryConfirmation, MarketVelocityEntryConfirmationDecision,
    MarketVelocityStrategySignalConfig, MarketVelocityStrategySignalDecision,
};
use rust_quant_services::rust_quan_web::{
    build_market_velocity_scoped_execution_worker_env,
    build_market_velocity_scoped_worker_handoff_readiness,
    market_velocity_existing_execution_worker_path, ExecutionTaskClient, ExecutionTaskConfig,
    ExecutionWorker, MarketVelocityExecutionTaskCreationPreviewRequest,
    StrategySignalSubmitRequest,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{collections::BTreeMap, time::Duration};

use super::market_velocity_backfill::{build_okx_http_client, fetch_okx_history_candles};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarketVelocityEntryCandleLoadStatus {
    pub source: String,
    pub refreshed_from_exchange: bool,
    pub db_error: Option<String>,
    pub candle_count: usize,
}

#[derive(Debug, Clone)]
struct MarketVelocityEntryCandleLoad {
    candles: Vec<Candle>,
    status: MarketVelocityEntryCandleLoadStatus,
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
            "DATABASE_URL",
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

pub fn build_market_velocity_live_preview_request(
    signal: &StrategySignalSubmitRequest,
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<MarketVelocityExecutionTaskCreationPreviewRequest> {
    if signal.strategy_slug.trim() != "market_velocity" {
        bail!("Market Velocity live handoff only accepts strategy_slug=market_velocity");
    }
    let payload: Value = serde_json::from_str(&signal.payload_json)
        .map_err(|error| anyhow!("parse market velocity signal payload_json failed: {error}"))?;
    if payload_string(&payload, "source_signal_type").as_deref() != Some("market_velocity") {
        bail!("payload source_signal_type must be market_velocity");
    }

    let exchange = payload_string(&payload, "exchange").unwrap_or_else(|| {
        signal
            .strategy_key
            .split(':')
            .nth(1)
            .unwrap_or("okx")
            .to_string()
    });
    let symbol = payload_string(&payload, "symbol").unwrap_or_else(|| signal.symbol.clone());
    let target_r = nested_payload_f64(&payload, "risk_plan", "target_r").unwrap_or(2.4);
    let horizon_hours =
        nested_payload_i64(&payload, "risk_plan", "max_holding_hours").unwrap_or(48);
    let entry_rule_version = payload_string(&payload, "entry_rule_version");
    let entry_trigger_filter_version = payload
        .get("entry_filter")
        .and_then(|value| payload_string(value, "entry_trigger_filter_version"));

    Ok(MarketVelocityExecutionTaskCreationPreviewRequest {
        rank_event_id: payload_i64(&payload, "rank_event_id"),
        buyer_email: buyer_email
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        combo_id,
        exchange: exchange.trim().to_ascii_lowercase(),
        symbol: symbol.trim().to_ascii_uppercase(),
        target_r,
        horizon_hours: horizon_hours.try_into().unwrap_or(i32::MAX),
        entry_rule_version,
        entry_trigger_filter_version,
        risk_adjusted_win_rate_edge: None,
    })
}

pub fn build_market_velocity_live_worker_manifest(task_id: i64) -> Value {
    json!({
        "execution_path": market_velocity_existing_execution_worker_path(),
        "next_worker_env": build_market_velocity_scoped_execution_worker_env(task_id)
    })
}

pub fn build_market_velocity_live_worker_handoff(
    task_id: i64,
    web_readiness: rust_quant_services::rust_quan_web::MarketVelocityExecutionTaskLiveReadinessResponse,
    worker_handoff_readiness: rust_quant_services::rust_quan_web::MarketVelocityWorkerHandoffReadiness,
) -> Value {
    json!({
        "task_id": task_id,
        "read_only": true,
        "mutation_allowed": false,
        "status": if web_readiness.status == "ready_for_live_worker"
            && worker_handoff_readiness.status == "ready_for_scoped_live_worker" {
            "ready_for_scoped_live_worker"
        } else {
            "blocked"
        },
        "manifest": build_market_velocity_live_worker_manifest(task_id),
        "web_owner_readiness": web_readiness,
        "worker_handoff_readiness": worker_handoff_readiness,
    })
}

pub fn market_velocity_required_live_owner_scope(
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<(&str, i64)> {
    let buyer_email = buyer_email
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("MARKET_VELOCITY_LIVE_BUYER_EMAIL is required for live task creation")
        })?;
    let combo_id = combo_id.filter(|value| *value > 0).ok_or_else(|| {
        anyhow!("MARKET_VELOCITY_LIVE_COMBO_ID is required for live task creation")
    })?;

    Ok((buyer_email, combo_id))
}

pub fn market_velocity_scope_signal_to_live_owner(
    mut signal: StrategySignalSubmitRequest,
    buyer_email: &str,
    combo_id: i64,
) -> Result<StrategySignalSubmitRequest> {
    let mut payload: Value = serde_json::from_str(&signal.payload_json)
        .map_err(|error| anyhow!("parse market velocity signal payload_json failed: {error}"))?;
    let payload_object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("Market Velocity signal payload_json must be an object"))?;
    payload_object.insert("target_buyer_email".to_string(), json!(buyer_email.trim()));
    payload_object.insert("target_combo_id".to_string(), json!(combo_id));
    payload_object.insert(
        "target_scope_source".to_string(),
        json!("market_velocity_live_handoff"),
    );
    signal.payload_json = serde_json::to_string(&payload)?;
    Ok(signal)
}

pub fn market_velocity_task_creation_apply_authorized(
    apply: bool,
    confirmation: Option<&str>,
) -> bool {
    apply && confirmation.map(str::trim) == Some(MARKET_VELOCITY_CREATE_TASK_CONFIRM_TOKEN)
}

pub fn market_velocity_scoped_worker_apply_authorized(
    apply: bool,
    confirmation: Option<&str>,
) -> bool {
    apply && confirmation.map(str::trim) == Some(MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN)
}

pub fn build_market_velocity_scoped_worker_env_overrides(
    task_id: i64,
    confirmation: &str,
) -> Result<BTreeMap<&'static str, String>> {
    if task_id <= 0 {
        bail!("scoped live worker task_id must be positive");
    }
    if !market_velocity_scoped_worker_apply_authorized(true, Some(confirmation)) {
        bail!(
            "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM={} is required before running scoped live worker",
            MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN
        );
    }

    Ok(BTreeMap::from([
        ("IS_RUN_EXECUTION_WORKER", "true".to_string()),
        ("EXECUTION_WORKER_ONLY", "true".to_string()),
        ("EXECUTION_WORKER_RUN_ONCE", "true".to_string()),
        ("EXECUTION_WORKER_DRY_RUN", "false".to_string()),
        (
            "EXECUTION_WORKER_ID",
            "market_velocity_scoped_live_worker".to_string(),
        ),
        ("EXECUTION_WORKER_LEASE_LIMIT", "1".to_string()),
        ("EXECUTION_WORKER_TARGET_TASK_IDS", task_id.to_string()),
        ("EXECUTION_WORKER_TASK_TYPES", "execute_signal".to_string()),
        (
            "EXECUTION_WORKER_TASK_STATUSES",
            "pending,leased".to_string(),
        ),
        ("EXECUTION_WORKER_CONFIRMATION_MODE", "false".to_string()),
        ("EXECUTION_WORKER_RECONCILIATION_ONLY", "false".to_string()),
        ("EXECUTION_WORKER_REPORT_REPLAY_MODE", "false".to_string()),
        (
            "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
            MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN.to_string(),
        ),
    ]))
}

async fn run_market_velocity_scoped_worker_once(task_id: i64, confirmation: &str) -> Result<usize> {
    let overrides = build_market_velocity_scoped_worker_env_overrides(task_id, confirmation)?;
    let _guard = EnvOverrideGuard::apply(&overrides);
    let worker = ExecutionWorker::from_env()?;
    worker.run_once().await
}

struct EnvOverrideGuard {
    previous: Vec<(&'static str, Option<String>)>,
}

impl EnvOverrideGuard {
    fn apply(overrides: &BTreeMap<&'static str, String>) -> Self {
        let previous = overrides
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var(key).ok();
                std::env::set_var(key, value);
                (*key, previous)
            })
            .collect();
        Self { previous }
    }
}

impl Drop for EnvOverrideGuard {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn normalize_candidate_limit(limit: i64) -> u32 {
    limit.clamp(1, 100) as u32
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

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        Value::String(raw) => Some(raw.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn payload_i64(payload: &Value, key: &str) -> Option<i64> {
    payload.get(key).and_then(|value| match value {
        Value::Number(number) => number.as_i64(),
        Value::String(raw) => raw.trim().parse::<i64>().ok(),
        _ => None,
    })
}

fn nested_payload_i64(payload: &Value, parent: &str, key: &str) -> Option<i64> {
    payload
        .get(parent)
        .and_then(|value| payload_i64(value, key))
}

fn payload_f64(payload: &Value, key: &str) -> Option<f64> {
    payload.get(key).and_then(|value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    })
}

fn nested_payload_f64(payload: &Value, parent: &str, key: &str) -> Option<f64> {
    payload
        .get(parent)
        .and_then(|value| payload_f64(value, key))
}

async fn load_market_velocity_live_candidate_events(
    pool: &PgPool,
    event_id: Option<i64>,
    lookback_hours: i64,
    limit: u32,
    config: &MarketVelocityStrategySignalConfig,
) -> Result<Vec<MarketRankEvent>> {
    let rows = sqlx::query(market_velocity_live_candidate_events_sql())
        .bind(config.min_delta_rank)
        .bind(config.max_new_rank)
        .bind(event_id)
        .bind(lookback_hours.to_string())
        .bind(i64::from(normalize_candidate_limit(i64::from(limit))))
        .fetch_all(pool)
        .await
        .context("load recent market velocity live candidate events")?;
    rows.into_iter().map(market_rank_event_from_row).collect()
}

fn market_velocity_live_candidate_events_sql() -> &'static str {
    r#"
        WITH eligible_events AS (
          SELECT
            id,
            lower(exchange) AS exchange,
            upper(symbol) AS symbol,
            event_type,
            timeframe,
            old_rank,
            new_rank,
            delta_rank,
            volume_24h_quote,
            current_price,
            previous_price,
            price_change_pct,
            price_direction,
            technical_timeframe,
            technical_period,
            technical_close_price,
            technical_ma_value,
            technical_ema_value,
            technical_ma_distance_pct,
            technical_ema_distance_pct,
            technical_ma_state,
            technical_ema_state,
            technical_candle_count,
            technical_snapshot_at,
            technical_snapshot_status,
            detected_at,
            source,
            notification_state
          FROM market_rank_events
          WHERE event_type IN ('rank_velocity', 'top_entry')
            AND delta_rank >= $1
            AND new_rank > 0
            AND new_rank <= $2
            AND lower(price_direction) = 'up'
            AND current_price IS NOT NULL
            AND lower(exchange) = 'okx'
            AND upper(replace(symbol, '-', '')) NOT LIKE 'LINKUSDT%'
            AND ($3::bigint IS NULL OR id = $3)
            AND detected_at >= NOW() - ($4::text || ' hours')::interval
        ),
        latest_per_symbol AS (
          SELECT DISTINCT ON (symbol) *
          FROM eligible_events
          ORDER BY symbol, detected_at DESC, id DESC
        )
        SELECT
          id,
          exchange,
          symbol,
          event_type,
          timeframe,
          old_rank,
          new_rank,
          delta_rank,
          volume_24h_quote,
          current_price,
          previous_price,
          price_change_pct,
          price_direction,
          technical_timeframe,
          technical_period,
          technical_close_price,
          technical_ma_value,
          technical_ema_value,
          technical_ma_distance_pct,
          technical_ema_distance_pct,
          technical_ma_state,
          technical_ema_state,
          technical_candle_count,
          technical_snapshot_at,
          technical_snapshot_status,
          detected_at,
          source,
          notification_state
        FROM latest_per_symbol
        ORDER BY detected_at DESC, id DESC
        LIMIT $5
        "#
}

fn market_rank_event_from_row(row: sqlx::postgres::PgRow) -> Result<MarketRankEvent> {
    let event_type_raw: String = row.get("event_type");
    let event_type = MarketRankEventType::try_from(event_type_raw.as_str())?;
    let technical_snapshot_status: String = row.get("technical_snapshot_status");
    let technical_snapshot = if technical_snapshot_status == "captured" {
        Some(MarketRankTechnicalSnapshot {
            timeframe: row.try_get::<String, _>("technical_timeframe")?,
            period: row.try_get::<i32, _>("technical_period")?,
            close_price: row.try_get::<Decimal, _>("technical_close_price")?,
            ma_value: row.try_get::<Decimal, _>("technical_ma_value")?,
            ema_value: row.try_get::<Decimal, _>("technical_ema_value")?,
            ma_distance_pct: row.try_get::<Decimal, _>("technical_ma_distance_pct")?,
            ema_distance_pct: row.try_get::<Decimal, _>("technical_ema_distance_pct")?,
            ma_state: row.try_get::<String, _>("technical_ma_state")?,
            ema_state: row.try_get::<String, _>("technical_ema_state")?,
            candle_count: row.try_get::<i32, _>("technical_candle_count")?,
            snapshot_at: row.try_get::<DateTime<Utc>, _>("technical_snapshot_at")?,
        })
    } else {
        None
    };

    Ok(MarketRankEvent {
        id: row.get("id"),
        exchange: row.get("exchange"),
        symbol: row.get("symbol"),
        event_type,
        timeframe: row.try_get("timeframe").ok(),
        old_rank: row.try_get("old_rank").ok(),
        new_rank: row.try_get("new_rank").ok(),
        delta_rank: row.try_get("delta_rank").ok(),
        volume_24h_quote: row.try_get("volume_24h_quote").ok(),
        current_price: row.try_get("current_price").ok(),
        previous_price: row.try_get("previous_price").ok(),
        price_change_pct: row.try_get("price_change_pct").ok(),
        price_direction: row.get("price_direction"),
        technical_snapshot_status,
        technical_snapshot,
        detected_at: row.get("detected_at"),
        source: row.get("source"),
        notification_state: row.get("notification_state"),
    })
}

async fn load_market_velocity_entry_candles(
    pool: &PgPool,
    symbol: &str,
    limit: u32,
) -> Result<Vec<Candle>> {
    let table_name = format!("{}_candles_15m", symbol.trim().to_ascii_lowercase());
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM {} ORDER BY ts DESC LIMIT $1",
        quote_identifier(&table_name)?
    );
    let mut rows = sqlx::query(&query)
        .bind(i64::from(limit.max(1)))
        .fetch_all(pool)
        .await
        .with_context(|| format!("load 15m entry candles from {table_name}"))?;
    rows.reverse();

    rows.into_iter()
        .map(|row| {
            let ts: i64 = row.get("ts");
            let mut candle = Candle::new(
                symbol.to_string(),
                Timeframe::M15,
                ts,
                Price::new(parse_decimal_text(row.get::<String, _>("o").as_str())?)?,
                Price::new(parse_decimal_text(row.get::<String, _>("h").as_str())?)?,
                Price::new(parse_decimal_text(row.get::<String, _>("l").as_str())?)?,
                Price::new(parse_decimal_text(row.get::<String, _>("c").as_str())?)?,
                Volume::new(parse_decimal_text(row.get::<String, _>("vol").as_str())?)?,
            );
            candle.confirm();
            Ok(candle)
        })
        .collect()
}

async fn load_market_velocity_live_entry_candles(
    pool: &PgPool,
    refresh_client: Option<&reqwest::Client>,
    config: &MarketVelocityLiveHandoffConfig,
    symbol: &str,
    limit: u32,
) -> Result<MarketVelocityEntryCandleLoad> {
    let db_result = load_market_velocity_entry_candles(pool, symbol, limit).await;
    let now = Utc::now();
    match db_result {
        Ok(candles)
            if !market_velocity_entry_candles_need_refresh(
                &candles,
                now,
                config.entry_candle_max_staleness_minutes,
            ) =>
        {
            let candle_count = candles.len();
            Ok(MarketVelocityEntryCandleLoad {
                candles,
                status: MarketVelocityEntryCandleLoadStatus {
                    source: "quant_core_db".to_string(),
                    refreshed_from_exchange: false,
                    db_error: None,
                    candle_count,
                },
            })
        }
        db_result => {
            let db_error = db_result.as_ref().err().map(ToString::to_string);
            let Some(client) = refresh_client else {
                return db_result.map(|candles| {
                    let candle_count = candles.len();
                    MarketVelocityEntryCandleLoad {
                        candles,
                        status: MarketVelocityEntryCandleLoadStatus {
                            source: "quant_core_db_stale_refresh_disabled".to_string(),
                            refreshed_from_exchange: false,
                            db_error: None,
                            candle_count,
                        },
                    }
                });
            };
            let candles =
                fetch_market_velocity_latest_entry_candles(client, config, symbol, limit.max(1))
                    .await?;
            let candle_count = candles.len();
            Ok(MarketVelocityEntryCandleLoad {
                candles,
                status: MarketVelocityEntryCandleLoadStatus {
                    source: "okx_history_candles_on_demand".to_string(),
                    refreshed_from_exchange: true,
                    db_error,
                    candle_count,
                },
            })
        }
    }
}

async fn fetch_market_velocity_latest_entry_candles(
    client: &reqwest::Client,
    config: &MarketVelocityLiveHandoffConfig,
    symbol: &str,
    limit: u32,
) -> Result<Vec<Candle>> {
    let now_ms = Utc::now().timestamp_millis();
    let candle_window_ms = i64::from(limit.max(1)) * 15 * 60 * 1_000;
    let start_ms = now_ms - candle_window_ms.saturating_mul(2);
    let page_limit = usize::try_from(limit.min(100)).unwrap_or(100).max(1);
    let candles = fetch_okx_history_candles(
        client,
        &config.entry_candle_okx_rest_base,
        symbol,
        "15m",
        start_ms,
        now_ms,
        page_limit,
        config.entry_candle_request_sleep_ms,
    )
    .await
    .with_context(|| format!("on-demand fetch latest 15m candles failed: symbol={symbol}"))?;
    okx_candles_to_market_velocity_domain(symbol, candles)
}

fn okx_candles_to_market_velocity_domain(
    symbol: &str,
    candles: Vec<CandleOkxRespDto>,
) -> Result<Vec<Candle>> {
    let mut converted = candles
        .into_iter()
        .map(|row| {
            let ts = row
                .ts
                .parse::<i64>()
                .with_context(|| format!("invalid OKX candle timestamp: {}", row.ts))?;
            let mut candle = Candle::new(
                symbol.to_string(),
                Timeframe::M15,
                ts,
                Price::new(parse_decimal_text(&row.o)?)?,
                Price::new(parse_decimal_text(&row.h)?)?,
                Price::new(parse_decimal_text(&row.l)?)?,
                Price::new(parse_decimal_text(&row.c)?)?,
                Volume::new(parse_decimal_text(&row.v)?)?,
            );
            if row.confirm.trim() == "1" {
                candle.confirm();
            }
            Ok(candle)
        })
        .collect::<Result<Vec<_>>>()?;
    converted.sort_by_key(|candle| candle.timestamp);
    Ok(converted)
}

fn market_velocity_entry_candles_need_refresh(
    candles: &[Candle],
    now: DateTime<Utc>,
    max_staleness_minutes: i64,
) -> bool {
    let Some(latest) = candles.last() else {
        return true;
    };
    if max_staleness_minutes <= 0 {
        return false;
    }
    let age_seconds = now
        .signed_duration_since(latest.datetime)
        .num_seconds()
        .max(0);
    let age_minutes = (age_seconds + 59) / 60;
    age_minutes > max_staleness_minutes
}

fn quote_identifier(identifier: &str) -> Result<String> {
    if identifier.is_empty()
        || !identifier
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        bail!("unsafe table identifier: {identifier}");
    }
    Ok(format!("\"{}\"", identifier.replace('"', "\"\"")))
}

fn parse_decimal_text(value: &str) -> Result<f64> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid decimal {value}: {error}"))?;
    if !parsed.is_finite() {
        bail!("decimal must be finite: {value}");
    }
    Ok(parsed)
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
    use rust_quant_services::rust_quan_web::{
        build_market_velocity_scoped_worker_handoff_readiness, ExecutionTask,
        MarketVelocityExecutionTaskLiveReadinessResponse, StrategySignalSubmitRequest,
    };
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};

    const LIVE_HANDOFF_ENV_KEYS: &[&str] = &[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
        "DATABASE_URL",
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

    fn sample_signal_request() -> StrategySignalSubmitRequest {
        StrategySignalSubmitRequest {
            source: "rust_quant".to_string(),
            external_id: "rust_quant:market_velocity:2042663".to_string(),
            strategy_slug: "market_velocity".to_string(),
            strategy_key: "market_velocity:okx:ASTER-USDT-SWAP".to_string(),
            symbol: "ASTER-USDT-SWAP".to_string(),
            signal_type: "entry".to_string(),
            direction: "long".to_string(),
            title: "Market Velocity long signal ASTER-USDT-SWAP".to_string(),
            summary: None,
            confidence: Some(0.72),
            payload_json: json!({
                "source_signal_type": "market_velocity",
                "rank_event_id": 2042663,
                "exchange": "okx",
                "symbol": "ASTER-USDT-SWAP",
                "entry_rule_version": "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1",
                "entry_filter": {
                    "entry_trigger_filter_version": "entry_trigger_allowlist_v1"
                },
                "risk_plan": {
                    "target_r": 2.4,
                    "max_holding_hours": 48
                }
            })
            .to_string(),
            generated_at: Some("2026-06-16T09:30:00Z".to_string()),
        }
    }

    fn sample_live_task_readiness(
        task_id: i64,
    ) -> MarketVelocityExecutionTaskLiveReadinessResponse {
        MarketVelocityExecutionTaskLiveReadinessResponse {
            read_only: true,
            mutation_allowed: false,
            owner_service: "quant_web".to_string(),
            status: "ready_for_live_worker".to_string(),
            task: sample_execution_task(task_id),
            checks: Vec::new(),
            blocker_codes: Vec::new(),
        }
    }

    fn sample_execution_task(task_id: i64) -> ExecutionTask {
        ExecutionTask {
            id: task_id,
            news_signal_id: None,
            strategy_signal_id: Some(991),
            combo_id: 85,
            buyer_email: "buyer@example.com".to_string(),
            strategy_slug: "market_velocity".to_string(),
            symbol: "ASTER-USDT-SWAP".to_string(),
            task_type: "execute_signal".to_string(),
            task_status: "pending".to_string(),
            priority: 3,
            lease_owner: None,
            lease_until: None,
            scheduled_at: "2026-06-16T09:30:00Z".to_string(),
            request_payload_json: json!({
                "source_signal_type": "market_velocity",
                "strategy_slug": "market_velocity",
                "symbol": "ASTER-USDT-SWAP",
                "exchange": "okx",
                "auto_execution_allowed": true,
                "execution_policy": {
                    "mode": "live_execution_authorized",
                    "live_order_allowed": true,
                    "paper_trade_required": false,
                    "production_stage": "live_execution_allowed"
                },
                "side": "buy",
                "position_side": "long",
                "trade_side": "open",
                "order_type": "market",
                "execution": {
                    "exchange": "okx",
                    "symbol": "ASTER-USDT-SWAP",
                    "side": "buy",
                    "order_type": "market",
                    "size_usdt": 50.0,
                    "position_side": "long",
                    "position_mode": "net"
                },
                "risk_plan": {
                    "entry_price": 100.0,
                    "selected_stop_loss_price": 97.5,
                    "direction": "long",
                    "protective_stop_loss_required": true
                }
            }),
            created_at: "2026-06-16T09:30:00Z".to_string(),
            updated_at: "2026-06-16T09:30:00Z".to_string(),
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
    fn scoped_worker_auto_run_requires_exact_live_order_confirmation() {
        assert!(!market_velocity_scoped_worker_apply_authorized(false, None));
        assert!(!market_velocity_scoped_worker_apply_authorized(true, None));
        assert!(!market_velocity_scoped_worker_apply_authorized(
            true,
            Some("true")
        ));
        assert!(market_velocity_scoped_worker_apply_authorized(
            true,
            Some(MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN)
        ));
    }

    #[test]
    fn scoped_worker_env_overrides_force_one_reviewed_live_task() {
        let overrides = build_market_velocity_scoped_worker_env_overrides(
            2042663,
            MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN,
        )
        .expect("overrides");

        assert_eq!(overrides["IS_RUN_EXECUTION_WORKER"], "true");
        assert_eq!(overrides["EXECUTION_WORKER_ONLY"], "true");
        assert_eq!(overrides["EXECUTION_WORKER_RUN_ONCE"], "true");
        assert_eq!(overrides["EXECUTION_WORKER_DRY_RUN"], "false");
        assert_eq!(overrides["EXECUTION_WORKER_TARGET_TASK_IDS"], "2042663");
        assert_eq!(overrides["EXECUTION_WORKER_LEASE_LIMIT"], "1");
        assert_eq!(overrides["EXECUTION_WORKER_TASK_TYPES"], "execute_signal");
        assert_eq!(
            overrides["EXECUTION_WORKER_TASK_STATUSES"],
            "pending,leased"
        );
        assert_eq!(
            overrides["EXECUTION_WORKER_LIVE_ORDER_CONFIRM"],
            "I_UNDERSTAND_LIVE_ORDERS"
        );
    }

    #[test]
    fn preview_request_is_derived_from_rust_market_velocity_signal_payload() {
        let preview = build_market_velocity_live_preview_request(
            &sample_signal_request(),
            Some("buyer@example.com"),
            Some(85),
        )
        .expect("preview request");

        assert_eq!(preview.rank_event_id, Some(2042663));
        assert_eq!(preview.buyer_email.as_deref(), Some("buyer@example.com"));
        assert_eq!(preview.combo_id, Some(85));
        assert_eq!(preview.exchange, "okx");
        assert_eq!(preview.symbol, "ASTER-USDT-SWAP");
        assert_eq!(preview.target_r, 2.4);
        assert_eq!(preview.horizon_hours, 48);
        assert_eq!(
            preview.entry_rule_version.as_deref(),
            Some("rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1")
        );
        assert_eq!(
            preview.entry_trigger_filter_version.as_deref(),
            Some("entry_trigger_allowlist_v1")
        );
    }

    #[test]
    fn live_worker_manifest_reuses_existing_execution_worker_without_scripts() {
        let manifest = build_market_velocity_live_worker_manifest(228);
        let encoded = serde_json::to_string(&manifest).expect("manifest json");

        assert_eq!(
            manifest["execution_path"]["kind"],
            "existing_execution_worker"
        );
        assert_eq!(
            manifest["execution_path"]["reuse"],
            "vegas_style_execution_task_worker"
        );
        assert_eq!(
            manifest["execution_path"]["creates_new_order_system"],
            false
        );
        assert_eq!(
            manifest["next_worker_env"]["EXECUTION_WORKER_TARGET_TASK_IDS"],
            "228"
        );
        assert_eq!(
            manifest["next_worker_env"]["EXECUTION_WORKER_DRY_RUN"],
            "false"
        );
        assert!(
            !encoded.contains(".sh") && !encoded.contains("scripts/dev"),
            "production handoff manifest must not point to shell scripts: {encoded}"
        );
    }

    #[test]
    fn live_worker_handoff_includes_web_and_worker_readiness() {
        let web_readiness = sample_live_task_readiness(228);
        let worker_readiness =
            build_market_velocity_scoped_worker_handoff_readiness(&web_readiness);

        let handoff =
            build_market_velocity_live_worker_handoff(228, web_readiness, worker_readiness);

        assert_eq!(handoff["status"], "ready_for_scoped_live_worker");
        assert_eq!(handoff["read_only"], true);
        assert_eq!(handoff["mutation_allowed"], false);
        assert_eq!(
            handoff["manifest"]["execution_path"]["reuse"],
            "vegas_style_execution_task_worker"
        );
        assert_eq!(
            handoff["manifest"]["next_worker_env"]["EXECUTION_WORKER_TARGET_TASK_IDS"],
            "228"
        );
        assert_eq!(
            handoff["web_owner_readiness"]["status"],
            "ready_for_live_worker"
        );
        assert_eq!(
            handoff["worker_handoff_readiness"]["status"],
            "ready_for_scoped_live_worker"
        );
    }

    #[test]
    fn live_task_creation_requires_owner_scope() {
        assert!(market_velocity_required_live_owner_scope(None, Some(85)).is_err());
        assert!(
            market_velocity_required_live_owner_scope(Some("buyer@example.com"), None).is_err()
        );
        assert!(
            market_velocity_required_live_owner_scope(Some("buyer@example.com"), Some(85)).is_ok()
        );
    }

    #[test]
    fn live_task_signal_payload_is_scoped_to_owner_combo_before_submit() {
        let signal = market_velocity_scope_signal_to_live_owner(
            sample_signal_request(),
            "buyer@example.com",
            85,
        )
        .expect("scoped signal");
        let payload: Value = serde_json::from_str(&signal.payload_json).expect("payload json");

        assert_eq!(payload["target_buyer_email"], "buyer@example.com");
        assert_eq!(payload["target_combo_id"], 85);
        assert_eq!(
            payload["target_scope_source"],
            "market_velocity_live_handoff"
        );
    }

    #[test]
    fn task_creation_apply_requires_explicit_confirmation() {
        assert!(!market_velocity_task_creation_apply_authorized(
            true,
            Some("wrong")
        ));
        assert!(market_velocity_task_creation_apply_authorized(
            true,
            Some("I_UNDERSTAND_THIS_CREATES_WEB_EXECUTION_TASK")
        ));
        assert!(!market_velocity_task_creation_apply_authorized(false, None));
    }

    #[test]
    fn candidate_scan_limit_is_bounded_for_live_handoff() {
        assert_eq!(normalize_candidate_limit(-10), 1);
        assert_eq!(normalize_candidate_limit(0), 1);
        assert_eq!(normalize_candidate_limit(20), 20);
        assert_eq!(normalize_candidate_limit(500), 100);
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
    fn candidate_scan_sql_uses_latest_event_per_symbol_before_limit() {
        let sql = market_velocity_live_candidate_events_sql();

        assert!(
            sql.contains("DISTINCT ON (symbol)"),
            "live candidate scan must not let repeated events from a few symbols fill the limit: {sql}"
        );
        assert!(
            sql.contains("ORDER BY symbol, detected_at DESC, id DESC"),
            "latest event per symbol must be selected before global ordering: {sql}"
        );
        assert!(
            sql.contains("FROM latest_per_symbol"),
            "global live scan should order already deduplicated symbols: {sql}"
        );
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
    fn entry_candle_on_demand_refresh_only_runs_for_missing_or_stale_db_candles() {
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 11, 30, 0).unwrap();
        let fresh = vec![sample_candle_at(now - chrono::Duration::minutes(30))];
        let stale = vec![sample_candle_at(now - chrono::Duration::minutes(90))];

        assert!(market_velocity_entry_candles_need_refresh(&[], now, 45));
        assert!(!market_velocity_entry_candles_need_refresh(&fresh, now, 45));
        assert!(market_velocity_entry_candles_need_refresh(&stale, now, 45));
        assert!(!market_velocity_entry_candles_need_refresh(&stale, now, 0));
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

    fn sample_candle_at(datetime: DateTime<Utc>) -> Candle {
        let mut candle = Candle::new(
            "ASTER-USDT-SWAP".to_string(),
            Timeframe::M15,
            datetime.timestamp_millis(),
            Price::new(100.0).unwrap(),
            Price::new(103.0).unwrap(),
            Price::new(99.0).unwrap(),
            Price::new(102.0).unwrap(),
            Volume::new(10_000.0).unwrap(),
        );
        candle.confirm();
        candle
    }
}
