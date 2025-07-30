use anyhow::anyhow;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_cron_scheduler::Job;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::indicator::vegas_indicator::{VegasIndicatorSignalValue, VegasStrategy};
use crate::trading::model::entity::candles::dto::SelectCandleReqDto;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::VEGAS_INDICATOR_VALUES;
use crate::trading::strategy::strategy_common::{
    parse_candle_to_data_item, BasicRiskStrategyConfig,
};
use crate::trading::strategy::{strategy_common, StrategyType};
use crate::trading::task;
use crate::CandleItem;
use std::env;

/// 策略配置
pub struct StrategyConfig {
    pub strategy_config_id: i64,
    pub strategy_config: VegasStrategy,
    pub risk_config: BasicRiskStrategyConfig,
}

/// 策略订单管理器 - 简化版本
pub struct StrategyOrder {
    /// 活跃任务映射：task_key -> job_uuid
    active_tasks: Arc<Mutex<HashMap<String, Uuid>>>,
}

impl StrategyOrder {
    /// 创建新的StrategyOrder实例
    pub fn new() -> Self {
        Self {
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 构建策略任务唯一标识
    fn build_task_key(inst_id: &str, time: &str) -> String {
        format!("vegas_{}_{}", inst_id, time)
    }

    /// 初始化策略数据并确保全局状态同步
    async fn initialize_strategy_data(
        strategy: &StrategyConfig,
        inst_id: &str,
        time: &str,
    ) -> anyhow::Result<()> {
        info!("开始初始化策略数据: {}_{}", inst_id, time);

        let dto = SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: time.to_string(),
            limit: 1,
            select_time: None,
            confirm: None,
        };
        //获取K线数据 confirm=1
        let candles = CandleDomainService::new_default()
            .await
            .get_candle_data_confirm(inst_id, time, 1, None)
            .await?;
        if candles.is_empty() {
            return Err(anyhow!("未获取到K线数据"));
        }

        // 初始化指标计算
        let mut vegas_indicator_values = VegasIndicatorSignalValue::default();
        let mut multi_strategy_indicators = strategy.strategy_config.get_indicator_combine();
        let mut candle_items = VecDeque::with_capacity(candles.len());

        // 计算所有指标值
        for candle in &candles {
            let data_item = parse_candle_to_data_item(candle);
            vegas_indicator_values = strategy_common::get_multi_indicator_values(
                &mut multi_strategy_indicators,
                &data_item,
            );
            candle_items.push_back(data_item);
        }

        // 生成存储键并保存数据
        let strategy_type = StrategyType::Vegas.to_string();
        let hash_key = arc_vegas_indicator_values::get_hash_key(inst_id, time, &strategy_type);

        // 保存到全局存储
        arc_vegas_indicator_values::set_strategy_indicator_values(
            inst_id.to_string(),
            time.to_string(),
            candles.last().unwrap().ts,
            hash_key.clone(),
            candle_items,
            multi_strategy_indicators,
        )
        .await;

        // 等待一小段时间确保数据写入完成
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 验证数据是否保存成功
        if arc_vegas_indicator_values::get_vegas_indicator_values_by_inst_id_with_period(
            hash_key.clone(),
        )
        .await
        .is_none()
        {
            return Err(anyhow!("策略数据初始化失败，未能验证数据存在"));
        }

        // 验证数据是否在新管理器中存在
        let manager = arc_vegas_indicator_values::get_indicator_manager();
        if !manager.key_exists(&hash_key).await {
            return Err(anyhow!("管理器中未找到策略数据: {}", hash_key));
        }

        info!("策略数据初始化完成: {}", hash_key);
        Ok(())
    }

    /// 创建定时任务
    fn create_scheduled_job(
        inst_id: String,
        time: String,
        strategy: Arc<StrategyConfig>,
    ) -> anyhow::Result<Job> {
        // 根据时间周期设置不同的执行频率
        let cron_expression = match time.as_str() {
            "1m" => "0 * * * * *",     // 每分钟开始时执行
            "5m" => "0 */5 * * * *",   // 每5分钟开始时执行
            "15m" => "0 */15 * * * *", // 每15分钟开始时执行
            "1H" => "0 0 * * * *",     // 每小时开始时执行
            "4H" => "0 0 */4 * * *",   // 每4小时开始时执行
            "1Dutc" => "0 0 0 * * *",  // 每天UTC 00:00执行
            _ => "*/30 * * * * *",     // 默认每30秒执行一次
        };

        if env::var("APP_ENV").unwrap() == "local" {
            //开发环境，每10秒执行一次
            let cron_expression = "*/10 * * * * *";
        }

        let job = Job::new_async(cron_expression, move |_uuid, _lock| {
            let inst_id = inst_id.clone();
            let time = time.clone();
            let strategy = Arc::clone(&strategy);

            Box::pin(async move {
                match task::basic::run_strategy_job(
                    &inst_id,
                    &time,
                    &strategy,
                    &VEGAS_INDICATOR_VALUES,
                )
                .await
                {
                    Ok(_) => {
                        tracing::debug!("策略任务执行成功: {}_{}", inst_id, time);
                    }
                    Err(e) => {
                        tracing::error!("策略任务执行失败: {}_{}, 错误: {}", inst_id, time, e);
                    }
                }
            })
        })?;

        Ok(job)
    }

    /// 主入口函数 - 启动策略
    pub async fn run_strategy(
        &self,
        strategy: StrategyConfig,
        inst_id: String,
        time: String,
    ) -> anyhow::Result<()> {
        let task_key = Self::build_task_key(&inst_id, &time);

        // 检查是否已有相同任务在运行
        {
            let active_tasks = self.active_tasks.lock().await;
            if active_tasks.contains_key(&task_key) {
                warn!("策略任务已在运行，跳过: {}", task_key);
                return Ok(());
            }
        }

        info!("启动Vegas策略: {}_{}", inst_id, time);

        // 步骤1: 初始化策略数据（带重试机制）
        let mut attempts = 0;
        const MAX_RETRIES: usize = 3;

        while attempts < MAX_RETRIES {
            match Self::initialize_strategy_data(&strategy, &inst_id, &time).await {
                Ok(_) => break,
                Err(e) => {
                    attempts += 1;
                    if attempts >= MAX_RETRIES {
                        return Err(anyhow!(
                            "策略数据初始化失败，已重试{}次: {}",
                            MAX_RETRIES,
                            e
                        ));
                    }
                    warn!(
                        "策略数据初始化失败，重试 {}/{}: {}",
                        attempts, MAX_RETRIES, e
                    );
                }
            }
        }

        // 步骤2: 等待系统完全准备好
        info!("策略数据初始化完成，等待系统准备就绪...");
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 步骤3: 创建并调度任务
        let strategy_arc = Arc::new(strategy);
        let job = Self::create_scheduled_job(inst_id.clone(), time.clone(), strategy_arc)?;

        let job_uuid = job.guid();

        // 步骤4: 添加到调度器
        let scheduler_lock = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_lock
            .as_ref()
            .ok_or_else(|| anyhow!("调度器未初始化"))?;

        scheduler.add(job).await?;

        // 步骤5: 记录活跃任务
        {
            let mut active_tasks = self.active_tasks.lock().await;
            active_tasks.insert(task_key.clone(), job_uuid);
        }

        info!("Vegas策略启动成功: {} (UUID: {})", task_key, job_uuid);
        Ok(())
    }

    /// 停止特定策略任务
    pub async fn stop_strategy(&self, inst_id: &str, time: &str) -> anyhow::Result<()> {
        let task_key = Self::build_task_key(inst_id, time);

        let job_uuid = {
            let mut active_tasks = self.active_tasks.lock().await;
            active_tasks
                .remove(&task_key)
                .ok_or_else(|| anyhow!("任务不存在或已停止: {}", task_key))?
        };

        let scheduler_lock = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_lock
            .as_ref()
            .ok_or_else(|| anyhow!("调度器未初始化"))?;

        scheduler.remove(&job_uuid).await?;
        info!("策略任务已停止: {}", task_key);
        Ok(())
    }

    /// 停止所有策略任务
    pub async fn stop_all_strategies(&self) -> anyhow::Result<usize> {
        let task_uuids = {
            let mut active_tasks = self.active_tasks.lock().await;
            let uuids: Vec<Uuid> = active_tasks.values().copied().collect();
            active_tasks.clear();
            uuids
        };

        let scheduler_lock = crate::SCHEDULER.lock().await;
        let scheduler = scheduler_lock
            .as_ref()
            .ok_or_else(|| anyhow!("调度器未初始化"))?;

        let mut stopped_count = 0;
        for uuid in task_uuids {
            if scheduler.remove(&uuid).await.is_ok() {
                stopped_count += 1;
            }
        }

        info!("已停止 {} 个策略任务", stopped_count);
        Ok(stopped_count)
    }

    /// 获取活跃任务数量
    pub async fn get_active_task_count(&self) -> usize {
        let active_tasks = self.active_tasks.lock().await;
        active_tasks.len()
    }

    /// 获取活跃任务列表
    pub async fn get_active_tasks(&self) -> Vec<String> {
        let active_tasks = self.active_tasks.lock().await;
        active_tasks.keys().cloned().collect()
    }

    /// 检查任务是否正在运行
    pub async fn is_task_running(&self, inst_id: &str, time: &str) -> bool {
        let task_key = Self::build_task_key(inst_id, time);
        let active_tasks = self.active_tasks.lock().await;
        active_tasks.contains_key(&task_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strategy_order_creation() {
        let order = StrategyOrder::new();
        assert_eq!(order.get_active_task_count().await, 0);
    }

    #[tokio::test]
    async fn test_task_key_generation() {
        let key = StrategyOrder::build_task_key("BTC-USDT-SWAP", "5m");
        assert_eq!(key, "vegas_BTC-USDT-SWAP_5m");
    }
}
