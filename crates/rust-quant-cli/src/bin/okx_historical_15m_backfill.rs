use anyhow::Result;
use rust_quant_cli::app::okx_historical_15m_backfill::{
    parse_historical_15m_backfill_args, run_historical_15m_backfill,
};

/// 校验官方分钟归档并严格聚合为本地研究 15m K 线；默认 dry-run。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_historical_15m_backfill_args(std::env::args().skip(1))?;
    let report = run_historical_15m_backfill(&args).await?;
    println!(
        "okx_historical_15m_backfill: symbols={} archives={} candles_15m={} rest_fallback_files={} partial_files={} optional_outcome_files_unavailable={} rows_upserted={} dry_run={}",
        report.symbols,
        report.archive_files,
        report.candles_15m,
        report.rest_fallback_files,
        report.partial_files,
        report.optional_outcome_files_unavailable,
        report.rows_upserted,
        report.dry_run
    );
    Ok(())
}
