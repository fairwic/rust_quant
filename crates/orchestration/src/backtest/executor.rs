//! 回测执行器
//!
//! 负责回测策略的执行，协调 BacktestService 和 CandleService
use crate::infra::data_validator;
use crate::workflow::job_param_generator::ParamMergeBuilder;
use anyhow::{anyhow, Result};
use futures::future::join_all;
use rust_quant_common::CandleItem;
use rust_quant_domain::StrategyType;
use rust_quant_indicators::trend::vegas::VegasStrategy;
use rust_quant_market::models::SelectTime;
use rust_quant_services::market::CandleService;
use rust_quant_services::strategy::BacktestService;
use rust_quant_strategies::framework::backtest::BackTestAbleStrategyTrait;
use rust_quant_strategies::implementations::nwe_strategy::{NweStrategy, NweStrategyConfig};
use rust_quant_strategies::implementations::vegas_backtest::VegasBacktestAdapter;
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{error, info};
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
    /// backtestservice，用于交易策略计算。
    backtest_service: Arc<BacktestService>,
    /// K 线service，用于交易策略计算。
    candle_service: Arc<CandleService>,
}
impl BacktestExecutor {
    /// 创建回测执行器实例
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
        source_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        let adapter = VegasBacktestAdapter::new(strategy);
        self.run_strategy_backtest(inst_id, time, adapter, risk_strategy_config, source_candles)
            .await
    }
    /// 使用独立策略身份运行共享 Vegas 引擎。
    pub async fn run_vegas_test_as(
        &self,
        inst_id: &str,
        time: &str,
        strategy: VegasStrategy,
        strategy_type: StrategyType,
        risk_strategy_config: BasicRiskStrategyConfig,
        source_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        let adapter = VegasBacktestAdapter::with_strategy_type(strategy, strategy_type);
        self.run_strategy_backtest(inst_id, time, adapter, risk_strategy_config, source_candles)
            .await
    }
    /// 运行 NWE 策略测试
    pub async fn run_nwe_test(
        &self,
        inst_id: &str,
        time: &str,
        strategy: NweStrategy,
        risk_strategy_config: BasicRiskStrategyConfig,
        source_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64> {
        self.run_strategy_backtest(
            inst_id,
            time,
            strategy,
            risk_strategy_config,
            source_candles,
        )
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
        let source_candles = self
            .candle_service
            .get_confirmed_candles_for_backtest(inst_id, period, limit, select_time)
            .await?;
        let db_query_duration = db_query_start.elapsed();
        let validation_start = Instant::now();
        data_validator::valid_candles_continuity(&source_candles, period)?;
        let validation_duration = validation_start.elapsed();
        let total_duration = start_time.elapsed();
        info!(
            "[性能跟踪] get_candle_data_confirm 完成 - 总耗时: {}ms, 数据库查询: {}ms, 数据验证: {}ms, 数据条数: {}",
            total_duration.as_millis(),
            db_query_duration.as_millis(),
            validation_duration.as_millis(),
            source_candles.len()
        );
        Ok(source_candles)
    }
    /// 加载并转换K线数据
    pub async fn load_and_convert_candle_data(
        &self,
        inst_id: &str,
        time: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> Result<Arc<Vec<CandleItem>>> {
        let start_time = Instant::now();
        info!(
            "[性能跟踪] 开始加载K线数据: inst_id={}, time={}, limit={}",
            inst_id, time, limit
        );
        let data_fetch_start = Instant::now();
        let source_candles = self
            .get_candle_data_confirm(inst_id, time, limit, select_time)
            .await
            .map_err(|e| anyhow!("获取K线数据失败: {}", e))?;
        let data_fetch_duration = data_fetch_start.elapsed();
        if source_candles.is_empty() {
            return Err(anyhow!("K线数据为空"));
        }
        let data_convert_start = Instant::now();
        let candle_item_vec = self
            .candle_service
            .convert_candles_to_items(&source_candles);
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
        strategy_type: StrategyType,
        arc_candle_item_clone: Arc<Vec<CandleItem>>,
        semaphore: Arc<Semaphore>,
    ) {
        let mut batch_tasks = Vec::with_capacity(params_batch.len());
        for param in params_batch {
            let risk_strategy_config = param.to_risk_config();
            let strategy = param.to_vegas_strategy(time.to_string());
            let inst_id = inst_id.to_string();
            let time = time.to_string();
            let strategy_type = strategy_type;
            let source_candles = Arc::clone(&arc_candle_item_clone);
            let permit = Arc::clone(&semaphore);
            // 创建任务
            let executor = self.clone_for_spawn();
            batch_tasks.push(tokio::spawn(async move {
                let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
                match executor
                    .run_vegas_test_as(
                        &inst_id,
                        &time,
                        strategy,
                        strategy_type,
                        risk_strategy_config,
                        source_candles,
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
            let source_candles = Arc::clone(&arc_candle_item_clone);
            let permit = Arc::clone(&semaphore);
            let executor = self.clone_for_spawn();
            batch_tasks.push(tokio::spawn(async move {
                let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
                match executor
                    .run_nwe_test(&inst_id, &time, strategy, risk_cfg, source_candles)
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
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn run_strategy_backtest<S>(
        &self,
        inst_id: &str,
        period: &str,
        strategy: S,
        risk_strategy_config: BasicRiskStrategyConfig,
        source_candles: Arc<Vec<CandleItem>>,
    ) -> Result<i64>
    where
        S: BackTestAbleStrategyTrait + Send + 'static,
        S::IndicatorValues: Send + Sync,
        S::IndicatorCombine: Send + Sync,
    {
        let start_time = Instant::now();
        let strategy_type = strategy.strategy_type();
        let compute_start = Instant::now();
        let compute_inst_id = inst_id.to_string();
        let compute_candles = Arc::clone(&source_candles);
        // Vegas 回放是 CPU 密集型同步循环；放入 blocking pool，避免并发批次饿死 Tokio
        // 的数据库保存、Redis 进度和停止检查。Semaphore 仍负责限制同时在跑的组合数。
        let (config_desc, res) = tokio::task::spawn_blocking(move || {
            let config_desc = strategy.config_json();
            let result =
                strategy.run_test(&compute_inst_id, &compute_candles, risk_strategy_config);
            (config_desc, result)
        })
        .await
        .map_err(|error| anyhow!("回测计算任务异常退出: {}", error))?;
        let compute_duration = compute_start.elapsed();
        let persist_start = Instant::now();
        let back_test_id = self
            .backtest_service
            .save_backtest_log(
                inst_id,
                period,
                config_desc,
                res,
                &source_candles,
                risk_strategy_config,
                strategy_type.as_str(),
            )
            .await?;
        let persist_duration = persist_start.elapsed();
        let elapsed = start_time.elapsed();
        info!(
            "[{} 回测] 完成 inst_id={}, period={}, back_test_id={}, total_ms={}, compute_ms={}, persist_ms={}",
            strategy_type.as_str(),
            inst_id,
            period,
            back_test_id,
            elapsed.as_millis(),
            compute_duration.as_millis(),
            persist_duration.as_millis(),
        );
        Ok(back_test_id)
    }
}
