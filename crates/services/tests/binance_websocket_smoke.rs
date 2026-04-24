use anyhow::Result;
use rust_quant_domain::traits::CandleRepository;
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use rust_quant_services::market::binance_websocket::{
    binance_kline_stream_name, parse_binance_kline_message, receive_one_binance_public_message,
};
use sqlx::postgres::PgPoolOptions;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn receives_binance_public_kline_message_when_enabled() -> Result<()> {
    if std::env::var("BINANCE_WEBSOCKET_SMOKE").ok().as_deref() != Some("1") {
        return Ok(());
    }

    let stream = binance_kline_stream_name("ETH-USDT-SWAP", "1m");
    let message = receive_one_with_retry(&[stream], 3).await?;
    let update = parse_binance_kline_message(&message, "ETH-USDT-SWAP", "1m")?;

    assert_eq!(update.inst_id, "ETH-USDT-SWAP");
    assert_eq!(update.time_interval, "1m");
    assert!(update.candle_entity.ts > 0);

    Ok(())
}

#[tokio::test]
async fn persists_binance_websocket_kline_to_quant_core_split_table_when_enabled() -> Result<()> {
    if std::env::var("BINANCE_WEBSOCKET_PERSIST_SMOKE")
        .ok()
        .as_deref()
        != Some("1")
    {
        return Ok(());
    }

    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")?;
    let stream = binance_kline_stream_name("ETH-USDT-SWAP", "1m");
    let message = receive_one_with_retry(&[stream], 3).await?;
    let update = parse_binance_kline_message(&message, "ETH-USDT-SWAP", "1m")?;
    let ts = update.domain_candle.timestamp;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&database_url)?;
    let repository = PostgresCandleRepository::new(pool.clone());
    repository.save_candles(vec![update.domain_candle]).await?;

    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "eth-usdt-swap_candles_1m" WHERE ts = $1"#)
            .bind(ts)
            .fetch_one(&pool)
            .await?;

    assert_eq!(count, 1);

    Ok(())
}

async fn receive_one_with_retry(streams: &[String], attempts: usize) -> Result<serde_json::Value> {
    let mut last_error = None;
    for attempt in 1..=attempts {
        match receive_one_binance_public_message(streams, 15).await {
            Ok(message) => return Ok(message),
            Err(error) => {
                last_error = Some(error);
                if attempt < attempts {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    Err(last_error.expect("attempts must be greater than zero"))
}
