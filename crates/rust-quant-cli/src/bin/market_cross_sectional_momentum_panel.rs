use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_sectional_momentum_panel::{
    parse_cross_sectional_momentum_args, run_cross_sectional_momentum_panel,
};

/// 运行冻结的等名义横截面动量价差 15m 因子面板。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_cross_sectional_momentum_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_cross_sectional_momentum_panel requires QUANT_CORE_DATABASE_URL")?;
    run_cross_sectional_momentum_panel(&args, &database_url).await?;
    Ok(())
}
