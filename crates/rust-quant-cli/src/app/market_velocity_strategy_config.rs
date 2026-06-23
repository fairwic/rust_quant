use anyhow::{anyhow, Context, Result};
use rust_quant_services::market::MarketVelocityStrategySignalConfig;
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use tracing::info;
const MARKET_VELOCITY_STRATEGY_KEY: &str = "market_velocity";
const MARKET_VELOCITY_CONFIG_ID_ENV: &str = "MARKET_VELOCITY_SIGNAL_STRATEGY_CONFIG_ID";
#[derive(Debug, FromRow)]
struct MarketVelocityStrategyConfigRow {
    /// 唯一标识。
    id: String,
    /// legacy ID；为空时使用默认值或表示不限制。
    legacy_id: Option<i64>,
    /// version，用于展示或持久化查询结果。
    version: String,
    /// 交易所名称。
    exchange: String,
    /// 交易对或资产符号。
    symbol: String,
    /// 周期。
    timeframe: String,
    /// 运行配置。
    config: Value,
    /// 配置项。
    risk_config: Value,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
pub async fn load_market_velocity_signal_config_or_env(
    pool: &PgPool,
) -> Result<MarketVelocityStrategySignalConfig> {
    if let Some(config) = load_market_velocity_signal_config_from_strategy_configs(pool).await? {
        return Ok(config);
    }
    MarketVelocityStrategySignalConfig::from_env()
        .context("load Market Velocity signal config from env/default fallback")
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn load_market_velocity_signal_config_from_strategy_configs(
    pool: &PgPool,
) -> Result<Option<MarketVelocityStrategySignalConfig>> {
    let selected_id = std::env::var(MARKET_VELOCITY_CONFIG_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let row = match selected_id.as_deref() {
        Some(config_id) => fetch_market_velocity_signal_config_by_id(pool, config_id)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "{MARKET_VELOCITY_CONFIG_ID_ENV}={} does not reference an enabled market_velocity strategy_config",
                    config_id
                )
            })?,
        None => match fetch_default_market_velocity_signal_config(pool).await? {
            Some(row) => row,
            None => {
                info!(
                    "Market Velocity strategy_config not found in quant_core.strategy_configs; using env/default config"
                );
                return Ok(None);
            }
        },
    };
    let config = MarketVelocityStrategySignalConfig::from_strategy_config_json(
        &row.config,
        &row.risk_config,
    )
    .with_context(|| {
        format!(
            "parse market_velocity strategy_config id={} legacy_id={:?} version={} exchange={} symbol={} timeframe={}",
            row.id, row.legacy_id, row.version, row.exchange, row.symbol, row.timeframe
        )
    })?;
    info!(
        "Market Velocity signal config loaded from quant_core.strategy_configs: id={}, legacy_id={:?}, version={}, exchange={}, symbol={}, timeframe={}",
        row.id, row.legacy_id, row.version, row.exchange, row.symbol, row.timeframe
    );
    Ok(Some(config))
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn fetch_market_velocity_signal_config_by_id(
    pool: &PgPool,
    config_id: &str,
) -> Result<Option<MarketVelocityStrategyConfigRow>> {
    sqlx::query_as::<_, MarketVelocityStrategyConfigRow>(
        r#"
        SELECT id::text AS id, legacy_id, version, exchange, symbol, timeframe, config, risk_config
        FROM strategy_configs
        WHERE enabled = true
          AND strategy_key = $1
          AND (id::text = $2 OR legacy_id::text = $2)
        LIMIT 1
        "#,
    )
    .bind(MARKET_VELOCITY_STRATEGY_KEY)
    .bind(config_id)
    .fetch_optional(pool)
    .await
    .context("query selected market_velocity strategy_config")
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn fetch_default_market_velocity_signal_config(
    pool: &PgPool,
) -> Result<Option<MarketVelocityStrategyConfigRow>> {
    sqlx::query_as::<_, MarketVelocityStrategyConfigRow>(
        r#"
        SELECT id::text AS id, legacy_id, version, exchange, symbol, timeframe, config, risk_config
        FROM strategy_configs
        WHERE enabled = true
          AND strategy_key = $1
          AND lower(exchange) IN ('okx', 'all')
          AND upper(symbol) = 'ALL'
          AND timeframe IN ('15m', '')
        ORDER BY
          CASE lower(exchange) WHEN 'okx' THEN 0 WHEN 'all' THEN 1 ELSE 2 END,
          CASE timeframe WHEN '15m' THEN 0 ELSE 1 END,
          CASE version WHEN 'default' THEN 0 ELSE 1 END,
          updated_at DESC,
          created_at DESC
        LIMIT 1
        "#,
    )
    .bind(MARKET_VELOCITY_STRATEGY_KEY)
    .fetch_optional(pool)
    .await
    .context("query default market_velocity strategy_config")
}
