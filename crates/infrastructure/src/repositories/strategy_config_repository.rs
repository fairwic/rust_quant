//! 策略配置仓储实现
//!
//! 从 src/trading/model/strategy/strategy_config.rs 迁移
//! rbatis → sqlx

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{FromRow, MySql, Pool};
use tracing::debug;

use rust_quant_domain::traits::StrategyConfigRepository;
use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};

/// 策略配置数据库实体
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct StrategyConfigEntity {
    pub id: i32, // MySQL int (32位)
    pub strategy_type: String,
    pub inst_id: String,
    pub time: String,
    pub value: Option<String>,         // JSON 格式的策略参数 (可为 NULL)
    pub risk_config: String,           // JSON 格式的风险配置
    pub kline_start_time: Option<i64>, // bigint 可为 NULL
    pub kline_end_time: Option<i64>,   // bigint 可为 NULL
    pub final_fund: f32,               // MySQL float (32位)
    pub is_deleted: i16,               // MySQL smallint (16位)
}

impl StrategyConfigEntity {
    /// 转换为领域实体
    pub fn to_domain(&self) -> Result<StrategyConfig> {
        let strategy_type =
            StrategyType::from_str(&self.strategy_type).unwrap_or(StrategyType::Custom(0));

        let timeframe = Timeframe::from_str(&self.time).unwrap_or(Timeframe::H1);

        // value 可能为 NULL，需要处理
        let value_str = self.value.as_deref().unwrap_or("{}");
        let parameters: serde_json::Value = serde_json::from_str(value_str)?;
        let risk_config: serde_json::Value = serde_json::from_str(&self.risk_config)?;

        let mut config = StrategyConfig::new(
            self.id as i64, // 转换为 i64 以匹配 domain 类型
            strategy_type,
            self.inst_id.clone(),
            timeframe,
            parameters,
            risk_config,
        );

        // 处理可选的回测时间范围
        let start_time = self.kline_start_time.unwrap_or(0);
        let end_time = self.kline_end_time.unwrap_or(0);
        config.set_backtest_range(start_time, end_time);

        Ok(config)
    }
}

/// 策略配置仓储实现 (基于 sqlx)
pub struct SqlxStrategyConfigRepository {
    pool: Pool<MySql>,
}

impl SqlxStrategyConfigRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    /// 根据ID查询配置
    pub async fn get_config_by_id(&self, id: i64) -> Result<Option<StrategyConfigEntity>> {
        debug!("查询策略配置: id={}", id);

        let entity = sqlx::query_as::<_, StrategyConfigEntity>(
            "SELECT id, strategy_type, inst_id, time, value, risk_config, 
                    kline_start_time, kline_end_time, final_fund, is_deleted
             FROM strategy_config WHERE id = ? LIMIT 1",
        )
        .bind(id as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(entity)
    }

    /// 获取所有未删除的配置
    pub async fn get_all(&self) -> Result<Vec<StrategyConfigEntity>> {
        let entities = sqlx::query_as::<_, StrategyConfigEntity>(
            "SELECT id, strategy_type, inst_id, time, value, risk_config, 
                    kline_start_time, kline_end_time, final_fund, is_deleted
             FROM strategy_config WHERE is_deleted = 0",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(entities)
    }

    /// 按周期查询配置
    pub async fn get_all_by_period(&self, period: &str) -> Result<Vec<StrategyConfigEntity>> {
        let entities = sqlx::query_as::<_, StrategyConfigEntity>(
            "SELECT id, strategy_type, inst_id, time, value, risk_config, 
                    kline_start_time, kline_end_time, final_fund, is_deleted
             FROM strategy_config WHERE is_deleted = 0 AND time = ?",
        )
        .bind(period)
        .fetch_all(&self.pool)
        .await?;

        Ok(entities)
    }

    /// 根据策略类型、产品、周期查询配置
    pub async fn get_config(
        &self,
        strategy_type: Option<&str>,
        inst_id: &str,
        time: &str,
    ) -> Result<Vec<StrategyConfigEntity>> {
        let entities = match strategy_type {
            Some(st) => {
                sqlx::query_as::<_, StrategyConfigEntity>(
                    "SELECT id, strategy_type, inst_id, time, value, risk_config, 
                            kline_start_time, kline_end_time, final_fund, is_deleted
                     FROM strategy_config 
                     WHERE is_deleted = 0 
                       AND strategy_type = ? 
                       AND inst_id = ? 
                       AND time = ?",
                )
                .bind(st)
                .bind(inst_id)
                .bind(time)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, StrategyConfigEntity>(
                    "SELECT id, strategy_type, inst_id, time, value, risk_config, 
                            kline_start_time, kline_end_time, final_fund, is_deleted
                     FROM strategy_config 
                     WHERE is_deleted = 0 
                       AND inst_id = ? 
                       AND time = ?",
                )
                .bind(inst_id)
                .bind(time)
                .fetch_all(&self.pool)
                .await?
            }
        };

        debug!(
            "查询策略配置: strategy_type={:?}, inst_id={}, time={}, 结果数量={}",
            strategy_type,
            inst_id,
            time,
            entities.len()
        );
        Ok(entities)
    }

    /// 插入新配置
    pub async fn insert(&self, entity: &StrategyConfigEntity) -> Result<u64> {
        let result = sqlx::query(
            "INSERT INTO strategy_config 
             (strategy_type, inst_id, time, value, risk_config, 
              kline_start_time, kline_end_time, final_fund, is_deleted)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entity.strategy_type)
        .bind(&entity.inst_id)
        .bind(&entity.time)
        .bind(&entity.value)
        .bind(&entity.risk_config)
        .bind(entity.kline_start_time)
        .bind(entity.kline_end_time)
        .bind(entity.final_fund as f64) // f32 -> f64 转换
        .bind(entity.is_deleted as i32) // i16 -> i32 转换
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_id())
    }

    /// 更新配置
    pub async fn update(&self, id: i32, entity: &StrategyConfigEntity) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE strategy_config 
             SET strategy_type = ?, inst_id = ?, time = ?, value = ?, 
                 risk_config = ?, kline_start_time = ?, kline_end_time = ?, 
                 final_fund = ?
             WHERE id = ?",
        )
        .bind(&entity.strategy_type)
        .bind(&entity.inst_id)
        .bind(&entity.time)
        .bind(&entity.value)
        .bind(&entity.risk_config)
        .bind(entity.kline_start_time)
        .bind(entity.kline_end_time)
        .bind(entity.final_fund as f64) // f32 -> f64 转换
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[async_trait]
impl StrategyConfigRepository for SqlxStrategyConfigRepository {
    async fn find_by_id(&self, id: i64) -> Result<Option<StrategyConfig>> {
        let entity = self.get_config_by_id(id).await?;
        match entity {
            Some(e) => Ok(Some(e.to_domain()?)),
            None => Ok(None),
        }
    }

    async fn find_all_enabled(&self) -> Result<Vec<StrategyConfig>> {
        let entities = self.get_all().await?;
        entities.into_iter().map(|e| e.to_domain()).collect()
    }

    async fn find_by_symbol_and_timeframe(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Vec<StrategyConfig>> {
        let entities = self.get_config(None, symbol, timeframe.as_str()).await?;
        entities.into_iter().map(|e| e.to_domain()).collect()
    }

    async fn save(&self, config: &StrategyConfig) -> Result<i64> {
        let entity = StrategyConfigEntity {
            id: config.id as i32, // 转换为 i32 以匹配数据库类型
            strategy_type: config.strategy_type.as_str().to_string(),
            inst_id: config.symbol.clone(),
            time: config.timeframe.as_str().to_string(),
            value: Some(serde_json::to_string(&config.parameters)?),
            risk_config: serde_json::to_string(&config.risk_config)?,
            kline_start_time: config.backtest_start,
            kline_end_time: config.backtest_end,
            final_fund: 0.0f32,
            is_deleted: 0i16,
        };

        self.insert(&entity).await.map(|id| id as i64)
    }

    async fn update(&self, config: &StrategyConfig) -> Result<()> {
        let entity = StrategyConfigEntity {
            id: config.id as i32, // 转换为 i32 以匹配数据库类型
            strategy_type: config.strategy_type.as_str().to_string(),
            inst_id: config.symbol.clone(),
            time: config.timeframe.as_str().to_string(),
            value: Some(serde_json::to_string(&config.parameters)?),
            risk_config: serde_json::to_string(&config.risk_config)?,
            kline_start_time: config.backtest_start,
            kline_end_time: config.backtest_end,
            final_fund: 0.0f32,
            is_deleted: 0i16,
        };

        SqlxStrategyConfigRepository::update(self, config.id as i32, &entity).await?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query("UPDATE strategy_config SET is_deleted = 1 WHERE id = ?")
            .bind(id as i32)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

/// 提供兼容旧代码的Model接口
///
/// # 架构说明
/// 这是一个兼容层，用于向后兼容旧代码。
/// 新代码应该直接使用 `SqlxStrategyConfigRepository` 并通过依赖注入传递。
pub struct StrategyConfigEntityModel {
    repository: SqlxStrategyConfigRepository,
}

impl StrategyConfigEntityModel {
    /// 创建新的Model实例（通过依赖注入）
    ///
    /// # 参数
    /// * `pool` - 数据库连接池
    ///
    /// # 架构说明
    /// 此方法通过构造函数注入数据库连接池，符合依赖注入原则。
    /// 调用方应该在应用入口创建 pool 并传递。
    pub fn new(pool: Pool<MySql>) -> Self {
        Self {
            repository: SqlxStrategyConfigRepository::new(pool),
        }
    }

    /// 根据ID查询配置
    pub async fn get_config_by_id(&self, id: i64) -> Result<Option<StrategyConfigEntity>> {
        self.repository.get_config_by_id(id).await
    }

    /// 查询配置
    pub async fn get_config(
        &self,
        strategy_type: Option<&str>,
        inst_id: &str,
        time: &str,
    ) -> Result<Vec<StrategyConfigEntity>> {
        self.repository
            .get_config(strategy_type, inst_id, time)
            .await
    }

    /// 获取所有配置
    pub async fn get_all(&self) -> Result<Vec<StrategyConfigEntity>> {
        self.repository.get_all().await
    }

    /// 按周期查询
    pub async fn get_all_by_period(&self, period: &str) -> Result<Vec<StrategyConfigEntity>> {
        self.repository.get_all_by_period(period).await
    }
}
