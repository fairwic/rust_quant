//! 指标缓存实现
//! 
//! 将原 strategies/cache/ 中的指标缓存迁移到这里

use anyhow::{Result, anyhow};
use redis::aio::MultiplexedConnection as ConnectionManager;
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, error};

/// 指标缓存管理器
pub struct IndicatorCache {
    redis_client: ConnectionManager,
}

impl IndicatorCache {
    pub fn new(redis_client: ConnectionManager) -> Self {
        Self { redis_client }
    }
    
    /// 保存指标数据到缓存
    /// 
    /// # 参数
    /// - `key`: 缓存键
    /// - `value`: 要缓存的值（需要实现 Serialize）
    /// - `expire_seconds`: 过期时间（秒），None 表示不过期
    pub async fn save<T: Serialize>(
        &mut self,
        key: &str,
        value: &T,
        expire_seconds: Option<usize>,
    ) -> Result<()> {
        // 序列化为 JSON
        let json_str = serde_json::to_string(value)
            .map_err(|e| anyhow!("序列化失败: {}", e))?;
        
        // 保存到 Redis
        self.redis_client.set::<_, _, ()>(key, json_str).await
            .map_err(|e| {
                error!("Redis SET 失败: key={}, error={}", key, e);
                anyhow!("Redis SET 失败: {}", e)
            })?;
        
        // 设置过期时间
        if let Some(seconds) = expire_seconds {
            self.redis_client.expire::<_, ()>(key, seconds as i64).await
                .map_err(|e| {
                    error!("Redis EXPIRE 失败: key={}, error={}", key, e);
                    anyhow!("Redis EXPIRE 失败: {}", e)
                })?;
            debug!("保存指标缓存: key={}, expire={}秒", key, seconds);
        } else {
            debug!("保存指标缓存: key={}, 永久有效", key);
        }
        
        Ok(())
    }
    
    /// 从缓存获取指标数据
    /// 
    /// # 参数
    /// - `key`: 缓存键
    /// 
    /// # 返回
    /// - `Ok(Some(T))`: 缓存存在且反序列化成功
    /// - `Ok(None)`: 缓存不存在
    /// - `Err`: Redis 错误或反序列化失败
    pub async fn get<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>> {
        // 从 Redis 获取
        let json_str: Option<String> = self.redis_client.get(key).await
            .map_err(|e| {
                error!("Redis GET 失败: key={}, error={}", key, e);
                anyhow!("Redis GET 失败: {}", e)
            })?;
        
        match json_str {
            Some(s) => {
                // 反序列化
                let value = serde_json::from_str::<T>(&s)
                    .map_err(|e| anyhow!("反序列化失败: {}", e))?;
                debug!("获取指标缓存命中: key={}", key);
                Ok(Some(value))
            }
            None => {
                debug!("获取指标缓存未命中: key={}", key);
                Ok(None)
            }
        }
    }
    
    /// 删除缓存
    /// 
    /// # 参数
    /// - `key`: 缓存键
    pub async fn delete(&mut self, key: &str) -> Result<()> {
        let deleted: i32 = self.redis_client.del(key).await
            .map_err(|e| {
                error!("Redis DEL 失败: key={}, error={}", key, e);
                anyhow!("Redis DEL 失败: {}", e)
            })?;
        
        if deleted > 0 {
            debug!("删除指标缓存: key={}", key);
        } else {
            debug!("缓存不存在: key={}", key);
        }
        
        Ok(())
    }
    
    /// 批量删除缓存（按模式匹配）
    /// 
    /// # 参数
    /// - `pattern`: 匹配模式，如 "indicator:vegas:*"
    pub async fn delete_by_pattern(&mut self, pattern: &str) -> Result<usize> {
        // 使用 SCAN 命令获取所有匹配的键
        let mut cursor = 0;
        let mut deleted_count = 0;
        
        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.redis_client)
                .await
                .map_err(|e| anyhow!("Redis SCAN 失败: {}", e))?;
            
            // 删除匹配的键
            if !keys.is_empty() {
                let deleted: i32 = self.redis_client.del(&keys).await
                    .map_err(|e| anyhow!("Redis DEL 失败: {}", e))?;
                deleted_count += deleted as usize;
            }
            
            cursor = new_cursor;
            if cursor == 0 {
                break;
            }
        }
        
        debug!("批量删除缓存: pattern={}, count={}", pattern, deleted_count);
        Ok(deleted_count)
    }
    
    /// 检查缓存是否存在
    pub async fn exists(&mut self, key: &str) -> Result<bool> {
        let exists: bool = self.redis_client.exists(key).await
            .map_err(|e| anyhow!("Redis EXISTS 失败: {}", e))?;
        Ok(exists)
    }
    
    /// 设置缓存过期时间
    pub async fn expire(&mut self, key: &str, seconds: usize) -> Result<()> {
        self.redis_client.expire::<_, ()>(key, seconds as i64).await
            .map_err(|e| anyhow!("Redis EXPIRE 失败: {}", e))?;
        Ok(())
    }
}

// 注意：arc_vegas_indicator_values.rs 和 arc_nwe_indicator_values.rs 
// 已经存在于 infrastructure/cache/ 目录中，无需重复迁移


