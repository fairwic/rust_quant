use crate::cache::default_provider;
use crate::models::{CandlesModel, TickersDataEntity};
use crate::repositories::candle_service::{CandleService, StrategyTrigger};
use crate::repositories::persist_worker::{CandlePersistWorker, PersistTask};
use crate::repositories::ticker_service::TickerService;
use crate::streams::{timeframe_duration_ms, CandleRuntimeRegistry, WatchdogDecision};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use okx::config::CONFIG;
use okx::dto::market_dto::CandleOkxRespDto;
use okx::dto::{CandleOkxWsResDto, CommonOkxWsResDto, TickerOkxResWsDto};
use okx::websocket::auto_reconnect_client::{
    AutoReconnectWebsocketClient, ConnectionState, ReconnectConfig,
};
use okx::websocket::{Args, ChannelType};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinError;
use tracing::{debug, error, info, span, warn, Level};

const WATCHDOG_POLL_INTERVAL: Duration = Duration::from_millis(500);
const WATCHDOG_BOUNDARY_OBSERVE_MS: i64 = 12_000;
const WATCHDOG_FALLBACK_QUERY_MS: i64 = 30_000;
const HEALTH_LOG_INTERVAL: Duration = Duration::from_secs(10);
const BUSINESS_MESSAGE_STALE_AFTER_MS: i64 = 15_000;

#[derive(Debug, Clone)]
struct WatchdogTarget {
    symbol: String,
    timeframe: String,
    timeframe_ms: i64,
    last_observed_candle_ts: Option<i64>,
    last_db_query_at_ms: Option<i64>,
}

impl WatchdogTarget {
    /// 只在预计收盘窗口高频查库，平时按低频兜底检查。
    fn should_query_db(&self, now_ms: i64) -> bool {
        if self.last_db_query_at_ms.is_none_or(|last_query| {
            now_ms.saturating_sub(last_query) >= WATCHDOG_FALLBACK_QUERY_MS
        }) {
            return true;
        }
        let Some(last_candle_ts) = self.last_observed_candle_ts else {
            return true;
        };
        // last_candle_ts 是“最后已确认 K 线的开盘时间”。下一次需要观察的是下一根 K 线收盘，
        // 因此要跨两个周期；只加一个周期会停在上一根的收盘点，真正边界退化成 30 秒轮询。
        let boundary_ms = last_candle_ts.saturating_add(self.timeframe_ms.saturating_mul(2));
        now_ms >= boundary_ms && now_ms.saturating_sub(boundary_ms) <= WATCHDOG_BOUNDARY_OBSERVE_MS
    }
}

/// 启动不带策略回调的 WebSocket 行情服务。
pub async fn run_socket(inst_ids: &[String], times: &[String]) {
    if let Err(error) = run_socket_with_strategy_trigger(inst_ids, times, None).await {
        error!("OKX WebSocket 行情服务退出: error={}", error);
    }
}

/// 启动带策略触发、DB watchdog 和接收任务监督的 WebSocket 服务。
pub async fn run_socket_with_strategy_trigger(
    inst_ids: &[String],
    times: &[String],
    strategy_trigger: Option<StrategyTrigger>,
) -> Result<()> {
    let span = span!(Level::DEBUG, "socket_logic");
    let _enter = span.enter();
    let mut targets = build_watchdog_targets(inst_ids, times)?;
    let runtime_registry = Arc::new(CandleRuntimeRegistry::default());
    for target in &targets {
        runtime_registry.register_target(&target.symbol, &target.timeframe);
    }

    info!("初始化 K 线批处理 Worker");
    let (persist_tx, persist_rx) = mpsc::unbounded_channel::<PersistTask>();
    let worker = CandlePersistWorker::new(persist_rx).with_config(100, Duration::from_millis(500));
    let mut persist_task = tokio::spawn(async move {
        worker.run().await;
    });

    let candle_service = if let Some(trigger) = strategy_trigger {
        Arc::new(CandleService::new_with_strategy_trigger_and_runtime(
            default_provider(),
            Some(persist_tx),
            trigger,
            Arc::clone(&runtime_registry),
        ))
    } else {
        Arc::new(CandleService::new_with_persist_worker_and_runtime(
            default_provider(),
            persist_tx,
            Arc::clone(&runtime_registry),
        ))
    };
    initialize_watchdog_baselines(&runtime_registry, &mut targets).await?;

    let public_client = AutoReconnectWebsocketClient::new_public();
    let mut public_receiver = public_client
        .start()
        .await
        .context("启动 OKX public WebSocket 失败")?;
    let business_client = AutoReconnectWebsocketClient::new_with_config(
        &CONFIG.business_websocket_url,
        None,
        ReconnectConfig::default(),
    );
    let mut business_receiver = business_client
        .start()
        .await
        .context("启动 OKX business WebSocket 失败")?;

    subscribe_candles(&business_client, inst_ids, times).await?;
    subscribe_tickers(&public_client, inst_ids).await?;

    let inst_filters = Arc::new(inst_ids.to_vec());
    let ticker_service = Arc::new(TickerService::new());
    let mut ticker_task = tokio::spawn(async move {
        while let Some(msg) = public_receiver.recv().await {
            if let Ok(ticker) = serde_json::from_value::<TickerOkxResWsDto>(msg.clone()) {
                let tickers = ticker
                    .data
                    .iter()
                    .map(TickersDataEntity::from_okx_ticker)
                    .collect::<Vec<_>>();
                if let Err(error) = ticker_service
                    .upsert_tickers(tickers, inst_filters.as_ref())
                    .await
                {
                    error!("更新 ticker 失败: error={:?}", error);
                }
            } else if let Ok(dto) = serde_json::from_value::<CommonOkxWsResDto>(msg) {
                if dto.code != "0" {
                    error!("收到 ticker 错误消息: code={}, msg={}", dto.code, dto.msg);
                } else {
                    debug!("收到 ticker 确认消息: {:?}", dto);
                }
            }
        }
        Err(anyhow!("OKX public WebSocket 接收通道已关闭"))
    });

    let candle_service_for_receiver = Arc::clone(&candle_service);
    let runtime_for_receiver = Arc::clone(&runtime_registry);
    let mut candle_task = tokio::spawn(async move {
        while let Some(msg) = business_receiver.recv().await {
            if let Ok(candle) = serde_json::from_value::<CandleOkxWsResDto>(msg.clone()) {
                let period = candle.arg.channel.replace("candle", "");
                runtime_for_receiver.record_message(
                    &candle.arg.inst_id,
                    &period,
                    Utc::now().timestamp_millis(),
                );
                let candle_data: Vec<CandleOkxRespDto> = candle
                    .data
                    .into_iter()
                    .map(CandleOkxRespDto::from_vec)
                    .collect();
                if let Err(error) = candle_service_for_receiver
                    .update_candles_batch(candle_data, &candle.arg.inst_id, &period)
                    .await
                {
                    error!(
                        "批量更新 K 线失败: inst_id={}, period={}, error={:?}",
                        candle.arg.inst_id, period, error
                    );
                }
            } else if let Ok(dto) = serde_json::from_value::<CommonOkxWsResDto>(msg) {
                if dto.code != "0" {
                    error!(
                        "收到 business WebSocket 错误消息: code={}, msg={}",
                        dto.code, dto.msg
                    );
                } else {
                    debug!("收到 business WebSocket 确认消息: {:?}", dto);
                }
            }
        }
        Err(anyhow!("OKX business WebSocket 接收通道已关闭"))
    });

    let mut watchdog_task = tokio::spawn(run_watchdog(
        Arc::clone(&candle_service),
        Arc::clone(&runtime_registry),
        targets,
    ));
    let mut health_task = tokio::spawn(run_health_monitor(
        public_client.clone(),
        business_client.clone(),
        Arc::clone(&runtime_registry),
    ));

    let exit_error = tokio::select! {
        result = &mut ticker_task => supervised_result("public_receiver", result),
        result = &mut candle_task => supervised_result("business_receiver", result),
        result = &mut watchdog_task => supervised_result("candle_watchdog", result),
        result = &mut health_task => supervised_result("websocket_health", result),
        result = &mut persist_task => match result {
            Ok(()) => anyhow!("candle_persist_worker 意外退出"),
            Err(error) => anyhow!("candle_persist_worker 崩溃: {error}"),
        },
    };

    error!(
        event = "websocket_supervised_task_exit",
        error = %exit_error,
        "WebSocket 关键任务退出，停止客户端并让进程重启"
    );
    public_client.stop().await;
    business_client.stop().await;
    ticker_task.abort();
    candle_task.abort();
    watchdog_task.abort();
    health_task.abort();
    persist_task.abort();
    Err(exit_error)
}

/// 登记所有 K 线订阅；真正 ACK 由 SDK 健康状态记录。
async fn subscribe_candles(
    client: &AutoReconnectWebsocketClient,
    inst_ids: &[String],
    times: &[String],
) -> Result<()> {
    for inst_id in inst_ids {
        for time in times {
            let args = Args::new()
                .with_inst_id(inst_id.to_string())
                .with_param("period".to_string(), time.to_string());
            client
                .subscribe(ChannelType::Candle(time.to_string()), args)
                .await
                .with_context(|| format!("登记 K 线订阅失败: {inst_id}:{time}"))?;
        }
    }
    Ok(())
}

/// 登记 ticker 订阅；真正 ACK 由 SDK 健康状态记录。
async fn subscribe_tickers(
    client: &AutoReconnectWebsocketClient,
    inst_ids: &[String],
) -> Result<()> {
    for inst_id in inst_ids {
        client
            .subscribe(
                ChannelType::Tickers,
                Args::new().with_inst_id(inst_id.to_string()),
            )
            .await
            .with_context(|| format!("登记 ticker 订阅失败: {inst_id}"))?;
    }
    Ok(())
}

/// 校验周期并构造 watchdog 目标。
fn build_watchdog_targets(inst_ids: &[String], times: &[String]) -> Result<Vec<WatchdogTarget>> {
    let mut targets = Vec::with_capacity(inst_ids.len().saturating_mul(times.len()));
    for symbol in inst_ids {
        for timeframe in times {
            targets.push(WatchdogTarget {
                symbol: symbol.clone(),
                timeframe: timeframe.clone(),
                timeframe_ms: timeframe_duration_ms(timeframe).map_err(anyhow::Error::msg)?,
                last_observed_candle_ts: None,
                last_db_query_at_ms: None,
            });
        }
    }
    Ok(targets)
}

/// 读取启动前最新确认 K 线作为基线，禁止重启后追溯触发历史信号。
async fn initialize_watchdog_baselines(
    registry: &CandleRuntimeRegistry,
    targets: &mut [WatchdogTarget],
) -> Result<()> {
    let model = CandlesModel::new();
    for target in targets {
        if let Some(candle) = model
            .get_latest_confirmed_data(&target.symbol, &target.timeframe)
            .await
            .with_context(|| {
                format!(
                    "初始化 watchdog DB 基线失败: {}:{}",
                    target.symbol, target.timeframe
                )
            })?
        {
            registry.seed_startup_baseline(&target.symbol, &target.timeframe, candle.ts);
            target.last_observed_candle_ts = Some(candle.ts);
        }
        target.last_db_query_at_ms = Some(Utc::now().timestamp_millis());
    }
    Ok(())
}

/// 在 K 线收盘边界后检查 DB，并执行十秒内幂等补触发或过期审计。
async fn run_watchdog(
    candle_service: Arc<CandleService>,
    runtime_registry: Arc<CandleRuntimeRegistry>,
    mut targets: Vec<WatchdogTarget>,
) -> Result<()> {
    let model = CandlesModel::new();
    let mut interval = tokio::time::interval(WATCHDOG_POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let now_ms = Utc::now().timestamp_millis();
        for target in &mut targets {
            if !target.should_query_db(now_ms) {
                continue;
            }
            target.last_db_query_at_ms = Some(now_ms);
            let Some(candle) = model
                .get_latest_confirmed_data(&target.symbol, &target.timeframe)
                .await
                .with_context(|| {
                    format!(
                        "watchdog 查询确认 K 线失败: {}:{}",
                        target.symbol, target.timeframe
                    )
                })?
            else {
                continue;
            };
            target.last_observed_candle_ts = Some(
                target
                    .last_observed_candle_ts
                    .map_or(candle.ts, |timestamp| timestamp.max(candle.ts)),
            );
            let baseline = runtime_registry
                .snapshot(&target.symbol, &target.timeframe)
                .and_then(|snapshot| snapshot.startup_baseline_candle_ts);
            if baseline.is_some_and(|timestamp| candle.ts <= timestamp) {
                continue;
            }
            runtime_registry.record_confirmed_candle(
                &target.symbol,
                &target.timeframe,
                candle.ts,
                now_ms,
            );
            let last_handled =
                runtime_registry.latest_handled_candle_ts(&target.symbol, &target.timeframe);
            match WatchdogDecision::for_confirmed_candle(
                candle.ts,
                target.timeframe_ms,
                now_ms,
                last_handled,
            ) {
                WatchdogDecision::Trigger => {
                    warn!(
                        event = "confirmed_candle_missing_trigger",
                        symbol = %target.symbol,
                        timeframe = %target.timeframe,
                        candle_ts = candle.ts,
                        age_after_close_ms = now_ms.saturating_sub(candle.ts.saturating_add(target.timeframe_ms)),
                        "DB 已确认 K 线尚未触发，watchdog 在十秒窗口内执行幂等补触发"
                    );
                    candle_service.trigger_confirmed_candle(
                        &target.symbol,
                        &target.timeframe,
                        candle,
                        "db_watchdog",
                    );
                }
                WatchdogDecision::Expired => {
                    if runtime_registry.record_expired(&target.symbol, &target.timeframe, candle.ts)
                    {
                        error!(
                            event = "expired_missed_trigger",
                            symbol = %target.symbol,
                            timeframe = %target.timeframe,
                            candle_ts = candle.ts,
                            age_after_close_ms = now_ms.saturating_sub(candle.ts.saturating_add(target.timeframe_ms)),
                            action = "audit_only_no_execution_task",
                            "确认 K 线漏触发且已超过十秒，禁止补执行和补下单"
                        );
                    }
                }
                WatchdogDecision::AlreadyHandled | WatchdogDecision::NotDue => {}
            }
        }
    }
}

/// 周期输出连接、ACK、重连、业务消息与各订阅目标的健康快照。
async fn run_health_monitor(
    public_client: AutoReconnectWebsocketClient,
    business_client: AutoReconnectWebsocketClient,
    runtime_registry: Arc<CandleRuntimeRegistry>,
) -> Result<()> {
    let mut interval = tokio::time::interval(HEALTH_LOG_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        interval.tick().await;
        let public = public_client.health_snapshot();
        let business = business_client.health_snapshot();
        let now_ms = Utc::now().timestamp_millis();
        let target_snapshots = runtime_registry.snapshots();
        let target_messages_fresh = target_snapshots.iter().all(|target| {
            message_is_fresh(
                target.last_message_at_ms,
                now_ms,
                BUSINESS_MESSAGE_STALE_AFTER_MS,
            )
        });
        let public_business_message_fresh = message_is_fresh(
            public.last_business_message_at_ms,
            now_ms,
            BUSINESS_MESSAGE_STALE_AFTER_MS,
        );
        let business_business_message_fresh = message_is_fresh(
            business.last_business_message_at_ms,
            now_ms,
            BUSINESS_MESSAGE_STALE_AFTER_MS,
        );
        let target_health = serde_json::to_string(&target_snapshots)
            .unwrap_or_else(|error| format!("health_snapshot_serialize_error:{error}"));
        let healthy = public.manager_task_alive
            && business.manager_task_alive
            && public.connection_state == ConnectionState::Connected
            && business.connection_state == ConnectionState::Connected
            && public.all_subscriptions_acknowledged
            && business.all_subscriptions_acknowledged
            && public_business_message_fresh
            && business_business_message_fresh
            && target_messages_fresh;
        let log_health = || {
            info!(
                event = "websocket_runtime_health",
                healthy,
                public_state = ?public.connection_state,
                public_manager_alive = public.manager_task_alive,
                public_ack = public.acknowledged_subscription_count,
                public_subscriptions = public.subscription_count,
                public_reconnects = public.reconnect_count,
                public_last_message_elapsed_ms = public.last_message_elapsed_ms,
                public_last_business_message_at_ms = ?public.last_business_message_at_ms,
                public_business_message_fresh,
                business_state = ?business.connection_state,
                business_manager_alive = business.manager_task_alive,
                business_ack = business.acknowledged_subscription_count,
                business_subscriptions = business.subscription_count,
                business_reconnects = business.reconnect_count,
                business_last_message_elapsed_ms = business.last_message_elapsed_ms,
                business_last_business_message_at_ms = ?business.last_business_message_at_ms,
                business_business_message_fresh,
                target_messages_fresh,
                candle_targets = %target_health,
                "OKX WebSocket 运行态健康检查"
            );
        };
        if healthy {
            log_health();
        } else {
            warn!(
                event = "websocket_runtime_unhealthy",
                public_state = ?public.connection_state,
                public_manager_alive = public.manager_task_alive,
                public_ack = public.acknowledged_subscription_count,
                public_subscriptions = public.subscription_count,
                public_reconnects = public.reconnect_count,
                public_last_message_elapsed_ms = public.last_message_elapsed_ms,
                public_last_business_message_at_ms = ?public.last_business_message_at_ms,
                public_business_message_fresh,
                business_state = ?business.connection_state,
                business_manager_alive = business.manager_task_alive,
                business_ack = business.acknowledged_subscription_count,
                business_subscriptions = business.subscription_count,
                business_reconnects = business.reconnect_count,
                business_last_message_elapsed_ms = business.last_message_elapsed_ms,
                business_last_business_message_at_ms = ?business.last_business_message_at_ms,
                business_business_message_fresh,
                target_messages_fresh,
                candle_targets = %target_health,
                "OKX WebSocket 健康检查失败"
            );
        }
    }
}

/// 判断业务消息是否存在且未超过允许的静默时间。
fn message_is_fresh(last_message_at_ms: Option<i64>, now_ms: i64, max_age_ms: i64) -> bool {
    last_message_at_ms.is_some_and(|timestamp| {
        now_ms >= timestamp && now_ms.saturating_sub(timestamp) <= max_age_ms
    })
}

/// 把被监督任务的退出结果统一转换为进程退出原因。
fn supervised_result(
    task_name: &str,
    result: std::result::Result<Result<()>, JoinError>,
) -> anyhow::Error {
    match result {
        Ok(Ok(())) => anyhow!("{task_name} 意外正常退出"),
        Ok(Err(error)) => anyhow!("{task_name} 退出: {error}"),
        Err(error) => anyhow!("{task_name} 崩溃: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{message_is_fresh, WatchdogTarget, WATCHDOG_FALLBACK_QUERY_MS};

    /// 收盘边界短窗口内必须高频查询，边界外不能持续压 DB。
    #[test]
    fn watchdog_target_queries_at_boundary_and_falls_back_after_thirty_seconds() {
        let timeframe_ms = 14_400_000;
        let next_close_boundary_ms = 1_000 + timeframe_ms * 2;
        let target = WatchdogTarget {
            symbol: "ETH-USDT-SWAP".to_string(),
            timeframe: "4H".to_string(),
            timeframe_ms,
            last_observed_candle_ts: Some(1_000),
            last_db_query_at_ms: Some(next_close_boundary_ms - 9_000),
        };

        assert!(!target.should_query_db(next_close_boundary_ms - 1));
        assert!(target.should_query_db(next_close_boundary_ms));
        assert!(target.should_query_db(next_close_boundary_ms + 12_000));
        assert!(!target.should_query_db(next_close_boundary_ms + 12_001));
        assert!(target.should_query_db(
            target.last_db_query_at_ms.expect("last query") + WATCHDOG_FALLBACK_QUERY_MS
        ));
    }

    /// 健康检查必须把缺失或过期的业务消息判为不健康。
    #[test]
    fn health_requires_fresh_business_message() {
        assert!(!message_is_fresh(None, 20_000, 15_000));
        assert!(message_is_fresh(Some(5_000), 20_000, 15_000));
        assert!(!message_is_fresh(Some(4_999), 20_000, 15_000));
    }
}
