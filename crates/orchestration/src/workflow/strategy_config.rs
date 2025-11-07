use std::env;

use anyhow::{anyhow, Result};
use tracing::warn;

use crate::trading::model::strategy::strategy_config::{StrategyConfigEntity, StrategyConfigEntityModel};
use rust_quant_orchestration::workflow::job_param_generator::ParamMergeBuilder;
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_indicators::vegas_indicator::VegasStrategy;
use rust_quant_strategies::nwe_strategy::NweStrategyConfig;

/// Vegas 策略回测配置
#[derive(Debug, Clone)]
pub struct BackTestConfig {
    /// 最大并发数
    pub max_concurrent: usize,
    /// K线数据限制
    pub candle_limit: usize,
    pub enable_random_test: bool,

    /// 是否启用随机策略测试
    pub enable_random_test_vegas: bool,
    /// 是否启用指定策略测试
    pub enable_specified_test_vegas: bool,
    /// 是否启用NWE随机回测
    pub enable_random_test_nwe: bool,
    /// 是否启用NWE指定配置回测
    pub enable_specified_test_nwe: bool,
}

impl Default for BackTestConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 30,
            candle_limit: 20000,
            enable_random_test: env::var("ENABLE_RANDOM_TEST").unwrap_or_default() == "true",
            enable_random_test_vegas: env::var("ENABLE_RANDOM_TEST_VEGAS").unwrap_or_default() == "true",
            enable_specified_test_vegas: env::var("ENABLE_SPECIFIED_TEST_VEGAS").unwrap_or_default() == "true",

            enable_random_test_nwe: env::var("ENABLE_RANDOM_TEST_NWE").unwrap_or_default() == "true",
            enable_specified_test_nwe: env::var("ENABLE_SPECIFIED_TEST_NWE").unwrap_or_default() == "true",
        }
    }
}

/// 获取指定的产品策略配置
pub async fn get_strate_config(
    inst_id: &str,
    time: &str,
    strategy_type: Option<&str>,
) -> Result<Vec<StrategyConfigEntity>> {
    //从策略配置中获取到对应的产品配置
    let strategy_config = StrategyConfigEntityModel::new()
        .await
        .get_config(strategy_type, inst_id, time)
        .await?;
    if strategy_config.len() < 1 {
        warn!("策略配置为空inst_id:{:?} time:{:?}", inst_id, time);
        return Ok(vec![]);
    }
    Ok(strategy_config)
}

/// 从数据库获取策略配置
pub async fn get_strategy_config_from_db(
    inst_id: &str,
    time: &str,
) -> Result<Vec<ParamMergeBuilder>> {
    // 从数据库获取策略配置
    let strategy_configs = get_strate_config(inst_id, time, Some("vegas"))
        .await
        .map_err(|e| anyhow!("获取策略配置失败: {}", e))?;

    if strategy_configs.is_empty() {
        warn!("未找到策略配置: inst_id={}, time={}", inst_id, time);
        return Ok(vec![]);
    }
    let mut params_batch = Vec::with_capacity(strategy_configs.len());

    tracing::info!("找到 {} 个策略配置", strategy_configs.len());
    for config in strategy_configs.iter() {
        match convert_strategy_config_to_param(config) {
            Ok(param) => params_batch.push(param),
            Err(e) => {
                tracing::error!("转换策略配置失败: {}, config_id: {:?}", e, config.id);
            }
        }
    }
    Ok(params_batch)
}

/// 测试指定策略配置
pub async fn test_specified_strategy_with_config(
    _inst_id: &str,
    _time: &str,
) -> Result<Vec<ParamMergeBuilder>> {
    //1Dutc
    let params_batch = vec![ParamMergeBuilder::build()
        .bb_multiplier(2.0)
        .bb_periods(10)
        .hammer_shadow_ratio(0.9)
        .breakthrough_threshold(0.003)
        .volume_bar_num(4)
        .volume_increase_ratio(2.0)
        .volume_decrease_ratio(2.5)
        .rsi_period(9)
        .rsi_overbought(90.0)
        .rsi_oversold(20.0)
        .max_loss_percent(0.03)
        .take_profit_ratio(1.5)
        .is_move_stop_loss(true)
        .is_used_signal_k_line_stop_loss(true)];
    //1H
    let params_batch = vec![ParamMergeBuilder::build()
        .bb_periods(13)
        .bb_multiplier(2.5)
        .hammer_shadow_ratio(0.6)
        .breakthrough_threshold(0.003)
        .volume_bar_num(6)
        .volume_increase_ratio(2.4)
        .volume_decrease_ratio(2.0)
        .rsi_period(9)
        .rsi_overbought(85.0)
        .rsi_oversold(15.0)
        .max_loss_percent(0.02)
        .take_profit_ratio(1.5)
        .is_move_stop_loss(false)
        .is_used_signal_k_line_stop_loss(true)];
    Ok(params_batch)
}

/// 转换策略配置为参数的辅助函数
fn convert_strategy_config_to_param(
    config: &StrategyConfigEntity,
) -> Result<ParamMergeBuilder> {
    let vegas_strategy = serde_json::from_str::<VegasStrategy>(&config.value)
        .map_err(|e| anyhow!("解析策略配置JSON失败: {}", e))?;

    let risk_config = serde_json::from_str::<BasicRiskStrategyConfig>(&config.risk_config)?;

    // 安全地提取配置值，避免unwrap
    let kline_hammer = vegas_strategy
        .kline_hammer_signal
        .ok_or_else(|| anyhow!("缺少kline_hammer_signal配置"))?;

    let ema_signal = vegas_strategy
        .ema_signal
        .ok_or_else(|| anyhow!("缺少ema_signal配置"))?;

    let bolling_signal = vegas_strategy
        .bolling_signal
        .as_ref()
        .ok_or_else(|| anyhow!("缺少bolling_signal配置"))?;

    let volume_signal = vegas_strategy
        .volume_signal
        .ok_or_else(|| anyhow!("缺少volume_signal配置"))?;

    let rsi_signal = vegas_strategy
        .rsi_signal
        .ok_or_else(|| anyhow!("缺少rsi_signal配置"))?;

    let param = ParamMergeBuilder::build()
        .hammer_shadow_ratio(kline_hammer.up_shadow_ratio)
        .breakthrough_threshold(ema_signal.ema_breakthrough_threshold)
        .bb_periods(bolling_signal.period as i32)
        .bb_multiplier(bolling_signal.multiplier)
        .volume_bar_num(volume_signal.volume_bar_num)
        .volume_increase_ratio(volume_signal.volume_increase_ratio)
        .volume_decrease_ratio(volume_signal.volume_decrease_ratio)
        .rsi_period(rsi_signal.rsi_length)
        .rsi_overbought(rsi_signal.rsi_overbought)
        .rsi_oversold(rsi_signal.rsi_oversold)
        .kline_start_time(config.kline_start_time)
        .kline_end_time(config.kline_end_time)
        //risk
        .max_loss_percent(risk_config.max_loss_percent)
        .take_profit_ratio(risk_config.take_profit_ratio)
        .is_move_stop_loss(risk_config.is_one_k_line_diff_stop_loss)
        .is_used_signal_k_line_stop_loss(risk_config.is_used_signal_k_line_stop_loss);

    Ok(param)
}

/// 将数据库中的策略配置转换为 NWE 策略配置与风险配置
pub fn convert_strategy_config_to_nwe(
    config: &StrategyConfigEntity,
) -> Result<(NweStrategyConfig, BasicRiskStrategyConfig)> {
    let nwe_cfg = serde_json::from_str::<NweStrategyConfig>(&config.value)
        .map_err(|e| anyhow!("解析NWE策略配置JSON失败: {}", e))?;
    let risk_cfg = serde_json::from_str::<BasicRiskStrategyConfig>(&config.risk_config)
        .map_err(|e| anyhow!("解析风险配置JSON失败: {}", e))?;
    Ok((nwe_cfg, risk_cfg))
}

/// 从数据库获取 NWE 指定策略配置
pub async fn get_nwe_strategy_config_from_db(
    inst_id: &str,
    time: &str,
) -> Result<Vec<(NweStrategyConfig, BasicRiskStrategyConfig)>> {
    let strategy_configs = get_strate_config(inst_id, time, Some("nwe"))
        .await
        .map_err(|e| anyhow!("获取策略配置失败: {}", e))?;

    if strategy_configs.is_empty() {
        warn!("未找到NWE策略配置: inst_id={}, time={}", inst_id, time);
        return Ok(vec![]);
    }

    let mut result = Vec::with_capacity(strategy_configs.len());
    tracing::info!("找到 {} 个NWE策略配置", strategy_configs.len());
    for cfg in strategy_configs.iter() {
        match convert_strategy_config_to_nwe(cfg) {
            Ok(pair) => result.push(pair),
            Err(e) => tracing::error!("转换NWE策略配置失败: {}, config_id: {:?}", e, cfg.id),
        }
    }
    Ok(result)
}
