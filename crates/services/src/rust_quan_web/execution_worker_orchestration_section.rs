impl ExecutionWorker {
    /// 执行 worker 的一次主循环：先确定运行模式，再租约任务、执行、回写结果。
    /// 实盘路径必须从这里统一记录 checkpoint，避免交易所侧已发生动作但 Web 状态缺少证据。
    pub async fn run_once(&self) -> Result<usize> {
        self.validate_runtime_scope()?;
        self.ensure_live_audit_repository()?;
        // 维护模式不进入普通租约路径，避免重放报告或确认订单时误触发新下单。
        if self.config.report_replay_mode {
            return self.run_report_replay_once().await;
        }
        if self.config.confirmation_mode {
            return self.run_confirmation_once().await;
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
            }),
        )
        .await;
        // 租约由 Web owner service 发放，Core 只处理拿到租约的任务；这样同一任务不会被多个 worker
        // 并发执行，也能把 task_types/statuses 的筛选规则集中在 Web 的状态机里。
        let leased = match self
            .client
            .lease_tasks(ExecutionTaskLeaseRequest {
                worker_id: self.config.worker_id.clone(),
                limit: self.config.lease_limit,
                task_ids: Vec::new(),
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
            }),
        )
        .await;
        if !leased.tasks.is_empty() {
            let leased_task_ids: Vec<i64> = leased.tasks.iter().map(|task| task.id).collect();
            let strategy_signal_ids: Vec<Option<i64>> =
                leased.tasks.iter().map(|task| task.strategy_signal_id).collect();
            info!(
                worker_id = %self.config.worker_id,
                dry_run = self.config.dry_run,
                leased_task_count = leased.tasks.len(),
                leased_task_ids = ?leased_task_ids,
                strategy_signal_ids = ?strategy_signal_ids,
                task_types = ?self.config.task_types,
                task_statuses = ?self.config.task_statuses,
                "execution worker leased tasks from quant_web"
            );
        }
        let mut handled = 0;
        let mut last_task_id = None;
        for task in leased.tasks {
            let lease_extend_worker_id = task
                .lease_owner
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(self.config.worker_id.as_str());
            info!(
                worker_id = %self.config.worker_id,
                lease_owner = %lease_extend_worker_id,
                execution_task_id = task.id,
                strategy_signal_id = ?task.strategy_signal_id,
                news_signal_id = ?task.news_signal_id,
                combo_id = task.combo_id,
                buyer_email = %task.buyer_email,
                strategy_slug = %task.strategy_slug,
                symbol = %task.symbol,
                task_type = %task.task_type,
                task_status = %task.task_status,
                dry_run = self.config.dry_run,
                "execution worker starts leased task"
            );
            if let Err(error) = self
                .client
                .extend_task_lease(
                    task.id,
                    ExecutionTaskLeaseExtendRequest {
                        worker_id: lease_extend_worker_id.to_string(),
                        extend_seconds: Some(120),
                    },
                )
                .await
            {
                let report = ExecutionTaskReportRequest::failed(
                    task.id,
                    self.config.default_exchange.as_str(),
                    "unknown",
                    format!("execution task lease heartbeat failed: {error}"),
                    json!({
                        "task_id": task.id,
                        "stage": "execution_task_lease_extend",
                        "mutation_allowed": false,
                        "place_order_allowed": false,
                    }),
                );
                if let Err(report_error) = self.client.report_result(report.clone()).await {
                    error!(
                        task_id = task.id,
                        "回写执行任务租约续租失败结果失败: {}", report_error
                    );
                    self.record_report_result_failure(
                        task.id,
                        &report,
                        report_error.to_string(),
                        "lease_extend_failed_report_result",
                    )
                    .await;
                } else {
                    self.record_checkpoint(
                        "lease_extend_failed",
                        Some(task.id),
                        json!({
                            "stage": "execution_task_lease_extend",
                            "error": error.to_string(),
                            "place_order_allowed": false,
                            "mutation_allowed": false,
                        }),
                    )
                    .await;
                }
                last_task_id = Some(task.id);
                handled += 1;
                continue;
            }
            self.record_checkpoint(
                "lease_extended",
                Some(task.id),
                json!({
                    "stage": "execution_task_lease_extend",
                    "extend_seconds": 120,
                }),
            )
            .await;
            let report = self.execute_task(&task).await;
            let report_status = report.execution_status.clone();
            // 交易执行结果必须回写 Web；回写失败不重试下单，只记录可重放证据，避免重复 mutation。
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
                info!(
                    worker_id = %self.config.worker_id,
                    execution_task_id = task.id,
                    strategy_signal_id = ?task.strategy_signal_id,
                    combo_id = task.combo_id,
                    report_status = %report_status,
                    "execution worker reported task result to quant_web"
                );
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
    /// 提供报告交易所reconciliationfortask的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    async fn check_exchange_reconciliation_before_live_order(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
        gateway: &CryptoExcAllGateway,
    ) -> Result<Option<ExecutionTaskReportRequest>> {
        let instrument = parse_instrument(&order_task.symbol)?;
        // 真实下单前先做 signed read-only 对账，用交易所当前仓位阻断重复开仓或脏状态；
        // 这里失败时返回 blocker，而不是继续尝试 place_order。
        let positions = CryptoExcAllGateway::with_signed_read_only_scope(
            gateway.positions(order_task.exchange, Some(&instrument)),
        )
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange position reconciliation failed before live order: {}",
                    error
                )
            })?;
        // open orders 和 positions 一起判断，避免已有挂单尚未成交时再次提交同方向订单。
        let open_orders = CryptoExcAllGateway::with_signed_read_only_scope(gateway.open_orders(
                order_task.exchange,
                OrderListQuery::for_instrument(instrument).with_limit(100),
            ))
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
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    async fn check_exchange_read_only_before_pending_close(
        &self,
        task: &ExecutionTask,
        request: &OrderPlacementRequest,
        gateway: &CryptoExcAllGateway,
        planned_protective_cancel: Option<&(ExchangeId, CancelOrderRequest)>,
    ) -> Result<()> {
        let instrument = request.instrument.clone();
        let positions = CryptoExcAllGateway::with_signed_read_only_scope(
            gateway.positions(request.exchange, Some(&instrument)),
        )
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange position reconciliation failed before pending close: {}",
                    error
                )
            })?;
        let open_orders = CryptoExcAllGateway::with_signed_read_only_scope(gateway.open_orders(
                request.exchange,
                OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
            ))
            .await
            .map_err(|error| {
                anyhow!(
                    "read-only exchange open-order reconciliation failed before pending close: {}",
                    error
                )
            })?;
        let matching_position_count = positions
            .iter()
            .filter(|position| pending_close_has_matching_position(position, request))
            .count();
        let conflicting_open_order_count = open_orders
            .iter()
            .filter(|order| {
                pending_close_has_conflicting_open_order(
                    order,
                    request,
                    planned_protective_cancel.map(|(_, request)| request),
                )
            })
            .count();
        if matching_position_count == 0 {
            self.record_checkpoint(
                "pending_close_exchange_reconciliation_read_only_blocked",
                Some(task.id),
                json!({
                    "stage": "pending_close_exchange_reconciliation_read_only",
                    "exchange": request.exchange.as_str(),
                    "symbol": instrument.symbol_for(request.exchange),
                    "position_count": positions.len(),
                    "open_order_count": open_orders.len(),
                    "matching_position_count": matching_position_count,
                    "blocker_code": "pending_close_no_matching_position",
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            )
            .await;
            return Err(anyhow!(
                "pending_close_no_matching_position: signed account has no matching non-zero position for {} {}; place_order_allowed=false; mutation_allowed=false",
                request.exchange.as_str(),
                instrument.symbol_for(request.exchange)
            ));
        }
        if conflicting_open_order_count > 0 {
            self.record_checkpoint(
                "pending_close_exchange_reconciliation_read_only_blocked",
                Some(task.id),
                json!({
                    "stage": "pending_close_exchange_reconciliation_read_only",
                    "exchange": request.exchange.as_str(),
                    "symbol": instrument.symbol_for(request.exchange),
                    "position_count": positions.len(),
                    "open_order_count": open_orders.len(),
                    "matching_position_count": matching_position_count,
                    "conflicting_open_order_count": conflicting_open_order_count,
                    "blocker_code": "pending_close_active_close_order_conflict",
                    "place_order_allowed": false,
                    "mutation_allowed": false,
                }),
            )
            .await;
            return Err(anyhow!(
                "pending_close_active_close_order_conflict: signed account already has active close-side open orders for {} {}; place_order_allowed=false; mutation_allowed=false",
                request.exchange.as_str(),
                instrument.symbol_for(request.exchange)
            ));
        }
        self.record_checkpoint(
            "pending_close_exchange_reconciliation_read_only_checked",
            Some(task.id),
            json!({
                "stage": "pending_close_exchange_reconciliation_read_only",
                "exchange": request.exchange.as_str(),
                "symbol": instrument.symbol_for(request.exchange),
                "position_count": positions.len(),
                "open_order_count": open_orders.len(),
                "matching_position_count": matching_position_count,
                "conflicting_open_order_count": conflicting_open_order_count,
                "place_order_allowed": false,
                "mutation_allowed": false,
            }),
        )
        .await;
        Ok(())
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn run_confirmation_once(&self) -> Result<usize> {
        self.record_checkpoint(
            "leasing_confirmations",
            None,
            json!({
                "lease_limit": self.config.lease_limit,
                "dry_run": self.config.dry_run,
            }),
        )
        .await;
        let leased = match self
            .client
            .lease_confirmation_tasks(self.config.lease_limit, &[])
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
            }),
        )
        .await;
        let mut handled = 0;
        let mut last_task_id = None;
        for item in leased.items {
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
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
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
            .list_report_result_replay_candidates(
                replay_limit,
                failure_backoff_seconds,
                &self.config.target_task_ids,
            )
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
            if !self.config.report_replay_task_allowed(candidate.report.task_id) {
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
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn pending_close_has_matching_position(position: &Position, request: &OrderPlacementRequest) -> bool {
    if position.exchange != request.exchange {
        return false;
    }
    if !position
        .exchange_symbol
        .eq_ignore_ascii_case(&request.instrument.symbol_for(request.exchange))
    {
        return false;
    }
    let Some(size) = position_size(&position.size).filter(|size| *size != 0.0) else {
        return false;
    };
    let Some(request_size) = position_size(&request.size).filter(|size| *size > 0.0) else {
        return false;
    };
    if request_size > size.abs() + f64::EPSILON {
        return false;
    }
    let expected_side = pending_close_expected_position_side(request);
    let actual_side = normalized_position_side(position.side.as_deref(), size);
    match (expected_side.as_deref(), actual_side.as_deref()) {
        (Some(expected), Some(actual)) => expected == actual,
        (Some(_), None) => false,
        (None, _) => true,
    }
}
/// 提供pending平仓hasconflicting开仓订单的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn pending_close_has_conflicting_open_order(
    order: &Order,
    request: &OrderPlacementRequest,
    planned_protective_cancel: Option<&CancelOrderRequest>,
) -> bool {
    if order.exchange != request.exchange {
        return false;
    }
    if !order
        .exchange_symbol
        .eq_ignore_ascii_case(&request.instrument.symbol_for(request.exchange))
    {
        return false;
    }
    if !active_open_order_status(order.status.as_deref()) {
        return false;
    }
    if !pending_close_order_side_matches_request(order.side.as_deref(), request.side) {
        return false;
    }
    if planned_protective_cancel
        .is_some_and(|cancel| open_order_matches_cancel_request(order, cancel))
    {
        return false;
    }
    true
}
/// 提供pending平仓订单sidematchesrequest的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn pending_close_order_side_matches_request(order_side: Option<&str>, request_side: OrderSide) -> bool {
    let Some(order_side) = order_side.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    matches!(
        (order_side.to_ascii_lowercase().as_str(), request_side),
        ("buy", OrderSide::Buy) | ("sell", OrderSide::Sell)
    )
}
/// 提供开仓订单matchescancelrequest的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn open_order_matches_cancel_request(order: &Order, cancel: &CancelOrderRequest) -> bool {
    if order.instrument != cancel.instrument {
        return false;
    }
    if let Some(cancel_client_id) = cancel.client_order_id.as_deref() {
        if order.client_order_id.as_deref() == Some(cancel_client_id) {
            return true;
        }
    }
    if let Some(cancel_order_id) = cancel.order_id.as_deref() {
        if order.order_id.as_deref() == Some(cancel_order_id) {
            return true;
        }
    }
    false
}
/// 提供仓位size的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn position_size(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|size| size.is_finite())
}
/// 提供pending平仓expected仓位side的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn pending_close_expected_position_side(request: &OrderPlacementRequest) -> Option<String> {
    if let Some(position_side) = request
        .position_side
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let normalized = position_side.to_ascii_lowercase();
        if matches!(normalized.as_str(), "long" | "short") {
            return Some(normalized);
        }
    }
    match request.side {
        OrderSide::Sell => Some("long".to_string()),
        OrderSide::Buy => Some("short".to_string()),
    }
}
/// 解析归一化仓位方向，把外部输入转换成Web 商业链路可用的内部值。
fn normalized_position_side(side: Option<&str>, size: f64) -> Option<String> {
    let side = side.map(str::trim).filter(|value| !value.is_empty());
    match side.map(|value| value.to_ascii_lowercase()).as_deref() {
        Some("long") => Some("long".to_string()),
        Some("short") => Some("short".to_string()),
        Some("net" | "both") | None if size > 0.0 => Some("long".to_string()),
        Some("net" | "both") | None if size < 0.0 => Some("short".to_string()),
        _ => None,
    }
}
