use anyhow::{Context, Result};
use rust_quant_cli::app::market_beta_hedged_residual_research::{
    parse_beta_hedged_residual_args, run_beta_hedged_residual_research,
};

/// 连接本地 Core 数据库并运行冻结的双腿残差只读研究。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_beta_hedged_residual_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_beta_hedged_residual_research requires QUANT_CORE_DATABASE_URL")?;
    run_beta_hedged_residual_research(&args, &database_url).await?;
    Ok(())
}
