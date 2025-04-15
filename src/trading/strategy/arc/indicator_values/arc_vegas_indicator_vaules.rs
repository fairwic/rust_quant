use crate::trading::indicator::vegas_indicator::VegasIndicatorSignalValue;
use crate::trading::indicator::vegas_indicator::IndicatorCombine;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::fmt::format;
use tokio::sync::RwLock;
use tracing::{error, info};
use crate::CandleItem;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug,Clone)]
pub struct ArcVegasIndicatorValues {
    pub timestamp: i64,
    pub inst_id: String,
    pub period: String,
    pub candle_item: Vec<CandleItem>,
    pub indicator_combines: IndicatorCombine
}

impl Default for ArcVegasIndicatorValues {
    fn default() -> Self {
        Self {
            timestamp: 0,
            inst_id: "".to_string(),
            period: "".to_string(),
            candle_item: vec![],
            indicator_combines: IndicatorCombine::default()
        }
    }
}

/// 获取hash key
pub fn get_hash_key(inst_id: &str, period: &str, strategy_type: &str) -> String {
    format!("{} {} {}", inst_id, period, strategy_type)
}

/// 存储策略的信号值 - 使用RwLock替代Mutex提高并发性能
pub static VEGAS_INDICATOR_VALUES: OnceCell<RwLock<HashMap<String, ArcVegasIndicatorValues>>> = OnceCell::new();

pub fn get_vegas_indicator_values() -> &'static RwLock<HashMap<String, ArcVegasIndicatorValues>> {
    VEGAS_INDICATOR_VALUES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 设置策略指标值
pub async fn set_ema_indicator_values(inst_id: String, period: String,milltime:i64, hash_key: String,candle_items:Vec<CandleItem>, values: IndicatorCombine) {
    let arc_vegas_indicator_values = ArcVegasIndicatorValues {
        timestamp: milltime, // 使用当前时间戳
        inst_id: inst_id.clone(),
        period: period.clone(),
        candle_item: candle_items, // 这里可以根据需要保存历史数据
        indicator_combines: values,
    };

    // 使用写锁更新数据
    let mut write_lock = get_vegas_indicator_values().write().await;
    write_lock.insert(hash_key, arc_vegas_indicator_values);
}

/// 根据哈希键获取指标值
pub async fn get_vegas_indicator_values_by_inst_id_with_period(
    inst_id_with_period: String,
) -> Option<ArcVegasIndicatorValues> {
    // 使用读锁获取数据，允许多个读取同时进行
    let read_lock = get_vegas_indicator_values().read().await;
    read_lock.get(&inst_id_with_period).cloned()
}

/// 使用通用函数处理写锁操作，减少代码重复
async fn with_write_lock<F, T>(hash_key: &str, f: F) -> Result<T, String>
where
    F: FnOnce(&mut ArcVegasIndicatorValues) -> T,
{
    // 首先用读锁检查键是否存在
    {
        let read_lock = get_vegas_indicator_values().read().await;
        if !read_lock.contains_key(hash_key) {
            return Err(format!("找不到键 {}", hash_key));
        }
    }
    
    // 然后获取写锁并执行操作
    let mut write_lock = get_vegas_indicator_values().write().await;
    match write_lock.get_mut(hash_key) {
        Some(values) => Ok(f(values)),
        None => Err(format!("获取写锁后键 {} 不存在", hash_key)), // 极少发生，但做好防御
    }
}

/// 更新策略指标值中的K线数据
pub async fn update_candle_items(hash_key: &str, candles: Vec<CandleItem>) -> Result<(), String> {
    tracing::info!("update_candle_items开始 - 键: {}, 数据项数量: {}", hash_key, candles.len());
    
    with_write_lock(hash_key, |values| {
        tracing::info!("更新数据开始 - 当前K线数量: {}, 新K线数量: {}", 
                       values.candle_item.len(), candles.len());
        values.candle_item = candles;
        tracing::info!("更新数据完成");
    }).await
}

/// 更新策略指标值中的指标值
pub async fn update_vegas_indicator_values(hash_key: &str, indicator_combine: IndicatorCombine) -> Result<(), String> {
    tracing::info!("update_vegas_indicator_values开始 - 键: {}", hash_key);
    
    with_write_lock(hash_key, |values| {
        values.indicator_combines = indicator_combine;
        tracing::info!("更新指标值完成");
    }).await
}
