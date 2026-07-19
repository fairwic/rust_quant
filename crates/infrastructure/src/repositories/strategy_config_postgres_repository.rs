//! quant_core.strategy_configs Postgres 仓储实现
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use rust_quant_domain::traits::StrategyConfigRepository;
use rust_quant_domain::{StrategyConfig, StrategyStatus, StrategyType, Timeframe};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use std::str::FromStr;
use tracing::debug;
#[derive(Debug, Clone, FromRow)]
struct QuantCoreStrategyConfigRow {
    /// 唯一标识。
    id: String,
    /// legacy ID；为空时使用默认值或表示不限制。
    legacy_id: Option<i64>,
    /// 策略Key，用于展示或持久化查询结果。
    strategy_key: String,
    /// 不可变策略版本。
    version: String,
    /// 交易所名称。
    exchange: String,
    /// 交易对或资产符号。
    symbol: String,
    /// 周期。
    timeframe: String,
    /// 是否启用。
    enabled: bool,
    /// 运行配置。
    config: Value,
    /// 配置项。
    risk_config: Value,
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    /// 提供quantcore数据行maps交易所intodomain策略配置的集中实现，避免回测策略调用方重复处理相同细节。
    fn quant_core_row_maps_exchange_into_domain_strategy_config() {
        let row = QuantCoreStrategyConfigRow {
            id: "6f9619ff-8b86-d011-b42d-00cf4fc964ff".to_string(),
            legacy_id: Some(42),
            strategy_key: "vegas".to_string(),
            version: "eth_4h_v2".to_string(),
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
        assert_eq!(config.version, "eth_4h_v2");
    }

    #[test]
    /// quant_core.strategy_configs 顶层 strategy_key 是 live 子策略入口，需注入 parameters 供执行器选择 preset。
    fn quant_core_row_injects_top_level_strategy_key_into_parameters() {
        let row = QuantCoreStrategyConfigRow {
            id: "6f9619ff-8b86-d011-b42d-00cf4fc964ff".to_string(),
            legacy_id: Some(43),
            strategy_key: "exhaustion_fade_short_v1".to_string(),
            version: "v1".to_string(),
            exchange: "binance".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: "5m".to_string(),
            enabled: true,
            config: json!({"thresholds": {"exhaustion_min_oi_growth_pct": 0.7}}),
            risk_config: json!({"max_loss_percent": 0.01}),
        };
        let config = row.to_domain().expect("row should map to domain config");

        assert_eq!(config.strategy_type, StrategyType::BearShortStack);
        assert_eq!(
            config.parameters["strategy_key"],
            "exhaustion_fade_short_v1"
        );
        assert_eq!(
            config.parameters["thresholds"]["exhaustion_min_oi_growth_pct"],
            0.7
        );
    }
}
impl QuantCoreStrategyConfigRow {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    fn runtime_id(&self) -> i64 {
        self.legacy_id
            .unwrap_or_else(|| derive_runtime_id_from_uuid(&self.id))
    }
    /// 将内部模型转换为输出结构，避免 回测与策略研究 的内部字段直接外泄。
    fn to_domain(&self) -> Result<StrategyConfig> {
        let strategy_type = StrategyType::from_str(&self.strategy_key)
            .map_err(|error| anyhow!("无效的 strategy_key: {} ({})", self.strategy_key, error))?;
        let timeframe = Timeframe::from_str(&self.timeframe)
            .map_err(|error| anyhow!("无效的 timeframe: {} ({})", self.timeframe, error))?;
        let mut parameters = self.config.clone();
        if let Value::Object(fields) = &mut parameters {
            fields
                .entry("strategy_key".to_string())
                .or_insert_with(|| Value::String(self.strategy_key.clone()));
        }
        let mut config = StrategyConfig::new(
            self.runtime_id(),
            strategy_type,
            self.symbol.clone(),
            timeframe,
            parameters,
            self.risk_config.clone(),
        );
        config.exchange = normalize_exchange(&self.exchange);
        config.version = self.version.clone();
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
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn json_i64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_i64().or_else(|| current.as_str()?.parse().ok())
}
/// 计算 回测与策略研究 指标，保持公式和边界处理集中可审计。
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
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn normalize_exchange(exchange: &str) -> Option<String> {
    let value = exchange.trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "all" => None,
        _ => Some(value),
    }
}
pub struct PostgresStrategyConfigRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl PostgresStrategyConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_by_external_id(&self, id: &str) -> Result<Option<QuantCoreStrategyConfigRow>> {
        debug!("查询 quant_core 策略配置: external_id={}", id);
        sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, version, exchange, symbol, timeframe, enabled, config, risk_config
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_by_legacy_id(&self, id: i64) -> Result<Option<QuantCoreStrategyConfigRow>> {
        debug!("查询 quant_core 策略配置: legacy_id={}", id);
        sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, version, exchange, symbol, timeframe, enabled, config, risk_config
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_runtime_rows(&self) -> Result<Vec<QuantCoreStrategyConfigRow>> {
        sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, version, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("query quant_core runtime strategy_configs")
    }
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_by_runtime_id(&self, id: i64) -> Result<Option<QuantCoreStrategyConfigRow>> {
        let rows = self.fetch_runtime_rows().await?;
        Ok(rows.into_iter().find(|row| row.runtime_id() == id))
    }
    /// 更新 回测与策略研究 状态，并保留调用方需要的结果或错误信息。
    async fn update_by_uuid(&self, row_id: &str, config: &StrategyConfig) -> Result<u64> {
        sqlx::query(
            r#"
            UPDATE strategy_configs
            SET strategy_key = $2,
                strategy_name = $3,
                version = $4,
                exchange = $5,
                symbol = $6,
                timeframe = $7,
                enabled = $8,
                config = $9,
                risk_config = $10,
                updated_at = NOW()
            WHERE id = $1::uuid
            "#,
        )
        .bind(row_id)
        .bind(config.strategy_type.as_str())
        .bind(config.strategy_type.as_str())
        .bind(&config.version)
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_id(&self, id: i64) -> Result<Option<StrategyConfig>> {
        if let Some(row) = self.fetch_by_legacy_id(id).await? {
            return row.to_domain().map(Some);
        }
        self.fetch_by_runtime_id(id)
            .await?
            .map(|row| row.to_domain())
            .transpose()
    }
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_external_id(&self, id: &str) -> Result<Option<StrategyConfig>> {
        self.fetch_by_external_id(id)
            .await?
            .map(|row| row.to_domain())
            .transpose()
    }
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_all_enabled(&self) -> Result<Vec<StrategyConfig>> {
        let rows = sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, version, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE enabled = true
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("query enabled quant_core strategy_configs")?;
        rows.into_iter().map(|row| row.to_domain()).collect()
    }
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_symbol_and_timeframe(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Vec<StrategyConfig>> {
        let rows = sqlx::query_as::<_, QuantCoreStrategyConfigRow>(
            r#"
            SELECT id::text AS id, legacy_id, strategy_key, version, exchange, symbol, timeframe, enabled, config, risk_config
            FROM strategy_configs
            WHERE enabled = true
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
    /// 提供save的集中实现，避免回测策略调用方重复处理相同细节。
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
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
        .bind(&config.version)
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
    /// 执行更新步骤，串起回测策略需要的状态推进和错误处理。
    async fn update(&self, config: &StrategyConfig) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE strategy_configs
            SET strategy_key = $2,
                strategy_name = $3,
                version = $4,
                exchange = $5,
                symbol = $6,
                timeframe = $7,
                enabled = $8,
                config = $9,
                risk_config = $10,
                updated_at = NOW()
            WHERE legacy_id = $1
            "#,
        )
        .bind(config.id)
        .bind(config.strategy_type.as_str())
        .bind(config.strategy_type.as_str())
        .bind(&config.version)
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
    /// 提供delete的集中实现，避免回测策略调用方重复处理相同细节。
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
