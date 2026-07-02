use super::execution_task_contract::{
    ApiCredentialCheckSummary, ExchangeAccountSnapshotReportRequest,
    ExchangeAccountSnapshotReportResponse, ExchangeCloseFillWritebackRequest,
    ExchangeCloseFillWritebackResponse, ExchangeReconciliationReportRequest,
    ExchangeReconciliationReportResponse, ExecutionRiskReservationRequest,
    ExecutionRiskReservationResponse, ExecutionTaskConfirmationLease, ExecutionTaskLease,
    ExecutionTaskLeaseExtendRequest, ExecutionTaskLeaseExtendResponse, ExecutionTaskLeaseRequest,
    ExecutionTaskReportRequest, ExecutionTaskReportResponse,
    MarketVelocityExecutionTaskCreationPreviewRequest,
    MarketVelocityExecutionTaskCreationPreviewResponse,
    MarketVelocityExecutionTaskLiveReadinessResponse, MarketVelocityPaperOutcomeRequest,
    MarketVelocityPaperOutcomeResponse, StrategySignalDispatchResponse,
    StrategySignalSubmitRequest, UserExchangeConfig,
};
#[cfg(test)]
use super::execution_task_contract::{
    ExchangeAccountBalanceSnapshotInput, ExchangeAccountBillSnapshotInput,
    ExchangeAccountOrderSnapshotInput, ExchangeAccountPositionHistorySnapshotInput,
    ExchangeAccountPositionSnapshotInput, ExchangeAccountTradeSnapshotInput,
    ExchangeReconciliationIssueType,
};
use anyhow::{anyhow, Result};
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{error::Error, fmt};
#[derive(Debug, Clone)]
pub struct ExecutionTaskConfig {
    /// 基础URL，用于配置运行参数。
    pub base_url: String,
    /// internalSecret，用于配置运行参数。
    pub internal_secret: String,
}
#[derive(Debug, Clone)]
pub struct ExecutionTaskClient {
    /// 外部服务客户端。
    client: reqwest::Client,
    /// 基础URL，用于记录交易或执行状态。
    base_url: String,
    /// internalsecret；为空时表示该条件不启用。
    internal_secret: Option<String>,
}
#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    success: bool,
    data: T,
}
#[derive(Debug, Deserialize)]
struct ApiErrorEnvelope {
    code: Option<String>,
}
#[derive(Debug, Clone)]
pub struct QuantWebClientError {
    method: &'static str,
    target: String,
    status: StatusCode,
    code: Option<String>,
    body_len: usize,
}

impl QuantWebClientError {
    /// 保存 Web owner service 的结构化错误码，同时继续隐藏响应正文，避免泄露凭证或内部信息。
    fn from_response(
        method: &'static str,
        target: impl Into<String>,
        status: StatusCode,
        body: &str,
    ) -> Self {
        Self {
            method,
            target: target.into(),
            status,
            code: parse_api_error_code(body),
            body_len: body.as_bytes().len(),
        }
    }

    pub fn error_code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

impl fmt::Display for QuantWebClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} returned {}; {}",
            self.method,
            self.target,
            self.status,
            response_body_context_for_len(self.body_len)
        )?;
        if let Some(code) = self.code.as_deref() {
            write!(f, "; code={code}")?;
        }
        Ok(())
    }
}

impl Error for QuantWebClientError {}
const LEASE_TASKS_PATH: &str = "/api/commerce/internal/execution-tasks/lease";
const EXECUTION_TASKS_PATH_PREFIX: &str = "/api/commerce/internal/execution-tasks";
const LEASE_CONFIRMATION_TASKS_PATH: &str =
    "/api/commerce/internal/execution-tasks/confirmations/lease";
const REPORT_RESULT_PATH: &str = "/api/commerce/internal/execution-results";
const EXCHANGE_RECONCILIATION_PATH: &str = "/api/commerce/internal/exchange-reconciliation";
const EXCHANGE_ACCOUNT_SNAPSHOT_PATH: &str = "/api/commerce/internal/exchange-account-snapshots";
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
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
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
    pub async fn extend_task_lease(
        &self,
        task_id: i64,
        request: ExecutionTaskLeaseExtendRequest,
    ) -> Result<ExecutionTaskLeaseExtendResponse> {
        self.post_json(&self.lease_extend_path(task_id), &request)
            .await
    }
    pub async fn reserve_execution_risk_budget(
        &self,
        task_id: i64,
        request: ExecutionRiskReservationRequest,
    ) -> Result<ExecutionRiskReservationResponse> {
        self.post_json(&self.risk_reservation_path(task_id), &request)
            .await
    }
    /// 提供lease确认tasks的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub async fn lease_confirmation_tasks(
        &self,
        limit: u32,
        task_ids: &[i64],
    ) -> Result<ExecutionTaskConfirmationLease> {
        self.get_json(&self.confirmation_lease_url_for_task_ids(limit, task_ids))
            .await
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
    /// 提供报告交易所account快照的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub async fn report_exchange_account_snapshot(
        &self,
        request: ExchangeAccountSnapshotReportRequest,
    ) -> Result<ExchangeAccountSnapshotReportResponse> {
        self.post_json(EXCHANGE_ACCOUNT_SNAPSHOT_PATH, &request)
            .await
    }
    /// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
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
    /// 执行提交市场动量paper结果步骤，串起Web 商业链路需要的状态推进和错误处理。
    pub async fn submit_market_velocity_paper_outcome(
        &self,
        request: MarketVelocityPaperOutcomeRequest,
    ) -> Result<MarketVelocityPaperOutcomeResponse> {
        self.post_json(MARKET_VELOCITY_PAPER_OUTCOME_PATH, &request)
            .await
    }
    /// 提供preview市场动量执行taskcreation的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub async fn preview_market_velocity_execution_task_creation(
        &self,
        request: MarketVelocityExecutionTaskCreationPreviewRequest,
    ) -> Result<MarketVelocityExecutionTaskCreationPreviewResponse> {
        self.post_json(MARKET_VELOCITY_TASK_CREATION_PREVIEW_PATH, &request)
            .await
    }
    /// 提供市场动量livetaskreadiness的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub async fn market_velocity_live_task_readiness(
        &self,
        task_id: i64,
    ) -> Result<MarketVelocityExecutionTaskLiveReadinessResponse> {
        self.get_json(&self.market_velocity_live_task_readiness_url(task_id))
            .await
    }
    /// 选择 Web 商业、会员和执行准备度 的最佳候选结果，避免选择规则分散在调用方。
    pub async fn resolve_user_exchange_config(
        &self,
        buyer_email: &str,
        exchange: &str,
    ) -> Result<UserExchangeConfig> {
        self.resolve_user_exchange_config_with_optional_credential(buyer_email, exchange, None)
            .await
    }
    /// 选择 Web 商业、会员和执行准备度 的最佳候选结果，避免选择规则分散在调用方。
    pub async fn resolve_user_exchange_config_for_credential(
        &self,
        buyer_email: &str,
        exchange: &str,
        credential_id: i64,
    ) -> Result<UserExchangeConfig> {
        self.resolve_user_exchange_config_with_optional_credential(
            buyer_email,
            exchange,
            Some(credential_id),
        )
        .await
    }
    /// 选择 Web 商业、会员和执行准备度 的最佳候选结果，避免选择规则分散在调用方。
    async fn resolve_user_exchange_config_with_optional_credential(
        &self,
        buyer_email: &str,
        exchange: &str,
        credential_id: Option<i64>,
    ) -> Result<UserExchangeConfig> {
        let mut url = reqwest::Url::parse(&self.url(USER_EXCHANGE_CONFIG_PATH))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("buyer_email", buyer_email)
                .append_pair("exchange", exchange);
            if let Some(credential_id) = credential_id {
                query.append_pair("credential_id", &credential_id.to_string());
            }
        }
        self.get_json(url.as_str()).await
    }
    /// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
    pub async fn check_internal_api_credential(
        &self,
        credential_id: i64,
    ) -> Result<ApiCredentialCheckSummary> {
        let path = format!("{API_CREDENTIAL_CHECK_PATH_PREFIX}/{credential_id}/check");
        self.post_json(&path, &serde_json::json!({})).await
    }
    /// 提供leaseURL的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn lease_url(&self, limit: u32) -> String {
        self.lease_url_for_request(&ExecutionTaskLeaseRequest {
            worker_id: String::new(),
            limit,
            task_ids: Vec::new(),
            task_types: Vec::new(),
            task_statuses: Vec::new(),
        })
    }
    pub fn lease_extend_url(&self, task_id: i64) -> String {
        self.url(&self.lease_extend_path(task_id))
    }
    pub fn risk_reservation_url(&self, task_id: i64) -> String {
        self.url(&self.risk_reservation_path(task_id))
    }
    pub fn confirmation_lease_url(&self, limit: u32) -> String {
        self.confirmation_lease_url_for_task_ids(limit, &[])
    }
    fn lease_extend_path(&self, task_id: i64) -> String {
        format!("{EXECUTION_TASKS_PATH_PREFIX}/{task_id}/lease/extend")
    }
    fn risk_reservation_path(&self, task_id: i64) -> String {
        format!("{EXECUTION_TASKS_PATH_PREFIX}/{task_id}/risk-reservation")
    }
    /// 提供确认leaseURLfortaskids的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn confirmation_lease_url_for_task_ids(&self, limit: u32, task_ids: &[i64]) -> String {
        let mut url = reqwest::Url::parse(&self.url(LEASE_CONFIRMATION_TASKS_PATH))
            .expect("execution confirmation lease URL should always be valid");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &limit.clamp(1, 100).to_string());
            for task_id in task_ids {
                if *task_id > 0 {
                    query.append_pair("task_id", &task_id.to_string());
                }
            }
        }
        url.to_string()
    }
    /// 提供leaseURLforrequest的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
    /// 提供市场动量livetaskreadinessURL的集中实现，避免Web 商业链路调用方重复处理相同细节。
    pub fn market_velocity_live_task_readiness_url(&self, task_id: i64) -> String {
        self.url(&format!(
            "{MARKET_VELOCITY_LIVE_TASK_READINESS_PATH_PREFIX}/{task_id}/live-readiness"
        ))
    }
    pub fn exchange_reconciliation_url(&self) -> String {
        self.url(EXCHANGE_RECONCILIATION_PATH)
    }
    pub fn exchange_account_snapshot_url(&self) -> String {
        self.url(EXCHANGE_ACCOUNT_SNAPSHOT_PATH)
    }
    pub fn exchange_close_fill_writeback_url(&self) -> String {
        self.url(EXCHANGE_CLOSE_FILL_WRITEBACK_PATH)
    }
    /// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
    pub fn parse_envelope<R>(body: &str) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let envelope = serde_json::from_str::<ApiEnvelope<R>>(body).map_err(|e| {
            anyhow!(
                "parse quant_web envelope failed: {}; {}",
                e,
                response_body_context(body)
            )
        })?;
        if !envelope.success {
            return Err(anyhow!("quant_web envelope success=false"));
        }
        Ok(envelope.data)
    }
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
    /// 加载 Web 商业、会员和执行准备度 运行所需数据，并把缺失或异常交给调用方处理。
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
            return Err(QuantWebClientError::from_response("GET", url, status, &body).into());
        }
        Self::parse_envelope(&body)
    }
    /// 提供postJSON的集中实现，避免Web 商业链路调用方重复处理相同细节。
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
            return Err(QuantWebClientError::from_response("POST", path, status, &body).into());
        }
        Self::parse_envelope(&body)
    }
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn response_body_context(body: &str) -> String {
    response_body_context_for_len(body.as_bytes().len())
}

fn response_body_context_for_len(body_len: usize) -> String {
    format!("response_body_omitted=true body_len={}", body_len)
}

fn parse_api_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<ApiErrorEnvelope>(body)
        .ok()
        .and_then(|envelope| envelope.code)
        .map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    mod http_contract;
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
    fn lease_extend_url_matches_quant_web_internal_contract() {
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.lease_extend_url(42),
            "https://quant-web.example/api/commerce/internal/execution-tasks/42/lease/extend"
        );
    }
    #[test]
    fn risk_reservation_url_matches_quant_web_internal_contract() {
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.risk_reservation_url(42),
            "https://quant-web.example/api/commerce/internal/execution-tasks/42/risk-reservation"
        );
    }
    #[test]
    fn risk_reservation_request_serializes_minimum_notional() {
        let request = ExecutionRiskReservationRequest {
            minimum_notional_usdt: Some(20.70342),
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["minimum_notional_usdt"], 20.70342);
    }
    #[test]
    fn lease_extend_request_serializes_worker_and_seconds() {
        let request = ExecutionTaskLeaseExtendRequest {
            worker_id: "worker-a".to_string(),
            extend_seconds: Some(180),
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["worker_id"], "worker-a");
        assert_eq!(value["extend_seconds"], 180);
    }
    #[test]
    fn confirmation_lease_url_scopes_target_task_ids() {
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.confirmation_lease_url_for_task_ids(5, &[42, -1, 43]),
            "https://quant-web.example/api/commerce/internal/execution-tasks/confirmations/lease?limit=5&task_id=42&task_id=43"
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
    fn exchange_account_snapshot_request_matches_quant_web_contract() {
        let request = ExchangeAccountSnapshotReportRequest {
            combo_id: 85,
            buyer_email: "buyer@example.com".to_string(),
            exchange: "okx".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            source_ref: "rq:acct:v1:ex=okx:combo=85:sym=BTC-USDT-SWAP".to_string(),
            snapshot_at: Some("2026-06-18T02:30:00".to_string()),
            orders: vec![ExchangeAccountOrderSnapshotInput {
                external_order_id: "3631557801300238336".to_string(),
                order_side: "buy".to_string(),
                order_status: "filled".to_string(),
                price: Some(66000.0),
                filled_qty: Some(0.01),
                filled_quote: Some(660.0),
                fee_amount: Some(0.33),
                raw_payload_json: Some(r#"{"ordId":"3631557801300238336"}"#.to_string()),
                observed_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            trades: vec![ExchangeAccountTradeSnapshotInput {
                external_trade_id: "211849844".to_string(),
                external_order_id: Some("3631557801300238336".to_string()),
                side: "buy".to_string(),
                quantity: Some(0.01),
                quote_amount: Some(660.0),
                fee_amount: Some(0.33),
                price: Some(66000.0),
                raw_payload_json: Some(r#"{"tradeId":"211849844"}"#.to_string()),
                executed_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            positions: vec![ExchangeAccountPositionSnapshotInput {
                side: "long".to_string(),
                quantity: 0.01,
                quote_amount: Some(660.0),
                leverage: Some(3.0),
                margin_mode: Some("isolated".to_string()),
                liquidation_price: Some(52000.0),
                margin_ratio: None,
                unrealized_pnl: Some(4.2),
                protective_order_status: Some("exchange_manual".to_string()),
                raw_payload_json: Some(r#"{"pos":"0.01"}"#.to_string()),
                snapshot_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            position_history: vec![ExchangeAccountPositionHistorySnapshotInput {
                external_position_id: "okx-position-1".to_string(),
                side: Some("long".to_string()),
                direction: Some("long".to_string()),
                close_type: Some("2".to_string()),
                margin_mode: Some("cross".to_string()),
                leverage: Some(3.0),
                open_avg_price: Some(0.6208),
                close_avg_price: Some(0.6047),
                open_max_position: Some(1.0),
                close_total_position: Some(1.0),
                realized_pnl_usdt: Some(-0.01),
                pnl_usdt: Some(-0.01),
                pnl_ratio: Some(-0.0817),
                fee_usdt: Some(-0.0002),
                funding_fee_usdt: Some(0.0),
                liquidation_penalty_usdt: Some(0.0),
                raw_payload_json: Some(r#"{"posId":"okx-position-1"}"#.to_string()),
                opened_at: Some("2026-06-18T00:30:00".to_string()),
                closed_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            balances: vec![ExchangeAccountBalanceSnapshotInput {
                asset: "USDT".to_string(),
                wallet_balance: Some(8211.49),
                available_balance: Some(6400.25),
                equity_usdt: Some(8211.49),
                raw_payload_json: Some(r#"{"ccy":"USDT","eqUsd":"8211.49"}"#.to_string()),
                snapshot_at: Some("2026-06-18T02:30:00".to_string()),
            }],
            bills: vec![ExchangeAccountBillSnapshotInput {
                external_bill_id: "okx-bill-1".to_string(),
                asset: "USDT".to_string(),
                balance_change: Some(9.7),
                balance_change_usdt: None,
                balance_after: Some(8211.49),
                fee_amount: Some(-0.3),
                fee_usdt: None,
                pnl_amount: Some(10.0),
                pnl_usdt: None,
                bill_type: Some("2".to_string()),
                bill_sub_type: Some("1".to_string()),
                external_order_id: Some("3631557801300238336".to_string()),
                external_trade_id: Some("211849844".to_string()),
                raw_payload_json: Some(r#"{"billId":"okx-bill-1"}"#.to_string()),
                bill_at: Some("2026-06-18T02:30:00".to_string()),
            }],
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["combo_id"], 85);
        assert_eq!(value["buyer_email"], "buyer@example.com");
        assert_eq!(
            value["orders"][0]["external_order_id"],
            "3631557801300238336"
        );
        assert_eq!(value["trades"][0]["external_trade_id"], "211849844");
        assert_eq!(value["positions"][0]["quantity"], 0.01);
        assert_eq!(
            value["position_history"][0]["external_position_id"],
            "okx-position-1"
        );
        assert_eq!(value["position_history"][0]["realized_pnl_usdt"], -0.01);
        assert_eq!(value["balances"][0]["asset"], "USDT");
        assert_eq!(value["balances"][0]["equity_usdt"], 8211.49);
        assert_eq!(value["bills"][0]["external_bill_id"], "okx-bill-1");
        assert!(!value.to_string().contains("plain-api-secret"));
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.exchange_account_snapshot_url(),
            "https://quant-web.example/api/commerce/internal/exchange-account-snapshots"
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
    #[test]
    fn parse_envelope_error_omits_sensitive_response_body() {
        let body = r#"{
            "success": true,
            "data": {
                "buyer_email": "buyer@example.com",
                "api_key": "plain-api-key",
                "api_secret": "plain-api-secret",
                "passphrase": "plain-passphrase"
            }
        }"#;
        let error =
            ExecutionTaskClient::parse_envelope::<UserExchangeConfig>(body).expect_err("bad body");
        let message = error.to_string();
        assert!(message.contains("response_body_omitted=true"));
        assert!(message.contains("body_len="));
        assert!(!message.contains("plain-api-key"));
        assert!(!message.contains("plain-api-secret"));
        assert!(!message.contains("plain-passphrase"));
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
