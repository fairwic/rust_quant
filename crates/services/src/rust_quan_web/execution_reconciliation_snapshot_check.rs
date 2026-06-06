use anyhow::{anyhow, bail, Result};
use crypto_exc_all::{ExchangeId, Fill, FillListQuery, Order, OrderListQuery, Position};
use serde_json::{json, Value};

use super::execution_audit::redact_error_message;
use super::execution_payload::{parse_exchange, parse_instrument};
use super::execution_task_client::{
    ExchangeCloseFillWritebackRequest, ExchangeCloseFillWritebackResponse,
};
use super::execution_worker::{
    build_exchange_reconciliation_sync_requests_from_read_only_snapshot, is_protected_link_symbol,
};
use crate::exchange::CryptoExcAllGateway;
use crate::rust_quan_web::{
    ExchangeReconciliationReportRequest, ExecutionTask, ExecutionTaskClient, ExecutionTaskConfig,
};

const RECONCILIATION_SNAPSHOT_CONFIRM_ENV: &str = "RECONCILIATION_SNAPSHOT_CONFIRM";
const RECONCILIATION_SNAPSHOT_CONFIRM_TOKEN: &str = "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION";
const CLOSE_FILL_WRITEBACK_APPLY_ENV: &str = "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY";
const CLOSE_FILL_WRITEBACK_CONFIRM_ENV: &str =
    "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM";
const CLOSE_FILL_WRITEBACK_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_THIS_WRITES_EXCHANGE_CLOSE_FILL_TO_WEB";
const CLOSE_FILL_WRITEBACK_INTENT_ENV: &str = "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationSnapshotCheckConfig {
    pub buyer_email: String,
    pub exchange: ExchangeId,
    pub symbol: String,
    pub combo_id: i64,
    pub task_id: i64,
    pub credential_ref: Option<String>,
    pub report_reconciliation: bool,
    pub include_fills: bool,
    pub close_fill_writeback_apply: bool,
    pub close_fill_writeback_intent: Option<String>,
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
        let report_reconciliation = lookup("RECONCILIATION_SNAPSHOT_REPORT")
            .map(|value| parse_bool_default_true(&value))
            .transpose()?
            .unwrap_or(true);
        let include_fills = lookup("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS")
            .map(|value| parse_bool_default_false(&value))
            .transpose()?
            .unwrap_or(false);
        let close_fill_writeback_apply = lookup(CLOSE_FILL_WRITEBACK_APPLY_ENV)
            .map(|value| parse_bool_default_false(&value))
            .transpose()?
            .unwrap_or(false);
        let close_fill_writeback_intent = lookup(CLOSE_FILL_WRITEBACK_INTENT_ENV)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if close_fill_writeback_apply {
            if !include_fills {
                bail!(
                    "RECONCILIATION_SNAPSHOT_INCLUDE_FILLS=true is required before close-fill writeback apply"
                );
            }
            let confirmation = lookup(CLOSE_FILL_WRITEBACK_CONFIRM_ENV);
            if confirmation.as_deref().map(str::trim) != Some(CLOSE_FILL_WRITEBACK_CONFIRM_TOKEN) {
                bail!(
                    "{CLOSE_FILL_WRITEBACK_CONFIRM_ENV}={CLOSE_FILL_WRITEBACK_CONFIRM_TOKEN} is required before close-fill writeback apply"
                );
            }
            let expected_intent = expected_close_fill_writeback_intent(combo_id, task_id, &symbol);
            if close_fill_writeback_intent.as_deref() != Some(expected_intent.as_str()) {
                bail!(
                    "{CLOSE_FILL_WRITEBACK_INTENT_ENV} must be {expected_intent} before close-fill writeback apply"
                );
            }
        }

        Ok(Self {
            buyer_email,
            exchange,
            symbol,
            combo_id,
            task_id,
            credential_ref,
            report_reconciliation,
            include_fills,
            close_fill_writeback_apply,
            close_fill_writeback_intent,
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
    build_exchange_reconciliation_sync_requests_from_read_only_snapshot(
        &task,
        positions,
        open_orders,
        None,
    )
}

pub fn build_close_fill_writeback_candidates(
    config: &ReconciliationSnapshotCheckConfig,
    positions: &[Position],
    open_orders: &[Order],
    fills: &[Fill],
) -> Vec<Value> {
    if non_zero_position_count(positions) > 0 || active_open_order_count(open_orders) > 0 {
        return Vec::new();
    }

    let source_ref = build_reconciliation_snapshot_requests(config, positions, open_orders)
        .into_iter()
        .find_map(|request| request.source_ref);
    let mut matching_fills: Vec<&Fill> = fills
        .iter()
        .filter(|fill| fill.exchange == config.exchange)
        .filter(|fill| same_exchange_symbol(&fill.exchange_symbol, &config.symbol))
        .filter(|fill| positive_decimal_option(fill.size.as_deref()))
        .collect();
    matching_fills.sort_by_key(|fill| fill.timestamp.unwrap_or(0));

    let mut latest_open_buy: Option<&Fill> = None;
    let mut candidates = Vec::new();
    for fill in matching_fills {
        match normalized_fill_side(fill).as_deref() {
            Some("buy") => latest_open_buy = Some(fill),
            Some("sell") => {
                let Some(open_fill) = latest_open_buy else {
                    continue;
                };
                candidates.push(json!({
                    "candidate_type": "stop_loss_close_fill_observed",
                    "writeback_mode": "dry_run_plan_only",
                    "exchange": config.exchange.as_str(),
                    "symbol": config.symbol.clone(),
                    "combo_id": config.combo_id,
                    "task_id": config.task_id,
                    "open_order_id": open_fill.order_id.clone(),
                    "open_trade_id": open_fill.trade_id.clone(),
                    "open_side": normalized_fill_side(open_fill),
                    "open_price": open_fill.price.clone(),
                    "open_size": open_fill.size.clone(),
                    "open_fee": open_fill.fee.clone(),
                    "open_fee_asset": open_fill.fee_asset.clone(),
                    "open_role": open_fill.role.clone(),
                    "open_timestamp": open_fill.timestamp,
                    "close_order_id": fill.order_id.clone(),
                    "close_trade_id": fill.trade_id.clone(),
                    "close_side": normalized_fill_side(fill),
                    "close_price": fill.price.clone(),
                    "close_size": fill.size.clone(),
                    "close_fee": fill.fee.clone(),
                    "close_fee_asset": fill.fee_asset.clone(),
                    "close_role": fill.role.clone(),
                    "close_timestamp": fill.timestamp,
                    "quantity_match": decimal_texts_equal(open_fill.size.as_deref(), fill.size.as_deref()),
                    "position_flat_confirmed": true,
                    "active_open_order_count": 0,
                    "web_writeback_allowed": false,
                    "exchange_mutation_allowed": false,
                    "report_result_allowed": false,
                    "source_ref": source_ref.clone(),
                }));
            }
            _ => {}
        }
    }

    candidates
}

pub fn build_close_fill_writeback_request_from_candidate(
    config: &ReconciliationSnapshotCheckConfig,
    candidate: &Value,
) -> Result<ExchangeCloseFillWritebackRequest> {
    require_candidate_string(candidate, "candidate_type", "stop_loss_close_fill_observed")?;
    require_candidate_string(candidate, "writeback_mode", "dry_run_plan_only")?;
    require_candidate_i64(candidate, "task_id", config.task_id)?;
    require_candidate_i64(candidate, "combo_id", config.combo_id)?;
    require_candidate_string(candidate, "exchange", config.exchange.as_str())?;
    require_candidate_string(candidate, "symbol", &config.symbol)?;
    require_candidate_bool(candidate, "position_flat_confirmed", true)?;
    require_candidate_i64(candidate, "active_open_order_count", 0)?;
    require_candidate_bool(candidate, "quantity_match", true)?;
    require_candidate_bool(candidate, "web_writeback_allowed", false)?;
    require_candidate_bool(candidate, "exchange_mutation_allowed", false)?;
    require_candidate_bool(candidate, "report_result_allowed", false)?;

    Ok(ExchangeCloseFillWritebackRequest {
        task_id: config.task_id,
        combo_id: config.combo_id,
        exchange: required_candidate_string(candidate, "exchange")?.to_ascii_lowercase(),
        symbol: required_candidate_string(candidate, "symbol")?,
        source_ref: required_candidate_string(candidate, "source_ref")?,
        open_order_id: optional_candidate_string(candidate, "open_order_id"),
        open_trade_id: optional_candidate_string(candidate, "open_trade_id"),
        close_order_id: required_candidate_string(candidate, "close_order_id")?,
        close_trade_id: optional_candidate_string(candidate, "close_trade_id"),
        close_side: required_candidate_string(candidate, "close_side")?.to_ascii_lowercase(),
        close_size: required_candidate_f64(candidate, "close_size")?,
        close_price: optional_candidate_f64(candidate, "close_price")?,
        close_fee: optional_candidate_f64(candidate, "close_fee")?,
        close_timestamp_ms: optional_candidate_i64(candidate, "close_timestamp"),
        position_flat_confirmed: true,
        active_open_order_count: 0,
        quantity_match: true,
        writeback_authorized: true,
    })
}

pub async fn run_reconciliation_snapshot_check_from_env() -> Result<Value> {
    let config = ReconciliationSnapshotCheckConfig::from_env()?;
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_default();
    let has_internal_secret = !internal_secret.trim().is_empty();
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
            OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
        )
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only open-orders snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let fills = if config.include_fills {
        gateway
            .fills(
                config.exchange,
                FillListQuery::for_instrument(instrument).with_limit(20),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "signed read-only fills snapshot failed: {}",
                    redact_error_message(error.to_string())
                )
            })?
    } else {
        Vec::new()
    };

    let requests = build_reconciliation_snapshot_requests(&config, &positions, &open_orders);
    let close_fill_writeback_candidates =
        build_close_fill_writeback_candidates(&config, &positions, &open_orders, &fills);
    let mut close_fill_writeback_responses = Vec::new();
    if config.close_fill_writeback_apply {
        if !has_internal_secret {
            bail!(
                "EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required before close-fill writeback apply"
            );
        }
        if close_fill_writeback_candidates.len() != 1 {
            bail!(
                "close-fill writeback apply requires exactly one candidate, found {}",
                close_fill_writeback_candidates.len()
            );
        }
        let request = build_close_fill_writeback_request_from_candidate(
            &config,
            &close_fill_writeback_candidates[0],
        )?;
        let response = apply_close_fill_writeback(&client, request).await?;
        close_fill_writeback_responses.push(json!({
            "order_result_id": response.order_result.id,
            "external_order_id": response.order_result.external_order_id,
            "order_side": response.order_result.order_side,
            "order_status": response.order_result.order_status,
            "trade_record": response.trade_record,
            "position_snapshot_cleared": response.position_snapshot_cleared,
            "exchange_mutation_allowed": false,
        }));
    }
    let mut report_responses = Vec::new();
    if config.report_reconciliation {
        for request in &requests {
            report_responses.push(report_reconciliation(&client, request).await?);
        }
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
        "reconciliation_report_enabled": config.report_reconciliation,
        "reported_issue_count": report_responses.len(),
        "position_summaries": position_summaries(&positions),
        "open_order_summaries": open_order_summaries(&open_orders),
        "fill_snapshot_enabled": config.include_fills,
        "fill_count": fills.len(),
        "fill_summaries": fill_summaries(&fills),
        "close_fill_writeback_candidates": close_fill_writeback_candidates,
        "close_fill_writeback_apply_enabled": config.close_fill_writeback_apply,
        "close_fill_writeback_apply_count": close_fill_writeback_responses.len(),
        "close_fill_writeback_responses": close_fill_writeback_responses,
        "source_refs": requests
            .iter()
            .filter_map(|request| request.source_ref.clone())
            .collect::<Vec<_>>(),
        "place_order_allowed": false,
        "mutation_allowed": false,
        "report_result_allowed": false,
    }))
}

async fn apply_close_fill_writeback(
    client: &ExecutionTaskClient,
    request: ExchangeCloseFillWritebackRequest,
) -> Result<ExchangeCloseFillWritebackResponse> {
    client
        .apply_exchange_close_fill_writeback(request)
        .await
        .map_err(|error| {
            anyhow!(
                "apply exchange close-fill writeback failed: {}",
                redact_error_message(error.to_string())
            )
        })
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

fn parse_bool_default_true(value: &str) -> Result<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => bail!("RECONCILIATION_SNAPSHOT_REPORT must be a boolean"),
    }
}

fn parse_bool_default_false(value: &str) -> Result<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "" | "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => bail!("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS must be a boolean"),
    }
}

fn expected_close_fill_writeback_intent(combo_id: i64, task_id: i64, symbol: &str) -> String {
    format!("web-close-fill:combo={combo_id}:task={task_id}:symbol={symbol}")
}

fn require_candidate_string(candidate: &Value, key: &str, expected: &str) -> Result<()> {
    let actual = required_candidate_string(candidate, key)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("{key} must be {expected}");
    }
    Ok(())
}

fn require_candidate_i64(candidate: &Value, key: &str, expected: i64) -> Result<()> {
    let actual = required_candidate_i64(candidate, key)?;
    if actual != expected {
        bail!("{key} must be {expected}");
    }
    Ok(())
}

fn require_candidate_bool(candidate: &Value, key: &str, expected: bool) -> Result<()> {
    let actual = candidate
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("{key} must be a boolean"))?;
    if actual != expected {
        bail!("{key} must be {expected}");
    }
    Ok(())
}

fn required_candidate_string(candidate: &Value, key: &str) -> Result<String> {
    candidate
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{key} is required"))
}

fn optional_candidate_string(candidate: &Value, key: &str) -> Option<String> {
    candidate
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required_candidate_i64(candidate: &Value, key: &str) -> Result<i64> {
    let Some(value) = candidate.get(key) else {
        bail!("{key} is required");
    };
    if let Some(parsed) = value.as_i64() {
        return Ok(parsed);
    }
    let Some(raw) = value.as_str() else {
        bail!("{key} must be an integer");
    };
    raw.trim()
        .parse::<i64>()
        .map_err(|_| anyhow!("{key} must be an integer"))
}

fn optional_candidate_i64(candidate: &Value, key: &str) -> Option<i64> {
    candidate.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str()?.trim().parse().ok())
    })
}

fn required_candidate_f64(candidate: &Value, key: &str) -> Result<f64> {
    let Some(value) = candidate.get(key) else {
        bail!("{key} is required");
    };
    let parsed = if let Some(parsed) = value.as_f64() {
        parsed
    } else if let Some(raw) = value.as_str() {
        raw.trim()
            .parse::<f64>()
            .map_err(|_| anyhow!("{key} must be numeric"))?
    } else {
        bail!("{key} must be numeric");
    };
    if !parsed.is_finite() {
        bail!("{key} must be finite");
    }
    Ok(parsed)
}

fn optional_candidate_f64(candidate: &Value, key: &str) -> Result<Option<f64>> {
    if candidate.get(key).is_none_or(Value::is_null) {
        return Ok(None);
    }
    required_candidate_f64(candidate, key).map(Some)
}

fn position_summaries(positions: &[Position]) -> Vec<Value> {
    positions
        .iter()
        .map(|position| {
            json!({
                "exchange": position.exchange.as_str(),
                "symbol": position.exchange_symbol,
                "side": position.side,
                "size": position.size,
                "entry_price": position.entry_price,
                "mark_price": position.mark_price,
                "unrealized_pnl": position.unrealized_pnl,
                "leverage": position.leverage,
                "margin_mode": position.margin_mode,
            })
        })
        .collect()
}

fn fill_summaries(fills: &[Fill]) -> Vec<Value> {
    fills
        .iter()
        .map(|fill| {
            json!({
                "exchange": fill.exchange.as_str(),
                "symbol": fill.exchange_symbol,
                "trade_id": fill.trade_id,
                "order_id": fill.order_id,
                "side": fill.side,
                "price": fill.price,
                "size": fill.size,
                "fee": fill.fee,
                "fee_asset": fill.fee_asset,
                "role": fill.role,
                "timestamp": fill.timestamp,
            })
        })
        .collect()
}

fn open_order_summaries(open_orders: &[Order]) -> Vec<Value> {
    open_orders
        .iter()
        .map(|order| {
            json!({
                "exchange": order.exchange.as_str(),
                "symbol": order.exchange_symbol,
                "order_id": order.order_id,
                "client_order_id": order.client_order_id,
                "side": order.side,
                "order_type": order.order_type,
                "size": order.size,
                "filled_size": order.filled_size,
                "status": order.status,
            })
        })
        .collect()
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

fn same_exchange_symbol(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn normalized_fill_side(fill: &Fill) -> Option<String> {
    fill.side
        .as_deref()
        .map(str::trim)
        .filter(|side| !side.is_empty())
        .map(str::to_ascii_lowercase)
}

fn positive_decimal_option(value: Option<&str>) -> bool {
    value.is_some_and(positive_decimal_text)
}

fn decimal_texts_equal(left: Option<&str>, right: Option<&str>) -> bool {
    let Some(left) = left else {
        return false;
    };
    let Some(right) = right else {
        return false;
    };
    let Ok(left) = left.trim().parse::<f64>() else {
        return false;
    };
    let Ok(right) = right.trim().parse::<f64>() else {
        return false;
    };
    left.is_finite() && right.is_finite() && (left - right).abs() < 1e-12
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
