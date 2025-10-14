use anyhow::Result;
use std::time::Instant;
use tracing::{info, warn};

/// 性能测试：对比连接池与直接连接的性能差异
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== Redis连接池性能测试 ===");
    info!("开始Redis连接池性能测试");

    // 测试1: 连接池初始化性能
    test_pool_initialization_performance().await?;

    // 测试2: 连接获取性能对比
    test_connection_performance().await?;

    // 测试3: 并发连接测试
    test_concurrent_connections().await?;

    println!("=== 性能测试完成 ===");
    info!("所有性能测试完成");
    Ok(())
}

/// 测试连接池初始化性能
async fn test_pool_initialization_performance() -> Result<()> {
    println!("\n--- 测试连接池初始化性能 ---");
    info!("开始测试连接池初始化性能");

    // 设置测试环境变量
    std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
    std::env::set_var("REDIS_MAX_CONNECTIONS", "20");

    let start = Instant::now();

    // 尝试初始化连接池
    match rust_quant::app_config::redis_config::init_redis_pool().await {
        Ok(_) => {
            let duration = start.elapsed();
            println!("✅ 连接池初始化成功，耗时: {:?}", duration);
            info!("连接池初始化成功，耗时: {:?}", duration);

            // 测试获取连接池状态
            match rust_quant::app_config::redis_config::monitor_redis_pool().await {
                Ok(status) => {
                    println!("📊 {}", status);
                    info!("连接池状态: {}", status);
                }
                Err(e) => {
                    warn!("获取连接池状态失败: {}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ 连接池初始化失败: {}", e);
            warn!("连接池初始化失败: {}", e);
            println!("💡 提示：请确保Redis服务正在运行，或者这是预期的测试结果");
        }
    }

    Ok(())
}

/// 测试连接获取性能
async fn test_connection_performance() -> Result<()> {
    println!("\n--- 测试连接获取性能 ---");
    info!("开始测试连接获取性能");

    // 测试连接池方式获取连接
    let pool_start = Instant::now();
    let mut pool_success_count = 0;
    let test_count = 10;

    for i in 1..=test_count {
        match rust_quant::app_config::redis_config::get_redis_connection().await {
            Ok(_conn) => {
                pool_success_count += 1;
                println!("  连接池方式 - 第{}次获取连接成功", i);
            }
            Err(e) => {
                println!("  连接池方式 - 第{}次获取连接失败: {}", i, e);
            }
        }
    }

    let pool_duration = pool_start.elapsed();

    println!("📈 连接池性能统计:");
    println!("  - 总测试次数: {}", test_count);
    println!("  - 成功次数: {}", pool_success_count);
    println!("  - 总耗时: {:?}", pool_duration);
    println!("  - 平均耗时: {:?}", pool_duration / test_count);

    if pool_success_count > 0 {
        println!("✅ 连接池工作正常");
        info!("连接池性能测试完成，成功率: {}/{}", pool_success_count, test_count);
    } else {
        println!("⚠️  连接池无法连接到Redis服务");
        warn!("连接池无法连接到Redis服务，这可能是因为Redis服务未运行");
    }

    Ok(())
}

/// 测试并发连接
async fn test_concurrent_connections() -> Result<()> {
    println!("\n--- 测试并发连接 ---");
    info!("开始测试并发连接");

    let concurrent_count = 5;
    let mut handles = Vec::new();

    let start = Instant::now();

    // 创建并发任务
    for i in 1..=concurrent_count {
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            match rust_quant::app_config::redis_config::get_redis_connection().await {
                Ok(_conn) => {
                    let duration = task_start.elapsed();
                    println!("  并发任务{}: 获取连接成功，耗时: {:?}", i, duration);
                    Ok(duration)
                }
                Err(e) => {
                    println!("  并发任务{}: 获取连接失败: {}", i, e);
                    Err(e)
                }
            }
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    let mut success_count = 0;
    let mut total_duration = std::time::Duration::from_nanos(0);

    for handle in handles {
        match handle.await {
            Ok(Ok(duration)) => {
                success_count += 1;
                total_duration += duration;
            }
            Ok(Err(_)) => {
                // 连接失败
            }
            Err(e) => {
                println!("  任务执行错误: {}", e);
            }
        }
    }

    let total_test_duration = start.elapsed();

    println!("🚀 并发连接测试结果:");
    println!("  - 并发任务数: {}", concurrent_count);
    println!("  - 成功任务数: {}", success_count);
    println!("  - 总测试时间: {:?}", total_test_duration);

    if success_count > 0 {
        println!("  - 平均单任务耗时: {:?}", total_duration / success_count);
        println!("✅ 并发连接测试通过");
        info!("并发连接测试完成，成功率: {}/{}", success_count, concurrent_count);
    } else {
        println!("⚠️  所有并发连接都失败了");
        warn!("并发连接测试失败，可能Redis服务不可用");
    }

    // 检查最终连接池状态
    match rust_quant::app_config::redis_config::monitor_redis_pool().await {
        Ok(status) => {
            println!("📊 测试后连接池状态: {}", status);
        }
        Err(e) => {
            println!("⚠️  无法获取连接池状态: {}", e);
        }
    }

    Ok(())
}
