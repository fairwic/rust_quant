//! 策略配置仓储实现
//! 
//! 从 src/trading/model/strategy/strategy_config.rs 迁移
//! rbatis → sqlx

use async_trait::async_trait;
use anyhow::Result;
use sqlx::{MySql, Pool, FromRow};
use tracing::debug;

use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};
use rust_quant_domain::traits::StrategyConfigRepository;

/// 策略配置数据库实体
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct StrategyConfigEntity {
    pub id: i64,
    pub strategy_type: String,
    pub inst_id: String,
    pub time: String,
    pub value: String,          // JSON 格式的策略参数
    pub risk_config: String,    // JSON 格式的风险配置
    pub kline_start_time: i64,
    pub kline_end_time: i64,
    pub final_fund: f64,
    pub is_deleted: i32,
}

impl StrategyConfigEntity {
    /// 转换为领域实体
    pub fn to_domain(&self) -> Result<StrategyConfig> {
        let strategy_type = StrategyType::from_str(&self.strategy_type)
            .unwrap_or(StrategyType::Custom(0));
        
        let timeframe = Timeframe::from_str(&self.time)
            .unwrap_or(Timeframe::H1);
        
        let parameters: serde_json::Value = serde_json::from_str(&self.value)?;
        let risk_config: serde_json::Value = serde_json::from_str(&self.risk_config)?;
        
        let mut config = StrategyConfig::new(
            self.id,
            strategy_type,
            self.inst_id.clone(),
            timeframe,
            parameters,
            risk_config,
        );
        
        config.set_backtest_range(self.kline_start_time, self.kline_end_time);
        
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
            "SELECT * FROM strategy_config WHERE id = ? LIMIT 1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(entity)
    }
    
    /// 获取所有未删除的配置
    pub async fn get_all(&self) -> Result<Vec<StrategyConfigEntity>> {
        let entities = sqlx::query_as::<_, StrategyConfigEntity>(
            "SELECT * FROM strategy_config WHERE is_deleted = 0"
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(entities)
    }
    
    /// 按周期查询配置
    pub async fn get_all_by_period(&self, period: &str) -> Result<Vec<StrategyConfigEntity>> {
        let entities = sqlx::query_as::<_, StrategyConfigEntity>(
            "SELECT * FROM strategy_config WHERE is_deleted = 0 AND time = ?"
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
        let query = match strategy_type {
            Some(st) => {
                sqlx::query_as::<_, StrategyConfigEntity>(
                    "SELECT * FROM strategy_config 
                     WHERE is_deleted = 0 
                       AND strategy_type = ? 
                       AND inst_id = ? 
                       AND time = ?"
                )
                .bind(st)
                .bind(inst_id)
                .bind(time)
            }
            None => {
                sqlx::query_as::<_, StrategyConfigEntity>(
                    "SELECT * FROM strategy_config 
                     WHERE is_deleted = 0 
                       AND inst_id = ? 
                       AND time = ?"
                )
                .bind(inst_id)
                .bind(time)
            }
        };
        
        let entities = query.fetch_all(&self.pool).await?;
        Ok(entities)
    }
    
    /// 插入新配置
    pub async fn insert(&self, entity: &StrategyConfigEntity) -> Result<u64> {
        let result = sqlx::query(
            "INSERT INTO strategy_config 
             (strategy_type, inst_id, time, value, risk_config, 
              kline_start_time, kline_end_time, final_fund, is_deleted)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&entity.strategy_type)
        .bind(&entity.inst_id)
        .bind(&entity.time)
        .bind(&entity.value)
        .bind(&entity.risk_config)
        .bind(entity.kline_start_time)
        .bind(entity.kline_end_time)
        .bind(entity.final_fund)
        .bind(entity.is_deleted)
        .execute(&self.pool)
        .await?;
        
        Ok(result.last_insert_id())
    }
    
    /// 更新配置
    pub async fn update(&self, id: i64, entity: &StrategyConfigEntity) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE strategy_config 
             SET strategy_type = ?, inst_id = ?, time = ?, value = ?, 
                 risk_config = ?, kline_start_time = ?, kline_end_time = ?, 
                 final_fund = ?
             WHERE id = ?"
        )
        .bind(&entity.strategy_type)
        .bind(&entity.inst_id)
        .bind(&entity.time)
        .bind(&entity.value)
        .bind(&entity.risk_config)
        .bind(entity.kline_start_time)
        .bind(entity.kline_end_time)
        .bind(entity.final_fund)
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
    
    async fn find_by_symbol_and_timeframe(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Vec<StrategyConfig>> {
        let entities = self.get_config(None, symbol, timeframe.as_str()).await?;
        entities.into_iter()
            .map(|e| e.to_domain())
            .collect()
    }
    
    async fn save(&self, config: &StrategyConfig) -> Result<i64> {
        let entity = StrategyConfigEntity {
            id: config.id,
            strategy_type: config.strategy_type.as_str().to_string(),
            inst_id: config.symbol.clone(),
            time: config.timeframe.as_str().to_string(),
            value: serde_json::to_string(&config.parameters)?,
            risk_config: serde_json::to_string(&config.risk_config)?,
            kline_start_time: config.backtest_start.unwrap_or(0),
            kline_end_time: config.backtest_end.unwrap_or(0),
            final_fund: 0.0,
            is_deleted: 0,
        };
        
        self.insert(&entity).await.map(|id| id as i64)
    }
    
    async fn update(&self, config: &StrategyConfig) -> Result<()> {
        let entity = StrategyConfigEntity {
            id: config.id,
            strategy_type: config.strategy_type.as_str().to_string(),
            inst_id: config.symbol.clone(),
            time: config.timeframe.as_str().to_string(),
            value: serde_json::to_string(&config.parameters)?,
            risk_config: serde_json::to_string(&config.risk_config)?,
            kline_start_time: config.backtest_start.unwrap_or(0),
            kline_end_time: config.backtest_end.unwrap_or(0),
            final_fund: 0.0,
            is_deleted: 0,
        };
        
        SqlxStrategyConfigRepository::update(self, config.id, &entity).await?;
        Ok(())
    }
    
    async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query("UPDATE strategy_config SET is_deleted = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

/// 提供兼容旧代码的Model接口
pub struct StrategyConfigEntityModel {
    repository: SqlxStrategyConfigRepository,
}

impl StrategyConfigEntityModel {
    /// 创建新的Model实例
    pub async fn new() -> Self {
        let pool = rust_quant_core::database::get_db_pool();
        Self {
            repository: SqlxStrategyConfigRepository::new(pool.clone()),
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
        self.repository.get_config(strategy_type, inst_id, time).await
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

