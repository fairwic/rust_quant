//! 策略性能优化模块
//!
//! 提供策略系统的性能优化功能，包括配置缓存、
//! 锁竞争减少和内存使用优化。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::debug;

use crate::trading::strategy::order::strategy_config::StrategyConfig;

/// 配置缓存条目
#[derive(Debug, Clone)]
struct ConfigCacheEntry {
    config: Arc<StrategyConfig>,
    last_access_time: i64,
    access_count: u64,
}

/// 策略配置缓存
pub struct StrategyConfigCache {
    /// 配置缓存：config_id -> 配置对象
    cache: Arc<RwLock<HashMap<i64, ConfigCacheEntry>>>,
    /// 缓存过期时间（秒）
    cache_ttl_secs: u64,
    /// 最大缓存条目数
    max_cache_size: usize,
}

impl StrategyConfigCache {
    /// 创建新的配置缓存
    pub fn new(cache_ttl_secs: u64, max_cache_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl_secs,
            max_cache_size,
        }
    }

    /// 获取配置（带缓存）
    pub async fn get_config(&self, config_id: i64) -> Option<Arc<StrategyConfig>> {
        let mut cache = self.cache.write().await;
        
        if let Some(entry) = cache.get_mut(&config_id) {
            // 检查是否过期
            let now = chrono::Utc::now().timestamp();
            if now - entry.last_access_time < self.cache_ttl_secs as i64 {
                entry.last_access_time = now;
                entry.access_count += 1;
                debug!("配置缓存命中: config_id={}", config_id);
                return Some(entry.config.clone());
            } else {
                // 过期，移除
                cache.remove(&config_id);
                debug!("配置缓存过期，已移除: config_id={}", config_id);
            }
        }
        
        None
    }

    /// 缓存配置
    pub async fn cache_config(&self, config_id: i64, config: Arc<StrategyConfig>) {
        let mut cache = self.cache.write().await;
        
        // 检查缓存大小限制
        if cache.len() >= self.max_cache_size {
            self.evict_lru_entry(&mut cache).await;
        }
        
        let entry = ConfigCacheEntry {
            config,
            last_access_time: chrono::Utc::now().timestamp(),
            access_count: 1,
        };
        
        cache.insert(config_id, entry);
        debug!("配置已缓存: config_id={}", config_id);
    }

    /// 清除过期缓存条目
    pub async fn cleanup_expired(&self) {
        let mut cache = self.cache.write().await;
        let now = chrono::Utc::now().timestamp();
        let cutoff_time = now - self.cache_ttl_secs as i64;
        
        cache.retain(|config_id, entry| {
            if entry.last_access_time < cutoff_time {
                debug!("清理过期配置缓存: config_id={}", config_id);
                false
            } else {
                true
            }
        });
    }

    /// 驱逐最少使用的条目（LRU）
    async fn evict_lru_entry(&self, cache: &mut HashMap<i64, ConfigCacheEntry>) {
        if let Some((&lru_id, _)) = cache.iter()
            .min_by_key(|(_, entry)| (entry.last_access_time, entry.access_count)) {
            cache.remove(&lru_id);
            debug!("驱逐LRU配置缓存: config_id={}", lru_id);
        }
    }

    /// 获取缓存统计信息
    pub async fn get_cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_entries = cache.len();
        let total_access_count: u64 = cache.values().map(|entry| entry.access_count).sum();
        
        CacheStats {
            total_entries,
            total_access_count,
            hit_rate: if total_access_count > 0 { 
                (total_access_count as f64) / (total_access_count as f64 + cache.len() as f64) 
            } else { 
                0.0 
            },
        }
    }
}

/// 缓存统计信息
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_access_count: u64,
    pub hit_rate: f64,
}

/// 性能优化工具
pub struct PerformanceOptimizer;

impl PerformanceOptimizer {
    /// 优化配置对象引用（避免不必要的克隆）
    pub async fn with_config_ref<F, R>(
        config: &Arc<RwLock<StrategyConfig>>,
        operation: F,
    ) -> R
    where
        F: FnOnce(&StrategyConfig) -> R,
    {
        let guard = config.read().await;
        operation(&*guard)
    }

    /// 批量操作优化（减少锁获取次数）
    pub async fn batch_config_operation<F, R>(
        configs: &[Arc<RwLock<StrategyConfig>>],
        operation: F,
    ) -> Vec<R>
    where
        F: Fn(&StrategyConfig) -> R + Send + Sync,
        R: Send,
    {
        let mut results = Vec::with_capacity(configs.len());
        
        for config in configs {
            let result = Self::with_config_ref(config, &operation).await;
            results.push(result);
        }
        
        results
    }

    /// 异步任务批处理优化
    pub async fn batch_async_operations<F, Fut, R>(
        operations: Vec<F>,
        batch_size: usize,
    ) -> Vec<Result<R, anyhow::Error>>
    where
        F: Fn() -> Fut + Send + Clone,
        Fut: std::future::Future<Output = Result<R, anyhow::Error>> + Send,
        R: Send,
    {
        let mut results = Vec::with_capacity(operations.len());
        
        for chunk in operations.chunks(batch_size) {
            let futures: Vec<_> = chunk.iter()
                .map(|op| async move { op().await })
                .collect();
            
            let chunk_results = futures::future::join_all(futures).await;
            results.extend(chunk_results);
        }
        
        results
    }
}

/// 全局配置缓存实例
static CONFIG_CACHE: once_cell::sync::OnceCell<Arc<StrategyConfigCache>> = once_cell::sync::OnceCell::new();

/// 获取全局配置缓存
pub fn get_config_cache() -> Arc<StrategyConfigCache> {
    CONFIG_CACHE
        .get_or_init(|| Arc::new(StrategyConfigCache::new(300, 100))) // 5分钟TTL，最多100个条目
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::indicator::vegas_indicator::VegasStrategy;
    use crate::trading::strategy::strategy_common::BasicRiskStrategyConfig;

    #[tokio::test]
    async fn test_config_cache() {
        let cache = StrategyConfigCache::new(60, 10);
        let config = Arc::new(StrategyConfig::new(
            1,
            serde_json::to_string(&VegasStrategy::default()).unwrap(),
            serde_json::to_string(&BasicRiskStrategyConfig::default()).unwrap(),
        ));
        
        // 缓存配置
        cache.cache_config(1, config.clone()).await;
        
        // 获取缓存的配置
        let cached_config = cache.get_config(1).await.unwrap();
        assert_eq!(cached_config.strategy_config_id, 1);
    }

    #[tokio::test]
    async fn test_performance_optimizer() {
        let config = Arc::new(RwLock::new(StrategyConfig::default()));
        
        let result = PerformanceOptimizer::with_config_ref(&config, |cfg| {
            cfg.strategy_config_id
        }).await;
        
        assert_eq!(result, 1);
    }
}