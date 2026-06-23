use anyhow::Result;
use rust_quant_cli::app::market_velocity_backfill::{
    config_from_env_and_args, parse_cli_args_from, run_market_velocity_backfill,
};
use std::time::Duration;
use tokio::time::sleep;
#[tokio::main]
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();
    let cli_args = parse_cli_args_from(std::env::args().skip(1))?;
    let loop_interval_seconds = cli_args.loop_interval_seconds;
    let config = config_from_env_and_args(cli_args)?;
    if let Some(interval_seconds) = loop_interval_seconds {
        loop {
            tracing::info!(
                interval_seconds,
                "starting market velocity candle backfill cycle"
            );
            match run_market_velocity_backfill(config.clone()).await {
                Ok(report) => print_report(&report),
                Err(error) => tracing::error!(
                    error = %error,
                    "market velocity candle backfill cycle failed"
                ),
            }
            tracing::info!(
                interval_seconds,
                "market velocity candle backfill cycle sleeping"
            );
            sleep(Duration::from_secs(interval_seconds)).await;
        }
    }
    let report = run_market_velocity_backfill(config).await?;
    print_report(&report);
    Ok(())
}
/// 执行输出报告步骤，串起行情数据需要的状态推进和错误处理。
fn print_report(
    report: &rust_quant_cli::app::market_velocity_backfill::MarketVelocityBackfillReport,
) {
    println!(
        "market_velocity_candle_backfill: symbols_total={} symbols_attempted={} symbols_failed={} candles_fetched={} rows_upserted={} dry_run={}",
        report.symbols_total,
        report.symbols_attempted,
        report.failed_symbols.len(),
        report.candles_fetched,
        report.rows_upserted,
        report.dry_run
    );
}
