//! 策略任务调度器模块
//!
//! 负责定时任务的创建、管理和调度器交互操作，
//! 与业务逻辑解耦，提供独立的调度服务。

use crate::{
    time_util,
    trading::{
        domain_service::candle_domain_service::CandleDomainService,
        model::entity::candles::entity::CandlesEntity,
        strategy::order::strategy_config::StrategyConfig,
    },
};
use okx::{api::api_trait::OkxApiTrait, OkxMarket};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::Job;
use tracing::{debug, error, info};
use uuid::Uuid;

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
            .unwrap_or(1); // 默认1秒偏移

        let sec = offset_sec.to_string();
        // 根据时间周期设置不同的执行频率
        let cron_expression: String = match time.as_str() {
            "1m" => format!("{} * * * * *", sec),          // 每分钟执行
            "5m" => format!("{} */5 * * * *", sec),        // 每5分钟执行
            "15m" => format!("{} */15 * * * *", sec),      // 每15分钟执行
            "1h" | "1H" => format!("{} 0 * * * *", sec),   // 每小时执行
            "4h" | "4H" => format!("{} 0 */4 * * *", sec), // 每4小时执行
            "1d" | "1D" | "1Dutc" => format!("{} 0 0 * * *", sec), // 每天执行
            _ => {
                error!("未知的时间周期: {}, 使用默认的每30秒执行", time);
                "*/30 * * * * *".to_string()
            }
        };

        // 本地环境：每秒执行一次，用于测试
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "LOCAL".to_string());
        let final_cron_expression = if app_env.eq_ignore_ascii_case("LOCAL") {
            "* * * * * *".to_string()
        } else {
            cron_expression
        };

        debug!(
            "创建定时任务: inst_id={}, time={}, cron={}",
            inst_id, time, final_cron_expression
        );

        let job = Job::new_async(final_cron_expression.as_str(), move |_uuid, _lock| {
            let inst_id = inst_id.clone();
            let time = time.clone();
            let strategy_cfg_handle: Arc<RwLock<StrategyConfig>> = Arc::clone(&strategy_cfg_handle);
            Box::pin(async move {
                info!("job tick started: inst_id={}, time={}", inst_id, time);
                // 每次触发时读取最新配置（支持热更新）
                let current_cfg = {
                    let guard = strategy_cfg_handle.read().await;
                    guard.clone()
                };

                println!("current_cfg: {:?}", current_cfg);
                // 此处特殊处理，直接从交易所获取最新K线数据,不走缓存
                let okx = OkxMarket::from_env();
                println!("okx: {:?}", okx);
                match okx {
                    Ok(okx) => {
                        let after=time_util::get_period_start_timestamp(&time).to_string();
                        match okx
                            .get_candles(&inst_id, &time, Some(&after), None, Some("1"))
                            .await
                        {
                            Ok(candle_data) => {
                                info!("获取到最新K线数据: {}_{}", inst_id, time);
                                if let Some(new_candle_data) = candle_data.first() {
                                    // 这里可以处理新的K线数据
                                   match crate::trading::task::basic::run_ready_to_order_with_manager(
                                    &inst_id,
                                    &time,
                                    &current_cfg,
                                    Some(CandlesEntity::from(new_candle_data)),
                                )
                                .await
                                {
                                    Ok(_) => {
                                        debug!("策略任务执行成功: {}_{}", inst_id, time);
                                    }
                                    Err(e) => {
                                        error!("策略任务执行失败: {}_{}, 错误: {}", inst_id, time, e);
                                    }
                                }
                                }
                            }
                            Err(e) => {
                                error!("获取K线数据失败: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("初始化OKX客户端失败: {:?}", e);
                    }
                }
            })



        })
        .map_err(|e| JobSchedulerError::JobCreationFailed {
            reason: format!("创建定时任务失败: {}", e),
        })?;

        debug!("定时任务创建成功: {}", job.guid());
        Ok(job)
    }

    /// 注册任务到调度器
    pub async fn register_job(job: Job) -> Result<(), JobSchedulerError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard
            .as_ref()
            .ok_or(JobSchedulerError::SchedulerNotInitialized)?;

        scheduler
            .add(job)
            .await
            .map_err(|e| JobSchedulerError::JobRegistrationFailed {
                reason: format!("添加任务到调度器失败: {}", e),
            })?;

        Ok(())
    }

    /// 从调度器中移除单个任务
    pub async fn remove_job(job_id: Uuid) -> Result<(), JobSchedulerError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard
            .as_ref()
            .ok_or(JobSchedulerError::SchedulerNotInitialized)?;

        scheduler
            .remove(&job_id)
            .await
            .map_err(|e| JobSchedulerError::JobRemovalFailed {
                reason: format!("从调度器移除任务失败: {}", e),
            })?;

        Ok(())
    }

    /// 从调度器中批量移除任务
    pub async fn remove_jobs(job_ids: Vec<Uuid>) -> Result<usize, JobSchedulerError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard
            .as_ref()
            .ok_or(JobSchedulerError::SchedulerNotInitialized)?;

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
