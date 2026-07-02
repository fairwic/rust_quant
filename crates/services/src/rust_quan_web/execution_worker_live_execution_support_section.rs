impl ExecutionWorker {
    /// 下主单前向 Web owner service 原子预留最终风险预算；失败时调用方必须 fail closed。
    async fn reserve_live_execution_risk_budget(
        &self,
        task: &ExecutionTask,
        _order_task: &ExecutionOrderTask,
        minimum_notional_usdt: Option<f64>,
    ) -> Result<ExecutionRiskReservationResponse> {
        self.client
            .reserve_execution_risk_budget(
                task.id,
                ExecutionRiskReservationRequest {
                    minimum_notional_usdt,
                },
            )
            .await
    }
    /// 下单前根据交易所过滤器派生最小名义金额，避免预留一笔最终无法成交的风险预算。
    async fn live_order_minimum_notional_usdt(
        &self,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<Option<f64>> {
        let enforce_min_notional = !order_task.reduce_only.unwrap_or(false)
            && !matches!(
                order_task.trade_side.as_deref().map(|value| value.to_ascii_lowercase()),
                Some(value) if value == "close"
            );
        if !enforce_min_notional {
            return Ok(None);
        }
        let instrument = parse_instrument(&order_task.symbol)?;
        let ticker = gateway.ticker(order_task.exchange, &instrument).await?;
        let reference_price =
            live_order_reference_price(&ticker, order_task.side, current_time_millis_u64())?;
        let filters = load_exchange_order_filters(order_task.exchange, &order_task.symbol)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "missing exchange symbol filters for {} on {}; run exchange symbol sync before live risk reservation",
                    order_task.symbol,
                    order_task.exchange.as_str()
                )
            })?;
        minimum_order_notional_usdt(
            decimal_from_f64(reference_price)?,
            &filters,
            enforce_min_notional,
        )
    }
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn live_order_request(
        &self,
        gateway: &CryptoExcAllGateway,
        order_task: &ExecutionOrderTask,
    ) -> Result<OrderPlacementRequest> {
        let instrument = parse_instrument(&order_task.symbol)?;
        let now_ms = current_time_millis_u64();
        let ticker = gateway.ticker(order_task.exchange, &instrument).await?;
        let reference_price = live_order_reference_price(&ticker, order_task.side, now_ms)?;
        let orderbook = gateway
            .orderbook(
                order_task.exchange,
                OrderBookQuery::new(instrument).with_limit(LIVE_ORDERBOOK_DEPTH_LIMIT),
            )
            .await?;
        let filters =
            load_exchange_order_filters(order_task.exchange, &order_task.symbol)
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "missing exchange symbol filters for {} on {}; run exchange symbol sync before live order",
                        order_task.symbol,
                        order_task.exchange.as_str()
                    )
                })?;
        let request = order_task.to_live_order_request(Some(reference_price), Some(&filters))?;
        validate_live_orderbook_execution_boundary(
            &orderbook,
            &request,
            reference_price,
            &filters,
            now_ms,
        )?;
        Ok(request)
    }
    /// 在实盘订单 mutation 前用交易所账户事实限制最终 size；只允许裁剪变小，不能放大策略或风控预算。
    async fn apply_live_max_order_size_gate(
        &self,
        task: &ExecutionTask,
        gateway: &CryptoExcAllGateway,
        order_task: &mut ExecutionOrderTask,
        request: &mut OrderPlacementRequest,
    ) -> Result<Option<MaxOrderSizeGateOutcome>> {
        let is_close_or_reduce_only = request.reduce_only.unwrap_or(false)
            || matches!(
                request.trade_side.as_deref().map(|value| value.to_ascii_lowercase()),
                Some(value) if value == "close"
            );
        if is_close_or_reduce_only {
            return Ok(None);
        }
        let instrument = parse_instrument(&order_task.symbol)?;
        let now_ms = current_time_millis_u64();
        let ticker = gateway.ticker(order_task.exchange, &instrument).await?;
        let reference_price = live_order_reference_price(&ticker, order_task.side, now_ms)?;
        let filters =
            load_exchange_order_filters(order_task.exchange, &order_task.symbol)
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "missing exchange symbol filters for {} on {}; run exchange symbol sync before live max-size preflight",
                        order_task.symbol,
                        order_task.exchange.as_str()
                    )
                })?;
        let reference_price = decimal_from_f64(reference_price)?;
        let margin_mode = request
            .margin_mode
            .clone()
            .or_else(|| order_task.margin_mode.clone())
            .ok_or_else(|| anyhow!("live_max_order_size_margin_mode_missing"))?;
        let mut max_size_request = MaxOrderSizeRequest::new(instrument, margin_mode);
        if let Some(margin_coin) = request
            .margin_coin
            .as_ref()
            .or(order_task.margin_coin.as_ref())
            .filter(|value| !value.trim().is_empty())
        {
            max_size_request = max_size_request.with_margin_coin(margin_coin.clone());
        }
        let price = request
            .price
            .clone()
            .unwrap_or_else(|| format_order_price_decimal(reference_price, &filters));
        max_size_request = max_size_request.with_price(price);
        if let Some(leverage) = order_task
            .leverage
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            max_size_request = max_size_request.with_leverage(leverage.clone());
        }
        let max_order_size = CryptoExcAllGateway::with_signed_read_only_scope(
            gateway.max_order_size(order_task.exchange, max_size_request),
        )
        .await?;
        let raw_max_size = match request.side {
            OrderSide::Buy => max_order_size.max_buy.as_str(),
            OrderSide::Sell => max_order_size.max_sell.as_str(),
        };
        let available_size = parse_positive_decimal(raw_max_size, "exchange max order size")?;
        let outcome = apply_exchange_max_order_size_to_request(
            request,
            available_size,
            reference_price,
            &filters,
        )?;
        if outcome.clipped {
            order_task.size = request.size.clone();
            info!(
                worker_id = %self.config.worker_id,
                execution_task_id = task.id,
                strategy_signal_id = ?task.strategy_signal_id,
                combo_id = task.combo_id,
                exchange = %order_task.exchange.as_str(),
                symbol = %order_task.symbol.as_str(),
                requested_size = %outcome.requested_size,
                max_available_size = %outcome.max_available_size,
                normalized_size = %outcome.normalized_size,
                "execution worker clipped live order size by exchange max-size preflight"
            );
            self.record_checkpoint(
                "live_max_order_size_clipped",
                Some(task.id),
                json!({
                    "stage": "max_order_size_preflight",
                    "exchange": order_task.exchange.as_str(),
                    "symbol": order_task.symbol,
                    "side": order_side_lower(order_task.side),
                    "requested_size": outcome.requested_size,
                    "max_available_size": outcome.max_available_size,
                    "normalized_size": outcome.normalized_size,
                    "place_order_allowed": true,
                    "mutation_allowed": false,
                }),
            )
            .await;
        }
        Ok(Some(outcome))
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
                let blocker_code = quant_web_readiness_blocker_code(&error).map(str::to_string);
                let blocker_message = blocker_code
                    .as_deref()
                    .map(|code| format!("Web API credential readiness blocked with {code}"));
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
                        "blocker_code": blocker_code,
                        "blocker_message": blocker_message,
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
    /// 在查询账户最大可下单数量前应用策略对应的账户杠杆、保证金和持仓模式。
    async fn prepare_order_settings_for_live_order(
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

/// 识别 Web owner service 已结构化的 API Key readiness blocker，未知 400 仍按错误处理。
fn quant_web_readiness_blocker_code(error: &anyhow::Error) -> Option<&str> {
    error
        .downcast_ref::<QuantWebClientError>()
        .and_then(QuantWebClientError::error_code)
        .filter(|code| matches!(*code, "ACTIVE_MEMBERSHIP_REQUIRED" | "MEMBERSHIP_EXPIRED"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MaxOrderSizeGateOutcome {
    /// 原始订单 size，已是本地过滤器量化后的下单单位。
    requested_size: String,
    /// 交易所账户返回的当前最大可下单数量。
    max_available_size: String,
    /// 应用交易所最大可下单数量和本地过滤器后的最终 size。
    normalized_size: String,
    /// 是否因为交易所账户可用数量不足而裁剪订单。
    clipped: bool,
}

/// 将交易所最大可下单数量应用到最终下单请求；只允许把订单 size 向下裁剪。
fn apply_exchange_max_order_size_to_request(
    request: &mut OrderPlacementRequest,
    max_available_size: rust_decimal::Decimal,
    reference_price: rust_decimal::Decimal,
    filters: &ExchangeOrderFilters,
) -> Result<MaxOrderSizeGateOutcome> {
    let requested_size = parse_positive_decimal(&request.size, "order size")?;
    let max_available_size_text = max_available_size.normalize().to_string();
    if max_available_size <= rust_decimal::Decimal::ZERO {
        return Err(anyhow!(
            "max_available_order_size_below_exchange_minimum: exchange max order size must be positive"
        ));
    }
    let enforce_min_notional = !request.reduce_only.unwrap_or(false)
        && !matches!(
            request.trade_side.as_deref().map(|value| value.to_ascii_lowercase()),
            Some(value) if value == "close"
        );
    let limited_size = requested_size.min(max_available_size);
    let normalized_size = quantize_order_size(
        limited_size,
        reference_price,
        filters,
        enforce_min_notional,
    )
    .map_err(|error| {
        anyhow!(
            "max_available_order_size_below_exchange_minimum: {}",
            error
        )
    })?;
    let clipped = normalized_size < requested_size;
    let normalized_size_text = format_order_size_decimal(normalized_size, filters);
    if clipped {
        request.size = normalized_size_text.clone();
    }
    Ok(MaxOrderSizeGateOutcome {
        requested_size: format_order_size_decimal(requested_size, filters),
        max_available_size: max_available_size_text,
        normalized_size: normalized_size_text,
        clipped,
    })
}
