use anyhow::Result;

/// 策略评估与信号 handoff 入口；真实交易 mutation 仍只允许 execution-worker 执行。
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::app::signal_worker::run_signal_worker().await
}
