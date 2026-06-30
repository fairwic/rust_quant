//! # 应用启动引导模块
//!  
//! 简化版本 - 只保留核心功能
use crate::app::exchange_symbol_sync::{
    run_exchange_symbol_sync_from_env, ExchangeSymbolSyncRequest,
};
use crate::app::internal_server;
use crate::app::market_velocity_strategy_config::load_market_velocity_signal_config_or_env;
use anyhow::{anyhow, Context, Result};
use rust_quant_core::config::env_is_true;
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use rust_quant_domain::{StrategyConfig, StrategyType};
use rust_quant_infrastructure::external_data::DuneQueryPerformance;
use rust_quant_infrastructure::repositories::fund_monitoring_repository::{
    SqlxFundFlowAlertRepository, SqlxMarketAnomalyRepository,
};
use rust_quant_infrastructure::repositories::{
    PostgresCandleRepository, PostgresStrategyConfigRepository, SqlxSwapOrderRepository,
};
use rust_quant_market::streams;
use rust_quant_orchestration::jobs::data::fund_monitor_job::FundMonitorJob;
use rust_quant_orchestration::jobs::maintenance::{
    MaintenanceScheduler, MarketRankSnapshotPruneJob,
};
use rust_quant_orchestration::workflow::{
    backtest_runner, data_sync, external_market_sync_job::ExternalMarketSyncJob, funding_rate_job,
    tickets_job,
};
use rust_quant_services::market::get_confirmed_candles_for_backtest;
use rust_quant_services::market::{market_velocity_signal_dispatch_is_enabled, CandleService};
use rust_quant_services::rust_quan_web::{
    run_market_velocity_live_readiness_from_env, run_protective_order_outcome_check_from_env,
    run_reconciliation_snapshot_check_from_env, ExecutionWorker,
};
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tracing::{error, info, warn};
include!("bootstrap_dune.rs");
include!("bootstrap_targets.rs");
include!("bootstrap_strategy_config.rs");
/// 运行基于环境变量控制的各个模式
/// 封装当前函数，减少配置运行时调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
pub async fn run_modes() -> Result<()> {
    if env_is_true("IS_RUN_INTERNAL_SERVER", false) {
        return internal_server::run_internal_server().await;
    }
    if env_is_true("IS_RUN_PROTECTIVE_OUTCOME_CHECK", false) {
        let result = run_protective_order_outcome_check_from_env().await?;
        info!("🛡️ 保护单 outcome 实盘验收完成: {}", result);
        return Ok(());
    }
    if env_is_true("IS_RUN_RECONCILIATION_SNAPSHOT_CHECK", false) {
        let result = run_reconciliation_snapshot_check_from_env().await?;
        info!(
            "🔎 signed read-only reconciliation snapshot 完成: {}",
            result
        );
        return Ok(());
    }
    if env_is_true("IS_RUN_MARKET_VELOCITY_LIVE_READINESS", false) {
        let result = run_market_velocity_live_readiness_from_env().await?;
        info!("🧭 Market Velocity live readiness 完成: {}", result);
        return Ok(());
    }
    let env = match std::env::var("APP_ENV") {
        Ok(v) if !v.is_empty() => v,
        _ => "local".to_string(),
    };
    let mut backtest_targets = default_backtest_targets();
    if env == "prod" {
        backtest_targets = load_backtest_targets_from_db()
            .await
            .map_err(|e| anyhow!("加载回测配置失败: {}", e))?;
    }
    // 可选：仅回测指定交易对（不影响数据同步 inst_ids 的覆盖逻辑）
    // 例：BACKTEST_ONLY_INST_IDS="ETH-USDT-SWAP"
    if let Ok(v) = std::env::var("BACKTEST_ONLY_INST_IDS") {
        let selected: BTreeSet<String> = v
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if !selected.is_empty() {
            backtest_targets.retain(|(inst, _)| selected.contains(inst));
        }
    }
    let inst_ids = dedup_strings(
        backtest_targets
            .iter()
            .map(|(inst, _)| inst.clone())
            .collect(),
    );
    // 可选：额外同步的交易对（只影响 IS_RUN_SYNC_DATA_JOB 的数据同步，不影响回测 targets）
    // 用于 BTC 作为大盘参考等场景：SYNC_EXTRA_INST_IDS="BTC-USDT-SWAP,SOL-USDT-SWAP"
    let inst_ids = {
        let mut merged = inst_ids.clone();
        if let Ok(v) = std::env::var("SYNC_EXTRA_INST_IDS") {
            let extra: Vec<String> = v
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            merged.extend(extra);
        }
        dedup_strings(merged)
    };
    // 可选：仅同步指定交易对（覆盖 inst_ids）
    // 例：SYNC_ONLY_INST_IDS="BTC-USDT-SWAP"
    let inst_ids = match std::env::var("SYNC_ONLY_INST_IDS") {
        Ok(v) if !v.trim().is_empty() => dedup_strings(
            v.split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
        ),
        _ => inst_ids,
    };
    let periods = dedup_strings(
        backtest_targets
            .iter()
            .map(|(_, period)| period.clone())
            .collect(),
    );
    let periods = if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        override_periods_from_csv(periods, std::env::var("SYNC_ONLY_PERIODS").ok().as_deref())
    } else {
        periods
    };
    info!(" 监控交易对: {:?}", inst_ids);
    info!("🕒 监控周期: {:?}", periods);
    info!("🎯 回测目标: {:?}", backtest_targets);
    // 0) rust_quan_web 执行任务 worker
    if env_is_true("IS_RUN_EXECUTION_WORKER", false) {
        if env_is_true("EXECUTION_WORKER_ONLY", true) {
            // 独占模式：阻塞运行（run_once 或持续轮询），完成后退出
            run_execution_worker_from_env().await?;
            return Ok(());
        }
        // 并行模式（EXECUTION_WORKER_ONLY=false）：后台持续轮询，与 WebSocket 并行运行
        tokio::spawn(async {
            if let Err(error) = run_execution_worker_loop().await {
                error!("❌ execution worker 后台轮询退出: {}", error);
            }
        });
    }
    // 0.5) 交易对事实表同步 worker。默认每 60 秒同步一次五个交易所。
    if env_is_true("IS_RUN_EXCHANGE_SYMBOL_SYNC_WORKER", false) {
        if env_is_true("EXCHANGE_SYMBOL_SYNC_WORKER_ONLY", true) {
            run_exchange_symbol_sync_worker_from_env().await?;
            return Ok(());
        }
        tokio::spawn(async {
            if let Err(error) = run_exchange_symbol_sync_worker_from_env().await {
                error!("❌ 交易对事实表同步 worker 退出: {}", error);
            }
        });
    }
    // 0.6) 市场动能雷达：复用全市场排名扫描，产出可被 Web/Admin 消费的市场事实。
    let envs: HashMap<String, String> = std::env::vars().collect();
    if should_run_market_velocity_radar_from_map(&envs) {
        if env_is_true("MARKET_VELOCITY_RADAR_ONLY", true) {
            run_market_velocity_radar_worker_from_env().await?;
            return Ok(());
        }
        tokio::spawn(async {
            if let Err(error) = run_market_velocity_radar_worker_from_env().await {
                error!("❌ 市场动能雷达 worker 退出: {}", error);
            }
        });
    }
    // 1) 数据同步任务（Ticker & Funding Rate）
    if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        info!("📡 启动数据同步任务");
        // 快速同步场景可跳过 tickers（例如只想补齐 BTC 4H）
        if !env_is_true("SYNC_SKIP_TICKERS", false) {
            if let Err(error) = tickets_job::sync_tickers(&inst_ids).await {
                error!("❌ Ticker同步失败: {}", error);
            }
        }
        let envs: HashMap<String, String> = std::env::vars().collect();
        if should_skip_market_data_sync_from_map(&envs) {
            info!("⏭️ 跳过市场数据同步（SYNC_SKIP_MARKET_DATA=true）");
        } else if let Err(error) = data_sync::sync_market_data(&inst_ids, &periods).await {
            error!("❌ K线数据同步失败: {}", error);
        }
        if should_run_funding_rate_sync_from_map(&envs) {
            if let Err(error) =
                funding_rate_job::FundingRateJob::sync_funding_rates(&inst_ids).await
            {
                error!("❌ 资金费率历史同步失败: {}", error);
            }
        }
        if let Err(error) = run_dune_sync_jobs_from_env().await {
            error!("❌ Dune外部市场数据同步失败: {}", error);
        }
        // // 新增：同步资金费率历史
        // // 执行资金费率同步任务
        // use rust_quant_orchestration::workflow::funding_rate_job;
        // if let Err(e) = funding_rate_job::FundingRateJob::sync_funding_rates(&inst_ids).await {
        //         tracing::error!("资金费率历史同步失败: {}", e);
        // }
        // // 新增：同步经济日历数据
        // if let Err(e) = economic_calendar_job::EconomicCalendarJob::sync_economic_calendar().await {
        //     tracing::error!("❌ 经济日历同步失败: {}", e);
        // }
    }
    // 2) 回测任务
    if env_is_true("IS_BACK_TEST", false) {
        info!("📈 回测模式已启用");
        if let Err(error) = backtest_runner::run_backtest_runner(&backtest_targets).await {
            error!("❌ 回测执行失败: {}", error);
        }
        // 回测专用流程：若未开启实时策略/Socket，则直接返回，避免主程序挂起等待信号
        let open_socket = env_is_true("IS_OPEN_SOCKET", false);
        let run_real = env_is_true("IS_RUN_REAL_STRATEGY", false);
        if !open_socket && !run_real {
            return Ok(());
        }
    }
    // 3) 实盘策略（包含预热）
    let mut live_runtime_configs: Vec<StrategyConfig> = Vec::new();
    let mut live_runtime_services: Option<(
        Arc<StrategyConfigService>,
        Arc<StrategyExecutionService>,
    )> = None;
    if env_is_true("IS_RUN_REAL_STRATEGY", false) {
        info!("🤖 实盘策略模式已启用");
        let config_service = Arc::new(create_strategy_config_service()?);
        let swap_order_repo = Arc::new(SqlxSwapOrderRepository::new(get_db_pool().clone()));
        let execution_service = Arc::new(StrategyExecutionService::new(swap_order_repo));
        match start_strategies_from_db(config_service.clone(), execution_service.clone()).await {
            Ok(started_configs) => {
                live_runtime_configs = started_configs;
                live_runtime_services = Some((config_service, execution_service));
            }
            Err(e) => {
                error!("❌ 启动策略失败: {}", e);
            }
        }
    }
    // 4) WebSocket 实时数据（长期运行：必须后台启动，避免阻塞 run() 后续心跳/信号处理）
    if env_is_true("IS_OPEN_SOCKET", false) {
        let (ws_inst_ids, ws_periods) = if !live_runtime_configs.is_empty() {
            derive_ws_targets_from_configs(&live_runtime_configs)
        } else {
            (inst_ids.clone(), periods.clone())
        };
        let market_exchange = derive_market_data_exchange_from_configs(
            &live_runtime_configs,
            Some(&market_data_exchange()),
        )
        .unwrap_or_else(|| market_data_exchange());
        info!("🌐 WebSocket模式已启用");
        info!(
            "📡 启动WebSocket监听: exchange={}, targets={:?}",
            market_exchange, ws_inst_ids
        );
        let (config_service, execution_service) = match live_runtime_services {
            Some((config_service, execution_service)) => (config_service, execution_service),
            None => {
                let config_service = Arc::new(create_strategy_config_service()?);
                let swap_order_repo = Arc::new(SqlxSwapOrderRepository::new(get_db_pool().clone()));
                let execution_service = Arc::new(StrategyExecutionService::new(swap_order_repo));
                (config_service, execution_service)
            }
        };
        // 注意：WebSocket 客户端内部包含 !Send 的锁卫（okx crate），不能用 tokio::spawn
        // 长期运行逻辑由 run() 通过 select! 方式与信号处理并行编排
        run_websocket(
            &ws_inst_ids,
            &ws_periods,
            &market_exchange,
            config_service,
            execution_service,
        )
        .await;
    }
    Ok(())
}
/// 执行 配置、基础设施和运行时 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn run_execution_worker_from_env() -> Result<()> {
    let worker = ExecutionWorker::from_env()?;
    worker.verify_live_audit_ready().await?;
    let run_once = env_is_true("EXECUTION_WORKER_RUN_ONCE", true);
    let envs: HashMap<String, String> = std::env::vars().collect();
    let poll_interval_secs = execution_worker_poll_interval_secs_from_map(&envs);
    if run_once {
        let handled = worker.run_once().await?;
        info!("🧾 执行任务 worker 单轮完成: handled={}", handled);
        return Ok(());
    }
    info!(
        "🧾 执行任务 worker 轮询启动: interval={}s",
        poll_interval_secs
    );
    loop {
        match worker.run_once().await {
            Ok(handled) => info!("🧾 执行任务 worker 轮询完成: handled={}", handled),
            Err(error) => error!("❌ 执行任务 worker 轮询失败: {}", error),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;
    }
}
/// 后台持续轮询模式，供 `tokio::spawn` 使用（与 WebSocket 并行运行时）。
/// 忽略 `EXECUTION_WORKER_RUN_ONCE`，始终持续轮询直到进程退出。
async fn run_execution_worker_loop() -> Result<()> {
    let worker = ExecutionWorker::from_env()?;
    worker.verify_live_audit_ready().await?;
    let envs: HashMap<String, String> = std::env::vars().collect();
    let poll_interval_secs = execution_worker_poll_interval_secs_from_map(&envs);
    info!(
        "🧾 execution worker 后台轮询启动: interval={}s, mode=live",
        poll_interval_secs,
    );
    loop {
        match worker.run_once().await {
            Ok(handled) if handled > 0 => {
                info!("🧾 execution worker 处理完成: handled={}", handled)
            }
            Ok(_) => {}
            Err(error) => error!("❌ execution worker 轮询失败: {}", error),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;
    }
}
/// 提供执行workerpollintervalsecsfrommap的集中实现，避免配置运行时调用方重复处理相同细节。
fn execution_worker_poll_interval_secs_from_map(envs: &HashMap<String, String>) -> u64 {
    match envs
        .get("EXECUTION_WORKER_POLL_INTERVAL_SECS")
        .map(|value| value.trim().parse::<u64>())
    {
        Some(Ok(0)) => 1,
        Some(Ok(value)) => value,
        Some(Err(_)) | None => 5,
    }
}
/// 执行 配置、基础设施和运行时 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn run_exchange_symbol_sync_worker_from_env() -> Result<()> {
    let interval_secs = std::env::var("EXCHANGE_SYMBOL_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(60);
    let run_once = env_is_true("EXCHANGE_SYMBOL_SYNC_RUN_ONCE", false);
    if run_once {
        let response = run_exchange_symbol_sync_from_env(ExchangeSymbolSyncRequest {
            sources: None,
            trigger_source: Some("scheduled".to_string()),
            submit_signals: None,
        })
        .await?;
        info!(
            "🔁 交易对事实表同步 worker 单轮完成: run_id={}, sources={:?}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
            response.run_id,
            response.requested_sources,
            response.persisted_rows,
            response.first_seen_rows,
            response.major_listing_signals
        );
        return Ok(());
    }
    info!(
        "🔁 交易对事实表同步 worker 启动: interval={}s",
        interval_secs
    );
    loop {
        match run_exchange_symbol_sync_from_env(ExchangeSymbolSyncRequest {
            sources: None,
            trigger_source: Some("scheduled".to_string()),
            submit_signals: None,
        })
        .await
        {
            Ok(response) => info!(
                "🔁 交易对事实表同步 worker 完成: run_id={}, sources={:?}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
                response.run_id,
                response.requested_sources,
                response.persisted_rows,
                response.first_seen_rows,
                response.major_listing_signals
            ),
            Err(error) => error!("❌ 交易对事实表同步 worker 失败: {}", error),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
    }
}
/// 执行 配置、基础设施和运行时 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn run_market_velocity_radar_worker_from_env() -> Result<()> {
    let interval_secs = std::env::var("MARKET_VELOCITY_SCAN_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10);
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("缺少 QUANT_CORE_DATABASE_URL，无法启动市场动能雷达")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("连接 quant_core 数据库失败，无法启动市场动能雷达")?;
    let anomaly_repo: Arc<dyn MarketAnomalyRepository> =
        Arc::new(SqlxMarketAnomalyRepository::new(pool.clone()));
    let maintenance_anomaly_repo = Arc::clone(&anomaly_repo);
    let candle_service = Arc::new(CandleService::new(Box::new(PostgresCandleRepository::new(
        pool.clone(),
    ))));
    let market_velocity_signal_config = if market_velocity_signal_dispatch_is_enabled() {
        Some(load_market_velocity_signal_config_or_env(&pool).await?)
    } else {
        None
    };
    let alert_repo = Arc::new(SqlxFundFlowAlertRepository::new(pool));
    let (mut job, analyzer) =
        FundMonitorJob::new_with_candle_service_and_market_velocity_signal_config(
            interval_secs,
            anomaly_repo,
            alert_repo,
            Some(candle_service),
            market_velocity_signal_config,
        )?;
    start_core_maintenance_scheduler(maintenance_anomaly_repo);
    tokio::spawn(async move {
        analyzer.run().await;
    });
    let dispatch_state = if market_velocity_signal_dispatch_is_enabled() {
        "web-dispatch"
    } else {
        "core-only"
    };
    info!(
        "📈 市场动能雷达 worker 启动: interval={}s, strategy_signal_dispatch={}",
        interval_secs, dispatch_state
    );
    if dispatch_state == "core-only" {
        warn!(
            "市场动能雷达当前仅写入 Core 事件；如需生成 Web 24h 信号，需配置 MARKET_VELOCITY_SIGNAL_DISPATCH_MODE=web 和 RUST_QUAN_WEB_BASE_URL/EXECUTION_EVENT_SECRET"
        );
    }
    job.run_loop().await;
    Ok(())
}
fn start_core_maintenance_scheduler(anomaly_repo: Arc<dyn MarketAnomalyRepository>) {
    let mut scheduler = MaintenanceScheduler::new(tokio::time::Duration::from_secs(60));
    scheduler.register_job(MarketRankSnapshotPruneJob::new("okx", anomaly_repo));
    tokio::spawn(async move {
        scheduler.run_forever().await;
    });
}
/// WebSocket数据监听
/// 启动WebSocket连接，监听实时行情和K线数据
/// # 架构说明
/// - 创建策略触发回调函数
/// - 注入到 CandleService 中
/// - K线确认时自动触发策略执行
/// # 注意
/// WebSocket 模式下策略预热由 `start_strategies_from_db` 统一处理
/// 确保 `IS_RUN_REAL_STRATEGY=true` 时先完成预热再启动 WebSocket
async fn run_websocket(
    inst_ids: &[String],
    periods: &[String],
    market_exchange: &str,
    config_service: Arc<StrategyConfigService>,
    execution_service: Arc<StrategyExecutionService>,
) {
    if inst_ids.is_empty() || periods.is_empty() {
        warn!(
            "⚠️  WebSocket启动参数为空，跳过启动: inst_ids={:?}, periods={:?}",
            inst_ids, periods
        );
        return;
    }
    info!(
        "🌐 启动WebSocket数据流: inst_ids={:?}, periods={:?}",
        inst_ids, periods
    );
    // 🚀 创建策略触发回调函数
    let strategy_trigger = {
        let handler = Arc::new(
            rust_quant_orchestration::workflow::websocket_handler::WebsocketStrategyHandler::new(
                config_service,
                execution_service,
            ),
        );
        Arc::new(
            move |inst_id: String,
                  time_interval: String,
                  snap: rust_quant_market::models::CandlesEntity| {
                let handler = handler.clone();
                tokio::spawn(async move {
                    handler.handle(inst_id, time_interval, snap).await;
                });
            },
        )
    };
    let inst_ids_vec: Vec<String> = inst_ids.to_vec();
    let periods_vec: Vec<String> = periods.to_vec();
    // 使用带策略触发的 WebSocket 服务
    if market_exchange == "binance" {
        if let Err(error) = rust_quant_services::market::binance_websocket::run_binance_websocket_with_strategy_trigger(
            &inst_ids_vec,
            &periods_vec,
            Some(strategy_trigger),
        )
        .await
        {
            error!("❌ Binance WebSocket启动失败: {}", error);
        }
    } else {
        streams::run_socket_with_strategy_trigger(
            &inst_ids_vec,
            &periods_vec,
            Some(strategy_trigger),
        )
        .await;
    }
}
/// 提供CSV过滤值的集中实现，避免配置运行时调用方重复处理相同细节。
fn csv_filter_values(raw: Option<String>) -> BTreeSet<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
/// 解析大小写不敏感的过滤值，避免交易所名称大小写导致生产策略误加载。
fn csv_lower_filter_values(raw: Option<String>) -> BTreeSet<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}
/// 解析过滤live策略配置，把外部输入转换成配置运行时可用的内部值。
fn filter_live_strategy_configs(configs: Vec<StrategyConfig>) -> Vec<StrategyConfig> {
    let inst_ids = csv_filter_values(std::env::var("LIVE_STRATEGY_ONLY_INST_IDS").ok());
    let periods = csv_filter_values(std::env::var("LIVE_STRATEGY_ONLY_PERIODS").ok());
    let exchanges = csv_lower_filter_values(std::env::var("LIVE_STRATEGY_ONLY_EXCHANGES").ok());
    let fallback_exchange = market_data_exchange();
    filter_live_strategy_configs_with_filters(
        configs,
        &inst_ids,
        &periods,
        &exchanges,
        &fallback_exchange,
    )
}

/// 应用 live 策略过滤规则，确保单一 WebSocket worker 不混用多个交易所行情源。
fn filter_live_strategy_configs_with_filters(
    configs: Vec<StrategyConfig>,
    inst_ids: &BTreeSet<String>,
    periods: &BTreeSet<String>,
    exchanges: &BTreeSet<String>,
    fallback_exchange: &str,
) -> Vec<StrategyConfig> {
    let before_event_driven_filter = configs.len();
    let configs: Vec<StrategyConfig> = configs
        .into_iter()
        .filter(|config| config.strategy_type != StrategyType::MarketVelocity)
        .collect();
    if configs.len() != before_event_driven_filter {
        info!(
            "🎯 实时策略启动跳过事件驱动策略配置: before={}, after={}",
            before_event_driven_filter,
            configs.len()
        );
    }
    if inst_ids.is_empty() && periods.is_empty() && exchanges.is_empty() {
        return configs;
    }
    let before = configs.len();
    let filtered: Vec<StrategyConfig> = configs
        .into_iter()
        .filter(|config| {
            let config_exchange = config
                .exchange
                .as_deref()
                .map(str::trim)
                .filter(|exchange| !exchange.is_empty() && !exchange.eq_ignore_ascii_case("all"))
                .map(|exchange| exchange.to_ascii_lowercase())
                .unwrap_or_else(|| fallback_exchange.trim().to_ascii_lowercase());
            (inst_ids.is_empty() || inst_ids.contains(&config.symbol))
                && (periods.is_empty() || periods.contains(config.timeframe.as_str()))
                && (exchanges.is_empty() || exchanges.contains(&config_exchange))
        })
        .collect();
    info!(
        "🎯 实时策略过滤后剩余: before={}, after={}, inst_ids={:?}, periods={:?}, exchanges={:?}",
        before,
        filtered.len(),
        inst_ids,
        periods,
        exchanges
    );
    filtered
}
/// 从数据库加载策略配置并启动
/// 通过services层加载配置，使用orchestration层启动策略
/// # 启动流程
/// 1. 加载启用的策略配置
/// 2. **预热策略数据**（加载历史K线到指标缓存）
/// 3. 启动策略定时任务
async fn start_strategies_from_db(
    config_service: Arc<StrategyConfigService>,
    execution_service: Arc<StrategyExecutionService>,
) -> Result<Vec<StrategyConfig>> {
    use rust_quant_domain::StrategyType;
    use rust_quant_domain::Timeframe;
    use rust_quant_market::models::CandlesEntity;
    use rust_quant_orchestration::workflow::strategy_runner;
    use rust_quant_services::strategy::StrategyDataService;
    info!("📚 从数据库加载策略配置");
    // 1. 通过服务层加载启用的策略配置
    let configs = config_service.load_all_enabled_configs().await?;
    let configs = filter_live_strategy_configs(configs);
    if configs.is_empty() {
        warn!("⚠️  未找到启用的策略配置");
        return Ok(Vec::new());
    }
    info!("✅ 加载了 {} 个策略配置", configs.len());
    for config in &configs {
        if let Err(e) = execution_service
            .compensate_close_algos_on_start(config)
            .await
        {
            warn!(
                "⚠️ 启动补偿撤单失败: id={}, symbol={}, err={}",
                config.id, config.symbol, e
            );
        }
    }
    // 2. 预热策略数据（关键步骤！）
    info!("🔥 开始预热策略数据...");
    let warmup_results = StrategyDataService::initialize_multiple_strategies(&configs).await;
    let warmup_success_count = warmup_results.iter().filter(|r| r.is_ok()).count();
    let warmup_fail_count = warmup_results.len() - warmup_success_count;
    if warmup_fail_count > 0 {
        warn!(
            "⚠️  预热部分失败: 成功 {}, 失败 {}",
            warmup_success_count, warmup_fail_count
        );
    } else {
        info!("✅ 预热完成: 成功 {} 个策略", warmup_success_count);
    }
    // 3. 启动每个策略
    let mut started_configs: Vec<StrategyConfig> = Vec::new();
    for (idx, config) in configs.iter().enumerate() {
        // 检查预热是否成功
        let warmup_failed = match warmup_results.get(idx) {
            Some(r) => r.is_err(),
            None => true,
        };
        if warmup_failed {
            warn!(
                "⚠️  策略预热失败，跳过启动: id={}, symbol={}",
                config.id, config.symbol
            );
            continue;
        }
        if let Err(e) = config_service.validate_config(config) {
            warn!("⚠️  策略配置校验失败，跳过: id={}, error={}", config.id, e);
            continue;
        }
        let inst_id = config.symbol.clone();
        let timeframe: Timeframe = config.timeframe;
        let strategy_type: StrategyType = config.strategy_type;
        let config_id = config.id;
        info!(
            "🚀 启动策略: {} - {} - {:?}",
            inst_id,
            timeframe.as_str(),
            strategy_type
        );
        // 4. 启动策略执行：
        // - WebSocket 模式：启动阶段没有 snap，不执行一次性分析，等待 WebSocket 的确认K线触发
        // - 非 WebSocket 模式：尝试从DB取最新确认K线作为 snap 触发一次执行（避免 “需要提供K线快照”）
        if env_is_true("IS_OPEN_SOCKET", false) {
            info!(
                "✅ 策略已预热并进入等待：{} - {} - {:?}（等待WebSocket确认K线触发）",
                inst_id,
                timeframe.as_str(),
                strategy_type
            );
            started_configs.push(config.clone());
            continue;
        }
        let snap: Option<CandlesEntity> = {
            let mut candles =
                get_confirmed_candles_for_backtest(&inst_id, timeframe.as_str(), 1, None)
                    .await
                    .map_err(|e| anyhow!("加载最新确认K线失败: {}", e))?;
            candles.sort_unstable_by_key(|a| a.ts);
            candles.pop()
        };
        if snap.is_none() {
            warn!(
                "⚠️  未找到最新确认K线，跳过首次执行: {} - {} - {:?}",
                inst_id,
                timeframe.as_str(),
                strategy_type
            );
            continue;
        }
        if let Err(e) = strategy_runner::execute_strategy(
            &inst_id,
            timeframe,
            strategy_type,
            Some(config_id),
            None,
            snap,
            config_service.as_ref(),
            execution_service.as_ref(),
        )
        .await
        {
            error!(
                "❌ 启动策略失败: {} - {} - {:?}: {}",
                inst_id,
                timeframe.as_str(),
                strategy_type,
                e
            );
        } else {
            info!(
                "✅ 策略启动成功: {} - {} - {:?}",
                inst_id,
                timeframe.as_str(),
                strategy_type
            );
            started_configs.push(config.clone());
        }
    }
    info!("✅ 策略启动完成");
    Ok(started_configs)
}
/// 应用入口总编排
pub async fn run() -> Result<()> {
    // 初始化并启动调度器
    let _scheduler = match crate::init_scheduler().await {
        Ok(s) => {
            info!("✅ 任务调度器初始化成功");
            s
        }
        Err(e) => {
            error!("❌ 初始化任务调度器失败: {}", e);
            return Err(anyhow!("初始化任务调度器失败: {}", e));
        }
    };
    // 非本地环境校验系统时间
    let app_env = match std::env::var("APP_ENV") {
        Ok(v) if !v.is_empty() => v,
        _ => "local".to_string(),
    };
    info!("🕐 应用环境: {}", app_env);
    if app_env != "local" {
        info!("校验系统时间与 OKX 时间差");
        if let Err(e) = okx::utils::validate_system_time().await {
            error!("⚠️  系统时间校验失败: {}", e);
        }
    }
    // 启动心跳任务
    let heartbeat_handle = tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            info!("💓 程序正在运行中...");
        }
    });
    // 运行模式编排：
    // - 若开启 WebSocket：run_modes() 会长期阻塞，因此必须与信号处理并行 select!
    // - 若未开启 WebSocket：先跑完 run_modes()，再进入信号等待
    let open_socket = env_is_true("IS_OPEN_SOCKET", false);
    let internal_server = env_is_true("IS_RUN_INTERNAL_SERVER", false);
    if open_socket || internal_server {
        // 注意：run_modes() 在 WebSocket 场景可能“阻塞”也可能“快速返回”（内部可能 spawn 任务后返回）。
        // 目标：无论 run_modes 是否返回，都必须持续等待退出信号。
        let mut run_modes_fut = Box::pin(run_modes());
        let mut signal_fut = Box::pin(setup_shutdown_signals());
        let mut signal_name_opt: Option<&'static str> = None;
        tokio::select! {
            res = &mut run_modes_fut => {
                if let Err(e) = res {
                    error!("❌ 运行模式执行失败: {}", e);
                }
            }
            signal_name = &mut signal_fut => {
                signal_name_opt = Some(signal_name);
            }
        }
        let signal_name = match signal_name_opt {
            Some(name) => name,
            None => signal_fut.await,
        };
        info!("📡 接收到 {} 信号", signal_name);
    } else {
        run_modes().await?;
        let envs: HashMap<String, String> = std::env::vars().collect();
        if should_exit_after_market_velocity_live_readiness_from_map(&envs) {
            heartbeat_handle.abort();
            info!("🧭 Market Velocity live readiness 已完成，直接优雅退出");
            let shutdown_config = crate::GracefulShutdownConfig {
                total_timeout_secs: 30,
                strategy_stop_timeout_secs: 20,
                scheduler_shutdown_timeout_secs: 5,
                db_cleanup_timeout_secs: 5,
            };
            if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
                error!("❌ 优雅关闭失败: {}", e);
                std::process::exit(1);
            }
            return Ok(());
        }
        // 数据同步-only 场景可直接退出（避免本地一键 sync 后进程挂起等待信号）
        // - local 环境默认退出；prod 如需退出可设置 EXIT_AFTER_SYNC=1
        let sync_only = env_is_true("IS_RUN_SYNC_DATA_JOB", false)
            && !env_is_true("IS_BACK_TEST", false)
            && !env_is_true("IS_OPEN_SOCKET", false)
            && !env_is_true("IS_RUN_REAL_STRATEGY", false);
        if sync_only {
            let exit_after_sync = env_is_true("EXIT_AFTER_SYNC", app_env == "local");
            if exit_after_sync {
                heartbeat_handle.abort();
                info!("📡 数据同步已完成，未启用实时/Socket/回测，直接优雅退出");
                let shutdown_config = crate::GracefulShutdownConfig {
                    total_timeout_secs: 30,
                    strategy_stop_timeout_secs: 20,
                    scheduler_shutdown_timeout_secs: 5,
                    db_cleanup_timeout_secs: 5,
                };
                if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
                    error!("❌ 优雅关闭失败: {}", e);
                    std::process::exit(1);
                }
                return Ok(());
            }
        }
        // 回测-only 场景直接退出（不等待信号），避免进程挂起
        let backtest_only = env_is_true("IS_BACK_TEST", false)
            && !env_is_true("IS_OPEN_SOCKET", false)
            && !env_is_true("IS_RUN_REAL_STRATEGY", false);
        if backtest_only {
            heartbeat_handle.abort();
            info!("📈 回测模式已完成，未启用实时/Socket，直接优雅退出");
            let shutdown_config = crate::GracefulShutdownConfig {
                total_timeout_secs: 30,
                strategy_stop_timeout_secs: 20,
                scheduler_shutdown_timeout_secs: 5,
                db_cleanup_timeout_secs: 5,
            };
            if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
                error!("❌ 优雅关闭失败: {}", e);
                std::process::exit(1);
            }
            return Ok(());
        }
        let live_strategy_one_shot = env_is_true("IS_RUN_REAL_STRATEGY", false)
            && !env_is_true("IS_OPEN_SOCKET", false)
            && env_is_true("EXIT_AFTER_REAL_STRATEGY_ONESHOT", false);
        if live_strategy_one_shot {
            heartbeat_handle.abort();
            info!("🤖 实时策略 one-shot 已完成，未启用 WebSocket，直接优雅退出");
            let shutdown_config = crate::GracefulShutdownConfig {
                total_timeout_secs: 30,
                strategy_stop_timeout_secs: 20,
                scheduler_shutdown_timeout_secs: 5,
                db_cleanup_timeout_secs: 5,
            };
            if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
                error!("❌ 优雅关闭失败: {}", e);
                std::process::exit(1);
            }
            return Ok(());
        }
        let execution_worker_once_only = env_is_true("IS_RUN_EXECUTION_WORKER", false)
            && env_is_true("EXECUTION_WORKER_ONLY", true)
            && env_is_true("EXECUTION_WORKER_RUN_ONCE", true)
            && !env_is_true("IS_RUN_SYNC_DATA_JOB", false)
            && !env_is_true("IS_BACK_TEST", false)
            && !env_is_true("IS_OPEN_SOCKET", false)
            && !env_is_true("IS_RUN_REAL_STRATEGY", false);
        if execution_worker_once_only {
            heartbeat_handle.abort();
            info!("🧾 执行任务 worker 单轮完成，未启用其他长期模式，直接优雅退出");
            let shutdown_config = crate::GracefulShutdownConfig {
                total_timeout_secs: 30,
                strategy_stop_timeout_secs: 20,
                scheduler_shutdown_timeout_secs: 5,
                db_cleanup_timeout_secs: 5,
            };
            if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
                error!("❌ 优雅关闭失败: {}", e);
                std::process::exit(1);
            }
            return Ok(());
        }
        // 信号处理
        let signal_name = setup_shutdown_signals().await;
        info!("📡 接收到 {} 信号", signal_name);
    }
    // 停止心跳
    heartbeat_handle.abort();
    // 优雅关闭
    info!("🛑 开始优雅关闭...");
    let shutdown_config = crate::GracefulShutdownConfig {
        total_timeout_secs: 30,
        strategy_stop_timeout_secs: 20,
        scheduler_shutdown_timeout_secs: 5,
        db_cleanup_timeout_secs: 5,
    };
    if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
        error!("❌ 优雅关闭失败: {}", e);
        std::process::exit(1);
    }
    info!("✅ 应用已优雅退出");
    Ok(())
}
include!("bootstrap_shutdown.rs");
#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
