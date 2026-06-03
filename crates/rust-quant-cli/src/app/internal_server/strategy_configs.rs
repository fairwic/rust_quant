use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{types::Json, PgPool, Postgres, QueryBuilder};

const MAX_STRATEGY_CONFIG_PAGE_SIZE: i64 = 200;
const DEFAULT_STRATEGY_CONFIG_PAGE_SIZE: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyConfigListQuery {
    pub page: i64,
    pub page_size: i64,
    pub keyword: Option<String>,
    pub exchange: Option<String>,
    pub symbol: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrategyConfigUpsertRequest {
    pub legacy_id: Option<i64>,
    pub strategy_key: String,
    pub strategy_name: Option<String>,
    pub version: Option<String>,
    pub exchange: Option<String>,
    pub symbol: String,
    pub timeframe: String,
    pub enabled: bool,
    pub config: Value,
    pub risk_config: Value,
    pub updated_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawStrategyConfigUpsertRequest {
    #[serde(rename = "legacyId", alias = "legacy_id")]
    legacy_id: Option<i64>,
    #[serde(rename = "strategyKey", alias = "strategy_key")]
    strategy_key: String,
    #[serde(rename = "strategyName", alias = "strategy_name")]
    strategy_name: Option<String>,
    version: Option<String>,
    exchange: Option<String>,
    symbol: String,
    timeframe: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    config: Value,
    #[serde(rename = "riskConfig", alias = "risk_config", default)]
    risk_config: Value,
    #[serde(rename = "updatedBy", alias = "updated_by")]
    updated_by: Option<String>,
}

pub fn strategy_config_list_query_from_path(path: &str) -> Result<StrategyConfigListQuery, String> {
    let query = path.split_once('?').map(|(_, query)| query).unwrap_or("");
    let page = query_param(query, &["page"])
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = query_param(query, &["pageSize", "page_size", "limit"])
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_STRATEGY_CONFIG_PAGE_SIZE)
        .clamp(1, MAX_STRATEGY_CONFIG_PAGE_SIZE);
    let keyword = query_param(query, &["keyword", "q"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let exchange = query_param(query, &["exchange"])
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let symbol = query_param(query, &["symbol"])
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty());
    let enabled = query_param(query, &["enabled", "status"])
        .as_deref()
        .and_then(parse_optional_enabled);

    Ok(StrategyConfigListQuery {
        page,
        page_size,
        keyword,
        exchange,
        symbol,
        enabled,
    })
}

pub fn strategy_config_upsert_request_from_body(
    body: &[u8],
) -> Result<StrategyConfigUpsertRequest, String> {
    let raw = serde_json::from_slice::<RawStrategyConfigUpsertRequest>(body)
        .map_err(|error| format!("invalid json body: {error}"))?;
    let strategy_key = required_text(raw.strategy_key, "strategyKey")?;
    let symbol = required_text(raw.symbol, "symbol")?.to_ascii_uppercase();
    let timeframe = required_text(raw.timeframe, "timeframe")?;

    Ok(StrategyConfigUpsertRequest {
        legacy_id: raw.legacy_id,
        strategy_key,
        strategy_name: optional_text(raw.strategy_name),
        version: optional_text(raw.version),
        exchange: optional_text(raw.exchange).map(|value| value.to_ascii_lowercase()),
        symbol,
        timeframe,
        enabled: raw.enabled,
        config: raw.config,
        risk_config: raw.risk_config,
        updated_by: optional_text(raw.updated_by),
    })
}

pub(super) fn create_quant_core_internal_pool() -> Result<PgPool> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("QUANT_CORE_DATABASE_URL is required for quant_core internal APIs")?;
    PgPoolOptions::new()
        .max_connections(3)
        .connect_lazy(&database_url)
        .context("create quant_core internal database pool")
}

pub(super) async fn upsert_strategy_config_response(
    pool: &PgPool,
    request: &StrategyConfigUpsertRequest,
) -> Result<Value> {
    let version = request.version.as_deref().unwrap_or("default");
    let exchange = request.exchange.as_deref().unwrap_or("all");
    let strategy_name = request
        .strategy_name
        .as_deref()
        .unwrap_or(request.strategy_key.as_str());
    let row: (Json<Value>,) = sqlx::query_as(
        r#"
        INSERT INTO strategy_configs (
            legacy_id,
            strategy_key,
            strategy_name,
            version,
            exchange,
            symbol,
            timeframe,
            enabled,
            config,
            risk_config,
            created_by,
            updated_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
        ON CONFLICT (strategy_key, version, exchange, symbol, timeframe)
        DO UPDATE SET
            legacy_id = EXCLUDED.legacy_id,
            strategy_name = EXCLUDED.strategy_name,
            enabled = EXCLUDED.enabled,
            config = EXCLUDED.config,
            risk_config = EXCLUDED.risk_config,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        RETURNING to_jsonb(strategy_configs) AS row
        "#,
    )
    .bind(request.legacy_id)
    .bind(&request.strategy_key)
    .bind(strategy_name)
    .bind(version)
    .bind(exchange)
    .bind(&request.symbol)
    .bind(&request.timeframe)
    .bind(request.enabled)
    .bind(Json(request.config.clone()))
    .bind(Json(request.risk_config.clone()))
    .bind(request.updated_by.as_deref())
    .fetch_one(pool)
    .await?;

    Ok(row.0 .0)
}

pub(super) async fn fetch_strategy_config_list_response(
    pool: &PgPool,
    query: &StrategyConfigListQuery,
) -> Result<(Vec<Value>, i64)> {
    let offset = (query.page - 1) * query.page_size;
    let mut data_builder = QueryBuilder::<Postgres>::new(
        "SELECT to_jsonb(strategy_configs) AS row FROM strategy_configs WHERE 1=1",
    );
    push_strategy_config_filters(&mut data_builder, query);
    data_builder
        .push(" ORDER BY updated_at DESC NULLS LAST, id DESC LIMIT ")
        .push_bind(query.page_size)
        .push(" OFFSET ")
        .push_bind(offset);

    let mut count_builder =
        QueryBuilder::<Postgres>::new("SELECT COUNT(*)::bigint FROM strategy_configs WHERE 1=1");
    push_strategy_config_filters(&mut count_builder, query);

    let total: (i64,) = count_builder.build_query_as().fetch_one(pool).await?;
    let rows: Vec<(Json<Value>,)> = data_builder.build_query_as().fetch_all(pool).await?;
    let items = rows.into_iter().map(|(row,)| row.0).collect();
    Ok((items, total.0))
}

fn push_strategy_config_filters(
    builder: &mut QueryBuilder<Postgres>,
    query: &StrategyConfigListQuery,
) {
    if let Some(keyword) = query.keyword.as_deref() {
        let pattern = format!("%{}%", keyword);
        builder
            .push(" AND to_jsonb(strategy_configs)::TEXT ILIKE ")
            .push_bind(pattern);
    }
    if let Some(exchange) = query.exchange.as_deref() {
        builder
            .push(" AND LOWER(exchange) = LOWER(")
            .push_bind(exchange.to_string())
            .push(")");
    }
    if let Some(symbol) = query.symbol.as_deref() {
        builder
            .push(" AND UPPER(symbol) = UPPER(")
            .push_bind(symbol.to_string())
            .push(")");
    }
    if let Some(enabled) = query.enabled {
        builder.push(" AND enabled = ").push_bind(enabled);
    }
}

fn query_param(query: &str, names: &[&str]) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(raw_name, raw_value)| {
            let name = raw_name.trim();
            if names.iter().any(|candidate| name == *candidate) {
                Some(raw_value.trim().replace('+', " "))
            } else {
                None
            }
        })
}

fn parse_optional_enabled(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "enabled" | "active" => Some(true),
        "false" | "0" | "disabled" | "inactive" => Some(false),
        _ => None,
    }
}

fn required_text(value: String, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_enabled() -> bool {
    true
}
