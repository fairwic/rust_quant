//! 策略管理器 - 简化版
//! 
//! 保留核心功能，移除对不存在模块的依赖
//! 
//! TODO: 完整版本见 strategy_manager.rs.backup

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe, StrategyStatus};
use rust_quant_infrastructure::{
    StrategyConfigEntity, StrategyConfigEntityModel,
};
use crate::nwe_strategy::NweStrategyConfig;
use crate::strategy_common::BasicRiskStrategyConfig;

/// 策略管理器错误类型
#[derive(Error, Debug)]
pub enum StrategyManagerError {
    #[error("策略配置不存在: {config_id}")]
    ConfigNotFound { config_id: i64 },

    #[error("策略已在运行: {strategy_key}")]
    StrategyAlreadyRunning { strategy_key: String },

    #[error("策略未运行: {strategy_key}")]
    StrategyNotRunning { strategy_key: String },

    #[error("配置解析失败: {field}")]
    ConfigParseError { field: String },

    #[error("参数验证失败: {field}")]
    ValidationError { field: String },
}

/// 策略运行时信息（简化版）
#[derive(Debug, Clone)]
pub struct StrategyRuntimeInfo {
    /// 策略配置ID  
    pub config_id: i64,
    /// 产品ID
    pub inst_id: String,
    /// 时间周期
    pub period: String,
    /// 策略类型
    pub strategy_type: String,
    /// 运行状态
    pub status: StrategyRunStatus,
    /// 当前配置对象
    pub current_config: Arc<RwLock<StrategyConfig>>,
}

/// 策略运行状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyRunStatus {
    Running,
    Stopped,
    Paused,
    Error(String),
}

/// 策略管理器
pub struct StrategyManager {
    /// 正在运行的策略 (key: "inst_id:period:strategy_type")
    running_strategies: Arc<DashMap<String, StrategyRuntimeInfo>>,
}

impl StrategyManager {
    /// 创建新的策略管理器
    pub fn new() -> Self {
        Self {
            running_strategies: Arc::new(DashMap::new()),
        }
    }

    /// 获取全局实例
    pub fn global() -> &'static StrategyManager {
        use once_cell::sync::OnceCell;
        static INSTANCE: OnceCell<StrategyManager> = OnceCell::new();
        INSTANCE.get_or_init(|| StrategyManager::new())
    }

    /// 构建策略键
    fn build_strategy_key(inst_id: &str, period: &str, strategy_type: &str) -> String {
        format!("{}:{}:{}", inst_id, period, strategy_type)
    }

    /// 加载策略配置
    async fn load_strategy_config(
        &self,
        strategy_config_id: i64,
    ) -> Result<(StrategyConfigEntity, Arc<StrategyConfig>)> {
        debug!("加载策略配置: config_id={}", strategy_config_id);

        // 1. 从数据库加载配置
        let config_entity = {
            let config_model = StrategyConfigEntityModel::new().await;
            let result = config_model.get_config_by_id(strategy_config_id).await?;
            result.ok_or_else(|| StrategyManagerError::ConfigNotFound {
                config_id: strategy_config_id,
            })?
        };

        // 2. 使用 to_domain() 转换
        let strategy_config = config_entity.to_domain()?;
        
        Ok((config_entity, Arc::new(strategy_config)))
    }

    /// 启动策略（简化版）
    pub async fn start_strategy(
        &self,
        strategy_config_id: i64,
        inst_id: String,
        period: String,
    ) -> Result<()> {
        // 参数验证
        if strategy_config_id <= 0 {
            return Err(anyhow!("策略配置ID必须大于0"));
        }

        info!("启动策略: config_id={}, inst_id={}, period={}", 
            strategy_config_id, inst_id, period
        );

        // 加载配置
        let (config_entity, strategy_config) =
            self.load_strategy_config(strategy_config_id).await?;

        let strategy_type_enum = StrategyType::from_str(&config_entity.strategy_type)
            .ok_or_else(|| anyhow!("未知的策略类型: {}", config_entity.strategy_type))?;

        // 注册策略
        crate::strategy_registry::register_strategy_on_demand(&strategy_type_enum);

        let strategy_key = Self::build_strategy_key(&inst_id, &period, &config_entity.strategy_type);

        // 创建运行时信息
        let runtime_info = StrategyRuntimeInfo {
            config_id: strategy_config_id,
            inst_id: inst_id.clone(),
            period: period.clone(),
            strategy_type: config_entity.strategy_type.clone(),
            status: StrategyRunStatus::Running,
            current_config: Arc::new(RwLock::new((*strategy_config).clone())),
        };

        self.running_strategies.insert(strategy_key.clone(), runtime_info);

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

        // 移除运行时信息
        if self.running_strategies.remove(&strategy_key).is_some() {
            info!("策略停止成功: {}", strategy_key);
            Ok(())
        } else {
            Err(StrategyManagerError::StrategyNotRunning { strategy_key }.into())
        }
    }

    /// 获取运行中的策略
    pub fn get_running_strategies(&self) -> Vec<String> {
        self.running_strategies
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// 检查策略是否运行中
    pub fn is_running(&self, inst_id: &str, period: &str, strategy_type: &str) -> bool {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        self.running_strategies.contains_key(&strategy_key)
    }
}

impl Default for StrategyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_strategy_key() {
        let key = StrategyManager::build_strategy_key("BTC-USDT", "1H", "vegas");
        assert_eq!(key, "BTC-USDT:1H:vegas");
    }

    #[test]
    fn test_strategy_manager_creation() {
        let manager = StrategyManager::new();
        assert_eq!(manager.get_running_strategies().len(), 0);
    }
}

