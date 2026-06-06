#![allow(dead_code)]
use super::*;
use crate::rust_quan_web::{ExecutionTask, ReportResultReplayCandidate};
use async_trait::async_trait;
use std::sync::Mutex;

pub(crate) fn task(payload: serde_json::Value) -> ExecutionTask {
    task_with_metadata("execute_signal", "pending", payload)
}

pub(crate) fn binance_eth_filters() -> ExchangeOrderFilters {
    ExchangeOrderFilters {
        min_qty: Some("0.001".parse().unwrap()),
        max_qty: Some("10000".parse().unwrap()),
        step_size: Some("0.001".parse().unwrap()),
        min_notional: Some("20".parse().unwrap()),
        quantity_precision: Some(3),
        tick_size: Some("0.01".parse().unwrap()),
        price_precision: Some(2),
        contract_value: None,
        contract_value_currency: None,
    }
}

pub(crate) fn task_with_metadata(
    task_type: &str,
    task_status: &str,
    payload: serde_json::Value,
) -> ExecutionTask {
    ExecutionTask {
        id: 42,
        news_signal_id: None,
        strategy_signal_id: None,
        combo_id: 9,
        buyer_email: "buyer@example.com".to_string(),
        strategy_slug: "news_momentum".to_string(),
        symbol: "BTC-USDT-SWAP".to_string(),
        task_type: task_type.to_string(),
        task_status: task_status.to_string(),
        priority: 3,
        lease_owner: None,
        lease_until: None,
        scheduled_at: "2026-04-23T12:00:00".to_string(),
        request_payload_json: payload,
        created_at: "2026-04-23T12:00:00".to_string(),
        updated_at: "2026-04-23T12:00:00".to_string(),
    }
}

#[derive(Default)]
pub(crate) struct CapturingAuditRepository {
    pub(crate) checkpoints: Mutex<Vec<ExecutionWorkerCheckpoint>>,
    pub(crate) audits: Mutex<Vec<ExchangeRequestAuditLog>>,
    pub(crate) report_replay_candidates: Mutex<Vec<ReportResultReplayCandidate>>,
    pub(crate) report_replay_queries: Mutex<Vec<(u32, u64)>>,
}

#[async_trait]
impl ExecutionAuditRepository for CapturingAuditRepository {
    async fn upsert_worker_checkpoint(&self, checkpoint: &ExecutionWorkerCheckpoint) -> Result<()> {
        self.checkpoints.lock().unwrap().push(checkpoint.clone());
        Ok(())
    }

    async fn insert_exchange_request_audit(&self, audit: &ExchangeRequestAuditLog) -> Result<()> {
        self.audits.lock().unwrap().push(audit.clone());
        Ok(())
    }

    async fn list_report_result_replay_candidates(
        &self,
        limit: u32,
        failure_backoff_seconds: u64,
    ) -> Result<Vec<ReportResultReplayCandidate>> {
        self.report_replay_queries
            .lock()
            .unwrap()
            .push((limit, failure_backoff_seconds));
        let mut candidates = self.report_replay_candidates.lock().unwrap();
        let take = usize::min(candidates.len(), limit as usize);
        Ok(candidates.drain(..take).collect())
    }
}
