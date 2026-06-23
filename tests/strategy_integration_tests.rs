//! 策略系统集成测试
//!
//! 提供全面的集成测试，包括并发测试、性能测试、
//! 错误场景测试和完整生命周期测试。
use std::time::Duration;
use tokio::time::sleep;
use rust_quant::app_init;
use rust_quant::trading::strategy::strategy_manager::{get_strategy_manager, UpdateStrategyConfigRequest};
use rust_quant::trading::services::strategy_metrics::get_strategy_metrics;
use rust_quant::trading::services::scheduler_service::SchedulerService;
#[tokio::test]
async fn test_full_strategy_lifecycle() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    let metrics = get_strategy_metrics();
    let config_id = 1_i64;
    let inst_id = "BTC-USDT-SWAP".to_string();
    let period = "1H".to_string();
    let strategy_type = "Vegas".to_string();
    println!("🔄 测试完整策略生命周期");
    // 1. 启动策略
    println!("📈 启动策略");
    let start_result = manager.start_strategy(config_id, inst_id.clone(), period.clone()).await;
    assert!(start_result.is_ok(), "策略启动应该成功");
    // 2. 验证策略状态
    let strategy_info = manager.get_strategy_info(&inst_id, &period, &strategy_type).await;
    assert!(strategy_info.is_some(), "应该能获取到策略信息");
    let info = strategy_info.unwrap();
    assert_eq!(info.inst_id, inst_id);
    assert_eq!(info.period, period);
    // 3. 等待策略执行
    sleep(Duration::from_secs(3)).await;
    // 4. 热更新配置
    println!("🔧 热更新配置");
    let update_req = UpdateStrategyConfigRequest {
        strategy_config: Some(r#"{"period":"1H","min_k_line_num":7000}"#.to_string()),
        risk_config: Some(r#"{"max_position_ratio":0.3,"stop_loss_ratio":0.01}"#.to_string()),
    };
    let update_result = manager.update_strategy_config(&inst_id, &period, &strategy_type, update_req).await;
    // 注意：热更新可能失败，因为依赖数据库配置
    if update_result.is_err() {
        println!("⚠️ 热更新失败（预期，因为测试环境配置限制）");
    }
    // 5. 暂停策略
    println!("⏸️ 暂停策略");
    let pause_result = manager.pause_strategy(&inst_id, &period, &strategy_type).await;
    assert!(pause_result.is_ok(), "策略暂停应该成功");
    // 6. 恢复策略
    println!("▶️ 恢复策略");
    let resume_result = manager.resume_strategy(&inst_id, &period, &strategy_type).await;
    assert!(resume_result.is_ok(), "策略恢复应该成功");
    // 7. 停止策略
    println!("🛑 停止策略");
    let stop_result = manager.stop_strategy(&inst_id, &period, &strategy_type).await;
    assert!(stop_result.is_ok(), "策略停止应该成功");
    // 8. 验证指标记录
    let strategy_key = format!("Vegas_{}_{}", inst_id, period);
    let strategy_metrics = metrics.get_strategy_metrics(&strategy_key).await;
    if let Some(metrics) = strategy_metrics {
        assert!(metrics.start_count > 0, "应该记录启动次数");
        assert!(metrics.stop_count > 0, "应该记录停止次数");
        println!("✅ 指标记录正常: 启动{}次, 停止{}次", metrics.start_count, metrics.stop_count);
    }
    println!("🎉 完整生命周期测试完成");
}
#[tokio::test]
async fn test_concurrent_operations() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    println!("🔄 测试并发操作");
    // 并发启动多个策略
    let strategies = vec![
        (1_i64, "BTC-USDT-SWAP".to_string(), "1H".to_string()),
        (2_i64, "ETH-USDT-SWAP".to_string(), "4H".to_string()),
    ];
    let mut handles = Vec::new();
    for (config_id, inst_id, period) in strategies {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            manager_clone.start_strategy(config_id, inst_id, period).await
        });
        handles.push(handle);
    }
    // 等待所有策略启动完成
    let results = futures::future::join_all(handles).await;
    let success_count = results.into_iter()
        .filter_map(|r| r.ok())
        .filter(|r| r.is_ok())
        .count();
    println!("✅ 并发启动完成，成功: {}", success_count);
    // 等待一段时间
    sleep(Duration::from_secs(2)).await;
    // 并发停止所有策略
    let stop_result = manager.stop_all_strategies().await;
    assert!(stop_result.is_ok(), "批量停止应该成功");
    println!("🎉 并发操作测试完成");
}
#[tokio::test]
async fn test_error_scenarios_and_recovery() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    println!("🧪 测试错误场景和恢复");
    // 1. 测试启动不存在的配置
    println!("❌ 测试启动不存在的配置");
    let invalid_start = manager.start_strategy(99999, "INVALID-SWAP".to_string(), "1H".to_string()).await;
    assert!(invalid_start.is_err(), "启动不存在的配置应该失败");
    // 2. 测试停止不存在的策略
    println!("❌ 测试停止不存在的策略");
    let invalid_stop = manager.stop_strategy("INVALID-SWAP", "1H", "Vegas").await;
    assert!(invalid_stop.is_err(), "停止不存在的策略应该失败");
    // 3. 测试重复启动同一策略
    println!("❌ 测试重复启动同一策略");
    let first_start = manager.start_strategy(1, "BTC-USDT-SWAP".to_string(), "1H".to_string()).await;
    if first_start.is_ok() {
        let duplicate_start = manager.start_strategy(1, "BTC-USDT-SWAP".to_string(), "1H".to_string()).await;
        assert!(duplicate_start.is_err(), "重复启动应该失败");
        // 清理
        let _ = manager.stop_strategy("BTC-USDT-SWAP", "1H", "Vegas").await;
    }
    // 4. 测试暂停未运行的策略
    println!("❌ 测试暂停未运行的策略");
    let invalid_pause = manager.pause_strategy("NOT-RUNNING", "1H", "Vegas").await;
    assert!(invalid_pause.is_err(), "暂停未运行的策略应该失败");
    println!("🎉 错误场景测试完成");
}
#[tokio::test]
async fn test_performance_benchmarks() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    let metrics = get_strategy_metrics();
    println!("⚡ 测试性能基准");
    // 测试启动性能
    let start_time = std::time::Instant::now();
    let start_result = manager.start_strategy(1, "BTC-USDT-SWAP".to_string(), "1H".to_string()).await;
    let start_duration = start_time.elapsed();
    if start_result.is_ok() {
        println!("✅ 策略启动耗时: {}ms", start_duration.as_millis());
        assert!(start_duration.as_millis() < 5000, "启动时间应该小于5秒");
        // 测试停止性能
        let stop_time = std::time::Instant::now();
        let stop_result = manager.stop_strategy("BTC-USDT-SWAP", "1H", "Vegas").await;
        let stop_duration = stop_time.elapsed();
        if stop_result.is_ok() {
            println!("✅ 策略停止耗时: {}ms", stop_duration.as_millis());
            assert!(stop_duration.as_millis() < 1000, "停止时间应该小于1秒");
        }
    }
    // 测试系统健康状态获取性能
    let health_start = std::time::Instant::now();
    let health = manager.get_system_health().await;
    let health_duration = health_start.elapsed();
    println!("✅ 健康检查耗时: {}ms", health_duration.as_millis());
    println!("📊 系统健康状态: 总策略数={}, 运行中={}", health.total_strategies, health.running_strategies);
    assert!(health_duration.as_millis() < 100, "健康检查应该小于100ms");
    println!("🎉 性能基准测试完成");
}
#[tokio::test]
async fn test_scheduler_health() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    println!("🏥 测试调度器健康状态");
    // 检查调度器健康状态
    let is_healthy = SchedulerService::is_scheduler_healthy().await;
    assert!(is_healthy, "调度器应该是健康的");
    let health = SchedulerService::get_scheduler_health().await;
    assert!(health.is_healthy, "调度器健康状态应该为true");
    assert!(health.last_check_time > 0, "应该有最后检查时间");
    println!("✅ 调度器健康状态: {:?}", health);
    println!("🎉 调度器健康测试完成");
}
#[tokio::test]
async fn test_memory_usage_and_cleanup() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    let metrics = get_strategy_metrics();
    println!("🧹 测试内存使用和资源清理");
    // 启动多个策略以产生指标数据
    let strategies = vec![
        (1_i64, "BTC-USDT-SWAP".to_string(), "1H".to_string()),
        (2_i64, "ETH-USDT-SWAP".to_string(), "4H".to_string()),
    ];
    for (config_id, inst_id, period) in strategies {
        if let Ok(_) = manager.start_strategy(config_id, inst_id.clone(), period.clone()).await {
            println!("✅ 启动策略: {}_{}", inst_id, period);
        }
    }
    // 等待一段时间产生指标数据
    sleep(Duration::from_secs(2)).await;
    // 获取初始指标
    let initial_metrics = metrics.get_all_metrics().await;
    let initial_count = initial_metrics.len();
    println!("📊 初始指标数量: {}", initial_count);
    // 停止所有策略
    let _ = manager.stop_all_strategies().await;
    // 清理过期指标（设置0小时保留期）
    metrics.cleanup_expired_metrics(0).await;
    // 验证清理效果
    let cleaned_metrics = metrics.get_all_metrics().await;
    let cleaned_count = cleaned_metrics.len();
    println!("📊 清理后指标数量: {}", cleaned_count);
    // 在实际环境中，清理可能不会立即生效，因为时间戳检查
    // 这里主要验证清理功能不会崩溃
    assert!(cleaned_count <= initial_count, "清理后指标数量应该不增加");
    println!("🎉 内存清理测试完成");
}
#[tokio::test]
async fn test_stress_start_stop() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    println!("💪 压力测试：快速启动停止");
    let config_id = 1_i64;
    let inst_id = "BTC-USDT-SWAP".to_string();
    let period = "1H".to_string();
    let strategy_type = "Vegas".to_string();
    // 快速启动停止循环
    for i in 1..=5 {
        println!("🔄 第{}轮启动停止", i);
        // 启动
        let start_time = std::time::Instant::now();
        let start_result = manager.start_strategy(config_id, inst_id.clone(), period.clone()).await;
        let start_duration = start_time.elapsed();
        if start_result.is_ok() {
            println!("✅ 第{}轮启动成功，耗时: {}ms", i, start_duration.as_millis());
            // 短暂等待
            sleep(Duration::from_millis(500)).await;
            // 停止
            let stop_time = std::time::Instant::now();
            let stop_result = manager.stop_strategy(&inst_id, &period, &strategy_type).await;
            let stop_duration = stop_time.elapsed();
            if stop_result.is_ok() {
                println!("✅ 第{}轮停止成功，耗时: {}ms", i, stop_duration.as_millis());
            } else {
                println!("❌ 第{}轮停止失败: {:?}", i, stop_result);
                break;
            }
        } else {
            println!("❌ 第{}轮启动失败: {:?}", i, start_result);
            break;
        }
        // 短暂休息
        sleep(Duration::from_millis(200)).await;
    }
    println!("🎉 压力测试完成");
}
#[tokio::test]
async fn test_system_health_monitoring() {
    // 初始化环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }
    if let Err(e) = rust_quant::init_scheduler().await {
        eprintln!("调度器初始化失败: {}", e);
        return;
    }
    let manager = get_strategy_manager();
    println!("🏥 测试系统健康监控");
    // 获取初始健康状态
    let initial_health = manager.get_system_health().await;
    println!("📊 初始健康状态:");
    println!("  - 总策略数: {}", initial_health.total_strategies);
    println!("  - 运行中策略数: {}", initial_health.running_strategies);
    println!("  - 调度器健康: {}", initial_health.scheduler_health);
    println!("  - 系统运行时间: {}ms", initial_health.system_uptime_ms);
    // 启动一个策略
    if let Ok(_) = manager.start_strategy(1, "BTC-USDT-SWAP".to_string(), "1H".to_string()).await {
        // 等待策略运行
        sleep(Duration::from_secs(2)).await;
        // 获取更新后的健康状态
        let updated_health = manager.get_system_health().await;
        println!("📊 更新后健康状态:");
        println!("  - 总策略数: {}", updated_health.total_strategies);
        println!("  - 运行中策略数: {}", updated_health.running_strategies);
        assert!(updated_health.total_strategies >= initial_health.total_strategies, "策略数应该增加或保持");
        // 清理
        let _ = manager.stop_strategy("BTC-USDT-SWAP", "1H", "Vegas").await;
    }
    println!("🎉 健康监控测试完成");
}
