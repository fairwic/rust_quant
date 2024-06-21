use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use clap::builder::TypedValueParser;
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RedisCandle {
    pub ts: i64,
    pub c: String,
}

pub struct RedisOperations;

impl RedisOperations {
    pub async fn save_candles_to_redis(con: &mut MultiplexedConnection, key: &str, candles: &[RedisCandle]) -> Result<()> {
        let mut pipe = redis::pipe();
        for candle in candles {
            let timestamp: i64 = candle.ts;
            let close_price: f64 = candle.c.parse().unwrap_or(0.0);
            pipe.zadd(key, timestamp, close_price).ignore();
        }
        pipe.query_async(con).await?;
        info!("Saved {} candles to Redis", candles.len());
        Ok(())
    }

    pub async fn fetch_candles_from_redis(con: &mut MultiplexedConnection, key: &str) -> Result<Vec<RedisCandle>> {
        // let data: Vec<(f64, f64)> = con.zrangebyscore_withscores(key, "-inf", "+inf").await?;
        let data: Vec<(f64, f64)> = con.zrange_withscores(key, 0, -1).await?;
        let mut candles: Vec<RedisCandle> = data
            .into_iter()
            .map(|(timestamp, close_price)| RedisCandle {
                ts: timestamp as i64,
                c: close_price.to_string(),
            })
            .collect();

        // Sort the candles by timestamp to ensure they are ordered correctly
        candles.sort_unstable_by(|a, b| a.ts.cmp(&b.ts));
        println!("candles: {:?}", candles);

        info!("Retrieved {} candles from Redis", candles.len());
        Ok(candles)
    }
}
