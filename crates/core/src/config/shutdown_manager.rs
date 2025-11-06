use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use anyhow::Result;

/// 优雅停止管理器 - 标准实现
pub struct ShutdownManager {
    /// 是否正在关闭
    is_shutting_down: Arc<AtomicBool>,
    /// 关闭回调函数列表
    shutdown_hooks: Arc<RwLock<Vec<ShutdownHook>>>,
    /// 配置
    config: ShutdownConfig,
}

/// 关闭回调函数
pub type ShutdownHook = Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>;

/// 关闭配置
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// 总超时时间
    pub total_timeout: Duration,
    /// 每个钩子的超时时间
    pub hook_timeout: Duration,
    /// 是否强制退出
    pub force_exit_on_timeout: bool,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            total_timeout: Duration::from_secs(30),
            hook_timeout: Duration::from_secs(10),
            force_exit_on_timeout: true,
        }
    }
}

impl ShutdownManager {
    /// 创建新的关闭管理器
    pub fn new(config: ShutdownConfig) -> Self {
        Self {
            is_shutting_down: Arc::new(AtomicBool::new(false)),
            shutdown_hooks: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }

    /// 创建默认配置的关闭管理器
    pub fn new_default() -> Self {
        Self::new(ShutdownConfig::default())
    }

    /// 检查是否正在关闭
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::Acquire)
    }

    /// 获取关闭状态的原子引用（用于在其他地方检查）
    pub fn shutdown_signal(&self) -> Arc<AtomicBool> {
        self.is_shutting_down.clone()
    }

    /// 注册关闭回调
    pub async fn register_shutdown_hook<F, Fut>(&self, name: String, hook: F) 
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let boxed_hook: ShutdownHook = Box::new(move || {
            Box::pin(hook())
        });
        
        let mut hooks = self.shutdown_hooks.write().await;
        hooks.push(boxed_hook);
        info!("注册关闭回调: {}", name);
    }

    /// 执行优雅关闭
    pub async fn shutdown(&self) -> Result<()> {
        // 设置关闭标志
        if self.is_shutting_down.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            warn!("关闭已在进行中");
            return Ok(());
        }

        info!("开始执行优雅关闭，总超时: {:?}", self.config.total_timeout);
        let start_time = std::time::Instant::now();

        // 使用总超时包装整个关闭过程
        let shutdown_result = tokio::time::timeout(self.config.total_timeout, self.execute_shutdown_hooks()).await;

        match shutdown_result {
            Ok(Ok(())) => {
                let elapsed = start_time.elapsed();
                info!("优雅关闭完成，耗时: {:?}", elapsed);
                Ok(())
            }
            Ok(Err(e)) => {
                error!("关闭过程中发生错误: {}", e);
                if self.config.force_exit_on_timeout {
                    error!("强制退出程序");
                    std::process::exit(1);
                }
                Err(e)
            }
            Err(_) => {
                error!("关闭超时 ({:?})，强制退出", self.config.total_timeout);
                if self.config.force_exit_on_timeout {
                    std::process::exit(1);
                }
                Err(anyhow::anyhow!("关闭超时"))
            }
        }
    }

    /// 执行所有关闭回调
    async fn execute_shutdown_hooks(&self) -> Result<()> {
        let hooks = self.shutdown_hooks.read().await;
        let hook_count = hooks.len();
        
        if hook_count == 0 {
            info!("没有注册的关闭回调");
            return Ok(());
        }

        info!("执行 {} 个关闭回调", hook_count);

        for (index, hook) in hooks.iter().enumerate() {
            let hook_start = std::time::Instant::now();
            
            info!("执行关闭回调 {}/{}", index + 1, hook_count);
            
            // 为每个钩子设置超时
            let hook_result = tokio::time::timeout(self.config.hook_timeout, hook()).await;
            
            match hook_result {
                Ok(Ok(())) => {
                    let elapsed = hook_start.elapsed();
                    info!("关闭回调 {}/{} 完成，耗时: {:?}", index + 1, hook_count, elapsed);
                }
                Ok(Err(e)) => {
                    error!("关闭回调 {}/{} 失败: {}", index + 1, hook_count, e);
                    // 继续执行其他回调，不中断整个关闭过程
                }
                Err(_) => {
                    error!("关闭回调 {}/{} 超时 ({:?})", index + 1, hook_count, self.config.hook_timeout);
                    // 继续执行其他回调
                }
            }
        }

        info!("所有关闭回调执行完成");
        Ok(())
    }

    /// 等待关闭信号
    pub async fn wait_for_shutdown_signal() -> &'static str {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            
            let mut sigterm = signal(SignalKind::terminate())
                .expect("Failed to register SIGTERM handler");
            let mut sigint = signal(SignalKind::interrupt())
                .expect("Failed to register SIGINT handler");
            let mut sigquit = signal(SignalKind::quit())
                .expect("Failed to register SIGQUIT handler");
            
            tokio::select! {
                _ = sigterm.recv() => "SIGTERM",
                _ = sigint.recv() => "SIGINT", 
                _ = sigquit.recv() => "SIGQUIT",
            }
        }
        
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
            "CTRL+C"
        }
    }
}

/// 全局关闭管理器实例
static SHUTDOWN_MANAGER: once_cell::sync::OnceCell<ShutdownManager> = once_cell::sync::OnceCell::new();

/// 初始化全局关闭管理器
pub fn init_shutdown_manager(config: Option<ShutdownConfig>) -> &'static ShutdownManager {
    SHUTDOWN_MANAGER.get_or_init(|| {
        ShutdownManager::new(config.unwrap_or_default())
    })
}

/// 获取全局关闭管理器
pub fn get_shutdown_manager() -> &'static ShutdownManager {
    SHUTDOWN_MANAGER.get().expect("ShutdownManager not initialized")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_shutdown_manager() {
        let config = ShutdownConfig {
            total_timeout: Duration::from_secs(5),
            hook_timeout: Duration::from_secs(2),
            force_exit_on_timeout: false,
        };
        
        let manager = ShutdownManager::new(config);
        
        // 注册测试回调
        manager.register_shutdown_hook("test_hook".to_string(), || async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            info!("测试回调执行完成");
            Ok(())
        }).await;
        
        // 执行关闭
        let result = manager.shutdown().await;
        assert!(result.is_ok());
        assert!(manager.is_shutting_down());
    }
}
