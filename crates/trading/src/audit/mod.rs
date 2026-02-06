#[derive(Debug, Clone)]
pub struct SignalSnapshot {
    pub ts: i64,
    pub payload: String,
    pub filtered: bool,
    pub filter_reasons: Vec<String>,
}

#[derive(Debug, Default)]
pub struct AuditTrail {
    pub run_id: String,
    pub signal_snapshots: Vec<SignalSnapshot>,
}

impl AuditTrail {
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            signal_snapshots: Vec::new(),
        }
    }

    pub fn record_signal(&mut self, _snapshot: SignalSnapshot) {
        self.signal_snapshots.push(_snapshot);
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
