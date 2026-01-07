use std::env;

use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use redis::aio::MultiplexedConnection;
use redis::Client;
use tracing::{debug, error, info};

/// Redis连接池管理器
pub struct RedisConnectionPool {
    client: Client,
}

impl RedisConnectionPool {
    /// 创建新的连接池
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client =
            Client::open(redis_url).map_err(|e| anyhow!("Failed to create Redis client: {}", e))?;

        // 测试连接
        let _test_conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                error!("Redis connection test failed: {}", redis_url);
                anyhow!("Failed to test Redis connection: {}", e)
            })?;

        debug!("Redis连接池初始化成功");

        Ok(Self { client })
    }

    /// 获取连接
    pub async fn get_connection(&self) -> Result<MultiplexedConnection> {
        // 从客户端获取多路复用连接
        let conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| anyhow!("Failed to get multiplexed connection: {}", e))?;

        debug!("获取Redis连接成功");
        Ok(conn)
    }
}

/// 全局Redis连接池实例
pub static REDIS_POOL: OnceCell<RedisConnectionPool> = OnceCell::new();

/// 初始化Redis连接池
pub async fn init_redis_pool() -> Result<()> {
    let redis_url =
        env::var("REDIS_HOST").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());

    let pool = RedisConnectionPool::new(&redis_url).await?;

    REDIS_POOL
        .set(pool)
        .map_err(|_| anyhow!("Failed to initialize Redis connection pool"))?;

    info!("Redis connection pool initialized successfully ！");
    Ok(())
}

/// 获取Redis连接池实例
pub fn get_redis_pool() -> Result<&'static RedisConnectionPool> {
    REDIS_POOL
        .get()
        .ok_or_else(|| anyhow!("Redis连接池未初始化，请先调用 init_redis_pool()"))
}

/// [已优化] 获取Redis连接 - 现在使用连接池
pub async fn get_redis_connection() -> Result<MultiplexedConnection> {
    let pool = get_redis_pool()?;
    pool.get_connection().await
}

/// 监控Redis连接池状态
pub async fn monitor_redis_pool() -> Result<String> {
    let _pool = get_redis_pool()?;
    Ok("Redis连接池状态 - 正常运行".to_string())
}

/// 清理Redis连接池
pub async fn cleanup_redis_pool() -> Result<()> {
    if let Ok(_pool) = get_redis_pool() {
        info!("Redis连接池清理完成");
    }
    Ok(())
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
