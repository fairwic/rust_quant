//! Nwe 策略指标值缓存管理器
//! 参考 arc_vegas_indicator_values.rs 的设计

use rust_quant_domain::nwe_strategy::indicator_combine::NweIndicatorCombine;
use rust_quant_common::CandleItem;
use dashmap::DashMap;
use once_cell::OnceCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{error, info};

// 定义最大容量常量
const MAX_CANDLE_ITEMS: usize = 100;

/// Nwe 策略指标值结构
#[derive(Debug, Clone)]
pub struct ArcNweIndicatorValues {
    pub timestamp: i64,
    pub inst_id: String,
    pub period: String,
    pub candle_item: VecDeque<CandleItem>,
    pub indicator_combines: NweIndicatorCombine,
}

impl Default for ArcNweIndicatorValues {
    fn default() -> Self {
        Self {
            timestamp: 0,
            inst_id: String::new(),
            period: String::new(),
            candle_item: VecDeque::new(),
            indicator_combines: NweIndicatorCombine::default(),
        }
    }
}

/// 获取 hash key
pub fn get_nwe_hash_key(inst_id: &str, period: &str, strategy_type: &str) -> String {
    format!("{} {} {}", inst_id, period, strategy_type)
}

/// Nwe 指标值管理器
#[derive(Clone)]
pub struct NweIndicatorValuesManager {
    values: Arc<DashMap<String, ArcNweIndicatorValues>>,
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

impl NweIndicatorValuesManager {
    /// 创建新的管理器实例
    pub fn new() -> Self {
        Self {
            values: Arc::new(DashMap::new()),
            metrics: Arc::new(DashMap::new()),
            key_mutex: Arc::new(DashMap::new()),
        }
    }

    /// 获取指定键的指标值
    pub async fn get(&self, key: &str) -> Option<ArcNweIndicatorValues> {
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
    ) -> Option<(Vec<CandleItem>, NweIndicatorCombine, i64)> {
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
    pub async fn set(&self, key: String, value: ArcNweIndicatorValues) -> Result<(), String> {
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
        indicators: NweIndicatorCombine,
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
        indicators: NweIndicatorCombine,
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

    /// 获取键互斥锁
    pub async fn acquire_key_mutex(&self, key: &str) -> Arc<Mutex<()>> {
        self.key_mutex
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone()
    }

    /// 记录性能指标
    async fn record_metrics(&self, key: &str, is_read: bool, time_ms: u64) {
        let mut entry = self.metrics.entry(key.to_string()).or_insert_with(|| {
            IndicatorMetrics {
                read_count: 0,
                write_count: 0,
                last_read_time_ms: 0,
                last_write_time_ms: 0,
                max_read_time_ms: 0,
                max_write_time_ms: 0,
            }
        });

        let metrics = entry.value_mut();
        if is_read {
            metrics.read_count += 1;
            metrics.last_read_time_ms = time_ms;
            metrics.max_read_time_ms = metrics.max_read_time_ms.max(time_ms);
        } else {
            metrics.write_count += 1;
            metrics.last_write_time_ms = time_ms;
            metrics.max_write_time_ms = metrics.max_write_time_ms.max(time_ms);
        }
    }

    /// 获取指标性能指标
    pub async fn get_metrics(&self, key: &str) -> Option<IndicatorMetrics> {
        self.metrics.get(key).map(|r| r.value().clone())
    }
}

// 全局单例实例
pub static NWE_INDICATOR_MANAGER: OnceCell<NweIndicatorValuesManager> = OnceCell::new();

/// 获取全局 Nwe 管理器实例
pub fn get_nwe_indicator_manager() -> &'static NweIndicatorValuesManager {
    NWE_INDICATOR_MANAGER.get_or_init(|| NweIndicatorValuesManager::new())
}

/// 设置 Nwe 策略指标值
pub async fn set_nwe_strategy_indicator_values(
    inst_id: String,
    period: String,
    timestamp: i64,
    hash_key: String,
    candle_items: VecDeque<CandleItem>,
    values: NweIndicatorCombine,
) {
    let arc_nwe_indicator_values = ArcNweIndicatorValues {
        timestamp,
        inst_id,
        period,
        candle_item: candle_items,
        indicator_combines: values,
    };

    if let Err(e) = get_nwe_indicator_manager()
        .set(hash_key.clone(), arc_nwe_indicator_values)
        .await
    {
        error!("设置 Nwe 策略指标值失败: {}", e);
    } else {
        info!("Nwe 策略指标值已设置: {}", hash_key);
    }
}

/// 根据哈希键获取指标值
pub async fn get_nwe_indicator_values_by_key(key: &str) -> Option<ArcNweIndicatorValues> {
    get_nwe_indicator_manager().get(key).await
}

/// 更新策略指标值中的K线数据
pub async fn update_nwe_candle_items(
    hash_key: &str,
    candles: VecDeque<CandleItem>,
) -> Result<(), String> {
    get_nwe_indicator_manager()
        .update_candle_items(hash_key, candles)
        .await
}

/// 更新策略指标值中的指标值
pub async fn update_nwe_indicator_values(
    hash_key: &str,
    indicators: NweIndicatorCombine,
) -> Result<(), String> {
    get_nwe_indicator_manager()
        .update_indicator_values(hash_key, indicators)
        .await
}

