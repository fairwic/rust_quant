//! 策略信号日志仓储
//!
//! 记录策略产生的交易信号

use anyhow::Result;
use rust_quant_core::database::get_db_pool;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
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
pub struct SignalLogRepository;

impl SignalLogRepository {
    pub fn new() -> Self {
        Self
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
        let pool = get_db_pool();

        let result = sqlx::query!(
            "INSERT INTO strategy_job_signal_log (inst_id, time, strategy_type, strategy_result)
             VALUES (?, ?, ?, ?)",
            inst_id,
            period,
            strategy_type,
            signal_json
        )
        .execute(pool)
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
        let pool = get_db_pool();

        let signals = sqlx::query_as!(
            SignalLogEntity,
            "SELECT * FROM strategy_job_signal_log
             WHERE inst_id = ? AND time = ?
             ORDER BY created_at DESC
             LIMIT ?",
            inst_id,
            period,
            limit as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(signals)
    }

    /// 查询所有信号日志
    pub async fn find_all(&self, limit: Option<usize>) -> Result<Vec<SignalLogEntity>> {
        let pool = get_db_pool();

        let limit = match limit {
            Some(v) => v,
            None => 100,
        };
        let signals = sqlx::query_as!(
            SignalLogEntity,
            "SELECT * FROM strategy_job_signal_log
             ORDER BY created_at DESC
             LIMIT ?",
            limit as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(signals)
    }

    /// 清理过期日志（保留最近N天）
    pub async fn cleanup_old_logs(&self, days: i64) -> Result<u64> {
        let pool = get_db_pool();

        let result = sqlx::query!(
            "DELETE FROM strategy_job_signal_log
             WHERE created_at < DATE_SUB(NOW(), INTERVAL ? DAY)",
            days
        )
        .execute(pool)
        .await?;

        info!(
            "清理 {} 天前的信号日志，删除 {} 条",
            days,
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }
}

impl Default for SignalLogRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要数据库
    async fn test_save_signal_log() {
        let repo = SignalLogRepository::new();
        let signal_json = r#"{"should_buy":true,"should_sell":false,"ts":1234567890}"#;

        let result = repo
            .save_signal_log("BTC-USDT", "1H", "vegas", signal_json)
            .await;

        assert!(result.is_ok());
    }
}
