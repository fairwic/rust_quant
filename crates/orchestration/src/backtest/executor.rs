//! 回测执行器
//!
//! 负责回测策略的执行，协调 BacktestService 和 CandleService

use anyhow::{anyhow, Result};
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{error, info};

use rust_quant_common::CandleItem;
use rust_quant_indicators::trend::vegas::VegasStrategy;
use rust_quant_market::models::SelectTime;
use rust_quant_services::market::CandleService;
use rust_quant_services::strategy::BacktestService;
use rust_quant_strategies::framework::backtest::BackTestAbleStrategyTrait;
use rust_quant_strategies::implementations::nwe_strategy::{NweStrategy, NweStrategyConfig};
use rust_quant_strategies::implementations::vegas_backtest::VegasBacktestAdapter;
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
    pub fn new(backtest_service: Arc<BacktestService>, candle_service: Arc<CandleService>) -> Self {
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
        strategy: VegasStrategy,
        risk_strategy_config: BasicRiskStrategyConfig,
        mysql_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        let risk_strategy_config = tighten_vegas_risk(risk_strategy_config);
        let adapter = VegasBacktestAdapter::new(strategy);
        self.run_strategy_backtest(inst_id, time, adapter, risk_strategy_config, mysql_candles)
            .await
    }

    /// 运行 NWE 策略测试
    pub async fn run_nwe_test(
        &self,
        inst_id: &str,
        time: &str,
        strategy: NweStrategy,
        risk_strategy_config: BasicRiskStrategyConfig,
        mysql_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        self.run_strategy_backtest(inst_id, time, strategy, risk_strategy_config, mysql_candles)
            .await
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
        let candle_item_vec = self.candle_service.convert_candles_to_items(&mysql_candles);
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
            let risk_strategy_config = param.to_risk_config();
            let strategy = param.to_vegas_strategy(time.to_string());

            let inst_id = inst_id.to_string();
            let time = time.to_string();
            let mysql_candles = Arc::clone(&arc_candle_item_clone);
            let permit = Arc::clone(&semaphore);

            // 创建任务
            let executor = self.clone_for_spawn();
            batch_tasks.push(tokio::spawn(async move {
                let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
                match executor
                    .run_vegas_test(
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

    async fn run_strategy_backtest<S>(
        &self,
        inst_id: &str,
        period: &str,
        mut strategy: S,
        risk_strategy_config: BasicRiskStrategyConfig,
        mysql_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64>
    where
        S: BackTestAbleStrategyTrait + Send + 'static,
        S::IndicatorValues: Send + Sync,
        S::IndicatorCombine: Send + Sync,
    {
        let start_time = Instant::now();
        let strategy_type = strategy.strategy_type();
        let config_desc = strategy.config_json();
        let res = strategy.run_test(inst_id, &mysql_candles, risk_strategy_config);

        let back_test_id = self
            .backtest_service
            .save_backtest_log(
                inst_id,
                period,
                config_desc,
                res,
                &mysql_candles,
                risk_strategy_config,
                strategy_type.as_str(),
            )
            .await?;

        let elapsed = start_time.elapsed();
        info!(
            "[{} 回测] 完成 inst_id={}, period={}, back_test_id={}, 耗时={}ms",
            strategy_type.as_str(),
            inst_id,
            period,
            back_test_id,
            elapsed.as_millis()
        );
        Ok(back_test_id)
    }
}

/// 针对 Vegas 的统一风控收紧：默认开启信号K线与单K振幅止损，并限制单笔最大亏损
fn tighten_vegas_risk(mut risk: BasicRiskStrategyConfig) -> BasicRiskStrategyConfig {
    let tighten = risk.tighten_vegas_risk.unwrap_or(false);
    if !tighten {
        return risk;
    }

    // 收紧单笔亏损上限，避免大振幅穿透
    risk.max_loss_percent = risk.max_loss_percent.min(0.05);

    // 启用信号K线止损
    if !risk.is_used_signal_k_line_stop_loss.unwrap_or(false) {
        risk.is_used_signal_k_line_stop_loss = Some(true);
    }

    risk
}
