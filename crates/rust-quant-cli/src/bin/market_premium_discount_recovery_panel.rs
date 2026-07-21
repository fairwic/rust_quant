use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_cross_exchange_basis_panel_args, run_premium_discount_recovery_panel,
};

/// 运行冻结的 premium 折价修复 15m 只读因子面板。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_cross_exchange_basis_panel_args(std::env::args().skip(1))?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("market_premium_discount_recovery_panel requires QUANT_CORE_DATABASE_URL")?;
    run_premium_discount_recovery_panel(&args, &database_url).await?;
    Ok(())
}
