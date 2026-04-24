//! quant_core.strategy_configs Postgres 仓储实现

use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use rust_quant_domain::traits::StrategyConfigRepository;
use rust_quant_domain::{StrategyConfig, StrategyStatus, StrategyType, Timeframe};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use tracing::debug;

#[derive(Debug, Clone, FromRow)]
struct QuantCoreStrategyConfigRow {
    id: String,
    legacy_id: Option<i64>,
    strategy_key: String,
    exchange: String,
    symbol: String,
    timeframe: String,
    enabled: bool,
    config: Value,
    risk_config: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quant_core_row_maps_exchange_into_domain_strategy_config() {
        let row = QuantCoreStrategyConfigRow {
            id: "6f9619ff-8b86-d011-b42d-00cf4fc964ff".to_string(),
            legacy_id: Some(42),
            strategy_key: "vegas".to_string(),
            exchange: "binance".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            timeframe: "4H".to_string(),
            enabled: true,
            config: json!({"window": 144}),
            risk_config: json!({"max_loss_percent": 0.02}),
        };

        let config = row.to_domain().expect("row should map to domain config");

        assert_eq!(config.exchange.as_deref(), Some("binance"));
        assert_eq!(config.symbol, "ETH-USDT-SWAP");
    }
}

impl QuantCoreStrategyConfigRow {
    fn runtime_id(&self) -> i64 {
        self.legacy_id
            .unwrap_or_else(|| derive_runtime_id_from_uuid(&self.id))
    }

    fn to_domain(&self) -> Result<StrategyConfig> {
        let strategy_type = StrategyType::from_str(&self.strategy_key)
            .map_err(|error| anyhow!("无效的 strategy_key: {} ({})", self.strategy_key, error))?;
        let timeframe = Timeframe::from_str(&self.timeframe)
            .map_err(|error| anyhow!("无效的 timeframe: {} ({})", self.timeframe, error))?;

        let mut config = StrategyConfig::new(
            self.runtime_id(),
            strategy_type,
            self.symbol.clone(),
            timeframe,
            self.config.clone(),
            self.risk_config.clone(),
        );
        config.exchange = normalize_exchange(&self.exchange);

        config.status = if self.enabled {
            StrategyStatus::Running
        } else {
            StrategyStatus::Stopped
        };

        if let (Some(start), Some(end)) = (
            json_i64(&self.config, &["kline_start_time"])
                .or_else(|| json_i64(&self.config, &["_migration", "kline_start_time"])),
            json_i64(&self.config, &["kline_end_time"])
                .or_else(|| json_i64(&self.config, &["_migration", "kline_end_time"])),
        ) {
            config.set_backtest_range(start, end);
        }

        Ok(config)
    }
}

fn json_i64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_i64().or_else(|| current.as_str()?.parse().ok())
}

fn derive_runtime_id_from_uuid(id: &str) -> i64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    1_500_000_000 + (hash % 500_000_000) as i64
}

fn enabled_from_status(status: StrategyStatus) -> bool {
    matches!(status, StrategyStatus::Running)
}

fn normalize_exchange(exchange: &str) -> Option<String> {
    let value = exchange.trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "all" => None,
        _ => Some(value),
    }
}

pub struct PostgresStrategyConfigRepository {
    pool: PgPool,
}

impl PostgresStrategyConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn fetch_by_external_id(&self, id: &str) -> Result<Option<QuantCoreStrategyConfigRow>> {
        debug!("查询 quant_core 策略配置: external_id={}", id);

        sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE id::text = $1
               OR legacy_id::text = $1
            LIMIT 1
            "#,
        )
        .bind(id.trim())
        .fetch_optional(&self.pool)
        .await
        .context("query quant_core strategy_config by external id")
    }

    async fn fetch_by_legacy_id(&self, id: i64) -> Result<Option<QuantCoreStrategyConfigRow>> {
        debug!("查询 quant_core 策略配置: legacy_id={}", id);

        sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE legacy_id = $1
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("query quant_core strategy_config by legacy_id")
    }

    async fn fetch_runtime_rows(&self) -> Result<Vec<QuantCoreStrategyConfigRow>> {
        sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE version NOT LIKE 'legacy-mysql%'
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("query quant_core runtime strategy_configs")
    }

    async fn fetch_by_runtime_id(&self, id: i64) -> Result<Option<QuantCoreStrategyConfigRow>> {
        let rows = self.fetch_runtime_rows().await?;
        Ok(rows.into_iter().find(|row| row.runtime_id() == id))
    }

    async fn update_by_uuid(&self, row_id: &str, config: &StrategyConfig) -> Result<u64> {
        sqlx::query(
            r#"
            UPDATE strategy_configs
            SET strategy_key = $2,
                strategy_name = $3,
                exchange = $4,
                symbol = $5,
                timeframe = $6,
                enabled = $7,
                config = $8,
                risk_config = $9,
                updated_at = NOW()
            WHERE id = $1::uuid
            "#,
        )
        .bind(row_id)
        .bind(config.strategy_type.as_str())
        .bind(config.strategy_type.as_str())
        .bind(config.exchange.as_deref().unwrap_or("all"))
        .bind(&config.symbol)
        .bind(config.timeframe.as_str())
        .bind(enabled_from_status(config.status))
        .bind(&config.parameters)
        .bind(&config.risk_config)
        .execute(&self.pool)
        .await
        .context("update quant_core strategy_config by uuid")
        .map(|result| result.rows_affected())
    }
}

#[async_trait]
impl StrategyConfigRepository for PostgresStrategyConfigRepository {
    async fn find_by_id(&self, id: i64) -> Result<Option<StrategyConfig>> {
        if let Some(row) = self.fetch_by_legacy_id(id).await? {
            return row.to_domain().map(Some);
        }

        self.fetch_by_runtime_id(id)
            .await?
            .map(|row| row.to_domain())
            .transpose()
    }

    async fn find_by_external_id(&self, id: &str) -> Result<Option<StrategyConfig>> {
        self.fetch_by_external_id(id)
            .await?
            .map(|row| row.to_domain())
            .transpose()
    }

    async fn find_all_enabled(&self) -> Result<Vec<StrategyConfig>> {
        let rows = sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE enabled = true
              AND version NOT LIKE 'legacy-mysql%'
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("query enabled quant_core strategy_configs")?;

        rows.into_iter().map(|row| row.to_domain()).collect()
    }

    async fn find_by_symbol_and_timeframe(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Vec<StrategyConfig>> {
        let rows = sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE enabled = true
              AND version NOT LIKE 'legacy-mysql%'
              AND symbol = $1
              AND timeframe = $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(symbol)
        .bind(timeframe.as_str())
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!(
                "query quant_core strategy_configs by symbol/timeframe: {}@{}",
                symbol,
                timeframe.as_str()
            )
        })?;

        rows.into_iter().map(|row| row.to_domain()).collect()
    }

    async fn save(&self, config: &StrategyConfig) -> Result<i64> {
        sqlx::query(
            r#"
            INSERT INTO strategy_configs (
                legacy_id,
                strategy_key,
                strategy_name,
                version,
                exchange,
                symbol,
                timeframe,
                enabled,
                config,
                risk_config
            )
            VALUES ($1, $2, $3, 'default', $4, $5, $6, $7, $8, $9)
            ON CONFLICT (strategy_key, version, exchange, symbol, timeframe)
            DO UPDATE SET
                legacy_id = EXCLUDED.legacy_id,
                strategy_name = EXCLUDED.strategy_name,
                enabled = EXCLUDED.enabled,
                config = EXCLUDED.config,
                risk_config = EXCLUDED.risk_config,
                updated_at = NOW()
            "#,
        )
        .bind(config.id)
        .bind(config.strategy_type.as_str())
        .bind(config.strategy_type.as_str())
        .bind(config.exchange.as_deref().unwrap_or("all"))
        .bind(&config.symbol)
        .bind(config.timeframe.as_str())
        .bind(enabled_from_status(config.status))
        .bind(&config.parameters)
        .bind(&config.risk_config)
        .execute(&self.pool)
        .await
        .context("upsert quant_core strategy_config")?;

        Ok(config.id)
    }

    async fn update(&self, config: &StrategyConfig) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE strategy_configs
            SET strategy_key = $2,
                strategy_name = $3,
                exchange = $4,
                symbol = $5,
                timeframe = $6,
                enabled = $7,
                config = $8,
                risk_config = $9,
                updated_at = NOW()
            WHERE legacy_id = $1
            "#,
        )
        .bind(config.id)
        .bind(config.strategy_type.as_str())
        .bind(config.strategy_type.as_str())
        .bind(config.exchange.as_deref().unwrap_or("all"))
        .bind(&config.symbol)
        .bind(config.timeframe.as_str())
        .bind(enabled_from_status(config.status))
        .bind(&config.parameters)
        .bind(&config.risk_config)
        .execute(&self.pool)
        .await
        .context("update quant_core strategy_config")?;

        if result.rows_affected() == 0 {
            if let Some(row) = self.fetch_by_runtime_id(config.id).await? {
                if self.update_by_uuid(&row.id, config).await? > 0 {
                    return Ok(());
                }
            }

            return Err(anyhow!("策略配置不存在: {}", config.id));
        }

        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE strategy_configs
            SET enabled = false,
                updated_at = NOW()
            WHERE legacy_id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("disable quant_core strategy_config")?;

        if result.rows_affected() == 0 {
            if let Some(row) = self.fetch_by_runtime_id(id).await? {
                sqlx::query(
                    r#"
                    UPDATE strategy_configs
                    SET enabled = false,
                        updated_at = NOW()
                    WHERE id = $1::uuid
                    "#,
                )
                .bind(&row.id)
                .execute(&self.pool)
                .await
                .context("disable quant_core strategy_config by uuid")?;
            }
        }

        Ok(())
    }
}
