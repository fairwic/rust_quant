//! 策略执行器公共逻辑
//!
//! 使用 trait 接口解耦 strategies 和 orchestration 的循环依赖
//!
//! 架构设计：
//! - strategies 定义 trait 接口（framework::execution_traits）
//! - orchestration 实现 trait 接口
//! - executor_common 依赖 trait 而非具体实现
//!
//! 这样实现单向依赖：orchestration → strategies

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use tracing::{debug, info, warn};

use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::execution_traits::{
    ExecutionStateManager, SignalLogger, StrategyExecutionContext, TimeChecker,
};
use crate::strategy_common::{parse_candle_to_data_item, BasicRiskStrategyConfig, SignalResult};
use crate::StrategyType;
use rust_quant_common::CandleItem;
use rust_quant_market::models::CandlesEntity;

/// 执行上下文 - 封装策略执行的公共数据
pub struct ExecutionContext {
    pub inst_id: String,
    pub period: String,
    pub hash_key: String,
    pub new_candle_item: CandleItem,
    pub new_candle_items: VecDeque<CandleItem>,
}

/// 检查时间戳和去重（使用 trait 接口）
pub fn should_execute_strategy(
    key: &str,
    old_time: i64,
    new_time: i64,
    period: &str,
    is_update: bool,
    context: &dyn StrategyExecutionContext,
) -> Result<bool> {
    // 1. 验证时间戳
    let time_checker = context.time_checker();
    let is_new_time = time_checker.check_new_time(old_time, new_time, period, is_update, true)?;
    if !is_new_time {
        debug!("时间未更新，跳过策略执行");
        return Ok(false);
    }

    // 2. 去重检查
    let state_manager = context.state_manager();
    if !state_manager.try_mark_processing(key, new_time) {
        debug!("重复执行检测，跳过策略执行");
        return Ok(false);
    }

    Ok(true)
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

/// 处理策略信号（仅记录日志）
///
/// 注意：实际的订单执行应该由 orchestration 或 execution 层负责
/// strategies 层只负责产生信号，不负责执行订单
pub fn process_signal(
    strategy_type: &StrategyType,
    inst_id: &str,
    period: &str,
    signal_result: &SignalResult,
    context: &dyn StrategyExecutionContext,
) -> Result<()> {
    if !signal_result.should_buy && !signal_result.should_sell {
        info!(
            "无交易信号 策略类型：{},交易周期：{}",
            strategy_type.as_str(),
            period
        );
        return Ok(());
    }

    warn!(
        "{} 策略信号！inst_id={}, period={}, should_buy={}, should_sell={}, ts={}",
        strategy_type.as_str(),
        inst_id,
        period,
        signal_result.should_buy,
        signal_result.should_sell,
        signal_result.ts
    );

    // 记录信号日志（使用 trait）
    let signal_logger = context.signal_logger();
    signal_logger.save_signal_log(inst_id, period, signal_result);

    Ok(())
}

/// 提取风险配置
pub fn extract_risk_config(strategy_config: &StrategyConfig) -> Result<BasicRiskStrategyConfig> {
    serde_json::from_value(strategy_config.risk_config.clone())
        .map_err(|e| anyhow!("解析风险配置失败: {}", e))
}

/// 转换K线数据为 CandleItem
pub fn convert_candles_to_items(candles: &[CandlesEntity]) -> VecDeque<CandleItem> {
    candles
        .iter()
        .map(|candle| parse_candle_to_data_item(candle))
        .collect()
}

/// 获取最近的 N 根 K 线数据
pub async fn get_recent_candles_from_db(
    inst_id: &str,
    period: &str,
    limit: usize,
) -> Result<Vec<CandlesEntity>> {
    // TODO: 这里需要使用 infrastructure::repositories::CandleRepository
    // 暂时返回空，调用方应该自己获取数据
    let _ = (inst_id, period, limit);
    Err(anyhow!("请使用 CandleRepository 获取历史数据"))
}

/// 验证K线数据
pub fn validate_candles(candles: &[CandlesEntity]) -> Result<i64> {
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

/// 基础的时间戳检查（不依赖 trait）
pub fn is_new_timestamp(old_time: i64, new_time: i64) -> bool {
    if new_time <= old_time {
        debug!("时间未更新: old={}, new={}, 跳过执行", old_time, new_time);
        return false;
    }
    true
}

/// 获取最新K线数据（公共逻辑）
///
/// 优先使用传入的 snap，如果没有则返回错误（需要调用方自行获取）
pub async fn get_latest_candle(
    _inst_id: &str,
    _period: &str,
    snap: Option<CandlesEntity>,
) -> Result<CandlesEntity> {
    if let Some(snap) = snap {
        Ok(snap)
    } else {
        Err(anyhow!("需要提供 K 线快照数据，或通过其他方式获取最新K线"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::execution_traits::DefaultExecutionContext;

    #[test]
    fn test_update_candle_queue() {
        let mut queue = VecDeque::new();
        let candle = CandleItem {
            ts: 1000,
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 105.0,
            v: 1000.0,
            confirm: 1,
        };

        update_candle_queue(&mut queue, candle, 3);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_is_new_timestamp() {
        assert_eq!(is_new_timestamp(1000, 2000), true);
        assert_eq!(is_new_timestamp(2000, 1000), false);
        assert_eq!(is_new_timestamp(1000, 1000), false);
    }

    #[tokio::test]
    async fn test_should_execute_strategy_with_noop() {
        let context = DefaultExecutionContext::new();
        let result = should_execute_strategy("BTC-USDT:1H", 1000, 2000, "1H", false, &context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }
}
