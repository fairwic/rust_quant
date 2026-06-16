use anyhow::Result;
use rust_quant_cli::app::market_velocity_event_backtest::{
    config_from_env_and_args, parse_paper_observation_command_from,
    run_market_velocity_event_backtest,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let command = parse_paper_observation_command_from(std::env::args().skip(1))?;
    let config = config_from_env_and_args(command.backtest_args)?;
    if let Some(interval_seconds) = command.loop_interval_seconds {
        loop {
            tracing::info!(
                interval_seconds,
                "starting market velocity paper observation cycle"
            );
            if let Err(error) = run_market_velocity_event_backtest(config.clone()).await {
                tracing::error!(
                    error = %error,
                    "market velocity paper observation cycle failed"
                );
            }
            tracing::info!(
                interval_seconds,
                "market velocity paper observation cycle sleeping"
            );
            sleep(Duration::from_secs(interval_seconds)).await;
        }
    }
    run_market_velocity_event_backtest(config).await
}
