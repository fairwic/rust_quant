use anyhow::{Context, Result};
use rust_quant_cli::app::market_beta_residual_momentum_research::{
    parse_residual_momentum_args, run_residual_momentum_research,
};

/// 运行冻结的 15m BTC Beta 残差动量只读研究，不触发交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_residual_momentum_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_beta_residual_momentum_research requires QUANT_CORE_DATABASE_URL")?;
    run_residual_momentum_research(&args, &database_url).await?;
    Ok(())
}
