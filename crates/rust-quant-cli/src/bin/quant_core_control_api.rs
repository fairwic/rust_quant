use anyhow::Result;

/// Core 控制面独立入口，避免通过通用模式变量误装配后台任务。
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::app::control_api::run_control_api().await
}
