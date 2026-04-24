use anyhow::{Context, Result};
use rust_quant_domain::entities::ExchangeSymbol;
use rust_quant_domain::traits::ExchangeSymbolRepository;
use rust_quant_infrastructure::repositories::PostgresExchangeSymbolRepository;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::test]
async fn upserts_and_reads_exchange_symbols_from_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!(
            "skipping quant_core exchange_symbols smoke; set QUANT_CORE_EXCHANGE_SYMBOL_SMOKE=1 and QUANT_CORE_DATABASE_URL"
        );
        return Ok(());
    }

    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .context("QUANT_CORE_DATABASE_URL is required for quant_core exchange symbol smoke")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = PostgresExchangeSymbolRepository::new(pool.clone());

    cleanup_test_symbol(&pool, "binance", "BTCUSDT").await?;

    let mut symbol = ExchangeSymbol::new(
        "binance".to_string(),
        "perpetual".to_string(),
        "BTCUSDT".to_string(),
        "BTC-USDT-SWAP".to_string(),
        "BTC".to_string(),
        "USDT".to_string(),
        "TRADING".to_string(),
    );
    symbol.contract_type = Some("PERPETUAL".to_string());
    symbol.price_precision = Some(2);
    symbol.quantity_precision = Some(3);
    symbol.min_qty = Some("0.001".to_string());
    symbol.max_qty = Some("1000".to_string());
    symbol.tick_size = Some("0.10".to_string());
    symbol.step_size = Some("0.001".to_string());
    symbol.min_notional = Some("100".to_string());
    symbol.raw_payload = Some(json!({"source": "quant_core_exchange_symbol_repository_test"}));

    let affected = repository.upsert_many(vec![symbol]).await?;
    assert_eq!(affected, 1);

    let rows = repository
        .find_by_exchange("binance", Some("TRADING"), Some(10))
        .await?;
    let saved = rows
        .into_iter()
        .find(|row| row.exchange_symbol == "BTCUSDT")
        .context("BTCUSDT should exist after upsert")?;
    assert_eq!(saved.normalized_symbol, "BTC-USDT-SWAP");
    assert_eq!(saved.base_asset, "BTC");
    assert_eq!(saved.quote_asset, "USDT");
    assert_eq!(saved.contract_type.as_deref(), Some("PERPETUAL"));

    cleanup_test_symbol(&pool, "binance", "BTCUSDT").await?;
    Ok(())
}

fn smoke_enabled() -> bool {
    env::var("QUANT_CORE_EXCHANGE_SYMBOL_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

async fn cleanup_test_symbol(
    pool: &sqlx::PgPool,
    exchange: &str,
    exchange_symbol: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM exchange_symbols WHERE exchange = $1 AND exchange_symbol = $2")
        .bind(exchange)
        .bind(exchange_symbol)
        .execute(pool)
        .await
        .context("delete test exchange_symbols row")?;
    Ok(())
}
