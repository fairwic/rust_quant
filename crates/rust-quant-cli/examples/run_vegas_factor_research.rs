use anyhow::{anyhow, Result};
use rust_quant_core::database::{close_db_pool, init_db_pool};
use rust_quant_services::strategy::{VegasFactorResearchQuery, VegasFactorResearchService};
use std::fs;

fn parse_baseline_ids(raw: &str) -> Result<Vec<i64>> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|e| anyhow!("baseline id 解析失败 {}: {}", value, e))
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    init_db_pool().await?;

    let baseline_ids = parse_baseline_ids(
        &std::env::var("VEGAS_RESEARCH_BASELINE_IDS")
            .unwrap_or_else(|_| "1428,1429,1430,1431".to_string()),
    )?;
    let timeframe = std::env::var("VEGAS_RESEARCH_TIMEFRAME").unwrap_or_else(|_| "4H".to_string());
    let output_path = std::env::var("VEGAS_RESEARCH_OUTPUT_PATH").ok();

    let service = VegasFactorResearchService::new()?;
    let report = service
        .run_report_text(VegasFactorResearchQuery {
            baseline_ids,
            timeframe,
        })
        .await?;

    println!("{}", report);

    if let Some(path) = output_path {
        fs::write(&path, &report)?;
        println!("\nreport_saved={}", path);
    }

    close_db_pool().await?;
    Ok(())
}
