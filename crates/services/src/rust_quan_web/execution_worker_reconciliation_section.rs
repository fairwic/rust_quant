/// 构建buildexchangereconciliationreport请求，集中维护Web 商业链路的载荷和字段组装规则。
fn build_exchange_reconciliation_report_request(
    task: &ExecutionTask,
    issue_type: ExchangeReconciliationIssueType,
    detected_at: Option<String>,
    message: impl Into<String>,
) -> ExchangeReconciliationReportRequest {
    let symbol = reconciliation_symbol(task);
    let exchange = reconciliation_exchange(task);
    let source_ref =
        build_exchange_reconciliation_source_ref(task, &exchange, &symbol, issue_type.as_str());
    let message = message.into().trim().to_string();
    let message = (!message.is_empty()).then_some(message);
    ExchangeReconciliationReportRequest {
        combo_id: task.combo_id,
        buyer_email: task.buyer_email.clone(),
        symbol,
        exchange: Some(exchange),
        api_credential_id: api_credential_id_from_task(task),
        issue_type,
        detected_at,
        source_ref: Some(source_ref),
        message,
    }
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub(crate) fn build_exchange_reconciliation_requests_from_read_only_snapshot(
    task: &ExecutionTask,
    positions: &[Position],
    open_orders: &[Order],
    detected_at: Option<String>,
) -> Vec<ExchangeReconciliationReportRequest> {
    if task_allows_parallel_hedge_entry(task) {
        return Vec::new();
    }
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
/// 明确的 hedge 开仓任务允许同币种多策略多腿，不用已有仓位或保护挂单阻断新腿。
fn task_allows_parallel_hedge_entry(task: &ExecutionTask) -> bool {
    if !task.task_type.eq_ignore_ascii_case("execute_signal") {
        return false;
    }
    let payload = &task.request_payload_json;
    let position_mode = json_string_path(&payload, &["execution", "position_mode"])
        .or_else(|| json_string_path(&payload, &["position_mode"]))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        position_mode.as_str(),
        "hedge" | "hedge_mode" | "long_short_mode"
    ) {
        return false;
    }
    let position_side = json_string_path(&payload, &["execution", "position_side"])
        .or_else(|| json_string_path(&payload, &["position_side"]))
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(position_side.as_str(), "long" | "short")
}
/// 从 JSON path 读取非空字符串，避免直接暴露 payload 里可能存在的原始交易所字段。
fn json_string_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub(crate) fn build_exchange_reconciliation_sync_requests_from_read_only_snapshot(
    task: &ExecutionTask,
    positions: &[Position],
    open_orders: &[Order],
    detected_at: Option<String>,
) -> Vec<ExchangeReconciliationReportRequest> {
    let mut requests = build_exchange_reconciliation_requests_from_read_only_snapshot(
        task,
        positions,
        open_orders,
        detected_at.clone(),
    );
    let position_count = positions
        .iter()
        .filter(|position| positive_decimal_text(&position.size))
        .count();
    if position_count == 0 {
        requests.push(build_exchange_reconciliation_report_request(
            task,
            ExchangeReconciliationIssueType::ExchangePositionFlat,
            detected_at,
            "read-only exchange snapshot confirmed zero position; local position snapshot sync allowed; place_order_allowed=false; mutation_allowed=false",
        ));
    }
    requests
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_live_order_blocked_by_exchange_reconciliation_report(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
    requests: &[ExchangeReconciliationReportRequest],
) -> ExecutionTaskReportRequest {
    let source_refs: Vec<String> = requests
        .iter()
        .filter_map(|request| request.source_ref.clone())
        .collect();
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
            "source_refs": source_refs,
            "place_order_allowed": false,
            "mutation_allowed": false,
        }),
    )
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_live_order_blocked_by_exchange_reconciliation_read_error_report(
    task: &ExecutionTask,
    order_task: &ExecutionOrderTask,
    error_message: impl Into<String>,
) -> ExecutionTaskReportRequest {
    let error_message = error_message.into();
    let source_ref = build_exchange_reconciliation_source_ref(
        task,
        order_task.exchange.as_str(),
        &order_task.symbol,
        "gateway_read_failed",
    );
    ExecutionTaskReportRequest::failed(
        task.id,
        order_task.exchange.as_str(),
        order_side_lower(order_task.side),
        format!(
            "live order blocked because read-only exchange reconciliation failed before live order: {error_message}; place_order_allowed=false; mutation_allowed=false"
        ),
        json!({
            "task_id": task.id,
            "stage": "exchange_reconciliation_read_only",
            "exchange": order_task.exchange.as_str(),
            "symbol": order_task.symbol,
            "source_ref": source_ref,
            "gateway_read_failed": true,
            "place_order_allowed": false,
            "mutation_allowed": false,
            "place_order_retried": false,
        }),
    )
}
/// 提供positive小数text的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn positive_decimal_text(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|parsed| parsed.is_finite() && parsed.abs() > 0.0)
}
/// 提供active开仓订单status的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn active_open_order_status(status: Option<&str>) -> bool {
    let normalized = status.unwrap_or_default().trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "canceled" | "cancelled" | "filled" | "closed" | "rejected" | "expired"
    )
}
/// 提供reconciliation交易对的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn reconciliation_symbol(task: &ExecutionTask) -> String {
    let payload = order_payload(&task.request_payload_json);
    payload_string(&payload, "symbol").unwrap_or_else(|| task.symbol.clone())
}
/// 提供reconciliation交易所的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn reconciliation_exchange(task: &ExecutionTask) -> String {
    let payload = order_payload(&task.request_payload_json);
    payload_string(&payload, "exchange")
        .map(|exchange| exchange.to_ascii_lowercase())
        .unwrap_or_else(|| "exchange_unknown".to_string())
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_exchange_reconciliation_source_ref(
    task: &ExecutionTask,
    exchange: &str,
    symbol: &str,
    issue_type: &str,
) -> String {
    let account_ref = reconciliation_account_ref(task);
    let credential_ref = reconciliation_credential_ref(task);
    let exchange = sanitize_source_ref_segment(exchange, "exchange_unknown");
    let symbol = sanitize_source_ref_segment(symbol, "symbol_unknown");
    let issue_type = sanitize_source_ref_segment(issue_type, "issue_unknown");
    format!(
        "rq:xrec:v2:ex={exchange}:acct={account_ref}:cred={credential_ref}:combo={combo_id}:task={task_id}:sym={symbol}:issue={issue_type}",
        combo_id = task.combo_id,
        task_id = task.id
    )
}
/// 提供reconciliationaccountref的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn reconciliation_account_ref(task: &ExecutionTask) -> String {
    let normalized = task.buyer_email.trim().to_ascii_lowercase();
    let digest = rust_quant_common::utils::function::sha256(&normalized);
    format!("email_sha256_{}", &digest[..16])
}
/// 提供reconciliationcredentialref的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn reconciliation_credential_ref(task: &ExecutionTask) -> String {
    let payload = order_payload(&task.request_payload_json);
    [
        "credential_ref",
        "api_credential_ref",
        "exchange_credential_ref",
        "credential_id",
        "api_credential_id",
        "exchange_credential_id",
    ]
    .iter()
    .find_map(|key| payload_string(&payload, key))
    .map(|value| sanitize_source_ref_segment(&value, "cred_unknown"))
    .unwrap_or_else(|| "cred_unknown".to_string())
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
fn sanitize_source_ref_segment(value: &str, fallback: &str) -> String {
    let sanitized: String = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}
/// 提供报告replayoperatorplaybooksummary的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 任务 ID。
    pub task_id: i64,
    /// 交易所名称。
    pub exchange: ExchangeId,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 交易方向。
    pub side: OrderSide,
    /// 类型标识。
    pub order_type: OrderType,
    /// 数量数值。
    pub size: String,
    /// 价格。
    pub price: Option<String>,
    /// 保证金模式；为空时使用交易所默认模式。
    pub margin_mode: Option<MarginMode>,
    /// 杠杆倍数。
    pub leverage: Option<String>,
    /// 仓位mode；为空时表示该条件不启用。
    pub position_mode: Option<PositionMode>,
    /// margincoin；为空时表示该条件不启用。
    pub margin_coin: Option<String>,
    /// position方向；为空时使用默认值或表示不限制。
    pub position_side: Option<String>,
    /// trade方向；为空时使用默认值或表示不限制。
    pub trade_side: Option<String>,
    /// clientorder ID；为空时使用默认值或表示不限制。
    pub client_order_id: Option<String>,
    /// reduceonly；为空时表示该条件不启用。
    pub reduce_only: Option<bool>,
    /// timeinforce；为空时表示该条件不启用。
    pub time_in_force: Option<TimeInForce>,
    /// size USDT 金额；为空时使用默认值或表示不限制。
    pub size_usdt: Option<f64>,
    /// 是否已应用 Web owner service 的最终风险预算预留。
    pub risk_reserved: bool,
    /// 止损价格。
    pub attached_stop_loss_price: Option<String>,
    /// 列表数据。
    pub take_profit_legs: Vec<TakeProfitLeg>,
}
