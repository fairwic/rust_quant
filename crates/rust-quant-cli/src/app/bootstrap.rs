//! # 应用启动引导模块
//!  
//! 简化版本 - 只保留核心功能

use anyhow::{anyhow, Result};
use rust_quant_core::config::env_is_true;
use tracing::{error, info, warn};

use rust_quant_orchestration::workflow::tickets_job;

/// 运行基于环境变量控制的各个模式
pub async fn run_modes() -> Result<()> {
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "local".to_string());

    // 默认交易对和周期
    let inst_ids = vec!["SOL-USDT-SWAP".to_string(), "BTC-USDT-SWAP".to_string()];
    let _periods = vec!["5m".to_string()];

    info!("🚀 应用环境: {}", env);
    info!("📊 监控交易对: {:?}", inst_ids);

    // 1) 数据同步任务（Ticker）
    if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        info!("📡 启动数据同步任务");
        if let Err(error) = tickets_job::sync_tickers(&inst_ids).await {
            error!("❌ Ticker同步失败: {}", error);
        }
    }

    // 2) 回测任务
    if env_is_true("IS_BACK_TEST", false) {
        info!("📈 回测模式已启用");
        // TODO: 实现回测逻辑
        // use rust_quant_orchestration::workflow::backtest_executor;
        // backtest_executor::run_vegas_test(...).await?;
        warn!("⚠️  回测功能待实现");
    }

    // 3) WebSocket 实时数据
    if env_is_true("IS_OPEN_SOCKET", false) {
        info!("🌐 WebSocket模式已启用");
        // TODO: 实现WebSocket逻辑
        // use rust_quant_market::streams::run_socket;
        // run_socket(&inst_ids, &periods).await;
        warn!("⚠️  WebSocket功能待实现");
    }

    // 4) 实盘策略
    if env_is_true("IS_RUN_REAL_STRATEGY", false) {
        info!("🤖 实盘策略模式已启用");
        // TODO: 实现策略运行逻辑
        // use rust_quant_strategies::strategy_manager::get_strategy_manager;
        // let manager = get_strategy_manager();
        // manager.start_all_strategies().await?;
        warn!("⚠️  实盘策略功能待实现");
    }

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
    if app_env != "local" {
        info!("🕐 校验系统时间与 OKX 时间差");
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
