//! 通用泛型缓存接口
//!
//! 提供内存和Redis双层缓存能力，支持任意可序列化类型
use anyhow::Result;
use dashmap::DashMap;
use redis::AsyncCommands;
use rust_quant_core::cache::get_redis_connection;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error};
/// 缓存提供者接口 - 通用trait
#[async_trait::async_trait]
pub trait CacheProvider<T>: Send + Sync
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    /// 获取缓存值
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn get(&self, key: &str) -> Result<Option<T>>;
    /// 设置缓存值
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn set(&self, key: &str, value: &T, ttl: Option<u64>) -> Result<()>;
    /// 删除缓存值
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn delete(&self, key: &str) -> Result<()>;
    /// 检查键是否存在
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn exists(&self, key: &str) -> Result<bool>;
    /// 批量获取
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<T>>>;
}
/// 内存缓存实现（使用DashMap）
pub struct InMemoryCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    map: Arc<DashMap<String, CacheEntry<T>>>,
    default_ttl: Option<Duration>,
}
#[derive(Clone)]
struct CacheEntry<T> {
    value: T,
    expire_at: Option<Instant>,
}
impl<T> InMemoryCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    /// 构建 配置、基础设施和运行时 所需实例，并集中初始化依赖和默认状态。
    pub fn new(default_ttl: Option<Duration>) -> Self {
        Self {
            map: Arc::new(DashMap::new()),
            default_ttl,
        }
    }
    /// 判断 配置、基础设施和运行时 条件是否满足，给上层流程提供布尔决策。
    fn is_expired(&self, entry: &CacheEntry<T>) -> bool {
        if let Some(expire_at) = entry.expire_at {
            Instant::now() > expire_at
        } else {
            false
        }
    }
}
#[async_trait::async_trait]
impl<T> CacheProvider<T> for InMemoryCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// 提供get的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn get(&self, key: &str) -> Result<Option<T>> {
        if let Some(entry) = self.map.get(key) {
            if !self.is_expired(&entry) {
                return Ok(Some(entry.value.clone()));
            } else {
                // 过期则删除
                drop(entry);
                self.map.remove(key);
            }
        }
        Ok(None)
    }
    /// 提供set的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn set(&self, key: &str, value: &T, ttl: Option<u64>) -> Result<()> {
        let expire_at = ttl
            .or(self.default_ttl.map(|d| d.as_secs()))
            .map(|secs| Instant::now() + Duration::from_secs(secs));
        let entry = CacheEntry {
            value: value.clone(),
            expire_at,
        };
        self.map.insert(key.to_string(), entry);
        Ok(())
    }
    /// 提供delete的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn delete(&self, key: &str) -> Result<()> {
        self.map.remove(key);
        Ok(())
    }
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }
    /// 提供mget的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<T>>> {
        let mut result = Vec::with_capacity(keys.len());
        for key in keys {
            result.push(self.get(key).await?);
        }
        Ok(result)
    }
}
/// Redis缓存实现
pub struct RedisCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    key_prefix: String,
    default_ttl: Option<u64>,
    _phantom: PhantomData<T>,
}
impl<T> RedisCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    /// 构建 配置、基础设施和运行时 所需实例，并集中初始化依赖和默认状态。
    pub fn new(key_prefix: String, default_ttl: Option<u64>) -> Self {
        Self {
            key_prefix,
            default_ttl,
            _phantom: PhantomData,
        }
    }
    fn make_key(&self, key: &str) -> String {
        format!("{}:{}", self.key_prefix, key)
    }
}
#[async_trait::async_trait]
impl<T> CacheProvider<T> for RedisCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// 提供get的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn get(&self, key: &str) -> Result<Option<T>> {
        let mut conn = get_redis_connection().await?;
        let redis_key = self.make_key(key);
        let result: redis::RedisResult<String> = conn.get(&redis_key).await;
        match result {
            Ok(s) => {
                let value: T = serde_json::from_str(&s)?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == redis::ErrorKind::TypeError => {
                // Key不存在
                Ok(None)
            }
            Err(e) => {
                error!("Redis get error: {:?}", e);
                Err(e.into())
            }
        }
    }
    /// 提供set的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn set(&self, key: &str, value: &T, ttl: Option<u64>) -> Result<()> {
        let mut conn = get_redis_connection().await?;
        let redis_key = self.make_key(key);
        let payload = serde_json::to_string(value)?;
        let ttl_secs = ttl.or(self.default_ttl).unwrap_or(3600); // 默认1小时
        let _: () = conn.set_ex(redis_key, payload, ttl_secs).await?;
        Ok(())
    }
    /// 提供delete的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = get_redis_connection().await?;
        let redis_key = self.make_key(key);
        let _: () = conn.del(redis_key).await?;
        Ok(())
    }
    /// 提供exists的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = get_redis_connection().await?;
        let redis_key = self.make_key(key);
        let result: bool = conn.exists(redis_key).await?;
        Ok(result)
    }
    /// 提供mget的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<T>>> {
        let mut conn = get_redis_connection().await?;
        let redis_keys: Vec<String> = keys.iter().map(|k| self.make_key(k)).collect();
        let result: Vec<Option<String>> = conn.get(&redis_keys).await?;
        let mut values = Vec::with_capacity(result.len());
        for opt_str in result {
            if let Some(s) = opt_str {
                let value: T = serde_json::from_str(&s)?;
                values.push(Some(value));
            } else {
                values.push(None);
            }
        }
        Ok(values)
    }
}
/// 双层缓存（内存 + Redis）
pub struct TwoLevelCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    memory: InMemoryCache<T>,
    redis: RedisCache<T>,
}
impl<T> TwoLevelCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    /// 构建 配置、基础设施和运行时 所需实例，并集中初始化依赖和默认状态。
    pub fn new(key_prefix: String, memory_ttl: Option<Duration>, redis_ttl: Option<u64>) -> Self {
        Self {
            memory: InMemoryCache::new(memory_ttl),
            redis: RedisCache::new(key_prefix, redis_ttl),
        }
    }
}
#[async_trait::async_trait]
impl<T> CacheProvider<T> for TwoLevelCache<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// 提供get的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn get(&self, key: &str) -> Result<Option<T>> {
        // 先查内存
        if let Some(value) = self.memory.get(key).await? {
            debug!("Cache hit (memory): {}", key);
            return Ok(Some(value));
        }
        // 再查Redis
        if let Some(value) = self.redis.get(key).await? {
            debug!("Cache hit (redis): {}", key);
            // 回填到内存
            let _ = self.memory.set(key, &value, None).await;
            return Ok(Some(value));
        }
        debug!("Cache miss: {}", key);
        Ok(None)
    }
    /// 提供set的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn set(&self, key: &str, value: &T, ttl: Option<u64>) -> Result<()> {
        // 同时写入内存和Redis
        self.memory.set(key, value, ttl).await?;
        self.redis.set(key, value, ttl).await?;
        Ok(())
    }
    /// 提供delete的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn delete(&self, key: &str) -> Result<()> {
        // 同时删除内存和Redis
        self.memory.delete(key).await?;
        self.redis.delete(key).await?;
        Ok(())
    }
    /// 提供exists的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn exists(&self, key: &str) -> Result<bool> {
        // 优先查内存
        if self.memory.exists(key).await? {
            return Ok(true);
        }
        self.redis.exists(key).await
    }
    /// 提供mget的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<T>>> {
        // TODO: 优化批量查询逻辑
        let mut result = Vec::with_capacity(keys.len());
        for key in keys {
            result.push(self.get(key).await?);
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        /// 唯一标识。
        id: i64,
        /// 名称。
        name: String,
    }
    #[tokio::test]
    async fn test_memory_cache() {
        let cache = InMemoryCache::<TestData>::new(Some(Duration::from_secs(60)));
        let data = TestData {
            id: 1,
            name: "test".to_string(),
        };
        // 设置
        cache.set("key1", &data, None).await.unwrap();
        // 获取
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, Some(data.clone()));
        // 删除
        cache.delete("key1").await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, None);
    }
}
