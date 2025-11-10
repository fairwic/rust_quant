//! 缓存管理

pub mod redis_client;

// 重新导出
pub use redis_client::{
    cleanup_redis_pool, get_redis_connection, get_redis_pool, init_redis_pool, latest_candle_key,
    latest_candle_ttl_secs,
};
