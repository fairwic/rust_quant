//! 审计链路实体

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRun {
    pub run_id: String,
    pub strategy_id: String,
    pub inst_id: String,
    pub period: String,
    pub status: String,
    pub start_at: Option<chrono::NaiveDateTime>,
    pub end_at: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshotLog {
    pub run_id: String,
    pub kline_ts: i64,
    pub filtered: bool,
    pub filter_reasons: Option<String>,
    pub signal_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskDecisionLog {
    pub run_id: String,
    pub kline_ts: i64,
    pub decision: String,
    pub reason: Option<String>,
    pub risk_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDecisionLog {
    pub run_id: String,
    pub kline_ts: i64,
    pub side: String,
    pub size: f64,
    pub price: f64,
    pub decision_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStateLog {
    pub order_id: i64,
    pub from_state: String,
    pub to_state: String,
    pub reason: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub run_id: String,
    pub strategy_id: String,
    pub inst_id: String,
    pub side: String,
    pub qty: f64,
    pub avg_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub run_id: String,
    pub total_equity: f64,
    pub available: f64,
    pub margin: f64,
    pub pnl: f64,
    pub ts: i64,
}
