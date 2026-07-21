use anyhow::{Context, Result};
use rust_quant_cli::app::market_cross_exchange_basis_panel::{
    parse_large_trade_panel_args, run_large_trade_absorption_panel,
};

/// 运行只读 BTC—ETH 大单吸收价差因子面板。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_large_trade_panel_args(std::env::args().skip(1))?;
    run_large_trade_absorption_panel(&args)
        .await
        .context("run BTC-ETH large-trade absorption panel")?;
    Ok(())
}
