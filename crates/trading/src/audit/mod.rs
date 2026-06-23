use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshot {
    /// 事件时间戳。
    pub ts: i64,
    /// 载荷。
    pub payload: String,
    /// 是否已被过滤。
    pub filtered: bool,
    /// 列表数据。
    pub filter_reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskDecision {
    /// 事件时间戳。
    pub ts: i64,
    /// decision，用于风控判断或风险展示。
    pub decision: String,
    /// 原因说明。
    pub reason: Option<String>,
    /// 风控 JSON 载荷；为空时表示没有风控快照。
    pub risk_json: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDecision {
    /// 事件时间戳。
    pub ts: i64,
    /// 交易方向。
    pub side: String,
    /// 数量数值。
    pub size: f64,
    /// 价格。
    pub price: f64,
    /// 决策 JSON 载荷；为空时表示没有决策快照。
    pub decision_json: Option<String>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuditTrail {
    /// run ID。
    pub run_id: String,
    /// 列表数据。
    pub signal_snapshots: Vec<SignalSnapshot>,
    /// 列表数据。
    pub risk_decisions: Vec<RiskDecision>,
    /// 列表数据。
    pub order_decisions: Vec<OrderDecision>,
}
impl AuditTrail {
    /// 构建 量化核心 所需实例，并集中初始化依赖和默认状态。
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            signal_snapshots: Vec::new(),
            risk_decisions: Vec::new(),
            order_decisions: Vec::new(),
        }
    }
    pub fn record_signal(&mut self, _snapshot: SignalSnapshot) {
        self.signal_snapshots.push(_snapshot);
    }
    pub fn record_risk_decision(&mut self, decision: RiskDecision) {
        self.risk_decisions.push(decision);
    }
    pub fn record_order_decision(&mut self, decision: OrderDecision) {
        self.order_decisions.push(decision);
    }
}
#[cfg(test)]
mod tests {
    use super::{AuditTrail, SignalSnapshot};
    #[test]
    /// 提供audittrailrecords信号的集中实现，避免量化核心调用方重复处理相同细节。
    fn audit_trail_records_signal() {
        let mut trail = AuditTrail::new("run-1".to_string());
        trail.record_signal(SignalSnapshot {
            ts: 1,
            payload: "{}".to_string(),
            filtered: false,
            filter_reasons: vec![],
        });
        assert_eq!(trail.signal_snapshots.len(), 1);
    }
}
