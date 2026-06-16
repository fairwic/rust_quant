use anyhow::Result;
use rust_quant_cli::app::market_velocity_event_backtest::{
    config_from_env_and_args, parse_cli_args_from, run_market_velocity_event_backtest,
};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let args = parse_cli_args_from(std::env::args().skip(1))?;
    let config = config_from_env_and_args(args)?;
    run_market_velocity_event_backtest(config).await
}
