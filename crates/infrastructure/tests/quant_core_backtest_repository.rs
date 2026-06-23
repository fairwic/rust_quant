use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use rust_quant_domain::entities::{
    BacktestDetail, BacktestLog, DynamicConfigLog, FilteredSignalLog,
};
use rust_quant_domain::traits::BacktestLogRepository;
use rust_quant_infrastructure::repositories::SqlxBacktestRepository;
use sqlx::postgres::PgPoolOptions;
use std::env;
#[tokio::test]
async fn inserts_backtest_log_numeric_columns_into_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!("skipping quant_core backtest log smoke; set QUANT_CORE_BACKTEST_LOG_SMOKE=1");
        return Ok(());
    }
    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").context("QUANT_CORE_DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = SqlxBacktestRepository::new(pool.clone());
    let log = BacktestLog::new(
        "vegas".to_string(),
        "ETH-USDT-SWAP".to_string(),
        "4H".to_string(),
        "56.7".to_string(),
        "123.45".to_string(),
        3,
        Some("{\"source\":\"quant_core_backtest_repository_test\"}".to_string()),
        "{\"max_loss_percent\":0.03}".to_string(),
        "23.45".to_string(),
        1_748_649_600_000,
        1_748_736_000_000,
        42,
    );
    let inserted_id = repository.insert_log(&log).await?;
    let row = sqlx::query_as::<_, (f64, f64)>(
        "SELECT final_fund, profit FROM back_test_log WHERE id = $1",
    )
    .bind(inserted_id)
    .fetch_one(&pool)
    .await
    .context("load inserted back_test_log row")?;
    assert_eq!(row.0, 123.45);
    assert_eq!(row.1, 23.45);
    cleanup(&pool, inserted_id).await?;
    Ok(())
}
#[tokio::test]
async fn inserts_backtest_detail_timestamp_columns_into_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!("skipping quant_core backtest detail smoke; set QUANT_CORE_BACKTEST_LOG_SMOKE=1");
        return Ok(());
    }
    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").context("QUANT_CORE_DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = SqlxBacktestRepository::new(pool.clone());
    let back_test_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO back_test_log (
            strategy_type,
            inst_type,
            time,
            win_rate,
            final_fund,
            open_positions_num,
            strategy_detail,
            risk_config_detail,
            profit,
            one_bar_after_win_rate,
            two_bar_after_win_rate,
            three_bar_after_win_rate,
            four_bar_after_win_rate,
            five_bar_after_win_rate,
            ten_bar_after_win_rate,
            kline_start_time,
            kline_end_time,
            kline_nums
        ) VALUES (
            'vegas',
            'ETH-USDT-SWAP',
            '4H',
            '50.0',
            120.5,
            1,
            '{\"source\":\"quant_core_backtest_detail_test\"}',
            '{\"max_loss_percent\":0.03}',
            20.5,
            0, 0, 0, 0, 0, 0,
            1748649600000,
            1748736000000,
            42
        )
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .context("insert parent back_test_log row")?;
    let detail = BacktestDetail::new(
        back_test_id,
        "close".to_string(),
        "vegas".to_string(),
        "ETH-USDT-SWAP".to_string(),
        "4H".to_string(),
        "2026-05-11 18:00:00".to_string(),
        Some("2026-05-11 17:55:00".to_string()),
        0,
        "2026-05-11 19:00:00".to_string(),
        "3200.5".to_string(),
        Some("3250.5".to_string()),
        "50.0".to_string(),
        "0.1".to_string(),
        "true".to_string(),
        "take_profit".to_string(),
        1,
        0,
        "{}".to_string(),
        "{}".to_string(),
        None,
        None,
    );
    repository.insert_details(&[detail]).await?;
    let row = sqlx::query_as::<_, (NaiveDateTime, Option<NaiveDateTime>, NaiveDateTime)>(
        "SELECT open_position_time, signal_open_position_time, close_position_time FROM back_test_detail WHERE back_test_id = $1",
    )
    .bind(back_test_id)
    .fetch_one(&pool)
    .await
    .context("load inserted back_test_detail row")?;
    assert_eq!(
        row.0,
        NaiveDateTime::parse_from_str("2026-05-11 18:00:00", "%Y-%m-%d %H:%M:%S")?
    );
    assert_eq!(
        row.1,
        Some(NaiveDateTime::parse_from_str(
            "2026-05-11 17:55:00",
            "%Y-%m-%d %H:%M:%S",
        )?)
    );
    assert_eq!(
        row.2,
        NaiveDateTime::parse_from_str("2026-05-11 19:00:00", "%Y-%m-%d %H:%M:%S")?
    );
    cleanup_details(&pool, back_test_id).await?;
    cleanup(&pool, back_test_id).await?;
    Ok(())
}
#[tokio::test]
async fn inserts_filtered_signal_timestamp_columns_into_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!("skipping quant_core filtered signal smoke; set QUANT_CORE_BACKTEST_LOG_SMOKE=1");
        return Ok(());
    }
    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").context("QUANT_CORE_DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = SqlxBacktestRepository::new(pool.clone());
    let back_test_id = insert_parent_log(&pool).await?;
    let signal = FilteredSignalLog::new(
        back_test_id,
        "ETH-USDT-SWAP".to_string(),
        "4H".to_string(),
        "2026-05-11 18:30:00".to_string(),
        "LONG".to_string(),
        "[\"trend_filter\"]".to_string(),
        3200.5,
    );
    repository.insert_filtered_signals(&[signal]).await?;
    let row = sqlx::query_as::<_, (NaiveDateTime,)>(
        "SELECT signal_time FROM filtered_signal_log WHERE backtest_id = $1",
    )
    .bind(back_test_id)
    .fetch_one(&pool)
    .await
    .context("load inserted filtered_signal_log row")?;
    assert_eq!(
        row.0,
        NaiveDateTime::parse_from_str("2026-05-11 18:30:00", "%Y-%m-%d %H:%M:%S")?
    );
    cleanup_filtered_signals(&pool, back_test_id).await?;
    cleanup(&pool, back_test_id).await?;
    Ok(())
}
#[tokio::test]
async fn inserts_dynamic_config_timestamp_columns_into_quant_core_postgres() -> Result<()> {
    if !smoke_enabled() {
        eprintln!("skipping quant_core dynamic config smoke; set QUANT_CORE_BACKTEST_LOG_SMOKE=1");
        return Ok(());
    }
    let database_url =
        env::var("QUANT_CORE_DATABASE_URL").context("QUANT_CORE_DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres")?;
    let repository = SqlxBacktestRepository::new(pool.clone());
    let back_test_id = insert_parent_log(&pool).await?;
    let log = DynamicConfigLog {
        backtest_id: back_test_id,
        inst_id: "ETH-USDT-SWAP".to_string(),
        period: "4H".to_string(),
        kline_time: "2026-05-11 18:45:00".to_string(),
        adjustments: "{\"atr_stop\":1.8}".to_string(),
        config_snapshot: Some("{\"risk\":\"tight\"}".to_string()),
    };
    repository.insert_dynamic_config_logs(&[log]).await?;
    let row = sqlx::query_as::<_, (NaiveDateTime,)>(
        "SELECT kline_time FROM dynamic_config_log WHERE backtest_id = $1",
    )
    .bind(back_test_id)
    .fetch_one(&pool)
    .await
    .context("load inserted dynamic_config_log row")?;
    assert_eq!(
        row.0,
        NaiveDateTime::parse_from_str("2026-05-11 18:45:00", "%Y-%m-%d %H:%M:%S")?
    );
    cleanup_dynamic_config_logs(&pool, back_test_id).await?;
    cleanup(&pool, back_test_id).await?;
    Ok(())
}
async fn cleanup(pool: &sqlx::PgPool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM back_test_log WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("delete test back_test_log row")?;
    Ok(())
}
async fn cleanup_details(pool: &sqlx::PgPool, back_test_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM back_test_detail WHERE back_test_id = $1")
        .bind(back_test_id)
        .execute(pool)
        .await
        .context("delete test back_test_detail row")?;
    Ok(())
}
async fn cleanup_filtered_signals(pool: &sqlx::PgPool, back_test_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM filtered_signal_log WHERE backtest_id = $1")
        .bind(back_test_id)
        .execute(pool)
        .await
        .context("delete test filtered_signal_log row")?;
    Ok(())
}
async fn cleanup_dynamic_config_logs(pool: &sqlx::PgPool, back_test_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM dynamic_config_log WHERE backtest_id = $1")
        .bind(back_test_id)
        .execute(pool)
        .await
        .context("delete test dynamic_config_log row")?;
    Ok(())
}
async fn insert_parent_log(pool: &sqlx::PgPool) -> Result<i64> {
    let back_test_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO back_test_log (
            strategy_type,
            inst_type,
            time,
            win_rate,
            final_fund,
            open_positions_num,
            strategy_detail,
            risk_config_detail,
            profit,
            one_bar_after_win_rate,
            two_bar_after_win_rate,
            three_bar_after_win_rate,
            four_bar_after_win_rate,
            five_bar_after_win_rate,
            ten_bar_after_win_rate,
            kline_start_time,
            kline_end_time,
            kline_nums
        ) VALUES (
            'vegas',
            'ETH-USDT-SWAP',
            '4H',
            '50.0',
            120.5,
            1,
            '{\"source\":\"quant_core_backtest_detail_test\"}',
            '{\"max_loss_percent\":0.03}',
            20.5,
            0, 0, 0, 0, 0, 0,
            1748649600000,
            1748736000000,
            42
        )
        RETURNING id
        "#,
    )
    .fetch_one(pool)
    .await
    .context("insert parent back_test_log row")?;
    Ok(back_test_id)
}
fn smoke_enabled() -> bool {
    env::var("QUANT_CORE_BACKTEST_LOG_SMOKE")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}
