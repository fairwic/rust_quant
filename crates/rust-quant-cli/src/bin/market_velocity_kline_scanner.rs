use anyhow::Result;
use rust_quant_cli::app::market_velocity_kline_scanner::{
    config_from_env_and_args, parse_cli_args_from, run_market_velocity_kline_scanner,
    MarketVelocityKlineScannerConfig, MarketVelocityKlineScannerReport,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
/// Runs the 15m K-line scanner that turns completed momentum candles into rank-event candidates.
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();
    let cli_args = parse_cli_args_from(std::env::args().skip(1))?;
    let loop_interval_seconds = cli_args.loop_interval_seconds;
    let config = config_from_env_and_args(cli_args)?;
    if let Some(interval_seconds) = loop_interval_seconds {
        loop {
            let report = run_market_velocity_kline_scanner(config.clone()).await?;
            print_report(&config, &report);
            sleep(Duration::from_secs(interval_seconds)).await;
        }
    }
    let report = run_market_velocity_kline_scanner(config.clone()).await?;
    print_report(&config, &report);
    Ok(())
}

fn print_report(
    config: &MarketVelocityKlineScannerConfig,
    report: &MarketVelocityKlineScannerReport,
) {
    println!(
        "market_velocity_kline_scanner: symbols_total={} candidate_events={} events_inserted={} duplicate_events={} lookback_minutes={} min_price_change_pct={} max_price_change_pct={:?} per_symbol_limit={} dry_run={}",
        report.symbols_total,
        report.candidate_events,
        report.events_inserted,
        report.duplicate_events,
        config.lookback_minutes,
        config.min_price_change_pct,
        config.max_price_change_pct,
        config.per_symbol_limit,
        report.dry_run
    );
}
