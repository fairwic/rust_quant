use std::env;

use anyhow::Result;
use redis::aio::MultiplexedConnection;
use redis::Client;

/// Get a Redis multiplexed async connection using REDIS_HOST from env
pub async fn get_redis_connection() -> Result<MultiplexedConnection> {
    let url = env::var("REDIS_HOST").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let client = Client::open(url)?;
    let conn = client.get_multiplexed_async_connection().await?;
    Ok(conn)
}

/// Helper to build a key for latest candle JSON
pub fn latest_candle_key(inst_id: &str, period: &str) -> String {
    format!("latest_candle:{}:{}", inst_id, period)
}

/// TTL for latest candle key, seconds
pub fn latest_candle_ttl_secs() -> u64 {
    env::var("LATEST_CANDLE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10u64)
}

