#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rust_quant::app_init().await?;
    rust_quant::app::bootstrap::run().await
}