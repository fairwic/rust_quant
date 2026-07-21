use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_cross_exchange_basis_panel_args, run_top_trader_positioning_spread_panel,
};

/// 运行只读 top-trader 规模确信度横截面面板。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_cross_exchange_basis_panel_args(std::env::args().skip(1))?;
    run_top_trader_positioning_spread_panel(&args)
        .await
        .context("run top-trader positioning spread panel")?;
    Ok(())
}
