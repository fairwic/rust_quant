use serde::{Deserialize, Serialize};

/// 交易对在某交易所首次被系统发现的事实记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExchangeSymbolListingEvent {
    pub id: Option<i64>,
    pub exchange: String,
    pub market_type: String,
    pub exchange_symbol: String,
    pub normalized_symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
    pub first_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub source: String,
    pub raw_payload: Option<serde_json::Value>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ExchangeSymbolListingEvent {
    pub fn from_exchange_symbol(symbol: &super::ExchangeSymbol, source: impl Into<String>) -> Self {
        Self {
            id: None,
            exchange: symbol.exchange.clone(),
            market_type: symbol.market_type.clone(),
            exchange_symbol: symbol.exchange_symbol.clone(),
            normalized_symbol: symbol.normalized_symbol.clone(),
            base_asset: symbol.base_asset.clone(),
            quote_asset: symbol.quote_asset.clone(),
            status: symbol.status.clone(),
            first_seen_at: None,
            source: source.into(),
            raw_payload: symbol.raw_payload.clone(),
            created_at: None,
            updated_at: None,
        }
    }
}
