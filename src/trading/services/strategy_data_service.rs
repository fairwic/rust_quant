//! 策略数据服务模块
//!
//! 负责策略数据的初始化、验证和管理，
//! 与策略生命周期管理解耦，提供独立的数据服务。

use std::collections::VecDeque;
use anyhow::{anyhow, Result};
use tracing::{debug, info};

use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::parse_candle_to_data_item;
use crate::CandleItem;

// 保留用于向后兼容（仅用于 validate_data_storage）
use crate::trading::indicator::vegas_indicator::IndicatorCombine;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;

/// 策略数据服务错误类型
#[derive(thiserror::Error, Debug)]
pub enum StrategyDataError {
    #[error("数据获取失败: {reason}")]
    DataFetchFailed { reason: String },

    #[error("数据验证失败: {reason}")]
    DataValidationFailed { reason: String },

    #[error("数据初始化失败: {reason}")]
    DataInitializationFailed { reason: String },

    #[error("参数验证失败: {field}")]
    ValidationError { field: String },
}

/// 策略数据快照
#[derive(Debug, Clone)]
pub struct StrategyDataSnapshot {
    pub hash_key: String,
    pub candle_items: VecDeque<CandleItem>,
    pub indicator_values: crate::trading::indicator::vegas_indicator::IndicatorCombine,
    pub last_timestamp: i64,
}

/// 策略数据服务
pub struct StrategyDataService;

impl StrategyDataService {
    /// 常量定义
    const MAX_CANDLE_HISTORY: usize = 4000;
    const DATA_FETCH_TIMEOUT_SECS: u64 = 30;

    /// 验证策略参数
    pub fn validate_strategy_params(
        strategy: &StrategyConfig,
        inst_id: &str,
        time: &str,
    ) -> Result<(), StrategyDataError> {
        if strategy.strategy_config_id <= 0 {
            return Err(StrategyDataError::ValidationError {
                field: "strategy_config_id 必须大于0".to_string(),
            });
        }
        if inst_id.trim().is_empty() {
            return Err(StrategyDataError::ValidationError {
                field: "inst_id 不能为空".to_string(),
            });
        }
        if time.trim().is_empty() {
            return Err(StrategyDataError::ValidationError {
                field: "time 不能为空".to_string(),
            });
        }
        Ok(())
    }

    /// 初始化策略数据并确保全局状态同步 - 使用策略注册中心（重构版）✨
    /// 
    /// 新增策略时，只需在 strategy_registry.rs 中注册即可，无需修改此函数！
    pub async fn initialize_strategy_data(
        strategy: &StrategyConfig,
        inst_id: &str,
        time: &str,
    ) -> Result<StrategyDataSnapshot, StrategyDataError> {
        use crate::trading::strategy::strategy_registry::get_strategy_registry;
        
        debug!("开始初始化策略数据: {}_{}", inst_id, time);

        // 参数验证
        Self::validate_strategy_params(strategy, inst_id, time)?;

        // 获取K线数据，带超时控制
        let candles = tokio::time::timeout(
            std::time::Duration::from_secs(Self::DATA_FETCH_TIMEOUT_SECS),
            CandleDomainService::new_default()
                .await
                .get_candle_data_confirm(inst_id, time, Self::MAX_CANDLE_HISTORY, None),
        )
        .await
        .map_err(|_| StrategyDataError::DataFetchFailed {
            reason: "获取K线数据超时".to_string(),
        })?
        .map_err(|e| StrategyDataError::DataFetchFailed {
            reason: format!("获取K线数据失败: {}", e),
        })?;

        if candles.is_empty() {
            return Err(StrategyDataError::DataInitializationFailed {
                reason: "未获取到K线数据".to_string(),
            });
        }

        // 1. 从注册中心获取策略（自动检测类型）
        let strategy_executor = get_strategy_registry()
            .detect_strategy(&strategy.strategy_config)
            .map_err(|e| StrategyDataError::ValidationError {
                field: format!("策略类型识别失败: {}", e),
            })?;

        info!(
            "🎯 初始化策略: {} (inst_id={}, period={}, candles={})",
            strategy_executor.name(),
            inst_id,
            time,
            candles.len()
        );

        // 2. 初始化数据（无需 match，无需新增代码）
        let result = strategy_executor
            .initialize_data(strategy, inst_id, time, candles.clone())
            .await
            .map_err(|e| StrategyDataError::DataInitializationFailed {
                reason: format!("策略数据初始化失败: {}", e),
            })?;

        // 3. 转换K线数据用于快照
        let mut candle_items = VecDeque::with_capacity(candles.len());
        for candle in &candles {
            candle_items.push_back(parse_candle_to_data_item(candle));
        }

        // 4. 返回快照
        Ok(StrategyDataSnapshot {
            hash_key: result.hash_key,
            candle_items,
            indicator_values: Default::default(), // 使用默认值，实际数据在各自的缓存中
            last_timestamp: result.last_timestamp,
        })
    }

    /// 验证数据存储是否成功（仅用于 Vegas 策略）
    async fn validate_data_storage(hash_key: &str) -> Result<(), StrategyDataError> {
        // 验证数据是否保存成功
        if arc_vegas_indicator_values::get_vegas_indicator_values_by_inst_id_with_period(hash_key.to_string())
            .await
            .is_none()
        {
            return Err(StrategyDataError::DataValidationFailed {
                reason: "数据保存验证失败".to_string(),
            });
        }

        // 验证数据是否在新管理器中存在
        let manager = arc_vegas_indicator_values::get_indicator_manager();
        if !manager.key_exists(hash_key).await {
            return Err(StrategyDataError::DataValidationFailed {
                reason: format!("管理器中未找到策略数据: {}", hash_key),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::{indicator::vegas_indicator::VegasStrategy, strategy::strategy_common::BasicRiskStrategyConfig};

    #[tokio::test]
    async fn test_validate_strategy_params() {
        let valid_config = StrategyConfig {
            strategy_config_id: 1,
            strategy_config: serde_json::to_string(&VegasStrategy::default()).unwrap(),
            risk_config: serde_json::to_string(&BasicRiskStrategyConfig::default()).unwrap(),
        };

        // 有效参数
        assert!(StrategyDataService::validate_strategy_params(&valid_config, "BTC-USDT-SWAP", "1H").is_ok());

        // 无效配置ID
        let invalid_config = StrategyConfig {
            strategy_config_id: 0,
            strategy_config: serde_json::to_string(&VegasStrategy::default()).unwrap(),
            risk_config: serde_json::to_string(&BasicRiskStrategyConfig::default()).unwrap(),
        };
        assert!(StrategyDataService::validate_strategy_params(&invalid_config, "BTC-USDT-SWAP", "1H").is_err());

        // 空的inst_id
        assert!(StrategyDataService::validate_strategy_params(&valid_config, "", "1H").is_err());

        // 空的时间周期
        assert!(StrategyDataService::validate_strategy_params(&valid_config, "BTC-USDT-SWAP", "").is_err());
    }
}
