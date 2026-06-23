impl ExecutionWorker {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn record_checkpoint(
        &self,
        worker_status: &str,
        last_task_id: Option<i64>,
        checkpoint_value: Value,
    ) {
        let checkpoint = ExecutionWorkerCheckpoint::heartbeat(
            self.config.worker_id.clone(),
            worker_status,
            last_task_id,
            checkpoint_value,
        );
        if let Err(error) = self
            .audit_repository
            .upsert_worker_checkpoint(&checkpoint)
            .await
        {
            warn!(
                worker_id = self.config.worker_id,
                "写入 execution worker checkpoint 失败: {}", error
            );
        }
    }
    /// 持久化 Web 商业、会员和执行准备度 结果，保证写入路径和幂等语义集中处理。
    async fn record_report_result_failure(
        &self,
        task_id: i64,
        report: &ExecutionTaskReportRequest,
        error_message: impl Into<String>,
        stage: &str,
    ) {
        let error_message = error_message.into();
        self.record_exchange_request_audit(ExchangeRequestAuditLog::report_result_failed(
            report,
            error_message.clone(),
        ))
        .await;
        self.record_checkpoint(
            "report_failed",
            Some(task_id),
            json!({
                "stage": stage,
                "error": error_message,
                "replay": {
                    "action": "retry_report_result_only",
                    "place_order_allowed": false,
                    "task_id": report.task_id,
                    "exchange": report.exchange,
                    "external_order_id": report.external_order_id,
                    "execution_status": report.execution_status,
                    "order_status": report.order_status,
                },
            }),
        )
        .await;
    }
    /// 提供write交易所requestaudit的集中实现，避免Web 商业链路调用方重复处理相同细节。
    async fn write_exchange_request_audit(&self, audit: ExchangeRequestAuditLog) -> Result<()> {
        self.audit_repository
            .insert_exchange_request_audit(&audit)
            .await
    }
    /// 持久化 Web 商业、会员和执行准备度 结果，保证写入路径和幂等语义集中处理。
    async fn record_exchange_request_audit(&self, audit: ExchangeRequestAuditLog) {
        if let Err(error) = self.write_exchange_request_audit(audit.clone()).await {
            warn!(
                request_id = audit.request_id,
                "写入 exchange request audit 失败: {}", error
            );
        }
    }
}
