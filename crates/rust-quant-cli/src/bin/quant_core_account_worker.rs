use anyhow::Result;
use rust_quant_services::rust_quan_web::ExecutionWorkerLane;

/// 启动账户侧订单确认角色；迁移期继续复用现有 confirmation 状态机。
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::app::execution_worker_runtime::run_execution_worker_lane(
        ExecutionWorkerLane::Confirmation,
    )
    .await
}
