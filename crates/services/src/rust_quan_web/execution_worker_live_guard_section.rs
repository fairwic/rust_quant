impl ExecutionWorker {
fn live_exchange_capability_report(
    &self,
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
) -> Option<ExecutionTaskReportRequest> {
    self.live_mutation_capability_report(
        task,
        order_task.exchange,
        order_task.exchange.as_str(),
        order_side_lower(order_task.side),
        json!({
            "task_id": task.id,
            "exchange": order_task.exchange.as_str(),
            "symbol": order_task.symbol,
            "api_credential_preflight_attempted": false,
        }),
    )
}

fn live_close_exchange_capability_report(
    &self,
    task: &ExecutionTask,
    close_task: &PendingCloseTask,
) -> Option<ExecutionTaskReportRequest> {
    self.live_mutation_capability_report(
        task,
        close_task.exchange,
        close_task.exchange.as_str(),
        "close",
        close_task.report_payload(false),
    )
}

fn live_mutation_capability_report(
    &self,
    task: &ExecutionTask,
    exchange: ExchangeId,
    exchange_name: &str,
    order_side: &str,
    mut payload: Value,
) -> Option<ExecutionTaskReportRequest> {
    if self.config.dry_run {
        return None;
    }
    let capability = worker_live_capability_for_exchange(exchange.as_str());
    if capability.protection_placement != ProtectionPlacementMode::Unsupported {
        return None;
    }

    if let Some(payload) = payload.as_object_mut() {
        payload.insert("stage".to_string(), json!("live_exchange_capability"));
        payload.insert("exchange".to_string(), json!(exchange_name));
        payload.insert("worker_capability".to_string(), json!(capability));
        payload.insert("place_order_allowed".to_string(), json!(false));
        payload.insert("mutation_allowed".to_string(), json!(false));
    }

    Some(ExecutionTaskReportRequest::failed(
        task.id,
        exchange_name,
        order_side,
        format!(
            "worker live execution is unsupported for exchange {exchange_name}; place_order_allowed=false; mutation_allowed=false"
        ),
        payload,
    ))
}

async fn place_order_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    request: OrderPlacementRequest,
) -> crypto_exc_all::Result<OrderAck> {
    if self.live_order_mode_requires_audit() && !self.audit_repository.can_audit_live_mutations() {
        return Err(CryptoExchangeError::Config(
            "QUANT_CORE_DATABASE_URL is required for live execution audit".to_string(),
        ));
    }
    if self.live_order_mode_requires_audit() {
        self.audit_repository
            .verify_live_audit_ready()
            .await
            .map_err(live_audit_write_error)?;
        self.write_exchange_request_audit(ExchangeRequestAuditLog::live_mutation_preflight(
            task,
            &request,
            self.config.dry_run,
        ))
        .await
        .map_err(live_audit_write_error)?;
    }
    let started_at = Instant::now();
    let result = gateway.place_order(request.clone()).await;
    let latency_ms = elapsed_ms(started_at);

    let audit = match &result {
        Ok(ack) => ExchangeRequestAuditLog::success(
            task,
            &request,
            self.config.dry_run,
            latency_ms,
            ack.raw.clone(),
        ),
        Err(error) => ExchangeRequestAuditLog::failed(
            task,
            &request,
            self.config.dry_run,
            latency_ms,
            error.to_string(),
        ),
    };
    if self.live_order_mode_requires_audit() {
        self.write_exchange_request_audit(audit)
            .await
            .map_err(live_audit_write_error)?;
    } else {
        self.record_exchange_request_audit(audit).await;
    }

    result
}
}
