use crate::trading::indicator::vegas_indicator::IndicatorCombine;
use crate::trading::indicator::vegas_indicator::VegasIndicatorSignalValue;
use crate::CandleItem;
use chrono::{DateTime, TimeZone, Utc};
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt::format;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};

// 定义最大容量常量
const MAX_CANDLE_ITEMS: usize = 10000;

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
    values: Arc<DashMap<String, ArcVegasIndicatorValues>>,
    metrics: Arc<DashMap<String, IndicatorMetrics>>, // 记录性能指标
    key_mutex: Arc<DashMap<String, Arc<Mutex<()>>>>, // 每键互斥，防止同键重入
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
            values: Arc::new(DashMap::new()),
            metrics: Arc::new(DashMap::new()),
            key_mutex: Arc::new(DashMap::new()),
        }
    }

    /// 获取指定键的指标值
    pub async fn get(&self, key: &str) -> Option<ArcVegasIndicatorValues> {
        let start = Instant::now();
        let result = self.values.get(key).map(|r| r.clone());
        self.record_metrics(key, true, start.elapsed().as_millis() as u64)
            .await;
        result
    }

    /// 仅返回末 n 根K线与当前指标的轻量快照，减少大对象克隆
    pub async fn get_snapshot_last_n(
        &self,
        key: &str,
        n: usize,
    ) -> Option<(Vec<CandleItem>, IndicatorCombine, i64)> {
        let start = Instant::now();
        let result = self.values.get(key).map(|r| {
            let v = r.value();
            let len = v.candle_item.len();
            let take_n = n.min(len);
            let mut last_n: Vec<CandleItem> = Vec::with_capacity(take_n);
            // 只克隆末 n 根，保持原始顺序
            for i in len.saturating_sub(take_n)..len {
                last_n.push(v.candle_item[i].clone());
            }
            (last_n, v.indicator_combines.clone(), v.timestamp)
        });
        self.record_metrics(key, true, start.elapsed().as_millis() as u64)
            .await;
        result
    }

    /// 设置指标值
    pub async fn set(&self, key: String, value: ArcVegasIndicatorValues) -> Result<(), String> {
        let start = Instant::now();
        let mut value_with_limited_history = value.clone();
        if value_with_limited_history.candle_item.len() > MAX_CANDLE_ITEMS {
            let excess = value_with_limited_history.candle_item.len() - MAX_CANDLE_ITEMS;
            for _ in 0..excess {
                value_with_limited_history.candle_item.pop_front();
            }
        }
        self.values.insert(key.clone(), value_with_limited_history);
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
        if !self.key_exists(key).await {
            return Err(format!("键 {} 不存在", key));
        }
        if let Some(mut entry) = self.values.get_mut(key) {
            let values = entry.value_mut();
            values.candle_item = candles;
            if values.candle_item.len() > MAX_CANDLE_ITEMS {
                let excess = values.candle_item.len() - MAX_CANDLE_ITEMS;
                for _ in 0..excess {
                    values.candle_item.pop_front();
                }
            }
            self.record_metrics(key, false, start.elapsed().as_millis() as u64)
                .await;
            Ok(())
        } else {
            Err(format!("键 {} 不存在", key))
        }
    }

    /// 更新指标计算结果
    pub async fn update_indicator_values(
        &self,
        key: &str,
        indicators: IndicatorCombine,
    ) -> Result<(), String> {
        let start = Instant::now();
        if !self.key_exists(key).await {
            return Err(format!("键 {} 不存在", key));
        }
        if let Some(mut entry) = self.values.get_mut(key) {
            let values = entry.value_mut();
            values.indicator_combines = indicators;
            self.record_metrics(key, false, start.elapsed().as_millis() as u64)
                .await;
            Ok(())
        } else {
            Err(format!("键 {} 不存在", key))
        }
    }

    /// 原子更新：同时更新K线与指标，避免中间态
    pub async fn update_both(
        &self,
        key: &str,
        candles: VecDeque<CandleItem>,
        indicators: IndicatorCombine,
        timestamp: i64,
    ) -> Result<(), String> {
        let start = Instant::now();
        if !self.key_exists(key).await {
            return Err(format!("键 {} 不存在", key));
        }
        if let Some(mut entry) = self.values.get_mut(key) {
            let values = entry.value_mut();
            let mut new_candles = candles;
            if new_candles.len() > MAX_CANDLE_ITEMS {
                let excess = new_candles.len() - MAX_CANDLE_ITEMS;
                for _ in 0..excess {
                    new_candles.pop_front();
                }
            }
            values.candle_item = new_candles;
            values.indicator_combines = indicators;
            values.timestamp = timestamp;
            self.record_metrics(key, false, start.elapsed().as_millis() as u64)
                .await;
            Ok(())
        } else {
            Err(format!("键 {} 不存在", key))
        }
    }

    /// 检查键是否存在
    pub async fn key_exists(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 获取所有键
    pub async fn get_all_keys(&self) -> Vec<String> {
        self.values.iter().map(|e| e.key().clone()).collect()
    }
    /// 获取当前缓存的键数量
    pub async fn get_count(&self) -> usize {
        self.values.len()
    }
    /// 清除指定键的数据
    pub async fn remove(&self, key: &str) -> bool {
        let removed = self.values.remove(key).is_some();
        if removed {
            self.metrics.remove(key);
        }
        removed
    }
    /// 记录性能指标
    async fn record_metrics(&self, key: &str, is_read: bool, elapsed_ms: u64) {
        let mut entry = self
            .metrics
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
    /// 获取性能指标
    pub async fn get_metrics(&self, key: &str) -> Option<IndicatorMetrics> {
        self.metrics.get(key).map(|e| e.clone())
    }

    /// 获取所有性能指标
    pub async fn get_all_metrics(&self) -> HashMap<String, IndicatorMetrics> {
        self.metrics
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    /// 获取某键的互斥锁（不存在则插入）
    pub async fn acquire_key_mutex(&self, key: &str) -> Arc<Mutex<()>> {
        if let Some(m) = self.key_mutex.get(key) {
            return m.value().clone();
        }
        let mutex = Arc::new(Mutex::new(()));
        let entry = self
            .key_mutex
            .entry(key.to_string())
            .or_insert_with(|| mutex.clone());
        entry.value().clone()
    }
}

// 全局单例实例
pub static INDICATOR_MANAGER: OnceCell<IndicatorValuesManager> = OnceCell::new();

// 获取全局管理器实例
pub fn get_indicator_manager() -> &'static IndicatorValuesManager {
    INDICATOR_MANAGER.get_or_init(|| IndicatorValuesManager::new())
}

// // 为了向后兼容，保留原来的全局变量，但改为从管理器获取数据
// pub static VEGAS_INDICATOR_VALUES: OnceCell<RwLock<HashMap<String, ArcVegasIndicatorValues>>> =
//     OnceCell::new();

// pub fn get_vegas_indicator_values() -> &'static RwLock<HashMap<String, ArcVegasIndicatorValues>> {
//     VEGAS_INDICATOR_VALUES.get_or_init(|| RwLock::new(HashMap::new()))
// }

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
pub async fn update_candle_items(
    hash_key: &str,
    candles: VecDeque<CandleItem>,
) -> Result<(), String> {
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
