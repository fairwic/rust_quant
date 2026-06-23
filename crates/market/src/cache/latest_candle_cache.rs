use dashmap::DashMap;
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use rust_quant_core::cache::{get_redis_connection, latest_candle_key, latest_candle_ttl_secs};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
// 从当前包引入 CandlesEntity
use crate::models::CandlesEntity;
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
    /// 映射。
    map: Arc<DashMap<String, CandlesEntity>>,
}
impl InMemoryRedisLatestCandleCache {
    /// 构建 行情与市场数据 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Self {
        Self {
            map: Arc::new(DashMap::new()),
        }
    }
}
impl Default for InMemoryRedisLatestCandleCache {
    fn default() -> Self {
        Self::new()
    }
}
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
    /// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
    fn get_or_fetch<'a>(
        &'a self,
        inst_id: &'a str,
        period: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CandlesEntity>> + Send + 'a>> {
        let key = make_key(inst_id, period);
        let map: Arc<DashMap<String, CandlesEntity>> = Arc::clone(&self.map);
        Box::pin(async move {
            if let Some(c) = map.get(&key).map(|v| v.clone()) {
                return Some(c);
            }
            if let Ok(mut conn) = get_redis_connection().await {
                let rkey = latest_candle_key(inst_id, period);
                let result: redis::RedisResult<String> = conn.get(&rkey).await;
                if let Ok(s) = result {
                    if let Ok(c) = serde_json::from_str::<CandlesEntity>(&s) {
                        map.insert(key, c.clone());
                        return Some(c);
                    }
                }
            }
            None
        })
    }
    /// 更新 行情与市场数据 状态，并保留调用方需要的结果或错误信息。
    fn set_both<'a>(
        &'a self,
        inst_id: &'a str,
        period: &'a str,
        candle: &'a CandlesEntity,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let key = make_key(inst_id, period);
        let map: Arc<DashMap<String, CandlesEntity>> = Arc::clone(&self.map);
        Box::pin(async move {
            map.insert(key, candle.clone());
            if let Ok(mut conn) = get_redis_connection().await {
                let rkey = latest_candle_key(inst_id, period);
                let ttl = latest_candle_ttl_secs();
                let payload = serde_json::to_string(candle).unwrap();
                let _: redis::RedisResult<()> = conn.set_ex::<_, _, ()>(rkey, payload, ttl).await;
            }
        })
    }
}
/// 默认缓存提供者（可用于全局注入）
pub static DEFAULT_PROVIDER: Lazy<Arc<dyn LatestCandleCacheProvider>> =
    Lazy::new(|| Arc::new(InMemoryRedisLatestCandleCache::new()));
pub fn default_provider() -> Arc<dyn LatestCandleCacheProvider> {
    Arc::clone(&DEFAULT_PROVIDER)
}
