use anyhow::{anyhow, Result};
use rust_quant_core::database::{close_db_pool, init_db_pool};
use rust_quant_services::strategy::{
    PathImpactQuery, VegasFactorResearchQuery, VegasFactorResearchService,
};
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

fn parse_usize_env(name: &str, default_value: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(raw) => raw
            .parse::<usize>()
            .map_err(|e| anyhow!("{} 解析失败 {}: {}", name, raw, e)),
        Err(_) => Ok(default_value),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    init_db_pool().await?;

    let timeframe = std::env::var("VEGAS_RESEARCH_TIMEFRAME").unwrap_or_else(|_| "4H".to_string());
    let output_path = std::env::var("VEGAS_RESEARCH_OUTPUT_PATH").ok();

    let service = VegasFactorResearchService::new()?;
    let report = if let Ok(raw_baseline_id) = std::env::var("VEGAS_RESEARCH_PATH_BASELINE_ID") {
        let baseline_id = raw_baseline_id.parse::<i64>().map_err(|e| {
            anyhow!(
                "VEGAS_RESEARCH_PATH_BASELINE_ID 解析失败 {}: {}",
                raw_baseline_id,
                e
            )
        })?;
        let experiment_ids = parse_baseline_ids(
            &std::env::var("VEGAS_RESEARCH_PATH_EXPERIMENT_IDS")
                .map_err(|_| anyhow!("路径影响模式需要设置 VEGAS_RESEARCH_PATH_EXPERIMENT_IDS"))?,
        )?;
        service
            .run_path_impact_report_text(PathImpactQuery {
                baseline_id,
                experiment_ids,
                timeframe,
                inst_id: std::env::var("VEGAS_RESEARCH_PATH_INST_ID").ok(),
                top_changed_limit: parse_usize_env("VEGAS_RESEARCH_PATH_TOP_LIMIT", 10)?,
            })
            .await?
    } else {
        let baseline_ids = parse_baseline_ids(
            &std::env::var("VEGAS_RESEARCH_BASELINE_IDS")
                .unwrap_or_else(|_| "1428,1429,1430,1431".to_string()),
        )?;
        service
            .run_report_text(VegasFactorResearchQuery {
                baseline_ids,
                timeframe,
            })
            .await?
    };

    println!("{}", report);

    if let Some(path) = output_path {
        fs::write(&path, &report)?;
        println!("\nreport_saved={}", path);
    }

    close_db_pool().await?;
    Ok(())
}
