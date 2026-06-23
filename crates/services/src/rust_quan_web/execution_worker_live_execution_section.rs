impl ExecutionWorker {
    /// 执行单个 Web 租约任务，并把风险合同、凭证、对账、下单、保护单和报告串成一个原子流程。
    /// 任一安全前置失败都返回可回写的失败报告，而不是抛错中断整批 worker。
    async fn execute_task(&self, task: &ExecutionTask) -> ExecutionTaskReportRequest {
        if is_pending_close_task(task) {
            return self.execute_pending_close_task(task).await;
        }
        // Web payload 先被解析成 Core 可执行的订单任务，后续所有校验都基于这个结构化合同。
        let order_task =
            match ExecutionOrderTask::from_task_with_default(task, self.config.default_exchange) {
                Ok(value) => value,
                Err(error) => {
                    return ExecutionTaskReportRequest::failed(
                        task.id,
                        self.config.default_exchange.as_str(),
                        "unknown",
                        error.to_string(),
                        json!({"task_id": task.id}),
                    );
                }
            };
        // 风险合同校验在任何交易所调用之前执行；缺 stop-loss、方向、入场价等关键字段时直接阻断。
        if let Err(mut violation) =
            validate_execute_signal_risk_contract(task, &order_task, !self.config.dry_run)
        {
            if let Some(contract) = violation
                .raw_payload
                .get_mut("risk_contract")
                .and_then(Value::as_object_mut)
            {
                contract.insert("worker_dry_run".to_string(), json!(self.config.dry_run));
            }
            return ExecutionTaskReportRequest::failed(
                task.id,
                order_task.exchange.as_str(),
                order_side_lower(order_task.side),
                violation.message,
                violation.raw_payload,
            );
        }
        // client_order_id 带有任务归属语义，提前检查可以避免复用其他任务的幂等键造成订单归属错乱。
        if let Ok(request) = order_task.to_order_request() {
            if let Some(report) = client_order_id_owner_violation_report(
                task.id,
                task.task_type.as_str(),
                order_side_lower(order_task.side),
                &request,
            ) {
                return report;
            }
        }
        // 实盘 mutation 必须有持久化审计仓库；没有审计能力时宁可失败，也不允许“无证据下单”。
        if let Some(report) = self.live_audit_repository_missing_report(
            task,
            order_task.exchange.as_str(),
            order_side_lower(order_task.side),
            json!({
                "task_id": task.id,
                "exchange": order_task.exchange.as_str(),
                "symbol": order_task.symbol,
            }),
        ) {
            return report;
        }
        if self.config.dry_run {
            // dry-run 复用同一条下单包装链路生成报告，但不会把保护单当作真实已确认，
            // 因此带保护合同的任务会保持 uncertain，提醒前端不要展示为实盘保护完成。
            return match order_task.to_order_request() {
                Ok(request) => match self
                    .place_order_with_audit(task, &self.gateway, request)
                    .await
                {
                    Ok(ack) => {
                        let mut report = ExecutionTaskReportRequest::success(
                            task.id,
                            ack.exchange.as_str(),
                            ack.order_id
                                .as_deref()
                                .or(ack.client_order_id.as_deref())
                                .unwrap_or("dry_run"),
                            order_side_lower(order_task.side),
                            ack.status.as_deref().unwrap_or("dry_run"),
                            ack.raw,
                        );
                        if let Some(protection) = ProtectionSyncContract::from_task(
                            task,
                            order_side_lower(order_task.side),
                        ) {
                            protection.apply_outcome_to_report(
                                &mut report,
                                ProtectionSyncOutcome::uncertain(
                                    "dry_run_protection_sync_not_confirmed",
                                    "dry-run order does not create a real protective stop-loss order",
                                ),
                            );
                        }
                        report
                    }
                    Err(error) => ExecutionTaskReportRequest::failed(
                        task.id,
                        order_task.exchange.as_str(),
                        order_side_lower(order_task.side),
                        error.to_string(),
                        json!({"task_id": task.id}),
                    ),
                },
                Err(error) => ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({"task_id": task.id}),
                ),
            };
        }
        // 交易所能力、用户凭证和 gateway 解析都在真实下单前完成，任何一步失败都必须返回阻断原因。
        if let Some(report) = self.live_exchange_capability_report(task, &order_task) {
            return report;
        }
        if let Some(report) = self
            .live_api_credential_preflight_report(task, &order_task)
            .await
        {
            return report;
        }
        let gateway = match self
            .resolve_live_gateway_for_task(task, order_task.exchange)
            .await
        {
            Ok(gateway) => gateway,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({"task_id": task.id}),
                );
            }
        };
        // 对账发生在生成最终订单请求之前，保证 request sizing 不会建立在已失效的账户状态上。
        match self
            .check_exchange_reconciliation_before_live_order(task, &order_task, &gateway)
            .await
        {
            Ok(Some(report)) => return report,
            Ok(None) => {}
            Err(error) => {
                return build_live_order_blocked_by_exchange_reconciliation_read_error_report(
                    task,
                    &order_task,
                    error.to_string(),
                );
            }
        }
        // live_order_request 会按交易所过滤器、最新价格和合约规则修正数量/价格，是 mutation 前的最后本地成单模型。
        let mut request = match self.live_order_request(&gateway, &order_task).await {
            Ok(request) => request,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "live_order_read_only_request_build",
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        };
        let protection = ProtectionSyncContract::from_task(task, order_side_lower(order_task.side));
        // 下单前查询同 client_order_id 的订单，用交易所事实处理“上一轮已下单但回写失败”的幂等场景。
        match self
            .pre_place_client_order_report(
                task,
                &gateway,
                Some(&order_task),
                order_side_lower(order_task.side),
                &request,
                protection.clone(),
            )
            .await
        {
            Ok(Some(report)) => return report,
            Ok(None) => {}
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "client_order_id_pre_place_check",
                        "exchange": order_task.exchange.as_str(),
                        "symbol": order_task.symbol,
                        "client_order_id": request.client_order_id.clone(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        }
        // 对要求保护单的实盘开仓，先尝试预挂保护单；保护单未确认时拒绝主订单，避免裸仓进入市场。
        let prearmed_protection = match prearm_protective_order_if_required(
            &gateway,
            &order_task,
            protection.as_ref(),
            task,
            self,
        )
        .await
        {
                Ok(value) => value,
                Err((protection, outcome)) => {
                    let mut report = ExecutionTaskReportRequest::failed(
                        task.id,
                        order_task.exchange.as_str(),
                        order_side_lower(order_task.side),
                        "prearmed protective stop-loss was not confirmed; refusing main order",
                        json!({
                            "task_id": task.id,
                            "stage": "prearmed_protective_order",
                            "exchange": order_task.exchange.as_str(),
                            "symbol": order_task.symbol,
                            "main_order_placed": false,
                            "place_order_allowed": false,
                            "mutation_allowed": false,
                        }),
                    );
                    protection.apply_outcome_to_report(&mut report, outcome);
                    return report;
                }
            };
        let post_fill_protection = if prearmed_protection.is_some() {
            request.attached_stop_loss_price = None;
            None
        } else {
            protection.clone()
        };
        // 杠杆/保证金/持仓模式属于账户 mutation，必须放在保护单路径确定之后；
        // 如果这里失败，要撤销已预挂的保护单，避免留下孤立风控订单。
        if let Err(error) = self
            .prepare_order_settings_after_protection(task, &gateway, &order_task)
            .await
        {
            let mut report = ExecutionTaskReportRequest::failed(
                task.id,
                order_task.exchange.as_str(),
                order_side_lower(order_task.side),
                error.to_string(),
                json!({
                    "task_id": task.id,
                    "stage": "prepare_order_settings",
                    "prearmed_protective_order": prearmed_protection.is_some(),
                    "main_order_placed": false,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            );
            if let Some(prearmed) = &prearmed_protection {
                let cancel_result = prearmed
                    .cancel_after_main_order_failure(task, &gateway, self)
                    .await;
                prearmed.apply_pre_main_order_failure_cancel_result(
                    &mut report,
                    "prepare_order_settings",
                    &error.to_string(),
                    cancel_result,
                );
            }
            return report;
        }
        // 主订单是整条链路唯一的开仓 mutation 点；失败时只做预挂保护单清理，不重新提交主订单。
        match self
            .place_order_with_audit(task, &gateway, request.clone())
            .await
        {
            Ok(ack) => {
                let mut report = self
                    .confirmed_live_order_report(
                        task,
                        &gateway,
                        Some(&order_task),
                        order_side_lower(order_task.side),
                        ack,
                        post_fill_protection.clone(),
                    )
                    .await;
                if let Some(prearmed) = &prearmed_protection {
                    prearmed.apply_after_main_order_report(&mut report);
                }
                report
            }
            Err(error) if is_duplicate_client_order_id_error(&error.to_string()) => {
                let mut report = self
                    .duplicate_client_order_id_report(
                        task,
                        &gateway,
                        Some(&order_task),
                        order_side_lower(order_task.side),
                        &request,
                        post_fill_protection,
                    )
                    .await;
                if let Some(prearmed) = &prearmed_protection {
                    prearmed.apply_after_main_order_report(&mut report);
                }
                report
            }
            Err(error) => {
                let mut report = ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "place_order",
                        "prearmed_protective_order": prearmed_protection.is_some(),
                    }),
                );
                if let Some(prearmed) = &prearmed_protection {
                    let cancel_result = prearmed
                        .cancel_after_main_order_failure(task, &gateway, self)
                        .await;
                    prearmed.apply_main_order_failure_cancel_result(
                        &mut report,
                        &error.to_string(),
                        cancel_result,
                    );
                }
                report
            }
        }
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn execute_pending_close_task(&self, task: &ExecutionTask) -> ExecutionTaskReportRequest {
        let close_task = match PendingCloseTask::from_task(task, self.config.default_exchange) {
            Ok(value) => value,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    self.config.default_exchange.as_str(),
                    "close",
                    error.to_string(),
                    json!({"task_id": task.id, "task_type": task.task_type}),
                );
            }
        };
        if self.config.dry_run {
            return match close_task.to_order_request() {
                Ok(Some(request)) => match self
                    .place_order_with_audit(task, &self.gateway, request.clone())
                    .await
                {
                    Ok(ack) => ExecutionTaskReportRequest::success(
                        task.id,
                        ack.exchange.as_str(),
                        ack.order_id
                            .as_deref()
                            .or(ack.client_order_id.as_deref())
                            .unwrap_or("dry_run"),
                        order_side_lower(request.side),
                        ack.status.as_deref().unwrap_or("dry_run"),
                        ack.raw,
                    ),
                    Err(error) => ExecutionTaskReportRequest::failed(
                        task.id,
                        close_task.exchange.as_str(),
                        "close",
                        error.to_string(),
                        close_task.report_payload(true),
                    ),
                },
                Ok(None) => close_task.dry_run_report(),
                Err(error) => ExecutionTaskReportRequest::failed(
                    task.id,
                    close_task.exchange.as_str(),
                    "close",
                    error.to_string(),
                    close_task.report_payload(true),
                ),
            };
        }
        if let Some(report) = self.live_close_exchange_capability_report(task, &close_task) {
            return report;
        }
        let request = match close_task.to_order_request() {
            Ok(Some(request)) => request,
            Ok(None) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    close_task.exchange.as_str(),
                    "close",
                    close_task.missing_live_contract_message(),
                    close_task.report_payload(false),
                );
            }
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    close_task.exchange.as_str(),
                    "close",
                    error.to_string(),
                    close_task.report_payload(false),
                );
            }
        };
        if let Some(report) = client_order_id_owner_violation_report(
            task.id,
            task.task_type.as_str(),
            order_side_lower(request.side),
            &request,
        ) {
            return report;
        }
        if let Some(report) = self.live_audit_repository_missing_report(
            task,
            request.exchange.as_str(),
            order_side_lower(request.side),
            close_task.report_payload(false),
        ) {
            return report;
        }
        if let Some(report) = self
            .live_api_credential_preflight_report_for_order(
                task,
                request.exchange,
                &request.instrument.symbol_for(request.exchange),
                order_side_lower(request.side),
            )
            .await
        {
            return report;
        }
        let gateway = match self
            .resolve_live_gateway_for_task(task, request.exchange)
            .await
        {
            Ok(gateway) => gateway,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    request.exchange.as_str(),
                    order_side_lower(request.side),
                    error.to_string(),
                    close_task.report_payload(false),
                );
            }
        };
        let planned_protective_cancel = match close_task.protective_cancel_request() {
            Ok(value) => value,
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    request.exchange.as_str(),
                    order_side_lower(request.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "build_pending_close_protective_cancel_request",
                        "exchange": request.exchange.as_str(),
                        "symbol": request.instrument.symbol_for(request.exchange),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        };
        if let Err(error) = self
            .check_exchange_read_only_before_pending_close(
                task,
                &request,
                &gateway,
                planned_protective_cancel.as_ref(),
            )
            .await
        {
            let symbol = request.instrument.symbol_for(request.exchange);
            let error_message = error.to_string();
            let no_matching_position =
                error_message.contains("pending_close_no_matching_position");
            let active_close_order_conflict =
                error_message.contains("pending_close_active_close_order_conflict");
            let blocker_code = if no_matching_position {
                "pending_close_no_matching_position"
            } else if active_close_order_conflict {
                "pending_close_active_close_order_conflict"
            } else {
                "pending_close_gateway_read_failed"
            };
            let gateway_read_failed = !no_matching_position && !active_close_order_conflict;
            let source_ref = build_exchange_reconciliation_source_ref(
                task,
                request.exchange.as_str(),
                &symbol,
                blocker_code,
            );
            return ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side_lower(request.side),
                format!(
                    "pending close blocked because read-only exchange reconciliation did not pass before live close: {error_message}; place_order_allowed=false; mutation_allowed=false"
                ),
                json!({
                    "task_id": task.id,
                    "stage": "pending_close_exchange_reconciliation_read_only",
                    "exchange": request.exchange.as_str(),
                    "symbol": symbol,
                    "source_ref": source_ref,
                    "blocker_code": blocker_code,
                    "gateway_read_failed": gateway_read_failed,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                    "place_order_retried": false,
                }),
            );
        }
        match self
            .pre_place_client_order_report(
                task,
                &gateway,
                None,
                order_side_lower(request.side),
                &request,
                None,
            )
            .await
        {
            Ok(Some(report)) => return report,
            Ok(None) => {}
            Err(error) => {
                return ExecutionTaskReportRequest::failed(
                    task.id,
                    request.exchange.as_str(),
                    order_side_lower(request.side),
                    error.to_string(),
                    json!({
                        "task_id": task.id,
                        "stage": "client_order_id_pre_place_check",
                        "exchange": request.exchange.as_str(),
                        "symbol": request.instrument.symbol_for(request.exchange),
                        "client_order_id": request.client_order_id.clone(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                );
            }
        }
        match self
            .place_order_with_audit(task, &gateway, request.clone())
            .await
        {
            Ok(ack) => {
                let mut report = self
                    .confirmed_live_order_report(
                        task,
                        &gateway,
                        None,
                        order_side_lower(request.side),
                        ack,
                        None,
                    )
                    .await;
                if report.order_status.trim().eq_ignore_ascii_case("FILLED") {
                    if let Some((exchange, cancel_request)) = planned_protective_cancel {
                        let cancel_result = self
                            .cancel_order_with_audit(task, &gateway, exchange, cancel_request)
                            .await;
                        apply_post_close_protection_cancel_result(&mut report, cancel_result);
                    }
                }
                report
            }
            Err(error) if is_duplicate_client_order_id_error(&error.to_string()) => {
                self.duplicate_client_order_id_report(
                    task,
                    &gateway,
                    None,
                    order_side_lower(request.side),
                    &request,
                    None,
                )
                .await
            }
            Err(error) => ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side_lower(request.side),
                error.to_string(),
                close_task.report_payload(false),
            ),
        }
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn execute_pending_confirmation_item(
        &self,
        item: &ExecutionTaskConfirmationLeaseItem,
    ) -> ExecutionTaskReportRequest {
        let pending =
            match PendingConfirmationTask::from_confirmation_item(&item.task, &item.order_result) {
                Ok(value) => value,
                Err(error) => {
                    return ExecutionTaskReportRequest::failed(
                        item.task.id,
                        item.order_result.exchange.as_str(),
                        item.order_result.order_side.as_str(),
                        error.to_string(),
                        json!({
                            "task_id": item.task.id,
                            "order_result_id": item.order_result.id,
                            "confirmation_stage": "parse_pending_confirmation",
                        }),
                    );
                }
            };
        if self.config.dry_run {
            return pending.pending_report(
                "pending confirmation requires live read-only order lookup",
                json!({
                    "task_id": item.task.id,
                    "order_result_id": item.order_result.id,
                    "confirmation_stage": "dry_run_blocked",
                }),
            );
        }
        let gateway = match self
            .resolve_live_gateway_for_task(&item.task, pending.exchange)
            .await
        {
            Ok(gateway) => gateway,
            Err(error) => {
                return pending.pending_report(
                    error.to_string(),
                    json!({
                        "task_id": item.task.id,
                        "order_result_id": item.order_result.id,
                        "confirmation_stage": "resolve_live_gateway",
                    }),
                );
            }
        };
        let query = match pending.to_order_query() {
            Ok(query) => query,
            Err(error) => {
                return pending.pending_report(
                    error.to_string(),
                    json!({
                        "task_id": item.task.id,
                        "order_result_id": item.order_result.id,
                        "confirmation_stage": "build_order_query",
                    }),
                );
            }
        };
        let order = match CryptoExcAllGateway::with_signed_read_only_scope(
            gateway.order(pending.exchange, query),
        )
        .await
        {
            Ok(order) => order,
            Err(error) => {
                return pending.pending_report(
                    error.to_string(),
                    json!({
                        "task_id": item.task.id,
                        "order_result_id": item.order_result.id,
                        "confirmation_stage": "query_order",
                    }),
                );
            }
        };
        let order_id = order.order_id.as_deref().or_else(|| {
            pending
                .external_order_id
                .as_deref()
                .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        });
        let fills = if let Some(order_id) = order_id {
            match CryptoExcAllGateway::with_signed_read_only_scope(gateway.fills(
                    pending.exchange,
                    FillListQuery::for_instrument(order.instrument.clone())
                        .with_order_id(order_id)
                        .with_limit(100),
                ))
            .await
            {
                Ok(fills) => fills,
                Err(error) => {
                    warn!(
                        exchange = pending.exchange.as_str(),
                        order_id, "pending confirmation fills query failed: {}", error
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let ack = pending.to_order_ack(Some(&order));
        let mut report = build_confirmed_order_report(
            item.task.id,
            pending.order_side.as_str(),
            &ack,
            Some(order),
            fills,
            None,
            None,
        );
        let take_profit_retry_required =
            take_profit_sync_retry_required(&item.order_result.raw_payload_json);
        let stop_reset_monitor_required =
            take_profit_stop_reset_monitor_required(&item.order_result.raw_payload_json);
        if take_profit_retry_required || stop_reset_monitor_required {
            carry_take_profit_tracking_from_previous_report(
                &mut report,
                &item.order_result.raw_payload_json,
            );
            if report.execution_status == "completed" {
                let order_task =
                    match ExecutionOrderTask::from_task_with_default(&item.task, pending.exchange) {
                        Ok(order_task) => order_task,
                        Err(error) => {
                            if take_profit_retry_required {
                                report.error_message = Some(error.to_string());
                            } else {
                                let outcome = TakeProfitStopResetOutcome::Failed {
                                    stage: "parse_take_profit_monitor_task".to_string(),
                                    message: error.to_string(),
                                    checked_orders: Vec::new(),
                                    reset_attempt: None,
                                };
                                apply_take_profit_stop_reset_outcome_to_report(
                                    &mut report,
                                    outcome,
                                    true,
                                );
                            }
                            return report;
                        }
                    };
                if take_profit_retry_required {
                    let outcome = sync_take_profit_orders_after_main_fill(
                        &gateway,
                        &order_task,
                        report.filled_qty,
                        &item.task,
                        self,
                    )
                    .await;
                    apply_take_profit_sync_outcome_to_report(&mut report, &order_task, outcome);
                    return report;
                }
                let outcome = match load_exchange_order_filters(order_task.exchange, &order_task.symbol)
                    .await
                {
                    Ok(Some(filters)) => {
                        sync_take_profit_stop_reset_after_fills(
                            &gateway,
                            &order_task,
                            &filters,
                            Some(item.order_result.raw_payload_json.as_str()),
                            &item.task,
                            self,
                        )
                        .await
                    }
                    Ok(None) => TakeProfitStopResetOutcome::Failed {
                        stage: "load_take_profit_stop_reset_filters".to_string(),
                        message: format!(
                            "missing exchange symbol filters for {} on {} before take-profit stop reset",
                            order_task.symbol,
                            order_task.exchange.as_str()
                        ),
                        checked_orders: Vec::new(),
                        reset_attempt: None,
                    },
                    Err(error) => TakeProfitStopResetOutcome::Failed {
                        stage: "load_take_profit_stop_reset_filters".to_string(),
                        message: error.to_string(),
                        checked_orders: Vec::new(),
                        reset_attempt: None,
                    },
                };
                apply_take_profit_stop_reset_outcome_to_report(&mut report, outcome, true);
            }
        }
        report
    }
}
