//! 策略任务调度器模块
//!
//! 负责定时任务的创建、管理和调度器交互操作，
//! 与业务逻辑解耦，提供独立的调度服务。

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::Job;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::trading::strategy::order::strategy_config::StrategyConfig;

/// 调度器相关的错误类型
#[derive(thiserror::Error, Debug)]
pub enum JobSchedulerError {
    #[error("调度器未初始化")]
    SchedulerNotInitialized,

    #[error("任务创建失败: {reason}")]
    JobCreationFailed { reason: String },

    #[error("任务注册失败: {reason}")]
    JobRegistrationFailed { reason: String },

    #[error("任务移除失败: {reason}")]
    JobRemovalFailed { reason: String },
}

/// 策略任务调度器
pub struct StrategyJobScheduler;

impl StrategyJobScheduler {
    /// 构建策略任务唯一标识
    pub fn build_task_key(inst_id: &str, time: &str) -> String {
        format!("vegas_{}_{}", inst_id, time)
    }

    /// 创建定时任务
    pub fn create_scheduled_job(
        inst_id: String,
        time: String,
        strategy_cfg_handle: Arc<RwLock<StrategyConfig>>,
    ) -> Result<Job, JobSchedulerError> {
        // 获取cron偏移秒数
        let offset_sec: u64 = std::env::var("STRATEGY_CRON_OFFSET_SEC")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|v| v.min(59))
            .unwrap_or(5);

        let sec = offset_sec.to_string();

        // 根据时间周期设置不同的执行频率
        let cron_expression: String = match time.as_str() {
            "1m" => format!("{} * * * * *", sec),
            "5m" => format!("{} */5 * * * *", sec),
            "15m" => format!("{} */15 * * * *", sec),
            "1H" => format!("{} 0 * * * *", sec),
            "4H" => format!("{} 0 */4 * * *", sec),
            "1Dutc" => format!("{} 0 0 * * *", sec),
            _ => "*/30 * * * * *".to_string(),
        };

        // 本地环境：每秒执行一次，用于测试
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_ | "LOCAL".to_string());
        let final_cron_expression = if app_env.eq_ignore_ascii_case("LOCAL") {
            "* * * * * *".to_string()
        } else {
            cron_expression
        };

        debug!("创建定时任务: inst_id={}, time={}, cron={}", inst_id, time, final_cron_expression);

        let job = Job::new_async(final_cron_expression.as_str(), move |_uuid, _lock| {
            let inst_id = inst_id.clone();
            let time = time.clone();
            info!("运行定时任务任务: {}_{}", inst_id, time);
            let strategy_cfg_handle: Arc<RwLock<StrategyConfig>> = Arc::clone(&strategy_cfg_handle);

            Box::pin(async move {
                // 每次触发时读取最新配置（支持热更新）
                let current_cfg = {
                    let guard = strategy_cfg_handle.read().await;
                    guard.clone()
                };
                match crate::trading::task::basic::run_strategy_job(&inst_id, &time, &current_cfg).await {
                    Ok(_) => {
                        debug!("策略任务执行成功: {}_{}", inst_id, time);
                    }
                    Err(e) => {
                        error!("策略任务执行失败: {}_{}, 错误: {}", inst_id, time, e);
                    }
                }
            })
        })
        .map_err(|e| JobSchedulerError::JobCreationFailed {
            reason: format!("创建定时任务失败: {}", e)
        })?;

        debug!("定时任务创建成功: {}", job.guid());
        Ok(job)
    }

    /// 注册任务到调度器
    pub async fn register_job(job: Job) -> Result<(), JobSchedulerError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard.as_ref().ok_or(JobSchedulerError::SchedulerNotInitialized)?;

        scheduler.add(job).await.map_err(|e| JobSchedulerError::JobRegistrationFailed {
            reason: format!("添加任务到调度器失败: {}", e)
        })?;

        Ok(())
    }

    /// 从调度器中移除单个任务
    pub async fn remove_job(job_id: Uuid) -> Result<(), JobSchedulerError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard.as_ref().ok_or(JobSchedulerError::SchedulerNotInitialized)?;

        scheduler.remove(&job_id).await.map_err(|e| JobSchedulerError::JobRemovalFailed {
            reason: format!("从调度器移除任务失败: {}", e)
        })?;

        Ok(())
    }

    /// 从调度器中批量移除任务
    pub async fn remove_jobs(job_ids: Vec<Uuid>) -> Result<usize, JobSchedulerError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard.as_ref().ok_or(JobSchedulerError::SchedulerNotInitialized)?;

        let mut removed_count = 0;
        for job_id in job_ids {
            if scheduler.remove(&job_id).await.is_ok() {
                removed_count += 1;
            }
        }

        Ok(removed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_task_key() {
        let key = StrategyJobScheduler::build_task_key("BTC-USDT-SWAP", "5m");
        assert_eq!(key, "vegas_BTC-USDT-SWAP_5m");
    }
}

