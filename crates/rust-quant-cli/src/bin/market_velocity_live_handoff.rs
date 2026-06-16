use anyhow::Result;
use rust_quant_cli::app::market_velocity_live_handoff::run_market_velocity_live_handoff_runtime_from_env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    run_market_velocity_live_handoff_runtime_from_env().await
}
