impl ExecutionWorker {
/// 封装实盘exchangecapabilityreport，减少Web 商业链路调用方重复实现相同细节。
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
/// 封装实盘closeexchangecapabilityreport，减少Web 商业链路调用方重复实现相同细节。
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
/// 封装实盘mutationcapabilityreport，减少Web 商业链路调用方重复实现相同细节。
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
/// 提供place订单withaudit的集中实现，避免Web 商业链路调用方重复处理相同细节。
async fn place_order_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    request: OrderPlacementRequest,
) -> crypto_exc_all::Result<OrderAck> {
    // 实盘写操作要求审计仓库先可用；如果审计不可写，直接阻断，避免交易所 mutation 没有本地证据。
    if self.live_order_mode_requires_audit() && !self.audit_repository.can_audit_live_mutations() {
        return Err(CryptoExchangeError::Config(
            "QUANT_CORE_DATABASE_URL is required for live execution audit".to_string(),
        ));
    }
    if self.live_order_mode_requires_audit() {
        // preflight 审计在调用交易所前落地，失败时能证明请求尚未进入交易所 mutation。
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
    let result =
        CryptoExcAllGateway::with_live_mutation_audit_scope(gateway.place_order(request.clone()))
            .await;
    let latency_ms = elapsed_ms(started_at);
    // 成功和失败都写同一套审计结构，后续 replay/人工排障不需要再根据异常路径拼证据。
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
pub(super) async fn place_take_profit_order_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    request: OrderPlacementRequest,
) -> crypto_exc_all::Result<OrderAck> {
    self.place_order_with_audit(task, gateway, request).await
}
/// 判断cancel订单withaudit，给Web 商业链路流程提供布尔结果。
pub(super) async fn cancel_order_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    request: CancelOrderRequest,
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
        self.write_exchange_request_audit(
            ExchangeRequestAuditLog::cancel_order_live_mutation_preflight(
                task,
                exchange,
                &request,
                self.config.dry_run,
            ),
        )
        .await
        .map_err(live_audit_write_error)?;
    }
    let started_at = Instant::now();
    let result = CryptoExcAllGateway::with_live_mutation_audit_scope(
        gateway.cancel_order(exchange, request.clone()),
    )
    .await;
    let latency_ms = elapsed_ms(started_at);
    let audit = match &result {
        Ok(ack) => ExchangeRequestAuditLog::cancel_order_success(
            task,
            exchange,
            &request,
            self.config.dry_run,
            latency_ms,
            ack.raw.clone(),
        ),
        Err(error) => ExchangeRequestAuditLog::cancel_order_failed(
            task,
            exchange,
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
/// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
pub(super) async fn prepare_order_settings_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    request: PrepareOrderSettingsRequest,
) -> crypto_exc_all::Result<PrepareOrderSettingsResult> {
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
        self.write_exchange_request_audit(
            ExchangeRequestAuditLog::prepare_order_settings_live_mutation_preflight(
                task,
                exchange,
                &request,
                self.config.dry_run,
            ),
        )
        .await
        .map_err(live_audit_write_error)?;
    }
    let started_at = Instant::now();
    let result = CryptoExcAllGateway::with_live_mutation_audit_scope(
        gateway.prepare_order_settings(exchange, request.clone()),
    )
    .await;
    let latency_ms = elapsed_ms(started_at);
    let audit = match &result {
        Ok(settings) => ExchangeRequestAuditLog::prepare_order_settings_success(
            task,
            exchange,
            &request,
            self.config.dry_run,
            latency_ms,
            json!(settings),
        ),
        Err(error) => ExchangeRequestAuditLog::prepare_order_settings_failed(
            task,
            exchange,
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
/// 下发保护性止损单并记录审计；保护单是开仓安全前提，不能作为普通附属日志静默失败。
pub(super) async fn place_protective_order_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    request: ProtectiveOrderRequest,
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
        self.write_exchange_request_audit(
            ExchangeRequestAuditLog::protective_order_live_mutation_preflight(
                task,
                exchange,
                &request,
                self.config.dry_run,
            ),
        )
        .await
        .map_err(live_audit_write_error)?;
    }
    let started_at = Instant::now();
    let result = CryptoExcAllGateway::with_live_mutation_audit_scope(
        gateway.place_protective_order(exchange, request.clone()),
    )
    .await;
    let latency_ms = elapsed_ms(started_at);
    let audit = match &result {
        Ok(ack) => ExchangeRequestAuditLog::protective_order_success(
            task,
            exchange,
            &request,
            self.config.dry_run,
            latency_ms,
            ack.raw.clone(),
        ),
        Err(error) => ExchangeRequestAuditLog::protective_order_failed(
            task,
            exchange,
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
/// 判断cancelprotective订单withaudit，给Web 商业链路流程提供布尔结果。
pub(super) async fn cancel_protective_order_with_audit(
    &self,
    task: &ExecutionTask,
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    request: CancelOrderRequest,
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
        self.write_exchange_request_audit(
            ExchangeRequestAuditLog::protective_cancel_live_mutation_preflight(
                task,
                exchange,
                &request,
                self.config.dry_run,
            ),
        )
        .await
        .map_err(live_audit_write_error)?;
    }
    let started_at = Instant::now();
    let result = CryptoExcAllGateway::with_live_mutation_audit_scope(
        gateway.cancel_protective_order(exchange, request.clone()),
    )
    .await;
    let latency_ms = elapsed_ms(started_at);
    let audit = match &result {
        Ok(ack) => ExchangeRequestAuditLog::protective_cancel_success(
            task,
            exchange,
            &request,
            self.config.dry_run,
            latency_ms,
            ack.raw.clone(),
        ),
        Err(error) => ExchangeRequestAuditLog::protective_cancel_failed(
            task,
            exchange,
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
impl TakeProfitOrderPlacer for ExecutionWorker {
    fn place_take_profit_order<'a>(
        &'a self,
        task: &'a ExecutionTask,
        gateway: &'a CryptoExcAllGateway,
        request: OrderPlacementRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = crypto_exc_all::Result<OrderAck>> + Send + 'a>,
    > {
        Box::pin(self.place_take_profit_order_with_audit(task, gateway, request))
    }
}
impl ProtectiveOrderMutator for ExecutionWorker {
    fn audit_place_protective<'a>(
        &'a self,
        task: &'a ExecutionTask,
        gateway: &'a CryptoExcAllGateway,
        exchange: ExchangeId,
        request: ProtectiveOrderRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = crypto_exc_all::Result<OrderAck>> + Send + 'a>,
    > {
        Box::pin(self.place_protective_order_with_audit(task, gateway, exchange, request))
    }
    fn audit_cancel_protective<'a>(
        &'a self,
        task: &'a ExecutionTask,
        gateway: &'a CryptoExcAllGateway,
        exchange: ExchangeId,
        request: CancelOrderRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = crypto_exc_all::Result<OrderAck>> + Send + 'a>,
    > {
        Box::pin(self.cancel_protective_order_with_audit(task, gateway, exchange, request))
    }
}
