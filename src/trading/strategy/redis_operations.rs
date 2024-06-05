use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Candle {
    pub ts: String,
    pub c: String,
}

pub struct RedisOperations;

impl RedisOperations {
    pub async fn save_candles_to_redis(con: &mut MultiplexedConnection, candles: &[Candle]) -> Result<()> {
        let mut pipe = redis::pipe();
        for candle in candles {
            let key = "btc_candles";
            let timestamp: f64 = candle.ts.parse().unwrap_or(0.0);
            let close_price: f64 = candle.c.parse().unwrap_or(0.0);
            pipe.zadd(key, close_price, timestamp).ignore();
        }
        pipe.query_async(con).await?;
        info!("Saved {} candles to Redis", candles.len());
        Ok(())
    }

    pub async fn fetch_candles_from_redis(con: &mut MultiplexedConnection) -> Result<Vec<Candle>> {
        let key = "btc_candles";
        let data: Vec<(f64, f64)> = con.zrangebyscore_withscores(key, "-inf", "+inf").await?;
        let candles: Vec<Candle> = data
            .into_iter()
            .map(|(close_price, timestamp)| Candle {
                ts: timestamp.to_string(),
                c: close_price.to_string(),
            })
            .collect();
        info!("Retrieved {} candles from Redis", candles.len());
        Ok(candles)
    }
}
