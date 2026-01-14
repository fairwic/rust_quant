use anyhow::Result;
use dotenv::dotenv;
use rust_quant_analytics::monte_carlo::MonteCarloAnalyzer;
use sqlx::mysql::MySqlPoolOptions;
use std::env;

#[derive(sqlx::FromRow)]
struct PnlRow {
    profit_loss: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env
    dotenv().ok();

    // Setup logging
    tracing_subscriber::fmt::init();

    // Get DB URL
    let database_url = env::var("DB_HOST")
        .or_else(|_| env::var("DATABASE_URL"))
        .expect("DB_HOST or DATABASE_URL must be set");

    // Connect to DB
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Get Backtest ID from args or default to 5640
    let backtest_id = env::args()
        .nth(1)
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(5640);

    println!(
        "Running Monte Carlo Analysis for Backtest ID: {}",
        backtest_id
    );

    // Query PnL data
    // Only fetch closing trades (close_type is not empty) to avoid double counting
    let rows: Vec<PnlRow> = sqlx::query_as::<_, PnlRow>(
        "SELECT profit_loss FROM back_test_detail WHERE back_test_id = ? AND LENGTH(close_type) > 0",
    )
    .bind(backtest_id)
    .fetch_all(&pool)
    .await?;

    if rows.is_empty() {
        println!("No records found for backtest_id {}", backtest_id);
        return Ok(());
    }

    // Parse PnL
    let pnls: Vec<f64> = rows
        .iter()
        .filter_map(|r| r.profit_loss.parse::<f64>().ok())
        .collect();

    println!("Loaded {} trades.", pnls.len());

    // Run Simulation
    let initial_capital = 100.0; // Match backtest initial fund
    let analyzer = MonteCarloAnalyzer::new(initial_capital);
    let report = analyzer.simulate(&pnls, 10000); // 10k iterations

    println!("{}", report);

    Ok(())
}
