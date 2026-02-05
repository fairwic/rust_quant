//! Vegas 策略执行器
//!
//! 封装 Vegas 策略的数据初始化和执行逻辑

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::VecDeque;
use tracing::{debug, info};

use super::executor_common::{
    convert_candles_to_items, get_latest_candle, get_recent_candles, is_new_timestamp,
    update_candle_queue, validate_candles,
};
use crate::cache::arc_vegas_indicator_values::{
    get_hash_key, get_indicator_manager, set_strategy_indicator_values,
};
use crate::framework::backtest::conversions::convert_domain_signal;
use crate::framework::config::strategy_config::StrategyConfig;
use crate::framework::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::strategy_common::{get_multi_indicator_values, parse_candle_to_data_item, SignalResult};
use crate::StrategyType;
use rust_quant_indicators::trend::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::VegasStrategy;
// ⏳ 移除orchestration依赖，避免循环依赖
// 使用 ExecutionContext trait 替代直接依赖
// use rust_quant_orchestration::workflow::strategy_runner::StrategyExecutionStateManager;
use rust_quant_common::CandleItem;

/// Vegas 策略执行器
pub struct VegasStrategyExecutor;

impl Default for VegasStrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl VegasStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for VegasStrategyExecutor {
    fn name(&self) -> &'static str {
        "Vegas"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::Vegas
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        serde_json::from_str::<VegasStrategy>(strategy_config).is_ok()
    }

    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandleItem>,
    ) -> Result<StrategyDataResult> {
        debug!("初始化 Vegas 策略数据: {}_{}", inst_id, period);

        // 1. 验证K线数据并获取时间戳
        let last_timestamp = validate_candles(&candles)?;

        // 2. 解析策略配置
        let vegas_strategy: VegasStrategy =
            serde_json::from_value(strategy_config.parameters.clone())
                .map_err(|e| anyhow!("解析 Vegas 策略配置失败: {}", e))?;

        // 3. 转换K线数据并计算指标
        let mut multi_strategy_indicators = vegas_strategy.get_indicator_combine();
        let mut candle_items = convert_candles_to_items(&candles);

        for item in &candle_items {
            get_multi_indicator_values(&mut multi_strategy_indicators, item);
        }

        // 4. 生成存储键并保存数据
        let hash_key = get_hash_key(inst_id, period, StrategyType::Vegas.as_str());

        set_strategy_indicator_values(
            inst_id.to_string(),
            period.to_string(),
            last_timestamp,
            hash_key.clone(),
            candle_items,
            multi_strategy_indicators,
        )
        .await;

        // 5. 验证数据保存成功
        let manager = get_indicator_manager();
        if !manager.key_exists(&hash_key).await {
            return Err(anyhow!("Vegas 策略数据保存验证失败: {}", hash_key));
        }

        info!("✅ Vegas 策略数据初始化完成: {}", hash_key);

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
        const MAX_HISTORY_SIZE: usize = 4000;

        // 1. 获取哈希键和管理n
        let key = get_hash_key(inst_id, period, StrategyType::Vegas.as_str());
        let manager = get_indicator_manager();

        // 2. 获取最新K线数据（使用公共函数）
        let new_candle_data = get_latest_candle(inst_id, period, snap).await?;
        let new_candle_item = parse_candle_to_data_item(&new_candle_data);

        // 3. 获取互斥锁和缓存快照
        let key_mutex = manager.acquire_key_mutex(&key).await;
        let _guard = key_mutex.lock().await;

        let (last_candles_vec, mut old_indicator_combines, old_time) = manager
            .get_snapshot_last_n(&key, MAX_HISTORY_SIZE)
            .await
            .ok_or_else(|| anyhow!("没有找到对应的 Vegas 策略值: {}", key))?;

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
                move_stop_open_price_when_touch_price: None,
                ts: new_candle_item.ts,
                single_value: None,
                single_result: None,
                signal_kline_stop_loss_price: None,
                stop_loss_source: None,
                is_ema_short_trend: None,
                is_ema_long_trend: None,
                atr_take_profit_level_1: None,
                atr_take_profit_level_2: None,
                atr_take_profit_level_3: None,
                filter_reasons: vec![],
                dynamic_adjustments: vec![],
                dynamic_config_snapshot: None,
                direction: rust_quant_domain::SignalDirection::None,
            });
        }

        // 5. 更新指标值
        let new_indicator_values =
            get_multi_indicator_values(&mut old_indicator_combines, &new_candle_item);

        // 6. 更新K线队列（使用公共函数）
        update_candle_queue(
            &mut new_candle_items,
            new_candle_item.clone(),
            MAX_HISTORY_SIZE,
        );

        // 7. 原子更新缓存
        manager
            .update_both(
                &key,
                new_candle_items.clone(),
                old_indicator_combines.clone(),
                new_candle_item.ts,
            )
            .await
            .map_err(|e| anyhow!("原子更新 Vegas 指标与K线失败: {}", e))?;

        // 8. 获取最近30根K线（使用公共函数）
        // 9. 生成交易信号
        let vegas_strategy: VegasStrategy =
            serde_json::from_value(strategy_config.parameters.clone())
                .map_err(|e| anyhow!("解析 Vegas 策略配置失败: {}", e))?;
        // ⚠️ 对齐回测：传入策略的窗口长度使用 min_k_line_num（而不是固定 30）
        let window_size = vegas_strategy.min_k_line_num.clamp(1, MAX_HISTORY_SIZE);
        let candle_vec = get_recent_candles(&new_candle_items, window_size);
        let default_weights = SignalWeightsConfig::default();
        let weights = vegas_strategy
            .signal_weights
            .as_ref()
            .unwrap_or(&default_weights);
        let domain_signal = vegas_strategy.get_trade_signal(
            &candle_vec,
            &mut new_indicator_values.clone(),
            weights,
            &serde_json::from_value(strategy_config.risk_config.clone())
                .map_err(|e| anyhow!("解析风险配置失败: {}", e))?,
        );

        info!("✅ Vegas策略信号生成完成: key={}", key);

        // 10. 转换 domain::SignalResult 到策略层 SignalResult（复用回测同一转换，避免字段丢失）
        let mut strategy_signal = convert_domain_signal(domain_signal);
        if strategy_signal.ts == 0 {
            strategy_signal.ts = new_candle_item.ts;
        }
        if strategy_signal.open_price == 0.0 {
            strategy_signal.open_price = new_candle_item.c;
        }

        // 11. 返回信号（下单逻辑由services层统一处理）
        Ok(strategy_signal)
    }
}
