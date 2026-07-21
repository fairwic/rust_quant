use anyhow::{Context, Result};
use rust_quant_cli::app::market_structure_choch_fvg_research::{
    parse_choch_fvg_research_args, run_choch_fvg_research,
};

/// 运行冻结参数的 15m CHoCH + FVG 回补只读研究，不触发任何交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_choch_fvg_research_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_structure_choch_fvg_research requires QUANT_CORE_DATABASE_URL")?;
    run_choch_fvg_research(&args, &database_url).await?;
    Ok(())
}
