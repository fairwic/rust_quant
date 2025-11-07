//! 策略系统指标监控模块
//!
//! 提供策略系统的性能指标收集、监控和健康检查功能，
//! 支持实时监控和历史数据分析。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};
use serde::{Deserialize, Serialize};

use crate::framework::scheduler_service::{SchedulerService, SchedulerHealth};

/// 策略性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyPerformanceMetrics {
    /// 策略标识
    pub strategy_key: String,
    /// 启动次数
    pub start_count: u64,
    /// 停止次数
    pub stop_count: u64,
    /// 平均启动时间（毫秒）
    pub avg_start_time_ms: f64,
    /// 平均停止时间（毫秒）
    pub avg_stop_time_ms: f64,
    /// 热更新次数
    pub hot_update_count: u64,
    /// 执行成功次数
    pub execution_success_count: u64,
    /// 执行失败次数
    pub execution_failure_count: u64,
    /// 最后更新时间
    pub last_update_time: i64,
}

/// 系统健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// 总策略数
    pub total_strategies: usize,
    /// 运行中策略数
    pub running_strategies: usize,
    /// 暂停策略数
    pub paused_strategies: usize,
    /// 失败策略数
    pub failed_strategies: usize,
    /// 调度器健康状态
    pub scheduler_health: String,
    /// 平均启动时间
    pub avg_start_time_ms: f64,
    /// 平均停止时间
    pub avg_stop_time_ms: f64,
    /// 系统运行时间
    pub system_uptime_ms: u64,
    /// 最后检查时间
    pub last_check_time: i64,
}

/// 策略指标收集器
pub struct StrategyMetrics {
    /// 性能指标存储
    metrics: Arc<RwLock<HashMap<String, StrategyPerformanceMetrics>>>,
    /// 系统启动时间
    system_start_time: Instant,
}

impl StrategyMetrics {
    /// 创建新的指标收集器
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            system_start_time: Instant::now(),
        }
    }

    /// 记录策略启动时间
    pub async fn record_strategy_start_time(&self, strategy_key: &str, duration: Duration) {
        let mut metrics = self.metrics.write().await;
        let metric = metrics.entry(strategy_key.to_string()).or_insert_with(|| {
            StrategyPerformanceMetrics {
                strategy_key: strategy_key.to_string(),
                start_count: 0,
                stop_count: 0,
                avg_start_time_ms: 0.0,
                avg_stop_time_ms: 0.0,
                hot_update_count: 0,
                execution_success_count: 0,
                execution_failure_count: 0,
                last_update_time: chrono::Utc::now().timestamp_millis(),
            }
        });

        // 更新平均启动时间
        let new_time_ms = duration.as_millis() as f64;
        metric.avg_start_time_ms = (metric.avg_start_time_ms * metric.start_count as f64 + new_time_ms) 
            / (metric.start_count + 1) as f64;
        metric.start_count += 1;
        metric.last_update_time = chrono::Utc::now().timestamp_millis();

        debug!("记录策略启动时间: {} - {}ms", strategy_key, new_time_ms);
    }

    /// 记录策略停止时间
    pub async fn record_strategy_stop_time(&self, strategy_key: &str, duration: Duration) {
        let mut metrics = self.metrics.write().await;
        if let Some(metric) = metrics.get_mut(strategy_key) {
            // 更新平均停止时间
            let new_time_ms = duration.as_millis() as f64;
            metric.avg_stop_time_ms = (metric.avg_stop_time_ms * metric.stop_count as f64 + new_time_ms) 
                / (metric.stop_count + 1) as f64;
            metric.stop_count += 1;
            metric.last_update_time = chrono::Utc::now().timestamp_millis();

            debug!("记录策略停止时间: {} - {}ms", strategy_key, new_time_ms);
        }
    }

    /// 记录热更新操作
    pub async fn record_hot_update(&self, strategy_key: &str) {
        let mut metrics = self.metrics.write().await;
        if let Some(metric) = metrics.get_mut(strategy_key) {
            metric.hot_update_count += 1;
            metric.last_update_time = chrono::Utc::now().timestamp_millis();
            debug!("记录策略热更新: {}", strategy_key);
        }
    }

    /// 记录策略执行成功
    pub async fn record_execution_success(&self, strategy_key: &str) {
        let mut metrics = self.metrics.write().await;
        if let Some(metric) = metrics.get_mut(strategy_key) {
            metric.execution_success_count += 1;
            metric.last_update_time = chrono::Utc::now().timestamp_millis();
        }
    }

    /// 记录策略执行失败
    pub async fn record_execution_failure(&self, strategy_key: &str) {
        let mut metrics = self.metrics.write().await;
        if let Some(metric) = metrics.get_mut(strategy_key) {
            metric.execution_failure_count += 1;
            metric.last_update_time = chrono::Utc::now().timestamp_millis();
        }
    }

    /// 获取策略性能指标
    pub async fn get_strategy_metrics(&self, strategy_key: &str) -> Option<StrategyPerformanceMetrics> {
        let metrics = self.metrics.read().await;
        metrics.get(strategy_key).cloned()
    }

    /// 获取所有策略指标
    pub async fn get_all_metrics(&self) -> HashMap<String, StrategyPerformanceMetrics> {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// 获取系统健康状态
    pub async fn get_system_health(&self, strategy_manager: &crate::trading::strategy::strategy_manager::StrategyManager) -> SystemHealth {
        let running_strategies = strategy_manager.get_running_strategies().await;
        let scheduler_health = SchedulerService::get_scheduler_health().await.to_string();
        
        let total_strategies = running_strategies.len();
        let running_count = running_strategies.iter()
            .filter(|s| matches!(s.status, crate::trading::strategy::strategy_manager::StrategyStatus::Running))
            .count();
        let paused_count = running_strategies.iter()
            .filter(|s| matches!(s.status, crate::trading::strategy::strategy_manager::StrategyStatus::Paused))
            .count();
        let failed_count = running_strategies.iter()
            .filter(|s| matches!(s.status, crate::trading::strategy::strategy_manager::StrategyStatus::Error(_)))
            .count();

        // 计算平均时间
        let metrics = self.metrics.read().await;
        let (avg_start_time, avg_stop_time) = if metrics.is_empty() {
            (0.0, 0.0)
        } else {
            let total_start_time: f64 = metrics.values().map(|m| m.avg_start_time_ms).sum();
            let total_stop_time: f64 = metrics.values().map(|m| m.avg_stop_time_ms).sum();
            (total_start_time / metrics.len() as f64, total_stop_time / metrics.len() as f64)
        };

        SystemHealth {
            total_strategies,
            running_strategies: running_count,
            paused_strategies: paused_count,
            failed_strategies: failed_count,
            scheduler_health,
            avg_start_time_ms: avg_start_time,
            avg_stop_time_ms: avg_stop_time,
            system_uptime_ms: self.system_start_time.elapsed().as_millis() as u64,
            last_check_time: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// 清理过期指标（定期清理）
    pub async fn cleanup_expired_metrics(&self, retention_hours: u64) {
        let mut metrics = self.metrics.write().await;
        let cutoff_time = chrono::Utc::now().timestamp_millis() - (retention_hours * 3600 * 1000) as i64;
        
        metrics.retain(|key, metric| {
            if metric.last_update_time < cutoff_time {
                debug!("清理过期指标: {}", key);
                false
            } else {
                true
            }
        });
    }
}

/// 全局指标收集器实例
static METRICS: once_cell::sync::OnceCell<Arc<StrategyMetrics>> = once_cell::sync::OnceCell::new();

/// 获取全局指标收集器
pub fn get_strategy_metrics() -> Arc<StrategyMetrics> {
    METRICS
        .get_or_init(|| Arc::new(StrategyMetrics::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_recording() {
        let metrics = StrategyMetrics::new();
        let strategy_key = "test_strategy";
        
        // 记录启动时间
        metrics.record_strategy_start_time(strategy_key, Duration::from_millis(100)).await;
        
        let metric = metrics.get_strategy_metrics(strategy_key).await.unwrap();
        assert_eq!(metric.start_count, 1);
        assert_eq!(metric.avg_start_time_ms, 100.0);
    }

    #[tokio::test]
    async fn test_metrics_cleanup() {
        let metrics = StrategyMetrics::new();
        let strategy_key = "test_strategy";
        
        metrics.record_strategy_start_time(strategy_key, Duration::from_millis(100)).await;
        assert!(metrics.get_strategy_metrics(strategy_key).await.is_some());
        
        // 清理过期指标（0小时保留期，应该清理所有）
        metrics.cleanup_expired_metrics(0).await;
        assert!(metrics.get_strategy_metrics(strategy_key).await.is_none());
    }
}
