use anyhow::{anyhow, bail, Result};
use crypto_exc_all::{ExchangeId, Order, OrderListQuery, Position};
use serde_json::{json, Value};

use super::execution_audit::redact_error_message;
use super::execution_payload::{parse_exchange, parse_instrument};
use super::execution_worker::{
    build_exchange_reconciliation_requests_from_read_only_snapshot, is_protected_link_symbol,
};
use crate::exchange::CryptoExcAllGateway;
use crate::rust_quan_web::{
    ExchangeReconciliationReportRequest, ExecutionTask, ExecutionTaskClient, ExecutionTaskConfig,
};

const RECONCILIATION_SNAPSHOT_CONFIRM_ENV: &str = "RECONCILIATION_SNAPSHOT_CONFIRM";
const RECONCILIATION_SNAPSHOT_CONFIRM_TOKEN: &str = "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationSnapshotCheckConfig {
    pub buyer_email: String,
    pub exchange: ExchangeId,
    pub symbol: String,
    pub combo_id: i64,
    pub task_id: i64,
    pub credential_ref: Option<String>,
}

impl ReconciliationSnapshotCheckConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let confirmation = lookup(RECONCILIATION_SNAPSHOT_CONFIRM_ENV);
        if confirmation.as_deref().map(str::trim) != Some(RECONCILIATION_SNAPSHOT_CONFIRM_TOKEN) {
            bail!(
                "{RECONCILIATION_SNAPSHOT_CONFIRM_ENV}={RECONCILIATION_SNAPSHOT_CONFIRM_TOKEN} is required before running signed read-only reconciliation"
            );
        }

        let buyer_email = required_trimmed(&lookup, "RECONCILIATION_SNAPSHOT_BUYER_EMAIL")?;
        let exchange = lookup("RECONCILIATION_SNAPSHOT_EXCHANGE")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "binance".to_string());
        let exchange = parse_exchange(&exchange)?;
        let symbol = required_trimmed(&lookup, "RECONCILIATION_SNAPSHOT_SYMBOL")?;
        if is_protected_link_symbol(&symbol) {
            bail!("LINKUSDT is excluded from reconciliation snapshot live validation");
        }

        let combo_id = required_i64(&lookup, "RECONCILIATION_SNAPSHOT_COMBO_ID")?;
        let task_id = required_i64(&lookup, "RECONCILIATION_SNAPSHOT_TASK_ID")?;
        let credential_ref = lookup("RECONCILIATION_SNAPSHOT_CREDENTIAL_REF")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Ok(Self {
            buyer_email,
            exchange,
            symbol,
            combo_id,
            task_id,
            credential_ref,
        })
    }
}

pub fn build_reconciliation_snapshot_task(
    config: &ReconciliationSnapshotCheckConfig,
) -> ExecutionTask {
    let now = chrono::Utc::now().to_rfc3339();
    let mut request_payload = json!({
        "exchange": config.exchange.as_str(),
        "symbol": config.symbol,
        "source": "signed_read_only_reconciliation_snapshot",
        "place_order_allowed": false,
        "mutation_allowed": false,
    });
    if let Some(credential_ref) = config.credential_ref.as_deref() {
        request_payload["credential_ref"] = json!(credential_ref);
    }

    ExecutionTask {
        id: config.task_id,
        news_signal_id: None,
        strategy_signal_id: None,
        combo_id: config.combo_id,
        buyer_email: config.buyer_email.clone(),
        strategy_slug: "signed_read_only_reconciliation_snapshot".to_string(),
        symbol: config.symbol.clone(),
        task_type: "execute_signal".to_string(),
        task_status: "reconciliation_snapshot".to_string(),
        priority: 0,
        lease_owner: None,
        lease_until: None,
        scheduled_at: now.clone(),
        request_payload_json: request_payload,
        created_at: now.clone(),
        updated_at: now,
    }
}

pub fn build_reconciliation_snapshot_requests(
    config: &ReconciliationSnapshotCheckConfig,
    positions: &[Position],
    open_orders: &[Order],
) -> Vec<ExchangeReconciliationReportRequest> {
    let task = build_reconciliation_snapshot_task(config);
    build_exchange_reconciliation_requests_from_read_only_snapshot(
        &task,
        positions,
        open_orders,
        None,
    )
}

pub async fn run_reconciliation_snapshot_check_from_env() -> Result<Value> {
    let config = ReconciliationSnapshotCheckConfig::from_env()?;
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_default();
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })?;

    let instrument = parse_instrument(&config.symbol)?;
    let user_config = client
        .resolve_user_exchange_config(&config.buyer_email, config.exchange.as_str())
        .await?;
    let gateway = CryptoExcAllGateway::from_single_exchange_credentials(
        config.exchange,
        user_config.api_key,
        user_config.api_secret,
        user_config.passphrase,
        user_config.simulated,
    )?;

    let positions = gateway
        .positions(config.exchange, Some(&instrument))
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only position snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let open_orders = gateway
        .open_orders(
            config.exchange,
            OrderListQuery::for_instrument(instrument).with_limit(100),
        )
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only open-orders snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;

    let requests = build_reconciliation_snapshot_requests(&config, &positions, &open_orders);
    let mut report_responses = Vec::new();
    for request in &requests {
        report_responses.push(report_reconciliation(&client, request).await?);
    }

    Ok(json!({
        "exchange": config.exchange.as_str(),
        "symbol": config.symbol,
        "combo_id": config.combo_id,
        "task_id": config.task_id,
        "position_count": positions.len(),
        "non_zero_position_count": non_zero_position_count(&positions),
        "open_order_count": open_orders.len(),
        "active_open_order_count": active_open_order_count(&open_orders),
        "issue_count": requests.len(),
        "reported_issue_count": report_responses.len(),
        "source_refs": requests
            .iter()
            .filter_map(|request| request.source_ref.clone())
            .collect::<Vec<_>>(),
        "place_order_allowed": false,
        "mutation_allowed": false,
        "report_result_allowed": false,
    }))
}

async fn report_reconciliation(
    client: &ExecutionTaskClient,
    request: &ExchangeReconciliationReportRequest,
) -> Result<Value> {
    let response = client
        .report_exchange_reconciliation(request.clone())
        .await
        .map_err(|error| {
            anyhow!(
                "report exchange reconciliation failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    Ok(json!({
        "combo_id": response.combo_id,
        "symbol": response.symbol,
        "signal_id": response.signal_id,
        "issue_type": response.issue_type,
        "api_execution_status": response.api_execution_status,
    }))
}

fn required_trimmed<F>(lookup: &F, key: &str) -> Result<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{key} is required"))
}

fn required_i64<F>(lookup: &F, key: &str) -> Result<i64>
where
    F: Fn(&str) -> Option<String>,
{
    let value = required_trimmed(lookup, key)?;
    let parsed = value
        .parse::<i64>()
        .map_err(|_| anyhow!("{key} must be a positive integer"))?;
    if parsed <= 0 {
        bail!("{key} must be a positive integer");
    }
    Ok(parsed)
}

fn non_zero_position_count(positions: &[Position]) -> usize {
    positions
        .iter()
        .filter(|position| positive_decimal_text(&position.size))
        .count()
}

fn active_open_order_count(open_orders: &[Order]) -> usize {
    open_orders
        .iter()
        .filter(|order| active_open_order_status(order.status.as_deref()))
        .count()
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
