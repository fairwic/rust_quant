use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_liquidation_relative_panel_args, run_liquidation_relative_panel,
};

/// 运行只读 BTC—ETH 强平耗竭价差因子面板。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_liquidation_relative_panel_args(std::env::args().skip(1))?;
    run_liquidation_relative_panel(&args)
        .await
        .context("run BTC-ETH liquidation exhaustion panel")?;
    Ok(())
}
