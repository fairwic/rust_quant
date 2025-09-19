use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::{
    get_indicator_manager, get_hash_key, IndicatorValuesManager
};
use rust_quant::trading::strategy::strategy_manager::get_strategy_manager;
use rust_quant::trading::services::candle_service::candle_service::CandleService;
use okx::dto::market_dto::CandleOkxRespDto;
use tracing::{info, warn};
use std::env;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// 测试多产品多周期并发策略执行的锁等待情况
#[tokio::test]
async fn test_concurrent_strategy_execution_lock_contention() {
    // 设置环境变量
    env::set_var("APP_ENV", "local");
    
    // 初始化日志
    setup_logging().await.expect("Failed to setup logging");
    
    info!("🧪 开始测试多产品多周期并发策略执行的锁等待情况");
    
    // 创建测试场景
    let test_scenarios = vec![
        // 场景1: 不同产品不同周期 - 应该无锁竞争
        ("BTC-USDT-SWAP", "1m", "场景1-BTC-1m"),
        ("ETH-USDT-SWAP", "5m", "场景1-ETH-5m"),
        ("SOL-USDT-SWAP", "15m", "场景1-SOL-15m"),
        
        // 场景2: 相同产品不同周期 - 应该无锁竞争
        ("BTC-USDT-SWAP", "1m", "场景2-BTC-1m"),
        ("BTC-USDT-SWAP", "5m", "场景2-BTC-5m"),
        ("BTC-USDT-SWAP", "15m", "场景2-BTC-15m"),
        
        // 场景3: 相同产品相同周期 - 会有锁竞争
        ("BTC-USDT-SWAP", "1m", "场景3-重复1"),
        ("BTC-USDT-SWAP", "1m", "场景3-重复2"),
        ("BTC-USDT-SWAP", "1m", "场景3-重复3"),
    ];
    
    // 获取指标管理器并检查key生成
    let manager = get_indicator_manager();
    info!("📊 检查不同场景的key生成:");
    
    for (inst_id, period, scenario) in &test_scenarios {
        let key = get_hash_key(inst_id, period, "Vegas");
        info!("  {} -> key: {}", scenario, key);
    }
    
    // 模拟并发K线确认触发
    info!("🚀 开始并发执行测试...");
    
    let mut handles = vec![];
    let start_time = Instant::now();
    
    for (inst_id, period, scenario) in test_scenarios {
        let inst_id = inst_id.to_string();
        let period = period.to_string();
        let scenario = scenario.to_string();
        
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            
            // 模拟策略执行中的关键锁获取步骤
            let key = get_hash_key(&inst_id, &period, "Vegas");
            let manager = get_indicator_manager();
            
            info!("🔄 {} 开始获取锁: key={}", scenario, key);
            
            // 获取key专用的互斥锁
            let key_mutex = manager.acquire_key_mutex(&key).await;
            let lock_acquire_time = task_start.elapsed();
            
            info!("🔒 {} 获取锁成功，耗时: {:?}", scenario, lock_acquire_time);
            
            // 模拟持有锁期间的处理时间
            let _guard = key_mutex.lock().await;
            let lock_held_start = Instant::now();
            
            // 模拟策略计算时间 (50-200ms)
            let processing_time = Duration::from_millis(50 + (scenario.len() as u64 * 10));
            sleep(processing_time).await;
            
            let total_time = task_start.elapsed();
            let lock_held_time = lock_held_start.elapsed();
            
            info!(
                "✅ {} 完成执行 - 总耗时: {:?}, 锁获取耗时: {:?}, 锁持有时间: {:?}",
                scenario, total_time, lock_acquire_time, lock_held_time
            );
            
            (scenario, total_time, lock_acquire_time, lock_held_time)
        });
        
        handles.push(handle);
        
        // 稍微错开启动时间，模拟真实场景
        sleep(Duration::from_millis(10)).await;
    }
    
    // 等待所有任务完成
    let mut results = vec![];
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }
    
    let total_test_time = start_time.elapsed();
    
    // 分析结果
    info!("📈 并发执行结果分析:");
    info!("  总测试时间: {:?}", total_test_time);
    
    // 按场景分组分析
    let mut scenario1_times = vec![];
    let mut scenario2_times = vec![];
    let mut scenario3_times = vec![];
    
    for (scenario, total_time, lock_acquire_time, _lock_held_time) in &results {
        if scenario.starts_with("场景1") {
            scenario1_times.push(*lock_acquire_time);
        } else if scenario.starts_with("场景2") {
            scenario2_times.push(*lock_acquire_time);
        } else if scenario.starts_with("场景3") {
            scenario3_times.push(*lock_acquire_time);
        }
        
        info!("  {}: 总耗时={:?}, 锁获取耗时={:?}", scenario, total_time, lock_acquire_time);
    }
    
    // 计算平均锁获取时间
    let avg_scenario1 = avg_duration(&scenario1_times);
    let avg_scenario2 = avg_duration(&scenario2_times);
    let avg_scenario3 = avg_duration(&scenario3_times);
    
    info!("🎯 锁竞争分析结果:");
    info!("  场景1 (不同产品不同周期) 平均锁获取时间: {:?}", avg_scenario1);
    info!("  场景2 (相同产品不同周期) 平均锁获取时间: {:?}", avg_scenario2);
    info!("  场景3 (相同产品相同周期) 平均锁获取时间: {:?}", avg_scenario3);
    
    // 验证预期
    if avg_scenario3 > avg_scenario1 && avg_scenario3 > avg_scenario2 {
        info!("✅ 验证通过: 相同key的场景确实存在更长的锁等待时间");
    } else {
        warn!("⚠️  验证结果: 锁等待时间差异不明显，可能需要更高的并发压力");
    }
    
    info!("🎉 并发策略执行锁等待测试完成!");
}

/// 计算平均持续时间
fn avg_duration(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::from_millis(0);
    }
    
    let total_nanos: u128 = durations.iter().map(|d| d.as_nanos()).sum();
    let avg_nanos = total_nanos / durations.len() as u128;
    Duration::from_nanos(avg_nanos as u64)
}

/// 测试高并发场景下的锁竞争
#[tokio::test]
async fn test_high_concurrency_lock_contention() {
    // 设置环境变量
    env::set_var("APP_ENV", "local");
    
    // 初始化日志
    setup_logging().await.expect("Failed to setup logging");
    
    info!("🧪 开始测试高并发场景下的锁竞争");
    
    let concurrent_count = 20;
    let inst_id = "BTC-USDT-SWAP";
    let period = "1m";
    
    info!("🚀 启动 {} 个并发任务，都使用相同的 key", concurrent_count);
    
    let mut handles = vec![];
    let start_time = Instant::now();
    
    for i in 0..concurrent_count {
        let task_id = format!("Task-{:02}", i + 1);
        let inst_id = inst_id.to_string();
        let period = period.to_string();
        
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            
            let key = get_hash_key(&inst_id, &period, "Vegas");
            let manager = get_indicator_manager();
            
            // 获取锁
            let key_mutex = manager.acquire_key_mutex(&key).await;
            let lock_acquire_time = task_start.elapsed();
            
            let _guard = key_mutex.lock().await;
            let lock_obtained_time = task_start.elapsed();
            
            // 模拟处理时间
            sleep(Duration::from_millis(20)).await;
            
            let total_time = task_start.elapsed();
            
            info!(
                "📊 {} 完成 - 锁获取: {:?}, 锁等待: {:?}, 总耗时: {:?}",
                task_id,
                lock_acquire_time,
                lock_obtained_time - lock_acquire_time,
                total_time
            );
            
            (task_id, lock_acquire_time, lock_obtained_time - lock_acquire_time, total_time)
        });
        
        handles.push(handle);
    }
    
    // 等待所有任务完成
    let mut results = vec![];
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }
    
    let total_test_time = start_time.elapsed();
    
    // 分析结果
    let lock_wait_times: Vec<Duration> = results.iter()
        .map(|(_, _, wait_time, _)| *wait_time)
        .collect();
    
    let default_duration = Duration::from_millis(0);
    let max_wait = lock_wait_times.iter().max().unwrap_or(&default_duration);
    let min_wait = lock_wait_times.iter().min().unwrap_or(&default_duration);
    let avg_wait = avg_duration(&lock_wait_times);
    
    info!("📈 高并发锁竞争分析结果:");
    info!("  总测试时间: {:?}", total_test_time);
    info!("  最大锁等待时间: {:?}", max_wait);
    info!("  最小锁等待时间: {:?}", min_wait);
    info!("  平均锁等待时间: {:?}", avg_wait);
    
    if *max_wait > Duration::from_millis(100) {
        warn!("⚠️  检测到较长的锁等待时间: {:?}", max_wait);
    } else {
        info!("✅ 锁等待时间在可接受范围内");
    }
    
    info!("🎉 高并发锁竞争测试完成!");
}

/// 测试时间戳去重机制
#[tokio::test]
async fn test_timestamp_deduplication() {
    use rust_quant::trading::task::strategy_runner::StrategyExecutionStateManager;

    // 设置环境变量
    env::set_var("APP_ENV", "local");

    // 初始化日志
    setup_logging().await.expect("Failed to setup logging");

    info!("🧪 开始测试时间戳去重机制");

    let key = "BTC-USDT-SWAP 1m Vegas";
    let timestamp = 1700000000000i64;

    // 测试1: 首次处理应该成功
    let result1 = StrategyExecutionStateManager::try_mark_processing(key, timestamp);
    assert!(result1, "首次处理应该返回 true");
    info!("✅ 首次处理标记成功");

    // 测试2: 重复处理应该被拒绝
    let result2 = StrategyExecutionStateManager::try_mark_processing(key, timestamp);
    assert!(!result2, "重复处理应该返回 false");
    info!("✅ 重复处理被正确拒绝");

    // 测试3: 不同时间戳应该可以处理
    let timestamp2 = timestamp + 60000; // 1分钟后
    let result3 = StrategyExecutionStateManager::try_mark_processing(key, timestamp2);
    assert!(result3, "不同时间戳应该可以处理");
    info!("✅ 不同时间戳处理成功");

    // 测试4: 不同key应该可以处理
    let key2 = "ETH-USDT-SWAP 1m Vegas";
    let result4 = StrategyExecutionStateManager::try_mark_processing(key2, timestamp);
    assert!(result4, "不同key应该可以处理");
    info!("✅ 不同key处理成功");

    // 测试5: 完成处理后应该可以重新处理
    StrategyExecutionStateManager::mark_completed(key, timestamp);
    let result5 = StrategyExecutionStateManager::try_mark_processing(key, timestamp);
    assert!(result5, "完成处理后应该可以重新处理");
    info!("✅ 完成处理后重新处理成功");

    // 测试6: 获取统计信息
    let (count, keys) = StrategyExecutionStateManager::get_stats();
    info!("📊 当前处理状态统计: 数量={}, keys={:?}", count, keys);
    assert!(count >= 3, "应该有至少3个处理状态");

    // 清理
    StrategyExecutionStateManager::mark_completed(key, timestamp);
    StrategyExecutionStateManager::mark_completed(key, timestamp2);
    StrategyExecutionStateManager::mark_completed(key2, timestamp);

    info!("🎉 时间戳去重机制测试完成!");
}

/// 测试优化后的并发策略执行性能
#[tokio::test]
async fn test_optimized_concurrent_strategy_execution() {
    use rust_quant::trading::task::strategy_runner::StrategyExecutionStateManager;

    // 设置环境变量
    env::set_var("APP_ENV", "local");

    // 初始化日志
    setup_logging().await.expect("Failed to setup logging");

    info!("🧪 开始测试优化后的并发策略执行性能");

    let concurrent_count = 10;
    let inst_id = "BTC-USDT-SWAP";
    let period = "1m";
    let timestamp = 1700000000000i64;

    info!("🚀 启动 {} 个并发任务，都使用相同的 key 和时间戳", concurrent_count);

    let mut handles = vec![];
    let start_time = Instant::now();

    for i in 0..concurrent_count {
        let task_id = format!("OptimizedTask-{:02}", i + 1);
        let inst_id = inst_id.to_string();
        let period = period.to_string();

        let handle = tokio::spawn(async move {
            let task_start = Instant::now();

            // 模拟策略执行中的时间戳去重检查
            let key = format!("{} {} Vegas", inst_id, period);

            // 尝试标记处理状态
            let can_process = StrategyExecutionStateManager::try_mark_processing(&key, timestamp);

            if can_process {
                info!("🔄 {} 开始处理策略", task_id);

                // 模拟策略处理时间
                sleep(Duration::from_millis(50)).await;

                // 标记完成
                StrategyExecutionStateManager::mark_completed(&key, timestamp);

                let total_time = task_start.elapsed();
                info!("✅ {} 处理完成 - 耗时: {:?}", task_id, total_time);

                (task_id, true, total_time)
            } else {
                let total_time = task_start.elapsed();
                info!("⏭️  {} 跳过重复处理 - 耗时: {:?}", task_id, total_time);

                (task_id, false, total_time)
            }
        });

        handles.push(handle);
    }

    // 等待所有任务完成
    let mut results = vec![];
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }

    let total_test_time = start_time.elapsed();

    // 分析结果
    let processed_count = results.iter().filter(|(_, processed, _)| *processed).count();
    let skipped_count = results.len() - processed_count;

    let processed_times: Vec<Duration> = results.iter()
        .filter(|(_, processed, _)| *processed)
        .map(|(_, _, time)| *time)
        .collect();

    let skipped_times: Vec<Duration> = results.iter()
        .filter(|(_, processed, _)| !*processed)
        .map(|(_, _, time)| *time)
        .collect();

    info!("📈 优化后的并发执行结果分析:");
    info!("  总测试时间: {:?}", total_test_time);
    info!("  处理任务数: {}", processed_count);
    info!("  跳过任务数: {}", skipped_count);

    if !processed_times.is_empty() {
        let avg_processed_time = avg_duration(&processed_times);
        info!("  平均处理时间: {:?}", avg_processed_time);
    }

    if !skipped_times.is_empty() {
        let avg_skipped_time = avg_duration(&skipped_times);
        info!("  平均跳过时间: {:?}", avg_skipped_time);
    }

    // 验证优化效果
    assert_eq!(processed_count, 1, "应该只有1个任务被处理");
    assert_eq!(skipped_count, concurrent_count - 1, "其他任务应该被跳过");

    // 验证总时间大幅减少（相比之前的串行执行）
    assert!(total_test_time < Duration::from_millis(200),
        "总时间应该大幅减少，实际: {:?}", total_test_time);

    info!("✅ 优化验证通过: 只处理了1个任务，其他{}个任务被正确跳过", skipped_count);
    info!("🎉 优化后的并发策略执行性能测试完成!");
}
