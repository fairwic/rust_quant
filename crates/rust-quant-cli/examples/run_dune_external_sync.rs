use anyhow::Result;
use dotenv::dotenv;
use rust_quant_infrastructure::external_data::DuneQueryPerformance;
use rust_quant_orchestration::workflow::external_market_sync_job::ExternalMarketSyncJob;
use std::collections::HashMap;
use std::env;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    env_logger::init();

    rust_quant_core::database::sqlx_pool::init_db_pool().await?;

    let symbol = env::var("DUNE_SYMBOL").unwrap_or_else(|_| "ETH".to_string());
    let start_time =
        env::var("DUNE_START_TIME").unwrap_or_else(|_| "2026-03-30T00:00:00Z".to_string());
    let end_time =
        env::var("DUNE_END_TIME").unwrap_or_else(|_| "2026-03-30T08:00:00Z".to_string());
    let min_usd = env::var("DUNE_MIN_USD").unwrap_or_else(|_| "100000".to_string());
    let metric_type =
        env::var("DUNE_METRIC_TYPE").unwrap_or_else(|_| "hyperliquid_basis".to_string());
    let template_path = env::var("DUNE_TEMPLATE_PATH").unwrap_or_else(|_| {
        "docs/external_market_data/dune/hyperliquid_funding_basis.sql".to_string()
    });

    let mut params = HashMap::new();
    params.insert("symbol".to_string(), symbol.clone());
    params.insert("start_time".to_string(), start_time.clone());
    params.insert("end_time".to_string(), end_time.clone());
    params.insert("min_usd".to_string(), min_usd.clone());

    info!(
        "执行 Dune 外部市场同步: metric_type={}, symbol={}, template_path={}",
        metric_type, symbol, template_path
    );

    ExternalMarketSyncJob::sync_dune_template(
        &metric_type,
        &symbol,
        &template_path,
        params,
        DuneQueryPerformance::Medium,
    )
    .await?;

    Ok(())
}
