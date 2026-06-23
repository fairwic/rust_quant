use anyhow::Result;
use rust_quant_cli::app::binance_eth_micro_live_validation::run_binance_eth_micro_live_validation_from_env;

#[tokio::main]
/// 提供入口的集中实现，避免量化核心调用方重复处理相同细节。
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let report = run_binance_eth_micro_live_validation_from_env().await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
