use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;
use redis::AsyncCommands;

use crate::app_config::redis as app_redis;
use crate::trading::model::entity::candles::entity::CandlesEntity;

fn make_key(inst_id: &str, period: &str) -> String {
    format!("{}:{}", inst_id, period)
}

/// 抽象：最新K线缓存提供者
pub trait LatestCandleCacheProvider: Send + Sync {
    fn get(&self, inst_id: &str, period: &str) -> Option<CandlesEntity>;
    fn set(&self, inst_id: &str, period: &str, candle: CandlesEntity);
    fn remove(&self, inst_id: &str, period: &str);

    fn get_or_fetch<'a>(
        &'a self,
        inst_id: &'a str,
        period: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CandlesEntity>> + Send + 'a>>;

    fn set_both<'a>(
        &'a self,
        inst_id: &'a str,
        period: &'a str,
        candle: &'a CandlesEntity,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// 具体实现：进程内(DashMap) + Redis
pub struct InMemoryRedisLatestCandleCache {
    map: Arc<DashMap<String, CandlesEntity>>,
}

impl InMemoryRedisLatestCandleCache {
    pub fn new() -> Self {
        Self { map: Arc::new(DashMap::new()) }
    }
}

impl Default for InMemoryRedisLatestCandleCache { fn default() -> Self { Self::new() } }

impl LatestCandleCacheProvider for InMemoryRedisLatestCandleCache {
    fn get(&self, inst_id: &str, period: &str) -> Option<CandlesEntity> {
        self.map.get(&make_key(inst_id, period)).map(|v| v.clone())
    }
    fn set(&self, inst_id: &str, period: &str, candle: CandlesEntity) {
        self.map.insert(make_key(inst_id, period), candle);
    }
    fn remove(&self, inst_id: &str, period: &str) {
        self.map.remove(&make_key(inst_id, period));
    }

    fn get_or_fetch<'a>(&'a self, inst_id: &'a str, period: &'a str)
        -> Pin<Box<dyn Future<Output = Option<CandlesEntity>> + Send + 'a>>
    {
        let key = make_key(inst_id, period);
        let map = Arc::clone(&self.map);
        Box::pin(async move {
            if let Some(c) = map.get(&key).map(|v| v.clone()) {
                return Some(c);
            }
            if let Ok(mut conn) = app_redis::get_redis_connection().await {
                let rkey = app_redis::latest_candle_key(inst_id, period);
                if let Ok(s) = conn.get::<_, String>(&rkey).await {
                    if let Ok(c) = serde_json::from_str::<CandlesEntity>(&s) {
                        map.insert(key, c.clone());
                        return Some(c);
                    }
                }
            }
            None
        })
    }

    fn set_both<'a>(&'a self, inst_id: &'a str, period: &'a str, candle: &'a CandlesEntity)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
    {
        let key = make_key(inst_id, period);
        let map = Arc::clone(&self.map);
        Box::pin(async move {
            map.insert(key, candle.clone());
            if let Ok(mut conn) = app_redis::get_redis_connection().await {
                let rkey = app_redis::latest_candle_key(inst_id, period);
                let ttl = app_redis::latest_candle_ttl_secs();
                let payload = serde_json::to_string(candle).unwrap();
                let _ : redis::RedisResult<()> = conn.set_ex::<_,_,()>(rkey, payload, ttl).await;
            }
        })
    }
}

/// 默认缓存提供者（可用于全局注入）
pub static DEFAULT_PROVIDER: Lazy<Arc<dyn LatestCandleCacheProvider>> =
    Lazy::new(|| Arc::new(InMemoryRedisLatestCandleCache::new()));

/// 获取默认提供者（便于调用方使用 trait 接口）
pub fn default_provider() -> Arc<dyn LatestCandleCacheProvider> {
    Arc::clone(&DEFAULT_PROVIDER)
}
//
// // 兼容层：保留原有模块函数，内部委托给默认提供者
// pub fn get(inst_id: &str, period: &str) -> Option<CandlesEntity> {
//     DEFAULT_PROVIDER.get(inst_id, period)
// }
// pub fn set(inst_id: &str, period: &str, candle: CandlesEntity) {
//     DEFAULT_PROVIDER.set(inst_id, period, candle)
// }
// pub fn remove(inst_id: &str, period: &str) {
//     DEFAULT_PROVIDER.remove(inst_id, period)
// }
// pub async fn get_or_fetch(inst_id: &str, period: &str) -> Option<CandlesEntity> {
//     DEFAULT_PROVIDER.get_or_fetch(inst_id, period).await
// }
// pub async fn set_both(inst_id: &str, period: &str, candle: &CandlesEntity) {
//     DEFAULT_PROVIDER.set_both(inst_id, period, candle).await
// }




