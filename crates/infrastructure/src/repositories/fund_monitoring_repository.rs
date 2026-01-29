use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::entities::{FundFlowAlert, FundFlowSide, MarketAnomaly};
use rust_quant_domain::traits::fund_monitoring_repository::{
    FundFlowAlertRepository, MarketAnomalyRepository,
};
use sqlx::MySqlPool;

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
    /// UPSERT: 按 symbol 唯一键更新或插入
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

    /// 标记跌出 Top 150 的币种为 EXITED
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
