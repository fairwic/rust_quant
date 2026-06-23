use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::{interval, sleep_until, Duration, Instant};
pub struct TaskScheduler {
    /// 键值扩展数据。
    periodic_tasks: HashMap<String, JoinHandle<()>>,
    /// 键值扩展数据。
    scheduled_tasks: HashMap<String, JoinHandle<()>>,
    /// shutdown发送器，用于当前结构体的业务数据。
    shutdown_sender: broadcast::Sender<()>,
}
impl TaskScheduler {
    /// 初始化new，确保量化核心依赖和内部状态可直接使用。
    pub fn new() -> Self {
        let (shutdown_sender, _) = broadcast::channel(16);
        Self {
            periodic_tasks: HashMap::new(),
            scheduled_tasks: HashMap::new(),
            shutdown_sender,
        }
    }
}
impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}
impl TaskScheduler {
    /// 封装当前函数，减少量化核心调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    pub fn add_periodic_task<F, Fut>(&mut self, name: String, every_n_millis: u64, task_fn: F)
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        if every_n_millis < 100 {
            panic!("Minimum interval is 100 milliseconds");
        }
        let mut interval_timer = interval(Duration::from_millis(every_n_millis));
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        let task_name = name.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = interval_timer.tick() => {
                        task_fn().await;
                    }
                    _ = shutdown_receiver.recv() => {
                        println!("Periodic task {} is shutting down", task_name);
                        break;
                    }
                }
            }
        });
        self.periodic_tasks.insert(name, handle);
    }
    /// 把数据加入 量化核心 聚合结果，保持集合构造逻辑集中。
    pub fn add_scheduled_task<F, Fut>(&mut self, name: String, target_time: DateTime<Utc>, task: F)
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let duration_until_target = (target_time - Utc::now()).to_std().unwrap();
        let target_instant = Instant::now() + duration_until_target;
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        let task_name = name.clone();
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = sleep_until(target_instant) => {
                    task().await;
                }
                _ = shutdown_receiver.recv() => {
                    println!("Scheduled task {} is shutting down", task_name);
                }
            }
        });
        self.scheduled_tasks.insert(name, handle);
    }
    /// 提供shutdown的集中实现，避免量化核心调用方重复处理相同细节。
    pub async fn shutdown(self) {
        // 发送关闭信号
        let _ = self.shutdown_sender.send(());
        // 等待所有任务完成
        for handle in self.periodic_tasks.into_values() {
            let _ = handle.await;
        }
        for handle in self.scheduled_tasks.into_values() {
            let _ = handle.await;
        }
    }
}
