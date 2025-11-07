//! Nwe 策略执行器
//! 
//! 封装 Nwe 策略的数据初始化和执行逻辑

use async_trait::async_trait;
use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use tracing::{debug, info};

use super::executor_common::{
    convert_candles_to_items, execute_order, get_latest_candle, get_recent_candles,
    should_execute_strategy, update_candle_queue, validate_candles,
};
use super::strategy_trait::{StrategyDataResult, StrategyExecutor};
use rust_quant_market::models::CandlesEntity;
use rust_quant_indicators::trend::nwe_indicator::{
    get_nwe_hash_key, get_nwe_indicator_manager,
};
// TODO: 暂时注释，等待 NweIndicatorCombine 移到 indicators 包后恢复
// use rust_quant_infrastructure::cache::arc_nwe_indicator_values;
use crate::nwe_strategy::{NweStrategy, NweStrategyConfig};
use crate::framework::config::strategy_config::StrategyConfig;
use crate::strategy_common::parse_candle_to_data_item;
use crate::StrategyType;
use rust_quant_orchestration::workflow::strategy_runner::StrategyExecutionStateManager;
use rust_quant_common::CandleItem;
use okx::dto::EnumToStrTrait;

/// Nwe 策略执行器
pub struct NweStrategyExecutor;

impl NweStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for NweStrategyExecutor {
    fn name(&self) -> &'static str {
        "Nwe"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::Nwe
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        serde_json::from_str::<NweStrategyConfig>(strategy_config).is_ok()
    }

    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandlesEntity>,
    ) -> Result<StrategyDataResult> {
        debug!("初始化 Nwe 策略数据: {}_{}", inst_id, period);

        // 1. 验证K线数据并获取时间戳
        let last_timestamp = validate_candles(&candles)?;

        // 2. 解析 Nwe 策略配置
        let nwe_config: NweStrategyConfig = serde_json::from_str(&strategy_config.strategy_config)
            .map_err(|e| anyhow!("解析 NweStrategyConfig 失败: {}", e))?;

        // 3. 转换K线数据并计算指标
        let nwe_strategy = NweStrategy::new(nwe_config);
        let mut indicator_combine = nwe_strategy.get_indicator_combine();
        let candle_items = convert_candles_to_items(&candles);
        
        for item in &candle_items {
            indicator_combine.next(item);
        }

        // 4. 生成存储键并保存数据
        let hash_key = get_nwe_hash_key(inst_id, period, StrategyType::Nwe.as_str());

        // TODO: 暂时注释，等待 NweIndicatorCombine 移到 indicators 包后恢复
        // arc_nwe_indicator_values::set_nwe_strategy_indicator_values(
        //     inst_id.to_string(),
        //     period.to_string(),
        //     last_timestamp,
        //     hash_key.clone(),
        //     candle_items,
        //     indicator_combine,
        // )
        // .await;

        // 5. 验证数据保存成功
        // TODO: 暂时注释，等待 NweIndicatorCombine 移到 indicators 包后恢复
        // let manager = get_nwe_indicator_manager();
        // if !manager.key_exists(&hash_key).await {
        //     return Err(anyhow!("Nwe 策略数据保存验证失败: {}", hash_key));
        // }

        info!("✅ Nwe 策略数据初始化完成: {}", hash_key);

        Ok(StrategyDataResult {
            hash_key,
            last_timestamp,
        })
    }

    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<()> {
        const MAX_HISTORY_SIZE: usize = 10000;

        // 1. 获取哈希键和管理器
        let key = get_nwe_hash_key(inst_id, period, StrategyType::Nwe.as_str());
        let manager = get_nwe_indicator_manager();

        // 2. 获取最新K线数据（使用公共函数）
        let new_candle_data = get_latest_candle(inst_id, period, snap).await?;
        let new_candle_item = parse_candle_to_data_item(&new_candle_data);

        // 3. 获取互斥锁和缓存快照
        let key_mutex = manager.acquire_key_mutex(&key).await;
        let _guard = key_mutex.lock().await;

        let (last_candles_vec, mut old_indicator_combines, old_time) =
            manager.get_snapshot_last_n(&key, MAX_HISTORY_SIZE).await
                .ok_or_else(|| anyhow!("没有找到对应的 Nwe 策略值: {}", key))?;

        let mut new_candle_items: VecDeque<CandleItem> = last_candles_vec.into_iter().collect();

        // 4. 检查是否应该执行（使用公共函数）
        if !should_execute_strategy(&key, old_time, new_candle_item.ts, period, new_candle_item.confirm == 1)? {
            return Ok(());
        }

        // 5. 更新指标值
        let new_indicator_values = old_indicator_combines.next(&new_candle_item);

        // 6. 更新K线队列（使用公共函数）
        update_candle_queue(&mut new_candle_items, new_candle_item.clone(), MAX_HISTORY_SIZE);

        // 7. 原子更新缓存
        manager
            .update_both(&key, new_candle_items.clone(), old_indicator_combines.clone(), new_candle_item.ts)
            .await
            .map_err(|e| anyhow!("原子更新 Nwe 指标与K线失败: {}", e))?;

        // 8. 获取最近10根K线（使用公共函数）
        let candle_vec = get_recent_candles(&new_candle_items, 10);

        // 9. 生成交易信号
        let nwe_config: NweStrategyConfig = serde_json::from_str(&strategy_config.strategy_config)?;
        let mut nwe_strategy = NweStrategy::new(nwe_config);
        let signal_result = nwe_strategy.get_trade_signal(&candle_vec, &new_indicator_values);

        // 10. 执行下单（使用公共函数）
        execute_order(&StrategyType::Nwe, inst_id, period, &signal_result, strategy_config).await?;

        // 11. 清理执行状态
        StrategyExecutionStateManager::mark_completed(&key, new_candle_item.ts);

        Ok(())
    }
}

