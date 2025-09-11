use anyhow::{anyhow, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::trading::indicator::vegas_indicator::VegasStrategy;
use crate::trading::model::strategy::strategy_config::{
    StrategyConfigEntity, StrategyConfigEntityModel,
};
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use crate::trading::strategy::order::vagas_order::{StrategyConfig, StrategyOrder};
use crate::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use crate::trading::strategy::StrategyType;
use crate::SCHEDULER;
use okx::dto::EnumToStrTrait;

/// 策略运行状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyStatus {
    /// 运行中
    Running,
    /// 已停止
    Stopped,
    /// 暂停中
    Paused,
    /// 更新中
    Updating,
    /// 错误状态
    Error(String),
}

/// 策略运行时信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRuntimeInfo {
    /// 策略配置ID
    pub strategy_config_id: i64,
    /// 产品ID
    pub inst_id: String,
    /// 时间周期
    pub period: String,
    /// 策略类型
    pub strategy_type: String,
    /// 运行状态
    pub status: StrategyStatus,
    /// 任务UUID (序列化为字符串)
    #[serde(with = "uuid_serde")]
    pub job_uuid: Option<Uuid>,
    /// 启动时间
    pub start_time: i64,
    /// 最后更新时间
    pub last_update_time: i64,
    /// 当前配置快照
    pub current_config: StrategyConfigSnapshot,
}

// UUID 序列化辅助模块
mod uuid_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use uuid::Uuid;

    pub fn serialize<S>(uuid: &Option<Uuid>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match uuid {
            Some(u) => u.to_string().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => Uuid::parse_str(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// 策略配置快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfigSnapshot {
    /// Vegas策略配置
    pub strategy_config: String,
    /// 风险配置
    pub risk_config: String,
}

/// 策略管理器 - 单例模式
pub struct StrategyManager {
    /// 运行中的策略信息: strategy_key -> RuntimeInfo
    running_strategies: Arc<DashMap<String, StrategyRuntimeInfo>>,
    /// 策略订单管理器
    strategy_order: Arc<StrategyOrder>,
    /// 策略配置缓存: strategy_config_id -> StrategyConfig
    config_cache: Arc<RwLock<HashMap<i64, Arc<StrategyConfig>>>>,
}

impl StrategyManager {
    /// 创建策略管理器实例
    pub fn new() -> Self {
        Self {
            running_strategies: Arc::new(DashMap::new()),
            strategy_order: Arc::new(StrategyOrder::new()),
            config_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 构建策略唯一标识
    fn build_strategy_key(inst_id: &str, period: &str, strategy_type: &str) -> String {
        format!("{}_{}_{}", strategy_type, inst_id, period)
    }

    /// 启动策略
    pub async fn start_strategy(
        &self,
        strategy_config_id: i64,
        inst_id: String,
        period: String,
    ) -> Result<()> {
        info!(
            "启动策略: config_id={}, inst_id={}, period={}",
            strategy_config_id, inst_id, period
        );

        // 1. 从数据库加载策略配置
        let config_model = StrategyConfigEntityModel::new().await;
        let configs = config_model
            .get_config_by_id(strategy_config_id)
            .await
            .map_err(|e| anyhow!("获取策略配置失败: {}", e))?;

        if configs.is_empty() {
            return Err(anyhow!("策略配置不存在: {}", strategy_config_id));
        }

        let config_entity = &configs[0];

        // 2. 解析策略配置
        let vegas_strategy: VegasStrategy = serde_json::from_str(&config_entity.value)
            .map_err(|e| anyhow!("解析VegasStrategy失败: {}", e))?;

        let risk_config: BasicRiskStrategyConfig = serde_json::from_str(&config_entity.risk_config)
            .map_err(|e| anyhow!("解析风险配置失败: {}", e))?;

        let strategy_config = Arc::new(StrategyConfig {
            strategy_config_id,
            strategy_config: vegas_strategy,
            risk_config,
        });

        // 3. 检查策略是否已在运行
        let strategy_key =
            Self::build_strategy_key(&inst_id, &period, &config_entity.strategy_type);
        if self.running_strategies.contains_key(&strategy_key) {
            return Err(anyhow!("策略已在运行: {}", strategy_key));
        }

        // 4. 启动策略
        self.strategy_order
            .run_strategy(
                StrategyConfig {
                    strategy_config_id: strategy_config.strategy_config_id,
                    strategy_config: strategy_config.strategy_config.clone(),
                    risk_config: strategy_config.risk_config.clone(),
                },
                inst_id.clone(),
                period.clone(),
            )
            .await?;

        // 5. 记录运行信息
        let runtime_info = StrategyRuntimeInfo {
            strategy_config_id,
            inst_id: inst_id.clone(),
            period: period.clone(),
            strategy_type: config_entity.strategy_type.clone(),
            status: StrategyStatus::Running,
            job_uuid: None, // TODO: 从strategy_order获取
            start_time: chrono::Utc::now().timestamp_millis(),
            last_update_time: chrono::Utc::now().timestamp_millis(),
            current_config: StrategyConfigSnapshot {
                strategy_config: config_entity.value.clone(),
                risk_config: config_entity.risk_config.clone(),
            },
        };

        self.running_strategies
            .insert(strategy_key.clone(), runtime_info);

        // 6. 缓存配置
        let mut cache = self.config_cache.write().await;
        cache.insert(strategy_config_id, strategy_config);

        info!("策略启动成功: {}", strategy_key);
        Ok(())
    }

    /// 停止策略
    pub async fn stop_strategy(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        info!("停止策略: {}", strategy_key);

        // 1. 检查策略是否在运行
        if !self.running_strategies.contains_key(&strategy_key) {
            return Err(anyhow!("策略未运行: {}", strategy_key));
        }

        // 2. 停止策略任务
        self.strategy_order.stop_strategy(inst_id, period).await?;

        // 3. 更新状态
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Stopped;
            entry.last_update_time = chrono::Utc::now().timestamp_millis();
        }

        // 4. 移除运行记录
        self.running_strategies.remove(&strategy_key);

        info!("策略已停止: {}", strategy_key);
        Ok(())
    }

    /// 更新运行中的策略配置
    pub async fn update_strategy_config(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
        new_config: UpdateStrategyConfigRequest,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        info!("更新策略配置: {}", strategy_key);

        // 1. 检查策略是否在运行
        let runtime_info = self
            .running_strategies
            .get(&strategy_key)
            .ok_or_else(|| anyhow!("策略未运行: {}", strategy_key))?;

        let strategy_config_id = runtime_info.strategy_config_id;
        drop(runtime_info); // 释放锁

        // 2. 标记为更新中
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Updating;
        }

        // 3. 停止当前策略
        if let Err(e) = self.strategy_order.stop_strategy(inst_id, period).await {
            // 恢复运行状态
            if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
                entry.status = StrategyStatus::Running;
            }
            return Err(anyhow!("停止策略失败: {}", e));
        }

        // 4. 更新数据库配置
        if let Some(ref strategy_config) = new_config.strategy_config {
            // TODO: 更新数据库中的策略配置
            let config_model = StrategyConfigEntityModel::new().await;
            config_model
                .update_strategy_config(strategy_config_id, strategy_config)
                .await?;
        }

        if let Some(ref risk_config) = new_config.risk_config {
            // TODO: 更新数据库中的风险配置
            let config_model = StrategyConfigEntityModel::new().await;
            config_model
                .update_risk_config(strategy_config_id, risk_config)
                .await?;
        }

        // 5. 重新加载配置并启动策略
        let config_model = StrategyConfigEntityModel::new().await;
        let configs = config_model.get_config_by_id(strategy_config_id).await?;

        if configs.is_empty() {
            return Err(anyhow!("策略配置不存在"));
        }

        let config_entity = &configs[0];
        let vegas_strategy: VegasStrategy = serde_json::from_str(&config_entity.value)?;
        let risk_config: BasicRiskStrategyConfig =
            serde_json::from_str(&config_entity.risk_config)?;

        let strategy_config = StrategyConfig {
            strategy_config_id,
            strategy_config: vegas_strategy,
            risk_config,
        };

        // 6. 重新启动策略
        self.strategy_order
            .run_strategy(strategy_config, inst_id.to_string(), period.to_string())
            .await?;

        // 7. 更新运行信息
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Running;
            entry.last_update_time = chrono::Utc::now().timestamp_millis();
            entry.current_config = StrategyConfigSnapshot {
                strategy_config: config_entity.value.clone(),
                risk_config: config_entity.risk_config.clone(),
            };
        }

        // 8. 更新缓存
        let mut cache = self.config_cache.write().await;
        cache.insert(
            strategy_config_id,
            Arc::new(StrategyConfig {
                strategy_config_id,
                strategy_config: serde_json::from_str(&config_entity.value)?,
                risk_config: serde_json::from_str(&config_entity.risk_config)?,
            }),
        );

        info!("策略配置更新成功: {}", strategy_key);
        Ok(())
    }

    /// 暂停策略
    pub async fn pause_strategy(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);

        // 1. 停止策略任务
        self.strategy_order.stop_strategy(inst_id, period).await?;

        // 2. 更新状态为暂停
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Paused;
            entry.last_update_time = chrono::Utc::now().timestamp_millis();
        }

        info!("策略已暂停: {}", strategy_key);
        Ok(())
    }

    /// 恢复策略
    pub async fn resume_strategy(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);

        // 1. 检查策略是否处于暂停状态
        let runtime_info = self
            .running_strategies
            .get(&strategy_key)
            .ok_or_else(|| anyhow!("策略不存在: {}", strategy_key))?;

        if !matches!(runtime_info.status, StrategyStatus::Paused) {
            return Err(anyhow!("策略未处于暂停状态"));
        }

        let strategy_config_id = runtime_info.strategy_config_id;
        drop(runtime_info);

        // 2. 从缓存获取配置
        let cache = self.config_cache.read().await;
        let strategy_config = cache
            .get(&strategy_config_id)
            .ok_or_else(|| anyhow!("策略配置未缓存"))?
            .clone();
        drop(cache);

        // 3. 重新启动策略
        self.strategy_order
            .run_strategy(
                StrategyConfig {
                    strategy_config_id: strategy_config.strategy_config_id,
                    strategy_config: strategy_config.strategy_config.clone(),
                    risk_config: strategy_config.risk_config.clone(),
                },
                inst_id.to_string(),
                period.to_string(),
            )
            .await?;

        // 4. 更新状态
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Running;
            entry.last_update_time = chrono::Utc::now().timestamp_millis();
        }

        info!("策略已恢复: {}", strategy_key);
        Ok(())
    }

    /// 获取所有运行中的策略
    pub async fn get_running_strategies(&self) -> Vec<StrategyRuntimeInfo> {
        self.running_strategies
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// 获取指定策略的运行信息
    pub async fn get_strategy_info(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Option<StrategyRuntimeInfo> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        self.running_strategies
            .get(&strategy_key)
            .map(|entry| entry.value().clone())
    }

    /// 批量启动策略
    pub async fn batch_start_strategies(
        &self,
        strategy_configs: Vec<(i64, String, String)>, // (config_id, inst_id, period)
    ) -> Result<BatchOperationResult> {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for (config_id, inst_id, period) in strategy_configs {
            match self
                .start_strategy(config_id, inst_id.clone(), period.clone())
                .await
            {
                Ok(_) => success.push(format!("{}_{}", inst_id, period)),
                Err(e) => failed.push(format!("{}_{}: {}", inst_id, period, e)),
            }
        }

        Ok(BatchOperationResult { success, failed })
    }

    /// 批量停止策略
    pub async fn batch_stop_strategies(
        &self,
        strategies: Vec<(String, String, String)>, // (inst_id, period, strategy_type)
    ) -> Result<BatchOperationResult> {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for (inst_id, period, strategy_type) in strategies {
            let key = Self::build_strategy_key(&inst_id, &period, &strategy_type);
            match self.stop_strategy(&inst_id, &period, &strategy_type).await {
                Ok(_) => success.push(key),
                Err(e) => failed.push(format!("{}: {}", key, e)),
            }
        }

        Ok(BatchOperationResult { success, failed })
    }

    /// 停止所有策略
    pub async fn stop_all_strategies(&self) -> Result<usize> {
        let strategies: Vec<_> = self
            .running_strategies
            .iter()
            .map(|entry| {
                let info = entry.value();
                (
                    info.inst_id.clone(),
                    info.period.clone(),
                    info.strategy_type.clone(),
                )
            })
            .collect();

        let count = strategies.len();
        for (inst_id, period, strategy_type) in strategies {
            if let Err(e) = self.stop_strategy(&inst_id, &period, &strategy_type).await {
                error!("停止策略失败: {}", e);
            }
        }

        Ok(count)
    }
}

/// 更新策略配置请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStrategyConfigRequest {
    /// 新的策略配置（可选）
    pub strategy_config: Option<String>,
    /// 新的风险配置（可选）
    pub risk_config: Option<String>,
}

/// 批量操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    /// 成功的策略列表
    pub success: Vec<String>,
    /// 失败的策略列表及原因
    pub failed: Vec<String>,
}

// 单例实例
static STRATEGY_MANAGER: once_cell::sync::OnceCell<Arc<StrategyManager>> =
    once_cell::sync::OnceCell::new();

/// 获取策略管理器单例
pub fn get_strategy_manager() -> Arc<StrategyManager> {
    STRATEGY_MANAGER
        .get_or_init(|| Arc::new(StrategyManager::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strategy_manager_creation() {
        let manager = StrategyManager::new();
        let strategies = manager.get_running_strategies().await;
        assert_eq!(strategies.len(), 0);
    }

    #[tokio::test]
    async fn test_strategy_key_generation() {
        let key = StrategyManager::build_strategy_key("BTC-USDT-SWAP", "1H", "Vegas");
        assert_eq!(key, "Vegas_BTC-USDT-SWAP_1H");
    }
}
