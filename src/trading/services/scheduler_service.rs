//! 调度器服务模块
//!
//! 提供统一的调度器操作接口，包含重试机制、错误处理和健康检查，
//! 与具体的策略业务逻辑解耦。

use anyhow::{anyhow, Result};
use okx::{api::api_trait::OkxApiTrait, OkxMarket};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_cron_scheduler::Job;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::app_config::redis_config;
use crate::time_util;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::task::basic;

/// 调度器服务错误类型
#[derive(thiserror::Error, Debug)]
pub enum SchedulerServiceError {
    #[error("调度器未初始化")]
    NotInitialized,

    #[error("任务创建失败: {reason}")]
    JobCreationFailed { reason: String },

    #[error("任务注册失败: {reason}")]
    JobRegistrationFailed { reason: String },

    #[error("任务移除失败: {reason}")]
    JobRemovalFailed { reason: String },

    #[error("调度器操作超时")]
    OperationTimeout,

    #[error("调度器不健康")]
    UnhealthyScheduler,
}

/// 系统健康状态
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchedulerHealth {
    pub is_healthy: bool,
    pub total_jobs: usize,
    pub last_check_time: i64,
    pub error_count: u64,
}

impl std::fmt::Display for SchedulerHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "健康: {}, 总任务: {}, 错误数: {}",
            if self.is_healthy { "是" } else { "否" },
            self.total_jobs,
            self.error_count
        )
    }
}

/// 调度器服务
pub struct SchedulerService;

impl SchedulerService {
    /// 常量定义
    const OPERATION_TIMEOUT_SECS: u64 = 5;
    const MAX_RETRY_ATTEMPTS: u32 = 3;
    const RETRY_DELAY_MS: u64 = 100;

    /// 构建策略任务唯一标识
    pub fn build_task_key(inst_id: &str, time: &str, strategy_type: &str) -> String {
        format!(
            "{}_{}_{}_{}",
            strategy_type.to_lowercase(),
            inst_id,
            time,
            "task"
        )
    }

    /// 创建定时任务
    pub fn create_scheduled_job(
        inst_id: String,
        time: String,
        strategy_type: String,
        strategy_cfg_handle: Arc<RwLock<StrategyConfig>>,
    ) -> Result<Job, SchedulerServiceError> {
        // 获取cron偏移秒数
        let offset_sec: u64 = std::env::var("STRATEGY_CRON_OFFSET_SEC")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|v| v.min(59))
            .unwrap_or(1);

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
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "LOCAL".to_string());
        let final_cron_expression = if app_env.eq_ignore_ascii_case("LOCAL") {
            "*/1 * * * * *".to_string()
        } else {
            cron_expression
        };
        debug!(
            "创建定时任务: inst_id={}, time={}, strategy_type={}, cron={}",
            inst_id, time, strategy_type, final_cron_expression
        );
        let job = Job::new_async(final_cron_expression.as_str(), move |_uuid, _lock| {
            let inst_id = inst_id.clone();
            let time = time.clone();
            let strategy_type = strategy_type.clone();
            let strategy_cfg_handle: Arc<RwLock<StrategyConfig>> = Arc::clone(&strategy_cfg_handle);

            Box::pin(async move {
             let after=time_util::get_period_start_timestamp(&time).to_string();
             //判断是已处理过最新的数据
            let is_processed = Self::is_processed_latest_data(&inst_id, &time,&after).await;
            if is_processed {
                info!("已处理过当前时间戳的数据: deal_confirm_candle:{}:{}:{}", inst_id, time,after);
                return;
            }
                // 每次触发时读取最新配置（支持热更新）
                let current_cfg = {
                    let guard = strategy_cfg_handle.read().await;
                    guard.clone()
                };
                // 此处特殊处理，直接从交易所获取最新K线数据,不走缓存
                let okx = OkxMarket::from_env();
                match okx {
                    Ok(okx) => {
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
                                        //记录到redis，当前k线路已被处理过
                                        Self::mark_processed_latest_data(&inst_id, &time, &new_candle_data.ts.to_string()).await;
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
        .map_err(|e| SchedulerServiceError::JobCreationFailed {
            reason: format!("创建定时任务失败: {}", e),
        })?;

        debug!("定时任务创建成功: {}", job.guid());
        Ok(job)
    }

    async fn is_processed_latest_data(inst_id: &str, time: &str, ts_str: &str) -> bool {
        let ts = time_util::ts_reduce_n_period(ts_str.parse::<i64>().unwrap(), time, 1).unwrap();
        let ts_str = ts.to_string();
        //从redis中获取数据
        let rkey = format!("deal_confirm_candle:{}:{}:{}", inst_id, time, ts_str);
        println!("get rkey:{}", rkey);
        let multi_connection = crate::app_config::redis_config::get_redis_connection().await;
        if let Err(e) = multi_connection {
            error!("获取Redis连接失败: {:?}", e);
            return false;
        }
        if let Ok(mut conn) = multi_connection {
            if let Ok(s) = conn.get::<_, String>(&rkey).await {
                return true;
            }
            return false;
        }
        return false;
    }
    async fn mark_processed_latest_data(inst_id: &str, time: &str, ts_str: &str) {
        let rkey = format!("deal_confirm_candle:{}:{}:{}", inst_id, time, ts_str);
        println!("set rkey:{}", rkey);
        let multi_connection = crate::app_config::redis_config::get_redis_connection().await;
        if let Ok(mut conn) = multi_connection {
            conn.set_ex::<_, _, ()>(&rkey, "1", 86400 * 7).await;
        }
    }
    /// 注册任务到调度器（带重试机制）
    pub async fn register_job(job: Job) -> Result<Uuid, SchedulerServiceError> {
        let job_id = job.guid();
        for attempt in 1..=Self::MAX_RETRY_ATTEMPTS {
            match Self::try_register_job(job.clone()).await {
                Ok(_) => {
                    info!("任务注册成功: {} (尝试次数: {})", job_id, attempt);
                    return Ok(job_id);
                }
                Err(e) if attempt < Self::MAX_RETRY_ATTEMPTS => {
                    warn!("任务注册失败，第{}次重试: {}", attempt, e);
                    tokio::time::sleep(Duration::from_millis(
                        Self::RETRY_DELAY_MS * attempt as u64,
                    ))
                    .await;
                }
                Err(e) => {
                    error!("任务注册最终失败: {}", e);
                    return Err(e);
                }
            }
        }

        Err(SchedulerServiceError::JobRegistrationFailed {
            reason: "达到最大重试次数".to_string(),
        })
    }

    /// 尝试注册任务（单次）
    async fn try_register_job(job: Job) -> Result<(), SchedulerServiceError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard
            .as_ref()
            .ok_or(SchedulerServiceError::NotInitialized)?;

        scheduler
            .add(job)
            .await
            .map_err(|e| SchedulerServiceError::JobRegistrationFailed {
                reason: format!("添加任务到调度器失败: {}", e),
            })?;

        Ok(())
    }

    /// 安全地移除任务（带超时和错误容忍）
    pub async fn remove_job_safe(job_id: Uuid) -> Result<(), SchedulerServiceError> {
        let timeout_duration = Duration::from_secs(Self::OPERATION_TIMEOUT_SECS);

        let result = tokio::time::timeout(timeout_duration, Self::try_remove_job(job_id)).await;

        match result {
            Ok(Ok(_)) => {
                debug!("成功移除调度器任务: {}", job_id);
                Ok(())
            }
            Ok(Err(e)) => {
                warn!("移除调度器任务失败，但不影响系统运行: {}", e);
                // 不返回错误，允许系统继续运行
                Ok(())
            }
            Err(_) => {
                warn!(
                    "移除调度器任务超时 ({}s)，任务可能仍在运行: {}",
                    Self::OPERATION_TIMEOUT_SECS,
                    job_id
                );
                // 超时也不返回错误，允许系统继续运行
                Ok(())
            }
        }
    }

    /// 尝试移除任务（单次）
    async fn try_remove_job(job_id: Uuid) -> Result<(), SchedulerServiceError> {
        let scheduler_guard = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_guard
            .as_ref()
            .ok_or(SchedulerServiceError::NotInitialized)?;

        scheduler
            .remove(&job_id)
            .await
            .map_err(|e| SchedulerServiceError::JobRemovalFailed {
                reason: format!("从调度器移除任务失败: {}", e),
            })?;

        Ok(())
    }

    /// 批量移除任务
    pub async fn batch_remove_jobs(job_ids: Vec<Uuid>) -> Result<usize, SchedulerServiceError> {
        let mut removed_count = 0;
        let mut failed_jobs = Vec::new();

        for job_id in job_ids {
            match Self::remove_job_safe(job_id).await {
                Ok(_) => removed_count += 1,
                Err(_) => failed_jobs.push(job_id),
            }
        }

        if !failed_jobs.is_empty() {
            warn!("部分任务移除失败: {:?}", failed_jobs);
        }

        info!(
            "批量移除任务完成: 成功 {}, 失败 {}",
            removed_count,
            failed_jobs.len()
        );
        Ok(removed_count)
    }

    /// 检查调度器健康状态
    pub async fn get_scheduler_health() -> SchedulerHealth {
        let scheduler_guard = crate::SCHEDULER.lock().await;

        match scheduler_guard.as_ref() {
            Some(scheduler) => {
                // 这里可以添加更多健康检查逻辑
                SchedulerHealth {
                    is_healthy: true,
                    total_jobs: 0, // TODO: 从调度器获取实际任务数量
                    last_check_time: chrono::Utc::now().timestamp_millis(),
                    error_count: 0,
                }
            }
            None => SchedulerHealth {
                is_healthy: false,
                total_jobs: 0,
                last_check_time: chrono::Utc::now().timestamp_millis(),
                error_count: 1,
            },
        }
    }

    /// 检查调度器是否健康
    pub async fn is_scheduler_healthy() -> bool {
        Self::get_scheduler_health().await.is_healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_task_key() {
        let key = SchedulerService::build_task_key("BTC-USDT-SWAP", "5m", "Vegas");
        assert_eq!(key, "vegas_BTC-USDT-SWAP_5m_task");
    }

    #[tokio::test]
    async fn test_scheduler_health_check() {
        let health = SchedulerService::get_scheduler_health().await;
        assert!(health.last_check_time > 0);
    }
}
