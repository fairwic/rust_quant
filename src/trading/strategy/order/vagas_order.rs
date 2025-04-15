use anyhow::anyhow;
use std::sync::Arc;
use tokio_cron_scheduler::{JobScheduler, Job};
use tracing::{error, info, warn};
use std::time::Duration;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use uuid::Uuid;
use tokio::sync::Mutex;

use crate::trading::indicator::vegas_indicator::{VegasIndicatorSignalValue, VegasStrategy};
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules::VEGAS_INDICATOR_VALUES;
use crate::trading::strategy::strategy_common::parse_candle_to_data_item;
use crate::trading::strategy::{strategy_common, Strategy, StrategyType};
use crate::trading::task;
use crate::SCHEDULER;
use chrono::Local;

// 存储任务名称与UUID的映射关系
static JOB_NAME_TO_UUID: Lazy<Mutex<HashMap<String, Uuid>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

pub struct VagasOrder {}

impl VagasOrder {
    pub fn new() -> Self {
        VagasOrder {}
    }
    /// 构建策略任务ID
    fn build_job_id(inst_id: &str, time: &str) -> String {
        format!("vegas_strategy_{}_{}", inst_id, time)
    }
    /// 保存任务UUID
    async fn save_job_uuid(job_name: String, uuid: Uuid) {
        let mut map = JOB_NAME_TO_UUID.lock().await;
        map.insert(job_name, uuid);
    }
    /// 获取任务UUID
    async fn get_job_uuid(job_name: &str) -> Option<Uuid> {
        let map = JOB_NAME_TO_UUID.lock().await;
        map.get(job_name).copied()
    }
    /// 停止特定策略任务
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
    
    /// 停止所有策略任务
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
    
    /// 初始化策略数据，获取历史K线并设置初始指标值
    async fn initialize_strategy_data(
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
            vegas_indicator_vaules = strategy_common::get_multi_indivator_values(
                &mut multi_strategy_indicators,
                &data_item,
            );
            candle_items.push(data_item);
        }
        
        // 创建并保存数据
        let strategy_type = StrategyType::Vegas.to_string();
        let hash_key = arc_vegas_indicator_vaules::get_hash_key(
            inst_id,
            time,
            &strategy_type,
        );
        
        info!("生成键: {}", hash_key);
        
        // 设置初始指标值
        info!("准备设置初始指标值");
        arc_vegas_indicator_vaules::set_ema_indicator_values(
            inst_id.to_string(),
            time.to_string(),
            candles.last().unwrap().ts,
            hash_key.clone(),
            candle_items,
            multi_strategy_indicators,
        ).await;
        info!("初始指标值设置完成");
        
        // 验证数据
        if arc_vegas_indicator_vaules::get_vegas_indicator_values_by_inst_id_with_period(hash_key.clone())
            .await
            .is_none() 
        {
            return Err(anyhow!("初始化数据后，无法验证数据是否存在"));
        }
        
        Ok(hash_key)
    }
    
    /// 创建并调度定时任务
    async fn schedule_strategy_task(
        strategy: Arc<VegasStrategy>,
        inst_id: Arc<String>,
        time: Arc<String>,
    ) -> anyhow::Result<()> {
        // 所有需要在闭包中使用的变量先在此处克隆
        let job_name_for_closure = Self::build_job_id(&inst_id, &time);
        let job_name_for_save = job_name_for_closure.clone();
        
        // 根据市场活跃度确定执行频率
        let cron_expression = "*/5 * * * * *";  // 活跃市场每5秒执行一次
        
        info!("为策略设置执行频率: {}", cron_expression);
        
        // 创建新任务 - 动态频率
        let job = Job::new_async(cron_expression, move |uuid, _lock| {
            // 对每个Arc进行克隆，以便在异步任务中使用
            let strategy_clone = Arc::clone(&strategy);
            let inst_id_clone = Arc::clone(&inst_id);
            let time_clone = Arc::clone(&time);
            // 提前复制job_id，避免移动错误
            let job_id_clone = job_name_for_closure.clone();
            
            Box::pin(async move {
                info!(
                    target: "strategy_execution",
                    job_id = %job_id_clone,
                    inst_id = %*inst_id_clone,
                    time = %*time_clone,
                    "开始执行策略计算任务"
                );
                let time1 = Local::now();
                // 使用克隆的Arc值
                let res = task::basic::run_strategy_job(
                    &inst_id_clone,
                    &time_clone,
                    &strategy_clone,
                    &VEGAS_INDICATOR_VALUES
                ).await;
                
                let execution_time = Local::now().signed_duration_since(time1).num_milliseconds();
                
                if let Err(error) = res {
                    error!(
                        target: "strategy_execution",
                        job_id = %job_id_clone,
                        inst_id = %*inst_id_clone,
                        time = %*time_clone,
                        error = %error,
                        execution_time_ms = %execution_time,
                        "策略执行错误"
                    );
                } else {
                    info!(
                        target: "strategy_execution",
                        job_id = %job_id_clone,
                        inst_id = %*inst_id_clone,
                        time = %*time_clone,
                        execution_time_ms = %execution_time,
                        "策略执行完成"
                    );
                }
            })
        })?;
        
        // 获取任务UUID
        let uuid = job.guid();
        
        // 添加任务到调度器
        let scheduler_lock = crate::SCHEDULER.lock().await;
        if let Some(scheduler) = &*scheduler_lock {
            scheduler.add(job).await?;
            
            // 保存任务名称与UUID的映射关系 (使用独立的变量)
            Self::save_job_uuid(job_name_for_save, uuid).await;
            
            info!("策略任务添加成功，执行频率: {}", cron_expression);
            Ok(())
        } else {
            Err(anyhow!("调度器未初始化，无法添加任务"))
        }
    }
    
    pub async fn order(
        strategy: VegasStrategy,
        inst_id: String,
        time: String,
    ) -> anyhow::Result<()> {
        info!("开始初始化Vegas策略，inst_id={}, time={}", inst_id, time);
        
        // 第一步：初始化策略数据
        Self::initialize_strategy_data(&strategy, &inst_id, &time).await?;
        
        // 第二步：创建Arc包装的对象以安全地在线程间共享
        let strategy_arc = Arc::new(strategy);
        let inst_id_arc = Arc::new(inst_id);
        let time_arc = Arc::new(time);
        
        // 第三步：创建并调度定时任务
        Self::schedule_strategy_task(strategy_arc, inst_id_arc, time_arc).await?;
        
        Ok(())
    }
}
