//! # 应用启动引导模块
//!  
//! 简化版本 - 只保留核心功能

use anyhow::{anyhow, Result};
use rust_quant_core::config::env_is_true;
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::StrategyType;
use rust_quant_infrastructure::repositories::SqlxStrategyConfigRepository;
use tracing::{error, info, warn};

use rust_quant_market::streams;
use rust_quant_orchestration::workflow::{backtest_runner, data_sync, tickets_job};
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};
use std::collections::BTreeSet;

/// 运行基于环境变量控制的各个模式
pub async fn run_modes() -> Result<()> {
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "local".to_string());

    let mut backtest_targets = default_backtest_targets();

    if env == "prod" {
        backtest_targets = load_backtest_targets_from_db()
            .await
            .map_err(|e| anyhow!("加载回测配置失败: {}", e))?;
    }

    let inst_ids = dedup_strings(
        backtest_targets
            .iter()
            .map(|(inst, _)| inst.clone())
            .collect(),
    );
    let periods = dedup_strings(
        backtest_targets
            .iter()
            .map(|(_, period)| period.clone())
            .collect(),
    );

    info!(" 监控交易对: {:?}", inst_ids);
    info!("🕒 监控周期: {:?}", periods);
    info!("🎯 回测目标: {:?}", backtest_targets);

    // 1) 数据同步任务（Ticker）
    if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        info!("📡 启动数据同步任务");
        if let Err(error) = tickets_job::sync_tickers(&inst_ids).await {
            error!("❌ Ticker同步失败: {}", error);
        }
        if let Err(error) = data_sync::sync_market_data(&inst_ids, &periods).await {
            error!("❌ K线数据同步失败: {}", error);
        }
    }

    // 2) 回测任务
    if env_is_true("IS_BACK_TEST", false) {
        info!("📈 回测模式已启用");
        if let Err(error) = backtest_runner::run_backtest_runner(&backtest_targets).await {
            error!("❌ 回测执行失败: {}", error);
        }
    }

    // 3) WebSocket 实时数据
    if env_is_true("IS_OPEN_SOCKET", false) {
        info!("🌐 WebSocket模式已启用");
        info!("📡 启动WebSocket监听: {:?}", inst_ids);

        // 调用WebSocket服务
        // 注意：这是一个长期运行的任务，会阻塞当前执行流
        run_websocket(&inst_ids, &periods).await;
    }

    // 4) 实盘策略
    if env_is_true("IS_RUN_REAL_STRATEGY", false) {
        info!("🤖 实盘策略模式已启用");
        // 从数据库加载策略配置并启动
        if let Err(e) = start_strategies_from_db().await {
            error!("❌ 启动策略失败: {}", e);
        }
    }

    Ok(())
}

fn default_backtest_targets() -> Vec<(String, String)> {
    vec![
        ("ETH-USDT-SWAP".to_string(), "5m".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "1H".to_string()),
        ("ETH-USDT-SWAP".to_string(), "4H".to_string()),
        ("ETH-USDT-SWAP".to_string(), "1Dutc".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "5m".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "1H".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "4H".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "1Dutc".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "5m".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "1H".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "4H".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "1Dutc".to_string()),
    ]
}

fn dedup_strings(values: Vec<String>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for value in values {
        if !value.is_empty() {
            set.insert(value);
        }
    }
    set.into_iter().collect()
}

/// 创建策略配置服务实例（依赖注入）
fn create_strategy_config_service() -> StrategyConfigService {
    let pool = get_db_pool().clone();
    let repository = SqlxStrategyConfigRepository::new(pool);
    StrategyConfigService::new(Box::new(repository))
}

async fn load_backtest_targets_from_db() -> Result<Vec<(String, String)>> {
    let service = create_strategy_config_service();
    let configs = service.load_all_enabled_configs().await?;

    let mut targets: Vec<(String, String)> = configs
        .into_iter()
        .filter(|cfg| cfg.strategy_type == StrategyType::Nwe)
        .map(|cfg| (cfg.symbol.clone(), cfg.timeframe.as_str().to_string()))
        .collect();

    if targets.is_empty() {
        return Err(anyhow!("未找到启用的 NWE 策略配置"));
    }

    Ok(targets)
}

/// WebSocket数据监听
///
/// 启动WebSocket连接，监听实时行情和K线数据
/// 
/// # 架构说明
/// - 创建策略触发回调函数
/// - 注入到 CandleService 中
/// - K线确认时自动触发策略执行
async fn run_websocket(inst_ids: &[String], periods: &[String]) {
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

    // 创建服务实例
    let config_service = std::sync::Arc::new(create_strategy_config_service());
    let execution_service = std::sync::Arc::new(StrategyExecutionService::new());

    // 🚀 创建策略触发回调函数
    let strategy_trigger = {
        let config_service = std::sync::Arc::clone(&config_service);
        let execution_service = std::sync::Arc::clone(&execution_service);

        std::sync::Arc::new(
            move |inst_id: String, time_interval: String, snap: rust_quant_market::models::CandlesEntity| {
                let config_service = std::sync::Arc::clone(&config_service);
                let execution_service = std::sync::Arc::clone(&execution_service);

                info!(
                    "🎯 K线确认触发策略检查: inst_id={}, time_interval={}, ts={}",
                    inst_id, time_interval, snap.ts
                );

                tokio::spawn(async move {
                    use rust_quant_domain::{StrategyType, Timeframe};
                    use rust_quant_orchestration::workflow::strategy_runner;

                    // 解析时间周期
                    let timeframe = match Timeframe::from_str(&time_interval) {
                        Some(tf) => tf,
                        None => {
                            error!("❌ 无效的时间周期: {}", time_interval);
                            return;
                        }
                    };

                    // 查询该交易对和时间周期的所有启用策略
                    let configs = match config_service
                        .load_configs(&inst_id, &time_interval, None)
                        .await
                    {
                        Ok(configs) => configs,
                        Err(e) => {
                            error!(
                                "❌ 加载策略配置失败: inst_id={}, time_interval={}, error={}",
                                inst_id, time_interval, e
                            );
                            return;
                        }
                    };

                    if configs.is_empty() {
                        info!(
                            "⚠️  未找到启用的策略配置: inst_id={}, time_interval={}",
                            inst_id, time_interval
                        );
                        return;
                    }

                    info!(
                        "✅ 找到 {} 个策略配置，开始执行",
                        configs.len()
                    );

                    // 执行每个策略
                    for config in configs {
                        let strategy_type = config.strategy_type;
                        let config_id = config.id;

                        if let Err(e) = strategy_runner::execute_strategy(
                            &inst_id,
                            timeframe,
                            strategy_type,
                            Some(config_id),
                            &config_service,
                            &execution_service,
                        )
                        .await
                        {
                            error!(
                                "❌ 策略执行失败: inst_id={}, time_interval={}, strategy={:?}, error={}",
                                inst_id, time_interval, strategy_type, e
                            );
                        } else {
                            info!(
                                "✅ 策略执行完成: inst_id={}, time_interval={}, strategy={:?}",
                                inst_id, time_interval, strategy_type
                            );
                        }
                    }
                });
            },
        )
    };

    let inst_ids_vec: Vec<String> = inst_ids.to_vec();
    let periods_vec: Vec<String> = periods.to_vec();

    // 使用带策略触发的 WebSocket 服务
    streams::run_socket_with_strategy_trigger(&inst_ids_vec, &periods_vec, Some(strategy_trigger))
        .await;
}

/// 从数据库加载策略配置并启动
///
/// 通过services层加载配置，使用orchestration层启动策略
async fn start_strategies_from_db() -> Result<()> {
    use rust_quant_domain::StrategyType;
    use rust_quant_domain::Timeframe;
    use rust_quant_orchestration::workflow::strategy_runner;

    info!("📚 从数据库加载策略配置");

    // 1. 通过服务层加载启用的策略配置
    let config_service = create_strategy_config_service();
    let execution_service = StrategyExecutionService::new();

    let configs = config_service.load_all_enabled_configs().await?;

    if configs.is_empty() {
        warn!("⚠️  未找到启用的策略配置");
        return Ok(());
    }

    info!("✅ 加载了 {} 个策略配置", configs.len());

    // 2. 启动每个策略
    for config in configs.iter() {
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

        // 3. 调用 orchestration 层启动策略
        if let Err(e) = strategy_runner::execute_strategy(
            &inst_id,
            timeframe,
            strategy_type,
            Some(config_id),
            &config_service,
            &execution_service,
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
        }
    }

    info!("✅ 策略启动完成");
    Ok(())
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
    let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "local".to_string());
    info!("🕐 应用环境: {}", app_env);
    if app_env != "local" {
        info!("校验系统时间与 OKX 时间差");
        if let Err(e) = okx::utils::validate_system_time().await {
            error!("⚠️  系统时间校验失败: {}", e);
        }
    }

    // 运行模式编排
    run_modes().await?;

    // 启动心跳任务
    let heartbeat_handle = tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            info!("💓 程序正在运行中...");
        }
    });

    // 信号处理
    let signal_name = setup_shutdown_signals().await;
    info!("📡 接收到 {} 信号", signal_name);

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

/// 设置多种退出信号处理
async fn setup_shutdown_signals() -> &'static str {
    use tokio::signal;

    #[cfg(unix)]
    {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .expect("Failed to register SIGINT handler");
        let mut sigquit = signal::unix::signal(signal::unix::SignalKind::quit())
            .expect("Failed to register SIGQUIT handler");

        tokio::select! {
            _ = sigterm.recv() => "SIGTERM",
            _ = sigint.recv() => "SIGINT",
            _ = sigquit.recv() => "SIGQUIT",
        }
    }

    #[cfg(not(unix))]
    {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        "CTRL+C"
    }
}
