//! 交易所API配置仓储实现

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{FromRow, MySql, Pool};
use tracing::debug;

use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_domain::traits::{ExchangeApiConfigRepository, StrategyApiConfigRepository};

/// 交易所API配置数据库实体
#[derive(Debug, Clone, FromRow)]
pub struct ExchangeApiConfigEntity {
    pub id: i32,
    pub exchange_name: String,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: Option<String>,
    pub is_sandbox: i8, // MySQL tinyint(1)
    pub is_enabled: i8, // MySQL tinyint(1)
    pub description: Option<String>,
}

impl ExchangeApiConfigEntity {
    /// 转换为领域实体
    pub fn to_domain(&self) -> ExchangeApiConfig {
        ExchangeApiConfig::new(
            self.id,
            self.exchange_name.clone(),
            self.api_key.clone(),
            self.api_secret.clone(),
            self.passphrase.clone(),
            self.is_sandbox != 0,
            self.is_enabled != 0,
            self.description.clone(),
        )
    }

    /// 从领域实体创建
    pub fn from_domain(config: &ExchangeApiConfig) -> Self {
        Self {
            id: config.id,
            exchange_name: config.exchange_name.clone(),
            api_key: config.api_key.clone(),
            api_secret: config.api_secret.clone(),
            passphrase: config.passphrase.clone(),
            is_sandbox: if config.is_sandbox { 1 } else { 0 },
            is_enabled: if config.is_enabled { 1 } else { 0 },
            description: config.description.clone(),
        }
    }
}

/// 策略与API配置关联数据库实体
#[derive(Debug, Clone, FromRow)]
pub struct StrategyApiConfigEntity {
    pub id: i32,
    pub strategy_config_id: i32,
    pub api_config_id: i32,
    pub priority: i32,
    pub is_enabled: i8,
}

/// 交易所API配置仓储实现
pub struct SqlxExchangeApiConfigRepository {
    pool: Pool<MySql>,
}

impl SqlxExchangeApiConfigRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExchangeApiConfigRepository for SqlxExchangeApiConfigRepository {
    async fn find_by_id(&self, id: i32) -> Result<Option<ExchangeApiConfig>> {
        debug!("查询API配置: id={}", id);

        let entity = sqlx::query_as::<_, ExchangeApiConfigEntity>(
            "SELECT id, exchange_name, api_key, api_secret, passphrase, 
                    is_sandbox, is_enabled, description
             FROM exchange_api_config 
             WHERE id = ? AND is_deleted = 0 LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(entity.map(|e| e.to_domain()))
    }

    async fn find_all_enabled(&self) -> Result<Vec<ExchangeApiConfig>> {
        let entities = sqlx::query_as::<_, ExchangeApiConfigEntity>(
            "SELECT id, exchange_name, api_key, api_secret, passphrase, 
                    is_sandbox, is_enabled, description
             FROM exchange_api_config 
             WHERE is_enabled = 1 AND is_deleted = 0
             ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn find_by_exchange(&self, exchange_name: &str) -> Result<Vec<ExchangeApiConfig>> {
        let entities = sqlx::query_as::<_, ExchangeApiConfigEntity>(
            "SELECT id, exchange_name, api_key, api_secret, passphrase, 
                    is_sandbox, is_enabled, description
             FROM exchange_api_config 
             WHERE exchange_name = ? AND is_enabled = 1 AND is_deleted = 0
             ORDER BY id",
        )
        .bind(exchange_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn save(&self, config: &ExchangeApiConfig) -> Result<i32> {
        let entity = ExchangeApiConfigEntity::from_domain(config);
        
        let result = sqlx::query(
            "INSERT INTO exchange_api_config 
             (exchange_name, api_key, api_secret, passphrase, is_sandbox, is_enabled, description)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entity.exchange_name)
        .bind(&entity.api_key)
        .bind(&entity.api_secret)
        .bind(&entity.passphrase)
        .bind(entity.is_sandbox)
        .bind(entity.is_enabled)
        .bind(&entity.description)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_id() as i32)
    }

    async fn update(&self, config: &ExchangeApiConfig) -> Result<()> {
        let entity = ExchangeApiConfigEntity::from_domain(config);
        
        sqlx::query(
            "UPDATE exchange_api_config 
             SET exchange_name = ?, api_key = ?, api_secret = ?, passphrase = ?, 
                 is_sandbox = ?, is_enabled = ?, description = ?
             WHERE id = ?",
        )
        .bind(&entity.exchange_name)
        .bind(&entity.api_key)
        .bind(&entity.api_secret)
        .bind(&entity.passphrase)
        .bind(entity.is_sandbox)
        .bind(entity.is_enabled)
        .bind(&entity.description)
        .bind(entity.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: i32) -> Result<()> {
        sqlx::query("UPDATE exchange_api_config SET is_deleted = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

/// 策略与API配置关联仓储实现
pub struct SqlxStrategyApiConfigRepository {
    pool: Pool<MySql>,
}

impl SqlxStrategyApiConfigRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StrategyApiConfigRepository for SqlxStrategyApiConfigRepository {
    async fn find_by_strategy_config_id(
        &self,
        strategy_config_id: i32,
    ) -> Result<Vec<ExchangeApiConfig>> {
        debug!("查询策略关联的API配置: strategy_config_id={}", strategy_config_id);

        // 联表查询，按优先级排序
        let entities = sqlx::query_as::<_, ExchangeApiConfigEntity>(
            "SELECT e.id, e.exchange_name, e.api_key, e.api_secret, e.passphrase, 
                    e.is_sandbox, e.is_enabled, e.description
             FROM exchange_api_config e
             INNER JOIN strategy_api_config s ON e.id = s.api_config_id
             WHERE s.strategy_config_id = ? 
               AND s.is_enabled = 1 
               AND s.is_deleted = 0
               AND e.is_enabled = 1 
               AND e.is_deleted = 0
             ORDER BY s.priority ASC, e.id ASC",
        )
        .bind(strategy_config_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn create_association(
        &self,
        strategy_config_id: i32,
        api_config_id: i32,
        priority: i32,
    ) -> Result<i32> {
        let result = sqlx::query(
            "INSERT INTO strategy_api_config 
             (strategy_config_id, api_config_id, priority, is_enabled)
             VALUES (?, ?, ?, 1)
             ON DUPLICATE KEY UPDATE priority = ?, is_enabled = 1, is_deleted = 0",
        )
        .bind(strategy_config_id)
        .bind(api_config_id)
        .bind(priority)
        .bind(priority)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_id() as i32)
    }

    async fn delete_association(&self, id: i32) -> Result<()> {
        sqlx::query("UPDATE strategy_api_config SET is_deleted = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_priority(
        &self,
        id: i32,
        priority: i32,
        is_enabled: bool,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE strategy_api_config 
             SET priority = ?, is_enabled = ?
             WHERE id = ?",
        )
        .bind(priority)
        .bind(if is_enabled { 1i8 } else { 0i8 })
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

