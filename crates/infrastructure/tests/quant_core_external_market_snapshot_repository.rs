use anyhow::Result;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use rust_quant_infrastructure::repositories::ShardedExternalMarketSnapshotRepository;
use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::test]
async fn upserts_and_reads_sharded_market_snapshots_from_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!(
            "skipping quant_core market snapshot smoke; set QUANT_CORE_EXTERNAL_MARKET_SNAPSHOT_SMOKE=1"
        );
        return Ok(());
    }

    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").expect("QUANT_CORE_DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let repo = ShardedExternalMarketSnapshotRepository::new(pool.clone());
    let source = "codex_test";
    let symbol = "BTC-TEST-SWAP";
    let metric_type = "funding_rate";
    let timestamp = 9_100_000_000_000_i64;
    let table_name =
        ShardedExternalMarketSnapshotRepository::quoted_table_name(source, symbol, metric_type)?;
    cleanup(&pool, &table_name).await?;

    let mut snapshot = ExternalMarketSnapshot::new(
        source.to_string(),
        symbol.to_string(),
        metric_type.to_string(),
        timestamp,
    );
    snapshot.funding_rate = Some(0.0001);
    snapshot.open_interest = Some(123_456.0);
    repo.save(snapshot).await?;

    let rows = repo
        .find_range(
            source,
            symbol,
            metric_type,
            timestamp - 1,
            timestamp + 1,
            Some(10),
        )
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].funding_rate, Some(0.0001));
    assert_eq!(rows[0].open_interest, Some(123_456.0));
    assert_table_comments(&pool, &table_name).await?;

    cleanup(&pool, &table_name).await?;
    Ok(())
}

async fn assert_table_comments(pool: &sqlx::PgPool, quoted_table_name: &str) -> Result<()> {
    let table_name = quoted_table_name.trim_matches('"');
    let table_comment: Option<String> = sqlx::query_scalar("SELECT obj_description($1::regclass)")
        .bind(table_name)
        .fetch_one(pool)
        .await?;
    assert_eq!(table_comment.as_deref(), Some("市场上下文快照分表"));

    let column_comment: Option<String> = sqlx::query_scalar(
        r#"
        SELECT col_description($1::regclass, ordinal_position::int)
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = $2
          AND column_name = 'funding_rate'
        "#,
    )
    .bind(table_name)
    .bind(table_name)
    .fetch_one(pool)
    .await?;
    assert_eq!(column_comment.as_deref(), Some("资金费率"));
    Ok(())
}

async fn cleanup(pool: &sqlx::PgPool, quoted_table_name: &str) -> Result<()> {
    sqlx::query(&format!("DROP TABLE IF EXISTS {}", quoted_table_name))
        .execute(pool)
        .await?;
    Ok(())
}

fn smoke_enabled() -> bool {
    env::var("QUANT_CORE_EXTERNAL_MARKET_SNAPSHOT_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}
