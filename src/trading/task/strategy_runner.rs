use anyhow::{anyhow, Result};
use dashmap::DashMap;
use okx::dto::EnumToStrTrait;
use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::model::strategy::strategy_job_signal_log::{
    StrategyJobSignalLog, StrategyJobSignalLogModel,
};
use crate::trading::services::order_service::swap_order_service::SwapOrderService;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::{
    self, get_hash_key, ArcVegasIndicatorValues,
};
use crate::trading::strategy::nwe_strategy::{NweStrategy, NweStrategyConfig};
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::{
    get_multi_indicator_values, parse_candle_to_data_item, BasicRiskStrategyConfig, SignalResult,
};
use crate::trading::strategy::{Strategy, StrategyType};
use crate::trading::task::backtest_executor::{
    load_and_convert_candle_data, run_back_test_strategy,
};
use crate::trading::task::job_param_generator::ParamGenerator;
use crate::trading::task::progress_manager::{RandomStrategyConfig, StrategyProgressManager};
use crate::trading::task::strategy_config::{
    get_strategy_config_from_db, test_specified_strategy_with_config, BackTestConfig,
};
use crate::CandleItem;

/// 策略执行状态跟踪 - 用于时间戳去重
#[derive(Debug, Clone)]
struct StrategyExecutionState {
    timestamp: i64,
    start_time: SystemTime,
}

/// 全局策略执行状态管理器 - 防止重复处理相同时间戳的K线
static STRATEGY_EXECUTION_STATES: Lazy<DashMap<String, StrategyExecutionState>> =
    Lazy::new(|| DashMap::new());

/// 策略执行状态管理器
pub struct StrategyExecutionStateManager;

impl StrategyExecutionStateManager {
    /// 检查并标记策略执行状态
    /// 返回 true 表示可以执行，false 表示应该跳过（正在处理或已处理）
    pub fn try_mark_processing(key: &str, timestamp: i64) -> bool {
        let state_key = format!("{}_{}", key, timestamp);

        // 检查是否已经在处理
        if STRATEGY_EXECUTION_STATES.contains_key(&state_key) {
            debug!("跳过重复处理: key={}, timestamp={}", key, timestamp);
            return false;
        }

        // 标记为正在处理
        let state = StrategyExecutionState {
            timestamp,
            start_time: SystemTime::now(),
        };

        STRATEGY_EXECUTION_STATES.insert(state_key.clone(), state);
        info!("标记策略执行状态: key={}, timestamp={}", key, timestamp);
        true
    }

    /// 完成策略执行，清理状态
    pub fn mark_completed(key: &str, timestamp: i64) {
        let state_key = format!("{}_{}", key, timestamp);
        if let Some((_, state)) = STRATEGY_EXECUTION_STATES.remove(&state_key) {
            let duration = SystemTime::now()
                .duration_since(state.start_time)
                .unwrap_or(Duration::from_millis(0));
            info!(
                "策略执行完成: key={}, timestamp={}, 耗时={:?}",
                key, timestamp, duration
            );
        }
    }

    /// 清理过期的执行状态（超过5分钟的记录）
    pub fn cleanup_expired_states() {
        let now = SystemTime::now();
        let mut expired_keys = Vec::new();

        for entry in STRATEGY_EXECUTION_STATES.iter() {
            if let Ok(duration) = now.duration_since(entry.value().start_time) {
                if duration > Duration::from_secs(300) {
                    // 5分钟
                    expired_keys.push(entry.key().clone());
                }
            }
        }

        for key in expired_keys {
            STRATEGY_EXECUTION_STATES.remove(&key);
        }
    }

    /// 获取当前处理状态统计
    pub fn get_stats() -> (usize, Vec<String>) {
        let count = STRATEGY_EXECUTION_STATES.len();
        let keys: Vec<String> = STRATEGY_EXECUTION_STATES
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        (count, keys)
    }
}

/// 测试随机策略
pub async fn test_random_strategy(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
) -> Result<()> {
    test_random_strategy_with_config(inst_id, time, semaphore, RandomStrategyConfig::default())
        .await
}

/// 带配置的随机策略测试（支持断点续传）
pub async fn test_random_strategy_with_config(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
    config: RandomStrategyConfig,
) -> Result<()> {
    let start_time = Instant::now();
    info!(
        "[断点续传] test_random_strategy_with_config 开始: inst_id={}, time={}",
        inst_id, time
    );

    // 🔄 **步骤1: 检查是否有已保存的进度**
    let progress_check_start = Instant::now();
    let mut current_progress = match StrategyProgressManager::load_progress(inst_id, time).await {
        Ok(Some(saved_progress)) => {
            if StrategyProgressManager::is_config_changed(&config, &saved_progress) {
                warn!(
                    "[断点续传] 配置已变化，重新开始测试: inst_id={}, time={}, 旧哈希={}, 新哈希={}",
                    inst_id, time, saved_progress.config_hash, config.calculate_hash()
                );
                StrategyProgressManager::create_new_progress(inst_id, time, &config)
            } else {
                info!(
                    "[断点续传] 发现已保存的进度: inst_id={}, time={}, 已完成 {}/{} 个组合",
                    inst_id,
                    time,
                    saved_progress.completed_combinations,
                    saved_progress.total_combinations
                );

                if saved_progress.status == "completed" {
                    info!("[断点续传] 测试已完成，跳过执行");
                    return Ok(());
                }
                saved_progress
            }
        }
        Ok(None) => {
            info!("[断点续传] 未找到已保存的进度，创建新的进度记录");
            StrategyProgressManager::create_new_progress(inst_id, time, &config)
        }
        Err(e) => {
            warn!("[断点续传] 加载进度失败，创建新的进度记录: {}", e);
            StrategyProgressManager::create_new_progress(inst_id, time, &config)
        }
    };
    let progress_check_duration = progress_check_start.elapsed();

    // 🔧 **步骤2: 构建参数生成器并设置起始位置**
    let param_gen_start = Instant::now();
    let mut param_generator = ParamGenerator::new(
        config.bb_periods.clone(),
        config.shadow_ratios.clone(),
        config.bb_multipliers.clone(),
        config.volume_bar_nums.clone(),
        config.volume_ratios.clone(),
        config.breakthrough_thresholds.clone(),
        config.rsi_periods.clone(),
        config.rsi_over_buy_sell.clone(),
        config.max_loss_percent.clone(),
        config.take_profit_ratios.clone(),
        config.is_move_stop_loss.clone(),
        config.is_used_signal_k_line_stop_loss.clone(),
    );

    // 🎯 **关键: 设置生成器的起始位置**
    param_generator.set_current_index(current_progress.current_index);

    let (current_index, total_count) = param_generator.progress();
    let param_gen_duration = param_gen_start.elapsed();
    info!(
        "[断点续传] 参数生成器创建完成 - 耗时: {}ms, 总参数组合: {}, 起始索引: {}, 剩余: {}",
        param_gen_duration.as_millis(),
        total_count,
        current_index,
        total_count - current_index
    );

    // 保存初始进度
    StrategyProgressManager::save_progress(&current_progress).await?;

    // 📊 **步骤3: 获取并转换K线数据**
    let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;

    // 🔄 **步骤4: 批量处理参数组合（支持断点续传）**
    let mut processed_count = current_progress.completed_combinations;
    let batch_processing_start = Instant::now();

    loop {
        let batch_start = Instant::now();
        let params_batch = param_generator.get_next_batch(config.batch_size);
        if params_batch.is_empty() {
            break;
        }

        // 执行回测
        let batch_len = params_batch.len();
        run_back_test_strategy(
            params_batch,
            inst_id,
            time,
            arc_candle_data.clone(),
            semaphore.clone(),
        )
        .await;

        // 更新进度
        processed_count += batch_len;
        let (current_index, _) = param_generator.progress();

        // 💾 **定期保存进度**
        if let Err(e) =
            StrategyProgressManager::update_progress(inst_id, time, processed_count, current_index)
                .await
        {
            warn!("[断点续传] 保存进度失败: {}", e);
        }

        let batch_duration = batch_start.elapsed();
        info!(
            "[断点续传] 批次处理完成 - 已处理 {}/{} 个参数组合, 本批次耗时: {}ms, 进度: {:.2}%",
            processed_count.min(total_count),
            total_count,
            batch_duration.as_millis(),
            (processed_count as f64 / total_count as f64) * 100.0
        );
    }

    // 🎉 **步骤5: 标记完成**
    StrategyProgressManager::mark_completed(inst_id, time).await?;

    let batch_processing_duration = batch_processing_start.elapsed();
    let total_duration = start_time.elapsed();
    info!(
        "[断点续传] test_random_strategy_with_config 完成 - 总耗时: {}ms, 进度检查: {}ms, 参数生成: {}ms, 批量处理: {}ms, 处理组合数: {}",
        total_duration.as_millis(),
        progress_check_duration.as_millis(),
        param_gen_duration.as_millis(),
        batch_processing_duration.as_millis(),
        total_count
    );
    Ok(())
}

/// 主函数，执行所有策略测试
pub async fn back_test(inst_id: &str, time: &str) -> Result<()> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] vegas_back_test 开始 - inst_id: {}, time: {}",
        inst_id, time
    );

    let result = back_test_with_config(inst_id, time, BackTestConfig::default()).await;

    let duration = start_time.elapsed();
    info!(
        "[性能跟踪] vegas_back_test 完成 - 总耗时: {}ms",
        duration.as_millis()
    );

    result
}

pub async fn back_test_with_config(
    inst_id: &str,
    time: &str,
    config: BackTestConfig,
) -> Result<()> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] back_test_with_config 开始 - inst_id={}, time={}, config={:?}",
        inst_id, time, config
    );

    // 验证输入参数
    if inst_id.is_empty() || time.is_empty() {
        return Err(anyhow!(
            "无效的输入参数: inst_id={}, time={}",
            inst_id,
            time
        ));
    }

    // 创建信号量限制并发数
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

    // 执行不同类型的测试
    let mut test_results = Vec::new();

    if config.enable_random_test_vegas {
        let random_start = Instant::now();
        info!("[性能跟踪] 开始执行随机策略测试");
        if let Err(e) = test_random_strategy(inst_id, time, semaphore.clone()).await {
            error!("随机策略测试失败: {}", e);
            test_results.push(("random", false));
        } else {
            test_results.push(("random", true));
        }
        let random_duration = random_start.elapsed();
        info!(
            "[性能跟踪] 随机策略测试完成 - 耗时: {}ms",
            random_duration.as_millis()
        );
    }

    if config.enable_specified_test_vegas {
        if let Err(e) = test_specified_strategy(inst_id, time, semaphore.clone()).await {
            error!("指定策略测试失败: {}", e);
            test_results.push(("specified", false));
        } else {
            test_results.push(("specified", true));
        }
    }

    // 新增：NWE 策略单独回测（通过环境变量开启）
    // NWE 随机回测
    if config.enable_random_test_nwe {
        let arc_candle_data =
            load_and_convert_candle_data(inst_id, time, config.candle_limit).await?;
        let mut risk_strategy_config = BasicRiskStrategyConfig::default();
        risk_strategy_config.take_profit_ratio = 1.5;

        // 断点续传：构建 NWE 随机配置
        use crate::trading::task::progress_manager::{
            NweRandomStrategyConfig, StrategyProgressManager,
        };
        let nwe_random_config = NweRandomStrategyConfig {
            rsi_periods: vec![11, 12, 13, 14, 15, 16],
            rsi_over_buy_sell: vec![
                (65.0, 35.0),
                (70.0, 30.0),
                (75.0, 25.0),
                (80.0, 20.0),
                (85.0, 15.0),
                (90.0, 10.0),
            ],
            atr_periods: vec![6, 8, 10],
            atr_multipliers: vec![2.5, 3.0, 3.5],
            volume_bar_nums: vec![3, 4, 5, 6],
            volume_ratios: vec![0.8, 0.9, 1.0],
            nwe_periods: vec![7, 8, 9, 10],
            nwe_multi: vec![1.0, 1.3, 1.5, 1.8, 2.0, 2.2, 2.4],
            batch_size: config.max_concurrent,
            // 风险参数空间（参考 Vegas）
            max_loss_percent: vec![0.01, 0.02, 0.03],
            take_profit_ratios: vec![0.0, 0.5, 1.0, 1.5, 1.8, 2.0, 2.5],
            is_move_stop_loss: vec![false],
            is_used_signal_k_line_stop_loss: vec![false],
        };

        // 加载或初始化进度
        let progress_key_check = Instant::now();
        let mut current_progress = match StrategyProgressManager::load_progress(inst_id, time).await
        {
            Ok(Some(saved)) => {
                if StrategyProgressManager::is_config_changed_nwe(&nwe_random_config, &saved) {
                    warn!(
                        "[NWE 断点续传] 配置变更，重置进度: inst_id={}, time={}, 旧哈希={}, 新哈希={}",
                        inst_id,
                        time,
                        saved.config_hash,
                        nwe_random_config.calculate_hash()
                    );
                    StrategyProgressManager::create_new_progress_nwe(
                        inst_id,
                        time,
                        &nwe_random_config,
                    )
                } else {
                    info!(
                        "[NWE 断点续传] 载入进度: {}/{}",
                        saved.completed_combinations, saved.total_combinations
                    );
                    saved
                }
            }
            Ok(None) => {
                info!("[NWE 断点续传] 未发现进度，创建新记录");
                StrategyProgressManager::create_new_progress_nwe(inst_id, time, &nwe_random_config)
            }
            Err(e) => {
                warn!("[NWE 断点续传] 读取进度失败，创建新记录: {}", e);
                StrategyProgressManager::create_new_progress_nwe(inst_id, time, &nwe_random_config)
            }
        };
        info!(
            "[NWE 断点续传] 进度检查耗时: {}ms",
            progress_key_check.elapsed().as_millis()
        );
        StrategyProgressManager::save_progress(&current_progress).await?;

        // 参数生成器并设置断点索引
        use crate::trading::task::job_param_generator::NweParamGenerator;
        let mut gen = NweParamGenerator::new(
            nwe_random_config.rsi_periods.clone(),
            nwe_random_config.rsi_over_buy_sell.clone(),
            nwe_random_config.atr_periods.clone(),
            nwe_random_config.atr_multipliers.clone(),
            nwe_random_config.volume_bar_nums.clone(),
            nwe_random_config.volume_ratios.clone(),
            nwe_random_config.nwe_periods.clone(),
            nwe_random_config.nwe_multi.clone(),
            nwe_random_config.max_loss_percent.clone(),
            nwe_random_config.take_profit_ratios.clone(),
            nwe_random_config.is_move_stop_loss.clone(),
            nwe_random_config.is_used_signal_k_line_stop_loss.clone(),
        );
        gen.set_current_index(current_progress.current_index);

        // 遍历所有组合（分批），并更新进度
        let mut processed = current_progress.completed_combinations;
        loop {
            let get_batch_start = Instant::now();
            let batch = gen.get_next_batch(nwe_random_config.batch_size);
            let get_batch_elapsed = get_batch_start.elapsed();
            if batch.is_empty() {
                break;
            }
            info!(
                "[NWE 断点续传] 获取批次: {} 条, 耗时: {}ms",
                batch.len(),
                get_batch_elapsed.as_millis()
            );

            let run_start = Instant::now();
            crate::trading::task::backtest_executor::run_nwe_random_batch(
                batch,
                inst_id,
                time,
                arc_candle_data.clone(),
                semaphore.clone(),
            )
            .await;
            let run_elapsed = run_start.elapsed();

            processed += nwe_random_config
                .batch_size
                .min(current_progress.total_combinations);
            let (current_index, total) = gen.progress();
            StrategyProgressManager::update_progress(
                inst_id,
                time,
                processed.min(total),
                current_index,
            )
            .await?;
            info!(
                "[NWE 断点续传] 批次完成: 进度 {}/{}, 批次耗时: {}ms",
                processed.min(total),
                total,
                run_elapsed.as_millis()
            );
        }

        StrategyProgressManager::mark_completed(inst_id, time).await?;
        info!("[NWE] 随机参数遍历完成，总回测组合: {}", processed);
        test_results.push(("nwe_random", true));
    }

    // NWE 指定配置回测（从DB或内置指定）
    if config.enable_specified_test_nwe {
        let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;
        let risk_strategy_config = BasicRiskStrategyConfig::default();
        //指定策略
        let nwe_strategy = NweStrategy::new(NweStrategyConfig::default());
        if let Err(e) = crate::trading::task::backtest_executor::run_nwe_test(
            inst_id,
            time,
            nwe_strategy,
            risk_strategy_config,
            arc_candle_data,
        )
        .await
        {
            error!("NWE 指定策略测试失败: {}", e);
            test_results.push(("nwe_specified", false));
        } else {
            test_results.push(("nwe_specified", true));
        }
    }

    // 汇总测试结果
    let success_count = test_results.iter().filter(|(_, success)| *success).count();
    let total_count = test_results.len();

    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] vegas_back_test_with_config 完成 - 总耗时: {}ms, 成功 {}/{}, 详情: {:?}",
        total_duration.as_millis(),
        success_count,
        total_count,
        test_results
    );

    if success_count == 0 && total_count > 0 {
        return Err(anyhow!("所有策略测试都失败了"));
    }

    Ok(())
}

/// 测试指定策略
pub async fn test_specified_strategy(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
) -> Result<()> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] test_specified_strategy 开始: inst_id={}, time={}",
        inst_id, time
    );

    // 获取策略配置阶段
    let config_get_start = Instant::now();
    let params_batch = get_strategy_config_from_db(inst_id, time).await?;
    let config_get_duration = config_get_start.elapsed();
    info!(
        "[性能跟踪] 策略配置获取完成 - 耗时: {}ms, 配置数量: {}",
        config_get_duration.as_millis(),
        params_batch.len()
    );

    // 加载K线数据阶段
    let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;

    // 执行回测阶段
    let backtest_start = Instant::now();
    run_back_test_strategy(params_batch, inst_id, time, arc_candle_data, semaphore).await;
    let backtest_duration = backtest_start.elapsed();

    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] test_specified_strategy 完成 - 总耗时: {}ms, 配置获取: {}ms, 回测执行: {}ms",
        total_duration.as_millis(),
        config_get_duration.as_millis(),
        backtest_duration.as_millis()
    );
    Ok(())
}

/// 运行准备好的订单函数 - 使用新的管理器
pub async fn run_ready_to_order_with_manager(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
    snap: Option<CandlesEntity>,
) -> Result<()> {
    // 常量定义
    const MAX_HISTORY_SIZE: usize = 10000;
    // 1. 预处理：获取哈希键和管理器
    let strategy_type = StrategyType::Vegas.as_str().to_owned();
    let key = get_hash_key(inst_id, period, &strategy_type);
    let manager = arc_vegas_indicator_values::get_indicator_manager();
    let mut new_candle_data: Option<CandlesEntity> = None;
    if snap.is_none() {
        // 2. 获取最新K线数据
        new_candle_data = CandleDomainService::new_default()
            .await
            .get_new_one_candle_fresh(inst_id, period, None)
            .await
            .map_err(|e| anyhow!("获取最新K线数据失败: {}", e))?;
    } else {
        //直接从传过来的数据中获取，传过来的参数默认是认为最新的
        new_candle_data = snap;
    }
    if new_candle_data.is_none() {
        warn!(
            "获取的最新K线数据为空,跳过本次策略执行: {:?}, {:?}",
            inst_id, period
        );
        return Ok(()); // 改为返回Ok，避免阻塞策略执行
    }
    let new_candle_data = new_candle_data.unwrap();
    let new_candle_item = parse_candle_to_data_item(&new_candle_data);

    // 3. 同键互斥，读取快照并验证
    let key_mutex = manager.acquire_key_mutex(&key).await;
    let _guard = key_mutex.lock().await;

    /// 获取缓存，快照
    let (mut last_candles_vec, mut old_indicator_combines, old_time) =
        match manager.get_snapshot_last_n(&key, MAX_HISTORY_SIZE).await {
            Some((v, indicators, ts)) => (v, indicators, ts),
            None => {
                return Err(anyhow!("没有找到对应的策略值: {}", key));
            }
        };
    // 转为 VecDeque 以保持原逻辑（并保证后续 push/pop_front 性能）
    let mut new_candle_items: VecDeque<CandleItem> = last_candles_vec.into_iter().collect();

    // 4. 验证时间戳，检查是否有新数据
    let new_time = new_candle_item.ts;
    let is_update = new_candle_item.confirm == 1;

    let is_new_time = check_new_time(old_time, new_time, period, is_update, true)?;
    if !is_new_time {
        info!(
            "跳过策略执行: inst_id:{:?} period:{:?} new_candle_data:{:?}",
            inst_id, period, new_candle_data
        );
        return Ok(());
    }

    // 6. 计算最新指标值
    let new_indicator_values =
        get_multi_indicator_values(&mut old_indicator_combines, &new_candle_item);

    // 5. 准备更新数据
    new_candle_items.push_back(new_candle_item.clone());

    // 限制历史数据大小 - 使用VecDeque的高效操作
    if new_candle_items.len() > MAX_HISTORY_SIZE {
        let excess = new_candle_items.len() - MAX_HISTORY_SIZE;
        for _ in 0..excess {
            new_candle_items.pop_front();
        }
    }

    // 7-8. 原子更新：同时写入K线与指标，避免中间态
    if let Err(e) = manager
        .update_both(
            &key,
            new_candle_items.clone(),
            old_indicator_combines.clone(),
            new_candle_item.ts,
        )
        .await
    {
        return Err(anyhow!("原子更新指标与K线失败: {}", e));
    }

    // 10. 计算交易信号
    // 将VecDeque转换为Vec,为了增加性能和部分场景需要，最后n根k线的情况，取最后N根,并保留原始排序，以供策略使用,
    let candle_vec: Vec<CandleItem> = new_candle_items
        .iter()
        .rev()
        .take(10)
        .cloned()
        .rev()
        .collect();

    // 解析策略配置
    let vegas_strategy: crate::trading::indicator::vegas_indicator::VegasStrategy =
        serde_json::from_str(&strategy.strategy_config)?;
    let signal_result = vegas_strategy.get_trade_signal(
        &candle_vec,
        &mut new_indicator_values.clone(),
        &SignalWeightsConfig::default(),
        &serde_json::from_str::<crate::trading::strategy::strategy_common::BasicRiskStrategyConfig>(
            &strategy.risk_config,
        )?,
    );
    info!(
            "出现买入或者卖出信号！inst_id:{:?} period:{:?},signal_result:should_buy:{},should_sell:{},ts:{}",
            inst_id,
            period,
            signal_result.should_buy,
            signal_result.should_sell,
            new_candle_item.ts
        );
    if signal_result.should_buy || signal_result.should_sell {
        //异步记录日志
        save_signal_log(inst_id, period, &signal_result);
        //执行交易
        let risk_config = strategy.risk_config.clone();
        let res = SwapOrderService::new()
            .ready_to_order(
                &StrategyType::Vegas,
                inst_id,
                period,
                &signal_result,
                &serde_json::from_str::<
                    crate::trading::strategy::strategy_common::BasicRiskStrategyConfig,
                >(&strategy.risk_config)?,
                strategy.strategy_config_id,
            )
            .await;
        match res {
            Ok(_) => {
                println!("执行ready_to_order成功");
            }
            Err(e) => {
                println!("{}", e.to_string())
            }
        }
    } else {
        debug!(
            "signal_result:{:?},ts:{}",
            signal_result,
            new_candle_items.back().unwrap().ts
        );
    }

    // 🧹 **清理执行状态** - 标记策略执行完成
    StrategyExecutionStateManager::mark_completed(&key, new_candle_item.ts);

    Ok(())
}

/// 检查新时间
pub fn check_new_time(
    old_time: i64,
    new_time: i64,
    period: &str,
    is_close_confim: bool,
    just_check_confim: bool,
) -> Result<bool> {
    if (new_time < old_time) {
        return Err(anyhow!(
            "K线时间戳异常: 上一时间戳 {}, 当前时间戳 {}, 预期时间戳 {}",
            old_time,
            new_time,
            period
        ));
    }
    if (is_close_confim) {
        return Ok(true);
    }
    //优先判断
    if old_time == new_time {
        info!("k线时间戳未更新，跳过策略执行: {:?}", period);
        return Ok(false);
    }

    //如果必须要在收盘价确认
    if (just_check_confim && !is_close_confim) {
        info!("k线未确认，跳过策略执行: {:?}", period);
        return Ok(false);
    }
    //TODO 如果不需要收盘价确认
    return Ok(true);
}

/// 保存信号日志
pub fn save_signal_log(inst_id: &str, period: &str, signal_result: &SignalResult) {
    // 异步记录日志（不阻塞下单），并移除 unwrap
    let strategy_result_str = match serde_json::to_string(&signal_result) {
        Ok(s) => s,
        Err(e) => {
            error!("序列化 signal_result 失败: {}", e);
            format!("{:?}", signal_result)
        }
    };
    let signal_record = StrategyJobSignalLog {
        inst_id: inst_id.to_string(),
        time: period.to_string(),
        strategy_type: StrategyType::Vegas.as_str().to_owned(),
        strategy_result: strategy_result_str,
    };
    //启动新线程执行（捕获所有 owned 数据，满足 'static）
    let inst_id_owned = signal_record.inst_id.clone();
    tokio::spawn(async move {
        let res = StrategyJobSignalLogModel::new()
            .await
            .add(signal_record)
            .await;
        if let Err(e) = res {
            error!("写入策略信号日志失败: {}", e);
        } else {
            info!("写入策略信号日志成功: {}", inst_id_owned);
        }
    });
}
