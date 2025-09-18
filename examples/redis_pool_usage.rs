use anyhow::Result;
use redis::AsyncCommands;
use rust_quant::app_config::redis::{get_redis_connection, monitor_redis_pool};
use std::time::Instant;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

/// 演示Redis连接池的使用和性能对比
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    info!("开始初始化Redis连接池...");

    // 初始化Redis连接池
    match rust_quant::app_config::redis::init_redis_pool().await {
        Ok(_) => info!("Redis连接池初始化成功"),
        Err(e) => {
            warn!("Redis连接池初始化失败: {}", e);
            info!("请确保Redis服务正在运行，或检查REDIS_HOST环境变量");
            return Ok(());
        }
    }
    
    info!("=== Redis连接池性能测试 ===");
    
    // 测试1: 并发获取连接
    test_concurrent_connections().await?;
    
    // 测试2: 连接池状态监控
    test_pool_monitoring().await?;
    
    // 测试3: 高频操作性能
    test_high_frequency_operations().await?;
    
    Ok(())
}

/// 测试并发连接获取
async fn test_concurrent_connections() -> Result<()> {
    info!("--- 测试并发连接获取 ---");
    
    let start = Instant::now();
    let mut handles = vec![];
    
    // 启动10个并发任务
    for i in 0..10 {
        let handle = tokio::spawn(async move {
            match get_redis_connection().await {
                Ok(mut conn) => {
                    // 执行简单的Redis操作
                    let key = format!("test_key_{}", i);
                    let _: () = conn.set(&key, format!("value_{}", i)).await.unwrap();
                    let value: String = conn.get(&key).await.unwrap();
                    info!("任务 {} 完成，值: {}", i, value);
                }
                Err(e) => warn!("任务 {} 获取连接失败: {}", i, e),
            }
        });
        handles.push(handle);
    }
    
    // 等待所有任务完成
    for handle in handles {
        handle.await?;
    }
    
    let duration = start.elapsed();
    info!("并发连接测试完成，耗时: {:?}", duration);
    
    Ok(())
}

/// 测试连接池监控
async fn test_pool_monitoring() -> Result<()> {
    info!("--- 测试连接池监控 ---");
    
    // 获取几个连接但不立即释放
    let _conn1 = get_redis_connection().await?;
    let _conn2 = get_redis_connection().await?;
    
    // 监控连接池状态
    let status = monitor_redis_pool().await?;
    info!("连接池状态: {}", status);
    
    // 释放连接（通过drop）
    drop(_conn1);
    drop(_conn2);
    
    // 等待一下让连接释放
    sleep(Duration::from_millis(100)).await;
    
    let status = monitor_redis_pool().await?;
    info!("释放后状态: {}", status);
    
    Ok(())
}

/// 测试高频操作性能
async fn test_high_frequency_operations() -> Result<()> {
    info!("--- 测试高频操作性能 ---");
    
    let start = Instant::now();
    let operations = 100;
    
    for i in 0..operations {
        let mut conn = get_redis_connection().await?;
        let key = format!("perf_test_{}", i);
        let _: () = conn.set(&key, i).await?;
        let value: i32 = conn.get(&key).await?;
        assert_eq!(value, i);
    }
    
    let duration = start.elapsed();
    let ops_per_sec = operations as f64 / duration.as_secs_f64();
    
    info!("高频操作测试完成:");
    info!("  操作数: {}", operations);
    info!("  总耗时: {:?}", duration);
    info!("  平均每秒操作数: {:.2}", ops_per_sec);
    
    Ok(())
}
