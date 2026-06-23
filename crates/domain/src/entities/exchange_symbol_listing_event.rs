use serde::{Deserialize, Serialize};
/// 交易对在某交易所首次被系统发现的事实记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExchangeSymbolListingEvent {
    /// 唯一标识。
    pub id: Option<i64>,
    /// 交易所名称。
    pub exchange: String,
    /// 类型标识。
    pub market_type: String,
    /// 交易所交易对，用于当前结构体的业务数据。
    pub exchange_symbol: String,
    /// normalized交易对，用于当前结构体的业务数据。
    pub normalized_symbol: String,
    /// 基础资产，用于当前结构体的业务数据。
    pub base_asset: String,
    /// 计价资产，用于当前结构体的业务数据。
    pub quote_asset: String,
    /// 当前状态。
    pub status: String,
    /// 时间字段。
    pub first_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 数据来源。
    pub source: String,
    /// 原始 payload；为空时表示没有保留原始响应。
    pub raw_payload: Option<serde_json::Value>,
    /// 创建时间。
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后更新时间。
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl ExchangeSymbolListingEvent {
    /// 从外部输入转换为内部模型，隔离 量化核心 的字段适配细节。
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
