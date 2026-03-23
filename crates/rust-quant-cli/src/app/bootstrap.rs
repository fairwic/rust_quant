//! # 应用启动引导模块
//!  
//! 简化版本 - 只保留核心功能

use anyhow::{anyhow, Result};
use rust_quant_core::config::env_is_true;
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::{StrategyConfig, StrategyType};
use rust_quant_infrastructure::repositories::{
    SqlxStrategyConfigRepository, SqlxSwapOrderRepository,
};
use std::sync::Arc;
use tracing::{error, info, warn};

use rust_quant_market::streams;
use rust_quant_orchestration::workflow::{backtest_runner, data_sync, tickets_job};
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};
use std::collections::BTreeSet;

/// 运行基于环境变量控制的各个模式
pub async fn run_modes() -> Result<()> {
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

    info!(" 监控交易对: {:?}", inst_ids);
    info!("🕒 监控周期: {:?}", periods);
    info!("🎯 回测目标: {:?}", backtest_targets);

    // 1) 数据同步任务（Ticker & Funding Rate）
    if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        info!("📡 启动数据同步任务");
        // 快速同步场景可跳过 tickers（例如只想补齐 BTC 4H）
        if !env_is_true("SYNC_SKIP_TICKERS", false) {
            if let Err(error) = tickets_job::sync_tickers(&inst_ids).await {
                error!("❌ Ticker同步失败: {}", error);
            }
        }
        if let Err(error) = data_sync::sync_market_data(&inst_ids, &periods).await {
            error!("❌ K线数据同步失败: {}", error);
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

        let config_service = Arc::new(create_strategy_config_service());
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

        info!("🌐 WebSocket模式已启用");
        info!("📡 启动WebSocket监听: {:?}", ws_inst_ids);

        let (config_service, execution_service) = match live_runtime_services {
            Some((config_service, execution_service)) => (config_service, execution_service),
            None => {
                let config_service = Arc::new(create_strategy_config_service());
                let swap_order_repo = Arc::new(SqlxSwapOrderRepository::new(get_db_pool().clone()));
                let execution_service = Arc::new(StrategyExecutionService::new(swap_order_repo));
                (config_service, execution_service)
            }
        };

        // 注意：WebSocket 客户端内部包含 !Send 的锁卫（okx crate），不能用 tokio::spawn
        // 长期运行逻辑由 run() 通过 select! 方式与信号处理并行编排
        run_websocket(&ws_inst_ids, &ws_periods, config_service, execution_service).await;
    }

    Ok(())
}

fn default_backtest_targets() -> Vec<(String, String)> {
    vec![
        // ("ETH-USDT-SWAP".to_string(), "15m".to_string()),
        ("ETH-USDT-SWAP".to_string(), "4H".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "1H".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "5m".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "1Dutc".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "5m".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "15m".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "1H".to_string()),
        ("BTC-USDT-SWAP".to_string(), "4H".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "1Dutc".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "5m".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "15m".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "1H".to_string()),
        ("SOL-USDT-SWAP".to_string(), "4H".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "1Dutc".to_string()),
        ("BCH-USDT-SWAP".to_string(), "4H".to_string()),
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

fn derive_ws_targets_from_configs(configs: &[StrategyConfig]) -> (Vec<String>, Vec<String>) {
    let inst_ids = dedup_strings(configs.iter().map(|cfg| cfg.symbol.clone()).collect());
    let periods = dedup_strings(
        configs
            .iter()
            .map(|cfg| cfg.timeframe.as_str().to_string())
            .collect(),
    );
    (inst_ids, periods)
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

    let targets: Vec<(String, String)> = configs
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
///
/// # 注意
/// WebSocket 模式下策略预热由 `start_strategies_from_db` 统一处理
/// 确保 `IS_RUN_REAL_STRATEGY=true` 时先完成预热再启动 WebSocket
async fn run_websocket(
    inst_ids: &[String],
    periods: &[String],
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
    streams::run_socket_with_strategy_trigger(&inst_ids_vec, &periods_vec, Some(strategy_trigger))
        .await;
}

/// 从数据库加载策略配置并启动
///
/// 通过services层加载配置，使用orchestration层启动策略
///
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
    use rust_quant_market::models::{CandlesEntity, CandlesModel, SelectCandleReqDto};
    use rust_quant_orchestration::workflow::strategy_runner;
    use rust_quant_services::strategy::StrategyDataService;

    info!("📚 从数据库加载策略配置");

    // 1. 通过服务层加载启用的策略配置
    let configs = config_service.load_all_enabled_configs().await?;

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
            continue;
        }

        let snap: Option<CandlesEntity> = {
            let candles_model = CandlesModel::new();
            let dto = SelectCandleReqDto {
                inst_id: inst_id.clone(),
                time_interval: timeframe.as_str().to_string(),
                limit: 1,
                select_time: None,
                confirm: Some(1),
            };
            let mut candles = candles_model
                .get_all(dto)
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
    if open_socket {
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

/// 设置多种退出信号处理
async fn setup_shutdown_signals() -> &'static str {
    use tokio::signal;

    #[cfg(unix)]
    {
        let mut sigterm = match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                error!("❌ 注册 SIGTERM 失败: {}", e);
                return "SIGNAL_SETUP_FAILED";
            }
        };
        let mut sigint = match signal::unix::signal(signal::unix::SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                error!("❌ 注册 SIGINT 失败: {}", e);
                return "SIGNAL_SETUP_FAILED";
            }
        };
        let mut sigquit = match signal::unix::signal(signal::unix::SignalKind::quit()) {
            Ok(s) => s,
            Err(e) => {
                error!("❌ 注册 SIGQUIT 失败: {}", e);
                return "SIGNAL_SETUP_FAILED";
            }
        };

        // 注意：tokio 的 unix Signal::recv() 返回 Option<()>。
        // 在极少数情况下（底层 stream 被关闭）会立刻返回 None，如果不处理会导致程序“无信号也退出”。
        loop {
            tokio::select! {
                v = sigterm.recv() => {
                    if v.is_some() {
                        break "SIGTERM";
                    }
                    warn!("⚠️ SIGTERM 信号流已关闭，继续等待其他信号");
                }
                v = sigint.recv() => {
                    if v.is_some() {
                        break "SIGINT";
                    }
                    warn!("⚠️ SIGINT 信号流已关闭，继续等待其他信号");
                }
                v = sigquit.recv() => {
                    if v.is_some() {
                        break "SIGQUIT";
                    }
                    warn!("⚠️ SIGQUIT 信号流已关闭，继续等待其他信号");
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        if let Err(e) = signal::ctrl_c().await {
            error!("❌ 监听 CTRL+C 失败: {}", e);
            return "SIGNAL_SETUP_FAILED";
        }
        "CTRL+C"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_quant_domain::{StrategyConfig, StrategyStatus, Timeframe};

    fn test_config(id: i64, symbol: &str, timeframe: Timeframe) -> StrategyConfig {
        StrategyConfig {
            id,
            strategy_type: StrategyType::Vegas,
            symbol: symbol.to_string(),
            timeframe,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        }
    }

    #[test]
    fn test_derive_ws_targets_from_configs_dedup() {
        let configs = vec![
            test_config(1, "BTC-USDT-SWAP", Timeframe::H4),
            test_config(2, "BTC-USDT-SWAP", Timeframe::H4),
            test_config(3, "ETH-USDT-SWAP", Timeframe::H1),
        ];

        let (inst_ids, periods) = derive_ws_targets_from_configs(&configs);
        assert_eq!(
            inst_ids,
            vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()]
        );
        assert_eq!(periods, vec!["1H".to_string(), "4H".to_string()]);
    }

    #[test]
    fn test_derive_ws_targets_from_configs_empty() {
        let configs = vec![];
        let (inst_ids, periods) = derive_ws_targets_from_configs(&configs);
        assert!(inst_ids.is_empty());
        assert!(periods.is_empty());
    }

    #[test]
    fn test_default_backtest_targets_only_keep_eth_btc_sol_for_4h() {
        let targets = default_backtest_targets();
        assert_eq!(
            targets,
            vec![
                ("ETH-USDT-SWAP".to_string(), "4H".to_string()),
                ("BTC-USDT-SWAP".to_string(), "4H".to_string()),
                ("SOL-USDT-SWAP".to_string(), "4H".to_string()),
            ]
        );
    }
}
