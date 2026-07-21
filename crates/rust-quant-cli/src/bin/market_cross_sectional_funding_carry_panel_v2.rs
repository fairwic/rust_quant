use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_cross_exchange_basis_panel_args, run_cross_sectional_funding_carry_panel_v2,
};

/// 运行只读共同可交易最小 30 成员的 funding carry V2 面板。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_cross_exchange_basis_panel_args(std::env::args().skip(1))?;
    run_cross_sectional_funding_carry_panel_v2(&args)
        .await
        .context("run cross-sectional funding carry V2 panel")?;
    Ok(())
}
