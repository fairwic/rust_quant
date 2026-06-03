use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{json_response, query_param, strategy_configs, InternalHttpJsonResponse};

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestLogListQuery {
    pub page: usize,
    pub page_size: usize,
    pub keyword: Option<String>,
    pub status: Option<String>,
    pub exchange: Option<String>,
    pub symbol: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

pub fn backtest_log_list_query_from_path(path: &str) -> Result<BacktestLogListQuery, String> {
    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let page = query_param(query, &["page"])
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = query_param(query, &["pageSize", "page_size"])
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);

    Ok(BacktestLogListQuery {
        page,
        page_size,
        keyword: optional_text(query, &["keyword"]),
        status: optional_text(query, &["status"]).map(|value| value.to_ascii_lowercase()),
        exchange: optional_text(query, &["exchange"]).map(|value| value.to_ascii_lowercase()),
        symbol: optional_text(query, &["symbol"]).map(|value| value.to_ascii_uppercase()),
        start_time: optional_text(query, &["startTime", "start_time"]),
        end_time: optional_text(query, &["endTime", "end_time"]),
    })
}

pub fn core_backtest_run_list_query_from_path(path: &str) -> Result<BacktestLogListQuery, String> {
    backtest_log_list_query_from_path(path)
}

pub(super) async fn handle_backtest_log_list_path(path: &str) -> InternalHttpJsonResponse {
    let query = match backtest_log_list_query_from_path(path) {
        Ok(query) => query,
        Err(error) => return json_response(400, json!({ "error": error })),
    };
    let pool = match strategy_configs::create_quant_core_internal_pool() {
        Ok(pool) => pool,
        Err(error) => return json_response(503, json!({ "error": error.to_string() })),
    };

    match backtest_logs_response(&pool, &query).await {
        Ok(response) => json_response(200, response),
        Err(error) => json_response(500, json!({ "error": error.to_string() })),
    }
}

pub(super) async fn handle_core_backtest_run_list_path(path: &str) -> InternalHttpJsonResponse {
    let query = match core_backtest_run_list_query_from_path(path) {
        Ok(query) => query,
        Err(error) => return json_response(400, json!({ "error": error })),
    };
    let pool = match strategy_configs::create_quant_core_internal_pool() {
        Ok(pool) => pool,
        Err(error) => return json_response(503, json!({ "error": error.to_string() })),
    };

    match core_backtest_runs_response(&pool, &query).await {
        Ok(response) => json_response(200, response),
        Err(error) => json_response(500, json!({ "error": error.to_string() })),
    }
}

async fn backtest_logs_response(pool: &PgPool, query: &BacktestLogListQuery) -> Result<Value> {
    let legacy_exists = table_exists(pool, "back_test_log").await?;
    if legacy_exists {
        let legacy_response = legacy_backtest_logs_response(pool, query).await?;
        if legacy_response
            .get("total")
            .and_then(Value::as_u64)
            .is_some_and(|total| total > 0)
        {
            return Ok(legacy_response);
        }

        if table_exists(pool, "backtest_runs").await? {
            let modern_response = modern_backtest_logs_response(pool, query).await?;
            if modern_response
                .get("total")
                .and_then(Value::as_u64)
                .is_some_and(|total| total > 0)
            {
                return Ok(modern_response);
            }
        }
        return Ok(legacy_response);
    }

    if table_exists(pool, "backtest_runs").await? {
        return modern_backtest_logs_response(pool, query).await;
    }

    Ok(json!({
        "items": [],
        "total": 0,
        "degraded": {
            "configured": true,
            "available": false,
            "error": "back_test_log and backtest_runs tables not found"
        }
    }))
}

async fn core_backtest_runs_response(pool: &PgPool, query: &BacktestLogListQuery) -> Result<Value> {
    let modern_exists = table_exists(pool, "backtest_runs").await?;
    if modern_exists {
        let modern_response = modern_core_backtest_runs_response(pool, query).await?;
        if modern_response
            .get("total")
            .and_then(Value::as_u64)
            .is_some_and(|total| total > 0)
        {
            return Ok(modern_response);
        }

        if table_exists(pool, "back_test_log").await? {
            let legacy_response = legacy_core_backtest_runs_response(pool, query).await?;
            if legacy_response
                .get("total")
                .and_then(Value::as_u64)
                .is_some_and(|total| total > 0)
            {
                return Ok(legacy_response);
            }
        }
        return Ok(modern_response);
    }

    if table_exists(pool, "back_test_log").await? {
        return legacy_core_backtest_runs_response(pool, query).await;
    }

    Ok(json!({
        "items": [],
        "total": 0,
        "degraded": {
            "configured": true,
            "available": false,
            "error": "backtest_runs and back_test_log tables not found"
        }
    }))
}

async fn legacy_backtest_logs_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM back_test_log
        WHERE ($1::TEXT IS NULL OR strategy_type ILIKE '%' || $1 || '%' OR inst_type ILIKE '%' || $1 || '%' OR time ILIKE '%' || $1 || '%')
          AND ($2::TEXT IS NULL OR $2 = 'completed')
          AND ($3::TEXT IS NULL OR $3 = 'legacy')
          AND ($4::TEXT IS NULL OR UPPER(inst_type) = $4)
          AND ($5::TEXT IS NULL OR created_at >= $5::TIMESTAMPTZ)
          AND ($6::TEXT IS NULL OR created_at <= $6::TIMESTAMPTZ)
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .fetch_one(pool)
    .await
    .context("count legacy backtest logs")?;

    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                id,
                strategy_type,
                inst_type,
                time,
                win_rate,
                open_positions_num,
                final_fund,
                strategy_detail,
                risk_config_detail,
                created_at,
                profit,
                one_bar_after_win_rate,
                two_bar_after_win_rate,
                three_bar_after_win_rate,
                four_bar_after_win_rate,
                five_bar_after_win_rate,
                ten_bar_after_win_rate,
                kline_start_time,
                kline_end_time,
                kline_nums,
                sharpe_ratio,
                annual_return,
                total_return,
                max_drawdown,
                volatility,
                'back_test_log'::text AS source_table
            FROM back_test_log
            WHERE ($1::TEXT IS NULL OR strategy_type ILIKE '%' || $1 || '%' OR inst_type ILIKE '%' || $1 || '%' OR time ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR $2 = 'completed')
              AND ($3::TEXT IS NULL OR $3 = 'legacy')
              AND ($4::TEXT IS NULL OR UPPER(inst_type) = $4)
              AND ($5::TEXT IS NULL OR created_at >= $5::TIMESTAMPTZ)
              AND ($6::TEXT IS NULL OR created_at <= $6::TIMESTAMPTZ)
            ORDER BY created_at DESC NULLS LAST, id DESC
            LIMIT $7 OFFSET $8
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list legacy backtest logs")?;

    Ok(json!({ "items": items, "total": usize::try_from(total).unwrap_or(0) }))
}

async fn legacy_core_backtest_runs_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM back_test_log
        WHERE ($1::TEXT IS NULL OR strategy_type ILIKE '%' || $1 || '%' OR inst_type ILIKE '%' || $1 || '%' OR time ILIKE '%' || $1 || '%')
          AND ($2::TEXT IS NULL OR $2 = 'completed')
          AND ($3::TEXT IS NULL OR $3 = 'legacy')
          AND ($4::TEXT IS NULL OR UPPER(inst_type) = $4)
          AND ($5::TEXT IS NULL OR created_at >= $5::TIMESTAMPTZ)
          AND ($6::TEXT IS NULL OR created_at <= $6::TIMESTAMPTZ)
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .fetch_one(pool)
    .await
    .context("count legacy core backtest runs")?;

    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                id,
                CONCAT('legacy-', id::text) AS run_id,
                CONCAT('legacy-backtest-', id::text) AS run_name,
                strategy_type AS strategy_key,
                'legacy'::text AS exchange,
                inst_type AS symbol,
                time AS timeframe,
                'completed'::text AS run_status,
                NULLIF(REPLACE(TRIM(win_rate), '%', ''), '')::double precision AS win_rate,
                profit AS net_profit,
                final_fund,
                open_positions_num,
                max_drawdown,
                created_at AS started_at,
                created_at AS completed_at,
                created_at,
                'back_test_log'::text AS source_table
            FROM back_test_log
            WHERE ($1::TEXT IS NULL OR strategy_type ILIKE '%' || $1 || '%' OR inst_type ILIKE '%' || $1 || '%' OR time ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR $2 = 'completed')
              AND ($3::TEXT IS NULL OR $3 = 'legacy')
              AND ($4::TEXT IS NULL OR UPPER(inst_type) = $4)
              AND ($5::TEXT IS NULL OR created_at >= $5::TIMESTAMPTZ)
              AND ($6::TEXT IS NULL OR created_at <= $6::TIMESTAMPTZ)
            ORDER BY created_at DESC NULLS LAST, id DESC
            LIMIT $7 OFFSET $8
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list legacy core backtest runs")?;

    Ok(json!({ "items": items, "total": usize::try_from(total).unwrap_or(0) }))
}

async fn modern_backtest_logs_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let results_exists = table_exists(pool, "backtest_results").await?;
    if results_exists {
        modern_backtest_logs_with_results_response(pool, query).await
    } else {
        modern_backtest_logs_without_results_response(pool, query).await
    }
}

async fn modern_core_backtest_runs_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let results_exists = table_exists(pool, "backtest_results").await?;
    if results_exists {
        modern_core_backtest_runs_with_results_response(pool, query).await
    } else {
        modern_core_backtest_runs_without_results_response(pool, query).await
    }
}

async fn modern_backtest_logs_with_results_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let total = modern_backtest_runs_count(pool, query).await?;
    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                run.id::text AS id,
                run.strategy_key AS strategy_type,
                run.symbol AS inst_type,
                run.timeframe AS time,
                result.win_rate::text AS win_rate,
                result.trade_count AS open_positions_num,
                NULL::double precision AS final_fund,
                NULL::text AS strategy_detail,
                NULL::text AS risk_config_detail,
                run.created_at,
                result.net_profit::double precision AS profit,
                NULL::double precision AS one_bar_after_win_rate,
                NULL::double precision AS two_bar_after_win_rate,
                NULL::double precision AS three_bar_after_win_rate,
                NULL::double precision AS four_bar_after_win_rate,
                NULL::double precision AS five_bar_after_win_rate,
                NULL::double precision AS ten_bar_after_win_rate,
                NULL::bigint AS kline_start_time,
                NULL::bigint AS kline_end_time,
                NULL::integer AS kline_nums,
                NULL::double precision AS sharpe_ratio,
                NULL::double precision AS annual_return,
                NULL::double precision AS total_return,
                result.max_drawdown::double precision AS max_drawdown,
                NULL::double precision AS volatility,
                run.id::text AS run_id,
                run.run_name,
                run.exchange,
                run.run_status,
                run.started_at,
                run.completed_at,
                'backtest_runs'::text AS source_table
            FROM backtest_runs run
            LEFT JOIN LATERAL (
                SELECT net_profit, max_drawdown, win_rate, trade_count
                FROM backtest_results
                WHERE run_id = run.id
                ORDER BY created_at DESC
                LIMIT 1
            ) result ON TRUE
            WHERE ($1::TEXT IS NULL OR run.strategy_key ILIKE '%' || $1 || '%' OR run.symbol ILIKE '%' || $1 || '%' OR run.timeframe ILIKE '%' || $1 || '%' OR run.run_name ILIKE '%' || $1 || '%' OR run.exchange ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR LOWER(run.run_status) = $2)
              AND ($3::TEXT IS NULL OR LOWER(run.exchange) = $3)
              AND ($4::TEXT IS NULL OR UPPER(run.symbol) = $4)
              AND ($5::TEXT IS NULL OR run.created_at >= $5::TIMESTAMPTZ)
              AND ($6::TEXT IS NULL OR run.created_at <= $6::TIMESTAMPTZ)
            ORDER BY run.started_at DESC NULLS LAST, run.created_at DESC, run.id DESC
            LIMIT $7 OFFSET $8
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list modern backtest logs")?;

    Ok(json!({ "items": items, "total": total }))
}

async fn modern_core_backtest_runs_with_results_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let total = modern_backtest_runs_count(pool, query).await?;
    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                run.id::text AS id,
                run.id::text AS run_id,
                run.run_name,
                run.strategy_key,
                run.exchange,
                run.symbol,
                run.timeframe,
                run.run_status,
                result.win_rate::double precision AS win_rate,
                result.net_profit::double precision AS net_profit,
                NULL::double precision AS final_fund,
                result.trade_count AS open_positions_num,
                result.max_drawdown::double precision AS max_drawdown,
                run.started_at,
                run.completed_at,
                run.created_at,
                'backtest_runs'::text AS source_table
            FROM backtest_runs run
            LEFT JOIN LATERAL (
                SELECT net_profit, max_drawdown, win_rate, trade_count
                FROM backtest_results
                WHERE run_id = run.id
                ORDER BY created_at DESC
                LIMIT 1
            ) result ON TRUE
            WHERE ($1::TEXT IS NULL OR run.strategy_key ILIKE '%' || $1 || '%' OR run.symbol ILIKE '%' || $1 || '%' OR run.timeframe ILIKE '%' || $1 || '%' OR run.run_name ILIKE '%' || $1 || '%' OR run.exchange ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR LOWER(run.run_status) = $2)
              AND ($3::TEXT IS NULL OR LOWER(run.exchange) = $3)
              AND ($4::TEXT IS NULL OR UPPER(run.symbol) = $4)
              AND ($5::TEXT IS NULL OR run.created_at >= $5::TIMESTAMPTZ)
              AND ($6::TEXT IS NULL OR run.created_at <= $6::TIMESTAMPTZ)
            ORDER BY run.started_at DESC NULLS LAST, run.created_at DESC, run.id DESC
            LIMIT $7 OFFSET $8
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list modern core backtest runs")?;

    Ok(json!({ "items": items, "total": total }))
}

async fn modern_backtest_logs_without_results_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let total = modern_backtest_runs_count(pool, query).await?;
    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                run.id::text AS id,
                run.strategy_key AS strategy_type,
                run.symbol AS inst_type,
                run.timeframe AS time,
                NULL::text AS win_rate,
                NULL::integer AS open_positions_num,
                NULL::double precision AS final_fund,
                NULL::text AS strategy_detail,
                NULL::text AS risk_config_detail,
                run.created_at,
                NULL::double precision AS profit,
                NULL::double precision AS one_bar_after_win_rate,
                NULL::double precision AS two_bar_after_win_rate,
                NULL::double precision AS three_bar_after_win_rate,
                NULL::double precision AS four_bar_after_win_rate,
                NULL::double precision AS five_bar_after_win_rate,
                NULL::double precision AS ten_bar_after_win_rate,
                NULL::bigint AS kline_start_time,
                NULL::bigint AS kline_end_time,
                NULL::integer AS kline_nums,
                NULL::double precision AS sharpe_ratio,
                NULL::double precision AS annual_return,
                NULL::double precision AS total_return,
                NULL::double precision AS max_drawdown,
                NULL::double precision AS volatility,
                run.id::text AS run_id,
                run.run_name,
                run.exchange,
                run.run_status,
                run.started_at,
                run.completed_at,
                'backtest_runs'::text AS source_table
            FROM backtest_runs run
            WHERE ($1::TEXT IS NULL OR run.strategy_key ILIKE '%' || $1 || '%' OR run.symbol ILIKE '%' || $1 || '%' OR run.timeframe ILIKE '%' || $1 || '%' OR run.run_name ILIKE '%' || $1 || '%' OR run.exchange ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR LOWER(run.run_status) = $2)
              AND ($3::TEXT IS NULL OR LOWER(run.exchange) = $3)
              AND ($4::TEXT IS NULL OR UPPER(run.symbol) = $4)
              AND ($5::TEXT IS NULL OR run.created_at >= $5::TIMESTAMPTZ)
              AND ($6::TEXT IS NULL OR run.created_at <= $6::TIMESTAMPTZ)
            ORDER BY run.started_at DESC NULLS LAST, run.created_at DESC, run.id DESC
            LIMIT $7 OFFSET $8
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list modern backtest logs without results")?;

    Ok(json!({ "items": items, "total": total }))
}

async fn modern_core_backtest_runs_without_results_response(
    pool: &PgPool,
    query: &BacktestLogListQuery,
) -> Result<Value> {
    let total = modern_backtest_runs_count(pool, query).await?;
    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                run.id::text AS id,
                run.id::text AS run_id,
                run.run_name,
                run.strategy_key,
                run.exchange,
                run.symbol,
                run.timeframe,
                run.run_status,
                NULL::double precision AS win_rate,
                NULL::double precision AS net_profit,
                NULL::double precision AS final_fund,
                NULL::integer AS open_positions_num,
                NULL::double precision AS max_drawdown,
                run.started_at,
                run.completed_at,
                run.created_at,
                'backtest_runs'::text AS source_table
            FROM backtest_runs run
            WHERE ($1::TEXT IS NULL OR run.strategy_key ILIKE '%' || $1 || '%' OR run.symbol ILIKE '%' || $1 || '%' OR run.timeframe ILIKE '%' || $1 || '%' OR run.run_name ILIKE '%' || $1 || '%' OR run.exchange ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR LOWER(run.run_status) = $2)
              AND ($3::TEXT IS NULL OR LOWER(run.exchange) = $3)
              AND ($4::TEXT IS NULL OR UPPER(run.symbol) = $4)
              AND ($5::TEXT IS NULL OR run.created_at >= $5::TIMESTAMPTZ)
              AND ($6::TEXT IS NULL OR run.created_at <= $6::TIMESTAMPTZ)
            ORDER BY run.started_at DESC NULLS LAST, run.created_at DESC, run.id DESC
            LIMIT $7 OFFSET $8
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list modern core backtest runs without results")?;

    Ok(json!({ "items": items, "total": total }))
}

async fn modern_backtest_runs_count(pool: &PgPool, query: &BacktestLogListQuery) -> Result<usize> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM backtest_runs run
        WHERE ($1::TEXT IS NULL OR run.strategy_key ILIKE '%' || $1 || '%' OR run.symbol ILIKE '%' || $1 || '%' OR run.timeframe ILIKE '%' || $1 || '%' OR run.run_name ILIKE '%' || $1 || '%' OR run.exchange ILIKE '%' || $1 || '%')
          AND ($2::TEXT IS NULL OR LOWER(run.run_status) = $2)
          AND ($3::TEXT IS NULL OR LOWER(run.exchange) = $3)
          AND ($4::TEXT IS NULL OR UPPER(run.symbol) = $4)
          AND ($5::TEXT IS NULL OR run.created_at >= $5::TIMESTAMPTZ)
          AND ($6::TEXT IS NULL OR run.created_at <= $6::TIMESTAMPTZ)
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.exchange.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .fetch_one(pool)
    .await
    .context("count modern backtest runs")?;

    Ok(usize::try_from(total).unwrap_or(0))
}

async fn table_exists(pool: &PgPool, table_name: &str) -> Result<bool> {
    let regclass = sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass($1)::TEXT")
        .bind(format!("public.{table_name}"))
        .fetch_one(pool)
        .await
        .with_context(|| format!("check table exists: {table_name}"))?;
    Ok(regclass.is_some())
}

fn optional_text(query: &str, names: &[&str]) -> Option<String> {
    query_param(query, names).and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn offset(query: &BacktestLogListQuery) -> i64 {
    ((query.page.saturating_sub(1)) * query.page_size) as i64
}
