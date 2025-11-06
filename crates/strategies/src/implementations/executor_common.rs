//! 策略执行器公共逻辑
//!
//! 提取所有策略执行器的公共代码，减少重复

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use tracing::{debug, info, warn};

use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::services::order_service::swap_order_service::SwapOrderService;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::{
    parse_candle_to_data_item, BasicRiskStrategyConfig, SignalResult,
};
use crate::trading::strategy::StrategyType;
use crate::trading::task::strategy_runner::{
    check_new_time, save_signal_log, StrategyExecutionStateManager,
};
use crate::CandleItem;
use okx::dto::EnumToStrTrait;

/// 执行上下文 - 封装策略执行的公共数据
pub struct ExecutionContext {
    pub inst_id: String,
    pub period: String,
    pub strategy_type: StrategyType,
    pub hash_key: String,
    pub new_candle_item: CandleItem,
    pub new_candle_items: VecDeque<CandleItem>,
}

/// 获取最新K线数据（公共逻辑）
pub async fn get_latest_candle(
    inst_id: &str,
    period: &str,
    snap: Option<CandlesEntity>,
) -> Result<CandlesEntity> {
    if let Some(snap) = snap {
        Ok(snap)
    } else {
        CandleDomainService::new_default()
            .await
            .get_new_one_candle_fresh(inst_id, period, None)
            .await
            .map_err(|e| anyhow!("获取最新K线数据失败: {}", e))?
            .ok_or_else(|| {
                warn!("获取的最新K线数据为空: {:?}, {:?}", inst_id, period);
                anyhow!("K线数据为空")
            })
    }
}

/// 检查时间戳和去重（公共逻辑）
pub fn should_execute_strategy(
    key: &str,
    old_time: i64,
    new_time: i64,
    period: &str,
    is_update: bool,
) -> Result<bool> {
    // 1. 验证时间戳
    let is_new_time = check_new_time(old_time, new_time, period, is_update, true)?;
    if !is_new_time {
        debug!("时间未更新，跳过策略执行");
        return Ok(false);
    }

    // 2. 去重检查
    if !StrategyExecutionStateManager::try_mark_processing(key, new_time) {
        debug!("重复执行检测，跳过策略执行");
        return Ok(false);
    }

    Ok(true)
}

/// 更新K线队列（公共逻辑）
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

/// 获取最近N根K线切片（公共逻辑）
pub fn get_recent_candles(candle_items: &VecDeque<CandleItem>, n: usize) -> Vec<CandleItem> {
    candle_items.iter().rev().take(n).cloned().rev().collect()
}

/// 执行下单逻辑（公共逻辑）
pub async fn execute_order(
    strategy_type: &StrategyType,
    inst_id: &str,
    period: &str,
    signal_result: &SignalResult,
    strategy_config: &StrategyConfig,
) -> Result<()> {
    if !signal_result.should_buy && !signal_result.should_sell {
        info!(
            "无交易信号，跳过下单 策略类型：{},交易周期：{}",
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

    // 记录信号日志
    save_signal_log(inst_id, period, signal_result);

    // 解析风险配置
    let risk_config: BasicRiskStrategyConfig = serde_json::from_str(&strategy_config.risk_config)?;

    // 执行下单
    let res = SwapOrderService::new()
        .ready_to_order(
            strategy_type,
            inst_id,
            period,
            signal_result,
            &risk_config,
            strategy_config.strategy_config_id,
        )
        .await;

    match res {
        Ok(_) => {
            info!("✅ {} 策略下单成功", strategy_type.as_str());
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("❌ {} 策略下单失败: {}", strategy_type.as_str(), e);
            tracing::error!("{}", error_msg);
            Err(anyhow!(error_msg))
        }
    }
}

/// 转换K线数据（公共逻辑）
pub fn convert_candles_to_items(candles: &[CandlesEntity]) -> VecDeque<CandleItem> {
    candles
        .iter()
        .map(|candle| parse_candle_to_data_item(candle))
        .collect()
}

/// 验证K线数据（公共逻辑）
pub fn validate_candles(candles: &[CandlesEntity]) -> Result<i64> {
    if candles.is_empty() {
        return Err(anyhow!("K线数据为空"));
    }

    candles
        .last()
        .map(|c| c.ts)
        .ok_or_else(|| anyhow!("无法获取最新K线时间戳"))
}
