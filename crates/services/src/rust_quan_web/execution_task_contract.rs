use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLeaseRequest {
    pub worker_id: String,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_statuses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLease {
    pub tasks: Vec<ExecutionTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskConfirmationLease {
    pub items: Vec<ExecutionTaskConfirmationLeaseItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskConfirmationLeaseItem {
    pub task: ExecutionTask,
    pub order_result: ExchangeOrderResult,
}

impl<'de> Deserialize<'de> for ExecutionTaskLease {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawLease {
            #[serde(default)]
            tasks: Vec<ExecutionTask>,
            #[serde(default)]
            items: Vec<RawLeaseItem>,
        }

        #[derive(Deserialize)]
        struct RawLeaseItem {
            task: ExecutionTask,
        }

        let raw = RawLease::deserialize(deserializer)?;
        let tasks = if raw.tasks.is_empty() {
            raw.items.into_iter().map(|item| item.task).collect()
        } else {
            raw.tasks
        };

        Ok(Self { tasks })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTask {
    pub id: i64,
    #[serde(default)]
    pub news_signal_id: Option<i64>,
    #[serde(default)]
    pub strategy_signal_id: Option<i64>,
    pub combo_id: i64,
    pub buyer_email: String,
    pub strategy_slug: String,
    pub symbol: String,
    pub task_type: String,
    pub task_status: String,
    pub priority: i32,
    pub lease_owner: Option<String>,
    pub lease_until: Option<String>,
    pub scheduled_at: String,
    #[serde(deserialize_with = "deserialize_json_value_from_string")]
    pub request_payload_json: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeOrderResult {
    pub id: i64,
    pub execution_task_id: i64,
    pub combo_id: i64,
    pub buyer_email: String,
    pub exchange: String,
    pub external_order_id: String,
    pub order_side: String,
    pub order_status: String,
    pub filled_qty: Option<f64>,
    pub filled_quote: Option<f64>,
    pub fee_amount: Option<f64>,
    pub raw_payload_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskReportRequest {
    pub task_id: i64,
    pub execution_status: String,
    pub exchange: String,
    pub external_order_id: String,
    pub order_side: String,
    pub order_status: String,
    pub filled_qty: Option<f64>,
    pub filled_quote: Option<f64>,
    pub fee_amount: Option<f64>,
    pub profit_usdt: Option<f64>,
    pub executed_at: Option<String>,
    pub error_message: Option<String>,
    pub raw_payload_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskReportResponse {
    pub task: ExecutionTask,
    pub attempt: Value,
    pub order_result: Option<Value>,
    pub trade_record: Option<Value>,
}

impl ExecutionTaskReportRequest {
    pub fn success(
        task_id: i64,
        exchange: impl Into<String>,
        external_order_id: impl Into<String>,
        order_side: impl Into<String>,
        order_status: impl Into<String>,
        raw_payload: Value,
    ) -> Self {
        Self {
            task_id,
            execution_status: "completed".to_string(),
            exchange: exchange.into(),
            external_order_id: external_order_id.into(),
            order_side: order_side.into(),
            order_status: order_status.into(),
            filled_qty: None,
            filled_quote: None,
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: None,
            raw_payload_json: Some(raw_payload.to_string()),
        }
    }

    pub fn failed(
        task_id: i64,
        exchange: impl Into<String>,
        order_side: impl Into<String>,
        message: impl Into<String>,
        raw_payload: Value,
    ) -> Self {
        Self {
            task_id,
            execution_status: "failed".to_string(),
            exchange: exchange.into(),
            external_order_id: format!("failed-task-{task_id}"),
            order_side: order_side.into(),
            order_status: "failed".to_string(),
            filled_qty: None,
            filled_quote: None,
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: Some(message.into()),
            raw_payload_json: Some(raw_payload.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeReconciliationIssueType {
    ExchangePositionStale,
    ExchangeOpenOrderConflict,
    ExchangePositionFlat,
}

impl ExchangeReconciliationIssueType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExchangePositionStale => "exchange_position_stale",
            Self::ExchangeOpenOrderConflict => "exchange_open_order_conflict",
            Self::ExchangePositionFlat => "exchange_position_flat",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeReconciliationReportRequest {
    pub combo_id: i64,
    pub buyer_email: String,
    pub symbol: String,
    pub issue_type: ExchangeReconciliationIssueType,
    pub detected_at: Option<String>,
    pub source_ref: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeReconciliationReportResponse {
    pub combo_id: i64,
    pub buyer_email: String,
    pub symbol: String,
    pub signal_id: String,
    pub issue_type: String,
    pub api_execution_status: String,
    pub log: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountOrderSnapshotInput {
    pub external_order_id: String,
    pub order_side: String,
    pub order_status: String,
    pub price: Option<f64>,
    pub filled_qty: Option<f64>,
    pub filled_quote: Option<f64>,
    pub fee_amount: Option<f64>,
    pub raw_payload_json: Option<String>,
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountTradeSnapshotInput {
    pub external_trade_id: String,
    pub external_order_id: Option<String>,
    pub side: String,
    pub quantity: Option<f64>,
    pub quote_amount: Option<f64>,
    pub fee_amount: Option<f64>,
    pub price: Option<f64>,
    pub raw_payload_json: Option<String>,
    pub executed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountPositionSnapshotInput {
    pub side: String,
    pub quantity: f64,
    pub quote_amount: Option<f64>,
    pub leverage: Option<f64>,
    pub margin_mode: Option<String>,
    pub liquidation_price: Option<f64>,
    pub margin_ratio: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub protective_order_status: Option<String>,
    pub raw_payload_json: Option<String>,
    pub snapshot_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountBalanceSnapshotInput {
    pub asset: String,
    pub wallet_balance: Option<f64>,
    pub available_balance: Option<f64>,
    pub equity_usdt: Option<f64>,
    pub raw_payload_json: Option<String>,
    pub snapshot_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountBillSnapshotInput {
    pub external_bill_id: String,
    pub asset: String,
    pub balance_change: Option<f64>,
    pub balance_change_usdt: Option<f64>,
    pub balance_after: Option<f64>,
    pub fee_amount: Option<f64>,
    pub fee_usdt: Option<f64>,
    pub pnl_amount: Option<f64>,
    pub pnl_usdt: Option<f64>,
    pub bill_type: Option<String>,
    pub bill_sub_type: Option<String>,
    pub external_order_id: Option<String>,
    pub external_trade_id: Option<String>,
    pub raw_payload_json: Option<String>,
    pub bill_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountSnapshotReportRequest {
    pub combo_id: i64,
    pub buyer_email: String,
    pub exchange: String,
    pub symbol: String,
    pub source_ref: String,
    pub snapshot_at: Option<String>,
    #[serde(default)]
    pub orders: Vec<ExchangeAccountOrderSnapshotInput>,
    #[serde(default)]
    pub trades: Vec<ExchangeAccountTradeSnapshotInput>,
    #[serde(default)]
    pub positions: Vec<ExchangeAccountPositionSnapshotInput>,
    #[serde(default)]
    pub balances: Vec<ExchangeAccountBalanceSnapshotInput>,
    #[serde(default)]
    pub bills: Vec<ExchangeAccountBillSnapshotInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountSnapshotReportResponse {
    pub combo_id: i64,
    pub buyer_email: String,
    pub exchange: String,
    pub symbol: String,
    pub source_ref: String,
    pub snapshot_at: String,
    pub orders_upserted: i64,
    pub trades_upserted: i64,
    pub positions_upserted: i64,
    pub balances_upserted: i64,
    pub bills_upserted: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeCloseFillWritebackRequest {
    pub task_id: i64,
    pub combo_id: i64,
    pub exchange: String,
    pub symbol: String,
    pub source_ref: String,
    pub open_order_id: Option<String>,
    pub open_trade_id: Option<String>,
    pub close_order_id: String,
    pub close_trade_id: Option<String>,
    pub close_side: String,
    pub close_size: f64,
    pub close_price: Option<f64>,
    pub close_fee: Option<f64>,
    pub close_timestamp_ms: Option<i64>,
    pub position_flat_confirmed: bool,
    pub active_open_order_count: i64,
    pub quantity_match: bool,
    pub writeback_authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeCloseFillWritebackResponse {
    pub order_result: ExchangeOrderResult,
    pub trade_record: Value,
    pub position_snapshot_cleared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalSubmitRequest {
    pub source: String,
    pub external_id: String,
    pub strategy_slug: String,
    pub strategy_key: String,
    pub symbol: String,
    pub signal_type: String,
    pub direction: String,
    pub title: String,
    pub summary: Option<String>,
    pub confidence: Option<f64>,
    pub payload_json: String,
    pub generated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalInbox {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    pub strategy_slug: String,
    pub strategy_key: String,
    pub symbol: String,
    pub signal_type: String,
    pub direction: String,
    pub title: String,
    pub summary: Option<String>,
    pub confidence: Option<f64>,
    pub payload_json: String,
    pub generated_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalDispatchResponse {
    pub inbox: StrategySignalInbox,
    pub generated_tasks: Vec<ExecutionTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityPaperOutcomeRequest {
    pub rank_event_id: i64,
    pub exchange: String,
    pub symbol: String,
    pub target_r: f64,
    pub horizon_hours: i32,
    pub entry_rule_version: String,
    pub entry_trigger: Option<String>,
    pub entry_price: f64,
    pub entry_at: String,
    pub outcome_status: String,
    pub exit_reason: String,
    pub result_r: Option<f64>,
    pub evaluated_at: String,
    pub evaluation_payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityPaperOutcomeResponse {
    pub outcome: Value,
    pub generated_execution_task_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskCreationPreviewRequest {
    pub rank_event_id: Option<i64>,
    pub buyer_email: Option<String>,
    pub combo_id: Option<i64>,
    pub exchange: String,
    pub symbol: String,
    pub target_r: f64,
    pub horizon_hours: i32,
    pub entry_rule_version: Option<String>,
    pub entry_trigger_filter_version: Option<String>,
    pub risk_adjusted_win_rate_edge: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskCreationPreviewCheck {
    pub code: String,
    pub label: String,
    pub status: String,
    pub blocker_code: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskCreationPreviewResponse {
    pub read_only: bool,
    pub dry_run_only: bool,
    pub mutation_allowed: bool,
    pub would_create_execution_task: bool,
    pub generated_execution_task_count: i64,
    pub owner_service: String,
    pub status: String,
    pub exchange: String,
    pub symbol: String,
    pub rank_event_id: Option<i64>,
    pub buyer_email: Option<String>,
    pub combo_id: Option<i64>,
    pub target_r: f64,
    pub horizon_hours: i32,
    pub entry_rule_version: String,
    pub entry_trigger_filter_version: Option<String>,
    pub risk_adjusted_win_rate_edge: Option<f64>,
    pub required_web_checks: Vec<MarketVelocityExecutionTaskCreationPreviewCheck>,
    pub blocker_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskLiveReadinessCheck {
    pub code: String,
    pub label: String,
    pub status: String,
    pub blocker_code: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskLiveReadinessResponse {
    pub read_only: bool,
    pub mutation_allowed: bool,
    pub owner_service: String,
    pub status: String,
    pub task: ExecutionTask,
    pub checks: Vec<MarketVelocityExecutionTaskLiveReadinessCheck>,
    pub blocker_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct UserExchangeConfig {
    pub buyer_email: String,
    pub exchange: String,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: Option<String>,
    #[serde(default)]
    pub simulated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ApiCredentialCheckSummary {
    pub id: i64,
    pub exchange: String,
    pub api_key_mask: String,
    pub permission_scope: String,
    pub status: String,
    pub credential_envelope_ready: bool,
    pub last_check_at: Option<String>,
    pub last_check_code: Option<String>,
    pub last_check_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub execution_readiness: ApiCredentialExecutionReadiness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ApiCredentialExecutionReadiness {
    pub can_execute: bool,
    pub blocker_code: Option<String>,
    pub blocker_message: Option<String>,
    pub next_action_label: Option<String>,
    pub next_action_href: Option<String>,
}

fn deserialize_json_value_from_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Value, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(raw) => serde_json::from_str(&raw).map_err(serde::de::Error::custom),
        other => Ok(other),
    }
}
