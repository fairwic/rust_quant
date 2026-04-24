use anyhow::{Context, Result};
use rust_quant_domain::traits::StrategyConfigRepository;
use rust_quant_domain::{StrategyType, Timeframe};
use rust_quant_infrastructure::repositories::PostgresStrategyConfigRepository;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn loads_enabled_strategy_configs_from_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!(
            "skipping quant_core strategy_config smoke; set QUANT_CORE_STRATEGY_CONFIG_SMOKE=1 and QUANT_CORE_DATABASE_URL"
        );
        return Ok(());
    }

    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .context("QUANT_CORE_DATABASE_URL is required for quant_core strategy config smoke")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = PostgresStrategyConfigRepository::new(pool.clone());

    let legacy_id = unique_legacy_id()?;
    let symbol = format!("TDD{}-USDT-SWAP", legacy_id % 100_000);
    delete_test_row(&pool, legacy_id).await?;

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
        VALUES ($1, 'vegas', 'Vegas smoke', 'test', 'binance', $2, '4H', true, $3, $4)
        "#,
    )
    .bind(legacy_id)
    .bind(&symbol)
    .bind(json!({
        "period": "4H",
        "ema_signal": {"is_open": true},
        "source": "quant_core_strategy_config_repository_test"
    }))
    .bind(json!({
        "max_loss_percent": 0.04,
        "atr_take_profit_ratio": 3.44
    }))
    .execute(&pool)
    .await
    .context("insert test strategy_config row")?;

    let loaded = repository
        .find_by_id(legacy_id)
        .await?
        .context("strategy config should be found by legacy_id")?;
    assert_eq!(loaded.id, legacy_id);
    assert_eq!(loaded.strategy_type, StrategyType::Vegas);
    assert_eq!(loaded.symbol, symbol);
    assert_eq!(loaded.timeframe, Timeframe::H4);
    assert_eq!(loaded.parameters["period"], "4H");
    assert_eq!(loaded.risk_config["max_loss_percent"], 0.04);

    let by_symbol = repository
        .find_by_symbol_and_timeframe(&symbol, Timeframe::H4)
        .await?;
    assert_eq!(by_symbol.len(), 1);
    assert_eq!(by_symbol[0].id, legacy_id);

    let all_enabled = repository.find_all_enabled().await?;
    assert!(all_enabled.iter().any(|config| config.id == legacy_id));

    delete_test_row(&pool, legacy_id).await?;
    Ok(())
}

#[tokio::test]
async fn loads_runtime_strategy_configs_without_legacy_id_from_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!(
            "skipping quant_core runtime strategy_config smoke; set QUANT_CORE_STRATEGY_CONFIG_SMOKE=1 and QUANT_CORE_DATABASE_URL"
        );
        return Ok(());
    }

    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .context("QUANT_CORE_DATABASE_URL is required for quant_core strategy config smoke")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = PostgresStrategyConfigRepository::new(pool.clone());

    let suffix = unique_legacy_id()?;
    let version = format!("test-runtime-{}", suffix);
    let symbol = format!("RT{}-USDT-SWAP", suffix % 100_000);
    delete_test_rows_by_version(&pool, &version).await?;

    sqlx::query(
        r#"
        INSERT INTO strategy_configs (
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
        VALUES ('vegas', 'Vegas runtime smoke', $1, 'binance', $2, '4H', true, $3, $4)
        "#,
    )
    .bind(&version)
    .bind(&symbol)
    .bind(json!({
        "period": "4H",
        "ema_signal": {"is_open": true},
        "source": "quant_core_runtime_strategy_config_repository_test"
    }))
    .bind(json!({
        "max_loss_percent": 0.035,
        "atr_take_profit_ratio": 5.9
    }))
    .execute(&pool)
    .await
    .context("insert runtime strategy_config row")?;

    let by_symbol = repository
        .find_by_symbol_and_timeframe(&symbol, Timeframe::H4)
        .await?;
    assert_eq!(by_symbol.len(), 1);

    let loaded = &by_symbol[0];
    assert!(loaded.id > 0);
    assert_eq!(loaded.strategy_type, StrategyType::Vegas);
    assert_eq!(loaded.symbol, symbol);
    assert_eq!(loaded.timeframe, Timeframe::H4);
    assert_eq!(loaded.parameters["period"], "4H");
    assert_eq!(loaded.risk_config["atr_take_profit_ratio"], 5.9);

    let reloaded = repository
        .find_by_id(loaded.id)
        .await?
        .context("runtime strategy config should be found by derived id")?;
    assert_eq!(reloaded.id, loaded.id);
    assert_eq!(reloaded.symbol, loaded.symbol);
    assert_eq!(reloaded.timeframe, loaded.timeframe);

    delete_test_rows_by_version(&pool, &version).await?;
    Ok(())
}

fn smoke_enabled() -> bool {
    env::var("QUANT_CORE_STRATEGY_CONFIG_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn unique_legacy_id() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis() as i64)
}

async fn delete_test_row(pool: &sqlx::PgPool, legacy_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM strategy_configs WHERE legacy_id = $1")
        .bind(legacy_id)
        .execute(pool)
        .await
        .context("delete test strategy_config row")?;
    Ok(())
}

async fn delete_test_rows_by_version(pool: &sqlx::PgPool, version: &str) -> Result<()> {
    sqlx::query("DELETE FROM strategy_configs WHERE version = $1")
        .bind(version)
        .execute(pool)
        .await
        .context("delete test strategy_config rows by version")?;
    Ok(())
}
