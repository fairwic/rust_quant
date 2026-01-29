use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 市场快照 (用于扫描器)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickerSnapshot {
    pub symbol: String,
    pub price: Decimal,
    pub volume_24h_base: Decimal,
    pub volume_24h_quote: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 资金流向分类
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FundFlowSide {
    Inflow,  // 主动买入
    Outflow, // 主动卖出
}

/// 资金流入数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundFlow {
    pub symbol: String,
    pub side: FundFlowSide,
    pub value: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 市场异动记录 (Top 150 币种，按 symbol 唯一)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketAnomaly {
    pub id: Option<i64>,
    pub symbol: String,
    pub current_rank: i32,
    pub rank_15m_ago: Option<i32>,
    pub rank_4h_ago: Option<i32>,
    pub rank_24h_ago: Option<i32>,
    pub delta_15m: Option<i32>,
    pub delta_4h: Option<i32>,
    pub delta_24h: Option<i32>,
    pub volume_24h: Option<Decimal>,
    pub updated_at: DateTime<Utc>,
    pub status: String, // ACTIVE, EXITED
}

/// 资金流向报警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundFlowAlert {
    pub id: Option<i64>,
    pub symbol: String,
    pub net_inflow: Decimal,
    pub total_volume: Decimal,
    pub side: FundFlowSide,
    pub window_secs: i32,
    pub alert_at: DateTime<Utc>,
}
