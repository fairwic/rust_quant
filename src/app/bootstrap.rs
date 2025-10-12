use anyhow::anyhow;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::job::RiskBalanceWithLevelJob;
use crate::socket;
use crate::trading::indicator::vegas_indicator::VegasStrategy;
use crate::trading::model::strategy::strategy_config::StrategyConfigEntityModel;
use crate::trading::strategy::{
    order::strategy_config::StrategyConfig,
    strategy_common::BasicRiskStrategyConfig,
    strategy_manager::get_strategy_manager,
    StrategyType,
};
use crate::trading::task::{self, tickets_job};
use okx::dto::EnumToStrTrait;
use crate::app_config::env::{env_is_true, env_or_default};

/// 运行基于环境变量控制的各个模式（数据同步、回测、WebSocket、实盘策略）
pub async fn run_modes() -> anyhow::Result<()> {
    // 可根据需要从环境加载，当前保持项目的默认值
    // let inst_ids = Some(vec!["ETH-USDT-SWAP","BTC-USDT-SWAP","SOL-USDT-SWAP"]);
    // let period = Some(vec!["1H","4H","1Dutc"]);

    let inst_ids = Some(vec!["ETH-USDT-SWAP"]);
    let period = Some(vec!["4H"]);

    // 1) 初始化需要同步的数据
    if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        if let Err(error) = tickets_job::init_all_ticker(inst_ids.clone()).await {
            error!("init all tickers error: {}", error);
        }
        match (&inst_ids, &period) {
            (Some(ids), Some(times)) => {
                if let Err(error) = task::basic::run_sync_data_job(Some(ids.clone()), times).await {
                    error!("run sync [tickets] data job error: {}", error);
                }
            }
            _ => warn!("跳过数据同步：未设置 inst_ids 或 period"),
        }
        // 可选：同步精英交易员交易数据（按需开启）
        // if let Err(error) = task::big_data_job::sync_top_contract(inst_ids.clone(), period.clone()).await {
        //     error!("run sync [top contract] data job error: {}", error);
        // }
    }

    // 2) 本地环境下执行回测任务
    if env_is_true("IS_BACK_TEST", false) {
        info!("IS_BACK_TEST 已启用");
        if let (Some(inst_id), Some(times)) = (inst_ids.clone(), period.clone()) {
            for inst_id in inst_id {
                for time in times.iter() {
                    let time = time.to_string();
                    if let Err(error) = task::basic::vegas_back_test(inst_id, &time).await {
                        error!(
                            "run strategy error: {} {} {}",
                            error,
                            inst_id,
                            time
                        );
                    }
                }
            }
        } else {
            warn!("跳过回测：未设置 inst_ids 或 period");
        }
    }

    // 3) WebSocket 实时数据
    if env_is_true("IS_OPEN_SOCKET", false) {
        match (inst_ids.clone(), period.clone()) {
            (Some(inst_id), Some(times)) => {
                socket::websocket_service::run_socket(inst_id, times).await;
            }
            _ => warn!("无法启动WebSocket：未设置 inst_ids 或 period"),
        }
    }

    // 4) 实盘策略
    if env_is_true("IS_RUN_REAL_STRATEGY", false) {
        info!("run real strategy job");
        if let Some(inst_ids) = inst_ids.clone() {
            // 风险控制初始化
            let risk_job = RiskBalanceWithLevelJob::new();
            if let Err(e) = risk_job.run(&inst_ids).await {
                error!("风险控制初始化失败: {}", e);
            }

            let strategy_list = StrategyConfigEntityModel::new().await.get_list().await;
            let strategy_list = match strategy_list {
                Ok(list) => {
                    info!("获取策略配置数量{:?}", list.len());
                    list
                }
                Err(e) => {
                    error!("获取策略配置失败: {:?}", e);
                    return Err(anyhow!("获取策略配置失败: {:?}", e));
                }
            };
            let strategy_manager = get_strategy_manager();

            for strategy in strategy_list.into_iter() {
                let inst_id = strategy.inst_id;
                let time = strategy.time;
                let strategy_type = strategy.strategy_type;

                if &strategy_type == StrategyType::Vegas.as_str() {
                    let strategy_config: VegasStrategy = serde_json::from_str::<VegasStrategy>(&*strategy.value)
                        .map_err(|e| anyhow!("Failed to parse VegasStrategy config: {}", e))?;

                    let risk_config: BasicRiskStrategyConfig = serde_json::from_str::<BasicRiskStrategyConfig>(&*strategy.risk_config)
                        .map_err(|e| anyhow!("Failed to parse BasicRiskStrategyConfig config: {}", e))?;

                    let _strategy_config = StrategyConfig {
                        strategy_config_id: strategy.id,
                        strategy_config: serde_json::to_string(&strategy_config)?,
                        risk_config: serde_json::to_string(&risk_config)?,
                    };

                    if let Err(e) = strategy_manager.start_strategy(strategy.id, inst_id.clone(), time.clone()).await {
                        error!("启动策略失败: 策略ID={}, 错误: {}", strategy.id, e);
                    }
                }
            }
        }
    }

    Ok(())
}


/// 应用入口总编排：初始化/校时/运行模式/心跳/信号/优雅关闭
pub async fn run() -> anyhow::Result<()> {
    // 初始化并启动调度器
    let _scheduler = match crate::init_scheduler().await {
        Ok(s) => s,
        Err(e) => {
            error!("初始化任务调度器失败: {}", e);
            return Err(anyhow!("初始化任务调度器失败: {}", e));
        }
    };

    // 非本地环境校验系统时间
    let app_env = env_or_default("APP_ENV", crate::ENVIRONMENT_LOCAL);
    if app_env != crate::ENVIRONMENT_LOCAL {
        info!("校验系统时间与 OKX 时间差");
        let _ = okx::utils::validate_system_time().await?;
    }

    // 运行模式编排（数据同步 / 回测 / WebSocket / 实盘策略）
    run_modes().await?;

    // 启动心跳任务，定期输出程序运行状态
    let heartbeat_handle = tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            info!("💓 程序正在运行中，策略任务正常执行...");

            let strategy_manager = get_strategy_manager();
            let running_strategies = strategy_manager.get_running_strategies().await;
            info!("📊 当前运行中的策略数量: {}", running_strategies.len());
        }
    });

    // 增强的信号处理 - 支持多种退出信号
    let shutdown_signal = setup_shutdown_signals();
    let signal_name = shutdown_signal.await;

    // 停止心跳任务
    heartbeat_handle.abort();

    // 优雅关闭流程
    info!("接收到 {} 信号，开始优雅关闭...", signal_name);

    // 创建优雅关闭配置
    let shutdown_config = crate::GracefulShutdownConfig {
        total_timeout_secs: 30,
        strategy_stop_timeout_secs: 20,
        scheduler_shutdown_timeout_secs: 5,
        db_cleanup_timeout_secs: 5,
    };

    // 1. 停止所有策略任务（带超时）
    let strategy_manager = get_strategy_manager();
    let strategy_stop_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(shutdown_config.strategy_stop_timeout_secs),
        strategy_manager.stop_all_strategies()
    ).await;

    match strategy_stop_result {
        Ok(Ok(count)) => info!("已停止 {} 个策略任务", count),
        Ok(Err(e)) => error!("停止策略任务失败: {}", e),
        Err(_) => error!("停止策略任务超时 ({}秒)", shutdown_config.strategy_stop_timeout_secs),
    }

    // 2. 执行优雅关闭
    if let Err(e) = crate::graceful_shutdown_with_config(shutdown_config).await {
        error!("优雅关闭失败: {}", e);
        std::process::exit(1);
    }

    info!("应用已优雅退出");
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
