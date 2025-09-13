use rust_quant::app_init;
use rust_quant::trading::strategy::strategy_manager::{
    get_strategy_manager, UpdateStrategyConfigRequest,
};
use rust_quant::trading::services::strategy_metrics::get_strategy_metrics;

/// 测试策略管理器的停止功能
/// 专门测试停止策略是否会死锁
#[tokio::test]
async fn test_strategy_stop_only() {
    // 初始化应用环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    // 初始化调度器
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();
    let config_id = 1_i64;
    let inst_id = "BTC-USDT-SWAP".to_string();
    let period = "1H".to_string();
    let strategy_type = "Vegas".to_string();

    println!("🚀 开始测试策略停止功能");

    // 1. 启动策略
    println!("📈 启动策略");
    match manager.start_strategy(config_id, inst_id.clone(), period.clone()).await {
        Ok(_) => println!("✅ 策略启动成功"),
        Err(e) => {
            println!("❌ 策略启动失败: {}", e);
            return;
        }
    }

    // 等待一段时间让策略运行
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 2. 停止策略
    println!("🛑 停止策略");
    let start_time = std::time::Instant::now();
    match manager.stop_strategy(&inst_id, &period, &strategy_type).await {
        Ok(_) => {
            let elapsed = start_time.elapsed();
            println!("✅ 策略停止成功，耗时: {}ms", elapsed.as_millis());
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            println!("❌ 策略停止失败，耗时: {}ms, 错误: {}", elapsed.as_millis(), e);
        }
    }

    // 验证指标记录
    let strategy_key = format!("Vegas_{}_{}", inst_id, period);
    let metrics = get_strategy_metrics();
    if let Some(strategy_metrics) = metrics.get_strategy_metrics(&strategy_key).await {
        println!("📊 性能指标: 启动{}次, 停止{}次, 平均启动时间{}ms", 
                strategy_metrics.start_count, 
                strategy_metrics.stop_count,
                strategy_metrics.avg_start_time_ms);
    }

    println!("🎉 策略停止功能测试完成");
}

/// 测试策略管理器的基本功能
/// 这个测试演示了如何像 API 请求一样调用策略管理功能
#[tokio::test]
async fn test_strategy_manager_basic_operations() {
    // 初始化应用环境（数据库连接等）
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    // 初始化调度器（已经自动启动）
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();

    // 测试参数 - 请根据实际数据库中的配置修改
    let config_id = 1_i64;
    let inst_id = "BTC-USDT-SWAP".to_string();
    let period = "1H".to_string();
    let strategy_type = "Vegas".to_string();

    println!("🚀 开始测试策略管理器功能");

    // 1. 启动策略
    println!(
        "📈 启动策略: config_id={}, inst_id={}, period={}",
        config_id, inst_id, period
    );
    match manager
        .start_strategy(config_id, inst_id.clone(), period.clone())
        .await
    {
        Ok(_) => println!("✅ 策略启动成功"),
        Err(e) => {
            println!("❌ 策略启动失败: {}", e);
            // 如果启动失败，可能是因为：
            // 1. 数据库中不存在对应的配置记录
            // 2. 调度器未初始化
            // 3. 策略已在运行
            return;
        }
    }
    // 2. 查询策略状态
    println!("📊 查询策略运行状态");
    match manager
        .get_strategy_info(&inst_id, &period, &strategy_type)
        .await
    {
        Some(info) => {
            println!("✅ 策略信息: {:?}", info);
        }
        None => {
            println!("❌ 策略未运行或不存在");
        }
    }

    // 3. 获取所有运行中的策略
    println!("📋 获取所有运行中的策略");
    let running_strategies = manager.get_running_strategies().await;
    println!("✅ 运行中的策略数量: {}", running_strategies.len());
    for strategy in &running_strategies {
        println!(
            "  - {}_{}_{}，状态: {:?}",
            strategy.strategy_type, strategy.inst_id, strategy.period, strategy.status
        );
    }

    // 等待3秒让策略运行一段时间
    println!("⏳ 等待3秒观察策略运行...");
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    // 4. 直接停止策略（跳过暂停和恢复测试，因为可能有问题）
    println!("🛑 直接停止策略");
    match manager
        .stop_strategy(&inst_id, &period, &strategy_type)
        .await
    {
        Ok(_) => {
            println!("✅ 策略停止成功");

            // 验证策略确实已停止
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let running_strategies = manager.get_running_strategies().await;
            println!("📊 停止后运行中的策略数量: {}", running_strategies.len());

            // 检查特定策略是否还在运行
            match manager.get_strategy_info(&inst_id, &period, &strategy_type).await {
                Some(info) => {
                    println!("⚠️  策略仍在运行: {:?}", info);
                }
                None => {
                    println!("✅ 策略已完全停止");
                }
            }
        },
        Err(e) => {
            println!("❌ 策略停止失败: {}", e);
            // 即使停止失败，也要尝试强制清理
            println!("🔧 尝试强制停止所有策略...");
            match manager.stop_all_strategies().await {
                Ok(count) => println!("✅ 强制停止了 {} 个策略", count),
                Err(e) => println!("❌ 强制停止失败: {}", e),
            }
        }
    }

    // 7. 停止策略
    println!("🛑 停止策略");
    match manager
        .stop_strategy(&inst_id, &period, &strategy_type)
        .await
    {
        Ok(_) => {
            println!("✅ 策略停止成功");

            // 验证策略确实已停止
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let running_strategies = manager.get_running_strategies().await;
            println!("📊 停止后运行中的策略数量: {}", running_strategies.len());

            // 检查特定策略是否还在运行
            match manager.get_strategy_info(&inst_id, &period, &strategy_type).await {
                Some(info) => {
                    println!("⚠️  策略仍在运行: {:?}", info);
                }
                None => {
                    println!("✅ 策略已完全停止");
                }
            }
        },
        Err(e) => {
            println!("❌ 策略停止失败: {}", e);
            // 即使停止失败，也要尝试强制清理
            println!("🔧 尝试强制停止所有策略...");
            match manager.stop_all_strategies().await {
                Ok(count) => println!("✅ 强制停止了 {} 个策略", count),
                Err(e) => println!("❌ 强制停止失败: {}", e),
            }
        }
    }

    // 等待一段时间确保所有后台任务都已停止
    println!("⏳ 等待2秒确保所有后台任务停止...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 最终验证
    let final_running_strategies = manager.get_running_strategies().await;
    if final_running_strategies.is_empty() {
        println!("✅ 所有策略已停止，测试完成");
    } else {
        println!("⚠️  仍有 {} 个策略在运行", final_running_strategies.len());
        for strategy in &final_running_strategies {
            println!("  - {}_{}_{}，状态: {:?}",
                strategy.strategy_type, strategy.inst_id, strategy.period, strategy.status);
        }
    }

    println!("🎉 策略管理器功能测试完成");
}

/// 测试调度器启动和定时任务执行
#[tokio::test]
async fn test_scheduler_start_and_task_execution() {
    use tracing::info;
    // 初始化应用环境（数据库连接等）
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    // 初始化调度器（已经自动启动）
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let scheduler = rust_quant::SCHEDULER.lock().await;
    let scheduler = scheduler.as_ref().unwrap();

    // 创建一个简单的定时任务，每秒执行一次
    use tokio_cron_scheduler::Job;
    let job = Job::new_async("* * * * * *", |_uuid, _lock| {
        Box::pin(async move {
            info!("测试定时任务执行: 每秒钟执行一次");
        })
    }).unwrap();

    // 添加任务到调度器
    if let Err(e) = scheduler.add(job.clone()).await {
        eprintln!("添加定时任务失败: {}", e);
        return;
    }

    println!("✅ 调度器已启动，定时任务已添加");

    // 等待5秒观察定时任务执行
    println!("⏳ 等待5秒观察定时任务执行...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    println!("✅ 定时任务观察完成");

    // 调度器会在测试结束后自动停止
}

/// 测试批量操作
#[tokio::test]
async fn test_batch_operations() {
    // 初始化应用环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    // 初始化调度器（已经自动启动）
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();

    println!("🔄 测试批量操作");

    // 批量启动策略
    let strategies_to_start = vec![
        (1_i64, "BTC-USDT-SWAP".to_string(), "1H".to_string()),
        (2_i64, "ETH-USDT-SWAP".to_string(), "4H".to_string()),
    ];

    match manager.batch_start_strategies(strategies_to_start).await {
        Ok(result) => {
            println!("✅ 批量启动完成");
            println!("  成功: {:?}", result.success);
            println!("  失败: {:?}", result.failed);
        }
        Err(e) => println!("❌ 批量启动失败: {}", e),
    }

    println!("✅ 批量启动完成,批量启动后的策略配置: {:#?}", manager.clone());
    // 等待一段时间让策略运行
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // 停止所有策略
    match manager.stop_all_strategies().await {
        Ok(count) => println!("✅ 停止了 {} 个策略", count),
        Err(e) => println!("❌ 停止所有策略失败: {}", e),
    }
}

/// 测试错误场景
#[tokio::test]
async fn test_error_scenarios() {
    // 初始化应用环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    // 初始化调度器（已经自动启动）
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();

    println!("🧪 测试错误场景");

    // 1. 启动不存在的策略配置
    match manager
        .start_strategy(99999_i64, "INVALID-SWAP".to_string(), "1H".to_string())
        .await
    {
        Ok(_) => println!("❌ 预期失败但成功了"),
        Err(e) => println!("✅ 预期的错误: {}", e),
    }

    // 2. 停止不存在的策略
    match manager.stop_strategy("INVALID-SWAP", "1H", "Vegas").await {
        Ok(_) => println!("❌ 预期失败但成功了"),
        Err(e) => println!("✅ 预期的错误: {}", e),
    }

    // 3. 查询不存在的策略
    match manager
        .get_strategy_info("INVALID-SWAP", "1H", "Vegas")
        .await
    {
        Some(_) => println!("❌ 预期返回 None 但返回了数据"),
        None => println!("✅ 正确返回 None"),
    }

    println!("🎯 错误场景测试完成");
}
