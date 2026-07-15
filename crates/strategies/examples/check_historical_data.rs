use anyhow::Result;
use sqlx::Row;
use std::env;

/// 检查数据库中的历史数据情况
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("========== 数据库历史数据检查 ==========\n");

    // 检查BTC数据
    let btc_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "btc-usdt-swap_candles_4h" WHERE confirm = '1'"#,
    )
    .fetch_one(&pool)
    .await?;

    let btc_first: Option<String> = sqlx::query_scalar(
        r#"SELECT to_timestamp(ts/1000)::text FROM "btc-usdt-swap_candles_4h"
           WHERE confirm = '1' ORDER BY ts ASC LIMIT 1"#,
    )
    .fetch_optional(&pool)
    .await?;

    let btc_last: Option<String> = sqlx::query_scalar(
        r#"SELECT to_timestamp(ts/1000)::text FROM "btc-usdt-swap_candles_4h"
           WHERE confirm = '1' ORDER BY ts DESC LIMIT 1"#,
    )
    .fetch_optional(&pool)
    .await?;

    println!("📊 BTC-USDT-SWAP (4H):");
    println!("  总K线数: {}", btc_count);
    println!(
        "  时间跨度: 约{:.1}天 ({}根/天)",
        btc_count as f64 * 4.0 / 24.0,
        btc_count
    );
    println!("  最早时间: {}", btc_first.unwrap_or("N/A".to_string()));
    println!("  最新时间: {}", btc_last.unwrap_or("N/A".to_string()));

    // 检查ETH数据
    let eth_result = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "eth-usdt-swap_candles_4h" WHERE confirm = '1'"#,
    )
    .fetch_optional(&pool)
    .await;

    if let Ok(Some(eth_count)) = eth_result {
        let eth_first: Option<String> = sqlx::query_scalar(
            r#"SELECT to_timestamp(ts/1000)::text FROM "eth-usdt-swap_candles_4h"
               WHERE confirm = '1' ORDER BY ts ASC LIMIT 1"#,
        )
        .fetch_optional(&pool)
        .await?;

        let eth_last: Option<String> = sqlx::query_scalar(
            r#"SELECT to_timestamp(ts/1000)::text FROM "eth-usdt-swap_candles_4h"
               WHERE confirm = '1' ORDER BY ts DESC LIMIT 1"#,
        )
        .fetch_optional(&pool)
        .await?;

        println!("\n📊 ETH-USDT-SWAP (4H):");
        println!("  总K线数: {}", eth_count);
        println!("  时间跨度: 约{:.1}天", eth_count as f64 * 4.0 / 24.0);
        println!("  最早时间: {}", eth_first.unwrap_or("N/A".to_string()));
        println!("  最新时间: {}", eth_last.unwrap_or("N/A".to_string()));
    } else {
        println!("\n⚠️  ETH-USDT-SWAP 数据不存在");
    }

    // 检查SOL数据
    let sol_result = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "sol-usdt-swap_candles_4h" WHERE confirm = '1'"#,
    )
    .fetch_optional(&pool)
    .await;

    if let Ok(Some(sol_count)) = sol_result {
        let sol_first: Option<String> = sqlx::query_scalar(
            r#"SELECT to_timestamp(ts/1000)::text FROM "sol-usdt-swap_candles_4h"
               WHERE confirm = '1' ORDER BY ts ASC LIMIT 1"#,
        )
        .fetch_optional(&pool)
        .await?;

        let sol_last: Option<String> = sqlx::query_scalar(
            r#"SELECT to_timestamp(ts/1000)::text FROM "sol-usdt-swap_candles_4h"
               WHERE confirm = '1' ORDER BY ts DESC LIMIT 1"#,
        )
        .fetch_optional(&pool)
        .await?;

        println!("\n📊 SOL-USDT-SWAP (4H):");
        println!("  总K线数: {}", sol_count);
        println!("  时间跨度: 约{:.1}天", sol_count as f64 * 4.0 / 24.0);
        println!("  最早时间: {}", sol_first.unwrap_or("N/A".to_string()));
        println!("  最新时间: {}", sol_last.unwrap_or("N/A".to_string()));
    } else {
        println!("\n⚠️  SOL-USDT-SWAP 数据不存在");
    }

    println!("\n💡 建议:");
    if btc_count >= 5000 {
        println!("  ✅ BTC数据充足，可以使用5000根K线进行长周期测试");
    } else if btc_count >= 3000 {
        println!("  ✅ BTC数据可用，建议使用{}根K线进行测试", btc_count);
    } else {
        println!("  ⚠️  BTC数据较少，建议先获取更多历史数据");
    }

    println!("\n================================");

    Ok(())
}
