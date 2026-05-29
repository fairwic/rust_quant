use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

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

/// 市场排名价格快照，用于重启后恢复排名历史和价格对比证据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRankSnapshot {
    pub id: Option<i64>,
    pub exchange: String,
    pub symbol: String,
    pub rank: i32,
    pub price: Decimal,
    pub volume_24h_quote: Decimal,
    pub captured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// 市场排名事件的技术指标快照，用于前端快速展示排名事件发生时的均线证据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketRankTechnicalSnapshot {
    pub timeframe: String,
    pub period: i32,
    pub close_price: Decimal,
    pub ma_value: Decimal,
    pub ema_value: Decimal,
    pub ma_distance_pct: Decimal,
    pub ema_distance_pct: Decimal,
    pub ma_state: String,
    pub ema_state: String,
    pub candle_count: i32,
    pub snapshot_at: DateTime<Utc>,
}

/// 市场排名事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketRankEventType {
    RankVelocity,
    TopEntry,
    TopExit,
}

impl MarketRankEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RankVelocity => "rank_velocity",
            Self::TopEntry => "top_entry",
            Self::TopExit => "top_exit",
        }
    }
}

impl TryFrom<&str> for MarketRankEventType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "rank_velocity" => Ok(Self::RankVelocity),
            "top_entry" => Ok(Self::TopEntry),
            "top_exit" => Ok(Self::TopExit),
            other => Err(anyhow::anyhow!("unknown market rank event type: {}", other)),
        }
    }
}

impl fmt::Display for MarketRankEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 市场排名事件流水，用于产品时间线、通知和 Admin 诊断
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRankEvent {
    pub id: Option<i64>,
    pub exchange: String,
    pub symbol: String,
    pub event_type: MarketRankEventType,
    pub timeframe: Option<String>,
    pub old_rank: Option<i32>,
    pub new_rank: Option<i32>,
    pub delta_rank: Option<i32>,
    pub volume_24h_quote: Option<Decimal>,
    pub current_price: Option<Decimal>,
    pub previous_price: Option<Decimal>,
    pub price_change_pct: Option<Decimal>,
    pub price_direction: String,
    pub technical_snapshot_status: String,
    pub technical_snapshot: Option<MarketRankTechnicalSnapshot>,
    pub detected_at: DateTime<Utc>,
    pub source: String,
    pub notification_state: String,
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
