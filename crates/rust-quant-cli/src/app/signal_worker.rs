use anyhow::{anyhow, Result};
use tracing::info;

/// 共享一组行情连接运行已预热的策略配置，并在同一进程顺序评估多个 live handoff 快照。
pub async fn run_signal_worker() -> Result<()> {
    let strategies = super::bootstrap::run_modes();
    let live_handoffs =
        super::market_velocity_live_handoff::run_market_velocity_live_handoff_multi_runtime_from_env();
    tokio::pin!(strategies, live_handoffs);
    tokio::select! {
        result = &mut strategies => critical_lane_result("strategy-runtime", result),
        result = &mut live_handoffs => critical_lane_result("live-handoff", result),
        signal = shutdown_signal() => {
            info!(signal, "signal-worker received shutdown signal");
            Ok(())
        }
    }
}

fn critical_lane_result(lane: &str, result: Result<()>) -> Result<()> {
    match result {
        Ok(()) => Err(anyhow!(
            "critical signal-worker lane exited unexpectedly: {lane}"
        )),
        Err(error) => Err(error.context(format!("critical signal-worker lane failed: {lane}"))),
    }
}

async fn shutdown_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
        "CTRL_C"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_critical_lane_exit_is_not_reported_as_healthy() {
        assert!(critical_lane_result("strategy-runtime", Ok(())).is_err());
    }
}
