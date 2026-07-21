use anyhow::Result;

/// Market 事实生产入口；不持有 Web execution secret 或交易所 mutation 配置。
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::app::market_worker::run_market_worker().await
}
