use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{FundFlowAlert, FundFlowSide, MarketAnomaly};
use rust_quant_domain::traits::fund_monitoring_repository::{
    FundFlowAlertRepository, MarketAnomalyRepository,
};
use sqlx::{MySqlPool, Row};

pub struct SqlxMarketAnomalyRepository {
    pool: MySqlPool,
}

impl SqlxMarketAnomalyRepository {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MarketAnomalyRepository for SqlxMarketAnomalyRepository {
    async fn save(&self, anomaly: &MarketAnomaly) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO market_anomalies 
                (symbol, current_rank, rank_15m_ago, rank_4h_ago, rank_24h_ago, 
                 delta_15m, delta_4h, delta_24h, volume_24h, updated_at, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                current_rank = VALUES(current_rank),
                rank_15m_ago = VALUES(rank_15m_ago),
                rank_4h_ago = VALUES(rank_4h_ago),
                rank_24h_ago = VALUES(rank_24h_ago),
                delta_15m = VALUES(delta_15m),
                delta_4h = VALUES(delta_4h),
                delta_24h = VALUES(delta_24h),
                volume_24h = VALUES(volume_24h),
                updated_at = VALUES(updated_at),
                status = VALUES(status)
            "#,
        )
        .bind(&anomaly.symbol)
        .bind(anomaly.current_rank)
        .bind(anomaly.rank_15m_ago)
        .bind(anomaly.rank_4h_ago)
        .bind(anomaly.rank_24h_ago)
        .bind(anomaly.delta_15m)
        .bind(anomaly.delta_4h)
        .bind(anomaly.delta_24h)
        .bind(anomaly.volume_24h)
        .bind(anomaly.updated_at)
        .bind(&anomaly.status)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_id() as i64)
    }

    async fn mark_exited(&self, symbol: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE market_anomalies SET status = 'EXITED', updated_at = NOW()
            WHERE symbol = ?
            "#,
        )
        .bind(symbol)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_latest_update_time(&self) -> Result<Option<DateTime<Utc>>> {
        let row = sqlx::query(
            r#"SELECT MAX(updated_at) as max_time FROM market_anomalies WHERE status = 'ACTIVE'"#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let max_time: Option<DateTime<Utc>> = row.try_get("max_time").ok();
            Ok(max_time)
        } else {
            Ok(None)
        }
    }

    async fn get_all_active(&self) -> Result<Vec<MarketAnomaly>> {
        let rows = sqlx::query(
            r#"
            SELECT id, symbol, current_rank, rank_15m_ago, rank_4h_ago, rank_24h_ago,
                   delta_15m, delta_4h, delta_24h, volume_24h, updated_at, status
            FROM market_anomalies 
            WHERE status = 'ACTIVE'
            ORDER BY current_rank ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(MarketAnomaly {
                id: row.try_get("id").ok(),
                symbol: row.try_get("symbol")?,
                current_rank: row.try_get("current_rank")?,
                rank_15m_ago: row.try_get("rank_15m_ago").ok(),
                rank_4h_ago: row.try_get("rank_4h_ago").ok(),
                rank_24h_ago: row.try_get("rank_24h_ago").ok(),
                delta_15m: row.try_get("delta_15m").ok(),
                delta_4h: row.try_get("delta_4h").ok(),
                delta_24h: row.try_get("delta_24h").ok(),
                volume_24h: row
                    .try_get::<Option<Decimal>, _>("volume_24h")
                    .unwrap_or(None),
                updated_at: row.try_get("updated_at")?,
                status: row.try_get("status")?,
            });
        }
        Ok(result)
    }

    async fn clear_stale_period_data(
        &self,
        clear_15m: bool,
        clear_4h: bool,
        clear_24h: bool,
    ) -> Result<()> {
        let mut updates = Vec::new();
        if clear_15m {
            updates.push("rank_15m_ago = NULL, delta_15m = NULL");
        }
        if clear_4h {
            updates.push("rank_4h_ago = NULL, delta_4h = NULL");
        }
        if clear_24h {
            updates.push("rank_24h_ago = NULL, delta_24h = NULL");
        }

        if updates.is_empty() {
            return Ok(());
        }

        let sql = format!(
            "UPDATE market_anomalies SET {} WHERE status = 'ACTIVE'",
            updates.join(", ")
        );
        sqlx::query(&sql).execute(&self.pool).await?;
        Ok(())
    }
}

pub struct SqlxFundFlowAlertRepository {
    pool: MySqlPool,
}

impl SqlxFundFlowAlertRepository {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FundFlowAlertRepository for SqlxFundFlowAlertRepository {
    async fn save(&self, alert: &FundFlowAlert) -> Result<i64> {
        let side_str = match alert.side {
            FundFlowSide::Inflow => "INFLOW",
            FundFlowSide::Outflow => "OUTFLOW",
        };

        let result = sqlx::query(
            r#"
            INSERT INTO fund_flow_alerts (symbol, net_inflow, total_volume, side, window_secs, alert_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&alert.symbol)
        .bind(alert.net_inflow)
        .bind(alert.total_volume)
        .bind(side_str)
        .bind(alert.window_secs)
        .bind(alert.alert_at)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_id() as i64)
    }
}
