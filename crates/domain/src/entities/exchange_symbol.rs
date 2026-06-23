use serde::{Deserialize, Serialize};
/// 交易所可交易交易对事实记录
///
/// 用于存储交易所原始可交易 symbol 元数据，供 quant_core 作为事实源统一管理。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExchangeSymbol {
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
    /// 类型标识。
    pub contract_type: Option<String>,
    /// 价格精度；为空时使用交易所默认值。
    pub price_precision: Option<i32>,
    /// 数量精度；为空时使用交易所默认值。
    pub quantity_precision: Option<i32>,
    /// 数量数值。
    pub min_qty: Option<String>,
    /// 数量数值。
    pub max_qty: Option<String>,
    /// 数量数值。
    pub tick_size: Option<String>,
    /// 数量数值。
    pub step_size: Option<String>,
    /// 最小名义金额；为空时使用交易所默认值。
    pub min_notional: Option<String>,
    /// 原始 payload；为空时表示没有保留原始响应。
    pub raw_payload: Option<serde_json::Value>,
    /// 时间字段。
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 创建时间。
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后更新时间。
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl ExchangeSymbol {
    /// 构建 量化核心 所需实例，并集中初始化依赖和默认状态。
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
