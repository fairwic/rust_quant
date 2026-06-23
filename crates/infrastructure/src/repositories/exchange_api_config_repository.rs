//! 交易所API配置仓储实现
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_domain::traits::{ExchangeApiConfigRepository, StrategyApiConfigRepository};
use sqlx::{FromRow, PgPool};
use tracing::debug;
/// 交易所API配置数据库实体
#[derive(Debug, Clone, FromRow)]
pub struct ExchangeAppkeyConfigEntity {
    /// 唯一标识。
    pub id: i32,
    /// 名称。
    pub exchange_name: String,
    /// API Key。
    pub api_key: String,
    /// APISecret，用于配置运行参数。
    pub api_secret: String,
    /// API passphrase；为空时表示该交易所不需要 passphrase。
    pub passphrase: Option<String>,
    pub is_sandbox: i8, // legacy tinyint(1)
    pub is_enabled: i8, // legacy tinyint(1)
    /// 描述信息。
    pub description: Option<String>,
}
impl ExchangeAppkeyConfigEntity {
    /// 转换为领域实体
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
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
pub struct ExchangeApiStrategyRelationEntity {
    /// 唯一标识。
    pub id: i32,
    /// 策略config ID。
    pub strategy_config_id: i32,
    /// 交易所 API Key 标识。
    pub api_key_config_id: i32,
    /// priority，用于运行时配置或基础设施依赖。
    pub priority: i32,
    /// 是否启用。
    pub is_enabled: i8,
}
/// 交易所API配置仓储实现
pub struct SqlxExchangeApiConfigRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxExchangeApiConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl ExchangeApiConfigRepository for SqlxExchangeApiConfigRepository {
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    async fn find_by_id(&self, id: i32) -> Result<Option<ExchangeApiConfig>> {
        debug!("查询API配置: id={}", id);
        let entity = sqlx::query_as::<_, ExchangeAppkeyConfigEntity>(
            "SELECT id, exchange_name, api_key, api_secret, passphrase,
                    is_sandbox, is_enabled, description
             FROM exchange_apikey_config
             WHERE id = $1 AND is_deleted = 0 LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(entity.map(|e| e.to_domain()))
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_all_enabled(&self) -> Result<Vec<ExchangeApiConfig>> {
        let entities = sqlx::query_as::<_, ExchangeAppkeyConfigEntity>(
            "SELECT id, exchange_name, api_key, api_secret, passphrase,
                    is_sandbox, is_enabled, description
             FROM exchange_apikey_config
             WHERE is_enabled = 1 AND is_deleted = 0
             ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_exchange(&self, exchange_name: &str) -> Result<Vec<ExchangeApiConfig>> {
        let entities = sqlx::query_as::<_, ExchangeAppkeyConfigEntity>(
            "SELECT id, exchange_name, api_key, api_secret, passphrase,
                    is_sandbox, is_enabled, description
             FROM exchange_apikey_config
             WHERE exchange_name = $1 AND is_enabled = 1 AND is_deleted = 0
             ORDER BY id",
        )
        .bind(exchange_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }
    /// 提供save的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn save(&self, config: &ExchangeApiConfig) -> Result<i32> {
        let entity = ExchangeAppkeyConfigEntity::from_domain(config);
        let inserted_id = sqlx::query_scalar::<_, i32>(
            "INSERT INTO exchange_apikey_config
             (exchange_name, api_key, api_secret, passphrase, is_sandbox, is_enabled, description)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(&entity.exchange_name)
        .bind(&entity.api_key)
        .bind(&entity.api_secret)
        .bind(&entity.passphrase)
        .bind(entity.is_sandbox)
        .bind(entity.is_enabled)
        .bind(&entity.description)
        .fetch_one(&self.pool)
        .await?;
        Ok(inserted_id)
    }
    /// 执行更新步骤，串起配置运行时需要的状态推进和错误处理。
    async fn update(&self, config: &ExchangeApiConfig) -> Result<()> {
        let entity = ExchangeAppkeyConfigEntity::from_domain(config);
        sqlx::query(
            "UPDATE exchange_apikey_config
             SET exchange_name = $1, api_key = $2, api_secret = $3, passphrase = $4,
                 is_sandbox = $5, is_enabled = $6, description = $7
             WHERE id = $8",
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
    /// 提供delete的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn delete(&self, id: i32) -> Result<()> {
        sqlx::query("UPDATE exchange_apikey_config SET is_deleted = 1 WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
/// 策略与API配置关联仓储实现
pub struct SqlxStrategyApiConfigRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxStrategyApiConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl StrategyApiConfigRepository for SqlxStrategyApiConfigRepository {
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 采用 async 以支持数据库/网络 I/O 的并发调度，避免阻塞。
    async fn find_by_strategy_config_id(
        &self,
        strategy_config_id: i32,
    ) -> Result<Vec<ExchangeApiConfig>> {
        debug!(
            "查询策略关联的API配置: strategy_config_id={}",
            strategy_config_id
        );
        // 联表查询，按优先级排序
        let entities = sqlx::query_as::<_, ExchangeAppkeyConfigEntity>(
            "SELECT e.id, e.exchange_name, e.api_key, e.api_secret, e.passphrase,
                    e.is_sandbox, e.is_enabled, e.description
             FROM exchange_apikey_config e
             INNER JOIN exchange_apikey_strategy_relation s ON e.id = s.api_config_id
             WHERE s.strategy_config_id = $1
               AND s.is_enabled = 1
               AND e.is_enabled = 1
               AND e.is_deleted = 0
             ORDER BY s.priority ASC, e.id ASC",
        )
        .bind(strategy_config_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }
    /// 创建 配置、基础设施和运行时 资源，并在入口处完成必要的参数归一。
    async fn create_association(
        &self,
        strategy_config_id: i32,
        api_config_id: i32,
        priority: i32,
    ) -> Result<i32> {
        if let Some(existing_id) = sqlx::query_scalar::<_, i32>(
            "SELECT id
             FROM exchange_apikey_strategy_relation
             WHERE strategy_config_id = $1 AND api_config_id = $2
             ORDER BY id ASC
             LIMIT 1",
        )
        .bind(strategy_config_id)
        .bind(api_config_id)
        .fetch_optional(&self.pool)
        .await?
        {
            sqlx::query(
                "UPDATE exchange_apikey_strategy_relation
                 SET priority = $1, is_enabled = 1
                 WHERE id = $2",
            )
            .bind(priority)
            .bind(existing_id)
            .execute(&self.pool)
            .await?;
            return Ok(existing_id);
        }
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO exchange_apikey_strategy_relation
             (strategy_config_id, api_config_id, priority, is_enabled)
             VALUES ($1, $2, $3, 1)
             RETURNING id",
        )
        .bind(strategy_config_id)
        .bind(api_config_id)
        .bind(priority)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
    /// 删除或清理 配置、基础设施和运行时 的临时数据，避免过期状态继续影响后续流程。
    async fn delete_association(&self, id: i32) -> Result<()> {
        sqlx::query("DELETE FROM exchange_apikey_strategy_relation WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    /// 更新 配置、基础设施和运行时 状态，并保留调用方需要的结果或错误信息。
    async fn update_priority(&self, id: i32, priority: i32, is_enabled: bool) -> Result<()> {
        sqlx::query(
            "UPDATE exchange_apikey_strategy_relation
             SET priority = $1, is_enabled = $2
             WHERE id = $3",
        )
        .bind(priority)
        .bind(if is_enabled { 1i8 } else { 0i8 })
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
