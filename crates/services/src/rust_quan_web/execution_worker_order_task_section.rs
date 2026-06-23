struct PendingCloseTask {
    /// 任务 ID。
    task_id: i64,
    /// 交易所名称。
    exchange: ExchangeId,
    /// 交易对或资产符号。
    symbol: String,
    /// 类型标识。
    task_type: String,
    /// 状态值。
    task_status: String,
    /// 风险controlaction，用于风控判断或风险展示。
    risk_control_action: String,
    /// 是否需要人工处理。
    manual_review: Value,
    /// closeorderpayload；为空时表示该条件不启用。
    close_order_payload: Option<Value>,
}
impl PendingCloseTask {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    fn from_task(task: &ExecutionTask, default_exchange: ExchangeId) -> Result<Self> {
        let payload = order_payload(&task.request_payload_json);
        let exchange = payload_string(&payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(default_exchange);
        let symbol = payload_string(&payload, "symbol").unwrap_or_else(|| task.symbol.clone());
        let risk_control_action = payload
            .get("manual_review")
            .and_then(|value| value.get("action"))
            .and_then(Value::as_str)
            .or_else(|| {
                payload
                    .get("risk_control")
                    .and_then(|value| value.get("action"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("close_candidate")
            .trim()
            .to_string();
        let close_order_payload = payload.get("close_order").cloned().or_else(|| {
            payload
                .get("execution")
                .and_then(|value| value.get("close_order"))
                .cloned()
        });
        Ok(Self {
            task_id: task.id,
            exchange,
            symbol,
            task_type: task.task_type.clone(),
            task_status: task.task_status.clone(),
            risk_control_action,
            manual_review: payload.get("manual_review").cloned().unwrap_or(Value::Null),
            close_order_payload,
        })
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    fn to_order_request(&self) -> Result<Option<OrderPlacementRequest>> {
        let Some(payload) = self.close_order_payload.as_ref() else {
            return Ok(None);
        };
        let side = close_order_side(payload)?;
        let exchange = payload_string(payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(self.exchange);
        let symbol = payload_string(payload, "symbol").unwrap_or_else(|| self.symbol.clone());
        let order_type = payload_string(payload, "order_type")
            .map(|value| parse_order_type(&value))
            .transpose()?
            .unwrap_or(OrderType::Market);
        let position_side = payload_string(payload, "position_side");
        // Hedge-mode closes use position_side to constrain the side being reduced.
        // In that mode Binance rejects reduceOnly, while one-way close tasks should
        // still default to reduce_only=true.
        let default_reduce_only = match (exchange, position_side.as_deref()) {
            (ExchangeId::Okx, _) => None,
            (ExchangeId::Binance, Some(_)) => None,
            _ => Some(true),
        };
        Ok(Some(OrderPlacementRequest {
            exchange,
            instrument: parse_instrument(&symbol)?,
            side,
            order_type,
            size: payload_string(payload, "size")
                .or_else(|| payload_string(payload, "quantity"))
                .or_else(|| payload_string(payload, "qty"))
                .unwrap_or_else(|| "0".to_string()),
            price: payload_string(payload, "price"),
            margin_mode: payload_string(payload, "margin_mode").map(MarginMode::from),
            margin_coin: payload_string(payload, "margin_coin"),
            position_side,
            trade_side: payload_string(payload, "trade_side").or_else(|| Some("close".to_string())),
            client_order_id: payload_string(payload, "client_order_id")
                .or_else(|| Some(format!("rqclose{}", self.task_id))),
            reduce_only: payload_bool(payload, "reduce_only").or(default_reduce_only),
            time_in_force: payload_string(payload, "time_in_force")
                .map(|value| parse_time_in_force(&value))
                .transpose()?,
            attached_stop_loss_price: None,
        }))
    }
    /// 提供protectivecancelrequest的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn protective_cancel_request(&self) -> Result<Option<(ExchangeId, CancelOrderRequest)>> {
        let Some(payload) = self.close_order_payload.as_ref() else {
            return Ok(None);
        };
        let client_order_id = payload_string(payload, "cancel_protective_client_order_id")
            .or_else(|| payload_string(payload, "protective_order_client_id"));
        let order_id = payload_string(payload, "cancel_protective_order_id")
            .or_else(|| payload_string(payload, "protective_order_external_id"));
        if client_order_id.is_none() && order_id.is_none() {
            return Ok(None);
        }
        let exchange = payload_string(payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(self.exchange);
        let symbol = payload_string(payload, "symbol").unwrap_or_else(|| self.symbol.clone());
        let instrument = parse_instrument(&symbol)?;
        let mut request = if let Some(client_order_id) = client_order_id {
            CancelOrderRequest::by_client_order_id(instrument, client_order_id)
        } else {
            CancelOrderRequest::by_order_id(
                instrument,
                order_id.expect("checked above that order id is present"),
            )
        };
        if let Some(margin_coin) = payload_string(payload, "margin_coin") {
            request = request.with_margin_coin(margin_coin);
        }
        Ok(Some((exchange, request)))
    }
    /// 提供dryrun报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn dry_run_report(&self) -> ExecutionTaskReportRequest {
        ExecutionTaskReportRequest::success(
            self.task_id,
            self.exchange.as_str(),
            format!("dry-run-close-task-{}", self.task_id),
            "close",
            "dry_run",
            self.report_payload(true),
        )
    }
    fn missing_live_contract_message(&self) -> String {
        "pending_close task requires Web close_order payload before live execution".to_string()
    }
    /// 提供报告载荷的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn report_payload(&self, dry_run: bool) -> Value {
        json!({
            "dry_run": dry_run,
            "task_type": self.task_type.clone(),
            "task_status": self.task_status.clone(),
            "symbol": self.symbol.clone(),
            "risk_control_action": self.risk_control_action.clone(),
            "manual_review": self.manual_review.clone(),
            "close_order": self.close_order_payload.clone(),
        })
    }
}
#[derive(Debug, Clone)]
struct PendingConfirmationTask {
    /// 任务 ID。
    task_id: i64,
    /// 交易所名称。
    exchange: ExchangeId,
    /// 交易对或资产符号。
    symbol: String,
    /// externalorder ID；为空时使用默认值或表示不限制。
    external_order_id: Option<String>,
    /// clientorder ID；为空时使用默认值或表示不限制。
    client_order_id: Option<String>,
    /// 订单方向，用于会员、订单或支付链路。
    order_side: String,
    /// 订单状态。
    order_status: String,
}
impl PendingConfirmationTask {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn from_task_and_order_result(
        task: &ExecutionTask,
        exchange: &str,
        external_order_id: &str,
        order_side: &str,
        order_status: &str,
    ) -> Result<Self> {
        let exchange = parse_exchange(exchange)?;
        let order_task = ExecutionOrderTask::from_task_with_default(task, exchange).ok();
        let client_order_id = order_task
            .as_ref()
            .and_then(|order| order.client_order_id.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(format!("rqtask{}", task.id)));
        let external_order_id =
            Some(external_order_id.trim().to_string()).filter(|value| !value.is_empty());
        Ok(Self {
            task_id: task.id,
            exchange,
            symbol: order_task
                .map(|order| order.symbol)
                .unwrap_or_else(|| task.symbol.clone()),
            external_order_id,
            client_order_id,
            order_side: order_side.trim().to_string(),
            order_status: order_status.trim().to_string(),
        })
    }
    /// 从外部输入转换为内部模型，隔离 Web 商业、会员和执行准备度 的字段适配细节。
    fn from_confirmation_item(
        task: &ExecutionTask,
        order_result: &ExchangeOrderResult,
    ) -> Result<Self> {
        Self::from_task_and_order_result(
            task,
            &order_result.exchange,
            &order_result.external_order_id,
            &order_result.order_side,
            &order_result.order_status,
        )
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    fn to_order_query(&self) -> Result<OrderQuery> {
        let instrument = parse_instrument(&self.symbol)?;
        if let Some(external_order_id) = self
            .external_order_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if external_order_id.chars().all(|ch| ch.is_ascii_digit()) {
                return Ok(OrderQuery::by_order_id(instrument, external_order_id));
            }
            return Ok(OrderQuery::by_client_order_id(
                instrument,
                external_order_id,
            ));
        }
        if let Some(client_order_id) = self
            .client_order_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(OrderQuery::by_client_order_id(instrument, client_order_id));
        }
        Err(anyhow!(
            "pending_confirmation task {} requires exchange order id or client_order_id",
            self.task_id
        ))
    }
    /// 提供externalorclient订单ID的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn external_or_client_order_id(&self) -> String {
        self.external_order_id
            .as_ref()
            .or(self.client_order_id.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("pending-confirmation-task-{}", self.task_id))
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    fn to_order_ack(&self, order: Option<&Order>) -> OrderAck {
        let instrument = parse_instrument(&self.symbol)
            .expect("pending confirmation symbol was already parsed for order query");
        let order_id = order.and_then(|order| order.order_id.clone()).or_else(|| {
            self.external_order_id
                .as_ref()
                .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
                .cloned()
        });
        let client_order_id = order
            .and_then(|order| order.client_order_id.clone())
            .or_else(|| {
                self.external_order_id
                    .as_ref()
                    .filter(|value| !value.chars().all(|ch| ch.is_ascii_digit()))
                    .cloned()
            })
            .or_else(|| self.client_order_id.clone());
        OrderAck {
            exchange: self.exchange,
            exchange_symbol: instrument.symbol_for(self.exchange),
            instrument,
            order_id,
            client_order_id,
            status: order
                .and_then(|order| order.status.clone())
                .or_else(|| Some(self.order_status.clone())),
            raw: json!({
                "source": "pending_confirmation_reconciler",
                "external_order_id": self.external_order_id,
                "client_order_id": self.client_order_id,
                "order_status": self.order_status,
            }),
        }
    }
    /// 提供pending报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn pending_report(
        &self,
        error_message: impl Into<String>,
        mut raw_payload: Value,
    ) -> ExecutionTaskReportRequest {
        if let Some(payload) = raw_payload.as_object_mut() {
            payload.insert(
                "execution_status".to_string(),
                json!("pending_confirmation"),
            );
            payload.insert(
                "external_order_id".to_string(),
                json!(self.external_order_id),
            );
            payload.insert("client_order_id".to_string(), json!(self.client_order_id));
        }
        let mut report = ExecutionTaskReportRequest::success(
            self.task_id,
            self.exchange.as_str(),
            self.external_or_client_order_id(),
            self.order_side.clone(),
            self.order_status.clone(),
            raw_payload,
        );
        report.execution_status = "pending_confirmation".to_string();
        report.error_message = Some(error_message.into());
        report
    }
}
impl ExecutionOrderTask {
    pub fn from_task(task: &ExecutionTask) -> Result<Self> {
        Self::from_task_with_default(task, ExchangeId::Okx)
    }
    /// 从外部输入转换为内部模型，隔离 Web 商业、会员和执行准备度 的字段适配细节。
    pub fn from_task_with_default(
        task: &ExecutionTask,
        default_exchange: ExchangeId,
    ) -> Result<Self> {
        let payload = order_payload(&task.request_payload_json);
        let payload = &payload;
        let exchange = payload_string(payload, "exchange")
            .map(|value| parse_exchange(&value))
            .transpose()?
            .unwrap_or(default_exchange);
        let symbol = payload_string(payload, "symbol").unwrap_or_else(|| task.symbol.clone());
        let side = payload_string(payload, "side")
            .or_else(|| payload_string(payload, "signal_type"))
            .map(|value| parse_side(&value))
            .transpose()?
            .unwrap_or(OrderSide::Buy);
        let take_profit_legs = parse_take_profit_legs(payload, direction_from_order_side(side))?;
        let order_type = payload_string(payload, "order_type")
            .map(|value| parse_order_type(&value))
            .transpose()?
            .unwrap_or(OrderType::Market);
        let size_usdt = payload_f64(payload, "size_usdt");
        let execution_size_usdt = task
            .request_payload_json
            .get("execution")
            .and_then(|value| payload_f64(value, "size_usdt"));
        let derived_size = execution_size_usdt.and_then(|size_usdt| {
            let entry_price = protection_entry_price(payload)?;
            (size_usdt.is_finite()
                && entry_price.is_finite()
                && size_usdt > 0.0
                && entry_price > 0.0)
                .then(|| format_order_size(size_usdt / entry_price))
        });
        Ok(Self {
            task_id: task.id,
            exchange,
            symbol,
            side,
            order_type,
            size_usdt,
            size: payload_string(payload, "size")
                .or_else(|| payload_string(payload, "quantity"))
                .or_else(|| payload_string(payload, "qty"))
                .or(derived_size)
                .unwrap_or_else(|| "0".to_string()),
            price: payload_string(payload, "price"),
            margin_mode: payload_string(payload, "margin_mode").map(MarginMode::from),
            leverage: payload_string(payload, "leverage"),
            position_mode: payload_string(payload, "position_mode")
                .map(|value| parse_position_mode(&value))
                .transpose()?,
            margin_coin: payload_string(payload, "margin_coin")
                .or_else(|| Some("USDT".to_string())),
            position_side: payload_string(payload, "position_side"),
            trade_side: payload_string(payload, "trade_side"),
            client_order_id: payload_string(payload, "client_order_id")
                .or_else(|| Some(format!("rqtask{}", task.id))),
            reduce_only: payload_bool(payload, "reduce_only"),
            time_in_force: payload_string(payload, "time_in_force")
                .map(|value| parse_time_in_force(&value))
                .transpose()?,
            attached_stop_loss_price: selected_stop_loss_price(payload)
                .filter(|price| price.is_finite() && *price > 0.0)
                .map(format_order_price),
            take_profit_legs,
        })
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    pub fn to_order_request(&self) -> Result<OrderPlacementRequest> {
        Ok(OrderPlacementRequest {
            exchange: self.exchange,
            instrument: parse_instrument(&self.symbol)?,
            side: self.side,
            order_type: self.order_type,
            size: self.size.clone(),
            price: self.price.clone(),
            margin_mode: self.margin_mode.clone(),
            margin_coin: self.margin_coin.clone(),
            position_side: self.position_side.clone(),
            trade_side: self.trade_side.clone(),
            client_order_id: self.client_order_id.clone(),
            reduce_only: self.reduce_only,
            time_in_force: self.time_in_force,
            attached_stop_loss_price: self.attached_stop_loss_price.clone(),
        })
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    pub fn to_order_request_with_last_price(
        &self,
        last_price: Option<f64>,
    ) -> Result<OrderPlacementRequest> {
        let mut request = self.to_order_request()?;
        if !is_zero_order_size(&request.size) {
            return Ok(request);
        }
        let Some(size_usdt) = self.size_usdt else {
            return Ok(request);
        };
        let Some(last_price) = last_price else {
            return Ok(request);
        };
        if size_usdt.is_finite() && last_price.is_finite() && size_usdt > 0.0 && last_price > 0.0 {
            request.size = format_order_size(size_usdt / last_price);
        }
        Ok(request)
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    fn to_live_order_request(
        &self,
        last_price: Option<f64>,
        filters: Option<&ExchangeOrderFilters>,
    ) -> Result<OrderPlacementRequest> {
        self.to_live_order_request_with_local_min_size(
            last_price,
            filters,
            local_live_min_order_size_enabled(),
        )
    }
    /// 将内部模型转换为输出结构，避免 Web 商业、会员和执行准备度 的内部字段直接外泄。
    fn to_live_order_request_with_local_min_size(
        &self,
        last_price: Option<f64>,
        filters: Option<&ExchangeOrderFilters>,
        use_local_min_order_size: bool,
    ) -> Result<OrderPlacementRequest> {
        let mut request = self.to_order_request_with_last_price(last_price)?;
        let filters = filters.ok_or_else(|| {
            anyhow!(
                "missing exchange symbol filters for {} on {}; run exchange symbol sync before live order",
                self.symbol,
                self.exchange.as_str()
            )
        })?;
        let last_price = decimal_from_f64(last_price.ok_or_else(|| {
            anyhow!(
                "missing ticker last_price for {} on {} before live order size validation",
                self.symbol,
                self.exchange.as_str()
            )
        })?)?;
        let size = parse_positive_decimal(&request.size, "order size")?;
        let enforce_min_notional = !request.reduce_only.unwrap_or(false)
            && !matches!(
                request.trade_side.as_deref().map(|value| value.to_ascii_lowercase()),
                Some(value) if value == "close"
            );
        let normalized_size = if use_local_min_order_size && enforce_min_notional {
            minimum_order_size(last_price, filters, enforce_min_notional)?
        } else {
            quantize_order_size(size, last_price, filters, enforce_min_notional)?
        };
        request.size = format_order_size_decimal(normalized_size, filters);
        if let Some(stop_loss_price) = request.attached_stop_loss_price.as_deref() {
            let stop_loss_price = stop_loss_price
                .trim()
                .parse::<f64>()
                .map_err(|err| anyhow!("invalid attached stop-loss price: {}", err))?;
            let normalized_stop_loss = quantize_protective_stop_price(
                stop_loss_price,
                direction_from_order_side(self.side),
                filters,
            )?;
            request.attached_stop_loss_price = Some(format_protective_stop_price_decimal(
                normalized_stop_loss,
                filters,
            ));
        }
        Ok(request)
    }
    /// 提供dryrun报告的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn dry_run_report(&self) -> Result<ExecutionTaskReportRequest> {
        Ok(ExecutionTaskReportRequest::success(
            self.task_id,
            self.exchange.as_str(),
            format!("dry-run-rq-task-{}", self.task_id),
            order_side_lower(self.side),
            "dry_run",
            json!({
                "dry_run": true,
                "symbol": self.symbol,
            }),
        ))
    }
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn local_live_min_order_size_enabled() -> bool {
    std::env::var("APP_ENV")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("local"))
}
