use anyhow::{Context, Result};
use rust_quant_cli::app::market_funding_squeeze_reversal_research::{
    parse_funding_squeeze_research_args, run_funding_squeeze_research,
};

/// 运行官方 funding 对齐的 15m 反转只读研究，不触发任何交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_funding_squeeze_research_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_funding_squeeze_reversal_research requires QUANT_CORE_DATABASE_URL")?;
    run_funding_squeeze_research(&args, &database_url).await?;
    Ok(())
}
