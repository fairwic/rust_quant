impl ExecutionWorker {
    async fn run_reconciliation_only_once(&self) -> Result<usize> {
        self.record_checkpoint(
            "leasing_reconciliation_only",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
                "default_exchange": self.config.default_exchange.as_str(),
                "task_types": self.config.task_types.clone(),
                "task_statuses": self.config.task_statuses.clone(),
                "target_task_ids": self.config.target_task_ids.clone(),
                "signed_read_only": !self.config.dry_run,
                "place_order_allowed": false,
                "mutation_allowed": false,
                "report_result_allowed": false,
            }),
        )
        .await;

        let leased = match self
            .client
            .lease_tasks(ExecutionTaskLeaseRequest {
                worker_id: self.config.worker_id.clone(),
                limit: self.config.lease_limit,
                task_ids: self.config.target_task_ids.clone(),
                task_types: self.config.task_types.clone(),
                task_statuses: self.config.task_statuses.clone(),
            })
            .await
        {
            Ok(leased) => leased,
            Err(error) => {
                self.record_checkpoint(
                    "failed",
                    None,
                    json!({
                        "stage": "reconciliation_only_lease_tasks",
                        "error": error.to_string(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                )
                .await;
                return Err(error);
            }
        };

        self.record_checkpoint(
            "reconciliation_only_leased",
            None,
            json!({
                "leased_count": leased.tasks.len(),
                "lease_limit": self.config.lease_limit,
                "target_task_ids": self.config.target_task_ids.clone(),
                "signed_read_only": !self.config.dry_run,
                "place_order_allowed": false,
                "mutation_allowed": false,
                "report_result_allowed": false,
            }),
        )
        .await;

        let mut handled = 0;
        let mut last_task_id = None;
        for task in leased.tasks {
            if !self.config.leased_task_allowed(task.id) {
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(task.id),
                    json!({
                        "stage": "reconciliation_only_target_task_allowlist",
                        "task_id": task.id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                )
                .await;
                continue;
            }

            self.execute_reconciliation_only_for_task(&task).await;
            last_task_id = Some(task.id);
            handled += 1;
        }

        self.record_checkpoint(
            "idle",
            last_task_id,
            json!({
                "handled": handled,
                "reconciliation_only_mode": true,
                "signed_read_only": !self.config.dry_run,
                "place_order_allowed": false,
                "mutation_allowed": false,
                "report_result_allowed": false,
            }),
        )
        .await;
        Ok(handled)
    }

    async fn execute_reconciliation_only_for_task(&self, task: &ExecutionTask) {
        let order_task =
            match ExecutionOrderTask::from_task_with_default(task, self.config.default_exchange) {
                Ok(value) => value,
                Err(error) => {
                    self.record_checkpoint(
                        "reconciliation_only_parse_failed",
                        Some(task.id),
                        json!({
                            "stage": "reconciliation_only_parse_task",
                            "task_id": task.id,
                            "error": error.to_string(),
                            "place_order_allowed": false,
                            "mutation_allowed": false,
                            "report_result_allowed": false,
                        }),
                    )
                    .await;
                    return;
                }
            };
        if is_protected_link_symbol(&order_task.symbol) {
            self.record_checkpoint(
                "reconciliation_only_protected_symbol_skipped",
                Some(task.id),
                json!({
                    "stage": "reconciliation_only_symbol_guard",
                    "exchange": order_task.exchange.as_str(),
                    "symbol": order_task.symbol,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                    "report_result_allowed": false,
                    "reason": "LINKUSDT is excluded from live validation",
                }),
            )
            .await;
            return;
        }

        let instrument = match parse_instrument(&order_task.symbol) {
            Ok(value) => value,
            Err(error) => {
                self.record_checkpoint(
                    "reconciliation_only_parse_failed",
                    Some(task.id),
                    json!({
                        "stage": "reconciliation_only_parse_symbol",
                        "exchange": order_task.exchange.as_str(),
                        "symbol": order_task.symbol,
                        "error": error.to_string(),
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                        "report_result_allowed": false,
                    }),
                )
                .await;
                return;
            }
        };

        let live_gateway = if self.config.dry_run {
            None
        } else {
            match self
                .resolve_live_gateway(&task.buyer_email, order_task.exchange)
                .await
            {
                Ok(gateway) => Some(gateway),
                Err(error) => {
                    self.record_reconciliation_only_read_failure(
                        task,
                        &order_task,
                        "resolve_live_gateway",
                        error.to_string(),
                    )
                    .await;
                    return;
                }
            }
        };
        let gateway = live_gateway.as_ref().unwrap_or(&self.gateway);

        let positions = match gateway
            .positions(order_task.exchange, Some(&instrument))
            .await
        {
            Ok(positions) => positions,
            Err(error) => {
                self.record_reconciliation_only_read_failure(
                    task,
                    &order_task,
                    "positions",
                    error.to_string(),
                )
                .await;
                return;
            }
        };
        let open_orders = match gateway
            .open_orders(
                order_task.exchange,
                OrderListQuery::for_instrument(instrument).with_limit(100),
            )
            .await
        {
            Ok(open_orders) => open_orders,
            Err(error) => {
                self.record_reconciliation_only_read_failure(
                    task,
                    &order_task,
                    "open_orders",
                    error.to_string(),
                )
                .await;
                return;
            }
        };
        let requests = build_exchange_reconciliation_sync_requests_from_read_only_snapshot(
            task,
            &positions,
            &open_orders,
            None,
        );
        let mut source_refs = Vec::new();
        let mut reported_count = 0usize;
        let mut report_error_count = 0usize;
        for request in &requests {
            if let Some(source_ref) = request.source_ref.clone() {
                source_refs.push(source_ref);
            }
            match self.client.report_exchange_reconciliation(request.clone()).await {
                Ok(_) => reported_count += 1,
                Err(error) => {
                    report_error_count += 1;
                    self.record_checkpoint(
                        "reconciliation_only_report_failed",
                        Some(task.id),
                        json!({
                            "stage": "reconciliation_only_report_exchange_reconciliation",
                            "exchange": order_task.exchange.as_str(),
                            "symbol": order_task.symbol,
                            "issue_type": request.issue_type.as_str(),
                            "source_ref": request.source_ref,
                            "error": error.to_string(),
                            "place_order_allowed": false,
                            "mutation_allowed": false,
                            "report_result_allowed": false,
                        }),
                    )
                    .await;
                }
            }
        }

        self.record_checkpoint(
            "reconciliation_only_checked",
            Some(task.id),
            json!({
                "stage": "reconciliation_only_signed_read",
                "exchange": order_task.exchange.as_str(),
                "symbol": order_task.symbol,
                "account_ref": reconciliation_account_ref(task),
                "source_refs": source_refs,
                "position_count": positions.len(),
                "non_zero_position_count": positions
                    .iter()
                    .filter(|position| positive_decimal_text(&position.size))
                    .count(),
                "open_order_count": open_orders.len(),
                "active_open_order_count": open_orders
                    .iter()
                    .filter(|order| active_open_order_status(order.status.as_deref()))
                    .count(),
                "issue_count": requests.len(),
                "reported_issue_count": reported_count,
                "report_error_count": report_error_count,
                "signed_read_only": !self.config.dry_run,
                "gateway_read_failed": false,
                "place_order_allowed": false,
                "mutation_allowed": false,
                "report_result_allowed": false,
                "place_order_retried": false,
            }),
        )
        .await;
    }
    async fn record_reconciliation_only_read_failure(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
        read_stage: &str,
        error: String,
    ) {
        let source_ref = build_exchange_reconciliation_source_ref(
            task,
            order_task.exchange.as_str(),
            &order_task.symbol,
            "gateway_read_failed",
        );
        self.record_checkpoint(
            "reconciliation_only_gateway_read_failed",
            Some(task.id),
            json!({
                "stage": "reconciliation_only_signed_read",
                "read_stage": read_stage,
                "exchange": order_task.exchange.as_str(),
                "symbol": order_task.symbol,
                "account_ref": reconciliation_account_ref(task),
                "source_ref": source_ref,
                "error": error,
                "signed_read_only": !self.config.dry_run,
                "gateway_read_failed": true,
                "place_order_allowed": false,
                "mutation_allowed": false,
                "report_result_allowed": false,
                "place_order_retried": false,
            }),
        )
        .await;
    }
}
