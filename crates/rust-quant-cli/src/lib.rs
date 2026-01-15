//! # Rust Quant CLI
//!
//! 量化交易系统主程序入口

pub mod app;

use anyhow::Result;
use dotenv::dotenv;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::JobScheduler;
use tracing::{error, info};

// 重新导出核心依赖
pub use rust_quant_execution::*;
pub use rust_quant_strategies::*;

/// 应用初始化
pub async fn app_init() -> Result<()> {
    env_logger::init();

    // 加载环境变量
    dotenv().ok();

    // 设置日志
    rust_quant_core::logger::setup_logging().await?;

    // 初始化数据库连接
    rust_quant_core::database::init_db_pool().await?;

    // 初始化 Redis 连接池
    rust_quant_core::cache::init_redis_pool().await?;

    info!("应用初始化完成");
    Ok(())
}

/// 全局调度器
pub static SCHEDULER: Lazy<Arc<Mutex<Option<Arc<JobScheduler>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

/// 初始化并启动调度器
pub async fn init_scheduler() -> Result<Arc<JobScheduler>> {
    let mut scheduler_opt = SCHEDULER.lock().await;

    if scheduler_opt.is_none() {
        let scheduler = JobScheduler::new().await?;
        scheduler.start().await?;
        let arc_scheduler = Arc::new(scheduler);
        *scheduler_opt = Some(Arc::clone(&arc_scheduler));
        return Ok(arc_scheduler);
    }

    Ok(Arc::clone(scheduler_opt.as_ref().unwrap()))
}

/// 运行主程序 - 委托给 app::bootstrap
pub async fn run() -> Result<()> {
    app::bootstrap::run().await
}

/// 优雅关闭配置
#[derive(Debug, Clone)]
pub struct GracefulShutdownConfig {
    pub total_timeout_secs: u64,
    pub strategy_stop_timeout_secs: u64,
    pub scheduler_shutdown_timeout_secs: u64,
    pub db_cleanup_timeout_secs: u64,
}

impl Default for GracefulShutdownConfig {
    fn default() -> Self {
        Self {
            total_timeout_secs: 30,
            strategy_stop_timeout_secs: 15,
            scheduler_shutdown_timeout_secs: 5,
            db_cleanup_timeout_secs: 5,
        }
    }
}

/// 优雅关闭
pub async fn graceful_shutdown() -> Result<()> {
    graceful_shutdown_with_config(GracefulShutdownConfig::default()).await
}

/// 带配置的优雅关闭
pub async fn graceful_shutdown_with_config(config: GracefulShutdownConfig) -> Result<()> {
    info!("开始优雅关闭... 总超时: {}秒", config.total_timeout_secs);

    let manager = rust_quant_core::config::shutdown_manager::ShutdownManager::new(
        rust_quant_core::config::shutdown_manager::ShutdownConfig {
            total_timeout: std::time::Duration::from_secs(config.total_timeout_secs),
            hook_timeout: std::time::Duration::from_secs(config.total_timeout_secs),
            force_exit_on_timeout: false,
        },
    );

    // 1) 关闭调度器
    let scheduler_secs = config.scheduler_shutdown_timeout_secs;
    manager
        .register_shutdown_hook("scheduler_shutdown".to_string(), move || async move {
            let dur = tokio::time::Duration::from_secs(scheduler_secs);
            let result = tokio::time::timeout(dur, shutdown_scheduler()).await;
            match result {
                Ok(_) => Ok(()),
                Err(_) => {
                    error!("调度器关闭超时 ({}秒)", scheduler_secs);
                    Ok(())
                }
            }
        })
        .await;

    // 2) 关闭数据库
    let db_secs = config.db_cleanup_timeout_secs;
    manager
        .register_shutdown_hook("db_cleanup".to_string(), move || async move {
            let dur = tokio::time::Duration::from_secs(db_secs);
            let result =
                tokio::time::timeout(dur, rust_quant_core::database::close_db_pool()).await;
            match result {
                Ok(_) => Ok(()),
                Err(_) => {
                    error!("数据库清理超时 ({}秒)", db_secs);
                    Ok(())
                }
            }
        })
        .await;

    // 3) 关闭 Redis
    manager
        .register_shutdown_hook("redis_cleanup".to_string(), || async {
            if let Err(e) = rust_quant_core::cache::cleanup_redis_pool().await {
                error!("清理 Redis 连接池失败: {}", e);
            }
            Ok(())
        })
        .await;

    // 统一执行关闭
    manager.shutdown().await
}

/// 关闭调度器
async fn shutdown_scheduler() -> Result<()> {
    info!("正在关闭调度器...");

    let scheduler_guard = SCHEDULER.lock().await;
    if let Some(scheduler) = scheduler_guard.as_ref() {
        info!("调度器引用计数: {}", Arc::strong_count(scheduler));
        drop(scheduler_guard);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        info!("调度器关闭完成");
    } else {
        info!("调度器未初始化，跳过关闭");
    }

    Ok(())
}
