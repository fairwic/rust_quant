use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // 使用新架构的 rust-quant-cli 入口
    rust_quant_cli::app_init().await?;
    rust_quant_cli::run().await
}