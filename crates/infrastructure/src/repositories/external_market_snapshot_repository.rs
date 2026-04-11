use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use serde_json::Value;
use sqlx::{types::Json, FromRow, MySql, Pool};
use tracing::error;

#[derive(Debug, Clone, FromRow)]
struct ExternalMarketSnapshotEntity {
    pub id: i64,
    pub source: String,
    pub symbol: String,
    pub metric_type: String,
    pub metric_time: i64,
    pub funding_rate: Option<f64>,
    pub premium: Option<f64>,
    pub open_interest: Option<f64>,
    pub oracle_price: Option<f64>,
    pub mark_price: Option<f64>,
    pub long_short_ratio: Option<f64>,
    pub raw_payload: Option<Json<Value>>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ExternalMarketSnapshotEntity {
    fn to_domain(&self) -> ExternalMarketSnapshot {
        ExternalMarketSnapshot {
            id: Some(self.id),
            source: self.source.clone(),
            symbol: self.symbol.clone(),
            metric_type: self.metric_type.clone(),
            metric_time: self.metric_time,
            funding_rate: self.funding_rate,
            premium: self.premium,
            open_interest: self.open_interest,
            oracle_price: self.oracle_price,
            mark_price: self.mark_price,
            long_short_ratio: self.long_short_ratio,
            raw_payload: self.raw_payload.clone().map(|json| json.0),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub struct SqlxExternalMarketSnapshotRepository {
    pool: Pool<MySql>,
}

impl SqlxExternalMarketSnapshotRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExternalMarketSnapshotRepository for SqlxExternalMarketSnapshotRepository {
    async fn save(&self, snapshot: ExternalMarketSnapshot) -> Result<()> {
        let query = r#"
            INSERT INTO external_market_snapshots (
                source, symbol, metric_type, metric_time, funding_rate, premium, open_interest,
                oracle_price, mark_price, long_short_ratio, raw_payload
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                funding_rate = VALUES(funding_rate),
                premium = VALUES(premium),
                open_interest = VALUES(open_interest),
                oracle_price = VALUES(oracle_price),
                mark_price = VALUES(mark_price),
                long_short_ratio = VALUES(long_short_ratio),
                raw_payload = VALUES(raw_payload),
                updated_at = CURRENT_TIMESTAMP
        "#;

        sqlx::query(query)
            .bind(snapshot.source)
            .bind(snapshot.symbol)
            .bind(snapshot.metric_type)
            .bind(snapshot.metric_time)
            .bind(snapshot.funding_rate)
            .bind(snapshot.premium)
            .bind(snapshot.open_interest)
            .bind(snapshot.oracle_price)
            .bind(snapshot.mark_price)
            .bind(snapshot.long_short_ratio)
            .bind(snapshot.raw_payload.map(Json))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("保存外部市场快照失败: {}", e);
                anyhow!("保存外部市场快照失败: {}", e)
            })?;

        Ok(())
    }

    async fn save_batch(&self, snapshots: Vec<ExternalMarketSnapshot>) -> Result<()> {
        for snapshot in snapshots {
            self.save(snapshot).await?;
        }
        Ok(())
    }

    async fn find_range(
        &self,
        source: &str,
        symbol: &str,
        metric_type: &str,
        start_time: i64,
        end_time: i64,
        limit: Option<i64>,
    ) -> Result<Vec<ExternalMarketSnapshot>> {
        let limit = limit.unwrap_or(500);
        let query = r#"
            SELECT *
            FROM external_market_snapshots
            WHERE source = ?
              AND symbol = ?
              AND metric_type = ?
              AND metric_time >= ?
              AND metric_time <= ?
            ORDER BY metric_time ASC
            LIMIT ?
        "#;

        let rows = sqlx::query_as::<_, ExternalMarketSnapshotEntity>(query)
            .bind(source)
            .bind(symbol)
            .bind(metric_type)
            .bind(start_time)
            .bind(end_time)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询外部市场快照失败: {}", e);
                anyhow!("查询外部市场快照失败: {}", e)
            })?;

        Ok(rows
            .into_iter()
            .map(|row| row.to_domain())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_entity_to_domain_can_be_called_without_consuming_entity() {
        let entity = ExternalMarketSnapshotEntity {
            id: 1,
            source: "hyperliquid".to_string(),
            symbol: "ETH".to_string(),
            metric_type: "funding".to_string(),
            metric_time: 1_744_000_000_000,
            funding_rate: Some(0.0001),
            premium: Some(0.001),
            open_interest: Some(1234.0),
            oracle_price: Some(2000.0),
            mark_price: Some(2001.0),
            long_short_ratio: Some(1.2),
            raw_payload: Some(Json(json!({"key": "value"}))),
            created_at: None,
            updated_at: None,
        };

        let first = entity.to_domain();
        let second = entity.to_domain();

        assert_eq!(first.id, Some(1));
        assert_eq!(first.source, "hyperliquid");
        assert_eq!(first.symbol, "ETH");
        assert_eq!(second.metric_type, "funding");
        assert_eq!(second.raw_payload, Some(json!({"key": "value"})));
    }
}
