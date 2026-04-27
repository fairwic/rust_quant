use anyhow::Result;
use dotenv::dotenv;
use rust_quant_cli::app::exchange_symbol_sync::{
    run_exchange_symbol_sync_from_env, ExchangeSymbolSyncRequest,
};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    rust_quant_core::logger::setup_logging().await?;

    let source = std::env::var("EXCHANGE_SYMBOL_SOURCE").ok();
    let sources = source.map(|source| vec![source]);
    let response = run_exchange_symbol_sync_from_env(ExchangeSymbolSyncRequest {
        sources,
        trigger_source: Some("cli".to_string()),
        submit_signals: None,
    })
    .await?;

    info!(
        "exchange symbol sync completed: run_id={}, sources={:?}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
        response.run_id,
        response.requested_sources,
        response.persisted_rows,
        response.first_seen_rows,
        response.major_listing_signals
    );
    println!(
        "exchange symbol sync completed: run_id={}, sources={}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
        response.run_id,
        response.requested_sources.join(","),
        response.persisted_rows,
        response.first_seen_rows,
        response.major_listing_signals
    );

    for source_report in &response.source_reports {
        println!(
            "exchange_symbol_sync_source: source={}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
            source_report.source,
            source_report.persisted_rows,
            source_report.first_seen_rows,
            source_report.major_listing_signals
        );
    }

    Ok(())
}
