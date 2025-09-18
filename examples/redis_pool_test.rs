use anyhow::Result;
use tracing::{info, warn};

/// 测试Redis连接池的基本功能（不需要实际Redis服务）
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("开始Redis连接池功能测试...");
    info!("=== Redis连接池功能测试 ===");
    
    // 测试1: 连接池初始化
    test_pool_initialization().await?;
    
    // 测试2: 连接池状态监控
    test_pool_monitoring().await?;

    println!("所有测试完成！");
    info!("所有测试完成！");
    Ok(())
}

/// 测试连接池初始化
async fn test_pool_initialization() -> Result<()> {
    println!("--- 测试连接池初始化 ---");
    info!("--- 测试连接池初始化 ---");
    
    // 设置一个无效的Redis URL来测试错误处理
    std::env::set_var("REDIS_HOST", "redis://invalid-host:6379/");
    std::env::set_var("REDIS_MAX_CONNECTIONS", "10");
    
    match rust_quant::app_config::redis::init_redis_pool().await {
        Ok(_) => {
            warn!("连接池初始化成功（意外，因为使用了无效主机）");
        }
        Err(e) => {
            info!("连接池初始化失败（预期）: {}", e);
            info!("✅ 错误处理正常工作");
        }
    }
    
    // 测试获取连接池实例（应该失败，因为未初始化）
    match rust_quant::app_config::redis::get_redis_pool() {
        Ok(_) => warn!("获取连接池成功（意外）"),
        Err(e) => {
            info!("获取连接池失败（预期）: {}", e);
            info!("✅ 连接池状态检查正常工作");
        }
    }
    
    Ok(())
}

/// 测试连接池监控
async fn test_pool_monitoring() -> Result<()> {
    info!("--- 测试连接池监控 ---");
    
    // 测试监控函数（应该失败，因为连接池未初始化）
    match rust_quant::app_config::redis::monitor_redis_pool().await {
        Ok(status) => {
            warn!("连接池监控成功（意外）: {}", status);
        }
        Err(e) => {
            info!("连接池监控失败（预期）: {}", e);
            info!("✅ 监控功能错误处理正常");
        }
    }
    
    // 测试清理函数
    match rust_quant::app_config::redis::cleanup_redis_pool().await {
        Ok(_) => {
            info!("连接池清理完成");
            info!("✅ 清理功能正常工作");
        }
        Err(e) => {
            warn!("连接池清理失败: {}", e);
        }
    }
    
    Ok(())
}
