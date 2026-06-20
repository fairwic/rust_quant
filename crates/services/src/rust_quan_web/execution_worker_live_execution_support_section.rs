impl ExecutionWorker {
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

    async fn resolve_live_gateway(
        &self,
        buyer_email: &str,
        exchange: ExchangeId,
    ) -> Result<CryptoExcAllGateway> {
        let config = self
            .client
            .resolve_user_exchange_config(buyer_email, exchange.as_str())
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

    async fn live_api_credential_preflight_report(
        &self,
        task: &ExecutionTask,
        order_task: &ExecutionOrderTask,
    ) -> Option<ExecutionTaskReportRequest> {
        let credential_id = api_credential_id_from_task(task)?;
        let checked = match self
            .client
            .check_internal_api_credential(credential_id)
            .await
        {
            Ok(checked) => checked,
            Err(error) => {
                return Some(ExecutionTaskReportRequest::failed(
                    task.id,
                    order_task.exchange.as_str(),
                    order_side_lower(order_task.side),
                    format!(
                        "API credential preflight failed before live order: {error}; place_order_allowed=false; mutation_allowed=false"
                    ),
                    json!({
                        "task_id": task.id,
                        "stage": "api_credential_preflight",
                        "api_credential_id": credential_id,
                        "exchange": order_task.exchange.as_str(),
                        "symbol": order_task.symbol,
                        "place_order_allowed": false,
                        "mutation_allowed": false,
                    }),
                ));
            }
        };

        if !api_credential_exchange_matches_task(&checked.exchange, order_task.exchange) {
            return Some(ExecutionTaskReportRequest::failed(
                task.id,
                order_task.exchange.as_str(),
                order_side_lower(order_task.side),
                format!(
                    "API credential preflight returned exchange {} for task exchange {}; place_order_allowed=false; mutation_allowed=false",
                    checked.exchange,
                    order_task.exchange.as_str()
                ),
                json!({
                    "task_id": task.id,
                    "stage": "api_credential_preflight",
                    "api_credential_id": credential_id,
                    "credential_exchange": checked.exchange,
                    "task_exchange": order_task.exchange.as_str(),
                    "symbol": order_task.symbol,
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
            order_task.exchange.as_str(),
            order_side_lower(order_task.side),
            format!(
                "API credential preflight blocked live order: {blocker_code}: {blocker_message}; place_order_allowed=false; mutation_allowed=false"
            ),
            json!({
                "task_id": task.id,
                "stage": "api_credential_preflight",
                "api_credential_id": credential_id,
                "exchange": order_task.exchange.as_str(),
                "symbol": order_task.symbol,
                "last_check_code": checked.last_check_code,
                "blocker_code": checked.execution_readiness.blocker_code,
                "blocker_message": checked.execution_readiness.blocker_message,
                "place_order_allowed": false,
                "mutation_allowed": false,
            }),
        ))
    }

    async fn prepare_binance_order_settings_after_protection(
        &self,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<()> {
        if order_task.exchange != ExchangeId::Binance
            || (order_task.margin_mode.is_none()
                && order_task.leverage.is_none()
                && order_task.position_mode.is_none())
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
        gateway
            .prepare_order_settings(order_task.exchange, prepare)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

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

        report
    }

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

        match gateway.order(request.exchange, lookup.query.clone()).await {
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
