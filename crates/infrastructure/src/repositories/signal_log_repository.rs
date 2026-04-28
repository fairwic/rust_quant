//! 策略信号日志仓储
//!
//! 记录策略产生的交易信号

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::{debug, info};

/// 策略信号日志实体
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SignalLogEntity {
    #[sqlx(default)]
    pub id: Option<i32>,
    pub inst_id: String,
    pub time: String,
    pub strategy_type: String,
    pub strategy_result: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

/// 信号日志仓储
pub struct SignalLogRepository {
    pool: PgPool,
}

impl SignalLogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 保存信号日志
    ///
    /// # Arguments
    /// * `inst_id` - 交易对
    /// * `period` - 周期（写入表字段：`time`）
    /// * `strategy_type` - 策略类型
    /// * `signal_json` - 信号JSON字符串
    pub async fn save_signal_log(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
        signal_json: &str,
    ) -> Result<u64> {
        let result = sqlx::query(
            "INSERT INTO strategy_job_signal_log (inst_id, time, strategy_type, strategy_result)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(inst_id)
        .bind(period)
        .bind(strategy_type)
        .bind(signal_json)
        .execute(&self.pool)
        .await?;

        debug!(
            "保存信号日志: inst_id={}, time={}, rows={}",
            inst_id,
            period,
            result.rows_affected()
        );

        Ok(result.rows_affected())
    }

    /// 查询最近的信号日志
    ///
    /// # Arguments
    /// * `inst_id` - 交易对
    /// * `period` - 周期（查询表字段：`time`）
    /// * `limit` - 数量限制
    pub async fn find_recent_signals(
        &self,
        inst_id: &str,
        period: &str,
        limit: usize,
    ) -> Result<Vec<SignalLogEntity>> {
        let signals = sqlx::query_as::<_, SignalLogEntity>(
            "SELECT * FROM strategy_job_signal_log
             WHERE inst_id = $1 AND time = $2
             ORDER BY created_at DESC
             LIMIT $3",
        )
        .bind(inst_id)
        .bind(period)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(signals)
    }

    /// 查询所有信号日志
    pub async fn find_all(&self, limit: Option<usize>) -> Result<Vec<SignalLogEntity>> {
        let limit = limit.unwrap_or(100);
        let signals = sqlx::query_as::<_, SignalLogEntity>(
            "SELECT * FROM strategy_job_signal_log
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(signals)
    }

    /// 清理过期日志（保留最近N天）
    pub async fn cleanup_old_logs(&self, days: i64) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM strategy_job_signal_log
             WHERE created_at < NOW() - ($1::bigint * INTERVAL '1 day')",
        )
        .bind(days)
        .execute(&self.pool)
        .await?;

        info!(
            "清理 {} 天前的信号日志，删除 {} 条",
            days,
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要数据库
    async fn test_save_signal_log() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@127.0.0.1/test")
            .expect("lazy postgres pool");
        let repo = SignalLogRepository::new(pool);
        let signal_json = r#"{"should_buy":true,"should_sell":false,"ts":1234567890}"#;

        let result = repo
            .save_signal_log("BTC-USDT", "1H", "vegas", signal_json)
            .await;

        assert!(result.is_ok());
    }
}
