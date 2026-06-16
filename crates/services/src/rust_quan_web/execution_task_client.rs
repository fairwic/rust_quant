use anyhow::{anyhow, Result};
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ExecutionTaskConfig {
    pub base_url: String,
    pub internal_secret: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionTaskClient {
    client: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
}

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

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    success: bool,
    data: T,
}

const LEASE_TASKS_PATH: &str = "/api/commerce/internal/execution-tasks/lease";
const LEASE_CONFIRMATION_TASKS_PATH: &str =
    "/api/commerce/internal/execution-tasks/confirmations/lease";
const REPORT_RESULT_PATH: &str = "/api/commerce/internal/execution-results";
const EXCHANGE_RECONCILIATION_PATH: &str = "/api/commerce/internal/exchange-reconciliation";
const EXCHANGE_CLOSE_FILL_WRITEBACK_PATH: &str =
    "/api/commerce/internal/exchange-close-fill-writeback";
const STRATEGY_SIGNAL_PATH: &str = "/api/commerce/internal/strategy-signals";
const MARKET_VELOCITY_PAPER_OUTCOME_PATH: &str =
    "/api/commerce/internal/market-velocity/paper-outcomes";
const MARKET_VELOCITY_TASK_CREATION_PREVIEW_PATH: &str =
    "/api/commerce/internal/market-velocity/execution-task-creation-preview";
const MARKET_VELOCITY_LIVE_TASK_READINESS_PATH_PREFIX: &str =
    "/api/commerce/internal/market-velocity/execution-tasks";
const USER_EXCHANGE_CONFIG_PATH: &str = "/api/commerce/internal/api-credentials/resolve";
const API_CREDENTIAL_CHECK_PATH_PREFIX: &str = "/api/commerce/internal/api-credentials";
const INTERNAL_SECRET_HEADER: &str = "x-alpha-execution-secret";

impl ExecutionTaskClient {
    pub fn new(config: ExecutionTaskConfig) -> Result<Self> {
        let base_url = config.base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(anyhow!("execution task base_url is empty"));
        }

        let client = reqwest::Client::builder().build()?;
        let internal_secret = {
            let secret = config.internal_secret.trim().to_string();
            (!secret.is_empty()).then_some(secret)
        };

        Ok(Self {
            client,
            base_url,
            internal_secret,
        })
    }

    pub async fn lease_tasks(
        &self,
        request: ExecutionTaskLeaseRequest,
    ) -> Result<ExecutionTaskLease> {
        self.get_json(&self.lease_url_for_request(&request)).await
    }

    pub async fn lease_confirmation_tasks(
        &self,
        limit: u32,
    ) -> Result<ExecutionTaskConfirmationLease> {
        self.get_json(&self.confirmation_lease_url(limit)).await
    }

    pub async fn report_result(
        &self,
        request: ExecutionTaskReportRequest,
    ) -> Result<ExecutionTaskReportResponse> {
        self.post_json(REPORT_RESULT_PATH, &request).await
    }

    pub async fn report_exchange_reconciliation(
        &self,
        request: ExchangeReconciliationReportRequest,
    ) -> Result<ExchangeReconciliationReportResponse> {
        self.post_json(EXCHANGE_RECONCILIATION_PATH, &request).await
    }

    pub async fn apply_exchange_close_fill_writeback(
        &self,
        request: ExchangeCloseFillWritebackRequest,
    ) -> Result<ExchangeCloseFillWritebackResponse> {
        self.post_json(EXCHANGE_CLOSE_FILL_WRITEBACK_PATH, &request)
            .await
    }

    pub async fn submit_strategy_signal(
        &self,
        request: StrategySignalSubmitRequest,
    ) -> Result<StrategySignalDispatchResponse> {
        self.post_json(STRATEGY_SIGNAL_PATH, &request).await
    }

    pub async fn submit_market_velocity_paper_outcome(
        &self,
        request: MarketVelocityPaperOutcomeRequest,
    ) -> Result<MarketVelocityPaperOutcomeResponse> {
        self.post_json(MARKET_VELOCITY_PAPER_OUTCOME_PATH, &request)
            .await
    }

    pub async fn preview_market_velocity_execution_task_creation(
        &self,
        request: MarketVelocityExecutionTaskCreationPreviewRequest,
    ) -> Result<MarketVelocityExecutionTaskCreationPreviewResponse> {
        self.post_json(MARKET_VELOCITY_TASK_CREATION_PREVIEW_PATH, &request)
            .await
    }

    pub async fn market_velocity_live_task_readiness(
        &self,
        task_id: i64,
    ) -> Result<MarketVelocityExecutionTaskLiveReadinessResponse> {
        self.get_json(&self.market_velocity_live_task_readiness_url(task_id))
            .await
    }

    pub async fn resolve_user_exchange_config(
        &self,
        buyer_email: &str,
        exchange: &str,
    ) -> Result<UserExchangeConfig> {
        let mut url = reqwest::Url::parse(&self.url(USER_EXCHANGE_CONFIG_PATH))?;
        url.query_pairs_mut()
            .append_pair("buyer_email", buyer_email)
            .append_pair("exchange", exchange);
        self.get_json(url.as_str()).await
    }

    pub async fn check_internal_api_credential(
        &self,
        credential_id: i64,
    ) -> Result<ApiCredentialCheckSummary> {
        let path = format!("{API_CREDENTIAL_CHECK_PATH_PREFIX}/{credential_id}/check");
        self.post_json(&path, &serde_json::json!({})).await
    }

    pub fn lease_url(&self, limit: u32) -> String {
        self.lease_url_for_request(&ExecutionTaskLeaseRequest {
            worker_id: String::new(),
            limit,
            task_ids: Vec::new(),
            task_types: Vec::new(),
            task_statuses: Vec::new(),
        })
    }

    pub fn confirmation_lease_url(&self, limit: u32) -> String {
        let mut url = reqwest::Url::parse(&self.url(LEASE_CONFIRMATION_TASKS_PATH))
            .expect("execution confirmation lease URL should always be valid");
        url.query_pairs_mut()
            .append_pair("limit", &limit.clamp(1, 100).to_string());
        url.to_string()
    }

    pub fn lease_url_for_request(&self, request: &ExecutionTaskLeaseRequest) -> String {
        let mut url = reqwest::Url::parse(&self.url(LEASE_TASKS_PATH))
            .expect("execution task lease URL should always be valid");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &request.limit.clamp(1, 100).to_string());
            for task_id in &request.task_ids {
                if *task_id > 0 {
                    query.append_pair("task_id", &task_id.to_string());
                }
            }
            for task_type in &request.task_types {
                if !task_type.trim().is_empty() {
                    query.append_pair("task_type", task_type);
                }
            }
            for task_status in &request.task_statuses {
                if !task_status.trim().is_empty() {
                    query.append_pair("task_status", task_status);
                }
            }
        }
        url.to_string()
    }

    pub fn strategy_signal_url(&self) -> String {
        self.url(STRATEGY_SIGNAL_PATH)
    }

    pub fn market_velocity_paper_outcome_url(&self) -> String {
        self.url(MARKET_VELOCITY_PAPER_OUTCOME_PATH)
    }

    pub fn market_velocity_live_task_readiness_url(&self, task_id: i64) -> String {
        self.url(&format!(
            "{MARKET_VELOCITY_LIVE_TASK_READINESS_PATH_PREFIX}/{task_id}/live-readiness"
        ))
    }

    pub fn exchange_reconciliation_url(&self) -> String {
        self.url(EXCHANGE_RECONCILIATION_PATH)
    }

    pub fn exchange_close_fill_writeback_url(&self) -> String {
        self.url(EXCHANGE_CLOSE_FILL_WRITEBACK_PATH)
    }

    pub fn parse_envelope<R>(body: &str) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let envelope = serde_json::from_str::<ApiEnvelope<R>>(body)
            .map_err(|e| anyhow!("parse quant_web envelope failed: {}; body={}", e, body))?;
        if !envelope.success {
            return Err(anyhow!("quant_web envelope success=false"));
        }
        Ok(envelope.data)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get_json<R>(&self, url: &str) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let mut request = self.client.get(url);
        if let Some(secret) = self.internal_secret.as_deref() {
            request = request.header(INTERNAL_SECRET_HEADER, secret);
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("GET {} returned {}: {}", url, status, body));
        }

        Self::parse_envelope(&body)
    }

    async fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let mut request = self
            .client
            .post(self.url(path))
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .json(body);

        if let Some(secret) = self.internal_secret.as_deref() {
            request = request.header(INTERNAL_SECRET_HEADER, secret);
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("{} returned {}: {}", path, status, body));
        }

        Self::parse_envelope(&body)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_lease_request_without_extra_noise() {
        let request = ExecutionTaskLeaseRequest {
            worker_id: "worker-a".to_string(),
            limit: 10,
            task_ids: vec![],
            task_types: vec![],
            task_statuses: vec![],
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["worker_id"], "worker-a");
        assert_eq!(value["limit"], 10);
        assert!(value.get("task_ids").is_none());
        assert!(value.get("task_types").is_none());
        assert!(value.get("task_statuses").is_none());
    }

    #[test]
    fn lease_url_matches_quant_web_internal_contract() {
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();

        assert_eq!(
            client.lease_url(25),
            "https://quant-web.example/api/commerce/internal/execution-tasks/lease?limit=25"
        );
    }

    #[test]
    fn confirmation_lease_url_matches_dedicated_internal_contract() {
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();

        assert_eq!(
            client.confirmation_lease_url(5),
            "https://quant-web.example/api/commerce/internal/execution-tasks/confirmations/lease?limit=5"
        );
    }

    #[test]
    fn parses_execution_task_envelope_from_quant_web() {
        let body = r#"{
            "success": true,
            "data": {
                "tasks": [{
                    "id": 42,
                    "news_signal_id": 7,
                    "combo_id": 9,
                    "buyer_email": "buyer@example.com",
                    "strategy_slug": "news_momentum",
                    "symbol": "BTC-USDT-SWAP",
                    "task_type": "execute_signal",
                    "task_status": "pending",
                    "priority": 3,
                    "lease_owner": null,
                    "lease_until": null,
                    "scheduled_at": "2026-04-23T12:00:00",
                    "request_payload_json": "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}",
                    "created_at": "2026-04-23T12:00:00",
                    "updated_at": "2026-04-23T12:00:00"
                }]
            }
        }"#;

        let parsed: ExecutionTaskLease = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(parsed.tasks[0].id, 42);
        assert_eq!(parsed.tasks[0].buyer_email, "buyer@example.com");
        assert_eq!(parsed.tasks[0].request_payload_json["size"], "0.01");
    }

    #[test]
    fn parses_strategy_execution_task_with_nullable_news_signal_id() {
        let body = r#"{
            "success": true,
            "data": {
                "items": [{
                    "task": {
                        "id": 43,
                        "news_signal_id": null,
                        "strategy_signal_id": 11,
                        "combo_id": 9,
                        "buyer_email": "buyer@example.com",
                        "strategy_slug": "vegas",
                        "symbol": "BTC-USDT-SWAP",
                        "task_type": "execute_signal",
                        "task_status": "leased",
                        "priority": 3,
                        "lease_owner": "rust_quant",
                        "lease_until": "2026-04-23T12:02:00",
                        "scheduled_at": "2026-04-23T12:00:00",
                        "request_payload_json": "{\"source_signal_type\":\"technical_strategy\",\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}",
                        "created_at": "2026-04-23T12:00:00",
                        "updated_at": "2026-04-23T12:00:00"
                    },
                    "api_credentials": []
                }]
            }
        }"#;

        let parsed: ExecutionTaskLease = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(parsed.tasks[0].id, 43);
        assert_eq!(parsed.tasks[0].news_signal_id, None);
        assert_eq!(parsed.tasks[0].strategy_signal_id, Some(11));
        assert_eq!(
            parsed.tasks[0].request_payload_json["source_signal_type"],
            "technical_strategy"
        );
    }

    #[test]
    fn strategy_signal_request_matches_quant_web_contract() {
        let request = StrategySignalSubmitRequest {
            source: "rust_quant".to_string(),
            external_id: "vegas-BTC-1713864000".to_string(),
            strategy_slug: "vegas".to_string(),
            strategy_key: "vegas_1h".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            signal_type: "entry".to_string(),
            direction: "long".to_string(),
            title: "Vegas long signal".to_string(),
            summary: Some("EMA alignment confirmed".to_string()),
            confidence: Some(0.82),
            payload_json: "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}".to_string(),
            generated_at: Some("2026-04-23T12:00:00Z".to_string()),
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["source"], "rust_quant");
        assert_eq!(value["strategy_slug"], "vegas");
        assert_eq!(value["strategy_key"], "vegas_1h");
        assert_eq!(value["direction"], "long");
        assert_eq!(
            value["payload_json"],
            "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}"
        );

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.strategy_signal_url(),
            "https://quant-web.example/api/commerce/internal/strategy-signals"
        );
    }

    #[test]
    fn parses_execution_task_items_envelope_from_quant_web() {
        let body = r#"{
            "success": true,
            "data": {
                "items": [{
                    "task": {
                        "id": 42,
                        "news_signal_id": 7,
                        "combo_id": 9,
                        "buyer_email": "buyer@example.com",
                        "strategy_slug": "news_momentum",
                        "symbol": "BTC-USDT-SWAP",
                        "task_type": "execute_signal",
                        "task_status": "leased",
                        "priority": 3,
                        "lease_owner": "rust_quant",
                        "lease_until": "2026-04-23T12:02:00",
                        "scheduled_at": "2026-04-23T12:00:00",
                        "request_payload_json": "{\"signal_type\":\"buy\",\"payload_json\":\"{\\\"exchange\\\":\\\"okx\\\",\\\"side\\\":\\\"buy\\\",\\\"size\\\":\\\"0.01\\\"}\"}",
                        "created_at": "2026-04-23T12:00:00",
                        "updated_at": "2026-04-23T12:00:00"
                    },
                    "api_credentials": []
                }]
            }
        }"#;

        let parsed: ExecutionTaskLease = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(parsed.tasks.len(), 1);
        assert_eq!(parsed.tasks[0].id, 42);
        assert_eq!(parsed.tasks[0].task_status, "leased");
    }

    #[test]
    fn report_request_matches_quant_web_order_result_contract() {
        let request = ExecutionTaskReportRequest::success(
            42,
            "okx",
            "order-1",
            "buy",
            "filled",
            serde_json::json!({"dry_run": true}),
        );
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["task_id"], 42);
        assert_eq!(value["execution_status"], "completed");
        assert_eq!(value["external_order_id"], "order-1");
        assert_eq!(value["raw_payload_json"], "{\"dry_run\":true}");
        assert!(value.get("worker_id").is_none());
    }

    #[test]
    fn exchange_reconciliation_request_matches_quant_web_contract() {
        let request = ExchangeReconciliationReportRequest {
            combo_id: 9,
            buyer_email: "buyer@example.com".to_string(),
            symbol: "ETHUSDT".to_string(),
            issue_type: ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
            detected_at: Some("2026-05-15T09:30:00Z".to_string()),
            source_ref: Some(
                "rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT"
                    .to_string(),
            ),
            message: Some("open order conflict detected".to_string()),
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["combo_id"], 9);
        assert_eq!(value["buyer_email"], "buyer@example.com");
        assert_eq!(value["symbol"], "ETHUSDT");
        assert_eq!(value["issue_type"], "exchange_open_order_conflict");
        assert_eq!(value["detected_at"], "2026-05-15T09:30:00Z");
        assert_eq!(
            value["source_ref"],
            "rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT"
        );

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.exchange_reconciliation_url(),
            "https://quant-web.example/api/commerce/internal/exchange-reconciliation"
        );
    }

    #[test]
    fn exchange_close_fill_writeback_request_matches_quant_web_contract() {
        let request = ExchangeCloseFillWritebackRequest {
            task_id: 86,
            combo_id: 85,
            exchange: "okx".to_string(),
            symbol: "ASTER-USDT-SWAP".to_string(),
            source_ref: "rq:xrec:v2:ex=okx:combo=85:task=86:sym=ASTER-USDT-SWAP".to_string(),
            open_order_id: Some("3631557801300238336".to_string()),
            open_trade_id: Some("211849844".to_string()),
            close_order_id: "3631564680998985728".to_string(),
            close_trade_id: Some("211850229".to_string()),
            close_side: "sell".to_string(),
            close_size: 1.0,
            close_price: Some(0.6047),
            close_fee: Some(-0.00030235),
            close_timestamp_ms: Some(1_780_731_461_395),
            position_flat_confirmed: true,
            active_open_order_count: 0,
            quantity_match: true,
            writeback_authorized: true,
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["task_id"], 86);
        assert_eq!(value["combo_id"], 85);
        assert_eq!(value["exchange"], "okx");
        assert_eq!(value["symbol"], "ASTER-USDT-SWAP");
        assert_eq!(value["close_order_id"], "3631564680998985728");
        assert_eq!(value["close_side"], "sell");
        assert_eq!(value["close_size"], 1.0);
        assert_eq!(value["close_price"], 0.6047);
        assert_eq!(value["position_flat_confirmed"], true);
        assert_eq!(value["active_open_order_count"], 0);
        assert_eq!(value["quantity_match"], true);
        assert_eq!(value["writeback_authorized"], true);
        assert!(!value.to_string().contains("buyer@example.com"));

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.exchange_close_fill_writeback_url(),
            "https://quant-web.example/api/commerce/internal/exchange-close-fill-writeback"
        );
    }

    #[test]
    fn parses_user_exchange_config_envelope_without_persisting_secret() {
        let body = r#"{
            "success": true,
            "data": {
                "buyer_email": "buyer@example.com",
                "exchange": "OKX",
                "api_key": "api-key",
                "api_secret": "api-secret",
                "passphrase": "passphrase",
                "simulated": true
            }
        }"#;

        let config: UserExchangeConfig = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(config.exchange, "OKX");
        assert_eq!(config.api_key, "api-key");
        assert_eq!(config.passphrase.as_deref(), Some("passphrase"));
        assert!(config.simulated);
    }

    #[tokio::test]
    async fn resolve_user_exchange_config_uses_internal_get_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"buyer_email":"buyer@example.com","exchange":"OKX","api_key":"plain-api-key","api_secret":"plain-api-secret","passphrase":"plain-passphrase","simulated":false}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let config = client
            .resolve_user_exchange_config("buyer@example.com", "OKX")
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(request.starts_with(
            "GET /api/commerce/internal/api-credentials/resolve?buyer_email=buyer%40example.com&exchange=OKX HTTP/1.1"
        ));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert_eq!(config.buyer_email, "buyer@example.com");
        assert_eq!(config.api_key, "plain-api-key");
        assert_eq!(config.api_secret, "plain-api-secret");
        assert_eq!(config.passphrase.as_deref(), Some("plain-passphrase"));
        assert!(!config.simulated);
    }

    #[tokio::test]
    async fn check_internal_api_credential_uses_internal_post_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"id":42,"exchange":"OKX","api_key_mask":"okx_***_tail","permission_scope":"trade","status":"active","credential_envelope_ready":true,"last_check_at":"2026-06-05T08:00:00","last_check_code":"signed_exchange_preflight_passed","last_check_message":"ok","created_at":"2026-06-05T07:00:00","updated_at":"2026-06-05T08:00:00","execution_readiness":{"can_execute":true,"blocker_code":null,"blocker_message":null,"next_action_label":null,"next_action_href":null}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let summary = client.check_internal_api_credential(42).await.unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(
            request.starts_with("POST /api/commerce/internal/api-credentials/42/check HTTP/1.1")
        );
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert!(request.contains("content-type: application/json"));
        assert!(!request.contains("plain-api-secret"));
        assert_eq!(summary.id, 42);
        assert_eq!(summary.exchange, "OKX");
        assert_eq!(
            summary.last_check_code.as_deref(),
            Some("signed_exchange_preflight_passed")
        );
        assert!(summary.credential_envelope_ready);
        assert!(summary.execution_readiness.can_execute);
    }

    #[tokio::test]
    async fn preview_market_velocity_task_creation_uses_internal_owner_route() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"read_only":true,"dry_run_only":true,"mutation_allowed":false,"would_create_execution_task":false,"generated_execution_task_count":0,"owner_service":"quant_web","status":"ready","exchange":"okx","symbol":"ASTER-USDT-SWAP","rank_event_id":2042663,"buyer_email":"buyer@example.com","combo_id":85,"target_r":2.4,"horizon_hours":48,"entry_rule_version":"rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1","entry_trigger_filter_version":"entry_trigger_allowlist_v1","risk_adjusted_win_rate_edge":null,"required_web_checks":[],"blocker_codes":[]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let preview = client
            .preview_market_velocity_execution_task_creation(
                MarketVelocityExecutionTaskCreationPreviewRequest {
                    rank_event_id: Some(2042663),
                    buyer_email: Some("buyer@example.com".to_string()),
                    combo_id: Some(85),
                    exchange: "okx".to_string(),
                    symbol: "ASTER-USDT-SWAP".to_string(),
                    target_r: 2.4,
                    horizon_hours: 48,
                    entry_rule_version: Some(
                        "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1".to_string(),
                    ),
                    entry_trigger_filter_version: Some("entry_trigger_allowlist_v1".to_string()),
                    risk_adjusted_win_rate_edge: None,
                },
            )
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(request.starts_with(
            "POST /api/commerce/internal/market-velocity/execution-task-creation-preview HTTP/1.1"
        ));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert!(request.contains("\"rank_event_id\":2042663"));
        assert!(preview.read_only);
        assert!(preview.dry_run_only);
        assert!(!preview.mutation_allowed);
        assert_eq!(preview.owner_service, "quant_web");
        assert!(preview.blocker_codes.is_empty());
    }

    #[tokio::test]
    async fn report_exchange_reconciliation_uses_internal_post_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"combo_id":9,"buyer_email":"buyer@example.com","symbol":"ETHUSDT","signal_id":"exchange-reconciliation-exchange_open_order_conflict-9-ref","issue_type":"exchange_open_order_conflict","api_execution_status":"blocked_by_reconciliation","log":{}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let response = client
            .report_exchange_reconciliation(ExchangeReconciliationReportRequest {
                combo_id: 9,
                buyer_email: "buyer@example.com".to_string(),
                symbol: "ETHUSDT".to_string(),
                issue_type: ExchangeReconciliationIssueType::ExchangeOpenOrderConflict,
                detected_at: Some("2026-05-15T09:30:00Z".to_string()),
                source_ref: Some(
                    "rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT"
                        .to_string(),
                ),
                message: Some("open order conflict detected".to_string()),
            })
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(request.starts_with("POST /api/commerce/internal/exchange-reconciliation HTTP/1.1"));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert!(request.contains(r#""combo_id":9"#));
        assert!(request.contains(r#""buyer_email":"buyer@example.com""#));
        assert!(request.contains(r#""symbol":"ETHUSDT""#));
        assert!(request.contains(r#""issue_type":"exchange_open_order_conflict""#));
        assert!(request.contains(r#""detected_at":"2026-05-15T09:30:00Z""#));
        assert!(request.contains(
            r#""source_ref":"rust_quant/exchange_reconciliation/exchange_open_order_conflict/combo/9/task/42/symbol/ETHUSDT""#
        ));
        assert_eq!(response.combo_id, 9);
        assert_eq!(response.issue_type, "exchange_open_order_conflict");
        assert_eq!(response.api_execution_status, "blocked_by_reconciliation");
    }

    #[tokio::test]
    async fn apply_exchange_close_fill_writeback_uses_internal_post_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"order_result":{"id":30,"execution_task_id":86,"combo_id":85,"buyer_email":"buyer@example.com","exchange":"okx","external_order_id":"3631564680998985728","order_side":"sell","order_status":"filled","filled_qty":1.0,"filled_quote":0.6047,"fee_amount":-0.00030235,"raw_payload_json":"{}","created_at":"2026-06-06T08:00:00","updated_at":"2026-06-06T08:00:00"},"trade_record":{"id":44,"exchange_order_result_id":30,"side":"sell"},"position_snapshot_cleared":true}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let response = client
            .apply_exchange_close_fill_writeback(ExchangeCloseFillWritebackRequest {
                task_id: 86,
                combo_id: 85,
                exchange: "okx".to_string(),
                symbol: "ASTER-USDT-SWAP".to_string(),
                source_ref: "rq:xrec:v2:ex=okx:combo=85:task=86:sym=ASTER-USDT-SWAP".to_string(),
                open_order_id: Some("3631557801300238336".to_string()),
                open_trade_id: Some("211849844".to_string()),
                close_order_id: "3631564680998985728".to_string(),
                close_trade_id: Some("211850229".to_string()),
                close_side: "sell".to_string(),
                close_size: 1.0,
                close_price: Some(0.6047),
                close_fee: Some(-0.00030235),
                close_timestamp_ms: Some(1_780_731_461_395),
                position_flat_confirmed: true,
                active_open_order_count: 0,
                quantity_match: true,
                writeback_authorized: true,
            })
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(request
            .starts_with("POST /api/commerce/internal/exchange-close-fill-writeback HTTP/1.1"));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert!(request.contains(r#""task_id":86"#));
        assert!(request.contains(r#""combo_id":85"#));
        assert!(request.contains(r#""exchange":"okx""#));
        assert!(request.contains(r#""symbol":"ASTER-USDT-SWAP""#));
        assert!(request.contains(r#""close_order_id":"3631564680998985728""#));
        assert!(request.contains(r#""close_side":"sell""#));
        assert!(request.contains(r#""close_size":1.0"#) || request.contains(r#""close_size":1"#));
        assert!(request.contains(r#""writeback_authorized":true"#));
        assert!(!request.contains("api-secret"));
        assert!(response.position_snapshot_cleared);
        assert_eq!(
            response.order_result.external_order_id,
            "3631564680998985728"
        );
        assert_eq!(response.trade_record["side"], "sell");
    }

    #[test]
    fn market_velocity_paper_outcome_request_matches_quant_web_contract() {
        let request = MarketVelocityPaperOutcomeRequest {
            rank_event_id: 77,
            exchange: "okx".to_string(),
            symbol: "ETH-USDT-SWAP".to_string(),
            target_r: 1.5,
            horizon_hours: 24,
            entry_rule_version: "rank_radar_4h_15m_v2".to_string(),
            entry_trigger: Some("breakout_previous_high".to_string()),
            entry_price: 100.0,
            entry_at: "2026-06-15T00:15:00Z".to_string(),
            outcome_status: "win".to_string(),
            exit_reason: "target_hit".to_string(),
            result_r: Some(1.5),
            evaluated_at: "2026-06-15T01:00:00Z".to_string(),
            evaluation_payload: serde_json::json!({
                "source": "market_velocity_event_backtest",
                "target_r": 1.5
            }),
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["rank_event_id"], 77);
        assert_eq!(value["exchange"], "okx");
        assert_eq!(value["symbol"], "ETH-USDT-SWAP");
        assert_eq!(value["target_r"], 1.5);
        assert_eq!(value["horizon_hours"], 24);
        assert_eq!(value["entry_rule_version"], "rank_radar_4h_15m_v2");
        assert_eq!(value["outcome_status"], "win");
        assert_eq!(value["generated_execution_task_count"], Value::Null);
        assert!(!value.to_string().contains("buyer_email"));
        assert!(!value.to_string().contains("execution_task"));

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.market_velocity_paper_outcome_url(),
            "https://quant-web.example/api/commerce/internal/market-velocity/paper-outcomes"
        );
    }

    #[tokio::test]
    async fn submit_market_velocity_paper_outcome_uses_internal_post_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"outcome":{"id":9,"rank_event_id":77,"exchange":"okx","symbol":"ETH-USDT-SWAP","target_r":1.5,"horizon_hours":24,"entry_rule_version":"rank_radar_4h_15m_v2","outcome_status":"win"},"generated_execution_task_count":0}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let response = client
            .submit_market_velocity_paper_outcome(MarketVelocityPaperOutcomeRequest {
                rank_event_id: 77,
                exchange: "okx".to_string(),
                symbol: "ETH-USDT-SWAP".to_string(),
                target_r: 1.5,
                horizon_hours: 24,
                entry_rule_version: "rank_radar_4h_15m_v2".to_string(),
                entry_trigger: Some("breakout_previous_high".to_string()),
                entry_price: 100.0,
                entry_at: "2026-06-15T00:15:00Z".to_string(),
                outcome_status: "win".to_string(),
                exit_reason: "target_hit".to_string(),
                result_r: Some(1.5),
                evaluated_at: "2026-06-15T01:00:00Z".to_string(),
                evaluation_payload: serde_json::json!({
                    "source": "market_velocity_event_backtest",
                    "target_r": 1.5
                }),
            })
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(request
            .starts_with("POST /api/commerce/internal/market-velocity/paper-outcomes HTTP/1.1"));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert!(request.contains(r#""rank_event_id":77"#));
        assert!(request.contains(r#""symbol":"ETH-USDT-SWAP""#));
        assert!(request.contains(r#""target_r":1.5"#));
        assert!(!request.contains("buyer@example.com"));
        assert_eq!(response.generated_execution_task_count, 0);
        assert_eq!(response.outcome["rank_event_id"], 77);
    }

    #[tokio::test]
    async fn lease_tasks_uses_task_type_filters_in_internal_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"tasks":[]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let leased = client
            .lease_tasks(ExecutionTaskLeaseRequest {
                worker_id: "worker-close".to_string(),
                limit: 5,
                task_ids: vec![42, 43],
                task_types: vec![
                    "execute_signal".to_string(),
                    "risk_control_close_candidate".to_string(),
                ],
                task_statuses: vec!["pending".to_string(), "pending_close".to_string()],
            })
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(leased.tasks.is_empty());
        assert!(request.starts_with(
            "GET /api/commerce/internal/execution-tasks/lease?limit=5&task_id=42&task_id=43&task_type=execute_signal&task_type=risk_control_close_candidate&task_status=pending&task_status=pending_close HTTP/1.1"
        ));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
    }

    #[test]
    fn validates_base_url() {
        let err = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "   ".to_string(),
            internal_secret: "secret".to_string(),
        })
        .expect_err("empty base_url must fail");

        assert!(err.to_string().contains("base_url"));
    }
}
