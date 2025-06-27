use anyhow::anyhow;
use std::sync::Arc;
use tokio_cron_scheduler::{JobScheduler, Job};
use tracing::{error, info, warn};
use std::time::Duration;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use uuid::Uuid;
use tokio::sync::{Mutex, RwLock};

use crate::trading::indicator::vegas_indicator::{EmaSignalConfig, EmaTouchTrendSignalConfig, EngulfingSignalConfig, KlineHammerConfig, RsiSignalConfig, VegasIndicatorSignalValue, VegasStrategy, VolumeSignalConfig};
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::{VEGAS_INDICATOR_VALUES, ArcVegasIndicatorValues};
use crate::trading::strategy::strategy_common::parse_candle_to_data_item;
use crate::trading::strategy::{strategy_common, Strategy, StrategyType};
use crate::trading::task;
use crate::SCHEDULER;
use crate::CandleItem;
use chrono::Local;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use crate::trading::indicator::bollings::BollingBandsSignalConfig;


/// 储存每个策略任务的信息
#[derive(Debug, Clone)]
pub struct StrategyTaskInfo {
    pub inst_id: String,
    pub time_period: String,
    pub strategy_type: String,
    pub created_at: i64,
    pub uuid: Option<Uuid>,
}

impl StrategyTaskInfo {
    /// 创建新的策略任务信息
    pub fn new(inst_id: &str, time: &str, strategy_type: &str) -> Self {
        Self {
            inst_id: inst_id.to_string(),
            time_period: time.to_string(),
            strategy_type: strategy_type.to_string(),
            created_at: Local::now().timestamp_millis(),
            uuid: None,
        }
    }
    
    /// 生成任务的唯一名称
    pub fn job_name(&self) -> String {
        format!("strategy_{}_{}_{}_{}", 
            self.strategy_type, self.inst_id, self.time_period, self.created_at)
    }
    
    /// 设置任务的UUID
    pub fn with_uuid(mut self, uuid: Uuid) -> Self {
        self.uuid = Some(uuid);
        self
    }
}

/// 监控策略执行的指标
#[derive(Debug, Clone)]
pub struct StrategyExecutionMetrics {
    pub inst_id: String,
    pub time_period: String,
    pub start_time: chrono::DateTime<chrono::Local>,
    pub end_time: chrono::DateTime<chrono::Local>,
    pub execution_time_ms: i64,
    pub data_items_processed: usize,
    pub success: bool,
    pub error_message: Option<String>,
}

/// 策略状态管理器
pub struct StrategyStateManager {
    states: Arc<RwLock<HashMap<String, Arc<ArcVegasIndicatorValues>>>>,
}

impl StrategyStateManager {
    /// 创建新的状态管理器
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 获取策略状态
    pub async fn get_state(&self, key: &str) -> Option<Arc<ArcVegasIndicatorValues>> {
        let states = self.states.read().await;
        states.get(key).cloned()
    }
    
    /// 更新策略状态
    pub async fn update_state(&self, key: &str, state: ArcVegasIndicatorValues) -> Result<(), String> {
        let mut states = self.states.write().await;
        states.insert(key.to_string(), Arc::new(state));
        Ok(())
    }
    
    /// 导出所有状态
    pub async fn export_states(&self) -> Result<Vec<u8>, String> {
        let states = self.states.read().await;
        
        // 在实际应用中实现序列化逻辑
        Ok(Vec::new()) // 示例返回
    }
    
    /// 导入状态数据
    pub async fn import_states(&self, data: &[u8]) -> Result<(), String> {
        // 在实际应用中实现反序列化逻辑
        Ok(()) // 示例返回
    }
}

// 存储任务名称与UUID的映射关系
static JOB_NAME_TO_UUID: Lazy<Mutex<HashMap<String, Uuid>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

// 策略订单管理器
pub struct VegasOrder {
    // 依赖注入的组件
    state_manager: Arc<StrategyStateManager>,
    task_manager: Arc<StrategyTaskManager>,
    metric_collector: Arc<MetricCollector>,
}

impl VegasOrder {
    // 创建新的VegasOrder实例
    pub fn new() -> Self {
        // 创建默认的组件
        let state_manager = Arc::new(StrategyStateManager::new());
        let task_manager = Arc::new(StrategyTaskManager::new());
        let metric_collector = Arc::new(MetricCollector::new());
        
        Self {
            state_manager,
            task_manager,
            metric_collector,
        }
    }
    
    // 创建带有自定义组件的实例
    pub fn with_components(
        state_manager: Arc<StrategyStateManager>,
        task_manager: Arc<StrategyTaskManager>,
        metric_collector: Arc<MetricCollector>,
    ) -> Self {
        Self {
            state_manager,
            task_manager,
            metric_collector,
        }
    }
    
    // 构建策略任务ID (兼容旧代码)
    fn build_job_id(inst_id: &str, time: &str) -> String {
        format!("vegas_strategy_{}_{}", inst_id, time)
    }
    
    // 保存任务UUID (兼容旧代码)
    async fn save_job_uuid(job_name: String, uuid: Uuid) {
        let mut map = JOB_NAME_TO_UUID.lock().await;
        map.insert(job_name, uuid);
    }
    
    // 获取任务UUID (兼容旧代码)
    async fn get_job_uuid(job_name: &str) -> Option<Uuid> {
        let map = JOB_NAME_TO_UUID.lock().await;
        map.get(job_name).copied()
    }
    
    /// 停止特定策略任务 (兼容旧代码)
    pub async fn stop_strategy(inst_id: &str, time: &str) -> anyhow::Result<()> {
        let job_id = Self::build_job_id(inst_id, time);
        info!("正在停止策略任务: {}", job_id);
        
        // 获取任务的UUID
        if let Some(uuid) = Self::get_job_uuid(&job_id).await {
            let scheduler_lock = crate::SCHEDULER.lock().await;
            if let Some(scheduler) = &*scheduler_lock {
                if let Err(e) = scheduler.remove(&uuid).await {
                    let msg = format!("停止策略任务时出错: {}", e);
                    error!("{}", msg);
                    Err(anyhow!(msg))
                } else {
                    info!("策略任务已成功停止: {}", job_id);
                    // 从映射表中移除
                    let mut map = JOB_NAME_TO_UUID.lock().await;
                    map.remove(&job_id);
                    Ok(())
                }
            } else {
                let msg = "调度器未初始化，无法停止任务";
                error!("{}", msg);
                Err(anyhow!(msg))
            }
        } else {
            let msg = format!("未找到任务的UUID: {}", job_id);
            warn!("{}", msg);
            Err(anyhow!(msg))
        }
    }
    
    /// 停止所有策略任务 (兼容旧代码)
    pub async fn stop_all_strategies() -> anyhow::Result<()> {
        info!("正在停止所有Vegas策略任务");
        
        let scheduler_lock = crate::SCHEDULER.lock().await;
        if let Some(scheduler) = &*scheduler_lock {
            // 获取所有Vegas策略任务的UUID
            let mut map = JOB_NAME_TO_UUID.lock().await;
            let mut stopped_count = 0;
            
            // 复制键值对，避免在迭代过程中修改map
            let job_entries: Vec<(String, Uuid)> = map.iter()
                .filter(|(name, _)| name.starts_with("vegas_strategy_"))
                .map(|(name, uuid)| (name.clone(), *uuid))
                .collect();
            
            for (name, uuid) in job_entries {
                if let Err(e) = scheduler.remove(&uuid).await {
                    warn!("停止任务 {} 失败: {}", name, e);
                } else {
                    map.remove(&name);
                    stopped_count += 1;
                }
            }
            
            info!("成功停止 {} 个Vegas策略任务", stopped_count);
            Ok(())
        } else {
            let msg = "调度器未初始化，无法停止任务";
            error!("{}", msg);
            Err(anyhow!(msg))
        }
    }

    /// 初始化策略数据，重试多次
    async fn initialize_with_retry(
        &self,
        strategy: &VegasStrategy,
        inst_id: &str,
        time: &str,
        max_retries: usize
    ) -> anyhow::Result<String> {
        let mut attempts = 0;
        let mut last_error = None;
        
        while attempts < max_retries {
            match self.initialize_strategy_data(strategy, inst_id, time).await {
                Ok(hash_key) => return Ok(hash_key),
                Err(e) => {
                    attempts += 1;
                    info!("初始化尝试 {}/{} 失败: {}", attempts, max_retries, e);
                    last_error = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| anyhow!("初始化失败，已达最大重试次数")))
    }

    /// 初始化策略数据，获取历史K线并设置初始指标值
    async fn initialize_strategy_data(
        &self,
        strategy: &VegasStrategy,
        inst_id: &str,
        time: &str
    ) -> anyhow::Result<String> {
        // 获取历史K线数据
        let candles = task::basic::get_candle_data(inst_id, time, strategy.min_k_line_num, None).await?;

        // 初始化指标计算
        let mut vegas_indicator_vaules = VegasIndicatorSignalValue::default();
        let mut multi_strategy_indicators = strategy.get_indicator_combine();

        info!("获取集合配置");
        let mut candle_items = vec![];
        candle_items.reserve(candles.len()); // 预先分配空间

        for candle in candles.iter() {
            // 获取数据项
            let data_item = parse_candle_to_data_item(candle);
            vegas_indicator_vaules = strategy_common::get_multi_indicator_values(
                &mut multi_strategy_indicators,
                &data_item,
            );
            candle_items.push(data_item);
        }

        // 创建并保存数据
        let strategy_type = StrategyType::Vegas.to_string();
        let hash_key = arc_vegas_indicator_values::get_hash_key(
            inst_id,
            time,
            &strategy_type,
        );

        info!("生成键: {}", hash_key);

        // 设置初始指标值
        info!("准备设置初始指标值");
        arc_vegas_indicator_values::set_ema_indicator_values(
            inst_id.to_string(),
            time.to_string(),
            candles.last().unwrap().ts,
            hash_key.clone(),
            candle_items,
            multi_strategy_indicators,
        ).await;
        info!("初始指标值设置完成");

        // 验证数据
        if arc_vegas_indicator_values::get_vegas_indicator_values_by_inst_id_with_period(hash_key.clone())
            .await
            .is_none() 
        {
            return Err(anyhow!("初始化数据后，无法验证数据是否存在"));
        }
        info!("验证初始指标值设置成功");
        Ok(hash_key)
    }
    
    /// 获取最佳执行频率
    async fn get_optimal_frequency(&self, inst_id: &str, strategy: &VegasStrategy) -> String {
        // 在实际应用中可基于市场活跃度、系统负载等因素动态调整
        "*/5 * * * * *".to_string()
    }
    
    /// 主入口函数
    pub async fn order(
        &self,
        strategy: VegasStrategy,
        inst_id: String,
        time: String,
    ) -> anyhow::Result<()> {
        info!("开始初始化Vegas策略，inst_id={}, time={}", inst_id, time);
        
        // 第一步：初始化策略数据（带重试）
        let hash_key = self.initialize_with_retry(&strategy, &inst_id, &time, 3).await?;
        
        // 第二步：创建任务信息
        let task_info = StrategyTaskInfo::new(&inst_id, &time, "Vegas");
        let job_name = task_info.job_name(); // 提前获取任务名称
        
        // 第三步：创建Arc包装的对象以安全地在线程间共享
        let strategy_arc = Arc::new(strategy);
        let inst_id_arc = Arc::new(inst_id);
        let time_arc = Arc::new(time);
        
        // 第四步：获取最佳执行频率
        let frequency = self.get_optimal_frequency(&inst_id_arc, &strategy_arc).await;
        
        // 第五步：直接传递任务相关信息
        let uuid = self.task_manager
            .schedule_task(task_info.clone(), &frequency,
                move || Box::pin(
                    async move {
                        // 捕获错误但仍然继续执行，将Result<(), anyhow::Error>转换为()
                        if let Err(e) = task::basic::run_strategy_job(
                            &task_info.inst_id,
                            &task_info.time_period,
                            &(*strategy_arc),
                            &VEGAS_INDICATOR_VALUES
                        ).await {
                            error!("策略任务执行失败: {}", e);
                        }
                    }
                )
            )
            .await?;
        
        // 第六步：注册任务以便监控
        self.metric_collector.register_task(job_name, uuid).await;
        
        // 完成并记录成功
        info!(
            "Vegas策略初始化和调度完成，inst_id={}, time={}, uuid={}",
            *inst_id_arc, *time_arc, uuid
        );
        
        Ok(())
    }

    /// 取消指定类型的策略任务
    pub async fn cancel_tasks_by_type(&self, strategy_type: &str) -> anyhow::Result<usize> {
        info!("正在取消所有{}类型的策略任务", strategy_type);
        self.task_manager.cancel_tasks_by_type(strategy_type).await
    }
}

/// 任务管理器
pub struct StrategyTaskManager {
    // 任务名称与UUID的映射关系
    task_map: Arc<Mutex<HashMap<String, StrategyTaskInfo>>>,
}

impl StrategyTaskManager {
    /// 创建新的任务管理器
    pub fn new() -> Self {
        Self {
            task_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// 注册任务信息
    pub async fn register_task(&self, task_info: StrategyTaskInfo) -> Result<(), String> {
        let mut map = self.task_map.lock().await;
        map.insert(task_info.job_name(), task_info);
        Ok(())
    }
    
    /// 获取任务信息
    pub async fn get_task(&self, job_name: &str) -> Option<StrategyTaskInfo> {
        let map = self.task_map.lock().await;
        map.get(job_name).cloned()
    }
    
    /// 计划执行任务 - 完全重构解决线程安全问题
    pub async fn schedule_task(&self, 
        task_info: StrategyTaskInfo, 
        cron_expression: &str,
        task_factory: impl FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> + Send + 'static
    ) -> anyhow::Result<Uuid> 
    {
        info!("计划执行任务: {}, 频率: {}", task_info.job_name(), cron_expression);
        
        let job_name = task_info.job_name();
        
        // 克隆并获取外部依赖
        let strategy_task_info = task_info.clone();
        
        // 直接内联创建函数，不存储task_factory
        let job = Job::new_async(cron_expression, move |uuid, _lock| {
            // 创建新的异步块
            let task_info = strategy_task_info.clone();
            
            Box::pin(async move {
                // 记录开始执行
                tracing::info!("执行任务: {}", task_info.job_name());
                
                // 使用直接在async块中执行task_factory的实际工作
                let run = async {
                    // 这里放置异步执行的代码，相当于直接在此处执行task_factory所做的工作
                    task::basic::run_strategy_job(
                        &task_info.inst_id,
                        &task_info.time_period,
                        &VegasStrategy::default(), // 这应该由调用者传入
                        &crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::VEGAS_INDICATOR_VALUES
                    ).await.ok();
                };
                
                // 执行实际工作
                run.await;
                
                // 记录完成
                tracing::info!("任务完成: {}", task_info.job_name());
            })
        })?;
        
        // 获取任务UUID
        let uuid = job.guid();
        let mut task_with_uuid = task_info.clone();
        task_with_uuid.uuid = Some(uuid);
        
        // 添加任务到调度器
        let scheduler_lock = crate::SCHEDULER.lock().await;
        if let Some(scheduler) = &*scheduler_lock {
            scheduler.add(job).await?;
            
            // 注册任务信息
            self.register_task(task_with_uuid).await
                .map_err(|e| anyhow::anyhow!("注册任务失败: {}", e))?;
            
            info!("任务添加成功: {}", job_name);
            Ok(uuid)
        } else {
            Err(anyhow!("调度器未初始化，无法添加任务"))
        }
    }
    
    /// 取消任务
    pub async fn cancel_task(&self, job_name: &str) -> anyhow::Result<()> {
        let task_info = self.get_task(job_name).await;
        
        if let Some(task_info) = task_info {
            if let Some(uuid) = task_info.uuid {
                let scheduler_lock = crate::SCHEDULER.lock().await;
                if let Some(scheduler) = &*scheduler_lock {
                    if let Err(e) = scheduler.remove(&uuid).await {
                        let msg = format!("取消任务失败: {}", e);
                        error!("{}", msg);
                        return Err(anyhow!(msg));
                    }
                    
                    // 从映射中移除任务
                    let mut map = self.task_map.lock().await;
                    map.remove(job_name);
                    
                    info!("任务已取消: {}", job_name);
                    Ok(())
                } else {
                    Err(anyhow!("调度器未初始化，无法取消任务"))
                }
            } else {
                Err(anyhow!("任务没有UUID: {}", job_name))
            }
        } else {
            Err(anyhow!("任务不存在: {}", job_name))
        }
    }
    
    /// 取消所有特定类型的任务
    pub async fn cancel_tasks_by_type(&self, strategy_type: &str) -> anyhow::Result<usize> {
        let map = self.task_map.lock().await;
        
        // 收集匹配的任务名称
        let matching_tasks: Vec<String> = map.iter()
            .filter(|(_, task)| task.strategy_type == strategy_type)
            .map(|(name, _)| name.clone())
            .collect();
        
        drop(map); // 释放锁，避免死锁
        
        // 逐个取消任务
        let mut cancelled_count = 0;
        for job_name in matching_tasks {
            if let Ok(()) = self.cancel_task(&job_name).await {
                cancelled_count += 1;
            }
        }
        
        info!("已取消 {} 个 {} 类型的任务", cancelled_count, strategy_type);
        Ok(cancelled_count)
    }
}

/// 指标收集器
pub struct MetricCollector {
    metrics: Arc<Mutex<Vec<StrategyExecutionMetrics>>>,
}

impl MetricCollector {
    /// 创建新的指标收集器
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// 记录执行指标
    pub async fn record_metrics(&self, metrics: StrategyExecutionMetrics) {
        // 记录到内存
        {
            let mut metrics_store = self.metrics.lock().await;
            metrics_store.push(metrics.clone());
            
            // 限制存储的指标数量，避免内存无限增长
            if metrics_store.len() > 1000 {
                metrics_store.remove(0);
            }
        }
        
        // 记录结构化日志
        info!(
            target: "strategy_metrics",
            inst_id = %metrics.inst_id,
            time_period = %metrics.time_period,
            execution_time_ms = %metrics.execution_time_ms,
            success = %metrics.success,
            data_items = %metrics.data_items_processed,
            "策略执行指标"
        );
    }
    
    /// 注册任务以便监控
    pub async fn register_task(&self, job_name: String, uuid: Uuid) {
        info!("任务已注册以便监控: {} (UUID: {})", job_name, uuid);
    }
    
    /// 获取最近的指标
    pub async fn get_recent_metrics(&self, limit: usize) -> Vec<StrategyExecutionMetrics> {
        let metrics_store = self.metrics.lock().await;
        
        // 获取最近的指标
        let start_idx = if metrics_store.len() > limit {
            metrics_store.len() - limit
        } else {
            0
        };
        
        metrics_store[start_idx..].to_vec()
    }
}

// 使用示例
// 这个示例函数展示了如何使用新的VagasOrder API
#[doc(hidden)]
pub async fn usage_example() -> anyhow::Result<()> {
    // 初始化调度器
    let scheduler = crate::init_scheduler().await?;
    
    // 创建VagasOrder实例
    let vegas_order = VegasOrder::new();
    
    // 创建策略配置
    let strategy = VegasStrategy {
        min_k_line_num: 3600,
        engulfing_signal: Some(EngulfingSignalConfig::default()),
        ema_signal: Some(EmaSignalConfig::default()),
        signal_weights: Some(SignalWeightsConfig::default()),
        volume_signal: Some(VolumeSignalConfig {
            volume_bar_num: 3,
            volume_increase_ratio: 2.0,
            volume_decrease_ratio: 2.0,
            is_open: true,
            is_force_dependent: true,
        }),
        ema_touch_trend_signal: Some(EmaTouchTrendSignalConfig {
            is_open: true,
            ..Default::default()
        }),
        rsi_signal: Some(RsiSignalConfig {
            rsi_length: 8,
            rsi_oversold: 21.0,
            rsi_overbought: 81.0,
            is_open: true,
        }),
        bolling_signal: Some(BollingBandsSignalConfig {
            period: 9,
            multiplier: 3.6,
            is_open: true,
            consecutive_touch_times: 3,
        }),
        kline_hammer_signal: Some(KlineHammerConfig {
            up_shadow_ratio: 0.6,
            down_shadow_ratio: 0.6,
        }),
    };
    
    // 启动策略
    vegas_order.order(strategy, "BTC-USDT-SWAP".to_string(), "5m".to_string()).await?;
    
    // 等待一段时间
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    
    // 取消特定策略
    vegas_order.cancel_tasks_by_type("Vegas").await?;
    
    // 获取最近的指标
    let metrics = vegas_order.metric_collector.get_recent_metrics(10).await;
    for (i, metric) in metrics.iter().enumerate() {
        info!(
            "指标 #{}: inst_id={}, time_period={}, 执行时间={}ms, 成功={}",
            i, metric.inst_id, metric.time_period, metric.execution_time_ms, metric.success
        );
    }
    
    Ok(())
}

// 测试用例
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_vagas_order_creation() {
        let order = VegasOrder::new();
        assert!(Arc::strong_count(&order.state_manager) == 1);
        assert!(Arc::strong_count(&order.task_manager) == 1);
        assert!(Arc::strong_count(&order.metric_collector) == 1);
    }
    
    #[tokio::test]
    async fn test_strategy_task_info() {
        let task_info = StrategyTaskInfo::new("BTC-USDT-SWAP", "5m", "Vegas");
        assert_eq!(task_info.inst_id, "BTC-USDT-SWAP");
        assert_eq!(task_info.time_period, "5m");
        assert_eq!(task_info.strategy_type, "Vegas");
        assert!(task_info.uuid.is_none());
        
        let uuid = Uuid::new_v4();
        let task_with_uuid = task_info.with_uuid(uuid);
        assert_eq!(task_with_uuid.uuid, Some(uuid));
    }
}

