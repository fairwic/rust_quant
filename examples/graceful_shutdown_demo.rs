use rust_quant::app_config::shutdown_manager::{get_shutdown_manager, init_shutdown_manager, ShutdownConfig};
use std::time::Duration;
use tracing::{info, error};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    // 初始化关闭管理器
    let shutdown_config = ShutdownConfig {
        total_timeout: Duration::from_secs(15),
        hook_timeout: Duration::from_secs(5),
        force_exit_on_timeout: true,
    };
    
    let shutdown_manager = init_shutdown_manager(Some(shutdown_config));
    
    // 注册各种关闭回调
    register_shutdown_hooks().await;
    
    // 启动模拟服务
    let service_handle = tokio::spawn(simulate_service(shutdown_manager.shutdown_signal()));
    
    info!("🚀 服务已启动，按 Ctrl+C 退出");
    
    // 等待关闭信号
    let signal_name = shutdown_manager.wait_for_shutdown_signal().await;
    info!("📡 接收到 {} 信号", signal_name);
    
    // 停止服务
    service_handle.abort();
    
    // 执行优雅关闭
    shutdown_manager.shutdown().await?;
    
    info!("✅ 程序已优雅退出");
    Ok(())
}

/// 注册关闭回调函数
async fn register_shutdown_hooks() {
    let manager = get_shutdown_manager();
    
    // 1. 数据库连接清理
    manager.register_shutdown_hook("database_cleanup".to_string(), || async {
        info!("🗄️  清理数据库连接...");
        tokio::time::sleep(Duration::from_millis(500)).await;
        info!("✅ 数据库连接清理完成");
        Ok(())
    }).await;
    
    // 2. 缓存清理
    manager.register_shutdown_hook("cache_cleanup".to_string(), || async {
        info!("🧹 清理缓存...");
        tokio::time::sleep(Duration::from_millis(300)).await;
        info!("✅ 缓存清理完成");
        Ok(())
    }).await;
    
    // 3. 文件句柄清理
    manager.register_shutdown_hook("file_cleanup".to_string(), || async {
        info!("📁 清理文件句柄...");
        tokio::time::sleep(Duration::from_millis(200)).await;
        info!("✅ 文件句柄清理完成");
        Ok(())
    }).await;
    
    // 4. 网络连接清理
    manager.register_shutdown_hook("network_cleanup".to_string(), || async {
        info!("🌐 清理网络连接...");
        tokio::time::sleep(Duration::from_millis(400)).await;
        info!("✅ 网络连接清理完成");
        Ok(())
    }).await;
    
    // 5. 风险清理（示例中改为总是成功，避免引入额外依赖）
    manager.register_shutdown_hook("risky_cleanup".to_string(), || async {
        info!("⚠️  执行风险清理操作...");
        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("✅ 风险清理操作完成");
        Ok(())
    }).await;

    // 6. 模拟一个超时的清理
    manager.register_shutdown_hook("timeout_cleanup".to_string(), || async {
        info!("⏰ 执行可能超时的清理操作...");
        // 故意设置一个较长的延迟来测试超时处理
        tokio::time::sleep(Duration::from_secs(8)).await;
        info!("✅ 超时清理操作完成");
        Ok(())
    }).await;
    
    info!("📋 已注册 6 个关闭回调");
}

/// 模拟服务运行
async fn simulate_service(shutdown_signal: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let mut counter = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    
    loop {
        interval.tick().await;
        
        // 检查关闭信号
        if shutdown_signal.load(std::sync::atomic::Ordering::Acquire) {
            info!("🛑 服务检测到关闭信号，停止运行");
            break;
        }
        
        counter += 1;
        info!("💓 服务心跳 #{}", counter);
        
        // 模拟一些工作
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    info!("🏁 服务已停止");
}
