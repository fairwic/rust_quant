use anyhow::{Context, Result};
use rust_quant_cli::app::market_flow_flip_reversal_research::{
    parse_flow_flip_research_args, run_flow_acceptance_research,
};

/// 运行冻结 V3 的去杠杆主动流翻转后回踩接受研究，不触发交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_flow_flip_research_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_flow_acceptance_reversal_research requires QUANT_CORE_DATABASE_URL")?;
    run_flow_acceptance_research(&args, &database_url).await?;
    Ok(())
}
