use anyhow::Result;
#[tokio::main]
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::run().await
}
