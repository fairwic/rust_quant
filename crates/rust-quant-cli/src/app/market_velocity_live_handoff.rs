use super::env_parse::{first_non_empty_env, parse_bool_env, parse_i64_env, parse_u64_env};
use super::market_velocity_backfill::build_okx_http_client;
use super::market_velocity_event_backtest::{
    select_live_entry_from_signal_shell, FvgEntryMode, MarketVelocityEventBacktestArgs,
};
use super::market_velocity_strategy_config::load_market_velocity_signal_config_or_env;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_quant_domain::entities::MarketRankEvent;
use rust_quant_domain::Candle;
use rust_quant_services::market::{
    build_market_velocity_entry_confirmation_from_candles,
    build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry,
    MarketVelocityEntryConfirmation, MarketVelocityEntryConfirmationDecision,
    MarketVelocityFvgEntryMode, MarketVelocitySelectedEntry, MarketVelocityStrategySignalConfig,
    MarketVelocityStrategySignalDecision,
};
use rust_quant_services::rust_quan_web::{
    build_market_velocity_scoped_worker_handoff_readiness,
    market_velocity_existing_execution_worker_path, ExecutionTaskClient, ExecutionTaskConfig,
    QuantWebClientError, StrategySignalSubmitRequest,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{collections::BTreeMap, time::Duration};
mod candidates;
mod entry_candles;
mod handoff;
use candidates::{load_market_velocity_live_candidate_events, normalize_candidate_limit};
use entry_candles::{load_market_velocity_live_entry_candles, MarketVelocityEntryCandleLoadStatus};
use handoff::market_velocity_handoff_log_context;
pub use handoff::{
    build_market_velocity_live_preview_request, build_market_velocity_live_worker_handoff,
    build_market_velocity_live_worker_manifest, market_velocity_handoff_hard_preview_blockers,
    market_velocity_live_owner_scope, market_velocity_required_live_owner_scope,
    market_velocity_scope_signal_to_live_owner,
    market_velocity_scope_signal_to_live_owner_if_configured,
};
const DEFAULT_OKX_REST_BASE: &str = "https://www.okx.com";
const DEFAULT_ENTRY_CANDLE_MAX_STALENESS_MINUTES: i64 = 45;
const DEFAULT_ENTRY_CANDLE_REQUEST_SLEEP_MS: u64 = 150;
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityLiveHandoffConfig {
    /// databaseURL，用于配置运行参数。
    pub database_url: String,
    /// web基础URL，用于配置运行参数。
    pub web_base_url: String,
    /// internalSecret，用于配置运行参数。
    pub internal_secret: String,
    /// 买家邮箱；为空时表示未绑定买家邮箱。
    pub buyer_email: Option<String>,
    /// combo ID；为空时使用默认值或表示不限制。
    pub combo_id: Option<i64>,
    /// API 凭证 ID。
    pub credential_id: Option<i64>,
    /// event ID；为空时使用默认值或表示不限制。
    pub event_id: Option<i64>,
    /// 小时级时长。
    pub lookback_hours: i64,
    /// candidatelimit，用于配置运行参数。
    pub candidate_limit: u32,
    /// 入场K 线最大staleness 分钟数。
    pub entry_candle_max_staleness_minutes: i64,
    /// 入场K 线ondemandrefresh，用于配置运行参数。
    pub entry_candle_on_demand_refresh: bool,
    /// 入场K 线okxrest基础，用于配置运行参数。
    pub entry_candle_okx_rest_base: String,
    /// 入场K 线proxyURL；为空时使用默认值或表示不限制。
    pub entry_candle_proxy_url: Option<String>,
    /// 毫秒级时间戳或时长。
    pub entry_candle_request_sleep_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketVelocityLiveHandoffRuntimeConfig {
    /// runonce，用于配置运行参数。
    pub run_once: bool,
    /// 秒级时长。
    pub interval_seconds: u64,
}

/// 表示 rank event 最近一次 live handoff 评估结果，独立于通知投递状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarketVelocityLiveHandoffState {
    Blocked,
    Expired,
    Created,
    Failed,
}

impl MarketVelocityLiveHandoffState {
    /// 返回数据库状态字面量，避免 SQL 更新和测试各自硬编码。
    fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Expired => "expired",
            Self::Created => "created",
            Self::Failed => "failed",
        }
    }
}
impl MarketVelocityLiveHandoffConfig {
    /// 判断是否进入单 combo canary 模式；默认空 scope 走 Web 订阅 fan-out。
    fn owner_scope_configured(&self) -> Result<bool> {
        market_velocity_live_owner_scope(self.buyer_email.as_deref(), self.combo_id)
            .map(|scope| scope.is_some())
    }
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
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
/// 提供市场动量livehandoff配置from环境变量的集中实现，避免行情数据调用方重复处理相同细节。
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
        )?
        .max(DEFAULT_ENTRY_CANDLE_REQUEST_SLEEP_MS),
    })
}
/// 提供市场动量livehandoffruntime配置from环境变量的集中实现，避免行情数据调用方重复处理相同细节。
pub fn market_velocity_live_handoff_runtime_config_from_env(
) -> Result<MarketVelocityLiveHandoffRuntimeConfig> {
    let envs = std::env::vars().collect::<BTreeMap<_, _>>();
    market_velocity_live_handoff_runtime_config_from_map(&envs)
}
/// 提供市场动量livehandoffruntime配置frommap的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn run_market_velocity_live_handoff(
    config: MarketVelocityLiveHandoffConfig,
) -> Result<Value> {
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url: config.web_base_url.clone(),
        internal_secret: config.internal_secret.clone(),
    })?;
    let owner_scope_configured = config.owner_scope_configured()?;
    let credential_readiness_applies = config.credential_id.is_some() && owner_scope_configured;
    let mut refresh_readiness = json!({
        "apply": credential_readiness_applies,
        "mutation_scope": "web_signed_readonly_preflight_snapshot_refresh_only",
        "exchange_mutation_allowed": false,
    });
    if let (true, Some(credential_id)) = (credential_readiness_applies, config.credential_id) {
        match client.check_internal_api_credential(credential_id).await {
            Ok(credential) => {
                refresh_readiness["credential_id"] = json!(credential.id);
                refresh_readiness["last_check_code"] = json!(credential.last_check_code);
                refresh_readiness["execution_readiness"] =
                    json!(credential.execution_readiness.can_execute);
            }
            Err(error) => {
                let Some(blocker_code) = api_credential_readiness_blocker_code(&error) else {
                    return Err(error);
                };
                refresh_readiness["credential_id"] = json!(credential_id);
                refresh_readiness["status"] = json!("blocked");
                refresh_readiness["blocker_code"] = json!(blocker_code);
                refresh_readiness["execution_readiness"] = json!(false);
                return Ok(
                    build_market_velocity_api_credential_readiness_blocked_response(
                        &config,
                        refresh_readiness,
                        &blocker_code,
                    ),
                );
            }
        }
    }
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core database for market velocity live handoff")?;
    let signal_config = load_market_velocity_signal_config_or_env(&pool).await?;
    validate_market_velocity_live_handoff_signal_config(&signal_config)?;
    let candidate_events = load_market_velocity_live_candidate_events(
        &pool,
        config.event_id,
        config.lookback_hours,
        config.candidate_limit,
        &signal_config,
    )
    .await?;
    tracing::info!(
        candidate_count = candidate_events.len(),
        explicit_event_id = ?config.event_id,
        lookback_hours = config.lookback_hours,
        candidate_limit = config.candidate_limit,
        "Market Velocity live handoff candidate scan completed"
    );
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
        Option<MarketVelocitySelectedEntry>,
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
                let blocker_code = "market_velocity_entry_candles_unavailable";
                let blocker_detail = error.to_string();
                record_market_velocity_live_handoff_blocker(
                    &pool,
                    &event,
                    blocker_code,
                    &blocker_detail,
                )
                .await?;
                skipped_candidates.push(json!({
                    "event_id": event.id,
                    "symbol": event.symbol,
                    "blocker_code": blocker_code,
                    "blocker_detail": blocker_detail,
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
            Err(error) => {
                let blocker_code = "market_velocity_entry_candles_unavailable";
                let blocker_detail = error.to_string();
                record_market_velocity_live_handoff_blocker(
                    &pool,
                    &event,
                    blocker_code,
                    &blocker_detail,
                )
                .await?;
                return Err(error);
            }
        };
        let candles = candle_load.candles.clone();
        let now = Utc::now();
        let (entry_confirmation, selected_entry) = if signal_config.hybrid_live_entry_enabled() {
            match select_market_velocity_live_entry(&event, &candles, &signal_config) {
                Ok(selection) => {
                    if let Some(blocker_detail) = market_velocity_selected_entry_stale_blocker(
                        &selection.selected_entry,
                        now,
                        config.entry_candle_max_staleness_minutes,
                    ) {
                        let blocker_code = "market_velocity_selected_entry_stale";
                        record_market_velocity_live_handoff_blocker(
                            &pool,
                            &event,
                            blocker_code,
                            &blocker_detail,
                        )
                        .await?;
                        skipped_candidates.push(json!({
                            "event_id": event.id,
                            "symbol": event.symbol,
                            "blocker_code": blocker_code,
                            "blocker_detail": blocker_detail,
                            "selected_entry": selection.selected_entry,
                            "entry_candles": candle_load.status,
                        }));
                        continue;
                    }
                    (selection.entry_confirmation, Some(selection.selected_entry))
                }
                Err(blocker_detail) => {
                    let blocker_code = "market_velocity_live_entry_shell_blocked";
                    record_market_velocity_live_handoff_blocker(
                        &pool,
                        &event,
                        blocker_code,
                        &blocker_detail,
                    )
                    .await?;
                    skipped_candidates.push(json!({
                        "event_id": event.id,
                        "symbol": event.symbol,
                        "blocker_code": blocker_code,
                        "blocker_detail": blocker_detail,
                        "entry_candles": candle_load.status,
                    }));
                    continue;
                }
            }
        } else {
            let entry_confirmation = match build_market_velocity_entry_confirmation_from_candles(
                "15m",
                &candles,
                &signal_config.entry_confirmation_config(),
            ) {
                MarketVelocityEntryConfirmationDecision::Confirmed(confirmation) => confirmation,
                MarketVelocityEntryConfirmationDecision::Blocked(blocker) => {
                    let blocker_code = "market_velocity_entry_confirmation_blocked";
                    let blocker_detail = format!("{:?}", blocker);
                    record_market_velocity_live_handoff_blocker(
                        &pool,
                        &event,
                        blocker_code,
                        &blocker_detail,
                    )
                    .await?;
                    skipped_candidates.push(json!({
                        "event_id": event.id,
                        "symbol": event.symbol,
                        "blocker_code": blocker_code,
                        "blocker_detail": blocker_detail,
                        "entry_candles": candle_load.status,
                    }));
                    continue;
                }
            };
            if let Some(blocker_detail) = market_velocity_entry_confirmation_stale_blocker(
                &entry_confirmation,
                now,
                config.entry_candle_max_staleness_minutes,
            ) {
                let blocker_code = "market_velocity_entry_confirmation_stale";
                record_market_velocity_live_handoff_blocker(
                    &pool,
                    &event,
                    blocker_code,
                    &blocker_detail,
                )
                .await?;
                skipped_candidates.push(json!({
                    "event_id": event.id,
                    "symbol": event.symbol,
                    "blocker_code": blocker_code,
                    "blocker_detail": blocker_detail,
                    "snapshot_at": entry_confirmation.snapshot_at,
                    "entry_candles": candle_load.status,
                }));
                continue;
            }
            (entry_confirmation, None)
        };
        let signal =
            match build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
                &event,
                &signal_config,
                Some(&entry_confirmation),
                selected_entry.as_ref(),
            )? {
            MarketVelocityStrategySignalDecision::Submit(signal) => signal,
            MarketVelocityStrategySignalDecision::Blocked(blocker) => {
                let blocker_code = format!("market_velocity_signal_{:?}", blocker);
                let blocker_detail = blocker_code.clone();
                record_market_velocity_live_handoff_blocker(
                    &pool,
                    &event,
                    &blocker_code,
                    &blocker_detail,
                )
                .await?;
                skipped_candidates.push(json!({
                    "event_id": event.id,
                    "symbol": event.symbol,
                    "blocker_code": blocker_code,
                    "blocker_detail": blocker_detail,
                    "selected_entry": selected_entry,
                    "entry_candles": candle_load.status,
                }));
                continue;
            }
        };
        selected = Some((
            event,
            entry_confirmation,
            selected_entry,
            signal,
            candle_load.status,
        ));
        break;
    }
    let Some((event, entry_confirmation, selected_entry, signal, candle_load)) = selected else {
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
    let log_context = market_velocity_handoff_log_context(&signal, None);
    tracing::info!(
        external_id = %log_context.external_id,
        source_signal_type = %log_context.source_signal_type,
        rank_event_id = ?log_context.rank_event_id,
        exchange = %log_context.exchange,
        symbol = %log_context.symbol,
        skipped_candidate_count = skipped_candidates.len(),
        entry_trigger = %selected_entry
            .as_ref()
            .map(|entry| entry.trigger.as_str())
            .unwrap_or(entry_confirmation.trigger.as_str()),
        "Market Velocity live handoff selected signal candidate"
    );
    let preview_request = build_market_velocity_live_preview_request(
        &signal,
        config.buyer_email.as_deref(),
        config.combo_id,
    )?;
    let preview = client
        .preview_market_velocity_execution_task_creation(preview_request)
        .await?;
    tracing::info!(
        external_id = %log_context.external_id,
        source_signal_type = %log_context.source_signal_type,
        rank_event_id = ?log_context.rank_event_id,
        exchange = %log_context.exchange,
        symbol = %log_context.symbol,
        preview_status = %preview.status,
        would_create_execution_task = preview.would_create_execution_task,
        generated_execution_task_count = preview.generated_execution_task_count,
        blocker_codes = ?preview.blocker_codes,
        "Market Velocity live handoff owner preview completed"
    );
    let hard_preview_blockers = market_velocity_handoff_hard_preview_blockers(
        &preview.blocker_codes,
        owner_scope_configured,
    );
    let skipped_summary = summarize_skipped_candidates(&skipped_candidates);
    let mut response = json!({
        "status": if hard_preview_blockers.is_empty() { "ready_for_task_creation" } else { "blocked" },
        "read_only": false,
        "mutation_allowed": true,
        "exchange_mutation_allowed": false,
        "creates_new_order_system": false,
        "owner_scope_configured": owner_scope_configured,
        "hard_preview_blocker_codes": hard_preview_blockers.clone(),
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
            "selected_entry": selected_entry,
            "entry_candles": candle_load,
        },
        "web_owner_preview": preview,
        "execution_path": market_velocity_existing_execution_worker_path(),
        "refresh_readiness": refresh_readiness,
    });
    if !hard_preview_blockers.is_empty() {
        let blocker_code = "market_velocity_owner_preview_blocked";
        let blocker_detail = hard_preview_blockers.join(",");
        let event_id = event
            .id
            .context("market rank event missing id for live handoff preview blocker update")?;
        update_market_velocity_live_handoff_status(
            &pool,
            event_id,
            MarketVelocityLiveHandoffState::Blocked,
            Some(blocker_code),
            Some(&blocker_detail),
        )
        .await?;
        response["blocker_code"] = json!(blocker_code);
        response["blocker_detail"] = json!(blocker_detail);
        response["read_only"] = json!(true);
        response["mutation_allowed"] = json!(false);
        return Ok(response);
    }
    let signal = market_velocity_scope_signal_to_live_owner_if_configured(
        signal,
        config.buyer_email.as_deref(),
        config.combo_id,
    )?;
    let dispatch_log_signal = signal.clone();
    let dispatch = match client.submit_strategy_signal(signal).await {
        Ok(dispatch) => dispatch,
        Err(error) => {
            let event_id = event
                .id
                .context("market rank event missing id for live handoff submit failure update")?;
            let blocker_detail = error.to_string();
            update_market_velocity_live_handoff_status(
                &pool,
                event_id,
                MarketVelocityLiveHandoffState::Failed,
                Some("market_velocity_submit_strategy_signal_failed"),
                Some(&blocker_detail),
            )
            .await?;
            return Err(error);
        }
    };
    let event_id = event
        .id
        .context("market rank event missing id for live handoff created update")?;
    update_market_velocity_live_handoff_status(
        &pool,
        event_id,
        MarketVelocityLiveHandoffState::Created,
        None,
        None,
    )
    .await?;
    let generated_task_ids: Vec<i64> = dispatch
        .generated_tasks
        .iter()
        .map(|task| task.id)
        .collect();
    let task_log_context =
        market_velocity_handoff_log_context(&dispatch_log_signal, dispatch.generated_tasks.first());
    tracing::info!(
        external_id = %task_log_context.external_id,
        source_signal_type = %task_log_context.source_signal_type,
        rank_event_id = ?task_log_context.rank_event_id,
        strategy_signal_id = dispatch.inbox.id,
        first_execution_task_id = ?task_log_context.execution_task_id,
        combo_id = ?task_log_context.combo_id,
        buyer_email = ?task_log_context.buyer_email,
        exchange = %task_log_context.exchange,
        symbol = %task_log_context.symbol,
        generated_task_count = dispatch.generated_tasks.len(),
        generated_task_ids = ?generated_task_ids,
        "Market Velocity live handoff created Web execution task"
    );
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
    response["status"] = json!("execution_task_created");
    response["read_only"] = json!(false);
    response["mutation_allowed"] = json!(true);
    response["strategy_signal_id"] = json!(dispatch.inbox.id);
    response["generated_tasks"] = json!(dispatch.generated_tasks);
    response["next_worker_handoff"] = next_worker.unwrap_or_else(|| json!(null));
    Ok(response)
}
/// 在 Core live handoff 入口阻断仍处于 paper-only 的破位做空配置，避免误配置绕过 compose/Web 门禁。
fn validate_market_velocity_live_handoff_signal_config(
    config: &MarketVelocityStrategySignalConfig,
) -> Result<()> {
    let normalized = [
        config.strategy_slug.as_str(),
        config.strategy_preset.as_str(),
        config.entry_rule_version.as_str(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    if [
        "market_velocity_breakdown_short",
        "support_breakdown",
        "breakdown_short",
        "15m_short",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        bail!(
            "market_velocity_live_handoff rejects paper-only breakdown short signal config; run market_velocity_paper_observation instead"
        );
    }
    Ok(())
}
/// 将 Web 的 API Key 业务 blocker 降级为 live handoff 的正常 blocked 响应，避免 scheduler 重复刷 ERROR。
fn api_credential_readiness_blocker_code(error: &anyhow::Error) -> Option<String> {
    error
        .downcast_ref::<QuantWebClientError>()
        .and_then(QuantWebClientError::error_code)
        .filter(|code| matches!(*code, "ACTIVE_MEMBERSHIP_REQUIRED" | "MEMBERSHIP_EXPIRED"))
        .map(str::to_string)
}

/// 构造 API Key readiness blocked 响应；不连接 Core DB，也不创建信号或执行任务。
fn build_market_velocity_api_credential_readiness_blocked_response(
    config: &MarketVelocityLiveHandoffConfig,
    refresh_readiness: Value,
    blocker_code: &str,
) -> Value {
    json!({
        "status": "blocked",
        "blocker_code": blocker_code,
        "read_only": true,
        "mutation_allowed": false,
        "exchange_mutation_allowed": false,
        "candidate_scan": {
            "limit": config.candidate_limit,
            "evaluated": 0,
            "lookback_hours": config.lookback_hours,
            "skipped": 0,
            "explicit_event_id": config.event_id,
        },
        "refresh_readiness": refresh_readiness,
        "execution_path": market_velocity_existing_execution_worker_path(),
        "next_action": "restore_api_credential_readiness_before_live_handoff",
    })
}

/// 根据 blocker 语义映射 handoff 状态；过期类 blocker 是终态，不再反复重扫。
fn market_velocity_live_handoff_state_for_blocker(
    blocker_code: &str,
) -> MarketVelocityLiveHandoffState {
    if blocker_code.contains("_stale") {
        MarketVelocityLiveHandoffState::Expired
    } else {
        MarketVelocityLiveHandoffState::Blocked
    }
}

/// 更新 rank event 的交易 handoff 状态；不复用通知状态，避免混淆诊断语义。
fn market_velocity_live_handoff_status_update_sql() -> &'static str {
    r#"
        UPDATE market_rank_events
        SET live_handoff_state = $2,
            live_handoff_blocker_code = $3,
            live_handoff_blocker_detail = $4,
            live_handoff_last_evaluated_at = NOW()
        WHERE id = $1
        "#
}

/// 持久化单个候选最近一次 live handoff 评估结果，供后续扫描和排障使用。
async fn update_market_velocity_live_handoff_status(
    pool: &PgPool,
    event_id: i64,
    state: MarketVelocityLiveHandoffState,
    blocker_code: Option<&str>,
    blocker_detail: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(market_velocity_live_handoff_status_update_sql())
        .bind(event_id)
        .bind(state.as_str())
        .bind(blocker_code)
        .bind(blocker_detail)
        .execute(pool)
        .await
        .context("update market velocity live handoff state")?;
    if result.rows_affected() == 0 {
        bail!("market_rank_events row not found for live handoff status update: {event_id}");
    }
    Ok(())
}

/// 将当前候选的 blocker 写回 rank event，避免同一过期候选永久停在 pending。
async fn record_market_velocity_live_handoff_blocker(
    pool: &PgPool,
    event: &MarketRankEvent,
    blocker_code: &str,
    blocker_detail: &str,
) -> Result<()> {
    let event_id = event
        .id
        .context("market rank event missing id for live handoff status update")?;
    update_market_velocity_live_handoff_status(
        pool,
        event_id,
        market_velocity_live_handoff_state_for_blocker(blocker_code),
        Some(blocker_code),
        Some(blocker_detail),
    )
    .await
}
/// 提供市场动量入场确认staleblocker的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供市场动量入场确认ageminutes的集中实现，避免行情数据调用方重复处理相同细节。
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

#[derive(Debug, Clone, PartialEq)]
struct MarketVelocityHybridLiveSelection {
    entry_confirmation: MarketVelocityEntryConfirmation,
    selected_entry: MarketVelocitySelectedEntry,
}

fn select_market_velocity_live_entry(
    event: &MarketRankEvent,
    candles: &[Candle],
    config: &MarketVelocityStrategySignalConfig,
) -> Result<MarketVelocityHybridLiveSelection, String> {
    let event_ts = event.detected_at.timestamp_millis();
    let current_price = event
        .current_price
        .and_then(|value| value.to_f64())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| "live_signal_shell_missing_current_price".to_string())?;
    let backtest_args = market_velocity_live_shell_args(config);
    let raw_candles = candles_to_backtest_candles(candles);
    let selection =
        select_live_entry_from_signal_shell(event_ts, current_price, &raw_candles, &backtest_args)?;
    let signal_candles = candles
        .get(..=selection.signal_idx)
        .ok_or_else(|| "live_signal_shell_missing_signal_candles".to_string())?;
    let entry_confirmation = match build_market_velocity_entry_confirmation_from_candles(
        "15m",
        signal_candles,
        &config.entry_confirmation_config(),
    ) {
        MarketVelocityEntryConfirmationDecision::Confirmed(confirmation) => confirmation,
        MarketVelocityEntryConfirmationDecision::Blocked(blocker) => {
            return Err(format!(
                "live_signal_shell_confirmation_rebuild_{:?}",
                blocker
            ));
        }
    };
    if entry_confirmation.trigger != selection.signal_trigger {
        return Err(format!(
            "live_signal_shell_trigger_mismatch:{}!={}",
            entry_confirmation.trigger, selection.signal_trigger
        ));
    }
    let entry_ts = DateTime::from_timestamp_millis(selection.entry_ts)
        .ok_or_else(|| "live_signal_shell_invalid_entry_ts".to_string())?;
    Ok(MarketVelocityHybridLiveSelection {
        entry_confirmation,
        selected_entry: MarketVelocitySelectedEntry {
            entry_price: selection.entry_price,
            entry_ts,
            trigger: selection.entry_trigger.clone(),
            entry_path: market_velocity_selected_entry_path(&selection.entry_trigger),
            signal_pullback_pct: market_velocity_selected_entry_pullback_pct(
                current_price,
                selection.entry_price,
            ),
            structure_stop_loss_price: selection.structure_stop_loss_price,
            structure_stop_loss_source: selection.structure_stop_loss_source,
        },
    })
}

fn market_velocity_live_shell_args(
    config: &MarketVelocityStrategySignalConfig,
) -> MarketVelocityEventBacktestArgs {
    MarketVelocityEventBacktestArgs {
        stop_loss_pct: config.stop_loss_pct,
        target_rs: vec![config.take_profit_r],
        entry_period: config.entry_confirmation_period,
        entry_max_distance_pct: config.entry_max_average_distance_pct,
        entry_min_volume_ratio: config.entry_min_volume_ratio,
        entry_min_rsi: config.entry_min_rsi,
        entry_max_rsi: config.entry_max_rsi,
        entry_min_rsi_delta: config.entry_min_rsi_delta,
        entry_rsi_delta_lookback_candles: config.entry_rsi_delta_lookback_candles,
        entry_bollinger_breakout: config.entry_bollinger_breakout,
        entry_min_bollinger_bandwidth_expansion_pct: config
            .entry_min_bollinger_bandwidth_expansion_pct,
        entry_min_recent_drawdown_pct: config.entry_min_recent_drawdown_pct,
        entry_recent_drawdown_lookback_candles: config.entry_recent_drawdown_lookback_candles,
        entry_max_signal_pullback_pct: config.entry_max_signal_pullback_pct,
        entry_retest_tolerance_pct: config.entry_retest_tolerance_pct,
        entry_retest_after_signal: config.entry_retest_after_signal,
        entry_retest_max_wait_candles: config.entry_retest_max_wait_candles,
        entry_retest_min_entry_open_gap_pct: config.entry_retest_min_entry_open_gap_pct,
        entry_retest_open_fade_min_volume_ratio: config.entry_retest_open_fade_min_volume_ratio,
        fvg_entry_mode: match config.fvg_entry_mode {
            MarketVelocityFvgEntryMode::Off => FvgEntryMode::Off,
            MarketVelocityFvgEntryMode::M15ImpulseRetrace => FvgEntryMode::M15ImpulseRetrace,
        },
        fvg_lookback_candles: config.fvg_lookback_candles,
        fvg_max_wait_candles: config.fvg_max_wait_candles,
        fvg_impulse_retrace_fill_pct: config.fvg_impulse_retrace_fill_pct,
        fvg_impulse_retrace_min_wait_candles: config.fvg_impulse_retrace_min_wait_candles,
        entry_trigger_allowlist: config.entry_trigger_allowlist.clone(),
        entry_trigger_blocklist: config.entry_trigger_blocklist.clone(),
        symbol_blocklist: config.symbol_blocklist.clone(),
        max_15m_staleness_min: DEFAULT_ENTRY_CANDLE_MAX_STALENESS_MINUTES,
        ..MarketVelocityEventBacktestArgs::default()
    }
}

fn candles_to_backtest_candles(
    candles: &[Candle],
) -> Vec<super::market_velocity_event_backtest::BacktestCandle> {
    candles
        .iter()
        .map(
            |candle| super::market_velocity_event_backtest::BacktestCandle {
                ts: candle.timestamp,
                open: candle.open.value(),
                high: candle.high.value(),
                low: candle.low.value(),
                close: candle.close.value(),
                volume: candle.volume.value(),
            },
        )
        .collect()
}

fn market_velocity_selected_entry_stale_blocker(
    selected_entry: &MarketVelocitySelectedEntry,
    now: DateTime<Utc>,
    max_staleness_minutes: i64,
) -> Option<String> {
    if max_staleness_minutes <= 0 {
        return None;
    }
    let age_seconds = now
        .signed_duration_since(selected_entry.entry_ts)
        .num_seconds()
        .max(0);
    let age_minutes = (age_seconds + 59) / 60;
    (age_minutes > max_staleness_minutes)
        .then(|| format!("SelectedEntryStale:{age_minutes}m>{max_staleness_minutes}m"))
}

fn market_velocity_selected_entry_path(trigger: &str) -> String {
    if trigger.contains("+fvg_15m_impulse_retrace") {
        "fvg_15m_impulse_retrace".to_string()
    } else if trigger.contains("+retest_after_signal") {
        "retest_after_signal".to_string()
    } else {
        "signal_confirmation".to_string()
    }
}

fn market_velocity_selected_entry_pullback_pct(
    current_price: f64,
    entry_price: f64,
) -> Option<f64> {
    if current_price <= 0.0 || entry_price <= 0.0 || entry_price >= current_price {
        return None;
    }
    Some(((current_price - entry_price) / current_price * 100.0 * 1000.0).round() / 1000.0)
}
/// 生成 行情与市场数据 需要的派生数据，供后续执行、展示或审计使用。
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
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
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
            "entry_candle_on_demand_refresh": config.entry_candle_on_demand_refresh,
        },
        "next_action": "wait_for_next_market_velocity_event",
        "execution_path": market_velocity_existing_execution_worker_path(),
        "refresh_readiness": refresh_readiness,
    })
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
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
    use crate::app::market_velocity_event_backtest::{parse_paper_observation_args_from, MS_15M};
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use rust_quant_domain::{Price, Timeframe, Volume};
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
    ];
    /// 封装环境变量lock，减少行情数据调用方重复实现相同细节。
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }
    /// 提供快照livehandoff环境变量的集中实现，避免行情数据调用方重复处理相同细节。
    fn snapshot_live_handoff_env() -> Vec<(&'static str, Option<String>)> {
        LIVE_HANDOFF_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect()
    }
    /// 提供restore环境变量的集中实现，避免行情数据调用方重复处理相同细节。
    fn restore_env(snapshot: Vec<(&'static str, Option<String>)>) {
        for (key, value) in snapshot {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
    /// 删除或清理 行情与市场数据 的临时数据，避免过期状态继续影响后续流程。
    fn clear_live_handoff_env() {
        for key in LIVE_HANDOFF_ENV_KEYS {
            std::env::remove_var(key);
        }
    }
    /// 构造样例实盘handoffconfig，集中维护行情数据的载荷组装规则。
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
        }
    }

    fn sample_hybrid_signal_config() -> MarketVelocityStrategySignalConfig {
        MarketVelocityStrategySignalConfig {
            require_technical_confirmation: false,
            require_entry_confirmation: true,
            entry_confirmation_period: 3,
            entry_max_average_distance_pct: 20.0,
            entry_min_volume_ratio: 1.2,
            entry_max_signal_pullback_pct: Some(3.0),
            entry_retest_tolerance_pct: 0.3,
            entry_retest_after_signal: true,
            entry_retest_max_wait_candles: 1,
            fvg_entry_mode: MarketVelocityFvgEntryMode::M15ImpulseRetrace,
            fvg_max_wait_candles: 6,
            entry_trigger_allowlist: vec!["reclaim_ema".to_string()],
            ..MarketVelocityStrategySignalConfig::default()
        }
    }

    fn sample_live_event(ts: i64, current_price: f64) -> MarketRankEvent {
        MarketRankEvent {
            id: Some(99),
            exchange: "okx".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            event_type: rust_quant_domain::entities::MarketRankEventType::RankVelocity,
            timeframe: Some("15分钟".to_string()),
            old_rank: Some(30),
            new_rank: Some(18),
            delta_rank: Some(22),
            volume_24h_quote: Some(Decimal::new(120_000_000, 0)),
            current_price: Some(Decimal::from_f64_retain(current_price).unwrap()),
            previous_price: Some(Decimal::new(100, 0)),
            price_change_pct: Some(Decimal::new(650, 2)),
            price_direction: "up".to_string(),
            technical_snapshot_status: "captured".to_string(),
            technical_snapshot: None,
            detected_at: DateTime::from_timestamp_millis(ts).unwrap(),
            source: "scanner_service".to_string(),
            notification_state: "pending".to_string(),
        }
    }

    fn sample_entry_candle(
        ts: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Candle {
        let mut candle = Candle::new(
            "ETH-USDT-SWAP".to_string(),
            Timeframe::M15,
            ts,
            Price::new(open).unwrap(),
            Price::new(high).unwrap(),
            Price::new(low).unwrap(),
            Price::new(close).unwrap(),
            Volume::new(volume).unwrap(),
        );
        candle.confirm();
        candle
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
        assert_eq!(config.entry_candle_request_sleep_ms, 150);
    }
    #[test]
    fn live_handoff_config_keeps_entry_candle_fetch_throttled_when_env_is_zero() {
        let _guard = env_lock();
        let snapshot = snapshot_live_handoff_env();
        clear_live_handoff_env();
        std::env::set_var(
            "QUANT_CORE_DATABASE_URL",
            "postgres://postgres:postgres123@localhost:5432/quant_core",
        );
        std::env::set_var("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:18000");
        std::env::set_var("EXECUTION_EVENT_SECRET", "local-dev-secret");
        std::env::set_var("MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS", "0");
        let config = market_velocity_live_handoff_config_from_env().expect("config");
        restore_env(snapshot);
        assert_eq!(
            config.entry_candle_request_sleep_ms,
            DEFAULT_ENTRY_CANDLE_REQUEST_SLEEP_MS
        );
    }
    #[test]
    fn live_handoff_config_without_owner_scope_is_broadcast_even_with_legacy_credential_id() {
        let mut config = sample_live_handoff_config();
        config.buyer_email = None;
        config.combo_id = None;
        config.credential_id = Some(1);

        assert!(!config.owner_scope_configured().expect("owner scope"));
    }
    #[test]
    fn live_handoff_config_rejects_partial_owner_scope() {
        let mut missing_combo = sample_live_handoff_config();
        missing_combo.combo_id = None;
        assert!(missing_combo.owner_scope_configured().is_err());

        let mut missing_buyer = sample_live_handoff_config();
        missing_buyer.buyer_email = None;
        assert!(missing_buyer.owner_scope_configured().is_err());
    }
    #[test]
    fn live_handoff_rejects_breakdown_short_signal_config_before_candidate_scan() {
        let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
            &json!({
                "strategy_slug": "market_velocity_breakdown_short",
                "strategy_preset": "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1",
                "entry_rule_version": "rank_radar_15m_short_r0375_10r_15msup_brkdn_d5_72_p1p5_12_v1",
                "trade_direction": "short"
            }),
            &json!({}),
        )
        .expect("short config");

        let error = validate_market_velocity_live_handoff_signal_config(&config)
            .expect_err("breakdown short must stay out of live handoff");

        assert!(
            error.to_string().contains("paper-only breakdown short"),
            "unexpected error: {error:#}"
        );
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
    #[tokio::test]
    async fn live_handoff_returns_blocked_when_api_credential_membership_expired() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let request = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..request]).to_string();
            assert!(
                request.starts_with("POST /api/commerce/internal/api-credentials/1/check HTTP/1.1")
            );
            let body = r#"{"success":false,"code":"MEMBERSHIP_EXPIRED","message":"内部校验 API Key 失败: 会员已过期"}"#;
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let mut config = sample_live_handoff_config();
        config.web_base_url = format!("http://{}", addr);
        config.database_url = "postgres://invalid:invalid@127.0.0.1:1/quant_core".to_string();

        let response = run_market_velocity_live_handoff(config)
            .await
            .expect("membership blocker should be a readiness response, not a scheduler error");
        server.await.unwrap();

        assert_eq!(response["status"], "blocked");
        assert_eq!(response["blocker_code"], "MEMBERSHIP_EXPIRED");
        assert_eq!(response["refresh_readiness"]["status"], "blocked");
        assert_eq!(
            response["refresh_readiness"]["blocker_code"],
            "MEMBERSHIP_EXPIRED"
        );
        assert_eq!(response["mutation_allowed"], false);
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
    fn live_entry_shell_uses_impulse_fvg_primary_when_fill_arrives() {
        let mut config = sample_hybrid_signal_config();
        config.entry_trigger_allowlist = vec!["breakout_previous_high".to_string()];
        let base_ts = 4 * 60 * 60 * 1_000 * 4;
        let event = sample_live_event(base_ts + 15 * 60 * 1_000 * 6, 105.0);
        let candles = vec![
            sample_entry_candle(base_ts, 100.0, 101.0, 99.5, 100.5, 10.0),
            sample_entry_candle(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5, 10.0),
            sample_entry_candle(base_ts + MS_15M * 2, 101.5, 103.0, 101.0, 102.5, 20.0),
            sample_entry_candle(base_ts + MS_15M * 3, 102.5, 104.0, 102.0, 103.0, 30.0),
            sample_entry_candle(base_ts + MS_15M * 4, 103.1, 106.0, 103.0, 105.0, 40.0),
            sample_entry_candle(base_ts + MS_15M * 5, 106.2, 109.0, 106.5, 108.4, 50.0),
            sample_entry_candle(base_ts + MS_15M * 6, 108.5, 110.0, 107.2, 108.0, 60.0),
            sample_entry_candle(base_ts + MS_15M * 7, 108.0, 108.4, 104.9, 105.6, 30.0),
            sample_entry_candle(base_ts + MS_15M * 8, 105.2, 105.4, 104.4, 104.6, 20.0),
            sample_entry_candle(base_ts + MS_15M * 9, 104.6, 106.0, 104.4, 105.5, 10.0),
        ];
        let selection = select_market_velocity_live_entry(&event, &candles, &config)
            .expect("hybrid live entry selection");
        assert_eq!(
            selection.entry_confirmation.trigger,
            "breakout_previous_high"
        );
        assert_eq!(selection.selected_entry.entry_price, 104.5);
        assert_eq!(
            selection.selected_entry.trigger,
            "breakout_previous_high+fvg_15m_impulse_retrace"
        );
        assert_eq!(
            selection.selected_entry.entry_path,
            "fvg_15m_impulse_retrace"
        );
        let selected_entry_json =
            serde_json::to_value(&selection.selected_entry).expect("selected entry json");
        assert_eq!(selected_entry_json["structure_stop_loss_price"], 104.0);
        assert_eq!(
            selected_entry_json["structure_stop_loss_source"],
            "fvg_15m_impulse_lower"
        );
    }

    #[test]
    fn live_entry_shell_falls_back_to_retest_after_signal() {
        let config = sample_hybrid_signal_config();
        let base_ts = 4 * 60 * 60 * 1_000 * 4;
        let event = sample_live_event(base_ts + MS_15M * 5, 105.0);
        let candles = vec![
            sample_entry_candle(base_ts, 100.0, 101.0, 99.5, 100.5, 10.0),
            sample_entry_candle(base_ts + MS_15M, 100.5, 102.0, 100.0, 101.5, 10.0),
            sample_entry_candle(base_ts + MS_15M * 2, 101.5, 103.0, 100.8, 102.6, 20.0),
            sample_entry_candle(base_ts + MS_15M * 3, 102.7, 103.2, 100.4, 100.9, 30.0),
            sample_entry_candle(base_ts + MS_15M * 4, 101.0, 103.6, 100.9, 103.1, 40.0),
            sample_entry_candle(base_ts + MS_15M * 5, 102.3, 103.4, 102.0, 103.0, 50.0),
            sample_entry_candle(base_ts + MS_15M * 6, 102.6, 103.5, 102.4, 103.2, 10.0),
        ];
        let selection = select_market_velocity_live_entry(&event, &candles, &config)
            .expect("hybrid live fallback selection");
        assert_eq!(selection.entry_confirmation.trigger, "reclaim_ema");
        assert_eq!(selection.selected_entry.entry_price, 102.6);
        assert_eq!(
            selection.selected_entry.trigger,
            "reclaim_ema+retest_after_signal+fvg_fallback"
        );
        assert_eq!(selection.selected_entry.entry_path, "retest_after_signal");
        assert_eq!(selection.selected_entry.signal_pullback_pct, Some(2.286));
        let selected_entry_json =
            serde_json::to_value(&selection.selected_entry).expect("selected entry json");
        let structure_stop = selected_entry_json["structure_stop_loss_price"]
            .as_f64()
            .expect("selected entry should carry structure stop");
        assert!((structure_stop - selection.entry_confirmation.ema_value).abs() < 1e-6);
        assert_eq!(
            selected_entry_json["structure_stop_loss_source"],
            "entry_confirmation_ema"
        );
    }

    #[test]
    fn production_default_hybrid_live_shell_matches_paper_preset_contract() {
        let preset = parse_paper_observation_args_from([
            "--paper-strategy-preset",
            "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2",
        ])
        .expect("paper preset");
        let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
            &json!({
                "strategy_slug": "market_velocity",
                "strategy_preset": "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2",
                "entry_rule_version": "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_pb3_vol11_d20_40_p5_10_v2",
                "min_delta_rank": 20,
                "max_delta_rank": 40,
                "min_price_change_pct": 5.0,
                "max_price_change_pct": 10.0,
                "stop_loss_pct": 0.04,
                "take_profit_r": 1.8,
                "max_holding_hours": 48,
                "require_technical_confirmation": true,
                "require_entry_confirmation": true,
                "trend_min_average_distance_pct": 0.0,
                "entry_confirmation_period": 20,
                "entry_confirmation_fetch_limit": 80,
                "entry_max_average_distance_pct": 5.0,
                "entry_min_volume_ratio": 1.1,
                "entry_max_signal_pullback_pct": 3.0,
                "entry_retest_tolerance_pct": 0.3,
                "entry_retest_after_signal": true,
                "entry_retest_max_wait_candles": 1,
                "fvg_entry_mode": "m15_impulse_retrace",
                "fvg_lookback_candles": 40,
                "fvg_max_wait_candles": 24,
                "fvg_impulse_retrace_fill_pct": 20.0,
                "fvg_impulse_retrace_min_wait_candles": 0,
                "entry_trigger_allowlist": ["reclaim_ema"],
            }),
            &json!({
                "max_loss_percent": 0.04,
                "take_profit_r": 1.8,
                "max_holding_hours": 48,
            }),
        )
        .expect("live strategy config");
        let live_shell = market_velocity_live_shell_args(&config);

        assert_eq!(live_shell.entry_period, preset.entry_period);
        assert_eq!(
            live_shell.entry_max_distance_pct,
            preset.entry_max_distance_pct
        );
        assert_eq!(
            live_shell.entry_min_volume_ratio,
            preset.entry_min_volume_ratio
        );
        assert_eq!(
            live_shell.entry_max_signal_pullback_pct,
            preset.entry_max_signal_pullback_pct
        );
        assert_eq!(
            live_shell.entry_retest_tolerance_pct,
            preset.entry_retest_tolerance_pct
        );
        assert_eq!(
            live_shell.entry_retest_after_signal,
            preset.entry_retest_after_signal
        );
        assert_eq!(
            live_shell.entry_retest_max_wait_candles,
            preset.entry_retest_max_wait_candles
        );
        assert_eq!(
            live_shell.entry_retest_min_entry_open_gap_pct,
            preset.entry_retest_min_entry_open_gap_pct
        );
        assert_eq!(
            live_shell.entry_retest_open_fade_min_volume_ratio,
            preset.entry_retest_open_fade_min_volume_ratio
        );
        assert_eq!(live_shell.fvg_entry_mode, preset.fvg_entry_mode);
        assert_eq!(live_shell.fvg_lookback_candles, preset.fvg_lookback_candles);
        assert_eq!(live_shell.fvg_max_wait_candles, preset.fvg_max_wait_candles);
        assert_eq!(
            live_shell.fvg_impulse_retrace_fill_pct,
            preset.fvg_impulse_retrace_fill_pct
        );
        assert_eq!(
            live_shell.fvg_impulse_retrace_min_wait_candles,
            preset.fvg_impulse_retrace_min_wait_candles
        );
    }

    #[test]
    fn kline15m_fast_live_shell_matches_paper_preset_contract() {
        let preset = parse_paper_observation_args_from([
            "--paper-strategy-preset",
            "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1",
        ])
        .expect("paper preset");
        let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
            &json!({
                "strategy_slug": "market_velocity",
                "strategy_preset": "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1",
                "entry_rule_version": "kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1",
                "min_delta_rank": 0,
                "max_delta_rank": null,
                "min_price_change_pct": 0.0,
                "max_price_change_pct": null,
                "stop_loss_pct": 0.04,
                "take_profit_r": 0.52,
                "max_holding_hours": 24,
                "require_technical_confirmation": false,
                "require_entry_confirmation": true,
                "trend_min_average_distance_pct": 0.0,
                "entry_confirmation_period": 20,
                "entry_confirmation_fetch_limit": 80,
                "entry_max_average_distance_pct": 14.0,
                "entry_min_volume_ratio": 1.3,
                "entry_min_rsi": 50.0,
                "entry_max_rsi": 90.0,
                "entry_bollinger_breakout": true,
                "entry_min_recent_drawdown_pct": 3.5,
                "entry_recent_drawdown_lookback_candles": 12,
                "fvg_entry_mode": "m15_impulse_retrace",
                "fvg_lookback_candles": 40,
                "fvg_max_wait_candles": 24,
                "fvg_impulse_retrace_fill_pct": 50.0,
                "fvg_impulse_retrace_min_wait_candles": 0,
                "entry_trigger_allowlist": ["breakout_previous_high"],
            }),
            &json!({
                "max_loss_percent": 0.04,
                "take_profit_r": 0.52,
                "max_holding_hours": 24,
            }),
        )
        .expect("live strategy config");
        assert_eq!(config.take_profit_r, 0.52);
        assert_eq!(config.max_delta_rank, None);
        assert_eq!(config.max_price_change_pct, None);
        let live_shell = market_velocity_live_shell_args(&config);

        assert_eq!(live_shell.stop_loss_pct, preset.stop_loss_pct);
        assert_eq!(live_shell.target_rs, preset.target_rs);
        assert_eq!(live_shell.entry_period, preset.entry_period);
        assert_eq!(
            live_shell.entry_max_distance_pct,
            preset.entry_max_distance_pct
        );
        assert_eq!(
            live_shell.entry_min_volume_ratio,
            preset.entry_min_volume_ratio
        );
        assert_eq!(live_shell.entry_min_rsi, preset.entry_min_rsi);
        assert_eq!(live_shell.entry_max_rsi, preset.entry_max_rsi);
        assert_eq!(
            live_shell.entry_bollinger_breakout,
            preset.entry_bollinger_breakout
        );
        assert_eq!(
            live_shell.entry_min_recent_drawdown_pct,
            preset.entry_min_recent_drawdown_pct
        );
        assert_eq!(
            live_shell.entry_recent_drawdown_lookback_candles,
            preset.entry_recent_drawdown_lookback_candles
        );
        assert_eq!(
            live_shell.entry_trigger_allowlist,
            preset.entry_trigger_allowlist
        );
        assert_eq!(live_shell.fvg_entry_mode, preset.fvg_entry_mode);
        assert_eq!(live_shell.fvg_lookback_candles, preset.fvg_lookback_candles);
        assert_eq!(live_shell.fvg_max_wait_candles, preset.fvg_max_wait_candles);
        assert_eq!(
            live_shell.fvg_impulse_retrace_fill_pct,
            preset.fvg_impulse_retrace_fill_pct
        );
        assert_eq!(
            live_shell.fvg_impulse_retrace_min_wait_candles,
            preset.fvg_impulse_retrace_min_wait_candles
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

    #[test]
    fn live_handoff_status_update_sql_does_not_reuse_notification_state() {
        let sql = market_velocity_live_handoff_status_update_sql();
        assert!(sql.contains("UPDATE market_rank_events"));
        assert!(sql.contains("live_handoff_state"));
        assert!(sql.contains("live_handoff_blocker_code"));
        assert!(sql.contains("live_handoff_blocker_detail"));
        assert!(sql.contains("live_handoff_last_evaluated_at"));
        assert!(
            !sql.contains("notification_state"),
            "live handoff state must not overload notification delivery state"
        );
    }

    #[test]
    fn stale_live_handoff_blockers_are_recorded_as_expired() {
        assert_eq!(
            market_velocity_live_handoff_state_for_blocker("market_velocity_selected_entry_stale")
                .as_str(),
            "expired"
        );
        assert_eq!(
            market_velocity_live_handoff_state_for_blocker(
                "market_velocity_entry_confirmation_stale"
            )
            .as_str(),
            "expired"
        );
        assert_eq!(
            market_velocity_live_handoff_state_for_blocker(
                "market_velocity_live_entry_shell_blocked"
            )
            .as_str(),
            "blocked"
        );
    }
    /// 构造样例entryconfirmation，集中维护行情数据的载荷组装规则。
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
