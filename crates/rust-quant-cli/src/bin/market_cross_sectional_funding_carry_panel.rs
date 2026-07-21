use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_cross_exchange_basis_panel_args, run_cross_sectional_funding_carry_panel,
};

/// 运行只读横截面 funding carry 因子面板。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_cross_exchange_basis_panel_args(std::env::args().skip(1))?;
    run_cross_sectional_funding_carry_panel(&args)
        .await
        .context("run cross-sectional funding carry panel")?;
    Ok(())
}
