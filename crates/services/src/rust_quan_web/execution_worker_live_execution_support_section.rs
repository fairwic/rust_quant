impl ExecutionWorker {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn live_order_request(
        &self,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<OrderPlacementRequest> {
        let instrument = parse_instrument(&order_task.symbol)?;
        let ticker = gateway.ticker(order_task.exchange, &instrument).await?;
        let last_price = ticker.last_price.trim().parse::<f64>().map_err(|err| {
            anyhow!(
                "invalid ticker last_price for {} on {}: {}",
                order_task.symbol,
                order_task.exchange.as_str(),
                err
            )
        })?;
        let filters = load_exchange_order_filters(order_task.exchange, &order_task.symbol).await?;
        order_task.to_live_order_request(Some(last_price), filters.as_ref())
    }
    /// 选择 Web 商业、会员和执行准备度 的最佳候选结果，避免选择规则分散在调用方。
    async fn resolve_live_gateway(
        &self,
        buyer_email: &str,
        exchange: ExchangeId,
        credential_id: i64,
    ) -> Result<CryptoExcAllGateway> {
        let config = self
            .client
            .resolve_user_exchange_config_for_credential(buyer_email, exchange.as_str(), credential_id)
            .await?;
        CryptoExcAllGateway::from_single_exchange_credentials(
            exchange,
            config.api_key,
            config.api_secret,
            config.passphrase,
            config.simulated,
        )
        .map_err(Into::into)
    }
    /// 选择 Web 商业、会员和执行准备度 的最佳候选结果，避免选择规则分散在调用方。
    async fn resolve_live_gateway_for_task(
        &self,
        task: &ExecutionTask,
        exchange: ExchangeId,
    ) -> Result<CryptoExcAllGateway> {
        let credential_id = api_credential_id_from_task(task).ok_or_else(|| {
            anyhow!(
                "api_credential_id_missing: execution task does not carry api_credential_id; place_order_allowed=false; mutation_allowed=false"
            )
        })?;
        self.resolve_live_gateway(&task.buyer_email, exchange, credential_id)
            .await
    }
    /// 封装实盘apicredentialpreflightreport，减少Web 商业链路调用方重复实现相同细节。
    async fn live_api_credential_preflight_report(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
    ) -> Option<ExecutionTaskReportRequest> {
        self.live_api_credential_preflight_report_for_order(
            task,
            order_task.exchange,
            &order_task.symbol,
            order_side_lower(order_task.side),
        )
        .await
    }
    /// 调用 Web owner service 复核用户 API Key 的签名预检结果，只有 verified 且交易所匹配时才允许实盘下单。
    async fn live_api_credential_preflight_report_for_order(
        &self,
        task: &ExecutionTask,
        exchange: ExchangeId,
        symbol: &str,
        order_side: &str,
    ) -> Option<ExecutionTaskReportRequest> {
        // Web 任务必须携带明确的 api_credential_id，Core 不通过 buyer_email 猜测凭证，避免误用其他交易所账户。
        let Some(credential_id) = api_credential_id_from_task(task) else {
            return Some(ExecutionTaskReportRequest::failed(
                task.id,
                exchange.as_str(),
                order_side,
                "API credential preflight blocked live order: api_credential_id_missing: execution task does not carry api_credential_id; place_order_allowed=false; mutation_allowed=false",
                json!({
                    "task_id": task.id,
                    "stage": "api_credential_preflight",
                    "blocker_code": "api_credential_id_missing",
                    "blocker_message": "execution task does not carry api_credential_id",
                    "exchange": exchange.as_str(),
                    "symbol": symbol,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            ));
        };
        // 这里读取的是 Web 的内部凭证状态摘要，不返回明文 secret；Core 只根据 readiness 决定是否继续。
        let checked = match self
            .client
            .check_internal_api_credential(credential_id)
            .await
        {
            Ok(checked) => checked,
            Err(error) => {
                return Some(ExecutionTaskReportRequest::failed(
                    task.id,
                    exchange.as_str(),
                    order_side,
                    format!(
                        "API credential preflight failed before live order: {error}; place_order_allowed=false; mutation_allowed=false"
                    ),
                    json!({
                        "task_id": task.id,
                        "stage": "api_credential_preflight",
                        "api_credential_id": credential_id,
                        "exchange": exchange.as_str(),
                        "symbol": symbol,
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                ));
            }
        };
        if !api_credential_exchange_matches_task(&checked.exchange, exchange) {
            return Some(ExecutionTaskReportRequest::failed(
                task.id,
                exchange.as_str(),
                order_side,
                format!(
                    "API credential preflight returned exchange {} for task exchange {}; place_order_allowed=false; mutation_allowed=false",
                    checked.exchange,
                    exchange.as_str()
                ),
                json!({
                    "task_id": task.id,
                    "stage": "api_credential_preflight",
                    "api_credential_id": credential_id,
                    "credential_exchange": checked.exchange,
                    "task_exchange": exchange.as_str(),
                    "symbol": symbol,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            ));
        }
        if checked.execution_readiness.can_execute {
            return None;
        }
        let blocker_code = checked
            .execution_readiness
            .blocker_code
            .as_deref()
            .or(checked.last_check_code.as_deref())
            .unwrap_or("api_credential_not_ready");
        let blocker_message = checked
            .execution_readiness
            .blocker_message
            .as_deref()
            .or(checked.last_check_message.as_deref())
            .unwrap_or("API credential is not ready for live execution");
        Some(ExecutionTaskReportRequest::failed(
            task.id,
            exchange.as_str(),
            order_side,
            format!(
                "API credential preflight blocked live order: {blocker_code}: {blocker_message}; place_order_allowed=false; mutation_allowed=false"
            ),
            json!({
                "task_id": task.id,
                "stage": "api_credential_preflight",
                "api_credential_id": credential_id,
                "exchange": exchange.as_str(),
                "symbol": symbol,
                "last_check_code": checked.last_check_code,
                "blocker_code": checked.execution_readiness.blocker_code,
                "blocker_message": checked.execution_readiness.blocker_message,
                "place_order_allowed": false,
                "mutation_allowed": false,
            }),
        ))
    }
    /// 创建 Web 商业、会员和执行准备度 资源，并在入口处完成必要的参数归一。
    async fn prepare_order_settings_after_protection(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<()> {
        if order_task.margin_mode.is_none()
            && order_task.leverage.is_none()
            && order_task.position_mode.is_none()
        {
            return Ok(());
        }
        let prepare = PrepareOrderSettingsRequest {
            instrument: parse_instrument(&order_task.symbol)?,
            margin_mode: order_task.margin_mode.clone(),
            leverage: order_task.leverage.clone(),
            position_mode: order_task.position_mode,
            product_type: None,
            margin_coin: order_task.margin_coin.clone(),
            position_side: order_task.position_side.clone(),
        };
        self.prepare_order_settings_with_audit(task, gateway, order_task.exchange, prepare)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
    /// 用交易所订单查询结果补齐 ack，随后同步保护单状态，确保 Web 看到的是“已确认事实”而不是单纯下单返回。
    async fn confirmed_live_order_report(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: Option<&ExecutionOrderTask>,
        order_side: &str,
        ack: OrderAck,
        protection: Option<ProtectionSyncContract>,
    ) -> ExecutionTaskReportRequest {
        let task_id = task.id;
        let mut confirmed_order = None;
        // place_order ack 只证明交易所接收请求；再次查询订单和成交明细，才能确定是否已成交、部分成交或等待确认。
        let mut report = match confirm_live_order(gateway, &ack).await {
            Ok((order, fills)) => {
                confirmed_order = Some(order.clone());
                build_confirmed_order_report_for_task(
                    task,
                    order_side,
                    &ack,
                    Some(order),
                    fills,
                    None,
                    protection.clone(),
                )
            }
            Err(error) => {
                warn!(
                    task_id,
                    exchange = ack.exchange.as_str(),
                    order_id = ack.order_id.as_deref().unwrap_or(""),
                    client_order_id = ack.client_order_id.as_deref().unwrap_or(""),
                    "live order confirmation failed after place_order ack: {}",
                    error
                );
                build_confirmed_order_report_for_task(
                    task,
                    order_side,
                    &ack,
                    None,
                    Vec::new(),
                    Some(error.to_string()),
                    protection.clone(),
                )
            }
        };
        // 保护单状态跟随确认后的主订单结果同步；如果保护失败，报告会保持阻塞或触发回滚，而不是直接完成任务。
        if let (Some(order_task), Some(protection)) = (
            order_task,
            ProtectionSyncContract::from_task_result(&report, protection),
        ) {
            let outcome = if let Some(outcome) =
                attached_stop_loss_order_ack_outcome(order_task, &ack, confirmed_order.as_ref())
            {
                outcome
            } else {
                match load_exchange_order_filters(order_task.exchange, &order_task.symbol).await {
                    Ok(Some(filters)) => match build_protective_stop_market_order_request(
                        order_task,
                        &protection,
                        &filters,
                    ) {
                        Ok(request) => {
                            place_and_confirm_protective_order(
                                gateway,
                                order_task.exchange,
                                request,
                                task,
                                self,
                            )
                            .await
                        }
                        Err(error) => ProtectionSyncOutcome::failed(
                            "build_protective_order_request",
                            error.to_string(),
                        ),
                    },
                    Ok(None) => ProtectionSyncOutcome::failed(
                        "load_protective_order_filters",
                        format!(
                            "missing exchange symbol filters for {} on {} before protective order",
                            order_task.symbol,
                            order_task.exchange.as_str()
                        ),
                    ),
                    Err(error) => ProtectionSyncOutcome::failed(
                        "load_protective_order_filters",
                        error.to_string(),
                    ),
                }
            };
            let should_rollback = protection_outcome_requires_rollback(&outcome);
            protection.apply_outcome_to_report(&mut report, outcome);
            if should_rollback {
                self.rollback_after_protective_failure(task, gateway, order_task, &mut report)
                    .await;
            }
        }
        if let Some(order_task) = order_task {
            if report.execution_status == "completed" && !order_task.take_profit_legs.is_empty() {
                let outcome =
                    sync_take_profit_orders_after_main_fill(
                        gateway,
                        order_task,
                        report.filled_qty,
                        task,
                        self,
                    )
                        .await;
                apply_take_profit_sync_outcome_to_report(&mut report, order_task, outcome);
            }
        }
        report
    }
    /// 提供rollback之后protectivefailure的集中实现，避免Web 商业链路调用方重复处理相同细节。
    async fn rollback_after_protective_failure(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
        report: &mut ExecutionTaskReportRequest,
    ) {
        let request = match build_protective_failure_rollback_order_request(order_task, report) {
            Ok(Some(request)) => request,
            Ok(None) => return,
            Err(error) => {
                apply_protective_failure_rollback_error(
                    report,
                    "build_rollback_order_request",
                    error.to_string(),
                );
                return;
            }
        };
        let rollback_side = order_side_lower(request.side);
        let ack = match self
            .place_order_with_audit(task, gateway, request.clone())
            .await
        {
            Ok(ack) => ack,
            Err(error) => {
                apply_protective_failure_rollback_error(
                    report,
                    "place_rollback_order",
                    error.to_string(),
                );
                return;
            }
        };
        let rollback_report = match confirm_live_order(gateway, &ack).await {
            Ok((order, fills)) => build_confirmed_order_report_for_task(
                task,
                rollback_side,
                &ack,
                Some(order),
                fills,
                None,
                None,
            ),
            Err(error) => build_confirmed_order_report_for_task(
                task,
                rollback_side,
                &ack,
                None,
                Vec::new(),
                Some(error.to_string()),
                None,
            ),
        };
        apply_protective_failure_rollback_report(report, &rollback_report);
    }
    /// 提供duplicateclient订单ID报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
    async fn duplicate_client_order_id_report(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: Option<&ExecutionOrderTask>,
        order_side: &str,
        request: &OrderPlacementRequest,
        protection: Option<ProtectionSyncContract>,
    ) -> ExecutionTaskReportRequest {
        match duplicate_client_order_id_reconciliation_ack(request) {
            Some(ack) => {
                self.confirmed_live_order_report(
                    task, gateway, order_task, order_side, ack, protection,
                )
                .await
            }
            None => ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side,
                "duplicate client order id error requires a stable client_order_id to reconcile",
                json!({
                    "reconciliation": {
                        "reason": "duplicate_client_order_id",
                        "action": "blocked_missing_client_order_id",
                        "place_order_retried": false,
                    }
                }),
            ),
        }
    }
    /// 提供preplaceclient订单报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
    async fn pre_place_client_order_report(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: Option<&ExecutionOrderTask>,
        order_side: &str,
        request: &OrderPlacementRequest,
        protection: Option<ProtectionSyncContract>,
    ) -> Result<Option<ExecutionTaskReportRequest>> {
        let Some(lookup) = pre_place_client_order_lookup(request) else {
            return Ok(None);
        };
        match CryptoExcAllGateway::with_signed_read_only_scope(
            gateway.order(request.exchange, lookup.query.clone()),
        )
        .await
        {
            Ok(_) => Ok(Some(
                self.confirmed_live_order_report(
                    task, gateway, order_task, order_side, lookup.ack, protection,
                )
                .await,
            )),
            Err(error) if is_order_not_found_for_client_order_preflight(&error.to_string()) => {
                Ok(None)
            }
            Err(error) => Err(anyhow!(
                "client order id pre-place check failed for {} on {}: {}",
                lookup.query.client_order_id.as_deref().unwrap_or("unknown"),
                request.exchange.as_str(),
                error
            )),
        }
    }
}
