impl ExecutionWorker {
    pub async fn run_once(&self) -> Result<usize> {
        self.ensure_live_audit_repository()?;

        if self.config.report_replay_mode {
            return self.run_report_replay_once().await;
        }
        if self.config.confirmation_mode {
            return self.run_confirmation_once().await;
        }
        if reconciliation_only_mode_from_env() {
            return self.run_reconciliation_only_once().await;
        }

        self.record_checkpoint(
            "leasing",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
                "default_exchange": self.config.default_exchange.as_str(),
                "task_types": self.config.task_types.clone(),
                "task_statuses": self.config.task_statuses.clone(),
                "target_task_ids": self.config.target_task_ids.clone(),
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
                        "stage": "lease_tasks",
                        "error": error.to_string(),
                    }),
                )
                .await;
                return Err(error);
            }
        };

        self.record_checkpoint(
            "leased",
            None,
            json!({
                "leased_count": leased.tasks.len(),
                "lease_limit": self.config.lease_limit,
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let mut handled = 0;
        let mut last_task_id = None;
        for task in leased.tasks {
            if !self.config.leased_task_allowed(task.id) {
                warn!(
                    task_id = task.id,
                    target_task_ids = ?self.config.target_task_ids,
                    "leased execution task is outside EXECUTION_WORKER_TARGET_TASK_IDS; skipping order execution"
                );
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(task.id),
                    json!({
                        "stage": "target_task_allowlist",
                        "task_id": task.id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                    }),
                )
                .await;
                continue;
            }

            let report = self.execute_task(&task).await;
            let report_status = report.execution_status.clone();
            if let Err(error) = self.client.report_result(report.clone()).await {
                error!(task_id = task.id, "回写执行任务结果失败: {}", error);
                self.record_report_result_failure(
                    task.id,
                    &report,
                    error.to_string(),
                    "report_result",
                )
                .await;
            } else {
                self.record_checkpoint(
                    &report_status,
                    Some(task.id),
                    json!({
                        "stage": "report_result",
                        "execution_status": report_status,
                    }),
                )
                .await;
            }
            last_task_id = Some(task.id);
            handled += 1;
        }
        self.record_checkpoint(
            "idle",
            last_task_id,
            json!({
                "handled": handled,
                "dry_run": self.config.dry_run,
            }),
        )
        .await;
        Ok(handled)
    }

    pub async fn report_exchange_reconciliation_for_task(
        &self,
        task: &ExecutionTask,
        issue_type: ExchangeReconciliationIssueType,
        detected_at: Option<String>,
        message: impl Into<String>,
    ) -> Result<ExchangeReconciliationReportResponse> {
        let request =
            build_exchange_reconciliation_report_request(task, issue_type, detected_at, message);
        let response = self
            .client
            .report_exchange_reconciliation(request.clone())
            .await?;
        self.record_checkpoint(
            "exchange_reconciliation_reported",
            Some(task.id),
            json!({
                "stage": "exchange_reconciliation",
                "combo_id": request.combo_id,
                "buyer_email": request.buyer_email,
                "symbol": request.symbol,
                "issue_type": request.issue_type.as_str(),
                "source_ref": request.source_ref,
                "place_order_allowed": false,
            }),
        )
        .await;
        Ok(response)
    }

    async fn check_exchange_reconciliation_before_live_order(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
        gateway: &CryptoExcAllGateway,
    ) -> Result<Option<ExecutionTaskReportRequest>> {
        let instrument = parse_instrument(&order_task.symbol)?;
        let positions = gateway
            .positions(order_task.exchange, Some(&instrument))
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange position reconciliation failed before live order: {}",
                    error
                )
            })?;
        let open_orders = gateway
            .open_orders(
                order_task.exchange,
                OrderListQuery::for_instrument(instrument).with_limit(100),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange open-order reconciliation failed before live order: {}",
                    error
                )
            })?;
        let requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
            task,
            &positions,
            &open_orders,
            None,
        );
        if requests.is_empty() {
            return Ok(None);
        }

        for request in &requests {
            self.client
                .report_exchange_reconciliation(request.clone())
                .await?;
            self.record_checkpoint(
                "exchange_reconciliation_read_only_blocker_reported",
                Some(task.id),
                json!({
                    "stage": "exchange_reconciliation_read_only",
                    "combo_id": request.combo_id,
                    "buyer_email": request.buyer_email,
                    "symbol": request.symbol,
                    "issue_type": request.issue_type.as_str(),
                    "source_ref": request.source_ref,
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            )
            .await;
        }

        Ok(Some(
            build_live_order_blocked_by_exchange_reconciliation_report(task, order_task, &requests),
        ))
    }

    async fn check_exchange_read_only_before_pending_close(
        &self,
        task: &ExecutionTask,
        request: &OrderPlacementRequest,
        gateway: &CryptoExcAllGateway,
    ) -> Result<()> {
        let instrument = request.instrument.clone();
        let positions = gateway
            .positions(request.exchange, Some(&instrument))
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange position reconciliation failed before pending close: {}",
                    error
                )
            })?;
        let open_orders = gateway
            .open_orders(
                request.exchange,
                OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange open-order reconciliation failed before pending close: {}",
                    error
                )
            })?;
        self.record_checkpoint(
            "pending_close_exchange_reconciliation_read_only_checked",
            Some(task.id),
            json!({
                "stage": "pending_close_exchange_reconciliation_read_only",
                "exchange": request.exchange.as_str(),
                "symbol": instrument.symbol_for(request.exchange),
                "position_count": positions.len(),
                "open_order_count": open_orders.len(),
                "place_order_allowed": true,
                "mutation_allowed": false,
            }),
        )
        .await;

        Ok(())
    }

    async fn run_confirmation_once(&self) -> Result<usize> {
        self.record_checkpoint(
            "leasing_confirmations",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let leased = match self
            .client
            .lease_confirmation_tasks(self.config.lease_limit)
            .await
        {
            Ok(leased) => leased,
            Err(error) => {
                self.record_checkpoint(
                    "failed",
                    None,
                    json!({
                        "stage": "lease_confirmation_tasks",
                        "error": error.to_string(),
                    }),
                )
                .await;
                return Err(error);
            }
        };

        self.record_checkpoint(
            "confirmations_leased",
            None,
            json!({
                "leased_count": leased.items.len(),
                "lease_limit": self.config.lease_limit,
                "target_task_ids": self.config.target_task_ids.clone(),
            }),
        )
        .await;

        let mut handled = 0;
        let mut last_task_id = None;
        for item in leased.items {
            if !self.config.leased_task_allowed(item.task.id) {
                warn!(
                    task_id = item.task.id,
                    target_task_ids = ?self.config.target_task_ids,
                    "leased pending confirmation task is outside EXECUTION_WORKER_TARGET_TASK_IDS; skipping confirmation"
                );
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(item.task.id),
                    json!({
                        "stage": "confirmation_target_task_allowlist",
                        "task_id": item.task.id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                    }),
                )
                .await;
                continue;
            }

            let report = self.execute_pending_confirmation_item(&item).await;
            let report_status = report.execution_status.clone();
            if let Err(error) = self.client.report_result(report.clone()).await {
                error!(task_id = item.task.id, "回写执行确认结果失败: {}", error);
                self.record_report_result_failure(
                    item.task.id,
                    &report,
                    error.to_string(),
                    "report_confirmation_result",
                )
                .await;
            } else {
                self.record_checkpoint(
                    &report_status,
                    Some(item.task.id),
                    json!({
                        "stage": "report_confirmation_result",
                        "execution_status": report_status,
                    }),
                )
                .await;
            }
            last_task_id = Some(item.task.id);
            handled += 1;
        }

        self.record_checkpoint(
            "idle",
            last_task_id,
            json!({
                "handled": handled,
                "confirmation_mode": true,
            }),
        )
        .await;
        Ok(handled)
    }

    async fn run_report_replay_once(&self) -> Result<usize> {
        let replay_limit = self.config.report_replay_limit();
        let failure_backoff_seconds = self.config.report_replay_failure_backoff_seconds;
        let throttle_ms = self.config.report_replay_throttle_ms;
        self.record_checkpoint(
            "leasing_report_replays",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "replay_limit": replay_limit,
                "report_replay_max_per_run": self.config.report_replay_max_per_run,
                "failure_backoff_seconds": failure_backoff_seconds,
                "throttle_ms": throttle_ms,
                "target_task_ids": self.config.target_task_ids.clone(),
                "place_order_allowed": false,
            }),
        )
        .await;

        let candidates = self
            .audit_repository
            .list_report_result_replay_candidates(replay_limit, failure_backoff_seconds)
            .await?;
        let leased_count = candidates.len();

        self.record_checkpoint(
            "report_replays_leased",
            None,
            json!({
                "leased_count": leased_count,
                "lease_limit": self.config.lease_limit,
                "replay_limit": replay_limit,
                "failure_backoff_seconds": failure_backoff_seconds,
                "throttle_ms": throttle_ms,
                "target_task_ids": self.config.target_task_ids.clone(),
                "place_order_allowed": false,
            }),
        )
        .await;

        let mut handled = 0;
        let mut replayed = 0;
        let mut failed = 0;
        let mut skipped_target_task_mismatch = 0;
        let mut last_task_id = None;
        let mut request_ids = Vec::new();
        let mut replayed_request_ids = Vec::new();
        let mut failed_request_ids = Vec::new();
        let mut skipped_request_ids = Vec::new();
        for candidate in candidates {
            request_ids.push(candidate.request_id.clone());
            if !self.config.leased_task_allowed(candidate.report.task_id) {
                skipped_target_task_mismatch += 1;
                skipped_request_ids.push(candidate.request_id.clone());
                self.record_checkpoint(
                    "skipped_target_task_mismatch",
                    Some(candidate.report.task_id),
                    json!({
                        "stage": "report_replay_target_task_allowlist",
                        "request_id": candidate.request_id,
                        "task_id": candidate.report.task_id,
                        "target_task_ids": self.config.target_task_ids.clone(),
                        "place_order_allowed": false,
                    }),
                )
                .await;
                continue;
            }

            if handled > 0 && throttle_ms > 0 {
                sleep(Duration::from_millis(throttle_ms)).await;
            }
            let task_id = candidate.report.task_id;
            let started_at = Instant::now();
            match self.client.report_result(candidate.report.clone()).await {
                Ok(response) => {
                    replayed += 1;
                    replayed_request_ids.push(candidate.request_id.clone());
                    let latency_ms = started_at.elapsed().as_millis().min(i32::MAX as u128) as i32;
                    self.record_exchange_request_audit(
                        ExchangeRequestAuditLog::report_result_replayed(
                            &candidate.report,
                            Some(latency_ms),
                            serde_json::to_value(&response).unwrap_or_else(|_| json!({})),
                        ),
                    )
                    .await;
                    self.record_checkpoint(
                        "report_replayed",
                        Some(task_id),
                        json!({
                            "stage": "report_replay",
                            "request_id": candidate.request_id,
                            "task_id": task_id,
                            "execution_status": candidate.report.execution_status,
                            "order_status": candidate.report.order_status,
                            "place_order_allowed": false,
                        }),
                    )
                    .await;
                }
                Err(error) => {
                    failed += 1;
                    failed_request_ids.push(candidate.request_id.clone());
                    self.record_report_result_failure(
                        task_id,
                        &candidate.report,
                        error.to_string(),
                        "report_replay",
                    )
                    .await;
                }
            }

            last_task_id = Some(task_id);
            handled += 1;
        }

        let health_status = if failed > 0 { "warn" } else { "ok" };
        let health_code = if failed > 0 {
            "QUANT_REPORT_REPLAY_FAILED"
        } else {
            "QUANT_REPORT_REPLAY_READY"
        };
        let batch_payload = json!({
            "handled": handled,
            "report_replay_mode": true,
            "place_order_allowed": false,
            "report_replay": {
                "leased_count": leased_count,
                "attempted_count": handled,
                "replayed_count": replayed,
                "failed_count": failed,
                "skipped_target_task_mismatch_count": skipped_target_task_mismatch,
                "lease_limit": self.config.lease_limit,
                "replay_limit": replay_limit,
                "max_per_run": self.config.report_replay_max_per_run,
                "failure_backoff_seconds": failure_backoff_seconds,
                "throttle_ms": throttle_ms,
                "request_ids": request_ids,
                "replayed_request_ids": replayed_request_ids,
                "failed_request_ids": failed_request_ids,
                "skipped_request_ids": skipped_request_ids,
            },
            "health_handoff": {
                "section": "quant_worker_checkpoint_audit",
                "status": health_status,
                "code": health_code,
                "read_only_input": false,
            },
            "operator_playbook_summary": report_replay_operator_playbook_summary(
                failed,
                failure_backoff_seconds,
            ),
        });
        self.record_checkpoint(
            if failed > 0 {
                "report_replay_batch_degraded"
            } else {
                "report_replay_batch_completed"
            },
            last_task_id,
            batch_payload.clone(),
        )
        .await;
        self.record_checkpoint("idle", last_task_id, batch_payload)
            .await;
        Ok(handled)
    }

}
