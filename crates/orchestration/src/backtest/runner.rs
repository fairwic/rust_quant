use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{error, info, warn};

use crate::backtest::executor::BacktestExecutor;
use crate::workflow::job_param_generator::{NweParamGenerator, ParamGenerator};
use crate::workflow::progress_manager::{
    NweRandomStrategyConfig, RandomStrategyConfig, StrategyProgressManager,
};
use crate::workflow::strategy_config::{
    get_nwe_strategy_config_from_db, get_strategy_config_from_db, BackTestConfig,
};

use rust_quant_core::database::get_db_pool;
use rust_quant_infrastructure::repositories::{
    SqlxBacktestRepository, SqlxCandleRepository, SqlxStrategyConfigRepository,
};
use rust_quant_services::market::CandleService;
use rust_quant_services::strategy::{BacktestService, StrategyConfigService};
use rust_quant_strategies::implementations::nwe_strategy::NweStrategy;

/// 回测运行器
///
/// 负责管理回测相关的服务和执行逻辑
/// 通过结构体持有服务依赖，避免频繁传递参数
pub struct BacktestRunner {
    /// 回测执行器
    executor: Arc<BacktestExecutor>,
    /// 策略配置服务
    config_service: StrategyConfigService,
}

impl BacktestRunner {
    /// 创建新的回测运行器实例
    ///
    /// # 架构说明
    /// - 在结构体中初始化所有服务依赖
    /// - 通过依赖注入方式创建服务实例
    pub fn new() -> Result<Self> {
        let pool = get_db_pool().clone();
        let backtest_repo = SqlxBacktestRepository::new(pool.clone());
        let backtest_service = Arc::new(BacktestService::new(Box::new(backtest_repo)));

        let candle_repo = SqlxCandleRepository::new(pool.clone());
        let candle_service = Arc::new(CandleService::new(Box::new(candle_repo)));

        let config_repo = SqlxStrategyConfigRepository::new(pool);
        let config_service = StrategyConfigService::new(Box::new(config_repo));

        let executor = Arc::new(BacktestExecutor::new(backtest_service, candle_service));

        Ok(Self {
            executor,
            config_service,
        })
    }

    /// 执行回测任务
    ///
    /// 根据环境变量控制随机/指定模式，遍历 inst_id 与 period 组合执行回测
    pub async fn run(&self, targets: &[(String, String)]) -> Result<()> {
        if targets.is_empty() {
            return Err(anyhow!("未提供任何回测目标"));
        }

        let config = BackTestConfig::default();

        let mut success = 0usize;

        for (inst_id, period) in targets.iter() {
            match self.run_backtest_for_pair(inst_id, period, &config).await {
                Ok(_) => success += 1,
                Err(e) => error!(
                    "回测执行失败: inst_id={}, period={}, err={}",
                    inst_id, period, e
                ),
            }
        }

        let total = targets.len();

        if success == 0 {
            return Err(anyhow!("所有回测任务都失败了，共 {} 个组合", total));
        }

        if success < total {
            warn!("部分回测失败，成功 {}/{}", success, total);
        } else {
            info!("全部回测执行成功，共 {} 个组合", total);
        }

        Ok(())
    }
}

/// 回测执行入口（兼容性函数）
///
/// 根据环境变量控制随机/指定模式，遍历 inst_id 与 period 组合执行回测
pub async fn run_backtest_runner(targets: &[(String, String)]) -> Result<()> {
    let runner = BacktestRunner::new()?;
    runner.run(targets).await
}

impl BacktestRunner {
    /// 执行单个交易对的回测
    async fn run_backtest_for_pair(
        &self,
        inst_id: &str,
        period: &str,
        config: &BackTestConfig,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "开始执行回测: inst_id={}, period={}, config={:?}",
            inst_id, period, config
        );

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

        let mut executed = false;

        if config.enable_random_test_nwe {
            executed = true;
            self.run_nwe_random_backtest(inst_id, period, semaphore.clone(), config)
                .await?;
        }

        if config.enable_specified_test_nwe {
            executed = true;
            self.run_nwe_specified_backtest(inst_id, period, config, semaphore.clone())
                .await?;
        }

        if config.enable_random_test_vegas {
            executed = true;
            self.run_vegas_random_backtest(inst_id, period, semaphore.clone(), config)
                .await?;
        }

        if config.enable_specified_test_vegas {
            executed = true;
            self.run_vegas_specified_backtest(inst_id, period, semaphore.clone(), config)
                .await?;
        }

        if !executed {
            warn!("未启用任何回测模式，inst_id={} period={}", inst_id, period);
        }

        let elapsed = start.elapsed();
        info!(
            "回测执行完成: inst_id={}, period={}, 耗时={}ms",
            inst_id,
            period,
            elapsed.as_millis()
        );

        Ok(())
    }

    /// 执行 NWE 随机回测
    async fn run_nwe_random_backtest(
        &self,
        inst_id: &str,
        period: &str,
        semaphore: Arc<Semaphore>,
        config: &BackTestConfig,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "[NWE 随机] 开始随机回测: inst_id={}, period={}",
            inst_id, period
        );

        let arc_candle_data = self
            .executor
            .load_and_convert_candle_data(inst_id, period, config.candle_limit)
            .await?;

        let random_config = build_default_nwe_random_config(config.max_concurrent);

        let progress = match StrategyProgressManager::load_progress(inst_id, period).await? {
            Some(saved) => {
                if StrategyProgressManager::is_config_changed_nwe(&random_config, &saved) {
                    warn!(
                        "[NWE 随机] 回测配置变更，重置进度: inst_id={}, period={}",
                        inst_id, period
                    );
                    StrategyProgressManager::create_new_progress_nwe(
                        inst_id,
                        period,
                        &random_config,
                    )
                } else if saved.status == "completed" {
                    info!(
                        "[NWE 随机] 已找到完成进度，跳过执行: inst_id={}, period={}",
                        inst_id, period
                    );
                    return Ok(());
                } else {
                    saved
                }
            }
            None => {
                StrategyProgressManager::create_new_progress_nwe(inst_id, period, &random_config)
            }
        };

        StrategyProgressManager::save_progress(&progress).await?;

        let mut generator = NweParamGenerator::new(
            random_config.stc_fast_length.clone(),
            random_config.stc_slow_length.clone(),
            random_config.stc_cycle_length.clone(),
            random_config.stc_d1_length.clone(),
            random_config.stc_d2_length.clone(),
            random_config.rsi_periods.clone(),
            random_config.rsi_over_buy_sell.clone(),
            random_config.atr_periods.clone(),
            random_config.atr_multipliers.clone(),
            random_config.volume_bar_num.clone(),
            random_config.volume_ratios.clone(),
            random_config.nwe_periods.clone(),
            random_config.nwe_multi.clone(),
            random_config.max_loss_percent.clone(),
            random_config.take_profit_ratios.clone(),
            random_config.is_move_stop_loss.clone(),
            random_config.is_used_signal_k_line_stop_loss.clone(),
            random_config
                .is_move_stop_open_price_when_touch_price
                .clone(),
            random_config.k_line_hammer_shadow_ratios.clone(),
        );

        generator.set_current_index(progress.current_index);

        loop {
            let batch = generator.get_next_batch(random_config.batch_size);
            if batch.is_empty() {
                break;
            }

            self.executor
                .run_nwe_random_batch(
                    batch,
                    inst_id,
                    period,
                    arc_candle_data.clone(),
                    semaphore.clone(),
                )
                .await;

            let (current_index, total_count) = generator.progress();

            StrategyProgressManager::update_progress(inst_id, period, current_index, current_index)
                .await?;

            info!(
                "[NWE 随机] 进度 {}/{} ({:.2}%), inst_id={}, period={}",
                current_index,
                total_count,
                if total_count == 0 {
                    0.0
                } else {
                    (current_index as f64 / total_count as f64) * 100.0
                },
                inst_id,
                period
            );
        }

        StrategyProgressManager::mark_completed(inst_id, period).await?;

        let elapsed = start.elapsed();
        info!(
            "[NWE 随机] 完成随机回测 inst_id={}, period={}, 耗时={}ms",
            inst_id,
            period,
            elapsed.as_millis()
        );

        Ok(())
    }

    /// 执行 NWE 指定配置回测
    async fn run_nwe_specified_backtest(
        &self,
        inst_id: &str,
        period: &str,
        config: &BackTestConfig,
        _semaphore: Arc<Semaphore>,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "[NWE 指定] 开始指定配置回测: inst_id={}, period={}",
            inst_id, period
        );

        let arc_candle_data = self
            .executor
            .load_and_convert_candle_data(inst_id, period, config.candle_limit)
            .await?;

        let pairs = get_nwe_strategy_config_from_db(&self.config_service, inst_id, period).await?;
        if pairs.is_empty() {
            warn!(
                "[NWE 指定] 未找到策略配置，跳过 inst_id={}, period={}",
                inst_id, period
            );
            return Ok(());
        }

        let mut success = 0usize;
        let total_configs = pairs.len();

        for (cfg, risk_cfg) in pairs.into_iter() {
            let strategy = NweStrategy::new(cfg);
            match self
                .executor
                .run_nwe_test(inst_id, period, strategy, risk_cfg, arc_candle_data.clone())
                .await
            {
                Ok(_) => {
                    success += 1;
                }
                Err(e) => {
                    error!(
                        "[NWE 指定] 策略执行失败: inst_id={}, period={}, err={}",
                        inst_id, period, e
                    );
                }
            }
        }

        if success == 0 {
            return Err(anyhow!(
                "NWE 指定配置回测全部失败: inst_id={}, period={}",
                inst_id,
                period
            ));
        }

        if success < total_configs {
            warn!(
                "[NWE 指定] 有策略执行失败，成功 {}/{}: inst_id={}, period={}",
                success, total_configs, inst_id, period
            );
        }

        let elapsed = start.elapsed();
        info!(
            "[NWE 指定] 完成指定配置回测 inst_id={}, period={}, 成功 {}/{}，耗时={}ms",
            inst_id,
            period,
            success,
            total_configs,
            elapsed.as_millis()
        );

        Ok(())
    }

    /// 执行 Vegas 随机回测
    async fn run_vegas_random_backtest(
        &self,
        inst_id: &str,
        period: &str,
        semaphore: Arc<Semaphore>,
        config: &BackTestConfig,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "[Vegas 随机] 开始随机回测: inst_id={}, period={}",
            inst_id, period
        );

        let arc_candle_data = self
            .executor
            .load_and_convert_candle_data(inst_id, period, config.candle_limit)
            .await?;

        let random_config = RandomStrategyConfig::default();

        let progress = match StrategyProgressManager::load_progress(inst_id, period).await? {
            Some(saved) => {
                if StrategyProgressManager::is_config_changed(&random_config, &saved) {
                    warn!(
                        "[Vegas 随机] 配置变更，重置进度: inst_id={}, period={}",
                        inst_id, period
                    );
                    StrategyProgressManager::create_new_progress(inst_id, period, &random_config)
                } else if saved.status == "completed" {
                    info!(
                        "[Vegas 随机] 已找到完成进度，跳过执行: inst_id={}, period={}",
                        inst_id, period
                    );
                    return Ok(());
                } else {
                    saved
                }
            }
            None => StrategyProgressManager::create_new_progress(inst_id, period, &random_config),
        };

        StrategyProgressManager::save_progress(&progress).await?;

        let mut generator = ParamGenerator::new(
            random_config.bb_periods.clone(),
            random_config.shadow_ratios.clone(),
            random_config.bb_multipliers.clone(),
            random_config.volume_bar_nums.clone(),
            random_config.volume_ratios.clone(),
            random_config.breakthrough_thresholds.clone(),
            random_config.rsi_periods.clone(),
            random_config.rsi_over_buy_sell.clone(),
            random_config.max_loss_percent.clone(),
            random_config.take_profit_ratios.clone(),
            random_config.is_move_stop_loss.clone(),
            random_config.is_used_signal_k_line_stop_loss.clone(),
            random_config
                .is_move_stop_open_price_when_touch_price
                .clone(),
            random_config.fix_signal_kline_take_profit_ratios.clone(),
        );

        generator.set_current_index(progress.current_index);

        loop {
            let batch = generator.get_next_batch(random_config.batch_size);
            if batch.is_empty() {
                break;
            }

            self.executor
                .run_back_test_strategy(
                    batch,
                    inst_id,
                    period,
                    arc_candle_data.clone(),
                    semaphore.clone(),
                )
                .await;

            let (current_index, total_count) = generator.progress();

            StrategyProgressManager::update_progress(inst_id, period, current_index, current_index)
                .await?;

            info!(
                "[Vegas 随机] 进度 {}/{} ({:.2}%), inst_id={}, period={}",
                current_index,
                total_count,
                if total_count == 0 {
                    0.0
                } else {
                    (current_index as f64 / total_count as f64) * 100.0
                },
                inst_id,
                period
            );
        }

        StrategyProgressManager::mark_completed(inst_id, period).await?;

        let elapsed = start.elapsed();
        info!(
            "[Vegas 随机] 完成随机回测 inst_id={}, period={}, 耗时={}ms",
            inst_id,
            period,
            elapsed.as_millis()
        );

        Ok(())
    }

    /// 执行 Vegas 指定配置回测
    async fn run_vegas_specified_backtest(
        &self,
        inst_id: &str,
        period: &str,
        semaphore: Arc<Semaphore>,
        config: &BackTestConfig,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "[Vegas 指定] 开始指定配置回测: inst_id={}, period={}",
            inst_id, period
        );

        let arc_candle_data = self
            .executor
            .load_and_convert_candle_data(inst_id, period, config.candle_limit)
            .await?;

        let params_batch =
            get_strategy_config_from_db(&self.config_service, inst_id, period).await?;
        if params_batch.is_empty() {
            warn!(
                "[Vegas 指定] 未找到策略配置，跳过 inst_id={}, period={}",
                inst_id, period
            );
            return Ok(());
        }

        info!(
            "[Vegas 指定] 找到 {} 个策略配置，开始执行回测",
            params_batch.len()
        );

        self.executor
            .run_back_test_strategy(params_batch, inst_id, period, arc_candle_data, semaphore)
            .await;

        let elapsed = start.elapsed();
        info!(
            "[Vegas 指定] 完成指定配置回测 inst_id={}, period={}, 耗时={}ms",
            inst_id,
            period,
            elapsed.as_millis()
        );

        Ok(())
    }
}

/// 构建默认的 NWE 随机配置
fn build_default_nwe_random_config(batch_size: usize) -> NweRandomStrategyConfig {
    NweRandomStrategyConfig {
        rsi_periods: vec![6],
        rsi_over_buy_sell: vec![(70.0, 30.0)],

        stc_fast_length: vec![23],
        stc_slow_length: vec![50],
        stc_cycle_length: vec![10],
        stc_d1_length: vec![3],
        stc_d2_length: vec![3],

        atr_periods: vec![6, 12, 24, 36, 48, 60],
        atr_multipliers: vec![2.0, 2.2, 2.5, 2.6, 2.7, 2.8, 3.0],

        nwe_periods: vec![6, 8, 12, 16, 20, 24],
        nwe_multi: vec![1.2, 1.4, 1.6, 1.8, 2.0],

        volume_bar_num: vec![3],
        volume_ratios: vec![0.8],
        batch_size,
        max_loss_percent: vec![0.02, 0.01],

        take_profit_ratios: vec![1.8, 2.0, 2.2, 2.4, 2.6, 2.8, 3.0],

        is_move_stop_loss: vec![false, true],
        k_line_hammer_shadow_ratios: vec![0.65],
        is_used_signal_k_line_stop_loss: vec![false, true],
        is_move_stop_open_price_when_touch_price: vec![false, true],
    }
}
