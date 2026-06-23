#![allow(dead_code)]
use super::*;
use crate::rust_quan_web::{ExecutionTask, ReportResultReplayCandidate};
use async_trait::async_trait;
use std::sync::Mutex;
pub(crate) fn task(payload: serde_json::Value) -> ExecutionTask {
    task_with_metadata("execute_signal", "pending", payload)
}
/// 提供binanceeth过滤器的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
/// 提供taskwithmetadata的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 列表数据。
    pub(crate) checkpoints: Mutex<Vec<ExecutionWorkerCheckpoint>>,
    /// 列表数据。
    pub(crate) audits: Mutex<Vec<ExchangeRequestAuditLog>>,
    /// 列表数据。
    pub(crate) report_replay_candidates: Mutex<Vec<ReportResultReplayCandidate>>,
    /// 列表数据。
    pub(crate) report_replay_queries: Mutex<Vec<(u32, u64, Vec<i64>)>>,
}
#[async_trait]
impl ExecutionAuditRepository for CapturingAuditRepository {
    fn can_audit_live_mutations(&self) -> bool {
        true
    }
    /// 持久化 Web 商业、会员和执行准备度 结果，保证写入路径和幂等语义集中处理。
    async fn upsert_worker_checkpoint(&self, checkpoint: &ExecutionWorkerCheckpoint) -> Result<()> {
        self.checkpoints.lock().unwrap().push(checkpoint.clone());
        Ok(())
    }
    /// 持久化 Web 商业、会员和执行准备度 结果，保证写入路径和幂等语义集中处理。
    async fn insert_exchange_request_audit(&self, audit: &ExchangeRequestAuditLog) -> Result<()> {
        self.audits.lock().unwrap().push(audit.clone());
        Ok(())
    }
    /// 列出 Web 商业、会员和执行准备度 的候选数据集合，并保持分页、过滤或排序语义集中。
    async fn list_report_result_replay_candidates(
        &self,
        limit: u32,
        failure_backoff_seconds: u64,
        target_task_ids: &[i64],
    ) -> Result<Vec<ReportResultReplayCandidate>> {
        self.report_replay_queries.lock().unwrap().push((
            limit,
            failure_backoff_seconds,
            target_task_ids.to_vec(),
        ));
        let mut candidates = self.report_replay_candidates.lock().unwrap();
        let mut selected = Vec::new();
        let mut remaining = Vec::new();
        for candidate in candidates.drain(..) {
            let target_matches =
                target_task_ids.is_empty() || target_task_ids.contains(&candidate.report.task_id);
            if target_matches && selected.len() < limit as usize {
                selected.push(candidate);
            } else {
                remaining.push(candidate);
            }
        }
        *candidates = remaining;
        Ok(selected)
    }
}
#[derive(Default)]
pub(crate) struct FailingAuditRepository;
#[async_trait]
impl ExecutionAuditRepository for FailingAuditRepository {
    fn can_audit_live_mutations(&self) -> bool {
        true
    }
    async fn upsert_worker_checkpoint(
        &self,
        _checkpoint: &ExecutionWorkerCheckpoint,
    ) -> Result<()> {
        Ok(())
    }
    async fn insert_exchange_request_audit(&self, _audit: &ExchangeRequestAuditLog) -> Result<()> {
        Err(anyhow!("audit write unavailable"))
    }
}
