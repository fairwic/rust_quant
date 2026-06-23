//! 资金费率数据访问层实现
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rust_quant_domain::entities::funding_rate::FundingRate;
use rust_quant_domain::traits::funding_rate_repository::FundingRateRepository;
use sqlx::{FromRow, PgPool};
use std::str::FromStr;
use tracing::error;
/// 资金费率数据库实体
#[derive(Debug, Clone, FromRow)]
struct FundingRateEntity {
    /// 唯一标识。
    pub id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 时间字段。
    pub funding_time: i64,
    /// 资金费率。
    pub funding_rate: String,
    /// HTTP 方法。
    pub method: String,
    /// 下一次funding 费率；为空时使用默认值或表示不限制。
    pub next_funding_rate: Option<String>,
    /// 时间字段。
    pub next_funding_time: Option<i64>,
    /// 最小funding 费率；为空时使用默认值或表示不限制。
    pub min_funding_rate: Option<String>,
    /// 最大funding 费率；为空时使用默认值或表示不限制。
    pub max_funding_rate: Option<String>,
    /// settfunding 费率；为空时使用默认值或表示不限制。
    pub sett_funding_rate: Option<String>,
    /// 状态值。
    pub sett_state: Option<String>,
    /// 溢价率；为空时表示交易所未返回该指标。
    pub premium: Option<String>,
    /// 事件时间戳。
    pub ts: i64,
    /// realized 费率；为空时使用默认值或表示不限制。
    pub realized_rate: Option<String>,
    /// interest 费率；为空时使用默认值或表示不限制。
    pub interest_rate: Option<String>,
}
impl FundingRateEntity {
    /// 将内部模型转换为输出结构，避免 配置、基础设施和运行时 的内部字段直接外泄。
    fn to_domain(&self) -> Result<FundingRate> {
        Ok(FundingRate {
            id: Some(self.id),
            inst_id: self.inst_id.clone(),
            funding_rate: f64::from_str(&self.funding_rate).unwrap_or(0.0),
            funding_time: self.funding_time,
            method: self.method.clone(),
            next_funding_rate: self
                .next_funding_rate
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
            next_funding_time: self.next_funding_time,
            min_funding_rate: self
                .min_funding_rate
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
            max_funding_rate: self
                .max_funding_rate
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
            sett_funding_rate: self
                .sett_funding_rate
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
            sett_state: self.sett_state.clone(),
            premium: self
                .premium
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
            ts: self.ts,
            realized_rate: self
                .realized_rate
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
            interest_rate: self
                .interest_rate
                .as_ref()
                .map(|v| f64::from_str(v).unwrap_or(0.0)),
        })
    }
}
/// 基于 sqlx 的资金费率仓储实现
pub struct SqlxFundingRateRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxFundingRateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl FundingRateRepository for SqlxFundingRateRepository {
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    async fn save(&self, funding_rate: FundingRate) -> Result<()> {
        let query = "
            INSERT INTO funding_rates (
                inst_id, funding_time, funding_rate, method, next_funding_rate, next_funding_time,
                min_funding_rate, max_funding_rate, sett_funding_rate, sett_state, premium, ts,
                realized_rate, interest_rate
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (inst_id, funding_time) DO UPDATE SET
                funding_rate = EXCLUDED.funding_rate,
                method = EXCLUDED.method,
                next_funding_rate = EXCLUDED.next_funding_rate,
                next_funding_time = EXCLUDED.next_funding_time,
                min_funding_rate = EXCLUDED.min_funding_rate,
                max_funding_rate = EXCLUDED.max_funding_rate,
                sett_funding_rate = EXCLUDED.sett_funding_rate,
                sett_state = EXCLUDED.sett_state,
                premium = EXCLUDED.premium,
                ts = EXCLUDED.ts,
                realized_rate = EXCLUDED.realized_rate,
                interest_rate = EXCLUDED.interest_rate,
                updated_at = CURRENT_TIMESTAMP
        ";
        sqlx::query(query)
            .bind(&funding_rate.inst_id)
            .bind(funding_rate.funding_time)
            .bind(funding_rate.funding_rate.to_string())
            .bind(&funding_rate.method)
            .bind(funding_rate.next_funding_rate.map(|v| v.to_string()))
            .bind(funding_rate.next_funding_time)
            .bind(funding_rate.min_funding_rate.map(|v| v.to_string()))
            .bind(funding_rate.max_funding_rate.map(|v| v.to_string()))
            .bind(funding_rate.sett_funding_rate.map(|v| v.to_string()))
            .bind(&funding_rate.sett_state)
            .bind(funding_rate.premium.map(|v| v.to_string()))
            .bind(funding_rate.ts)
            .bind(funding_rate.realized_rate.map(|v| v.to_string()))
            .bind(funding_rate.interest_rate.map(|v| v.to_string()))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("保存资金费率失败: {}", e);
                anyhow!("保存资金费率失败: {}", e)
            })?;
        Ok(())
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn save_batch(&self, funding_rates: Vec<FundingRate>) -> Result<()> {
        for rate in funding_rates {
            self.save(rate).await?;
        }
        Ok(())
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_latest(&self, inst_id: &str) -> Result<Option<FundingRate>> {
        let query = "
            SELECT * FROM funding_rates 
            WHERE inst_id = $1
            ORDER BY funding_time DESC 
            LIMIT 1
        ";
        let entity = sqlx::query_as::<_, FundingRateEntity>(query)
            .bind(inst_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("查询最新资金费率失败: {}", e);
                anyhow!("查询最新资金费率失败: {}", e)
            })?;
        match entity {
            Some(e) => Ok(Some(e.to_domain()?)),
            None => Ok(None),
        }
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_history(
        &self,
        inst_id: &str,
        start_time: i64,
        end_time: i64,
        limit: Option<i64>,
    ) -> Result<Vec<FundingRate>> {
        let limit = limit.unwrap_or(100);
        let query = "
            SELECT * FROM funding_rates 
            WHERE inst_id = $1 AND funding_time >= $2 AND funding_time <= $3
            ORDER BY funding_time DESC
            LIMIT $4
        ";
        let entities = sqlx::query_as::<_, FundingRateEntity>(query)
            .bind(inst_id)
            .bind(start_time)
            .bind(end_time)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询资金费率历史失败: {}", e);
                anyhow!("查询资金费率历史失败: {}", e)
            })?;
        let mut funding_rates = Vec::new();
        for entity in entities {
            funding_rates.push(entity.to_domain()?);
        }
        Ok(funding_rates)
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_oldest(&self, inst_id: &str) -> Result<Option<FundingRate>> {
        let query = "
            SELECT * FROM funding_rates 
            WHERE inst_id = $1
            ORDER BY funding_time ASC 
            LIMIT 1
        ";
        let entity = sqlx::query_as::<_, FundingRateEntity>(query)
            .bind(inst_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("查询最早资金费率失败: {}", e);
                anyhow!("查询最早资金费率失败: {}", e)
            })?;
        match entity {
            Some(e) => Ok(Some(e.to_domain()?)),
            None => Ok(None),
        }
    }
}
