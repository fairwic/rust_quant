use anyhow::Result;

/// 启动 Core 内部控制面；该进程只暴露 HTTP API，不装配行情、策略或交易 worker。
pub async fn run_control_api() -> Result<()> {
    super::internal_server::run_internal_server().await
}
