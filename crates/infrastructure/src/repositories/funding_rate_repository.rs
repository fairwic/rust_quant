//! 资金费率数据访问层实现

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use sqlx::{FromRow, MySql, Pool};
use tracing::{debug, error};

use rust_quant_domain::entities::funding_rate::FundingRate;
use rust_quant_domain::traits::funding_rate_repository::FundingRateRepository;
use std::str::FromStr;

/// 资金费率数据库实体
#[derive(Debug, Clone, FromRow)]
struct FundingRateEntity {
    pub id: i64,
    pub inst_id: String,
    pub funding_time: i64,
    pub funding_rate: String,
    pub method: String,
    pub next_funding_rate: Option<String>,
    pub next_funding_time: Option<i64>,
    pub min_funding_rate: Option<String>,
    pub max_funding_rate: Option<String>,
    pub sett_funding_rate: Option<String>,
    pub sett_state: Option<String>,
    pub premium: Option<String>,
    pub ts: i64,
    pub realized_rate: Option<String>,
    pub interest_rate: Option<String>,
}

impl FundingRateEntity {
    fn to_domain(&self) -> Result<FundingRate> {
        Ok(FundingRate {
            id: Some(self.id),
            inst_id: self.inst_id.clone(),
            funding_rate: f64::from_str(&self.funding_rate).unwrap_or(0.0),
            funding_time: self.funding_time,
            method: self.method.clone(),
            next_funding_rate: self.next_funding_rate.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
            next_funding_time: self.next_funding_time,
            min_funding_rate: self.min_funding_rate.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
            max_funding_rate: self.max_funding_rate.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
            sett_funding_rate: self.sett_funding_rate.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
            sett_state: self.sett_state.clone(),
            premium: self.premium.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
            ts: self.ts,
            realized_rate: self.realized_rate.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
            interest_rate: self.interest_rate.as_ref().map(|v| f64::from_str(v).unwrap_or(0.0)),
        })
    }
}

/// 基于 sqlx 的资金费率仓储实现
pub struct SqlxFundingRateRepository {
    pool: Pool<MySql>,
}

impl SqlxFundingRateRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FundingRateRepository for SqlxFundingRateRepository {
    async fn save(&self, funding_rate: FundingRate) -> Result<()> {
        let query = "
            INSERT INTO funding_rates (
                inst_id, funding_time, funding_rate, method, next_funding_rate, next_funding_time,
                min_funding_rate, max_funding_rate, sett_funding_rate, sett_state, premium, ts,
                realized_rate, interest_rate
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                funding_rate = VALUES(funding_rate),
                method = VALUES(method),
                next_funding_rate = VALUES(next_funding_rate),
                next_funding_time = VALUES(next_funding_time),
                min_funding_rate = VALUES(min_funding_rate),
                max_funding_rate = VALUES(max_funding_rate),
                sett_funding_rate = VALUES(sett_funding_rate),
                sett_state = VALUES(sett_state),
                premium = VALUES(premium),
                ts = VALUES(ts),
                realized_rate = VALUES(realized_rate),
                interest_rate = VALUES(interest_rate),
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

    async fn save_batch(&self, funding_rates: Vec<FundingRate>) -> Result<()> {
        for rate in funding_rates {
            self.save(rate).await?;
        }
        Ok(())
    }

    async fn find_latest(&self, inst_id: &str) -> Result<Option<FundingRate>> {
        let query = "
            SELECT * FROM funding_rates 
            WHERE inst_id = ? 
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
            WHERE inst_id = ? AND funding_time >= ? AND funding_time <= ?
            ORDER BY funding_time DESC
            LIMIT ?
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

    async fn find_oldest(&self, inst_id: &str) -> Result<Option<FundingRate>> {
        let query = "
            SELECT * FROM funding_rates 
            WHERE inst_id = ? 
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
