impl ExecutionWorker {
    async fn execute_task(&self, task: &ExecutionTask) -> ExecutionTaskReportRequest {
        if is_pending_close_task(task) {
            return self.execute_pending_close_task(task).await;
        }

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
        if let Err(mut violation) = validate_execute_signal_risk_contract(task, &order_task) {
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
            .resolve_live_gateway(&task.buyer_email, order_task.exchange)
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
        let prearmed_protection =
            match prearm_protective_order_if_required(&gateway, &order_task, protection.as_ref())
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
        if let Err(error) = self
            .prepare_binance_order_settings_after_protection(&gateway, &order_task)
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
                let cancel_result = prearmed.cancel_after_main_order_failure(&gateway).await;
                prearmed.apply_pre_main_order_failure_cancel_result(
                    &mut report,
                    "prepare_order_settings",
                    &error.to_string(),
                    cancel_result,
                );
            }
            return report;
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
                    let cancel_result = prearmed.cancel_after_main_order_failure(&gateway).await;
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
        let gateway = match self
            .resolve_live_gateway(&task.buyer_email, request.exchange)
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

        if let Err(error) = self
            .check_exchange_read_only_before_pending_close(task, &request, &gateway)
            .await
        {
            let symbol = request.instrument.symbol_for(request.exchange);
            let source_ref = build_exchange_reconciliation_source_ref(
                task,
                request.exchange.as_str(),
                &symbol,
                "pending_close_gateway_read_failed",
            );
            return ExecutionTaskReportRequest::failed(
                task.id,
                request.exchange.as_str(),
                order_side_lower(request.side),
                format!(
                    "pending close blocked because read-only exchange reconciliation failed before live close: {error}; place_order_allowed=false; mutation_allowed=false"
                ),
                json!({
                    "task_id": task.id,
                    "stage": "pending_close_exchange_reconciliation_read_only",
                    "exchange": request.exchange.as_str(),
                    "symbol": symbol,
                    "source_ref": source_ref,
                    "gateway_read_failed": true,
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
                    if let Ok(Some((exchange, cancel_request))) =
                        close_task.protective_cancel_request()
                    {
                        let cancel_result = gateway.cancel_order(exchange, cancel_request).await;
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
            .resolve_live_gateway(&item.task.buyer_email, pending.exchange)
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

        let order = match gateway.order(pending.exchange, query).await {
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
            match gateway
                .fills(
                    pending.exchange,
                    FillListQuery::for_instrument(order.instrument.clone())
                        .with_order_id(order_id)
                        .with_limit(100),
                )
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
        build_confirmed_order_report(
            item.task.id,
            pending.order_side.as_str(),
            &ack,
            Some(order),
            fills,
            None,
            None,
        )
    }

}
