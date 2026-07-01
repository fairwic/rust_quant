use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use serde_json::Value;
use sqlx::{types::Json, FromRow, PgPool};
use std::collections::BTreeMap;
use tracing::error;
#[derive(Debug, Clone, FromRow)]
struct ExternalMarketSnapshotEntity {
    /// 唯一标识。
    pub id: i64,
    /// 数据来源。
    pub source: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub metric_type: String,
    /// 时间字段。
    pub metric_time: i64,
    /// 资金费率；为空时使用默认值或表示不限制。
    pub funding_rate: Option<String>,
    /// 溢价率；为空时表示交易所未返回该指标。
    pub premium: Option<String>,
    /// 未平仓量；为空时表示交易所未返回该指标。
    pub open_interest: Option<String>,
    /// 价格数值。
    pub oracle_price: Option<String>,
    /// 价格数值。
    pub mark_price: Option<String>,
    /// longshort 比例；为空时使用默认值或表示不限制。
    pub long_short_ratio: Option<String>,
    /// 原始 payload；为空时表示没有保留原始响应。
    pub raw_payload: Option<Json<Value>>,
    /// 创建时间。
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后更新时间。
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl ExternalMarketSnapshotEntity {
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    fn to_domain(&self) -> ExternalMarketSnapshot {
        ExternalMarketSnapshot {
            id: Some(self.id),
            source: self.source.clone(),
            symbol: self.symbol.clone(),
            metric_type: self.metric_type.clone(),
            metric_time: self.metric_time,
            funding_rate: self.funding_rate.as_deref().and_then(|v| v.parse().ok()),
            premium: self.premium.as_deref().and_then(|v| v.parse().ok()),
            open_interest: self.open_interest.as_deref().and_then(|v| v.parse().ok()),
            oracle_price: self.oracle_price.as_deref().and_then(|v| v.parse().ok()),
            mark_price: self.mark_price.as_deref().and_then(|v| v.parse().ok()),
            long_short_ratio: self
                .long_short_ratio
                .as_deref()
                .and_then(|v| v.parse().ok()),
            raw_payload: self.raw_payload.clone().map(|json| json.0),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
pub struct SqlxExternalMarketSnapshotRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxExternalMarketSnapshotRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
/// Stores external market context in one physical table per source/symbol/metric.
pub struct ShardedExternalMarketSnapshotRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl ShardedExternalMarketSnapshotRepository {
    /// Creates a repository over the quant_core Postgres pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    /// Returns a safely quoted table name for dynamic SQL against sharded context tables.
    pub fn quoted_table_name(source: &str, symbol: &str, metric_type: &str) -> Result<String> {
        Ok(Self::quote_identifier(&Self::table_name(
            source,
            symbol,
            metric_type,
        )?))
    }

    /// Reads an existing shard without creating it, so live preflight can fail closed on missing data.
    pub async fn find_range_existing(
        &self,
        source: &str,
        symbol: &str,
        metric_type: &str,
        start_time: i64,
        end_time: i64,
        limit: Option<i64>,
    ) -> Result<Vec<ExternalMarketSnapshot>> {
        let raw_table_name = Self::table_name(source, symbol, metric_type)?;
        let existed = sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NOT NULL")
            .bind(&raw_table_name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("检查市场上下文分表是否存在失败: {}", e);
                anyhow!("检查市场上下文分表是否存在失败: {}", e)
            })?;
        if !existed {
            return Ok(Vec::new());
        }
        let table_name = Self::quote_identifier(&raw_table_name);
        let limit = limit.unwrap_or(500);
        let query = format!(
            r#"
            SELECT *
            FROM {}
            WHERE source = $1
              AND symbol = $2
              AND metric_type = $3
              AND metric_time >= $4
              AND metric_time <= $5
            ORDER BY metric_time ASC
            LIMIT $6
            "#,
            table_name
        );
        let rows = sqlx::query_as::<_, ExternalMarketSnapshotEntity>(&query)
            .bind(source)
            .bind(symbol)
            .bind(metric_type)
            .bind(start_time)
            .bind(end_time)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询市场上下文分表快照失败: {}", e);
                anyhow!("查询市场上下文分表快照失败: {}", e)
            })?;
        Ok(rows.into_iter().map(|row| row.to_domain()).collect())
    }

    fn table_name(source: &str, symbol: &str, metric_type: &str) -> Result<String> {
        let table_name = format!(
            "{}_{}_{}_market_snapshots",
            Self::table_part(source, "source")?,
            Self::table_part(symbol, "symbol")?,
            Self::table_part(metric_type, "metric_type")?
        );
        if table_name.len() > 63 {
            return Err(anyhow!("市场上下文分表名过长: {}", table_name));
        }
        Ok(table_name)
    }
    fn quote_identifier(table_name: &str) -> String {
        format!("\"{}\"", table_name)
    }
    fn table_part(value: &str, field: &str) -> Result<String> {
        let mut normalized = String::new();
        let mut last_underscore = false;
        for ch in value.trim().to_ascii_lowercase().chars() {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                normalized.push(ch);
                last_underscore = false;
            } else if !last_underscore {
                normalized.push('_');
                last_underscore = true;
            }
        }
        let normalized = normalized.trim_matches('_').to_string();
        if normalized.is_empty() {
            return Err(anyhow!("非法市场上下文分表{}: {}", field, value));
        }
        Ok(normalized)
    }
    async fn ensure_table(&self, source: &str, symbol: &str, metric_type: &str) -> Result<String> {
        let raw_table_name = Self::table_name(source, symbol, metric_type)?;
        let existed = sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NOT NULL")
            .bind(&raw_table_name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("检查市场上下文分表是否存在失败: {}", e);
                anyhow!("检查市场上下文分表是否存在失败: {}", e)
            })?;
        let table_name = Self::quote_identifier(&raw_table_name);
        let create_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id BIGSERIAL PRIMARY KEY,
                source VARCHAR(64) NOT NULL,
                symbol VARCHAR(64) NOT NULL,
                metric_type VARCHAR(96) NOT NULL,
                metric_time BIGINT NOT NULL,
                funding_rate VARCHAR(64),
                premium VARCHAR(64),
                open_interest VARCHAR(64),
                oracle_price VARCHAR(64),
                mark_price VARCHAR(64),
                long_short_ratio VARCHAR(64),
                raw_payload JSONB,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE (source, symbol, metric_type, metric_time)
            )
            "#,
            table_name
        );
        sqlx::query(&create_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("创建市场上下文分表失败: {}", e);
                anyhow!("创建市场上下文分表失败: {}", e)
            })?;
        if !existed {
            self.comment_table(&table_name).await?;
        }
        Ok(table_name)
    }
    /// Writes table and column comments only once when a new shard is created.
    async fn comment_table(&self, table_name: &str) -> Result<()> {
        let table_comment_sql = format!("COMMENT ON TABLE {} IS '市场上下文快照分表'", table_name);
        sqlx::query(&table_comment_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("写入市场上下文分表注释失败: {}", e);
                anyhow!("写入市场上下文分表注释失败: {}", e)
            })?;
        for (column, comment) in [
            ("id", "主键ID"),
            ("source", "数据来源"),
            ("symbol", "交易标的"),
            ("metric_type", "指标类型"),
            ("metric_time", "指标时间戳"),
            ("funding_rate", "资金费率"),
            ("premium", "溢价率"),
            ("open_interest", "未平仓量"),
            ("oracle_price", "预言机价格"),
            ("mark_price", "标记价格"),
            ("long_short_ratio", "多空比"),
            ("raw_payload", "原始响应"),
            ("created_at", "创建时间"),
            ("updated_at", "更新时间"),
        ] {
            let column_comment_sql = format!(
                "COMMENT ON COLUMN {}.{} IS '{}'",
                table_name, column, comment
            );
            sqlx::query(&column_comment_sql)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    error!("写入市场上下文分表字段注释失败: {}", e);
                    anyhow!("写入市场上下文分表字段注释失败: {}", e)
                })?;
        }
        Ok(())
    }
    async fn insert_snapshot(
        &self,
        table_name: &str,
        snapshot: ExternalMarketSnapshot,
    ) -> Result<()> {
        let query = format!(
            r#"
            INSERT INTO {} (
                source, symbol, metric_type, metric_time, funding_rate, premium, open_interest,
                oracle_price, mark_price, long_short_ratio, raw_payload
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (source, symbol, metric_type, metric_time) DO UPDATE SET
                funding_rate = EXCLUDED.funding_rate,
                premium = EXCLUDED.premium,
                open_interest = EXCLUDED.open_interest,
                oracle_price = EXCLUDED.oracle_price,
                mark_price = EXCLUDED.mark_price,
                long_short_ratio = EXCLUDED.long_short_ratio,
                raw_payload = EXCLUDED.raw_payload,
                updated_at = CURRENT_TIMESTAMP
            "#,
            table_name
        );
        sqlx::query(&query)
            .bind(snapshot.source)
            .bind(snapshot.symbol)
            .bind(snapshot.metric_type)
            .bind(snapshot.metric_time)
            .bind(snapshot.funding_rate.map(|v| v.to_string()))
            .bind(snapshot.premium.map(|v| v.to_string()))
            .bind(snapshot.open_interest.map(|v| v.to_string()))
            .bind(snapshot.oracle_price.map(|v| v.to_string()))
            .bind(snapshot.mark_price.map(|v| v.to_string()))
            .bind(snapshot.long_short_ratio.map(|v| v.to_string()))
            .bind(snapshot.raw_payload.map(Json))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("保存市场上下文分表快照失败: {}", e);
                anyhow!("保存市场上下文分表快照失败: {}", e)
            })?;
        Ok(())
    }
}
#[async_trait]
impl ExternalMarketSnapshotRepository for SqlxExternalMarketSnapshotRepository {
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn save(&self, snapshot: ExternalMarketSnapshot) -> Result<()> {
        let query = r#"
            INSERT INTO external_market_snapshots (
                source, symbol, metric_type, metric_time, funding_rate, premium, open_interest,
                oracle_price, mark_price, long_short_ratio, raw_payload
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (source, symbol, metric_type, metric_time) DO UPDATE SET
                funding_rate = EXCLUDED.funding_rate,
                premium = EXCLUDED.premium,
                open_interest = EXCLUDED.open_interest,
                oracle_price = EXCLUDED.oracle_price,
                mark_price = EXCLUDED.mark_price,
                long_short_ratio = EXCLUDED.long_short_ratio,
                raw_payload = EXCLUDED.raw_payload,
                updated_at = CURRENT_TIMESTAMP
        "#;
        sqlx::query(query)
            .bind(snapshot.source)
            .bind(snapshot.symbol)
            .bind(snapshot.metric_type)
            .bind(snapshot.metric_time)
            .bind(snapshot.funding_rate.map(|v| v.to_string()))
            .bind(snapshot.premium.map(|v| v.to_string()))
            .bind(snapshot.open_interest.map(|v| v.to_string()))
            .bind(snapshot.oracle_price.map(|v| v.to_string()))
            .bind(snapshot.mark_price.map(|v| v.to_string()))
            .bind(snapshot.long_short_ratio.map(|v| v.to_string()))
            .bind(snapshot.raw_payload.map(Json))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("保存外部市场快照失败: {}", e);
                anyhow!("保存外部市场快照失败: {}", e)
            })?;
        Ok(())
    }
    /// 持久化 行情与市场数据 结果，保证写入路径和幂等语义集中处理。
    async fn save_batch(&self, snapshots: Vec<ExternalMarketSnapshot>) -> Result<()> {
        for snapshot in snapshots {
            self.save(snapshot).await?;
        }
        Ok(())
    }
    /// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
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
            WHERE source = $1
              AND symbol = $2
              AND metric_type = $3
              AND metric_time >= $4
              AND metric_time <= $5
            ORDER BY metric_time ASC
            LIMIT $6
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
        Ok(rows.into_iter().map(|row| row.to_domain()).collect())
    }
}
#[async_trait]
impl ExternalMarketSnapshotRepository for ShardedExternalMarketSnapshotRepository {
    async fn save(&self, snapshot: ExternalMarketSnapshot) -> Result<()> {
        let table_name = self
            .ensure_table(&snapshot.source, &snapshot.symbol, &snapshot.metric_type)
            .await?;
        self.insert_snapshot(&table_name, snapshot).await
    }
    async fn save_batch(&self, snapshots: Vec<ExternalMarketSnapshot>) -> Result<()> {
        let mut by_table = BTreeMap::<(String, String, String), Vec<ExternalMarketSnapshot>>::new();
        for snapshot in snapshots {
            by_table
                .entry((
                    snapshot.source.clone(),
                    snapshot.symbol.clone(),
                    snapshot.metric_type.clone(),
                ))
                .or_default()
                .push(snapshot);
        }
        for ((source, symbol, metric_type), table_snapshots) in by_table {
            let table_name = self.ensure_table(&source, &symbol, &metric_type).await?;
            for snapshot in table_snapshots {
                self.insert_snapshot(&table_name, snapshot).await?;
            }
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
        let table_name = self.ensure_table(source, symbol, metric_type).await?;
        let limit = limit.unwrap_or(500);
        let query = format!(
            r#"
            SELECT *
            FROM {}
            WHERE source = $1
              AND symbol = $2
              AND metric_type = $3
              AND metric_time >= $4
              AND metric_time <= $5
            ORDER BY metric_time ASC
            LIMIT $6
            "#,
            table_name
        );
        let rows = sqlx::query_as::<_, ExternalMarketSnapshotEntity>(&query)
            .bind(source)
            .bind(symbol)
            .bind(metric_type)
            .bind(start_time)
            .bind(end_time)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询市场上下文分表快照失败: {}", e);
                anyhow!("查询市场上下文分表快照失败: {}", e)
            })?;
        Ok(rows.into_iter().map(|row| row.to_domain()).collect())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn sharded_external_market_snapshot_table_name_uses_source_symbol_and_metric() {
        let table = ShardedExternalMarketSnapshotRepository::quoted_table_name(
            "okx",
            "BTC-USDT-SWAP",
            "funding_rate",
        )
        .expect("valid market context table name");
        assert_eq!(table, "\"okx_btc_usdt_swap_funding_rate_market_snapshots\"");
    }
    #[test]
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn test_entity_to_domain_can_be_called_without_consuming_entity() {
        let entity = ExternalMarketSnapshotEntity {
            id: 1,
            source: "hyperliquid".to_string(),
            symbol: "ETH".to_string(),
            metric_type: "funding".to_string(),
            metric_time: 1_744_000_000_000,
            funding_rate: Some("0.0001".to_string()),
            premium: Some("0.001".to_string()),
            open_interest: Some("1234.0".to_string()),
            oracle_price: Some("2000.0".to_string()),
            mark_price: Some("2001.0".to_string()),
            long_short_ratio: Some("1.2".to_string()),
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
