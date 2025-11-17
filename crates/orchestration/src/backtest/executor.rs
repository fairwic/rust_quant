//! 回测执行器
//!
//! 负责回测策略的执行，协调 BacktestService 和 CandleService

use anyhow::{anyhow, Result};
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{error, info, warn};

use rust_quant_common::CandleItem;
use rust_quant_domain::StrategyType;
use rust_quant_indicators::volatility::BollingBandsSignalConfig;
use rust_quant_indicators::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::{
    EmaSignalConfig, EmaTouchTrendSignalConfig, EngulfingSignalConfig, KlineHammerConfig,
    RsiSignalConfig, VegasStrategy, VolumeSignalConfig,
};
use rust_quant_market::models::SelectTime;
use rust_quant_services::market::CandleService;
use rust_quant_services::strategy::BacktestService;
use rust_quant_strategies::implementations::nwe_strategy::{NweStrategy, NweStrategyConfig};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;

use crate::infra::data_validator;
use crate::workflow::job_param_generator::ParamMergeBuilder;

/// 回测执行器
///
/// 职责：
/// 1. 执行回测策略（Vegas、NWE）
/// 2. 加载和转换K线数据
/// 3. 协调回测流程
///
/// 依赖：
/// - BacktestService: 保存回测结果
/// - CandleService: 获取K线数据
pub struct BacktestExecutor {
    backtest_service: Arc<BacktestService>,
    candle_service: Arc<CandleService>,
}

impl BacktestExecutor {
    /// 创建回测执行器实例
    ///
    /// # 参数
    /// * `backtest_service` - 回测服务
    /// * `candle_service` - K线服务
    pub fn new(
        backtest_service: Arc<BacktestService>,
        candle_service: Arc<CandleService>,
    ) -> Self {
        Self {
            backtest_service,
            candle_service,
        }
    }

    /// 运行 Vegas 策略测试
    pub async fn run_vegas_test(
        &self,
        inst_id: &str,
        time: &str,
        mut strategy: VegasStrategy,
        risk_strategy_config: BasicRiskStrategyConfig,
        mysql_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        let start_time = Instant::now();

        // 获取信号权重配置
        let signal_weights = strategy
            .signal_weights
            .as_ref()
            .cloned()
            .unwrap_or_default();

        // 类型转换：strategies::BasicRiskStrategyConfig -> domain::BasicRiskConfig
        let domain_risk_config = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: risk_strategy_config.max_loss_percent,
            take_profit_ratio: risk_strategy_config.take_profit_ratio,
            is_used_signal_k_line_stop_loss: risk_strategy_config.is_used_signal_k_line_stop_loss,
            is_move_stop_loss: risk_strategy_config.is_one_k_line_diff_stop_loss,
            max_hold_time: None,
            max_leverage: None,
        };

        // 获取最小数据长度和指标组合
        let min_len = strategy.get_min_data_length();
        let mut indicator_combine = strategy.get_indicator_combine();

        // 使用通用回测引擎
        let res = rust_quant_strategies::strategy_common::run_back_test_generic(
            |candles, values: &mut rust_quant_indicators::trend::vegas::VegasIndicatorSignalValue| {
                // VegasStrategy::get_trade_signal 返回 domain::SignalResult
                // 需要转换为 strategies::SignalResult
                let domain_signal = strategy.get_trade_signal(
                    candles,
                    values,
                    &signal_weights,
                    &domain_risk_config,
                );

                // 转换信号类型：domain::SignalResult -> strategies::SignalResult
                convert_domain_signal_to_strategies_signal(domain_signal)
            },
            &mysql_candles,
            risk_strategy_config.clone(),
            min_len,
            &mut indicator_combine,
            |ic, data_item| {
                // 计算 Vegas 指标值
                rust_quant_strategies::strategy_common::get_multi_indicator_values(ic, data_item)
            },
        );

        // 配置序列化
        let config_desc = serde_json::to_string(&strategy).ok();

        // 保存测试日志并获取 back_test_id
        let back_test_id = self
            .backtest_service
            .save_backtest_log(
                inst_id,
                time,
                config_desc,
                res,
                &mysql_candles,
                risk_strategy_config,
                &StrategyType::Vegas.as_str(),
            )
            .await?;

        let elapsed = start_time.elapsed();
        info!(
            "[Vegas 回测] 完成 inst_id={}, period={}, back_test_id={}, 耗时={}ms",
            inst_id,
            time,
            back_test_id,
            elapsed.as_millis()
        );

        Ok(back_test_id)
    }

    /// 运行 NWE 策略测试
    pub async fn run_nwe_test(
        &self,
        inst_id: &str,
        time: &str,
        mut strategy: NweStrategy,
        risk_strategy_config: BasicRiskStrategyConfig,
        mysql_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        let start_time = Instant::now();

        // 策略测试阶段
        let res = strategy.run_test(&mysql_candles, risk_strategy_config.clone());

        // 配置描述构建阶段
        let config_desc = serde_json::to_string(&strategy.config).ok();

        // 保存测试日志并获取 back_test_id
        let back_test_id = self
            .backtest_service
            .save_backtest_log(
                inst_id,
                time,
                config_desc,
                res,
                &mysql_candles,
                risk_strategy_config,
                &StrategyType::Nwe.as_str(),
            )
            .await?;

        let elapsed = start_time.elapsed();
        info!(
            "[NWE 回测] 完成 inst_id={}, period={}, back_test_id={}, 耗时={}ms",
            inst_id,
            time,
            back_test_id,
            elapsed.as_millis()
        );

        Ok(back_test_id)
    }

    /// 获取K线数据并确认
    async fn get_candle_data_confirm(
        &self,
        inst_id: &str,
        period: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> Result<Vec<rust_quant_market::models::CandlesEntity>> {
        let start_time = Instant::now();

        let db_query_start = Instant::now();
        let mysql_candles = self
            .candle_service
            .get_confirmed_candles_for_backtest(inst_id, period, limit, select_time)
            .await?;
        let db_query_duration = db_query_start.elapsed();

        let validation_start = Instant::now();
        data_validator::valid_candles_continuity(&mysql_candles, period)?;
        let validation_duration = validation_start.elapsed();

        let total_duration = start_time.elapsed();
        info!(
            "[性能跟踪] get_candle_data_confirm 完成 - 总耗时: {}ms, 数据库查询: {}ms, 数据验证: {}ms, 数据条数: {}",
            total_duration.as_millis(),
            db_query_duration.as_millis(),
            validation_duration.as_millis(),
            mysql_candles.len()
        );

        Ok(mysql_candles)
    }

    /// 加载并转换K线数据
    pub async fn load_and_convert_candle_data(
        &self,
        inst_id: &str,
        time: &str,
        limit: usize,
    ) -> Result<Arc<Vec<CandleItem>>> {
        let start_time = Instant::now();
        info!(
            "[性能跟踪] 开始加载K线数据: inst_id={}, time={}, limit={}",
            inst_id, time, limit
        );

        let data_fetch_start = Instant::now();
        let mysql_candles = self
            .get_candle_data_confirm(inst_id, time, limit, None)
            .await
            .map_err(|e| anyhow!("获取K线数据失败: {}", e))?;
        let data_fetch_duration = data_fetch_start.elapsed();

        if mysql_candles.is_empty() {
            return Err(anyhow!("K线数据为空"));
        }

        let data_convert_start = Instant::now();
        let candle_item_vec = self
            .candle_service
            .convert_candles_to_items(&mysql_candles);
        let data_convert_duration = data_convert_start.elapsed();

        let total_duration = start_time.elapsed();
        info!(
            "[性能跟踪] K线数据加载完成 - 总耗时: {}ms, 数据获取: {}ms, 数据转换: {}ms, 数据条数: {}",
            total_duration.as_millis(),
            data_fetch_duration.as_millis(),
            data_convert_duration.as_millis(),
            candle_item_vec.len()
        );
        Ok(Arc::new(candle_item_vec))
    }

    /// 运行回测策略
    pub async fn run_back_test_strategy(
        &self,
        params_batch: Vec<ParamMergeBuilder>,
        inst_id: &str,
        time: &str,
        arc_candle_item_clone: Arc<Vec<CandleItem>>,
        semaphore: Arc<Semaphore>,
    ) {
        let mut batch_tasks = Vec::with_capacity(params_batch.len());
        for param in params_batch {
        let bb_period = param.bb_period;
        let shadow_ratio = param.hammer_shadow_ratio;
        let bb_multiplier = param.bb_multiplier;
        let volume_bar_num = param.volume_bar_num;
        let volume_increase_ratio = param.volume_increase_ratio;
        let volume_decrease_ratio = param.volume_decrease_ratio;
        let rsi_period = param.rsi_period;
        let rsi_overbought = param.rsi_overbought;
        let rsi_oversold = param.rsi_oversold;

        let risk_strategy_config = BasicRiskStrategyConfig {
            max_loss_percent: param.max_loss_percent,
            take_profit_ratio: param.take_profit_ratio,
            is_one_k_line_diff_stop_loss: param.is_move_stop_loss,
            is_used_signal_k_line_stop_loss: param.is_used_signal_k_line_stop_loss,
            is_move_stop_open_price_when_touch_price: param.is_move_stop_open_price_when_touch_price,
        };

        let volume_signal = VolumeSignalConfig {
            volume_bar_num,
            volume_increase_ratio,
            volume_decrease_ratio,
            is_open: true,
        };

        let rsi_signal = RsiSignalConfig {
            rsi_length: rsi_period,
            rsi_oversold,
            rsi_overbought,
            is_open: true,
        };

        let ema_touch_trend_signal = EmaTouchTrendSignalConfig {
            is_open: true,
            ..Default::default()
        };

        let kline_hammer_signal = KlineHammerConfig {
            up_shadow_ratio: shadow_ratio,
            down_shadow_ratio: shadow_ratio,
        };

        let strategy = VegasStrategy {
            period: time.to_string(),
            min_k_line_num: 3600,
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            ema_signal: Some(EmaSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            volume_signal: Some(volume_signal),
            ema_touch_trend_signal: Some(ema_touch_trend_signal),
            rsi_signal: Some(rsi_signal),
            bolling_signal: Some(BollingBandsSignalConfig {
                period: bb_period as usize,
                multiplier: bb_multiplier,
                is_open: true,
                consecutive_touch_times: 4,
            }),
            kline_hammer_signal: Some(kline_hammer_signal),
        };

        let inst_id = inst_id.to_string();
        let time = time.to_string();
        let mysql_candles = Arc::clone(&arc_candle_item_clone);
        let permit = Arc::clone(&semaphore);

        // 创建任务
        let executor = self.clone_for_spawn();
        batch_tasks.push(tokio::spawn(async move {
            let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
            match executor.run_vegas_test(
                &inst_id,
                &time,
                strategy,
                risk_strategy_config,
                mysql_candles,
            )
            .await
            {
                Ok(back_test_id) => Some(back_test_id),
                Err(e) => {
                    error!("Vegas test failed: {:?}", e);
                    None
                }
            }
        }));
    }

        // 等待当前批次完成
        join_all(batch_tasks).await;
    }

    /// 运行一组 NWE 策略（随机/网格参数）回测，复用与 Vegas 相同的并发调度思路
    pub async fn run_nwe_random_batch(
        &self,
        params_batch: Vec<(
            NweStrategyConfig,
            rust_quant_strategies::strategy_common::BasicRiskStrategyConfig,
        )>,
        inst_id: &str,
        time: &str,
        arc_candle_item_clone: Arc<Vec<CandleItem>>,
        semaphore: Arc<Semaphore>,
    ) {
        let mut batch_tasks = Vec::with_capacity(params_batch.len());
        for (cfg, risk_cfg) in params_batch {
        let strategy = NweStrategy::new(cfg);
        let inst_id = inst_id.to_string();
        let time = time.to_string();
        let mysql_candles = Arc::clone(&arc_candle_item_clone);
        let permit = Arc::clone(&semaphore);

        let executor = self.clone_for_spawn();
        batch_tasks.push(tokio::spawn(async move {
            let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
            match executor
                .run_nwe_test(&inst_id, &time, strategy, risk_cfg, mysql_candles)
                .await
            {
                Ok(back_test_id) => Some(back_test_id),
                Err(e) => {
                    error!("NWE test failed: {:?}", e);
                    None
                }
            }
        }));
        }

        // 等待当前批次完成
        join_all(batch_tasks).await;
    }

    /// 克隆执行器用于异步任务（内部方法）
    fn clone_for_spawn(&self) -> Arc<Self> {
        Arc::new(BacktestExecutor {
            backtest_service: Arc::clone(&self.backtest_service),
            candle_service: Arc::clone(&self.candle_service),
        })
    }
}

/// 类型转换：domain::SignalResult -> strategies::SignalResult
fn convert_domain_signal_to_strategies_signal(
    domain_signal: rust_quant_domain::SignalResult,
) -> rust_quant_strategies::strategy_common::SignalResult {
    rust_quant_strategies::strategy_common::SignalResult {
        should_buy: domain_signal.should_buy.unwrap_or(false),
        should_sell: domain_signal.should_sell.unwrap_or(false),
        open_price: domain_signal.open_price.unwrap_or(0.0),
        best_open_price: domain_signal.best_open_price,
        best_take_profit_price: domain_signal.best_take_profit_price,
        signal_kline_stop_loss_price: domain_signal.signal_kline_stop_loss_price,
        move_stop_open_price_when_touch_price: domain_signal.move_stop_open_price_when_touch_price.clone(),
        ts: domain_signal.ts.unwrap_or(0),
        single_value: domain_signal.single_value.map(|v| v.to_string()),
        single_result: domain_signal.single_result.map(|v| v.to_string()),
    }
}
