#[derive(Debug, Clone)]
struct PrePlaceClientOrderLookup {
    query: OrderQuery,
    ack: OrderAck,
}

fn pre_place_client_order_lookup(
    request: &OrderPlacementRequest,
) -> Option<PrePlaceClientOrderLookup> {
    let client_order_id = request
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let mut query =
        OrderQuery::by_client_order_id(request.instrument.clone(), client_order_id.clone());
    if let Some(margin_coin) = request
        .margin_coin
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query = query.with_margin_coin(margin_coin.to_string());
    }

    Some(PrePlaceClientOrderLookup {
        query,
        ack: OrderAck {
            exchange: request.exchange,
            instrument: request.instrument.clone(),
            exchange_symbol: request.instrument.symbol_for(request.exchange),
            order_id: None,
            client_order_id: Some(client_order_id.clone()),
            status: Some("client_order_id_pre_place_check".to_string()),
            raw: json!({
                "reconciliation": {
                    "reason": "client_order_id_pre_place_check",
                    "action": "query_existing_order_before_place_order",
                    "place_order_allowed": false,
                    "place_order_retried": false,
                },
                "client_order_id": client_order_id,
            }),
        },
    })
}

fn is_order_not_found_for_client_order_preflight(error_message: &str) -> bool {
    let normalized = error_message.to_ascii_lowercase();
    normalized.contains("-2013")
        || normalized.contains("order does not exist")
        || normalized.contains("order not found")
        || normalized.contains("not found")
        || normalized.contains("not exist")
}

fn client_order_id_owner_violation_report(
    task_id: i64,
    task_type: &str,
    order_side: &str,
    request: &OrderPlacementRequest,
) -> Option<ExecutionTaskReportRequest> {
    let client_order_id = request.client_order_id.as_deref()?.trim();
    let owner_task_id = generated_client_order_id_owner_task_id(task_type, client_order_id)?;
    if owner_task_id == task_id {
        return None;
    }

    Some(ExecutionTaskReportRequest::failed(
        task_id,
        request.exchange.as_str(),
        order_side,
        format!(
            "client_order_id {client_order_id} belongs to task {owner_task_id}, not task {task_id}"
        ),
        json!({
            "task_id": task_id,
            "stage": "client_order_id_owner_check",
            "exchange": request.exchange.as_str(),
            "symbol": request.instrument.symbol_for(request.exchange),
            "client_order_id": client_order_id,
            "client_order_id_owner_task_id": owner_task_id,
            "expected_task_id": task_id,
            "place_order_allowed": false,
            "mutation_allowed": false,
            "protection_sync_allowed": false,
            "reconciliation": {
                "reason": "client_order_id_owner_mismatch",
                "action": "blocked_foreign_client_order_id",
                "place_order_retried": false,
            },
        }),
    ))
}

fn generated_client_order_id_owner_task_id(task_type: &str, client_order_id: &str) -> Option<i64> {
    let prefix = match task_type {
        "execute_signal" => "rqtask",
        "risk_control_close_candidate" => "rqclose",
        _ => return None,
    };
    parse_generated_client_order_id_owner(client_order_id, prefix)
}

fn parse_generated_client_order_id_owner(client_order_id: &str, prefix: &str) -> Option<i64> {
    let suffix = client_order_id.trim().strip_prefix(prefix)?;
    if suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn duplicate_client_order_id_reconciliation_ack(
    request: &OrderPlacementRequest,
) -> Option<OrderAck> {
    let client_order_id = request
        .client_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    Some(OrderAck {
        exchange: request.exchange,
        instrument: request.instrument.clone(),
        exchange_symbol: request.instrument.symbol_for(request.exchange),
        order_id: None,
        client_order_id: Some(client_order_id.clone()),
        status: Some("duplicate_client_order_id".to_string()),
        raw: json!({
            "reconciliation": {
                "reason": "duplicate_client_order_id",
                "action": "query_existing_order_by_client_order_id",
                "place_order_retried": false,
            },
            "client_order_id": client_order_id,
        }),
    })
}

async fn confirm_live_order(
    gateway: &CryptoExcAllGateway,
    ack: &OrderAck,
) -> Result<(Order, Vec<Fill>)> {
    let query = if let Some(order_id) = ack.order_id.as_deref() {
        OrderQuery::by_order_id(ack.instrument.clone(), order_id)
    } else if let Some(client_order_id) = ack.client_order_id.as_deref() {
        OrderQuery::by_client_order_id(ack.instrument.clone(), client_order_id)
    } else {
        return Err(anyhow!(
            "place_order ack missing order_id and client_order_id for confirmation"
        ));
    };

    let mut confirmed_order = None;
    for attempt in 0..3 {
        let order = gateway.order(ack.exchange, query.clone()).await?;
        let status = order.status.as_deref().unwrap_or_default();
        if status != "NEW" || attempt == 2 {
            confirmed_order = Some(order);
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }
    let order = confirmed_order.expect("confirmation loop must set order");
    let order_id = order.order_id.as_deref().or(ack.order_id.as_deref());
    let fills = if let Some(order_id) = order_id {
        match gateway
            .fills(
                ack.exchange,
                FillListQuery::for_instrument(ack.instrument.clone())
                    .with_order_id(order_id)
                    .with_limit(100),
            )
            .await
        {
            Ok(fills) => fills,
            Err(error) => {
                warn!(
                    exchange = ack.exchange.as_str(),
                    order_id, "live order fills confirmation failed: {}", error
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Ok((order, fills))
}

fn build_confirmed_order_report(
    task_id: i64,
    order_side: &str,
    ack: &OrderAck,
    order: Option<Order>,
    fills: Vec<Fill>,
    confirmation_error: Option<String>,
    protection: Option<ProtectionSyncContract>,
) -> ExecutionTaskReportRequest {
    let external_order_id = order
        .as_ref()
        .and_then(|order| order.order_id.as_deref())
        .or(ack.order_id.as_deref())
        .or_else(|| {
            order
                .as_ref()
                .and_then(|order| order.client_order_id.as_deref())
        })
        .or(ack.client_order_id.as_deref())
        .unwrap_or("unknown");
    let order_status = order
        .as_ref()
        .and_then(|order| order.status.as_deref())
        .or(ack.status.as_deref())
        .unwrap_or("submitted");
    let execution_status = live_order_execution_status(order_status);

    let filled_qty = order
        .as_ref()
        .and_then(|order| parse_optional_f64(order.filled_size.as_deref()))
        .or_else(|| sum_fill_sizes(&fills));
    let filled_quote = sum_fill_quote(&fills).or_else(|| {
        let qty = filled_qty?;
        let avg_price = order
            .as_ref()
            .and_then(|order| parse_optional_f64(order.average_price.as_deref()))?;
        Some(qty * avg_price)
    });
    let fee_amount = sum_fill_fees(&fills);

    let raw_payload = json!({
        "ack": ack.raw,
        "order_detail": order.as_ref().map(|order| order.raw.clone()),
        "fills": fills.iter().map(|fill| fill.raw.clone()).collect::<Vec<_>>(),
        "confirmation_error": confirmation_error,
        "execution_status": execution_status,
    });

    let mut report = ExecutionTaskReportRequest::success(
        task_id,
        ack.exchange.as_str(),
        external_order_id,
        order_side,
        order_status,
        raw_payload,
    );
    report.execution_status = execution_status.to_string();
    if execution_status == "failed" {
        report.error_message = Some(format!("live order terminal status: {order_status}"));
    }
    report.filled_qty = filled_qty;
    report.filled_quote = filled_quote;
    report.fee_amount = fee_amount;
    if let Some(protection) = protection {
        protection.apply_to_report(&mut report);
    }
    report
}

fn build_confirmed_order_report_for_task(
    task: &ExecutionTask,
    order_side: &str,
    ack: &OrderAck,
    order: Option<Order>,
    fills: Vec<Fill>,
    confirmation_error: Option<String>,
    protection: Option<ProtectionSyncContract>,
) -> ExecutionTaskReportRequest {
    let mut report = build_confirmed_order_report(
        task.id,
        order_side,
        ack,
        order,
        fills,
        confirmation_error,
        protection,
    );
    attach_execution_task_context_to_report(&mut report, task);
    report
}

fn attach_execution_task_context_to_report(
    report: &mut ExecutionTaskReportRequest,
    task: &ExecutionTask,
) {
    let mut raw_payload = report
        .raw_payload_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));
    let source_signal_type = task
        .request_payload_json
        .get("source_signal_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if task.news_signal_id.is_some() {
                "news_event".to_string()
            } else {
                "technical_strategy".to_string()
            }
        });

    raw_payload["execution_task"] = json!({
        "task_id": task.id,
        "news_signal_id": task.news_signal_id,
        "strategy_signal_id": task.strategy_signal_id,
        "combo_id": task.combo_id,
        "strategy_slug": task.strategy_slug,
        "symbol": task.symbol,
        "task_type": task.task_type,
        "source_signal_type": source_signal_type,
    });
    report.raw_payload_json = Some(raw_payload.to_string());
}

fn live_order_execution_status(order_status: &str) -> &'static str {
    match order_status.trim().to_ascii_uppercase().as_str() {
        "FILLED" => "completed",
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => "failed",
        _ => "pending_confirmation",
    }
}

fn protection_outcome_requires_rollback(outcome: &ProtectionSyncOutcome) -> bool {
    matches!(
        outcome,
        ProtectionSyncOutcome::Failed { .. } | ProtectionSyncOutcome::Uncertain { .. }
    )
}

fn parse_optional_f64(value: Option<&str>) -> Option<f64> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())
}

fn sum_fill_sizes(fills: &[Fill]) -> Option<f64> {
    let mut total = 0.0;
    let mut seen = false;
    for fill in fills {
        if let Some(size) = parse_optional_f64(fill.size.as_deref()) {
            total += size;
            seen = true;
        }
    }
    seen.then_some(total)
}

fn sum_fill_quote(fills: &[Fill]) -> Option<f64> {
    let mut total = 0.0;
    let mut seen = false;
    for fill in fills {
        let price = parse_optional_f64(fill.price.as_deref());
        let size = parse_optional_f64(fill.size.as_deref());
        if let (Some(price), Some(size)) = (price, size) {
            total += price * size;
            seen = true;
        }
    }
    seen.then_some(total)
}

fn sum_fill_fees(fills: &[Fill]) -> Option<f64> {
    let mut total = 0.0;
    let mut seen = false;
    for fill in fills {
        if let Some(fee) = parse_optional_f64(fill.fee.as_deref()) {
            total += fee;
            seen = true;
        }
    }
    seen.then_some(total)
}
