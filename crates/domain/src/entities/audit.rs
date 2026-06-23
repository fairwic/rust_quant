//! 审计链路实体
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRun {
    /// run ID。
    pub run_id: String,
    /// 策略 ID。
    pub strategy_id: String,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 计算周期。
    pub period: String,
    /// 当前状态。
    pub status: String,
    /// 开始时间。
    pub start_at: Option<chrono::NaiveDateTime>,
    /// 结束时间。
    pub end_at: Option<chrono::NaiveDateTime>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshotLog {
    /// run ID。
    pub run_id: String,
    /// 时间戳。
    pub kline_ts: i64,
    /// 是否已被过滤。
    pub filtered: bool,
    /// 过滤原因列表；为空时表示没有过滤原因。
    pub filter_reasons: Option<String>,
    /// 信号 JSON 载荷。
    pub signal_json: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskDecisionLog {
    /// run ID。
    pub run_id: String,
    /// 时间戳。
    pub kline_ts: i64,
    /// decision，用于风控判断或风险展示。
    pub decision: String,
    /// 原因说明。
    pub reason: Option<String>,
    /// 风控 JSON 载荷；为空时表示没有风控快照。
    pub risk_json: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDecisionLog {
    /// run ID。
    pub run_id: String,
    /// 时间戳。
    pub kline_ts: i64,
    /// 交易方向。
    pub side: String,
    /// 数量数值。
    pub size: f64,
    /// 价格。
    pub price: f64,
    /// 决策 JSON 载荷；为空时表示没有决策快照。
    pub decision_json: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStateLog {
    /// 订单 ID。
    pub order_id: i64,
    /// 状态值。
    pub from_state: String,
    /// 状态值。
    pub to_state: String,
    /// 原因说明。
    pub reason: Option<String>,
    /// 事件时间戳。
    pub ts: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    /// run ID。
    pub run_id: String,
    /// 策略 ID。
    pub strategy_id: String,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 交易方向。
    pub side: String,
    /// 数量。
    pub qty: f64,
    /// 价格数值。
    pub avg_price: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 当前状态。
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    /// run ID。
    pub run_id: String,
    /// 账户总权益。
    pub total_equity: f64,
    /// 可用余额。
    pub available: f64,
    /// 保证金占用。
    pub margin: f64,
    /// 盈亏。
    pub pnl: f64,
    /// 事件时间戳。
    pub ts: i64,
}
