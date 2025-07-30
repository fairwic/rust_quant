use crate::trading::indicator::vegas_indicator::IndicatorCombine;
use crate::trading::indicator::vegas_indicator::VegasIndicatorSignalValue;
use crate::CandleItem;
use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt::format;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::{error, info, warn};

// 定义最大容量常量
const MAX_CANDLE_ITEMS: usize = 10000;
const MAX_LOCKS_WAIT_TIME_MS: u64 = 500; // 最大锁等待时间

#[derive(Debug, Clone)]
pub struct ArcVegasIndicatorValues {
    pub timestamp: i64,
    pub inst_id: String,
    pub period: String,
    pub candle_item: VecDeque<CandleItem>,
    pub indicator_combines: IndicatorCombine,
}

impl Default for ArcVegasIndicatorValues {
    fn default() -> Self {
        Self {
            timestamp: 0,
            inst_id: "".to_string(),
            period: "".to_string(),
            candle_item: VecDeque::new(),
            indicator_combines: IndicatorCombine::default(),
        }
    }
}

/// 获取hash key
pub fn get_hash_key(inst_id: &str, period: &str, strategy_type: &str) -> String {
    format!("{} {} {}", inst_id, period, strategy_type)
}

// 指标值存储管理器 - 替代全局静态变量
#[derive(Clone)]
pub struct IndicatorValuesManager {
    values: Arc<RwLock<HashMap<String, ArcVegasIndicatorValues>>>,
    metrics: Arc<RwLock<HashMap<String, IndicatorMetrics>>>, // 记录性能指标
}

// 指标操作的性能指标
#[derive(Debug, Clone, Default)]
pub struct IndicatorMetrics {
    pub read_count: usize,
    pub write_count: usize,
    pub last_read_time_ms: u64,
    pub last_write_time_ms: u64,
    pub max_read_time_ms: u64,
    pub max_write_time_ms: u64,
}

impl IndicatorValuesManager {
    /// 创建新的管理器实例
    pub fn new() -> Self {
        Self {
            values: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取指定键的指标值
    pub async fn get(&self, key: &str) -> Option<ArcVegasIndicatorValues> {
        // 记录读取开始时间
        let start = Instant::now();

        // 获取读锁
        let read_result = tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.read(),
        )
        .await;

        // 处理锁获取超时
        let read_guard = match read_result {
            Ok(guard) => guard,
            Err(_) => {
                warn!("获取指标值读锁超时: {}", key);
                return None;
            }
        };

        // 获取值并克隆
        let result = read_guard.get(key).cloned();

        // 更新指标
        self.record_metrics(key, true, start.elapsed().as_millis() as u64)
            .await;

        result
    }

    /// 设置指标值
    pub async fn set(&self, key: String, value: ArcVegasIndicatorValues) -> Result<(), String> {
        // 记录写入开始时间
        let start = Instant::now();

        // 获取写锁
        let write_result = tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.write(),
        )
        .await;

        // 处理锁获取超时
        let mut write_guard = match write_result {
            Ok(guard) => guard,
            Err(_) => {
                let error_msg = format!("获取指标值写锁超时: {}", key);
                error!("{}", error_msg);
                return Err(error_msg);
            }
        };

        // 限制K线历史数据大小
        let mut value_with_limited_history = value.clone();
        if value_with_limited_history.candle_item.len() > MAX_CANDLE_ITEMS {
            // 使用VecDeque的高效操作：从前端移除多余的元素
            let excess = value_with_limited_history.candle_item.len() - MAX_CANDLE_ITEMS;
            for _ in 0..excess {
                value_with_limited_history.candle_item.pop_front();
            }
        }

        // 更新数据
        write_guard.insert(key.clone(), value_with_limited_history);

        // 更新指标
        self.record_metrics(&key, false, start.elapsed().as_millis() as u64)
            .await;

        Ok(())
    }

    /// 更新指标值中的K线数据
    pub async fn update_candle_items(
        &self,
        key: &str,
        candles: VecDeque<CandleItem>,
    ) -> Result<(), String> {
        let start = Instant::now();

        // 先检查键是否存在
        if !self.key_exists(key).await {
            return Err(format!("键 {} 不存在", key));
        }

        // 获取写锁
        let write_result = tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.write(),
        )
        .await;

        let mut write_guard = match write_result {
            Ok(guard) => guard,
            Err(_) => {
                let error_msg = format!("更新K线数据时获取写锁超时: {}", key);
                error!("{}", error_msg);
                return Err(error_msg);
            }
        };

        // 更新K线数据
        if let Some(values) = write_guard.get_mut(key) {
            // 限制数据大小 - 使用VecDeque的高效操作
            values.candle_item = candles;
            if values.candle_item.len() > MAX_CANDLE_ITEMS {
                let excess = values.candle_item.len() - MAX_CANDLE_ITEMS;
                for _ in 0..excess {
                    values.candle_item.pop_front();
                }
            }

            // 更新指标
            self.record_metrics(key, false, start.elapsed().as_millis() as u64)
                .await;

            Ok(())
        } else {
            Err(format!("获取写锁后键 {} 不存在", key))
        }
    }

    /// 更新指标计算结果
    pub async fn update_indicator_values(
        &self,
        key: &str,
        indicators: IndicatorCombine,
    ) -> Result<(), String> {
        let start = Instant::now();

        // 先检查键是否存在
        if !self.key_exists(key).await {
            return Err(format!("键 {} 不存在", key));
        }

        // 获取写锁
        let write_result = tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.write(),
        )
        .await;

        let mut write_guard = match write_result {
            Ok(guard) => guard,
            Err(_) => {
                let error_msg = format!("更新指标值时获取写锁超时: {}", key);
                error!("{}", error_msg);
                return Err(error_msg);
            }
        };

        // 更新指标数据
        if let Some(values) = write_guard.get_mut(key) {
            values.indicator_combines = indicators;

            // 更新指标
            self.record_metrics(key, false, start.elapsed().as_millis() as u64)
                .await;

            Ok(())
        } else {
            Err(format!("获取写锁后键 {} 不存在", key))
        }
    }

    /// 检查键是否存在
    pub async fn key_exists(&self, key: &str) -> bool {
        match tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.read(),
        )
        .await
        {
            Ok(guard) => guard.contains_key(key),
            Err(_) => {
                warn!("检查键是否存在时获取读锁超时: {}", key);
                false
            }
        }
    }

    /// 获取所有键
    pub async fn get_all_keys(&self) -> Vec<String> {
        match tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.read(),
        )
        .await
        {
            Ok(guard) => guard.keys().cloned().collect(),
            Err(_) => {
                warn!("获取所有键时读锁超时");
                vec![]
            }
        }
    }
    /// 获取当前缓存的键数量
    pub async fn get_count(&self) -> usize {
        match tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.read(),
        )
        .await
        {
            Ok(guard) => guard.len(),
            Err(_) => {
                warn!("获取键数量时读锁超时");
                0
            }
        }
    }
    /// 清除指定键的数据
    pub async fn remove(&self, key: &str) -> bool {
        match tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.values.write(),
        )
        .await
        {
            Ok(mut guard) => {
                let removed = guard.remove(key).is_some();
                if removed {
                    // 同时清理指标记录
                    if let Ok(mut metrics_guard) = tokio::time::timeout(
                        Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
                        self.metrics.write(),
                    )
                    .await
                    {
                        metrics_guard.remove(key);
                    }
                }
                removed
            }
            Err(_) => {
                warn!("移除键 {} 时写锁超时", key);
                false
            }
        }
    }
    /// 记录性能指标
    async fn record_metrics(&self, key: &str, is_read: bool, elapsed_ms: u64) {
        if let Ok(mut metrics_guard) = tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.metrics.write(),
        )
        .await
        {
            let entry = metrics_guard
                .entry(key.to_string())
                .or_insert_with(IndicatorMetrics::default);
            if is_read {
                entry.read_count += 1;
                entry.last_read_time_ms = elapsed_ms;
                entry.max_read_time_ms = entry.max_read_time_ms.max(elapsed_ms);
            } else {
                entry.write_count += 1;
                entry.last_write_time_ms = elapsed_ms;
                entry.max_write_time_ms = entry.max_write_time_ms.max(elapsed_ms);
            }
        }
    }
    /// 获取性能指标
    pub async fn get_metrics(&self, key: &str) -> Option<IndicatorMetrics> {
        match tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.metrics.read(),
        )
        .await
        {
            Ok(metrics_guard) => metrics_guard.get(key).cloned(),
            Err(_) => None,
        }
    }

    /// 获取所有性能指标
    pub async fn get_all_metrics(&self) -> HashMap<String, IndicatorMetrics> {
        match tokio::time::timeout(
            Duration::from_millis(MAX_LOCKS_WAIT_TIME_MS),
            self.metrics.read(),
        )
        .await
        {
            Ok(metrics_guard) => metrics_guard.clone(),
            Err(_) => HashMap::new(),
        }
    }
}

// 全局单例实例
pub static INDICATOR_MANAGER: OnceCell<IndicatorValuesManager> = OnceCell::new();

// 获取全局管理器实例
pub fn get_indicator_manager() -> &'static IndicatorValuesManager {
    INDICATOR_MANAGER.get_or_init(|| IndicatorValuesManager::new())
}

// 为了向后兼容，保留原来的全局变量，但改为从管理器获取数据
pub static VEGAS_INDICATOR_VALUES: OnceCell<RwLock<HashMap<String, ArcVegasIndicatorValues>>> =
    OnceCell::new();

pub fn get_vegas_indicator_values() -> &'static RwLock<HashMap<String, ArcVegasIndicatorValues>> {
    VEGAS_INDICATOR_VALUES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 设置策略指标值 - 使用新的管理器
pub async fn set_strategy_indicator_values(
    inst_id: String,
    period: String,
    mille_time: i64,
    hash_key: String,
    candle_items: VecDeque<CandleItem>,
    values: IndicatorCombine,
) {
    let arc_vegas_indicator_values = ArcVegasIndicatorValues {
        timestamp: mille_time,
        inst_id: inst_id.clone(),
        period: period.clone(),
        candle_item: candle_items,
        indicator_combines: values,
    };

    // 使用新的管理器设置值
    if let Err(e) = get_indicator_manager()
        .set(hash_key.clone(), arc_vegas_indicator_values)
        .await
    {
        error!("设置策略指标值失败: {}", e);
    } else {
        info!("策略指标值已设置: {}", hash_key);
    }
}

/// 根据哈希键获取指标值 - 使用新的管理器
pub async fn get_vegas_indicator_values_by_inst_id_with_period(
    inst_id_with_period: String,
) -> Option<ArcVegasIndicatorValues> {
    get_indicator_manager().get(&inst_id_with_period).await
}

/// 更新策略指标值中的K线数据 - 使用新的管理器
pub async fn update_candle_items(hash_key: &str, candles: VecDeque<CandleItem>) -> Result<(), String> {
    get_indicator_manager()
        .update_candle_items(hash_key, candles)
        .await
}

/// 更新策略指标值中的指标值 - 使用新的管理器
pub async fn update_vegas_indicator_values(
    hash_key: &str,
    indicator_combine: IndicatorCombine,
) -> Result<(), String> {
    get_indicator_manager()
        .update_indicator_values(hash_key, indicator_combine)
        .await
}

// 新增的辅助函数，用于性能监控
pub async fn get_indicators_performance_metrics() -> HashMap<String, IndicatorMetrics> {
    get_indicator_manager().get_all_metrics().await
}

// 新增的辅助函数，用于获取当前缓存状态
pub async fn get_indicators_cache_stats() -> (usize, Vec<String>) {
    let manager = get_indicator_manager();
    let count = manager.get_count().await;
    let keys = manager.get_all_keys().await;
    (count, keys)
}
