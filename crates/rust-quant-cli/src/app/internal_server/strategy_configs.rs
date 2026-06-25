use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{types::Json, PgPool, Postgres, QueryBuilder};
const MAX_STRATEGY_CONFIG_PAGE_SIZE: i64 = 200;
const DEFAULT_STRATEGY_CONFIG_PAGE_SIZE: i64 = 10;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyConfigListQuery {
    /// 页码。
    pub page: i64,
    /// 分页大小。
    pub page_size: i64,
    /// 关键词；为空时不做关键词过滤。
    pub keyword: Option<String>,
    /// 交易所名称。
    pub exchange: Option<String>,
    /// 交易对或资产符号。
    pub symbol: Option<String>,
    /// 是否启用。
    pub enabled: Option<bool>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyConfigUpsertRequest {
    /// legacy ID；为空时使用默认值或表示不限制。
    pub legacy_id: Option<i64>,
    /// 策略Key，用于构建接口请求。
    pub strategy_key: String,
    /// 策略名称。
    pub strategy_name: Option<String>,
    /// version；为空时表示该条件不启用。
    pub version: Option<String>,
    /// 交易所名称。
    pub exchange: Option<String>,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 周期。
    pub timeframe: String,
    /// 是否启用。
    pub enabled: bool,
    /// 运行配置。
    pub config: Value,
    /// 配置项。
    pub risk_config: Value,
    /// 展示风险等级；为空时由商品侧自行降级展示。
    pub risk_level: Option<String>,
    /// 策略简介。
    pub description: Option<String>,
    /// 策略详情。
    pub detail: Option<String>,
    /// 策略展示图。
    pub cover_image: Option<String>,
    /// 展示总收益率百分比。
    pub display_total_return_pct: Option<f64>,
    /// 展示夏普比率。
    pub display_sharpe_ratio: Option<f64>,
    /// 展示累计交易笔数。
    pub display_trade_count: Option<i32>,
    /// 展示最大回撤百分比。
    pub display_max_drawdown_pct: Option<f64>,
    /// updatedby；为空时表示该条件不启用。
    pub updated_by: Option<String>,
}
#[derive(Debug, Deserialize)]
struct RawStrategyConfigUpsertRequest {
    #[serde(rename = "legacyId", alias = "legacy_id")]
    /// legacy ID；为空时使用默认值或表示不限制。
    legacy_id: Option<i64>,
    #[serde(rename = "strategyKey", alias = "strategy_key")]
    /// 策略Key，用于构建接口请求。
    strategy_key: String,
    #[serde(rename = "strategyName", alias = "strategy_name")]
    /// 策略名称。
    strategy_name: Option<String>,
    /// version；为空时表示该条件不启用。
    version: Option<String>,
    /// 交易所名称。
    exchange: Option<String>,
    /// 交易对或资产符号。
    symbol: String,
    /// 周期。
    timeframe: String,
    #[serde(default = "default_enabled")]
    /// 是否启用。
    enabled: bool,
    #[serde(default)]
    /// 运行配置。
    config: Value,
    #[serde(rename = "riskConfig", alias = "risk_config", default)]
    /// 配置项。
    risk_config: Value,
    #[serde(rename = "riskLevel", alias = "risk_level")]
    /// 展示风险等级；为空时由商品侧自行降级展示。
    risk_level: Option<String>,
    /// 策略简介。
    description: Option<String>,
    /// 策略详情。
    detail: Option<String>,
    #[serde(rename = "coverImage", alias = "cover_image")]
    /// 策略展示图。
    cover_image: Option<String>,
    #[serde(rename = "displayTotalReturnPct", alias = "display_total_return_pct")]
    /// 展示总收益率百分比。
    display_total_return_pct: Option<f64>,
    #[serde(rename = "displaySharpeRatio", alias = "display_sharpe_ratio")]
    /// 展示夏普比率。
    display_sharpe_ratio: Option<f64>,
    #[serde(rename = "displayTradeCount", alias = "display_trade_count")]
    /// 展示累计交易笔数。
    display_trade_count: Option<i32>,
    #[serde(rename = "displayMaxDrawdownPct", alias = "display_max_drawdown_pct")]
    /// 展示最大回撤百分比。
    display_max_drawdown_pct: Option<f64>,
    #[serde(rename = "updatedBy", alias = "updated_by")]
    /// updatedby；为空时表示该条件不启用。
    updated_by: Option<String>,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
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
/// 提供策略配置upsertrequestfrom请求体的集中实现，避免回测策略调用方重复处理相同细节。
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
        risk_level: optional_text(raw.risk_level),
        description: optional_text(raw.description),
        detail: optional_text(raw.detail),
        cover_image: optional_text(raw.cover_image),
        display_total_return_pct: raw.display_total_return_pct,
        display_sharpe_ratio: raw.display_sharpe_ratio,
        display_trade_count: raw.display_trade_count,
        display_max_drawdown_pct: raw.display_max_drawdown_pct,
        updated_by: optional_text(raw.updated_by),
    })
}
pub fn strategy_config_risk_config_update_value(
    request: &StrategyConfigUpsertRequest,
) -> Option<&Value> {
    if request.risk_config.is_null() {
        None
    } else {
        Some(&request.risk_config)
    }
}
/// 创建 回测与策略研究 资源，并在入口处完成必要的参数归一。
pub(super) fn create_quant_core_internal_pool() -> Result<PgPool> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context(
            "QUANT_CORE_DATABASE_URL or POSTGRES_QUANT_CORE_DATABASE_URL is required for quant_core internal APIs",
        )?;
    PgPoolOptions::new()
        .max_connections(3)
        .connect_lazy(&database_url)
        .context("create quant_core internal database pool")
}
/// 持久化 回测与策略研究 结果，保证写入路径和幂等语义集中处理。
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
    let risk_config = strategy_config_risk_config_update_value(request)
        .cloned()
        .map(Json);
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
            risk_level,
            description,
            detail,
            cover_image,
            display_total_return_pct,
            display_sharpe_ratio,
            display_trade_count,
            display_max_drawdown_pct,
            created_by,
            updated_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, COALESCE($10::jsonb, '{}'::jsonb), $11, $12, $13, $14, $15, $16, $17, $18, $19, $19)
        ON CONFLICT (strategy_key, version, exchange, symbol, timeframe)
        DO UPDATE SET
            legacy_id = EXCLUDED.legacy_id,
            strategy_name = EXCLUDED.strategy_name,
            enabled = EXCLUDED.enabled,
            config = EXCLUDED.config,
            risk_config = COALESCE($10::jsonb, strategy_configs.risk_config),
            risk_level = EXCLUDED.risk_level,
            description = EXCLUDED.description,
            detail = EXCLUDED.detail,
            cover_image = EXCLUDED.cover_image,
            display_total_return_pct = EXCLUDED.display_total_return_pct,
            display_sharpe_ratio = EXCLUDED.display_sharpe_ratio,
            display_trade_count = EXCLUDED.display_trade_count,
            display_max_drawdown_pct = EXCLUDED.display_max_drawdown_pct,
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
    .bind(risk_config)
    .bind(request.risk_level.as_deref())
    .bind(request.description.as_deref())
    .bind(request.detail.as_deref())
    .bind(request.cover_image.as_deref())
    .bind(request.display_total_return_pct)
    .bind(request.display_sharpe_ratio)
    .bind(request.display_trade_count)
    .bind(request.display_max_drawdown_pct)
    .bind(request.updated_by.as_deref())
    .fetch_one(pool)
    .await?;
    Ok(row.0 .0)
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
/// 把数据加入 回测与策略研究 聚合结果，保持集合构造逻辑集中。
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
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_optional_enabled(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "enabled" | "active" => Some(true),
        "false" | "0" | "disabled" | "inactive" => Some(false),
        _ => None,
    }
}
/// 封装必需text，减少回测策略调用方重复实现相同细节。
fn required_text(value: String, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}
/// 提供optionaltext的集中实现，避免回测策略调用方重复处理相同细节。
fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
fn default_enabled() -> bool {
    true
}
