use anyhow::{Context, Result};
use crypto_exc_all::{CandleQuery, ExchangeId, Instrument};
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use rust_quant_services::market::CandleService;
use rust_quant_services::CryptoExcAllGateway;
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;
use std::env;

#[tokio::test]
async fn fetches_binance_candles_via_crypto_exc_all_when_enabled() -> Result<()> {
    if !smoke_enabled() {
        eprintln!("skipping Binance K-line smoke; set BINANCE_KLINE_SMOKE=1");
        return Ok(());
    }

    let gateway = CryptoExcAllGateway::from_single_exchange_credentials(
        ExchangeId::Binance,
        env::var("BINANCE_API_KEY").unwrap_or_else(|_| "public-market-only".to_string()),
        env::var("BINANCE_API_SECRET").unwrap_or_else(|_| "public-market-only".to_string()),
        Option::<String>::None,
        false,
    )
    .context("build Binance crypto_exc_all gateway")?;

    let candles = gateway
        .candles(
            ExchangeId::Binance,
            CandleQuery::new(Instrument::perp("BTC", "USDT"), "1m").with_limit(2),
        )
        .await
        .context("fetch Binance BTCUSDT 1m candles through crypto_exc_all")?;

    assert!(!candles.is_empty(), "Binance returned no candles");
    assert_eq!(candles[0].exchange, ExchangeId::Binance);
    assert_eq!(candles[0].exchange_symbol, "BTCUSDT");
    assert!(candles[0].open_time.is_some(), "missing candle open_time");
    assert!(
        candles[0].close.parse::<f64>().unwrap_or_default() > 0.0,
        "invalid candle close price: {}",
        candles[0].close
    );

    Ok(())
}

#[tokio::test]
async fn fetches_and_persists_binance_eth_candles_to_quant_core_sharded_table() -> Result<()> {
    if !persist_smoke_enabled() {
        eprintln!("skipping Binance K-line persist smoke; set BINANCE_KLINE_PERSIST_SMOKE=1");
        return Ok(());
    }

    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").expect("QUANT_CORE_DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let service = CandleService::new(Box::new(PostgresCandleRepository::new(pool.clone())));

    let max_candles = env::var("BINANCE_KLINE_PERSIST_MAX_CANDLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2);
    let page_limit = env::var("BINANCE_KLINE_PERSIST_PAGE_LIMIT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(2)
        .min(1500);
    let mut cursor = env::var("BINANCE_KLINE_PERSIST_START_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());

    let mut collected = BTreeMap::new();
    while collected.len() < max_candles {
        let candles = service
            .fetch_candles_from_crypto_exc_all(
                "binance",
                "ETH-USDT-SWAP",
                "4H",
                cursor,
                None,
                page_limit,
            )
            .await
            .context("fetch Binance ETHUSDT 4H candles as domain candles")?;
        if candles.is_empty() {
            break;
        }

        let next_cursor = candles
            .iter()
            .filter_map(|candle| u64::try_from(candle.timestamp).ok())
            .max()
            .and_then(|timestamp| timestamp.checked_add(1));
        for candle in candles {
            collected.insert(candle.timestamp, candle);
        }

        if cursor.is_none() || next_cursor <= cursor || collected.len() >= max_candles {
            break;
        }
        cursor = next_cursor;
    }

    let candles: Vec<_> = collected.into_values().take(max_candles).collect();
    assert!(!candles.is_empty(), "Binance returned no ETH candles");

    let saved = service.save_candles(candles.clone()).await?;
    assert!(saved >= candles.len());

    let persisted_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "eth-usdt-swap_candles_4h""#)
            .fetch_one(&pool)
            .await?;
    assert!(
        persisted_count >= candles.len() as i64,
        "expected persisted ETH candles, got {}",
        persisted_count
    );

    Ok(())
}

fn smoke_enabled() -> bool {
    env::var("BINANCE_KLINE_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn persist_smoke_enabled() -> bool {
    env::var("BINANCE_KLINE_PERSIST_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}
