use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
/// 市场快照 (用于扫描器)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickerSnapshot {
    /// 交易对或资产符号。
    pub symbol: String,
    /// 价格。
    pub price: Decimal,
    /// 24 小时基础币成交量。
    pub volume_24h_base: Decimal,
    /// 24 小时计价币成交额。
    pub volume_24h_quote: Decimal,
    /// 事件时间戳。
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
    /// 交易对或资产符号。
    pub symbol: String,
    /// 交易方向。
    pub side: FundFlowSide,
    /// 值。
    pub value: Decimal,
    /// 事件时间戳。
    pub timestamp: DateTime<Utc>,
}
/// 市场异动记录 (Top 150 币种，按 symbol 唯一)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketAnomaly {
    /// 唯一标识。
    pub id: Option<i64>,
    /// 交易对或资产符号。
    pub symbol: String,
    /// current排名，用于当前结构体的业务数据。
    pub current_rank: i32,
    /// 15 分钟前排名；为空时表示没有历史排名。
    pub rank_15m_ago: Option<i32>,
    /// 4 小时前排名；为空时表示没有历史排名。
    pub rank_4h_ago: Option<i32>,
    /// 24 小时前排名；为空时表示没有历史排名。
    pub rank_24h_ago: Option<i32>,
    /// 变化15 分钟；为空时表示该条件不启用。
    pub delta_15m: Option<i32>,
    /// 变化4 小时；为空时表示该条件不启用。
    pub delta_4h: Option<i32>,
    /// 变化24 小时；为空时表示该条件不启用。
    pub delta_24h: Option<i32>,
    /// 成交量24 小时；为空时表示该条件不启用。
    pub volume_24h: Option<Decimal>,
    /// 最后更新时间。
    pub updated_at: DateTime<Utc>,
    pub status: String, // ACTIVE, EXITED
}
/// 市场排名价格快照，用于重启后恢复排名历史和价格对比证据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRankSnapshot {
    /// 唯一标识。
    pub id: Option<i64>,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 排名。
    pub rank: i32,
    /// 价格。
    pub price: Decimal,
    /// 24 小时计价币成交额。
    pub volume_24h_quote: Decimal,
    /// 时间字段。
    pub captured_at: DateTime<Utc>,
    /// 创建时间。
    pub created_at: DateTime<Utc>,
}
/// 市场排名事件的技术指标快照，用于前端快速展示排名事件发生时的均线证据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketRankTechnicalSnapshot {
    /// 周期。
    pub timeframe: String,
    /// 计算周期。
    pub period: i32,
    /// 离场价格。
    pub close_price: Decimal,
    /// MA 指标值。
    pub ma_value: Decimal,
    /// EMA 指标值。
    pub ema_value: Decimal,
    /// 价格相对 MA 的距离百分比。
    pub ma_distance_pct: Decimal,
    /// 价格相对 EMA 的距离百分比。
    pub ema_distance_pct: Decimal,
    /// 状态值。
    pub ma_state: String,
    /// 状态值。
    pub ema_state: String,
    /// K 线数量。
    pub candle_count: i32,
    /// 时间字段。
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
    /// 提供转换为字符串的集中实现，避免量化核心调用方重复处理相同细节。
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
    /// 封装当前函数，减少量化核心调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
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
    /// 唯一标识。
    pub id: Option<i64>,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub event_type: MarketRankEventType,
    /// 时间周期；为空时使用默认周期。
    pub timeframe: Option<String>,
    /// 旧排名；为空时表示没有上一期排名。
    pub old_rank: Option<i32>,
    /// 新排名；为空时表示没有当前排名。
    pub new_rank: Option<i32>,
    /// 排名变化值；为空时表示无法计算排名变化。
    pub delta_rank: Option<i32>,
    /// 24 小时计价成交额；为空时表示没有成交额。
    pub volume_24h_quote: Option<Decimal>,
    /// 价格数值。
    pub current_price: Option<Decimal>,
    /// 价格数值。
    pub previous_price: Option<Decimal>,
    /// 价格涨跌幅百分比。
    pub price_change_pct: Option<Decimal>,
    /// 价格方向。
    pub price_direction: String,
    /// 状态值。
    pub technical_snapshot_status: String,
    /// technical快照；为空时使用默认值或表示不限制。
    pub technical_snapshot: Option<MarketRankTechnicalSnapshot>,
    /// 时间字段。
    pub detected_at: DateTime<Utc>,
    /// 数据来源。
    pub source: String,
    /// 状态值。
    pub notification_state: String,
}
/// 市场动能机会状态。一个 active episode 表示同一交易对/周期的排名跃迁机会仍在延续。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketVelocityEpisode {
    /// 唯一标识。
    pub id: Option<i64>,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub event_type: MarketRankEventType,
    /// 时间周期；为空时使用默认周期。
    pub timeframe: Option<String>,
    /// 当前状态。
    pub status: String,
    /// 开始时间。
    pub started_at: DateTime<Utc>,
    /// 时间字段。
    pub last_seen_at: DateTime<Utc>,
    /// first旧排名；为空时表示该条件不启用。
    pub first_old_rank: Option<i32>,
    /// latest旧排名；为空时表示该条件不启用。
    pub latest_old_rank: Option<i32>,
    /// latest新排名；为空时表示该条件不启用。
    pub latest_new_rank: Option<i32>,
    /// best新排名；为空时表示该条件不启用。
    pub best_new_rank: Option<i32>,
    /// latest变化排名；为空时表示该条件不启用。
    pub latest_delta_rank: Option<i32>,
    /// 最大排名变化；为空时不限制。
    pub max_delta_rank: Option<i32>,
    /// 命中次数。
    pub hit_count: i32,
    /// 24 小时计价成交额；为空时表示没有成交额。
    pub volume_24h_quote: Option<Decimal>,
    /// 价格数值。
    pub current_price: Option<Decimal>,
    /// 价格数值。
    pub previous_price: Option<Decimal>,
    /// 价格涨跌幅百分比。
    pub price_change_pct: Option<Decimal>,
    /// 价格方向。
    pub price_direction: String,
    /// 状态值。
    pub technical_snapshot_status: String,
    /// 最近rankevent ID；为空时使用默认值或表示不限制。
    pub last_rank_event_id: Option<i64>,
    /// 时间字段。
    pub last_escalated_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityEpisodeWrite {
    Created,
    Escalated,
    Updated,
}
impl MarketVelocityEpisodeWrite {
    pub fn should_append_rank_event(self) -> bool {
        matches!(self, Self::Created | Self::Escalated)
    }
}
/// 资金流向报警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundFlowAlert {
    /// 唯一标识。
    pub id: Option<i64>,
    /// 交易对或资产符号。
    pub symbol: String,
    /// net流入，用于当前结构体的业务数据。
    pub net_inflow: Decimal,
    /// 数量数值。
    pub total_volume: Decimal,
    /// 交易方向。
    pub side: FundFlowSide,
    /// 秒级时长。
    pub window_secs: i32,
    /// 时间字段。
    pub alert_at: DateTime<Utc>,
}
