//! 策略数据服务模块
//!
//! 负责策略数据的初始化、验证和管理，
//! 与策略生命周期管理解耦，提供独立的数据服务。

use std::collections::VecDeque;
use anyhow::{anyhow, Result};
use tracing::{debug, info};

use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::indicator::vegas_indicator::VegasStrategy;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::{parse_candle_to_data_item, BasicRiskStrategyConfig};
use crate::trading::strategy::{strategy_common, StrategyType};
use crate::CandleItem;
use okx::dto::EnumToStrTrait;

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
    const MAX_CANDLE_HISTORY: usize = 7000;
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

    /// 初始化策略数据并确保全局状态同步
    pub async fn initialize_strategy_data(
        strategy: &StrategyConfig,
        inst_id: &str,
        time: &str,
    ) -> Result<StrategyDataSnapshot, StrategyDataError> {
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

        // 初始化指标计算
        // 解析策略配置
        let vegas_strategy: crate::trading::indicator::vegas_indicator::VegasStrategy = 
            serde_json::from_str(&strategy.strategy_config)
                .map_err(|e| StrategyDataError::DataValidationFailed {
                    reason: format!("解析策略配置失败: {}", e)
                })?;
        let mut multi_strategy_indicators = vegas_strategy.get_indicator_combine();
        let mut candle_items = VecDeque::with_capacity(candles.len());

        // 计算所有指标值
        for candle in &candles {
            let data_item = parse_candle_to_data_item(candle);
            strategy_common::get_multi_indicator_values(&mut multi_strategy_indicators, &data_item);
            candle_items.push_back(data_item);
        }

        // 验证数据完整性
        if candle_items.is_empty() {
            return Err(StrategyDataError::DataInitializationFailed {
                reason: "K线数据转换失败".to_string(),
            });
        }

        // 生成存储键并保存数据
        let hash_key = arc_vegas_indicator_values::get_hash_key(inst_id, time, StrategyType::Vegas.as_str());

        // 保存到全局存储
        let last_timestamp = candles
            .last()
            .ok_or_else(|| StrategyDataError::DataInitializationFailed {
                reason: "无法获取最新K线时间戳".to_string(),
            })?
            .ts;

        arc_vegas_indicator_values::set_strategy_indicator_values(
            inst_id.to_string(),
            time.to_string(),
            last_timestamp,
            hash_key.clone(),
            candle_items.clone(),
            multi_strategy_indicators.clone(),
        )
        .await;

        // 验证数据保存成功
        Self::validate_data_storage(&hash_key).await?;

        let snapshot = StrategyDataSnapshot {
            hash_key: hash_key.clone(),
            candle_items,
            indicator_values: multi_strategy_indicators,
            last_timestamp,
        };

        info!("策略数据初始化完成: {}", hash_key);
        Ok(snapshot)
    }

    /// 验证数据存储是否成功
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
    use crate::trading::indicator::vegas_indicator::VegasStrategy;

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
