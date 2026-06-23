use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_quant_domain::entities::{
    exchange_symbol::ExchangeSymbol, exchange_symbol_listing_event::ExchangeSymbolListingEvent,
};
use rust_quant_domain::traits::exchange_symbol_repository::ExchangeSymbolRepository;
use serde_json::Value;
use sqlx::{postgres::PgRow, FromRow, PgPool, Row};
#[derive(Debug, Clone)]
struct ExchangeSymbolRow {
    /// 唯一标识。
    id: i64,
    /// 交易所名称。
    exchange: String,
    /// 类型标识。
    market_type: String,
    /// 交易所交易对，用于展示或持久化查询结果。
    exchange_symbol: String,
    /// normalized交易对，用于展示或持久化查询结果。
    normalized_symbol: String,
    /// 基础资产，用于展示或持久化查询结果。
    base_asset: String,
    /// 计价资产，用于展示或持久化查询结果。
    quote_asset: String,
    /// 当前状态。
    status: String,
    /// 类型标识。
    contract_type: Option<String>,
    /// 价格精度；为空时使用交易所默认值。
    price_precision: Option<i32>,
    /// 数量精度；为空时使用交易所默认值。
    quantity_precision: Option<i32>,
    /// 数量数值。
    min_qty: Option<String>,
    /// 数量数值。
    max_qty: Option<String>,
    /// 数量数值。
    tick_size: Option<String>,
    /// 数量数值。
    step_size: Option<String>,
    /// 最小名义金额；为空时使用交易所默认值。
    min_notional: Option<String>,
    /// 原始 payload；为空时表示没有保留原始响应。
    raw_payload: Option<Value>,
    /// 时间字段。
    last_synced_at: chrono::DateTime<chrono::Utc>,
    /// 创建时间。
    created_at: chrono::DateTime<chrono::Utc>,
    /// 最后更新时间。
    updated_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Clone)]
struct ExchangeSymbolListingEventRow {
    /// 唯一标识。
    id: i64,
    /// 交易所名称。
    exchange: String,
    /// 类型标识。
    market_type: String,
    /// 交易所交易对，用于展示或持久化查询结果。
    exchange_symbol: String,
    /// normalized交易对，用于展示或持久化查询结果。
    normalized_symbol: String,
    /// 基础资产，用于展示或持久化查询结果。
    base_asset: String,
    /// 计价资产，用于展示或持久化查询结果。
    quote_asset: String,
    /// 当前状态。
    status: String,
    /// 时间字段。
    first_seen_at: chrono::DateTime<chrono::Utc>,
    /// 数据来源。
    source: String,
    /// 原始 payload；为空时表示没有保留原始响应。
    raw_payload: Option<Value>,
    /// 创建时间。
    created_at: chrono::DateTime<chrono::Utc>,
    /// 最后更新时间。
    updated_at: chrono::DateTime<chrono::Utc>,
}
impl<'r> FromRow<'r, PgRow> for ExchangeSymbolRow {
    /// 从外部输入转换为内部模型，隔离 配置、基础设施和运行时 的字段适配细节。
    fn from_row(row: &'r PgRow) -> std::result::Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            exchange: row.try_get("exchange")?,
            market_type: row.try_get("market_type")?,
            exchange_symbol: row.try_get("exchange_symbol")?,
            normalized_symbol: row.try_get("normalized_symbol")?,
            base_asset: row.try_get("base_asset")?,
            quote_asset: row.try_get("quote_asset")?,
            status: row.try_get("status")?,
            contract_type: row.try_get("contract_type")?,
            price_precision: row.try_get("price_precision")?,
            quantity_precision: row.try_get("quantity_precision")?,
            min_qty: row.try_get("min_qty")?,
            max_qty: row.try_get("max_qty")?,
            tick_size: row.try_get("tick_size")?,
            step_size: row.try_get("step_size")?,
            min_notional: row.try_get("min_notional")?,
            raw_payload: row.try_get("raw_payload")?,
            last_synced_at: row.try_get("last_synced_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}
impl ExchangeSymbolRow {
    /// 将内部模型转换为输出结构，避免 配置、基础设施和运行时 的内部字段直接外泄。
    fn into_domain(self) -> ExchangeSymbol {
        ExchangeSymbol {
            id: Some(self.id),
            exchange: self.exchange,
            market_type: self.market_type,
            exchange_symbol: self.exchange_symbol,
            normalized_symbol: self.normalized_symbol,
            base_asset: self.base_asset,
            quote_asset: self.quote_asset,
            status: self.status,
            contract_type: self.contract_type,
            price_precision: self.price_precision,
            quantity_precision: self.quantity_precision,
            min_qty: self.min_qty,
            max_qty: self.max_qty,
            tick_size: self.tick_size,
            step_size: self.step_size,
            min_notional: self.min_notional,
            raw_payload: self.raw_payload,
            last_synced_at: Some(self.last_synced_at),
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
        }
    }
}
impl<'r> FromRow<'r, PgRow> for ExchangeSymbolListingEventRow {
    /// 从外部输入转换为内部模型，隔离 配置、基础设施和运行时 的字段适配细节。
    fn from_row(row: &'r PgRow) -> std::result::Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            exchange: row.try_get("exchange")?,
            market_type: row.try_get("market_type")?,
            exchange_symbol: row.try_get("exchange_symbol")?,
            normalized_symbol: row.try_get("normalized_symbol")?,
            base_asset: row.try_get("base_asset")?,
            quote_asset: row.try_get("quote_asset")?,
            status: row.try_get("status")?,
            first_seen_at: row.try_get("first_seen_at")?,
            source: row.try_get("source")?,
            raw_payload: row.try_get("raw_payload")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}
impl ExchangeSymbolListingEventRow {
    /// 将内部模型转换为输出结构，避免 配置、基础设施和运行时 的内部字段直接外泄。
    fn into_domain(self) -> ExchangeSymbolListingEvent {
        ExchangeSymbolListingEvent {
            id: Some(self.id),
            exchange: self.exchange,
            market_type: self.market_type,
            exchange_symbol: self.exchange_symbol,
            normalized_symbol: self.normalized_symbol,
            base_asset: self.base_asset,
            quote_asset: self.quote_asset,
            status: self.status,
            first_seen_at: Some(self.first_seen_at),
            source: self.source,
            raw_payload: self.raw_payload,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
        }
    }
}
pub struct PostgresExchangeSymbolRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl PostgresExchangeSymbolRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl ExchangeSymbolRepository for PostgresExchangeSymbolRepository {
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    async fn upsert_many(&self, symbols: Vec<ExchangeSymbol>) -> Result<u64> {
        let mut affected = 0u64;
        for symbol in symbols {
            affected += sqlx::query(
                r#"
                INSERT INTO exchange_symbols (
                    exchange,
                    market_type,
                    exchange_symbol,
                    normalized_symbol,
                    base_asset,
                    quote_asset,
                    status,
                    contract_type,
                    price_precision,
                    quantity_precision,
                    min_qty,
                    max_qty,
                    tick_size,
                    step_size,
                    min_notional,
                    raw_payload,
                    last_synced_at
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, NOW()
                )
                ON CONFLICT (exchange, market_type, exchange_symbol)
                DO UPDATE SET
                    normalized_symbol = EXCLUDED.normalized_symbol,
                    base_asset = EXCLUDED.base_asset,
                    quote_asset = EXCLUDED.quote_asset,
                    status = EXCLUDED.status,
                    contract_type = EXCLUDED.contract_type,
                    price_precision = EXCLUDED.price_precision,
                    quantity_precision = EXCLUDED.quantity_precision,
                    min_qty = EXCLUDED.min_qty,
                    max_qty = EXCLUDED.max_qty,
                    tick_size = EXCLUDED.tick_size,
                    step_size = EXCLUDED.step_size,
                    min_notional = EXCLUDED.min_notional,
                    raw_payload = EXCLUDED.raw_payload,
                    last_synced_at = NOW(),
                    updated_at = NOW()
                "#,
            )
            .bind(symbol.exchange)
            .bind(symbol.market_type)
            .bind(symbol.exchange_symbol)
            .bind(symbol.normalized_symbol)
            .bind(symbol.base_asset)
            .bind(symbol.quote_asset)
            .bind(symbol.status)
            .bind(symbol.contract_type)
            .bind(symbol.price_precision)
            .bind(symbol.quantity_precision)
            .bind(symbol.min_qty)
            .bind(symbol.max_qty)
            .bind(symbol.tick_size)
            .bind(symbol.step_size)
            .bind(symbol.min_notional)
            .bind(symbol.raw_payload)
            .execute(&self.pool)
            .await
            .context("upsert exchange_symbols")?
            .rows_affected();
        }
        Ok(affected)
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_exchange(
        &self,
        exchange: &str,
        status: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ExchangeSymbol>> {
        let rows = if let Some(status) = status {
            sqlx::query_as::<_, ExchangeSymbolRow>(
                r#"
                SELECT
                    id,
                    exchange,
                    market_type,
                    exchange_symbol,
                    normalized_symbol,
                    base_asset,
                    quote_asset,
                    status,
                    contract_type,
                    price_precision,
                    quantity_precision,
                    min_qty,
                    max_qty,
                    tick_size,
                    step_size,
                    min_notional,
                    raw_payload,
                    last_synced_at,
                    created_at,
                    updated_at
                FROM exchange_symbols
                WHERE exchange = $1
                  AND status = $2
                ORDER BY updated_at DESC
                LIMIT $3
                "#,
            )
            .bind(exchange)
            .bind(status)
            .bind(limit.unwrap_or(500))
            .fetch_all(&self.pool)
            .await
            .context("query exchange_symbols by exchange/status")?
        } else {
            sqlx::query_as::<_, ExchangeSymbolRow>(
                r#"
                SELECT
                    id,
                    exchange,
                    market_type,
                    exchange_symbol,
                    normalized_symbol,
                    base_asset,
                    quote_asset,
                    status,
                    contract_type,
                    price_precision,
                    quantity_precision,
                    min_qty,
                    max_qty,
                    tick_size,
                    step_size,
                    min_notional,
                    raw_payload,
                    last_synced_at,
                    created_at,
                    updated_at
                FROM exchange_symbols
                WHERE exchange = $1
                ORDER BY updated_at DESC
                LIMIT $2
                "#,
            )
            .bind(exchange)
            .bind(limit.unwrap_or(500))
            .fetch_all(&self.pool)
            .await
            .context("query exchange_symbols by exchange")?
        };
        Ok(rows
            .into_iter()
            .map(ExchangeSymbolRow::into_domain)
            .collect())
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbol>> {
        let rows = sqlx::query_as::<_, ExchangeSymbolRow>(
            r#"
            SELECT
                id,
                exchange,
                market_type,
                exchange_symbol,
                normalized_symbol,
                base_asset,
                quote_asset,
                status,
                contract_type,
                price_precision,
                quantity_precision,
                min_qty,
                max_qty,
                tick_size,
                step_size,
                min_notional,
                raw_payload,
                last_synced_at,
                created_at,
                updated_at
            FROM exchange_symbols
            WHERE base_asset = $1
              AND quote_asset = $2
              AND market_type = $3
            ORDER BY updated_at DESC, id ASC
            "#,
        )
        .bind(base_asset.trim().to_ascii_uppercase())
        .bind(quote_asset.trim().to_ascii_uppercase())
        .bind(market_type.trim().to_ascii_lowercase())
        .fetch_all(&self.pool)
        .await
        .context("query exchange_symbols by asset")?;
        Ok(rows
            .into_iter()
            .map(ExchangeSymbolRow::into_domain)
            .collect())
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn record_first_seen_many(
        &self,
        symbols: &[ExchangeSymbol],
    ) -> Result<Vec<ExchangeSymbolListingEvent>> {
        let mut inserted = Vec::new();
        for symbol in symbols {
            let row = sqlx::query_as::<_, ExchangeSymbolListingEventRow>(
                r#"
                INSERT INTO exchange_symbol_listing_events (
                    exchange,
                    market_type,
                    exchange_symbol,
                    normalized_symbol,
                    base_asset,
                    quote_asset,
                    status,
                    source,
                    raw_payload
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, 'exchange_symbol_sync', $8)
                ON CONFLICT (exchange, market_type, exchange_symbol) DO NOTHING
                RETURNING
                    id,
                    exchange,
                    market_type,
                    exchange_symbol,
                    normalized_symbol,
                    base_asset,
                    quote_asset,
                    status,
                    first_seen_at,
                    source,
                    raw_payload,
                    created_at,
                    updated_at
                "#,
            )
            .bind(&symbol.exchange)
            .bind(&symbol.market_type)
            .bind(&symbol.exchange_symbol)
            .bind(&symbol.normalized_symbol)
            .bind(&symbol.base_asset)
            .bind(&symbol.quote_asset)
            .bind(&symbol.status)
            .bind(&symbol.raw_payload)
            .fetch_optional(&self.pool)
            .await
            .context("record first seen exchange_symbol_listing_events")?;
            if let Some(row) = row {
                inserted.push(row.into_domain());
            }
        }
        Ok(inserted)
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_listing_events_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbolListingEvent>> {
        let rows = sqlx::query_as::<_, ExchangeSymbolListingEventRow>(
            r#"
            SELECT
                id,
                exchange,
                market_type,
                exchange_symbol,
                normalized_symbol,
                base_asset,
                quote_asset,
                status,
                first_seen_at,
                source,
                raw_payload,
                created_at,
                updated_at
            FROM exchange_symbol_listing_events
            WHERE base_asset = $1
              AND quote_asset = $2
              AND market_type = $3
            ORDER BY first_seen_at ASC, id ASC
            "#,
        )
        .bind(base_asset.trim().to_ascii_uppercase())
        .bind(quote_asset.trim().to_ascii_uppercase())
        .bind(market_type.trim().to_ascii_lowercase())
        .fetch_all(&self.pool)
        .await
        .context("query exchange_symbol_listing_events by asset")?;
        Ok(rows
            .into_iter()
            .map(ExchangeSymbolListingEventRow::into_domain)
            .collect())
    }
}
