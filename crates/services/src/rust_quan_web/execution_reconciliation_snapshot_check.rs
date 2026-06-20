use anyhow::{anyhow, bail, Result};
use crypto_exc_all::{
    AccountBill, AccountBillQuery, Balance, ExchangeId, Fill, FillListQuery, Order, OrderListQuery,
    Position,
};
use serde_json::{json, Value};

use super::execution_audit::redact_error_message;
use super::execution_payload::{parse_exchange, parse_instrument};
use super::execution_task_contract::{
    ExchangeAccountBalanceSnapshotInput, ExchangeAccountBillSnapshotInput,
    ExchangeAccountOrderSnapshotInput, ExchangeAccountPositionSnapshotInput,
    ExchangeAccountSnapshotReportRequest, ExchangeAccountSnapshotReportResponse,
    ExchangeAccountTradeSnapshotInput, ExchangeCloseFillWritebackRequest,
    ExchangeCloseFillWritebackResponse,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshotSyncConfig {
    pub buyer_email: String,
    pub exchange: ExchangeId,
    pub symbol: String,
    pub combo_id: i64,
    pub task_id: i64,
    pub credential_ref: Option<String>,
    pub report_reconciliation: bool,
    pub include_fills: bool,
}

impl AccountSnapshotSyncConfig {
    pub fn into_reconciliation_config(self) -> ReconciliationSnapshotCheckConfig {
        ReconciliationSnapshotCheckConfig {
            buyer_email: self.buyer_email,
            exchange: self.exchange,
            symbol: self.symbol,
            combo_id: self.combo_id,
            task_id: self.task_id,
            credential_ref: self.credential_ref,
            report_reconciliation: self.report_reconciliation,
            include_fills: self.include_fills,
            close_fill_writeback_apply: false,
            close_fill_writeback_intent: None,
        }
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

include!("execution_reconciliation_snapshot_account_report_section.rs");
include!("execution_reconciliation_snapshot_runtime_section.rs");
include!("execution_reconciliation_snapshot_parse_helpers_section.rs");
include!("execution_reconciliation_snapshot_summary_helpers_section.rs");
include!("execution_reconciliation_snapshot_tests.rs");
