use anyhow::Result;
use rust_quant_services::rust_quan_web::ExecutionWorkerLane;

/// 启动对账恢复角色；迁移期继续复用现有 report replay 状态机。
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::app::execution_worker_runtime::run_execution_worker_lane(
        ExecutionWorkerLane::ReportReplay,
    )
    .await
}
