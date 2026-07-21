use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_bvol_relative_panel_args, run_bvol_relative_momentum_panel,
};

/// 运行只读 BTC—ETH BVOL 确认相对动量因子面板。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_bvol_relative_panel_args(std::env::args().skip(1))?;
    run_bvol_relative_momentum_panel(&args)
        .await
        .context("run BTC-ETH BVOL relative momentum panel")?;
    Ok(())
}
