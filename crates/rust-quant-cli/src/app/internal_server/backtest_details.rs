use super::{json_response, query_param, strategy_configs, InternalHttpJsonResponse};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 200;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestDetailListQuery {
    /// 页码。
    pub page: usize,
    /// 分页大小。
    pub page_size: usize,
    /// 关键词；为空时不做关键词过滤。
    pub keyword: Option<String>,
    /// 当前状态。
    pub status: Option<String>,
    /// backtest ID；为空时使用默认值或表示不限制。
    pub back_test_id: Option<String>,
    /// 交易对或资产符号。
    pub symbol: Option<String>,
    /// 交易方向。
    pub side: Option<String>,
    /// 开始时间。
    pub start_time: Option<String>,
    /// 结束时间。
    pub end_time: Option<String>,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
pub fn backtest_detail_list_query_from_path(path: &str) -> Result<BacktestDetailListQuery, String> {
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
    Ok(BacktestDetailListQuery {
        page,
        page_size,
        keyword: optional_text(query, &["keyword"]),
        status: optional_text(query, &["status"]).map(|value| value.to_ascii_lowercase()),
        back_test_id: optional_text(query, &["backTestId", "back_test_id"]),
        symbol: optional_text(query, &["symbol"]).map(|value| value.to_ascii_uppercase()),
        side: optional_text(query, &["side"]).map(|value| value.to_ascii_lowercase()),
        start_time: optional_text(query, &["startTime", "start_time"]),
        end_time: optional_text(query, &["endTime", "end_time"]),
    })
}
/// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub(super) async fn handle_backtest_detail_list_path(path: &str) -> InternalHttpJsonResponse {
    let query = match backtest_detail_list_query_from_path(path) {
        Ok(query) => query,
        Err(error) => return json_response(400, json!({ "error": error })),
    };
    let pool = match strategy_configs::create_quant_core_internal_pool() {
        Ok(pool) => pool,
        Err(error) => return json_response(503, json!({ "error": error.to_string() })),
    };
    match backtest_details_response(&pool, &query).await {
        Ok(response) => json_response(200, response),
        Err(error) => json_response(500, json!({ "error": error.to_string() })),
    }
}
/// 提供回测detailsresponse的集中实现，避免回测策略调用方重复处理相同细节。
async fn backtest_details_response(
    pool: &PgPool,
    query: &BacktestDetailListQuery,
) -> Result<Value> {
    if !table_exists(pool, "back_test_detail").await? {
        return Ok(json!({
            "items": [],
            "total": 0,
            "degraded": {
                "configured": true,
                "available": false,
                "error": "back_test_detail table not found"
            }
        }));
    }
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM back_test_detail
        WHERE ($1::TEXT IS NULL OR strategy_type ILIKE '%' || $1 || '%' OR inst_id ILIKE '%' || $1 || '%' OR time ILIKE '%' || $1 || '%' OR option_type ILIKE '%' || $1 || '%' OR close_type ILIKE '%' || $1 || '%')
          AND ($2::TEXT IS NULL OR LOWER(CAST(signal_status AS TEXT)) = $2)
          AND ($3::TEXT IS NULL OR CAST(back_test_id AS TEXT) = $3)
          AND ($4::TEXT IS NULL OR UPPER(inst_id) = $4)
          AND ($5::TEXT IS NULL OR LOWER(option_type) = $5)
          AND ($6::TEXT IS NULL OR created_at >= $6::TIMESTAMPTZ)
          AND ($7::TEXT IS NULL OR created_at <= $7::TIMESTAMPTZ)
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.back_test_id.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.side.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .fetch_one(pool)
    .await
    .context("count backtest details")?;
    let items = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT to_jsonb(row)
        FROM (
            SELECT
                id,
                back_test_id,
                inst_id,
                time,
                strategy_type,
                option_type,
                signal_open_position_time,
                open_position_time,
                close_position_time,
                open_price,
                close_price,
                fee,
                profit_loss,
                quantity,
                full_close,
                close_type,
                signal_status,
                signal_value,
                signal_result,
                created_at,
                win_nums,
                loss_nums,
                stop_loss_source,
                stop_loss_update_history,
                'back_test_detail'::text AS source_table
            FROM back_test_detail
            WHERE ($1::TEXT IS NULL OR strategy_type ILIKE '%' || $1 || '%' OR inst_id ILIKE '%' || $1 || '%' OR time ILIKE '%' || $1 || '%' OR option_type ILIKE '%' || $1 || '%' OR close_type ILIKE '%' || $1 || '%')
              AND ($2::TEXT IS NULL OR LOWER(CAST(signal_status AS TEXT)) = $2)
              AND ($3::TEXT IS NULL OR CAST(back_test_id AS TEXT) = $3)
              AND ($4::TEXT IS NULL OR UPPER(inst_id) = $4)
              AND ($5::TEXT IS NULL OR LOWER(option_type) = $5)
              AND ($6::TEXT IS NULL OR created_at >= $6::TIMESTAMPTZ)
              AND ($7::TEXT IS NULL OR created_at <= $7::TIMESTAMPTZ)
            ORDER BY created_at DESC NULLS LAST, id DESC
            LIMIT $8 OFFSET $9
        ) row
        "#,
    )
    .bind(query.keyword.as_deref())
    .bind(query.status.as_deref())
    .bind(query.back_test_id.as_deref())
    .bind(query.symbol.as_deref())
    .bind(query.side.as_deref())
    .bind(query.start_time.as_deref())
    .bind(query.end_time.as_deref())
    .bind(query.page_size as i64)
    .bind(offset(query))
    .fetch_all(pool)
    .await
    .context("list backtest details")?;
    Ok(json!({ "items": items, "total": usize::try_from(total).unwrap_or(0) }))
}
/// 提供tableexists的集中实现，避免回测策略调用方重复处理相同细节。
async fn table_exists(pool: &PgPool, table_name: &str) -> Result<bool> {
    let regclass = sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass($1)::TEXT")
        .bind(format!("public.{table_name}"))
        .fetch_one(pool)
        .await
        .with_context(|| format!("check table exists: {table_name}"))?;
    Ok(regclass.is_some())
}
/// 提供optionaltext的集中实现，避免回测策略调用方重复处理相同细节。
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
fn offset(query: &BacktestDetailListQuery) -> i64 {
    ((query.page.saturating_sub(1)) * query.page_size) as i64
}
