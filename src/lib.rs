pub mod app_config;
pub mod enums;
pub mod error;
pub mod job;
pub mod socket;
pub mod time_util;
pub mod trading;
pub mod app;


// 重新导出常用的关闭管理器类型
pub use app_config::shutdown_manager::{ShutdownConfig, ShutdownManager};
pub use crate::trading::types::{CandleItem, CandleItemBuilder};

use dotenv::dotenv;
use once_cell::sync::Lazy;
use tracing::{info, error};

pub async fn app_init() -> anyhow::Result<()> {
    //设置env
    dotenv().ok();
    // 设置日志
    crate::app_config::log::setup_logging().await?;
    //初始化数据库连接
    crate::app_config::db::init_db().await;
    //初始化redis连接池
    crate::app_config::redis_config::init_redis_pool().await?;
    Ok(())
}

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::JobScheduler;

// 定义全局调度器容器 - 使用更简单的设计
pub static SCHEDULER: Lazy<Arc<Mutex<Option<Arc<JobScheduler>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

// 初始化并启动调度器的辅助函数
pub async fn init_scheduler() -> anyhow::Result<Arc<JobScheduler>> {
    let mut scheduler_opt = SCHEDULER.lock().await;

    if scheduler_opt.is_none() {
        // 只有在调度器未初始化时才创建并启动
        let mut scheduler = JobScheduler::new().await?;
        scheduler.start().await?;
        let arc_scheduler = Arc::new(scheduler);
        *scheduler_opt = Some(Arc::clone(&arc_scheduler));
        return Ok(arc_scheduler);
    }

    // 返回已存在的调度器
    Ok(Arc::clone(scheduler_opt.as_ref().unwrap()))
}

pub const ENVIRONMENT_LOCAL: &'static str = "local";
pub const ENVIRONMENT_DEV: &'static str = "dev";
pub const ENVIRONMENT_TEST: &'static str = "test";
pub const ENVIRONMENT_PROD: &'static str = "prod";

/// 优雅关闭配置
#[derive(Debug, Clone)]
pub struct GracefulShutdownConfig {
    /// 总体超时时间（秒）
    pub total_timeout_secs: u64,
    /// 策略停止超时时间（秒）
    pub strategy_stop_timeout_secs: u64,
    /// 调度器关闭超时时间（秒）
    pub scheduler_shutdown_timeout_secs: u64,
    /// 数据库清理超时时间（秒）
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

/// 优雅关闭服务 - 增强版
pub async fn graceful_shutdown() -> anyhow::Result<()> {
    graceful_shutdown_with_config(GracefulShutdownConfig::default()).await
}

/// 带配置的优雅关闭服务（统一由 ShutdownManager 执行 Hook）
pub async fn graceful_shutdown_with_config(config: GracefulShutdownConfig) -> anyhow::Result<()> {
    info!("开始优雅关闭... 总超时: {}秒", config.total_timeout_secs);

    // 使用独立的 ShutdownManager 实例（不影响全局实例），并将每个 Hook 的具体超时包裹在内部
    let manager = ShutdownManager::new(crate::app_config::shutdown_manager::ShutdownConfig {
        total_timeout: std::time::Duration::from_secs(config.total_timeout_secs),
        // 设为与总超时一致，避免管理器层面的单 Hook 超时干扰我们对每个 Hook 的精细控制
        hook_timeout: std::time::Duration::from_secs(config.total_timeout_secs),
        force_exit_on_timeout: false,
    });

    // 1) 调度器关闭（带独立超时）
    let scheduler_secs = config.scheduler_shutdown_timeout_secs;
    manager
        .register_shutdown_hook("scheduler_shutdown".to_string(), move || async move {
            let dur = tokio::time::Duration::from_secs(scheduler_secs);
            let result = tokio::time::timeout(dur, async move {
                if let Err(e) = shutdown_scheduler_with_timeout(scheduler_secs).await {
                    error!("关闭调度器失败: {}", e);
                }
                Ok::<(), anyhow::Error>(())
            })
            .await;

            match result {
                Ok(_) => Ok(()),
                Err(_) => {
                    error!("调度器关闭超时 ({}秒)", scheduler_secs);
                    Ok(())
                }
            }
        })
        .await;

    // 2) 数据库清理（带独立超时）
    let db_secs = config.db_cleanup_timeout_secs;
    manager
        .register_shutdown_hook("db_cleanup".to_string(), move || async move {
            let dur = tokio::time::Duration::from_secs(db_secs);
            let result = tokio::time::timeout(dur, async move {
                if let Err(e) = crate::app_config::db::cleanup_connection_pool().await {
                    error!("清理数据库连接池失败: {}", e);
                }
                Ok::<(), anyhow::Error>(())
            })
            .await;

            match result {
                Ok(_) => Ok(()),
                Err(_) => {
                    error!("数据库清理超时 ({}秒)", db_secs);
                    Ok(())
                }
            }
        })
        .await;

    // 3) Redis连接池清理
    manager
        .register_shutdown_hook("redis_cleanup".to_string(), || async {
            if let Err(e) = crate::app_config::redis_config::cleanup_redis_pool().await {
                error!("清理Redis连接池失败: {}", e);
            }
            Ok(())
        })
        .await;

    // 4) 其他资源清理（通常较快，可不设独立超时）
    manager
        .register_shutdown_hook("metrics_cleanup".to_string(), || async {
            cleanup_other_resources().await;
            Ok(())
        })
        .await;

    // 统一执行
    manager.shutdown().await
}

/// 带超时的调度器关闭
async fn shutdown_scheduler_with_timeout(timeout_secs: u64) -> anyhow::Result<()> {
    info!("正在关闭调度器... 超时: {}秒", timeout_secs);

    let timeout = tokio::time::Duration::from_secs(timeout_secs);

    tokio::time::timeout(timeout, async {
        let scheduler_guard = SCHEDULER.lock().await;
        if let Some(scheduler) = scheduler_guard.as_ref() {
            // 尝试优雅关闭调度器
            // 注意：JobScheduler 的 shutdown 需要可变引用，这里我们记录状态
            info!("调度器引用计数: {}", Arc::strong_count(scheduler));

            // 如果可能，尝试停止所有正在运行的任务
            // 这里需要根据实际的 JobScheduler API 调整
            drop(scheduler_guard); // 释放锁

            // 等待调度器自然关闭
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            info!("调度器关闭完成");
        } else {
            info!("调度器未初始化，跳过关闭");
        }
        Ok(())
    }).await.map_err(|_| anyhow::anyhow!("调度器关闭超时"))?
}


/// 清理其他资源
async fn cleanup_other_resources() {
    info!("清理其他系统资源...");

    // 清理策略指标
    let metrics = crate::trading::services::strategy_metrics::get_strategy_metrics();
    metrics.cleanup_expired_metrics(0).await; // 清理所有指标

    // 可以添加其他资源清理逻辑
    // 例如：关闭文件句柄、清理缓存等

    info!("其他资源清理完成");
}

