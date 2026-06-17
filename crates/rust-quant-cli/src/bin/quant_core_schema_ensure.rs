use anyhow::{Context, Result};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

const POSTGRES_QUANT_CORE_DDL: &str = include_str!("../../../../sql/postgres_quant_core.sql");

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    rust_quant_core::logger::setup_logging().await?;

    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("QUANT_CORE_DATABASE_URL or DATABASE_URL must be set")?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .context("connect quant_core database for schema ensure")?;

    sqlx::raw_sql(POSTGRES_QUANT_CORE_DDL)
        .execute(&pool)
        .await
        .context("apply quant_core postgres schema DDL")?;

    let checked_tables = [
        "market_rank_events",
        "market_rank_snapshots",
        "execution_worker_checkpoints",
    ];
    let existing_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::BIGINT
          FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name = ANY($1)
        "#,
    )
    .bind(&checked_tables[..])
    .fetch_one(&pool)
    .await
    .context("verify quant_core schema ensure tables")?;

    pool.close().await;

    println!(
        "{}",
        json!({
            "status": "ok",
            "source": "quant_core_schema_ensure",
            "checked_tables": checked_tables,
            "existing_checked_table_count": existing_count,
        })
    );

    Ok(())
}
