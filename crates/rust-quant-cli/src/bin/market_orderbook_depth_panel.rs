use anyhow::{Context, Result};
use rust_quant_cli::app::market_orderbook_depth_panel::{
    parse_orderbook_depth_panel_args, run_orderbook_depth_panel,
};

/// 运行冻结 V1 的订单簿深度前瞻收益因子面板，不触发交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_orderbook_depth_panel_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_orderbook_depth_panel requires QUANT_CORE_DATABASE_URL")?;
    run_orderbook_depth_panel(&args, &database_url).await?;
    Ok(())
}
