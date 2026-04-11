use serde::{Deserialize, Serialize};

/// 外部市场数据快照
///
/// 用于统一存储交易所/链上/第三方数据源在某个时间点的特征值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMarketSnapshot {
    /// 自增ID
    pub id: Option<i64>,
    /// 数据来源，如 hyperliquid / okx / binance / dune
    pub source: String,
    /// 标的符号，统一使用基础币，如 ETH / BTC
    pub symbol: String,
    /// 指标类型，如 funding / meta / open_interest / long_short_ratio / onchain_flow
    pub metric_type: String,
    /// 指标时间（Unix时间戳，毫秒）
    pub metric_time: i64,
    /// 资金费率
    pub funding_rate: Option<f64>,
    /// 溢价
    pub premium: Option<f64>,
    /// 持仓量
    pub open_interest: Option<f64>,
    /// 预言机价格
    pub oracle_price: Option<f64>,
    /// 标记价格
    pub mark_price: Option<f64>,
    /// 多空比
    pub long_short_ratio: Option<f64>,
    /// 原始响应，用于调试和二次开发
    pub raw_payload: Option<serde_json::Value>,
    /// 创建时间
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 更新时间
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ExternalMarketSnapshot {
    pub fn new(source: String, symbol: String, metric_type: String, metric_time: i64) -> Self {
        Self {
            id: None,
            source,
            symbol,
            metric_type,
            metric_time,
            funding_rate: None,
            premium: None,
            open_interest: None,
            oracle_price: None,
            mark_price: None,
            long_short_ratio: None,
            raw_payload: None,
            created_at: None,
            updated_at: None,
        }
    }
}
