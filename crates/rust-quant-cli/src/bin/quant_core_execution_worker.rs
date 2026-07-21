use anyhow::Result;
use rust_quant_services::rust_quan_web::ExecutionWorkerLane;

/// 启动唯一允许租约并执行新交易任务的长期运行角色。
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::app::execution_worker_runtime::run_execution_worker_lane(
        ExecutionWorkerLane::Execution,
    )
    .await
}
