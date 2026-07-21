use anyhow::Result;
use rust_quant_services::rust_quan_web::{ExecutionWorker, ExecutionWorkerLane};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

/// 按代码入口固定的职责通道持续轮询，避免部署环境把一个进程切换成另一类消费者。
///
/// Worker 构造与实盘审计检查在进入循环前完成；任一检查失败都会阻止进程启动。
pub async fn run_execution_worker_lane(lane: ExecutionWorkerLane) -> Result<()> {
    let worker = ExecutionWorker::from_env_for_lane(lane)?;
    worker.verify_live_audit_ready().await?;
    let poll_interval_secs = execution_worker_poll_interval_secs();
    info!(
        lane = lane.as_str(),
        poll_interval_secs, "execution runtime lane started"
    );

    loop {
        match worker.run_once().await {
            Ok(handled) if handled > 0 => info!(
                lane = lane.as_str(),
                handled, "execution runtime lane completed a poll"
            ),
            Ok(_) => {}
            Err(error) => error!(
                lane = lane.as_str(),
                error = %error,
                "execution runtime lane poll failed"
            ),
        }
        sleep(Duration::from_secs(poll_interval_secs)).await;
    }
}

/// 将异常或零轮询间隔收敛到安全默认值，防止错误配置形成数据库忙循环。
fn execution_worker_poll_interval_secs() -> u64 {
    std::env::var("EXECUTION_WORKER_POLL_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5)
}

#[cfg(test)]
mod tests {
    use super::execution_worker_poll_interval_secs;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn poll_interval_rejects_zero_and_invalid_values() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let previous = std::env::var("EXECUTION_WORKER_POLL_INTERVAL_SECS").ok();

        for value in ["0", "invalid"] {
            std::env::set_var("EXECUTION_WORKER_POLL_INTERVAL_SECS", value);
            assert_eq!(execution_worker_poll_interval_secs(), 5);
        }
        std::env::set_var("EXECUTION_WORKER_POLL_INTERVAL_SECS", "7");
        assert_eq!(execution_worker_poll_interval_secs(), 7);

        match previous {
            Some(value) => std::env::set_var("EXECUTION_WORKER_POLL_INTERVAL_SECS", value),
            None => std::env::remove_var("EXECUTION_WORKER_POLL_INTERVAL_SECS"),
        }
    }
}
