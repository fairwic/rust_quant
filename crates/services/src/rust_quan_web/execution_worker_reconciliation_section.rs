fn build_exchange_reconciliation_report_request(
    task: &ExecutionTask,
    issue_type: ExchangeReconciliationIssueType,
    detected_at: Option<String>,
    message: impl Into<String>,
) -> ExchangeReconciliationReportRequest {
    let symbol = reconciliation_symbol(task);
    let source_ref = format!(
        "rust_quant/exchange_reconciliation/{}/combo/{}/task/{}/symbol/{}",
        issue_type.as_str(),
        task.combo_id,
        task.id,
        symbol
    );
    let message = message.into().trim().to_string();
    let message = (!message.is_empty()).then_some(message);

    ExchangeReconciliationReportRequest {
        combo_id: task.combo_id,
        buyer_email: task.buyer_email.clone(),
        symbol,
        issue_type,
        detected_at,
        source_ref: Some(source_ref),
        message,
    }
}

fn build_exchange_reconciliation_requests_from_read_only_snapshot(
    task: &ExecutionTask,
    positions: &[Position],
    open_orders: &[Order],
    detected_at: Option<String>,
) -> Vec<ExchangeReconciliationReportRequest> {
    let mut requests = Vec::new();
    let position_count = positions
        .iter()
        .filter(|position| positive_decimal_text(&position.size))
        .count();
    if position_count > 0 {
        requests.push(build_exchange_reconciliation_report_request(
            task,
            ExchangeReconciliationIssueType::ExchangePositionStale,
            detected_at.clone(),
            format!(
                "read-only exchange snapshot detected {position_count} non-zero position(s); place_order_allowed=false; mutation_allowed=false"
            ),
        ));
    }

    let open_order_count = open_orders
        .iter()
        .filter(|order| active_open_order_status(order.status.as_deref()))
        .count();
    if open_order_count > 0 {
        requests.push(build_exchange_reconciliation_report_request(
            task,
            ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
            detected_at,
            format!(
                "read-only exchange snapshot detected {open_order_count} open order(s); place_order_allowed=false; mutation_allowed=false"
            ),
        ));
    }

    requests
}

fn build_live_order_blocked_by_exchange_reconciliation_report(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
    requests: &[ExchangeReconciliationReportRequest],
) -> ExecutionTaskReportRequest {
    let issues: Vec<Value> = requests
        .iter()
        .map(|request| {
            json!({
                "issue_type": request.issue_type.as_str(),
                "source_ref": request.source_ref,
                "message": request.message,
            })
        })
        .collect();
    let issue_codes: Vec<&str> = requests
        .iter()
        .map(|request| request.issue_type.as_str())
        .collect();
    let message = format!(
        "live order blocked by read-only exchange reconciliation: {}; place_order_allowed=false; mutation_allowed=false",
        issue_codes.join(", ")
    );

    ExecutionTaskReportRequest::failed(
        task.id,
        order_task.exchange.as_str(),
        order_side_lower(order_task.side),
        message,
        json!({
            "task_id": task.id,
            "stage": "exchange_reconciliation_read_only",
            "exchange": order_task.exchange.as_str(),
            "symbol": order_task.symbol,
            "issues": issues,
            "place_order_allowed": false,
            "mutation_allowed": false,
        }),
    )
}

fn positive_decimal_text(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|parsed| parsed.is_finite() && parsed.abs() > 0.0)
}

fn active_open_order_status(status: Option<&str>) -> bool {
    let normalized = status.unwrap_or_default().trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "canceled" | "cancelled" | "filled" | "closed" | "rejected" | "expired"
    )
}

fn reconciliation_symbol(task: &ExecutionTask) -> String {
    let payload = order_payload(&task.request_payload_json);
    payload_string(&payload, "symbol").unwrap_or_else(|| task.symbol.clone())
}

fn report_replay_operator_playbook_summary(
    failed_count: usize,
    failure_backoff_seconds: u64,
) -> Value {
    if failed_count == 0 {
        return json!({
            "item_count": 0,
            "blocking_item_count": 0,
            "manual_review_item_count": 0,
            "observe_only_item_count": 0,
            "items": [],
        });
    }

    json!({
        "item_count": 1,
        "blocking_item_count": 0,
        "manual_review_item_count": 1,
        "observe_only_item_count": 0,
        "items": [
            {
                "source": "execution_worker_checkpoint",
                "severity": "P1",
                "code": "QUANT_REPORT_REPLAY_FAILED",
                "section": "quant_worker_checkpoint_audit",
                "message": "report_result replay batch has failed attempts",
                "operator_action": "manual_review_before_release",
                "owner": "quant_ops",
                "default_next_action": "review_report_replay_batch",
                "admin_link_target": "admin.full_product_health.quant_worker_checkpoint_audit",
                "metadata": {
                    "failed_count": failed_count,
                    "failure_backoff_seconds": failure_backoff_seconds,
                    "place_order_allowed": false,
                },
            }
        ],
    })
}

fn elapsed_ms(started_at: Instant) -> Option<i32> {
    Some(started_at.elapsed().as_millis().min(i32::MAX as u128) as i32)
}

#[derive(Debug, Clone)]
pub struct ExecutionOrderTask {
    pub task_id: i64,
    pub exchange: ExchangeId,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub size: String,
    pub price: Option<String>,
    pub margin_mode: Option<MarginMode>,
    pub leverage: Option<String>,
    pub position_mode: Option<PositionMode>,
    pub margin_coin: Option<String>,
    pub position_side: Option<String>,
    pub trade_side: Option<String>,
    pub client_order_id: Option<String>,
    pub reduce_only: Option<bool>,
    pub time_in_force: Option<TimeInForce>,
    pub size_usdt: Option<f64>,
    pub attached_stop_loss_price: Option<String>,
}
