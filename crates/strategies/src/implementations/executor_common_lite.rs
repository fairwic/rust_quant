//! 策略执行器公共逻辑（轻量级版本）
//!
//! 提取不依赖 orchestration 的通用逻辑，避免循环依赖
//!
//! 注意：本模块不包含数据库访问逻辑，调用方需要自行获取数据

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use tracing::debug;

use rust_quant_common::CandleItem;

/// 执行上下文 - 封装策略执行的公共数据
pub struct ExecutionContext {
    pub inst_id: String,
    pub period: String,
    pub hash_key: String,
    pub new_candle_item: CandleItem,
    pub new_candle_items: VecDeque<CandleItem>,
}

/// 更新K线队列
pub fn update_candle_queue(
    candle_items: &mut VecDeque<CandleItem>,
    new_candle: CandleItem,
    max_size: usize,
) {
    candle_items.push_back(new_candle);
    if candle_items.len() > max_size {
        let excess = candle_items.len() - max_size;
        for _ in 0..excess {
            candle_items.pop_front();
        }
    }
}

/// 获取最近N根K线切片
pub fn get_recent_candles(candle_items: &VecDeque<CandleItem>, n: usize) -> Vec<CandleItem> {
    candle_items.iter().rev().take(n).cloned().rev().collect()
}

/// 转换K线数据
pub fn convert_candles_to_items(candles: &[CandleItem]) -> VecDeque<CandleItem> {
    candles.iter().cloned().collect()
}

/// 验证K线数据
pub fn validate_candles(candles: &[CandleItem]) -> Result<i64> {
    if candles.is_empty() {
        return Err(anyhow!("K线数据为空"));
    }

    let last_ts = candles
        .last()
        .ok_or_else(|| anyhow!("无法获取最后一根K线"))?
        .ts;

    debug!(
        "K线数据验证通过，共 {} 根，最后时间戳: {}",
        candles.len(),
        last_ts
    );
    Ok(last_ts)
}

/// 基础的时间戳检查
pub fn is_new_timestamp(old_time: i64, new_time: i64) -> bool {
    if new_time <= old_time {
        debug!("时间未更新: old={}, new={}, 跳过执行", old_time, new_time);
        return false;
    }
    true
}
