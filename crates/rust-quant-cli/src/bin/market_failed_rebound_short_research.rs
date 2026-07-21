use anyhow::{Context, Result};
use rust_quant_cli::app::market_flow_flip_reversal_research::{
    parse_failed_rebound_research_args, run_failed_rebound_research,
};

/// 运行冻结 V4 的反弹失败顺势空头研究，不触发交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_failed_rebound_research_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_failed_rebound_short_research requires QUANT_CORE_DATABASE_URL")?;
    run_failed_rebound_research(&args, &database_url).await?;
    Ok(())
}
