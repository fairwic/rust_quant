//! 交易所API配置服务

use anyhow::{anyhow, Result};
use redis::AsyncCommands;
use std::sync::Arc;
use tracing::{debug, info, warn};

use rust_quant_core::cache::get_redis_connection;
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_domain::traits::{ExchangeApiConfigRepository, StrategyApiConfigRepository};
use rust_quant_infrastructure::repositories::{
    SqlxExchangeApiConfigRepository, SqlxStrategyApiConfigRepository,
};

/// 交易所API配置服务
pub struct ExchangeApiService {
    #[allow(dead_code)]
    api_repo: Arc<dyn ExchangeApiConfigRepository>,
    strategy_api_repo: Arc<dyn StrategyApiConfigRepository>,
}

impl ExchangeApiService {
    pub fn new(
        api_repo: Arc<dyn ExchangeApiConfigRepository>,
        strategy_api_repo: Arc<dyn StrategyApiConfigRepository>,
    ) -> Self {
        Self {
            api_repo,
            strategy_api_repo,
        }
    }

    /// 根据策略配置ID获取关联的API配置（优先从Redis缓存）
    pub async fn get_api_configs_for_strategy(
        &self,
        strategy_config_id: i32,
    ) -> Result<Vec<ExchangeApiConfig>> {
        let cache_key = format!("strategy_api_config:{}", strategy_config_id);

        // 1. 尝试从Redis获取
        let mut redis_conn = get_redis_connection().await?;
        let cache_key_str: String = cache_key.clone();
        if let Ok(Some(cached_json)) = redis_conn
            .get::<String, Option<String>>(cache_key_str)
            .await
        {
            if let Ok(configs) = serde_json::from_str::<Vec<ExchangeApiConfig>>(&cached_json) {
                debug!(
                    "从Redis缓存获取API配置: strategy_config_id={}, count={}",
                    strategy_config_id,
                    configs.len()
                );
                return Ok(configs);
            }
        }

        // 2. 从数据库查询
        let configs = self
            .strategy_api_repo
            .find_by_strategy_config_id(strategy_config_id)
            .await?;

        // 3. 缓存到Redis（1小时过期）
        if let Ok(configs_json) = serde_json::to_string(&configs) {
            let cache_key_str: String = cache_key.clone();
            let _: () = redis_conn
                .set_ex::<String, String, ()>(cache_key_str, configs_json, 3600)
                .await
                .unwrap_or_else(|e| {
                    warn!("缓存API配置到Redis失败: {}", e);
                });
        }
        info!(
            "从数据库获取API配置: strategy_config_id={}, count={}",
            strategy_config_id,
            configs.len()
        );

        Ok(configs)
    }

    /// 获取第一个可用的API配置（按优先级）
    pub async fn get_first_api_config(&self, strategy_config_id: i32) -> Result<ExchangeApiConfig> {
        let configs = self
            .get_api_configs_for_strategy(strategy_config_id)
            .await?;

        if configs.is_empty() {
            return Err(anyhow!("策略配置 {} 未关联任何API配置", strategy_config_id));
        }
        // 按优先级排序后返回第一个
        Ok(configs[0].clone())
    }

    /// 清除策略API配置的Redis缓存
    pub async fn clear_cache(&self, strategy_config_id: i32) -> Result<()> {
        let cache_key = format!("strategy_api_config:{}", strategy_config_id);
        let mut redis_conn = get_redis_connection().await?;
        redis_conn.del::<String, ()>(cache_key.clone()).await?;
        debug!("清除API配置缓存: {}", cache_key);
        Ok(())
    }

    /// 创建策略与API配置的关联
    pub async fn associate_strategy_with_api(
        &self,
        strategy_config_id: i32,
        api_config_id: i32,
        priority: i32,
    ) -> Result<i32> {
        let id = self
            .strategy_api_repo
            .create_association(strategy_config_id, api_config_id, priority)
            .await?;

        // 清除缓存
        self.clear_cache(strategy_config_id).await?;

        info!(
            "创建策略API关联: strategy_config_id={}, api_config_id={}, priority={}",
            strategy_config_id, api_config_id, priority
        );

        Ok(id)
    }
}

/// 创建默认的ExchangeApiService实例
pub fn create_exchange_api_service() -> ExchangeApiService {
    use rust_quant_core::database::get_db_pool;
    let pool = get_db_pool();

    let api_repo: Arc<dyn ExchangeApiConfigRepository> =
        Arc::new(SqlxExchangeApiConfigRepository::new(pool.clone()));
    let strategy_api_repo: Arc<dyn StrategyApiConfigRepository> =
        Arc::new(SqlxStrategyApiConfigRepository::new(pool.clone()));

    ExchangeApiService::new(api_repo, strategy_api_repo)
}
