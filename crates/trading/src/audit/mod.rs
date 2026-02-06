use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshot {
    pub ts: i64,
    pub payload: String,
    pub filtered: bool,
    pub filter_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskDecision {
    pub ts: i64,
    pub decision: String,
    pub reason: Option<String>,
    pub risk_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDecision {
    pub ts: i64,
    pub side: String,
    pub size: f64,
    pub price: f64,
    pub decision_json: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuditTrail {
    pub run_id: String,
    pub signal_snapshots: Vec<SignalSnapshot>,
    pub risk_decisions: Vec<RiskDecision>,
    pub order_decisions: Vec<OrderDecision>,
}

impl AuditTrail {
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
