use rust_quant::app_config::shutdown_manager::{ShutdownConfig, ShutdownManager};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;



#[tokio::test]
async fn test_basic_shutdown() {
    let config = ShutdownConfig {
        total_timeout: Duration::from_secs(5),
        hook_timeout: Duration::from_secs(2),
        force_exit_on_timeout: false,
    };
    
    let manager = ShutdownManager::new(config);
    
    // 注册一个简单的回调
    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();
    
    manager.register_shutdown_hook("test_hook".to_string(), move || {
        let executed = executed_clone.clone();
        async move {
            executed.store(true, Ordering::Release);
            Ok(())
        }
    }).await;
    
    // 执行关闭
    let result = manager.shutdown().await;
    
    assert!(result.is_ok());
    assert!(manager.is_shutting_down());
    assert!(executed.load(Ordering::Acquire));
}


#[tokio::test]
async fn test_shutdown_with_timeout() {
    let config = ShutdownConfig {
        total_timeout: Duration::from_secs(2),
        hook_timeout: Duration::from_secs(1),
        force_exit_on_timeout: false,
    };
    
    let manager = ShutdownManager::new(config);
    
    // 注册一个会超时的回调
    manager.register_shutdown_hook("timeout_hook".to_string(), || async {
        tokio::time::sleep(Duration::from_secs(3)).await;
        Ok(())
    }).await;
    
    // 执行关闭，应该超时
    let result = manager.shutdown().await;
    
    assert!(result.is_err());
    assert!(manager.is_shutting_down());
}


#[tokio::test]
async fn test_shutdown_with_error() {
    let config = ShutdownConfig {
        total_timeout: Duration::from_secs(5),
        hook_timeout: Duration::from_secs(2),
        force_exit_on_timeout: false,
    };
    
    let manager = ShutdownManager::new(config);
    
    let success_executed = Arc::new(AtomicBool::new(false));
    let success_executed_clone = success_executed.clone();
    
    // 注册一个会失败的回调
    manager.register_shutdown_hook("error_hook".to_string(), || async {
        Err(anyhow::anyhow!("测试错误"))
    }).await;
    
    // 注册一个成功的回调
    manager.register_shutdown_hook("success_hook".to_string(), move || {
        let executed = success_executed_clone.clone();
        async move {
            executed.store(true, Ordering::Release);
            Ok(())
        }
    }).await;
    
    // 执行关闭，即使有错误也应该继续
    let result = manager.shutdown().await;
    
    assert!(result.is_ok()); // 整体应该成功，因为我们不中断流程
    assert!(manager.is_shutting_down());
    assert!(success_executed.load(Ordering::Acquire)); // 成功的回调应该执行
}


#[tokio::test]
async fn test_multiple_shutdown_calls() {
    let config = ShutdownConfig::default();
    let manager = ShutdownManager::new(config);
    
    let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();
    
    manager.register_shutdown_hook("counter_hook".to_string(), move || {
        let count = call_count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::Release);
            Ok(())
        }
    }).await;
    
    // 多次调用关闭
    let result1 = manager.shutdown().await;
    let result2 = manager.shutdown().await;
    let result3 = manager.shutdown().await;
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());
    
    // 回调应该只执行一次
    assert_eq!(call_count.load(Ordering::Acquire), 1);
}


#[tokio::test]
async fn test_shutdown_signal_sharing() {
    let manager = ShutdownManager::new_default();
    let shutdown_signal = manager.shutdown_signal();
    
    // 初始状态应该是 false
    assert!(!shutdown_signal.load(Ordering::Acquire));
    assert!(!manager.is_shutting_down());
    
    // 启动一个任务来监控关闭信号
    let signal_clone = shutdown_signal.clone();
    let monitor_task = tokio::spawn(async move {
        while !signal_clone.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        "shutdown_detected"
    });
    
    // 等待一小段时间确保监控任务启动
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // 执行关闭
    let shutdown_task = tokio::spawn(async move {
        manager.shutdown().await
    });
    
    // 等待监控任务检测到关闭信号
    let monitor_result = timeout(Duration::from_secs(2), monitor_task).await;
    assert!(monitor_result.is_ok());
    assert_eq!(monitor_result.unwrap().unwrap(), "shutdown_detected");
    
    // 等待关闭完成
    let shutdown_result = timeout(Duration::from_secs(2), shutdown_task).await;
    assert!(shutdown_result.is_ok());
    assert!(shutdown_result.unwrap().is_ok());
    
    // 最终状态检查
    assert!(shutdown_signal.load(Ordering::Acquire));
}


#[tokio::test]
async fn test_performance_with_many_hooks() {
    let config = ShutdownConfig {
        total_timeout: Duration::from_secs(10),
        hook_timeout: Duration::from_secs(1),
        force_exit_on_timeout: false,
    };
    
    let manager = ShutdownManager::new(config);
    let hook_count = 50;
    let executed_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    
    // 注册多个回调
    for i in 0..hook_count {
        let count = executed_count.clone();
        manager.register_shutdown_hook(format!("hook_{}", i), move || {
            let count = count.clone();
            async move {
                // 模拟一些工作
                tokio::time::sleep(Duration::from_millis(10)).await;
                count.fetch_add(1, Ordering::Release);
                Ok(())
            }
        }).await;
    }
    
    let start_time = std::time::Instant::now();
    let result = manager.shutdown().await;
    let elapsed = start_time.elapsed();
    
    assert!(result.is_ok());
    assert_eq!(executed_count.load(Ordering::Acquire), hook_count);
    assert!(elapsed < Duration::from_secs(5)); // 应该在合理时间内完成
    
    println!("执行 {} 个回调耗时: {:?}", hook_count, elapsed);
}
