use crate::workflow::job_param_generator::ParamMergeBuilder;
use anyhow::{anyhow, Result};
use rust_quant_domain::StrategyConfig;
use rust_quant_indicators::trend::vegas::VegasStrategy;
use rust_quant_services::strategy::StrategyConfigService;
use rust_quant_strategies::implementations::nwe_strategy::NweStrategyConfig;
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use std::env;
use tracing::warn;
/// Vegas 策略回测配置
#[derive(Debug, Clone)]
pub struct BackTestConfig {
    /// 策略配置id；为空时表示该条件不启用。
    pub strategy_config_id: Option<String>,
    /// 指定配置回测使用的独立策略 Key。
    pub strategy_key: Option<String>,
    /// 最大并发数
    pub max_concurrent: usize,
    /// K线数据限制
    pub candle_limit: usize,
    /// enablerandomtest，用于配置运行参数。
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
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            strategy_config_id: None,
            strategy_key: None,
            max_concurrent: 30,
            candle_limit: 40000,
            enable_random_test: env::var("ENABLE_RANDOM_TEST").unwrap_or_default() == "true",
            enable_random_test_vegas: env::var("ENABLE_RANDOM_TEST_VEGAS").unwrap_or_default()
                == "true",
            enable_specified_test_vegas: env::var("ENABLE_SPECIFIED_TEST_VEGAS")
                .unwrap_or_default()
                == "true",
            enable_random_test_nwe: env::var("ENABLE_RANDOM_TEST_NWE").unwrap_or_default()
                == "true",
            enable_specified_test_nwe: env::var("ENABLE_SPECIFIED_TEST_NWE").unwrap_or_default()
                == "true",
        }
    }
}
pub async fn get_strate_config(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
    strategy_type: Option<&str>,
) -> Result<Vec<StrategyConfig>> {
    get_strate_config_with_selector(config_service, inst_id, time, strategy_type, None).await
}
/// 获取指定产品策略配置，支持按 Admin 行ID精确选择单条配置。
pub async fn get_strate_config_with_selector(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
    strategy_type: Option<&str>,
    strategy_config_id: Option<&str>,
) -> Result<Vec<StrategyConfig>> {
    if let Some(strategy_config_id) = strategy_config_id.and_then(non_empty_trimmed) {
        let config = config_service
            .load_config_by_external_id(strategy_config_id)
            .await?;
        validate_selected_strategy_config(&config, inst_id, time, strategy_type)?;
        return Ok(vec![config]);
    }
    let strategy_configs = config_service
        .load_configs(inst_id, time, strategy_type)
        .await?;
    if strategy_configs.is_empty() {
        warn!("策略配置为空inst_id:{:?} time:{:?}", inst_id, time);
        return Ok(vec![]);
    }
    Ok(strategy_configs)
}
/// 封装非空trimmed，减少回测策略调用方重复实现相同细节。
fn non_empty_trimmed(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
/// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
fn validate_selected_strategy_config(
    config: &StrategyConfig,
    inst_id: &str,
    time: &str,
    strategy_type: Option<&str>,
) -> Result<()> {
    if config.symbol != inst_id {
        return Err(anyhow!(
            "策略配置 {} 的交易对 {} 与回测目标 {} 不匹配",
            config.id,
            config.symbol,
            inst_id
        ));
    }
    let timeframe = time
        .parse()
        .map_err(|_| anyhow!("无效的时间周期: {}", time))?;
    if config.timeframe != timeframe {
        return Err(anyhow!(
            "策略配置 {} 的周期 {} 与回测目标 {} 不匹配",
            config.id,
            config.timeframe.as_str(),
            time
        ));
    }
    if let Some(strategy_type) = strategy_type {
        let expected = strategy_type
            .parse()
            .map_err(|_| anyhow!("无效的策略类型: {}", strategy_type))?;
        if config.strategy_type != expected {
            return Err(anyhow!(
                "策略配置 {} 的策略类型 {} 与回测目标 {} 不匹配",
                config.id,
                config.strategy_type.as_str(),
                strategy_type
            ));
        }
    }
    Ok(())
}
pub async fn get_strategy_config_from_db(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
) -> Result<Vec<ParamMergeBuilder>> {
    get_strategy_config_from_db_with_selector(config_service, inst_id, time, None).await
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn get_strategy_config_from_db_with_selector(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
    strategy_config_id: Option<&str>,
) -> Result<Vec<ParamMergeBuilder>> {
    get_strategy_config_from_db_with_strategy_selector(
        config_service,
        inst_id,
        time,
        "vegas",
        strategy_config_id,
    )
    .await
}

/// 加载共享 Vegas 引擎下指定独立策略身份的配置。
pub async fn get_strategy_config_from_db_with_strategy_selector(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
    strategy_key: &str,
    strategy_config_id: Option<&str>,
) -> Result<Vec<ParamMergeBuilder>> {
    let strategy_configs = get_strate_config_with_selector(
        config_service,
        inst_id,
        time,
        Some(strategy_key),
        strategy_config_id,
    )
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
#[cfg(test)]
mod selected_config_tests {
    use super::*;
    use anyhow::Result;
    use async_trait::async_trait;
    use rust_quant_domain::traits::StrategyConfigRepository;
    use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};
    use serde_json::json;
    struct FakeStrategyConfigRepository;
    #[async_trait]
    impl StrategyConfigRepository for FakeStrategyConfigRepository {
        /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
        async fn find_by_id(&self, id: i64) -> Result<Option<StrategyConfig>> {
            Ok(Some(test_strategy_config(
                id,
                "ETH-USDT-SWAP",
                Timeframe::H4,
            )))
        }
        /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
        async fn find_by_external_id(&self, id: &str) -> Result<Option<StrategyConfig>> {
            if id == "admin-row-43" {
                Ok(Some(test_strategy_config(
                    43,
                    "ETH-USDT-SWAP",
                    Timeframe::H4,
                )))
            } else {
                Ok(None)
            }
        }
        async fn find_all_enabled(&self) -> Result<Vec<StrategyConfig>> {
            Ok(vec![])
        }
        /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
        async fn find_by_symbol_and_timeframe(
            &self,
            _symbol: &str,
            _timeframe: Timeframe,
        ) -> Result<Vec<StrategyConfig>> {
            Ok(vec![
                test_strategy_config(42, "ETH-USDT-SWAP", Timeframe::H4),
                test_strategy_config(43, "ETH-USDT-SWAP", Timeframe::H4),
            ])
        }
        async fn save(&self, config: &StrategyConfig) -> Result<i64> {
            Ok(config.id)
        }
        async fn update(&self, _config: &StrategyConfig) -> Result<()> {
            Ok(())
        }
        async fn delete(&self, _id: i64) -> Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn selected_strategy_config_id_loads_one_exact_config() {
        let service = StrategyConfigService::new(Box::new(FakeStrategyConfigRepository));
        let configs = get_strate_config_with_selector(
            &service,
            "ETH-USDT-SWAP",
            "4H",
            Some("vegas"),
            Some("admin-row-43"),
        )
        .await
        .expect("selected config");
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].id, 43);
    }
    #[test]
    fn vegas_config_conversion_preserves_candle_momentum_activation() {
        let expected = rust_quant_indicators::trend::vegas::CandleMomentumActivationConfig {
            is_open: true,
            allow_delayed_fib_volume_confirmation: false,
            restrict_delayed_fib_to_choppy_band: false,
            allow_high_volatility_delayed_short: false,
            baseline_bars: 20,
            valid_for_bars: 6,
            min_wait_bars: 3,
            min_volume_ratio: 2.0,
            min_range_ratio: 1.5,
            allow_trigger_bar_entry: false,
            direction_mode:
                rust_quant_indicators::trend::vegas::CandleMomentumDirectionMode::Opposite,
            min_entry_rsi: Some(25.0),
            max_entry_rsi: Some(55.0),
        };
        let mut vegas = VegasStrategy::new("4H".to_string());
        vegas.candle_momentum_activation = expected;
        let config = StrategyConfig::new(
            44,
            StrategyType::Vegas,
            "ETH-USDT-SWAP".to_string(),
            Timeframe::H4,
            serde_json::to_value(vegas).expect("vegas config"),
            serde_json::to_value(BasicRiskStrategyConfig::default()).expect("risk config"),
        );

        let converted = convert_strategy_config_to_param(&config)
            .expect("convert config")
            .to_vegas_strategy("4H".to_string());

        assert_eq!(
            converted.candle_momentum_activation.is_open,
            expected.is_open
        );
        assert_eq!(
            converted.candle_momentum_activation.valid_for_bars,
            expected.valid_for_bars
        );
        assert_eq!(
            converted.candle_momentum_activation.min_wait_bars,
            expected.min_wait_bars
        );
        assert_eq!(
            converted.candle_momentum_activation.min_volume_ratio,
            expected.min_volume_ratio
        );
        assert_eq!(
            converted.candle_momentum_activation.min_range_ratio,
            expected.min_range_ratio
        );
        assert_eq!(
            converted.candle_momentum_activation.direction_mode,
            expected.direction_mode
        );
        assert_eq!(
            converted.candle_momentum_activation.min_entry_rsi,
            expected.min_entry_rsi
        );
        assert_eq!(
            converted.candle_momentum_activation.max_entry_rsi,
            expected.max_entry_rsi
        );
    }

    #[test]
    fn vegas_config_conversion_preserves_cross_asset_adaptive_threshold() {
        let expected = rust_quant_indicators::trend::vegas::CrossAssetAdaptiveThresholdConfig {
            is_open: true,
            atr_period: 21,
            volume_lookback_bars: 180,
            min_volume_percentile: 0.8,
            min_swing_atr_multiple: 5.0,
            choppy_volatility_filter:
                rust_quant_indicators::trend::vegas::ChoppyVolatilityFilterConfig {
                    is_open: true,
                    min_atr_ratio: 0.02,
                    max_atr_ratio: 0.03,
                    min_volume_percentile: 0.99,
                },
        };
        let mut vegas = VegasStrategy::new("4H".to_string());
        vegas.cross_asset_adaptive_threshold = expected;
        let config = StrategyConfig::new(
            45,
            StrategyType::Vegas,
            "ETH-USDT-SWAP".to_string(),
            Timeframe::H4,
            serde_json::to_value(vegas).expect("vegas config"),
            serde_json::to_value(BasicRiskStrategyConfig::default()).expect("risk config"),
        );

        let converted = convert_strategy_config_to_param(&config)
            .expect("convert config")
            .to_vegas_strategy("4H".to_string())
            .cross_asset_adaptive_threshold;

        assert!(converted.is_open);
        assert_eq!(converted.atr_period, expected.atr_period);
        assert_eq!(
            converted.volume_lookback_bars,
            expected.volume_lookback_bars
        );
        assert_eq!(
            converted.min_volume_percentile,
            expected.min_volume_percentile
        );
        assert_eq!(
            converted.min_swing_atr_multiple,
            expected.min_swing_atr_multiple
        );
        assert_eq!(
            converted.choppy_volatility_filter.min_volume_percentile,
            expected.choppy_volatility_filter.min_volume_percentile
        );
    }

    #[test]
    fn vegas_config_conversion_preserves_min_k_line_num() {
        let mut vegas = VegasStrategy::new("4H".to_string());
        vegas.min_k_line_num = 720;
        let ema = vegas.ema_signal.as_mut().expect("EMA config");
        ema.ema6_length = 576;
        ema.ema7_length = 676;
        let config = StrategyConfig::new(
            47,
            StrategyType::Vegas,
            "ALLO-USDT-SWAP".to_string(),
            Timeframe::H4,
            serde_json::to_value(vegas).expect("vegas config"),
            serde_json::to_value(BasicRiskStrategyConfig::default()).expect("risk config"),
        );

        let converted = convert_strategy_config_to_param(&config)
            .expect("convert config")
            .to_vegas_strategy("4H".to_string());

        assert_eq!(converted.min_k_line_num, 720);
        let converted_ema = converted.ema_signal.expect("converted EMA config");
        assert_eq!(converted_ema.ema6_length, 576);
        assert_eq!(converted_ema.ema7_length, 676);
    }

    #[test]
    fn vegas_config_conversion_preserves_disabled_optional_indicators() {
        let mut vegas = VegasStrategy::new("4H".to_string());
        vegas
            .engulfing_signal
            .as_mut()
            .expect("engulfing config")
            .is_open = false;
        vegas
            .ema_touch_trend_signal
            .as_mut()
            .expect("EMA touch config")
            .is_open = false;
        let config = StrategyConfig::new(
            48,
            StrategyType::Vegas,
            "ETH-USDT-SWAP".to_string(),
            Timeframe::H4,
            serde_json::to_value(vegas).expect("vegas config"),
            serde_json::to_value(BasicRiskStrategyConfig::default()).expect("risk config"),
        );

        let converted = convert_strategy_config_to_param(&config)
            .expect("convert config")
            .to_vegas_strategy("4H".to_string());

        assert!(
            !converted
                .engulfing_signal
                .expect("converted engulfing config")
                .is_open
        );
        assert!(
            !converted
                .ema_touch_trend_signal
                .expect("converted EMA touch config")
                .is_open
        );
    }

    #[test]
    fn vegas_config_conversion_preserves_optional_risk_controls() {
        let vegas = VegasStrategy::new("4H".to_string());
        let risk = BasicRiskStrategyConfig {
            fixed_signal_kline_take_profit_ratio: Some(0.25),
            dynamic_max_loss: Some(false),
            ..BasicRiskStrategyConfig::default()
        };
        let config = StrategyConfig::new(
            46,
            StrategyType::Vegas,
            "ETH-USDT-SWAP".to_string(),
            Timeframe::H4,
            serde_json::to_value(vegas).expect("vegas config"),
            serde_json::to_value(risk).expect("risk config"),
        );

        let converted = convert_strategy_config_to_param(&config)
            .expect("convert config")
            .to_risk_config();

        assert_eq!(converted.fixed_signal_kline_take_profit_ratio, Some(0.25));
        assert_eq!(converted.dynamic_max_loss, Some(false));
    }
    /// 提供test策略配置的集中实现，避免回测策略调用方重复处理相同细节。
    fn test_strategy_config(id: i64, symbol: &str, timeframe: Timeframe) -> StrategyConfig {
        StrategyConfig::new(
            id,
            StrategyType::Vegas,
            symbol.to_string(),
            timeframe,
            json!({
                "kline_hammer_signal": {"up_shadow_ratio": 0.6},
                "ema_signal": {"ema_breakthrough_threshold": 0.003},
                "bolling_signal": {"period": 13, "multiplier": 2.5},
                "volume_signal": {
                    "volume_bar_num": 6,
                    "volume_increase_ratio": 2.4,
                    "volume_decrease_ratio": 2.0
                },
                "rsi_signal": {
                    "rsi_length": 9,
                    "rsi_overbought": 85.0,
                    "rsi_oversold": 15.0
                },
                "entry_block_config": {},
                "ema_distance_config": {},
                "atr_stop_loss_multiplier": 2.0,
                "emit_debug": false
            }),
            json!({"max_loss_percent": 0.02}),
        )
    }
}
/// 测试指定策略配置
pub async fn test_specified_strategy_with_config(
    _inst_id: &str,
    _time: &str,
) -> Result<Vec<ParamMergeBuilder>> {
    //1Dutc
    #[allow(unused)]
    let params_batch = [ParamMergeBuilder::build()
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
        .is_used_signal_k_line_stop_loss(true)];
    Ok(params_batch)
}
/// 转换策略配置为参数的辅助函数
/// # 架构说明
/// - 接受 domain 层的 StrategyConfig，而不是 infrastructure 层的 StrategyConfigEntity
fn convert_strategy_config_to_param(config: &StrategyConfig) -> Result<ParamMergeBuilder> {
    // parameters 是 JsonValue，需要转换为字符串再解析
    let value_str = serde_json::to_string(&config.parameters)
        .map_err(|e| anyhow!("序列化策略配置JSON失败: {}", e))?;
    let vegas_strategy = serde_json::from_str::<VegasStrategy>(&value_str)
        .map_err(|e| anyhow!("解析策略配置JSON失败: {}", e))?;
    let signal_weights = vegas_strategy.signal_weights.clone();
    let engulfing_signal = vegas_strategy.engulfing_signal;
    let ema_touch_trend_signal = vegas_strategy.ema_touch_trend_signal;
    let leg_detection_signal = vegas_strategy.leg_detection_signal;
    let market_structure_signal = vegas_strategy.market_structure_signal;
    let range_filter_signal = vegas_strategy.range_filter_signal;
    let chase_confirm_config = vegas_strategy.chase_confirm_config;
    let extreme_k_filter_signal = vegas_strategy.extreme_k_filter_signal;
    let fib_retracement_signal = vegas_strategy.fib_retracement_signal;
    let entry_block_config = vegas_strategy.entry_block_config;
    let candle_momentum_activation = vegas_strategy.candle_momentum_activation;
    let cross_asset_adaptive_threshold = vegas_strategy.cross_asset_adaptive_threshold;
    let ema_distance_config = vegas_strategy.ema_distance_config;
    let atr_stop_loss_multiplier = vegas_strategy.atr_stop_loss_multiplier;
    let emit_debug = vegas_strategy.emit_debug;
    // println!("config.risk_config: {:#?}", config.risk_config);
    let risk_config = serde_json::from_value::<BasicRiskStrategyConfig>(config.risk_config.clone())
        .map_err(|e| anyhow!("解析风险配置JSON失败: {}", e))?;
    // println!("risk_config: {:#?}", risk_config);
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
    let mut param = ParamMergeBuilder::build()
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
        .kline_start_time(config.backtest_start.unwrap_or(0))
        .kline_end_time(config.backtest_end.unwrap_or(0))
        //risk
        .max_loss_percent(risk_config.max_loss_percent)
        .take_profit_ratio(risk_config.atr_take_profit_ratio.unwrap_or(0.0))
        .is_used_signal_k_line_stop_loss(
            risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false),
        );
    param.signal_weights = signal_weights;
    param.engulfing_signal = engulfing_signal;
    param.ema_touch_trend_signal = ema_touch_trend_signal;
    param.ema_signal = Some(ema_signal);
    param.min_k_line_num = Some(vegas_strategy.min_k_line_num);
    param.leg_detection_signal = leg_detection_signal;
    param.market_structure_signal = market_structure_signal;
    param.range_filter_signal = range_filter_signal;
    param.chase_confirm_config = chase_confirm_config;
    param.extreme_k_filter_signal = extreme_k_filter_signal;
    param.fib_retracement_signal = fib_retracement_signal;
    param.entry_block_config = Some(entry_block_config);
    param.candle_momentum_activation = Some(candle_momentum_activation);
    param.cross_asset_adaptive_threshold = Some(cross_asset_adaptive_threshold);
    param.ema_distance_config = Some(ema_distance_config);
    param.atr_stop_loss_multiplier = Some(atr_stop_loss_multiplier);
    param.emit_debug = Some(emit_debug);
    param.macd_signal = vegas_strategy.macd_signal;
    param.fix_signal_kline_take_profit_ratio = risk_config.fixed_signal_kline_take_profit_ratio;
    param.dynamic_max_loss = risk_config.dynamic_max_loss;
    param.dynamic_entry_amp_threshold = risk_config.dynamic_entry_amp_threshold;
    param.dynamic_entry_loss_percent = risk_config.dynamic_entry_loss_percent;
    param.dynamic_entry_require_direction_mismatch =
        risk_config.dynamic_entry_require_direction_mismatch;
    param.dynamic_range_threshold = risk_config.dynamic_range_threshold;
    param.dynamic_range_loss_percent = risk_config.dynamic_range_loss_percent;
    param.position_leverage = risk_config.position_leverage;
    Ok(param)
}
/// 将数据库中的策略配置转换为 NWE 策略配置与风险配置
/// # 架构说明
/// - 接受 domain 层的 StrategyConfig，而不是 infrastructure 层的 StrategyConfigEntity
pub fn convert_strategy_config_to_nwe(
    config: &StrategyConfig,
) -> Result<(NweStrategyConfig, BasicRiskStrategyConfig)> {
    // parameters 是 JsonValue，需要转换为字符串再解析
    let value_str = serde_json::to_string(&config.parameters)
        .map_err(|e| anyhow!("序列化策略配置JSON失败: {}", e))?;
    let nwe_cfg = serde_json::from_str::<NweStrategyConfig>(&value_str).map_err(|e| {
        // 输出详细错误信息便于调试
        tracing::error!(
            "解析NWE策略配置JSON失败: config_id={:?}, error={}, json_preview={}",
            config.id,
            e,
            &value_str[..value_str.len().min(300)]
        );
        anyhow!("{}", e)
    })?;
    let risk_cfg = serde_json::from_value::<BasicRiskStrategyConfig>(config.risk_config.clone())
        .map_err(|e| anyhow!("解析风险配置JSON失败: {}", e))?;
    Ok((nwe_cfg, risk_cfg))
}
pub async fn get_nwe_strategy_config_from_db(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
) -> Result<Vec<(NweStrategyConfig, BasicRiskStrategyConfig)>> {
    get_nwe_strategy_config_from_db_with_selector(config_service, inst_id, time, None).await
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn get_nwe_strategy_config_from_db_with_selector(
    config_service: &StrategyConfigService,
    inst_id: &str,
    time: &str,
    strategy_config_id: Option<&str>,
) -> Result<Vec<(NweStrategyConfig, BasicRiskStrategyConfig)>> {
    let strategy_configs = get_strate_config_with_selector(
        config_service,
        inst_id,
        time,
        Some("nwe"),
        strategy_config_id,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::{StrategyType, Timeframe};
    use rust_quant_indicators::trend::vegas::{
        EmaSignalConfig, KlineHammerConfig, RsiSignalConfig, VolumeSignalConfig,
    };
    use rust_quant_indicators::volatility::BollingBandsSignalConfig;
    use serde_json::json;

    #[test]
    fn vegas_config_conversion_preserves_position_leverage() {
        let vegas_strategy = VegasStrategy {
            period: "4H".to_string(),
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
            rsi_signal: Some(RsiSignalConfig::default()),
            bolling_signal: Some(BollingBandsSignalConfig::default()),
            kline_hammer_signal: Some(KlineHammerConfig::default()),
            ..VegasStrategy::default()
        };
        let config = StrategyConfig::new(
            1,
            StrategyType::Vegas,
            "ETH-USDT-SWAP".to_string(),
            Timeframe::H4,
            serde_json::to_value(vegas_strategy).expect("vegas strategy json"),
            json!({
                "max_loss_percent": 0.04,
                "atr_take_profit_ratio": 3.0,
                "is_used_signal_k_line_stop_loss": true,
                "position_leverage": 0.6
            }),
        );

        let param = convert_strategy_config_to_param(&config).expect("convert vegas config");
        let risk = param.to_risk_config();

        assert_eq!(risk.position_leverage, Some(0.6));
    }
}
