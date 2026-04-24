use serde::{Deserialize, Serialize};

/// 交易所可交易交易对事实记录
///
/// 用于存储交易所原始可交易 symbol 元数据，供 quant_core 作为事实源统一管理。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExchangeSymbol {
    pub id: Option<i64>,
    pub exchange: String,
    pub market_type: String,
    pub exchange_symbol: String,
    pub normalized_symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
    pub contract_type: Option<String>,
    pub price_precision: Option<i32>,
    pub quantity_precision: Option<i32>,
    pub min_qty: Option<String>,
    pub max_qty: Option<String>,
    pub tick_size: Option<String>,
    pub step_size: Option<String>,
    pub min_notional: Option<String>,
    pub raw_payload: Option<serde_json::Value>,
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ExchangeSymbol {
    pub fn new(
        exchange: String,
        market_type: String,
        exchange_symbol: String,
        normalized_symbol: String,
        base_asset: String,
        quote_asset: String,
        status: String,
    ) -> Self {
        Self {
            id: None,
            exchange,
            market_type,
            exchange_symbol,
            normalized_symbol,
            base_asset,
            quote_asset,
            status,
            contract_type: None,
            price_precision: None,
            quantity_precision: None,
            min_qty: None,
            max_qty: None,
            tick_size: None,
            step_size: None,
            min_notional: None,
            raw_payload: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
        }
    }
}
