use anyhow::{Context, Result};
use rust_quant_cli::app::market_flow_flip_reversal_research::{
    parse_flow_flip_research_args, run_leverage_continuation_research,
};

/// 运行冻结 V1 的杠杆资金流延续研究，不触发交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_flow_flip_research_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_leverage_flow_continuation_research requires QUANT_CORE_DATABASE_URL")?;
    run_leverage_continuation_research(&args, &database_url).await?;
    Ok(())
}
