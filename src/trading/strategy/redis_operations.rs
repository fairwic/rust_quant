use anyhow::Result;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::app_config::redis_config as app_redis;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RedisCandle {
    pub ts: i64,
    pub c: String,
}

pub struct RedisOperations;

impl RedisOperations {
    /// 使用提供的连接保存蜡烛图数据到Redis
    pub async fn save_candles_to_redis(
        con: &mut MultiplexedConnection,
        key: &str,
        candles: &[RedisCandle],
    ) -> Result<()> {
        let mut pipe = redis::pipe();
        for candle in candles {
            let timestamp: i64 = candle.ts;
            let close_price: f64 = candle.c.parse().unwrap_or(0.0);
            pipe.zadd(key, timestamp, close_price).ignore();
        }
        pipe.query_async::<_, ()>(con).await?;
        info!("Saved {} candles to Redis", candles.len());
        Ok(())
    }

    /// 使用提供的连接从Redis获取蜡烛图数据
    pub async fn fetch_candles_from_redis(
        con: &mut MultiplexedConnection,
        key: &str,
    ) -> Result<Vec<RedisCandle>> {
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

    /// [已优化] 使用标准连接池保存蜡烛图数据到Redis
    pub async fn save_candles_to_redis_with_pool(
        key: &str,
        candles: &[RedisCandle],
    ) -> Result<()> {
        let mut con = app_redis::get_redis_connection().await?;
        Self::save_candles_to_redis(&mut con, key, candles).await
    }

    /// [已优化] 使用标准连接池从Redis获取蜡烛图数据
    pub async fn fetch_candles_from_redis_with_pool(
        key: &str,
    ) -> Result<Vec<RedisCandle>> {
        let mut con = app_redis::get_redis_connection().await?;
        Self::fetch_candles_from_redis(&mut con, key).await
    }
}
