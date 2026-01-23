use sqlx::mysql::MySqlPoolOptions;
use sqlx::{FromRow, Row};
use std::collections::HashMap;
use std::env;

#[derive(Debug, FromRow)]
struct TradeDetail {
    inst_id: String,
    open_position_time: String,
    option_type: String, // long/short
    open_price: String,
    close_price: Option<String>,
    profit_loss: String,
    stop_loss_source: Option<String>,
    close_type: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    // 1. Setup DB connection
    let database_url = env::var("DATABASE_URL")
        .or_else(|_| env::var("DB_HOST"))
        .expect("DATABASE_URL or DB_HOST must be set in .env");

    println!("Connecting to DB: {}", database_url);

    let pool = MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("DB Connection failed: {}", e))?;

    let old_id = 15650;
    let new_id = 15653;

    println!("Comparing Backtest ID {} (Old) vs {} (New)", old_id, new_id);

    // 2. Fetch Trades
    let old_trades = fetch_trades(&pool, old_id).await?;
    let new_trades = fetch_trades(&pool, new_id).await?;

    println!("Old Trades Count: {}", old_trades.len());
    println!("New Trades Count: {}", new_trades.len());

    // 3. Compare
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    let mut large_entity_triggers = Vec::new();

    for (key, new_trade) in &new_trades {
        if let Some(source) = &new_trade.stop_loss_source {
            if !source.is_empty() {
                large_entity_triggers.push(new_trade);
            }
        }

        if let Some(old_trade) = old_trades.get(key) {
            let new_pnl = new_trade.profit_loss.parse::<f64>().unwrap_or(0.0);
            let old_pnl = old_trade.profit_loss.parse::<f64>().unwrap_or(0.0);
            let diff = new_pnl - old_pnl;

            if diff < -1.0 {
                // Significant regression
                regressions.push((diff, old_trade, new_trade));
            } else if diff > 1.0 {
                improvements.push((diff, old_trade, new_trade));
            }
        }
    }

    // Sort by regression magnitude (worst first)
    regressions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    println!("\n=== 📉 Regressions (Worse Performance) ===");
    for (diff, old, new) in regressions.iter().take(10) {
        let old_pnl = old.profit_loss.parse::<f64>().unwrap_or(0.0);
        let new_pnl = new.profit_loss.parse::<f64>().unwrap_or(0.0);
        let old_close = old
            .close_price
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);
        let new_close = new
            .close_price
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);

        println!("--------------------------------------------------");
        println!(
            "Time: {} | Inst: {} | Side: {}",
            new.open_position_time, new.inst_id, new.option_type
        );
        println!(
            "  Old PnL: {:.2} | Close: {:.2} | Type: {}",
            old_pnl, old_close, old.close_type
        );
        println!(
            "  New PnL: {:.2} | Close: {:.2} | Type: {} | Source: {:?}",
            new_pnl, new_close, new.close_type, new.stop_loss_source
        );
        println!("  Diff: {:.2}", diff);
    }

    println!("\n=== 📈 Improvements (Better Performance) ===");
    for (diff, old, new) in improvements.iter().take(5) {
        let old_pnl = old.profit_loss.parse::<f64>().unwrap_or(0.0);
        let new_pnl = new.profit_loss.parse::<f64>().unwrap_or(0.0);
        let old_close = old
            .close_price
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);
        let new_close = new
            .close_price
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);

        println!("--------------------------------------------------");
        println!(
            "Time: {} | Inst: {} | Side: {}",
            new.open_position_time, new.inst_id, new.option_type
        );
        println!(
            "  Old PnL: {:.2} | Close: {:.2} | Type: {}",
            old_pnl, old_close, old.close_type
        );
        println!(
            "  New PnL: {:.2} | Close: {:.2} | Type: {} | Source: {:?}",
            new_pnl, new_close, new.close_type, new.stop_loss_source
        );
        println!("  Diff: {:.2}", diff);
    }

    println!(
        "\n=== 🛑 Large Entity Stop Loss Triggers (Total: {}) ===",
        large_entity_triggers.len()
    );
    let mut negative_outcome = 0;
    for trade in large_entity_triggers.iter() {
        let pnl = trade.profit_loss.parse::<f64>().unwrap_or(0.0);
        if pnl < 0.0 {
            negative_outcome += 1;
        }
    }
    println!(
        "Times triggered resulting in LOSS: {}/{}",
        negative_outcome,
        large_entity_triggers.len()
    );

    // Check specific examples of Large Entity triggering prematurely
    println!("\n--- Large Entity Stop Loss Examples ---");
    for trade in large_entity_triggers.iter().take(5) {
        let pnl = trade.profit_loss.parse::<f64>().unwrap_or(0.0);
        let close = trade
            .close_price
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);
        let old_match = old_trades.get(&(trade.inst_id.clone(), trade.open_position_time.clone()));
        let old_pnl = old_match
            .map(|t| t.profit_loss.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);
        println!(
            "Time: {} | PnL: {:.2} (Old: {:.2}) | Close: {:.2}",
            trade.open_position_time, pnl, old_pnl, close
        );
    }

    Ok(())
}

async fn fetch_trades(
    pool: &sqlx::Pool<sqlx::MySql>,
    backtest_id: i64,
) -> anyhow::Result<HashMap<(String, String), TradeDetail>> {
    let rows = sqlx::query_as::<_, TradeDetail>(
        r#"
        SELECT inst_id, CAST(open_position_time AS CHAR) as open_position_time, option_type, open_price, close_price, profit_loss, stop_loss_source, close_type
        FROM back_test_detail
        WHERE back_test_id = ?
        "#
    )
    .bind(backtest_id)
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();
    for trade in rows {
        // Key by (InstId, OpenTime) to match same trade across backtests
        map.insert(
            (trade.inst_id.clone(), trade.open_position_time.clone()),
            trade,
        );
    }
    Ok(map)
}
