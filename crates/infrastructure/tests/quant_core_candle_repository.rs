use anyhow::Result;
use rust_quant_domain::traits::CandleRepository;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::test]
async fn upserts_and_reads_legacy_sharded_candles_from_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!("skipping quant_core candle smoke; set QUANT_CORE_CANDLE_SMOKE=1");
        return Ok(());
    }

    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").expect("QUANT_CORE_DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let repo = PostgresCandleRepository::new(pool.clone());

    let symbol = "ETH-USDT-SWAP";
    let timeframe = Timeframe::H4;
    let timestamp = 9_100_000_000_000_i64;

    repo.ensure_table(symbol, timeframe).await?;
    cleanup(&pool, timestamp).await?;

    let mut candle = Candle::new(
        symbol.to_string(),
        timeframe,
        timestamp,
        Price::new(2000.0).unwrap(),
        Price::new(2100.0).unwrap(),
        Price::new(1900.0).unwrap(),
        Price::new(2050.0).unwrap(),
        Volume::new(123.45).unwrap(),
    );
    candle.confirm();

    let saved = repo.save_candles(vec![candle]).await?;
    assert_eq!(saved, 1);

    let candles = repo
        .find_candles(symbol, timeframe, timestamp - 1, timestamp + 1, Some(10))
        .await?;
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].symbol, symbol);
    assert_eq!(candles[0].timeframe, timeframe);
    assert_eq!(candles[0].timestamp, timestamp);
    assert_eq!(candles[0].close.value(), 2050.0);
    assert!(candles[0].confirmed);

    let latest = repo.get_latest_candle(symbol, timeframe).await?;
    assert_eq!(latest.map(|c| c.timestamp), Some(timestamp));

    cleanup(&pool, timestamp).await?;
    Ok(())
}

async fn cleanup(pool: &sqlx::PgPool, timestamp: i64) -> Result<()> {
    sqlx::query(
        r#"
        DELETE FROM "eth-usdt-swap_candles_4h"
        WHERE ts = $1
        "#,
    )
    .bind(timestamp)
    .execute(pool)
    .await?;

    Ok(())
}

fn smoke_enabled() -> bool {
    env::var("QUANT_CORE_CANDLE_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}
