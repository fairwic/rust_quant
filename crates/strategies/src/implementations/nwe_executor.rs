//! Nwe 策略执行器
//!
//! 封装 Nwe 策略的数据初始化和执行逻辑

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::VecDeque;
use tracing::{debug, info};

use super::executor_common::{
    convert_candles_to_items, get_latest_candle, get_recent_candles, is_new_timestamp,
    update_candle_queue, validate_candles,
};
use crate::cache::arc_nwe_indicator_values::{
    get_nwe_hash_key, get_nwe_indicator_manager, set_nwe_strategy_indicator_values,
};
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::implementations::nwe_strategy::{NweSignalValues, NweStrategy, NweStrategyConfig};
use crate::strategy_common::{parse_candle_to_data_item, SignalResult};
use crate::StrategyType;
use rust_quant_common::CandleItem;
use rust_quant_indicators::trend::nwe::NweIndicatorValues;

/// Nwe 策略执行器
pub struct NweStrategyExecutor;

impl NweStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for NweStrategyExecutor {
    fn name(&self) -> &'static str {
        "Nwe"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::Nwe
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        serde_json::from_str::<NweStrategyConfig>(strategy_config).is_ok()
    }

    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandleItem>,
    ) -> Result<StrategyDataResult> {
        debug!("初始化 Nwe 策略数据: {}_{}", inst_id, period);

        // 1. 验证K线数据并获取时间戳
        let last_timestamp = validate_candles(&candles)?;

        // 2. 解析 Nwe 策略配置
        let nwe_config: NweStrategyConfig =
            serde_json::from_value(strategy_config.parameters.clone())
                .map_err(|e| anyhow!("解析 NweStrategyConfig 失败: {}", e))?;

        // 3. 转换K线数据并计算指标
        let nwe_strategy = NweStrategy::new(nwe_config);
        let mut indicator_combine = nwe_strategy.get_indicator_combine();
        let candle_items = convert_candles_to_items(&candles);

        for item in &candle_items {
            indicator_combine.next(item);
        }

        // 4. 生成存储键并保存数据
        let hash_key = get_nwe_hash_key(inst_id, period, StrategyType::Nwe.as_str());

        set_nwe_strategy_indicator_values(
            inst_id.to_string(),
            period.to_string(),
            last_timestamp,
            hash_key.clone(),
            candle_items,
            indicator_combine,
        )
        .await;

        // 5. 验证数据保存成功
        let manager = get_nwe_indicator_manager();
        if !manager.key_exists(&hash_key).await {
            return Err(anyhow!("Nwe 策略数据保存验证失败: {}", hash_key));
        }

        info!("✅ Nwe 策略数据初始化完成: {}", hash_key);

        Ok(StrategyDataResult {
            hash_key,
            last_timestamp,
        })
    }

    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandleItem>,
    ) -> Result<SignalResult> {
        const MAX_HISTORY_SIZE: usize = 500;

        // 1. 获取哈希键和管理器
        let key = get_nwe_hash_key(inst_id, period, StrategyType::Nwe.as_str());
        let manager = get_nwe_indicator_manager();

        // 2. 获取最新K线数据（使用公共函数）
        let new_candle_data = get_latest_candle(inst_id, period, snap).await?;
        let new_candle_item = parse_candle_to_data_item(&new_candle_data);

        // 3. 获取互斥锁和缓存快照
        let key_mutex = manager.acquire_key_mutex(&key).await;
        let _guard = key_mutex.lock().await;

        let (last_candles_vec, mut old_indicator_combines, old_time) = manager
            .get_snapshot_last_n(&key, MAX_HISTORY_SIZE)
            .await
            .ok_or_else(|| anyhow!("没有找到对应的 Nwe 策略值: {}", key))?;

        let mut new_candle_items: VecDeque<CandleItem> = last_candles_vec.into_iter().collect();

        // 4. 检查是否应该执行（使用简化版本，只检查时间戳）
        if !is_new_timestamp(old_time, new_candle_item.ts) {
            debug!(
                "时间未更新，跳过策略执行: old_time={}, new_time={}",
                old_time, new_candle_item.ts
            );
            // 返回空的信号结果
            return Ok(SignalResult {
                should_buy: false,
                should_sell: false,
                open_price: new_candle_item.c,
                best_open_price: None,
                atr_take_profit_ratio_price: None,
                atr_stop_loss_price: None,
                long_signal_take_profit_price: None,
                short_signal_take_profit_price: None,
                ts: new_candle_item.ts,
                single_value: None,
                single_result: None,
                signal_kline_stop_loss_price: None,
            stop_loss_source: None,
                move_stop_open_price_when_touch_price: None,
                counter_trend_pullback_take_profit_price: None,
                is_ema_short_trend: None,
                is_ema_long_trend: None,
                atr_take_profit_level_1: None,
                atr_take_profit_level_2: None,
                atr_take_profit_level_3: None,
                filter_reasons: vec![],
                direction: rust_quant_domain::SignalDirection::None,
            });
        }

        // 5. 更新指标值
        let new_indicator_values: NweIndicatorValues =
            old_indicator_combines.next(&new_candle_item);

        // 6. 将 NweIndicatorValues 转换为 NweSignalValues
        let nwe_signal_values = NweSignalValues {
            stc_value: new_indicator_values.stc_value,
            volume_ratio: new_indicator_values.volume_ratio,
            atr_value: new_indicator_values.atr_value,
            atr_short_stop: new_indicator_values.atr_short_stop,
            atr_long_stop: new_indicator_values.atr_long_stop,
            nwe_upper: new_indicator_values.nwe_upper,
            nwe_lower: new_indicator_values.nwe_lower,
        };

        // 7. 更新K线队列（使用公共函数）
        update_candle_queue(
            &mut new_candle_items,
            new_candle_item.clone(),
            MAX_HISTORY_SIZE,
        );

        // 8. 原子更新缓存
        manager
            .update_both(
                &key,
                new_candle_items.clone(),
                old_indicator_combines.clone(),
                new_candle_item.ts,
            )
            .await
            .map_err(|e| anyhow!("原子更新 Nwe 指标与K线失败: {}", e))?;

        // 9. 获取最近K线用于信号计算（需要足够的历史数据用于动态波动率计算）
        // 5分钟级别：近期48根+历史288根 = 336根，取500根确保足够
        let candle_vec = get_recent_candles(&new_candle_items, MAX_HISTORY_SIZE);

        // 10. 生成交易信号
        let nwe_config: NweStrategyConfig =
            serde_json::from_value(strategy_config.parameters.clone())
                .map_err(|e| anyhow!("解析 NweStrategyConfig 失败: {}", e))?;
        let mut nwe_strategy = NweStrategy::new(nwe_config);
        let risk_config = serde_json::from_value(strategy_config.risk_config.clone())
            .map_err(|e| anyhow!("解析风险配置失败: {}", e))?;
        let signal_result =
            nwe_strategy.get_trade_signal(&candle_vec, &nwe_signal_values, &risk_config);

        // 11. 记录信号生成结果
        if signal_result.should_buy || signal_result.should_sell {
            info!(
                "🎯 Nwe策略信号: key={}, buy={}, sell={}, price={:.4}, ts={}",
                key,
                signal_result.should_buy,
                signal_result.should_sell,
                signal_result.open_price,
                signal_result.ts
            );
        } else {
            debug!("Nwe策略无信号: key={}", key);
        }

        // 12. 返回信号（下单逻辑由 StrategyExecutionService 统一处理）
        Ok(signal_result)
    }
}
