//! 缓存管理

pub mod redis_client;

// 重新导出
pub use redis_client::{
    init_redis_pool, 
    get_redis_pool, 
    cleanup_redis_pool,
    get_redis_connection,
    latest_candle_key,
    latest_candle_ttl_secs,
};

